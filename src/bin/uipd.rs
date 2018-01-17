#![feature(conservative_impl_trait)]
extern crate tokio_core;
extern crate serde_json;
extern crate tokio_file_unix;
extern crate tokio_io;
extern crate tokio_signal;
extern crate futures;
#[macro_use]
extern crate log;
extern crate env_logger;
extern crate clap;

extern crate uip;

use uip::Configuration;
use uip::{State, Identity};

use tokio_core::reactor::Core;
use std::fs::File;
use std::path::Path;
use futures::stream::Stream;
use futures::Future;
use clap::{Arg, App, SubCommand};

fn main() {
    env_logger::init().unwrap();
    let matches = app().get_matches();

    let mut core = Core::new().unwrap();

    let config_file_path = matches.value_of("config").unwrap_or(".server.json");

    if Some("generate-config") == matches.subcommand_name() {
        let id = Identity::generate().expect("Unable to generate an ID");
        let state = State::from_id(id, &core.handle());
        return write_configuration(&config_file_path, &state.to_configuration())
            .unwrap_or_else(|err| error!("{}", err));
    }

    if !Path::new(&config_file_path).is_file() {
        return error!("Configuration file {} does not exist", config_file_path);
    };
    let config = match read_configuration(config_file_path.to_string()) {
        Ok(c) => c,
        Err(err) => return error!("{}", err),
    };
    let state = State::from_configuration(config, &core.handle());

    println!("Starting client for ID {}", state.read().id.identifier);
    core.handle().spawn(state.clone());
    let handle = core.handle();
    let _ = core.run(
        tokio_signal::ctrl_c(&handle)
            .flatten_stream()
            .into_future()
            .map(|_| println!("Received CTRL-C"))
            .map_err(|_| println!("Panic")),
    );
    let _ = write_configuration(&config_file_path, &state.to_configuration());
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

fn read_configuration(path: String) -> Result<Configuration, String> {
    let config_file = match File::open(path) {
        Ok(file) => file,
        Err(err) => {
            return Err(format!("Error while opening configuration file: {}", err));
        }
    };
    serde_json::from_reader(config_file).map_err(|err| {
        format!("Error while reading configuration file: {}", err)
    })
}
fn write_configuration(path: &str, conf: &Configuration) -> Result<(), String> {
    let config_file = File::create(path).map_err(|err| {
        format!("Error while opening configuration file: {}", err)
    })?;
    serde_json::to_writer_pretty(config_file, conf)
        .map(|_| ())
        .map_err(|err| {
            format!("Error while reading configuration file: {}", err)
        })
}
