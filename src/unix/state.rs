use std::collections::HashMap;
use futures::{Async, Future, Poll, Sink, Stream};
use futures::sync::mpsc::Sender;
use tokio_uds::{UnixListener, UnixStream};
use tokio_core::reactor::Handle;
use tokio_io::AsyncRead;
use tokio_io::codec::Framed;
use bytes::BytesMut;
use std::io;
use std::path::Path;
use std::fs;
use rand::{thread_rng, Rng};
use std::u16;

use {Identifier, Shared};
use unix::{Connection, ControlProtocolCodec, Frame};

#[derive(Clone)]
pub struct UnixState {
    pub ctl_socket: String,
    handle: Handle,
    connections: HashMap<(Identifier, u16, u16), Connection>,
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
        handle: Handle,
        upstream: Sender<(Identifier, u16, u16, BytesMut)>,
    ) -> UnixState {
        UnixState {
            ctl_socket: ctl_socket,
            handle: handle,
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

    fn open_ctl_socket(&self) {
        if self.read().ctl_socket == "" {
            return;
        }
        let state = self.clone();
        let done = UnixListener::bind(&self.read().ctl_socket, &self.read().handle)
            .expect("Unable to open unix control socket")
            .incoming()
            .for_each(move |(stream, _addr)| state.handle_new_stream(stream))
            .map_err(|e| println!("Control socket was closed: {}", e));
        self.spawn(done);
    }

    pub fn handle_new_stream(
        &self,
        connection: UnixStream,
    ) -> impl Future<Item = (), Error = io::Error> {
        let state = self.clone();
        let socket = connection.framed(ControlProtocolCodec);
        socket
            .into_future()
            .and_then(move |(frame, socket)| match frame {
                Some(Frame::Connect(host_id, dst_port)) => {
                    state.stream_connect(socket, host_id, dst_port);
                    Ok(())
                }
                _ => {
                    return Err((
                        io::Error::new(io::ErrorKind::Other, "Unexpected message"),
                        socket,
                    ))
                }
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

    pub fn send_frame(&self, host_id: Identifier, src_port: u16, dst_port: u16, data: BytesMut) {
        let task = self.read()
            .upstream
            .clone()
            .send((host_id, src_port, dst_port, data))
            .map(|_| ())
            .map_err(|err| warn!("Failed to pass message to upstream: {}", err));
        self.spawn(task);
    }

    pub fn used_ports(&self) -> Vec<u16> {
        self.read()
            .connections
            .keys()
            .map(|&(_host_id, src_port, _dst_port)| src_port)
            .collect()
    }

    pub fn spawn<F: Future<Item = (), Error = ()> + 'static>(&self, f: F) {
        self.read().handle.spawn(f)
    }

    pub fn deliver_frame(&self, host_id: Identifier, src_port: u16, dst_port: u16, data: BytesMut) {
        debug!(
            "Received new data from {}:{} in to {} {:?}",
            host_id, src_port, dst_port, data
        );
        if let Some(socket) = self.read().connections.get(&(host_id, src_port, dst_port)) {
            self.spawn(socket.send_frame(data).map(|_| ()).map_err(|_| ()));
        }
    }
}

impl Future for Shared<UnixState> {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        self.open_ctl_socket();
        Ok(Async::NotReady)
    }
}
