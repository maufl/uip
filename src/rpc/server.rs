use std::collections::HashMap;

use bytes::Bytes;
use capnp_rpc::pry;
use tokio::sync::mpsc::Sender;

use crate::{Identifier, Shared};

struct LocalDevice {
    upstream: Sender<(Identifier, String, String, Bytes)>,
}

impl LocalDevice {
    fn new(upstream: Sender<(Identifier, String, String, Bytes)>) -> Self {
        LocalDevice { upstream }
    }

    fn open(&mut self, local_service: &str) -> Socket {
        Socket {
            local_service: local_service.to_owned(),
            upstream: self.upstream.clone(),
        }
    }
}

struct Socket {
    local_service: String,
    upstream: Sender<(Identifier, String, String, Bytes)>,
}

struct Channel {}

impl crate::uipd_capnp::local_device::Server for LocalDevice {
    fn open(
        &mut self,
        params: crate::uipd_capnp::local_device::OpenParams,
        mut results: crate::uipd_capnp::local_device::OpenResults,
    ) -> capnp::capability::Promise<(), capnp::Error> {
        let local_service = pry!(pry!(params.get()).get_local_service());
        let socket = self.open(local_service);
        results.get().set_sock(capnp_rpc::new_client(socket));
        capnp::capability::Promise::ok(())
    }
}

impl crate::uipd_capnp::socket::Server for Socket {}
