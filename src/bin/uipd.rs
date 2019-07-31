extern crate clap;
extern crate bytes;
extern crate fern;
extern crate futures;
#[macro_use]
extern crate log;
extern crate serde_json;
extern crate tokio;
extern crate tokio_signal;

extern crate uip;

use uip::Configuration;
use uip::{Identity, Identifier};
use uip::{
    PeerInformationBase,
    NetworkState,
    UnixState
};

use std::fs::File;
use std::path::Path;
use bytes::BytesMut;
use futures::stream::Stream;
use futures::sync::mpsc::{channel};
use tokio::prelude::*;
use tokio::runtime::Runtime;
use clap::{App, Arg, SubCommand};

fn main() {
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{}][{}] {}",
                record.level(),
                record.target(),
                message
            ))
        })
        .level(log::LevelFilter::Error)
        .level_for("uip", log::LevelFilter::Debug)
        .level_for("uipd", log::LevelFilter::Debug)
        .chain(std::io::stdout())
        .apply()
        .expect("Unable to initialize logger");

    let matches = app().get_matches();

    let config_file_path = matches.value_of("config").unwrap_or(".server.json");

    if Some("generate-config") == matches.subcommand_name() {
        return generate_configuration(config_file_path);
    }

    if !Path::new(&config_file_path).is_file() {
        return error!("Configuration file {} does not exist", config_file_path);
    };
    let mut config = match read_configuration(config_file_path.to_string()) {
        Ok(c) => c,
        Err(err) => return error!("{}", err),
    };
    if config.port == 0u16 {
        config.port = 0xfe11;
    }

    let (network_sink, network_source) = channel::<(Identifier, u16, u16, BytesMut)>(5);
    let (unix_sink, unix_source) = channel::<(Identifier, u16, u16, BytesMut)>(5);
    let network =  NetworkState::new(
            config.id.clone(),
            config.pib,
            config.relays,
            config.port,
            network_sink,
    ).shared();
    let network_clone = network.clone();

    let unix = UnixState::new(config.ctl_socket, unix_sink).shared();
    let unix_clone = unix.clone();

    let forward_network_packets = network_source
        .for_each(move |(host_id, src_port, dst_port, data)| {
            unix_clone.deliver_frame(host_id, src_port, dst_port, data);
            Ok(())
        })
        .map_err(|_| error!("Failed to receive frames from network layer"));
    let forward_unix_packets = unix_source
        .for_each(move |(host_id, src_port, dst_port, data)| {
            network_clone.send_frame(host_id, src_port, dst_port, data);
            Ok(())
        })
        .map_err(|_| error!("Failed to receive frames from network layer"));

    info!("Starting client for ID {}", config.id.identifier);
    let await_ctrl_c = tokio_signal::ctrl_c()
        .flatten_stream()
        .into_future()
        .map(|_| info!("Received CTRL-C"))
        .map_err(|_| warn!("Error waiting for SIG TERM"));
    let network_future = {
        let network = network.clone();
        future::lazy(move || {
            info!("Starting network forwarding");
            network.run()
        })
    };
    let unix_future = {
        let unix = unix.clone();
        future::lazy(move || {
            info!("Starting unix forwarding");
            unix.run()
        })
    };
    let mut rt = Runtime::new().expect("Unable to initialize tokio runtime");
    rt.block_on(
        await_ctrl_c
            .select2(network_future)
            .select2(unix_future)
            .select2(forward_network_packets)
            .select2(forward_unix_packets)
            .map(|_| info!("Shutting down without errors") )
            .map_err(|err| warn!("Shutting down with error") )
    );
    let config = Configuration{
        id: config.id,
        pib: network.read().pib.clone(),
        relays: network.read().relays.clone(),
        port: network.read().port,
        ctl_socket: unix.read().ctl_socket.clone()
    };
    let _ = write_configuration(config_file_path, &config);
}

fn app() -> App<'static, 'static> {
    App::new("The UIP daemon")
        .version("0.1")
        .author("Felix K. Maurer <maufl@maufl.de>")
        .about("The UIP daemon provides peer to peer connectivity based on unique and stable peer ids.")
        .arg(Arg::with_name("config")
             .short("c")
             .help("The configuration file.")
             .takes_value(true))
        .subcommand(SubCommand::with_name("generate-config")
                    .about("generates a new configuration file with default values"))
}

fn generate_configuration(config_file_path: &str) {
    let id = Identity::generate().expect("Unable to generate an ID");
    let config = Configuration{
        id: id.clone(),
        pib: PeerInformationBase::new(),
        relays: Vec::new(),
        port: 0,
        ctl_socket: format!("/run/user/1000/{}.sock", &id.identifier)
    };
    write_configuration(config_file_path, &config)
        .unwrap_or_else(|err| error!("{}", err))
}

fn read_configuration(path: String) -> Result<Configuration, String> {
    let config_file = match File::open(path) {
        Ok(file) => file,
        Err(err) => {
            return Err(format!("Error while opening configuration file: {}", err));
        }
    };
    serde_json::from_reader(config_file)
        .map_err(|err| format!("Error while reading configuration file: {}", err))
}
fn write_configuration(path: &str, conf: &Configuration) -> Result<(), String> {
    let config_file = File::create(path)
        .map_err(|err| format!("Error while opening configuration file: {}", err))?;
    serde_json::to_writer_pretty(config_file, conf)
        .map(|_| ())
        .map_err(|err| format!("Error while reading configuration file: {}", err))
}
