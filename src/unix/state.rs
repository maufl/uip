use std::collections::HashMap;
use futures::sync::mpsc::{channel, Sender};
use futures::future;
use tokio::net::{UnixListener, UnixStream};
use tokio::codec::{Framed,Decoder};
use tokio;
use tokio::prelude::*;
use bytes::BytesMut;
use std::io::{Error, ErrorKind};
use std::path::Path;
use std::fs;
use std::u16;

use {Identifier, Shared};
use unix::{ControlProtocolCodec, Frame};

#[derive(Clone)]
pub struct UnixState {
    pub ctl_socket: String,
    connections: HashMap<u16, Sender<Frame>>,
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

    fn open_ctl_socket(&self) -> Box<dyn Future<Item=(),Error=()> + Send> {
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
                Some(Frame::Bind(src_port)) => {
                    debug!("Received a listen message for port {}", src_port);
                    state.stream_bind(socket, src_port);
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

    pub fn stream_bind(
        &self,
        connection: Framed<UnixStream, ControlProtocolCodec>,
        src_port: u16,
    ) {
        let socket = self.clone();
        let (unix_sink, unix_stream) = connection.split();
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
        let task = unix_stream.for_each(move |frame| match frame {
            Frame::Data(host_id, dst_port, data) => Ok(socket.send_frame(host_id, src_port, dst_port, data.into())),
            _ => Ok(warn!("Unhandled frame received on Unix stream."))
        }).map_err(|err| warn!("Error while forwarding frames from Unix stream"));
        tokio::spawn(task);
        self.write().connections.insert(src_port, sender);
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
            .cloned()
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
            .get(&local_port)
        {
            tokio::spawn(
                socket
                    .clone()
                    .send(Frame::Data(host_id, remote_port, data.to_vec()))
                    .map(|_| ())
                    .map_err(|err| warn!("Unable to deliver frame locally: {}", err)),
            );
            return;
        }
    }

    pub fn run(&self) -> impl Future<Item=(), Error=()> + Send {
        self.open_ctl_socket()
    }
}
