#![feature(conservative_impl_trait)]
extern crate tokio_core;
extern crate serde_json;
extern crate async_readline;
extern crate bytes;
extern crate futures;
extern crate env_logger;

extern crate uip;

use uip::Configuration;
use uip::State;
use uip::Id;

use tokio_core::reactor::{Core};
use std::fs::File;
use std::path::Path;
use futures::{Stream};
use std::env;
use std::io::{Write,Error,ErrorKind};
use std::net::SocketAddr;
use bytes::BytesMut;

fn main() {
    env_logger::init().unwrap();

    let mut core = Core::new().unwrap();
    
    let config_file_path = if env::args().count() > 1 {
        env::args().skip(1).next().expect("No config file given")
    } else {
        ".client.json".to_string()
    };
    let state = if Path::new(&config_file_path).is_file() {
        let config = match read_configuration(&config_file_path) {
            Ok(conf) => conf,
            Err(err) => {
                return println!("Error while reading configuration file: {}", err);
            }
        };
        State::from_configuration(config, core.handle()).unwrap()
    } else {
        println!("Generating new ID");
        let id = Id::generate().expect("Unable to generate an ID");
        State::from_id(id, core.handle()).unwrap()
    };
    println!("Starting client for ID {}", state.read().id.hash);

    let state2 = state.clone();
    core.handle().spawn(state);

    let stdio = async_readline::RawStdio::new(&core.handle()).unwrap();
    let (stdin, stdout, _) = stdio.split();
    let (commands, rl_writer) = async_readline::init(stdin, stdout);

    let done = commands.and_then(move |line| {
        let command = String::from_utf8(line.line.clone()).unwrap();
        if command == "exit" {
            if let Err(err) = write_configuration(config_file_path.as_ref(), &state2.to_configuration()) {
                println!("Error while saving configuration: {}", err);
            };
            return Err(Error::new(ErrorKind::Other, "Program exit"));
        } else if command.starts_with("peer add") {
            let mut args = command.split_whitespace();
            if args.clone().count() < 4 {
                println!("peer add requires ID and socket address")
            } else {
                let id = args.nth(2).expect("Peer id required").to_string();
                let addr = args.next().expect("Peer address required");
                let sock_addr: SocketAddr = addr.parse().expect("Invalid socket address");
                state2.add_peer_address(id, sock_addr);
            }
        } else if command.starts_with("relay add") {
            let mut args = command.split_whitespace();
            if args.clone().count() < 3 {
                println!("relay add requires ID")
            } else {
                let id = args.nth(2).expect("Peer id required").to_string();
                state2.add_relay(id);
                state2.connect_to_relays();
            }
        } else if command.starts_with("send ") {
            let mut args = command.split_whitespace();
            if args.clone().count() < 4 {
                println!("send requires ID and channel number")
            } else {
                let id = args.nth(1).expect("Peer id required").to_string();
                let channel = args.next().expect("Channel number required").parse::<u16>().expect("Channel number invalid");
                let data = args.next().expect("No data to send");
                state2.send_frame(id, channel, BytesMut::from(data));
            }
        }
        let mut v = vec!();
        let _ = write!(v, "\n> ");
        v.append(&mut line.line.clone());
        Ok(v)
    }).forward(rl_writer);
    let _ = core.run(done);
}

fn read_configuration(path: &String) -> Result<Configuration, String> {
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
