use std::collections::HashMap;
use std::rc::Rc;
use std::cell::{RefCell, Ref, RefMut};
use futures::{Future, Poll, Stream};
use futures::sync::mpsc::channel;
use tokio_core::reactor::Handle;
use tokio_uds::UnixListener;
use tokio_io::AsyncRead;
use bytes::BytesMut;
use std::io;
use std::path::Path;
use std::fs;

use network::NetworkState;
use peer_information_base::PeerInformationBase;
use configuration::Configuration;
use unix_codec::{ControlProtocolCodec, Frame};
use unix_socket::UnixSocket;
use {Identity, Identifier};



pub struct InnerState {
    pub id: Identity,
    sockets: HashMap<(Identifier, u16), UnixSocket>,
    handle: Handle,
    ctl_socket: String,
    pub network: NetworkState,
}

impl Drop for InnerState {
    fn drop(&mut self) {
        let path = &self.ctl_socket;
        if Path::new(path).exists() {
            let _ = fs::remove_file(path);
        };
    }
}




#[derive(Clone)]
pub struct State(pub Rc<RefCell<InnerState>>);

impl State {
    pub fn from_configuration(config: Configuration, handle: &Handle) -> State {
        let (sink, source) = channel::<(Identifier, u16, BytesMut)>(5);
        let state = State(Rc::new(RefCell::new(InnerState {
            id: config.id.clone(),
            network: NetworkState::new(
                config.id,
                config.pib,
                config.relays,
                config.port,
                handle.clone(),
                sink,
            ),
            sockets: HashMap::new(),
            handle: handle.clone(),
            ctl_socket: config.ctl_socket,
        })));
        let state2 = state.clone();
        let task = source
            .for_each(move |(host_id, channel_id, data)| {
                Ok(state2.deliver_frame(host_id, channel_id, data))
            })
            .map_err(|_| error!("Failed to receive frames from network layer"));
        handle.spawn(task);
        state
    }

    pub fn from_id(id: Identity, handle: &Handle) -> State {
        let (sink, source) = channel::<(Identifier, u16, BytesMut)>(5);
        let state = State(Rc::new(RefCell::new(InnerState {
            ctl_socket: format!("/run/user/1000/uip/{}.ctl", id.identifier),
            id: id.clone(),
            network: NetworkState::new(
                id,
                PeerInformationBase::new(),
                Vec::new(),
                0,
                handle.clone(),
                sink,
            ),
            sockets: HashMap::new(),
            handle: handle.clone(),
        })));
        let state2 = state.clone();
        let task = source
            .for_each(move |(host_id, channel_id, data)| {
                Ok(state2.deliver_frame(host_id, channel_id, data))
            })
            .map_err(|_| error!("Failed to receive frames from network layer"));
        handle.spawn(task);
        state
    }

    pub fn to_configuration(&self) -> Configuration {
        let state = self.read();
        let network = state.network.read();
        Configuration {
            id: state.id.clone(),
            pib: network.pib.clone(),
            relays: network.relays.clone(),
            port: network.port,
            ctl_socket: state.ctl_socket.clone(),
        }
    }

    pub fn read(&self) -> Ref<InnerState> {
        self.0.borrow()
    }

    pub fn write(&self) -> RefMut<InnerState> {
        self.0.borrow_mut()
    }

    pub fn spawn<F: Future<Item = (), Error = ()> + 'static>(&self, f: F) {
        self.read().handle.spawn(f)
    }

    pub fn handle(&self) -> Handle {
        self.read().handle.clone()
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
                socket
                    .into_future()
                    .and_then(move |(frame, socket)| {
                        let (host_id, channel_id) = match frame {
                            Some(Frame::Connect(host_id, channel_id)) => (host_id, channel_id),
                            _ => {
                                return Err((
                                    io::Error::new(io::ErrorKind::Other, "Unexpected message"),
                                    socket,
                                ))
                            }
                        };
                        let unix_socket = UnixSocket::from_unix_socket(
                            state.clone(),
                            socket,
                            host_id,
                            channel_id,
                        );
                        state.write().sockets.insert(
                            (host_id, channel_id),
                            unix_socket,
                        );
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

    pub fn send_frame(&self, host_id: Identifier, channel_id: u16, data: BytesMut) {
        self.read().network.send_frame(host_id, channel_id, data)
    }

    pub fn deliver_frame(&self, host_id: Identifier, channel_id: u16, data: BytesMut) {
        println!(
            "Received new data from {} in channel {}: {:?}",
            host_id,
            channel_id,
            data
        );
        if let Some(socket) = self.read().sockets.get(&(host_id, channel_id)) {
            self.spawn(socket.send_frame(data).map(|_| ()).map_err(|_| ()));
        }
    }
}

impl Future for State {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        self.open_ctl_socket();
        self.write().network.poll()
    }
}
