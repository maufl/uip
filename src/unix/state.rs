use std::collections::HashMap;
use futures::{Async, Future, Poll, Sink, Stream};
use futures::sync::mpsc::Sender;
use tokio_uds::UnixListener;
use tokio_core::reactor::Handle;
use tokio_io::AsyncRead;
use bytes::BytesMut;
use std::io;
use std::path::Path;
use std::fs;

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
            .for_each(move |(stream, _addr)| {
                let state = state.clone();
                let socket = stream.framed(ControlProtocolCodec);
                let src_port = 1u16;
                socket
                    .into_future()
                    .and_then(move |(frame, socket)| {
                        let (host_id, dst_port) = match frame {
                            Some(Frame::Connect(host_id, dst_port)) => (host_id, dst_port),
                            _ => {
                                return Err((
                                    io::Error::new(io::ErrorKind::Other, "Unexpected message"),
                                    socket,
                                ))
                            }
                        };
                        let connection = Connection::from_unix_socket(
                            state.clone(),
                            socket,
                            host_id,
                            src_port,
                            dst_port,
                        );
                        state
                            .write()
                            .connections
                            .insert((host_id, src_port, dst_port), connection);
                        Ok(())
                    })
                    .then(|result| {
                        if let Err(err) = result {
                            println!("Error in unix stream: {}", err.0);
                        }
                        Ok(())
                    })
            })
            .map_err(|e| println!("Control socket was closed: {}", e));
        self.spawn(done);
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
