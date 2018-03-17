use std::rc::Rc;
use std::cell::{Ref, RefCell, RefMut};
use futures::{Future, Poll, Stream};
use futures::sync::mpsc::{channel, Receiver};
use tokio_core::reactor::Handle;
use bytes::BytesMut;

use network::NetworkState;
use data::PeerInformationBase;
use configuration::Configuration;
use {Identifier, Identity};
use unix::State as UnixState;

pub struct InnerState {
    pub id: Identity,
    handle: Handle,
    pub unix: UnixState,
    pub network: NetworkState,
}

#[derive(Clone)]
pub struct State(pub Rc<RefCell<InnerState>>);

impl State {
    pub fn from_configuration(config: Configuration, handle: &Handle) -> State {
        let (network_sink, network_source) = channel::<(Identifier, u16, u16, BytesMut)>(5);
        let (unix_sink, unix_source) = channel::<(Identifier, u16, u16, BytesMut)>(5);
        let state = State(Rc::new(RefCell::new(InnerState {
            id: config.id.clone(),
            network: NetworkState::new(
                config.id,
                config.pib,
                config.relays,
                config.port,
                handle.clone(),
                network_sink,
            ),
            unix: UnixState::new(handle.clone(), config.ctl_socket, unix_sink),
            handle: handle.clone(),
        })));
        state.forward_network_data(network_source);
        state.forward_unix_data(unix_source);
        state
    }

    pub fn from_id(id: Identity, handle: &Handle) -> State {
        let (network_sink, network_source) = channel::<(Identifier, u16, u16, BytesMut)>(5);
        let (unix_sink, unix_source) = channel::<(Identifier, u16, u16, BytesMut)>(5);
        let state = State(Rc::new(RefCell::new(InnerState {
            id: id.clone(),
            network: NetworkState::new(
                id.clone(),
                PeerInformationBase::new(),
                Vec::new(),
                0,
                handle.clone(),
                network_sink,
            ),
            unix: UnixState::new(
                handle.clone(),
                format!("/run/user/1000/{}.sock", &id.identifier),
                unix_sink,
            ),
            handle: handle.clone(),
        })));
        state.forward_network_data(network_source);
        state.forward_unix_data(unix_source);
        state
    }

    pub fn forward_network_data(&self, source: Receiver<(Identifier, u16, u16, BytesMut)>) {
        let state = self.clone();
        let task = source
            .for_each(move |(host_id, src_port, dst_port, data)| {
                state.deliver_frame(host_id, src_port, dst_port, data);
                Ok(())
            })
            .map_err(|_| error!("Failed to receive frames from network layer"));
        self.spawn(task);
    }

    pub fn forward_unix_data(&self, source: Receiver<(Identifier, u16, u16, BytesMut)>) {
        let state = self.clone();
        let task = source
            .for_each(move |(host_id, src_port, dst_port, data)| {
                state.send_frame(host_id, src_port, dst_port, data);
                Ok(())
            })
            .map_err(|_| error!("Failed to receive frames from network layer"));
        self.spawn(task);
    }

    pub fn to_configuration(&self) -> Configuration {
        let state = self.read();
        let network = state.network.read();
        Configuration {
            id: state.id.clone(),
            pib: network.pib.clone(),
            relays: network.relays.clone(),
            port: network.port,
            ctl_socket: state.unix.ctl_socket().clone(),
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

    pub fn send_frame(&self, host_id: Identifier, src_port: u16, dst_port: u16, data: BytesMut) {
        self.read()
            .network
            .send_frame(host_id, src_port, dst_port, data)
    }

    pub fn deliver_frame(&self, host_id: Identifier, src_port: u16, dst_port: u16, data: BytesMut) {
        debug!(
            "Received new data from {}:{} in to {} {:?}",
            host_id, src_port, dst_port, data
        );
        self.read()
            .unix
            .deliver_frame(host_id, src_port, dst_port, data)
    }
}

impl Future for State {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        self.write().network.poll()
    }
}
