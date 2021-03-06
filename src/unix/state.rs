use std::collections::HashMap;
use futures::sync::mpsc::{channel, Sender};
use futures::future;
use tokio::net::{UnixListener, UnixStream};
use tokio::io::AsyncRead;
use tokio::codec::{Framed,Decoder};
use tokio;
use tokio::prelude::*;
use tokio::timer::Delay;
use bytes::BytesMut;
use std::io::{Error, ErrorKind};
use std::path::Path;
use std::fs;
use rand::{thread_rng, Rng};
use std::u16;
use std::time::{Duration, Instant};

use {Identifier, Shared};
use unix::{Connection, ControlProtocolCodec, Frame};

#[derive(Clone)]
pub struct UnixState {
    pub ctl_socket: String,
    connections: HashMap<(Identifier, u16, u16), Connection>,
    listeners: HashMap<u16, Sender<Frame>>,
    upstream: Sender<(Identifier, u16, u16, BytesMut)>,
}

impl Drop for UnixState {
    fn drop(&mut self) {
        let path = &self.ctl_socket;
        if Path::new(path).exists() {
            let _ = fs::remove_file(path);
        };
    }
}

impl UnixState {
    pub fn new(
        ctl_socket: String,
        upstream: Sender<(Identifier, u16, u16, BytesMut)>,
    ) -> UnixState {
        UnixState {
            ctl_socket: ctl_socket,
            connections: HashMap::new(),
            listeners: HashMap::new(),
            upstream: upstream,
        }
    }

    pub fn shared(self) -> Shared<UnixState> {
        Shared::new(self)
    }
}

impl Shared<UnixState> {
    pub fn ctl_socket(&self) -> String {
        self.read().ctl_socket.clone()
    }

    fn open_ctl_socket(&self) -> Box<Future<Item=(),Error=()> + Send> {
        if self.read().ctl_socket == "" {
            warn!("No unix control socket path is given");
            return Box::new(future::err(()));
        }
        let state = self.clone();
        let future = UnixListener::bind(&self.read().ctl_socket)
            .expect("Unable to open unix control socket")
            .incoming()
            .for_each(move |stream| state.handle_new_stream(stream))
            .map_err(|e| println!("Control socket was closed: {}", e));
        Box::new(future)
    }

    pub fn handle_new_stream(
        &self,
        connection: UnixStream,
    ) -> impl Future<Item = (), Error = Error> {
        let state = self.clone();
        let socket = ControlProtocolCodec{}.framed(connection);
        socket
            .into_future()
            .and_then(move |(frame, socket)| match frame {
                Some(Frame::Connect(host_id, dst_port)) => {
                    debug!(
                        "Received a connect message for {} to port {}",
                        host_id, dst_port
                    );
                    state.stream_connect(socket, host_id, dst_port);
                    Ok(())
                }
                Some(Frame::Accept(host_id, local_port, remote_port)) => {
                    debug!("Received a accept message for connection from {} port {} on listening port {}", host_id, remote_port, local_port);
                    state.stream_accept(socket, host_id, local_port, remote_port);
                    Ok(())
                }
                Some(Frame::Listen(src_port)) => {
                    debug!("Received a listen message for port {}", src_port);
                    state.stream_listen(socket, src_port);
                    Ok(())
                }
                _ => return Err((Error::new(ErrorKind::Other, "Unexpected message"), socket)),
            })
            .then(|result| {
                if let Err(err) = result {
                    warn!("Error in unix stream: {}", err.0);
                }
                Ok(())
            })
    }

    pub fn stream_connect(
        &self,
        connection: Framed<UnixStream, ControlProtocolCodec>,
        host_id: Identifier,
        dst_port: u16,
    ) {
        let used_ports = self.used_ports();
        let mut rng = thread_rng();
        let mut src_port: u16 = rng.gen_range(1024, u16::MAX);
        while used_ports.contains(&src_port) {
            src_port = rng.gen_range(1024, u16::MAX);
        }
        let connection =
            Connection::from_unix_socket(self.clone(), connection, host_id, src_port, dst_port);
        self.write()
            .connections
            .insert((host_id, src_port, dst_port), connection);
    }

    pub fn stream_accept(
        &self,
        connection: Framed<UnixStream, ControlProtocolCodec>,
        host_id: Identifier,
        local_port: u16,
        remote_port: u16,
    ) {
        let connection = Connection::from_unix_socket(
            self.clone(),
            connection,
            host_id,
            local_port,
            remote_port,
        );
        self.write()
            .connections
            .insert((host_id, local_port, remote_port), connection);
    }

    pub fn stream_listen(
        &self,
        connection: Framed<UnixStream, ControlProtocolCodec>,
        src_port: u16,
    ) {
        let (unix_sink, _unix_stream) = connection.split();
        let (sender, receiver) = channel::<Frame>(10);
        tokio::spawn(
            receiver
                .forward(
                    unix_sink
                        .sink_map_err(|err| warn!("Sink error for Unix listening socket: {}", err)),
                )
                .map(|_| ())
                .map_err(|_| warn!("Forwarding error for Unix listening socket.")),
        );
        self.write().listeners.insert(src_port, sender);
    }

    pub fn send_frame(&self, host_id: Identifier, src_port: u16, dst_port: u16, data: BytesMut) {
        let task = self.read()
            .upstream
            .clone()
            .send((host_id, src_port, dst_port, data))
            .map(|_| ())
            .map_err(|err| warn!("Failed to pass message to upstream: {}", err));
        tokio::spawn(task);
    }

    pub fn used_ports(&self) -> Vec<u16> {
        self.read()
            .connections
            .keys()
            .map(|&(_host_id, src_port, _dst_port)| src_port)
            .collect()
    }

    pub fn deliver_frame(
        &self,
        host_id: Identifier,
        remote_port: u16,
        local_port: u16,
        data: BytesMut,
    ) {
        debug!(
            "Received new data from {}:{} to port {} {:?}",
            host_id, remote_port, local_port, data
        );
        if let Some(socket) = self.read()
            .connections
            .get(&(host_id, local_port, remote_port))
        {
            tokio::spawn(
                socket
                    .send_frame(data)
                    .map(|_| ())
                    .map_err(|err| warn!("Unable to deliver frame locally: {}", err)),
            );
            return;
        }
        if let Some(listener) = self.read().listeners.get(&local_port) {
            let state = self.clone();
            let notify_listener = listener
                .clone()
                .send(Frame::IncomingConnection(host_id, remote_port))
                .map(|_| ())
                .map_err(move |err| {
                    warn!(
                        "Unable to notify Unix listener about new connection: {}",
                        err
                    );
                    state.write().listeners.remove(&local_port);
                });
            tokio::spawn(notify_listener);
            let state = self.clone();
            let resend = Delay::new(Instant::now() + Duration::new(0, 200 * 1000 * 1000))
                .map_err(|err| warn!("Timeout error: {}", err))
                .map(move |_| {
                    state.deliver_frame(host_id, remote_port, local_port, data);
                });
            tokio::spawn(resend);
        }
    }

    pub fn run(&self) -> impl Future<Item=(), Error=()> + Send {
        self.open_ctl_socket()
    }
}
