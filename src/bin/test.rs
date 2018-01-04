extern crate tokio_core;
extern crate igd;
extern crate env_logger;

use igd::{search_gateway, PortMappingProtocol};

fn main() {
    env_logger::init().unwrap();

    let socket_addr = "192.168.2.15:31337".parse().expect(
        "Invalid socket address",
    );
    let gateway = search_gateway().expect("Unable to find gateway");
    gateway
        .add_any_port(PortMappingProtocol::UDP, socket_addr, 100u32, "Test")
        .expect("Unable to add portmapping");
}
