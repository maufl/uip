#![feature(conservative_impl_trait)]
extern crate tokio_core;
extern crate serde_json;
extern crate tokio_file_unix;
extern crate tokio_io;
extern crate futures;

extern crate uip;

use uip::Configuration;
use uip::{State,Id};

use tokio_core::reactor::{Core};
use std::fs::File;
use std::path::Path;
use std::env;
use futures::stream::Stream;
use std::io::{Error, ErrorKind};

fn main() {
    let mut core = Core::new().unwrap();
    let config_file_path = if env::args().count() > 1 {
        env::args().skip(1).next().expect("No config file given")
    } else {
        ".server.json".to_string()
    };
    let state =  if Path::new(&config_file_path).is_file() {
        let config = match read_configuration(config_file_path.to_string()) {
            Ok(c) => c,
            Err(err) => return println!("{}", err)
        };
        State::from_configuration(config, core.handle()).unwrap()
    } else {
        println!("Generating new ID");
        let id = Id::generate().expect("Unable to generate an ID");
        State::from_id(id, core.handle()).unwrap()
    };
    let stdin = std::io::stdin();
    let file = tokio_file_unix::StdFile(stdin.lock());
    let file = tokio_file_unix::File::new_nb(file).unwrap();
    let file = file.into_reader(&core.handle()).unwrap();
    println!("Starting client for ID {}", state.read().id.hash);
    core.handle().spawn(state.clone());
    let _ = core.run(tokio_io::io::lines(file).for_each(|line| {
        if line.trim() == "exit" {
            if let Err(err) = write_configuration(config_file_path.as_ref(), &state.to_configuration()) {
                println!("Error while saving configuration: {}", err);
            };
            Err(Error::new(ErrorKind::Other, "Closing programm"))
        } else {
            Ok(())
        }
    }));
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
    serde_json::to_writer(config_file, conf)
        .map(|_| ())
        .map_err(|err| format!("Error while reading configuration file: {}", err))
}
