#[macro_use]
extern crate log;

use uip::Configuration;
use uip::{Identity, Identifier};
use uip::{
    PeerInformationBase,
    NetworkState,
    UnixState
};

use std::fs::File;
use std::path::Path;
use bytes::Bytes;
use tokio::runtime::Runtime;
use clap::{App, Arg, SubCommand};
use futures_util::StreamExt;

#[tokio::main]
async fn main() {
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

    let config_file_path = matches.value_of("config").unwrap_or(".server.toml");

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

    let (network_sink, mut network_source) = tokio::sync::mpsc::channel::<(Identifier, u16, u16, Bytes)>(5);
    let (unix_sink, mut unix_source) = tokio::sync::mpsc::channel::<(Identifier, u16, u16, Bytes)>(5);
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

    tokio::spawn(async move { 
        loop {
            match network_source.next().await {
                Some((host_id, src_port, dst_port, data)) => unix_clone.deliver_frame(host_id, src_port, dst_port, data).await,
                None => break,
            }
        }
    });
    tokio::spawn(async move {
        loop {
            match unix_source.next().await {
                Some((host_id, src_port, dst_port, data)) => network_clone.send_frame(host_id, src_port, dst_port, data).await,
                None => break,
            }
        }
    });
    let network_clone = network.clone();
    tokio::spawn(async move {
        info!("Starting network forwarding");
        network_clone.run().await;
    });
    let unix_clone = unix.clone();
    tokio::spawn(async move {
        info!("Starting unix forwarding");
        unix_clone.run().await;
    });

    info!("Starting client for ID {}", config.id.identifier);
    tokio::signal::ctrl_c().await;

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
    let config_content = std::fs::read_to_string(path)
        .map_err(|err| format!("Unable to read configuration file: {}", err))?;
    toml::from_str(&config_content)
        .map_err(|err| format!("Unable to deserialize configuration file: {}", err))
}
fn write_configuration(path: &str, conf: &Configuration) -> Result<(), String> {
    let config_content = toml::to_string_pretty(conf)
        .map_err(|err| format!("Error serializing configuration: {}", err))?;
    std::fs::write(path, config_content)
        .map_err(|err| format!("Error writing configuration file: {}", err))
}
