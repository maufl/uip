#![feature(conservative_impl_trait)]
extern crate tokio_core;
extern crate serde_json;

extern crate uip;

use uip::Configuration;
use uip::{State,Id};

use tokio_core::reactor::{Core};
use std::fs::File;
use std::env;

fn main() {
    let mut core = Core::new().unwrap();
    let state = if env::args().count() > 1 {
        let config_file_path = env::args().skip(1).next().expect("No config file given");

        let config_file = match File::open(config_file_path) {
            Ok(file) => file,
            Err(err) => {
                return println!("Error while opening configuration file: {}", err);
            }
        };
        let config: Configuration = match serde_json::from_reader(config_file) {
            Ok(conf) => conf,
            Err(err) => {
                return println!("Error while reading configuration file: {}", err);
            }
        };

        State::from_configuration(config, core.handle()).unwrap()
    } else {
        let id = Id::generate().expect("Unable to generate an ID");
        State::from_id(id, core.handle()).unwrap()
    };
    println!("Starting client for ID {}", state.read().id.hash);
    core.run(state).unwrap();
}
