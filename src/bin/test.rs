extern crate fern;
extern crate igd;
extern crate log;
extern crate tokio_core;

use igd::{search_gateway, PortMappingProtocol};

fn main() {
    fern::Dispatch::new()
        .level_for("uip", log::LevelFilter::Debug)
        .chain(std::io::stdout())
        .apply()
        .expect("Unable to initialize logger");

    let socket_addr = "192.168.2.15:31337"
        .parse()
        .expect("Invalid socket address");
    let gateway = search_gateway().expect("Unable to find gateway");
    gateway
        .add_any_port(PortMappingProtocol::UDP, socket_addr, 100u32, "Test")
        .expect("Unable to add portmapping");
}
