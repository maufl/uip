use tokio;
use tokio::prelude::*;
use futures::sync::mpsc::{channel, Receiver};
use bytes::BytesMut;

use network::NetworkState;
use data::PeerInformationBase;
use configuration::Configuration;
use {Identifier, Identity, Shared};
use unix::UnixState;

pub struct State {
    pub id: Identity,
    pub unix: Shared<UnixState>,
    pub network: Shared<NetworkState>,
    network_source: Receiver<(Identifier, u16, u16, BytesMut)>,
    unix_source: Receiver<(Identifier, u16, u16, BytesMut)>,
}

impl State {
    pub fn shared(self) -> Shared<State> {
        Shared::new(self)
    }
}

impl Shared<State> {
    pub fn from_configuration(config: Configuration) -> Shared<State> {
        let (network_sink, network_source) = channel::<(Identifier, u16, u16, BytesMut)>(5);
        let (unix_sink, unix_source) = channel::<(Identifier, u16, u16, BytesMut)>(5);
        State {
            id: config.id.clone(),
            network: NetworkState::new(
                config.id,
                config.pib,
                config.relays,
                config.port,
                network_sink,
            ).shared(),
            unix: UnixState::new(config.ctl_socket, unix_sink).shared(),
            network_source: network_source,
            unix_source: unix_source
        }.shared()
    }

    pub fn from_id(id: Identity) -> Shared<State> {
        let (network_sink, network_source) = channel::<(Identifier, u16, u16, BytesMut)>(5);
        let (unix_sink, unix_source) = channel::<(Identifier, u16, u16, BytesMut)>(5);
        State {
            id: id.clone(),
            network: NetworkState::new(
                id.clone(),
                PeerInformationBase::new(),
                Vec::new(),
                0,
                network_sink,
            ).shared(),
            unix: UnixState::new(format!("/run/user/1000/{}.sock", &id.identifier), unix_sink)
                .shared(),
            network_source: network_source,
            unix_source: unix_source
        }.shared()
    }

//    pub fn forward_network_data(&self) -> impl Future<Item=(), Error=()> {
//        let state = self.clone();
//        self.read().network_source
//            .for_each(move |(host_id, src_port, dst_port, data)| {
//                state.deliver_frame(host_id, src_port, dst_port, data);
//                Ok(())
//            })
//            .map_err(|_| error!("Failed to receive frames from network layer"))
//    }
//
//    pub fn forward_unix_data(&self) -> impl Future<Item=(), Error=()> {
//        let state = self.clone();
//        self.read().unix_source
//            .for_each(move |(host_id, src_port, dst_port, data)| {
//                state.send_frame(host_id, src_port, dst_port, data);
//                Ok(())
//            })
//            .map_err(|_| error!("Failed to receive frames from network layer"))
//    }

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

    pub fn run(&self) -> impl Future<Item=(), Error=()> + Send {
        let state = self.clone();
        self.read().unix.run()
            .map(|_| ())
            .map_err(|_| ())
    }
}
