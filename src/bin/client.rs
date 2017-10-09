#![feature(conservative_impl_trait)]
extern crate tokio_core;
extern crate serde_json;
extern crate async_readline;
extern crate bytes;
extern crate futures;

extern crate uip;

use uip::Configuration;
use uip::State;
use uip::Id;

use tokio_core::reactor::{Core};
use std::fs::File;
use futures::{Stream};
use std::env;
use std::io::Write;
use std::net::SocketAddr;
use bytes::BytesMut;


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

    let state2 = state.clone();
    core.handle().spawn(state);

    let stdio = async_readline::RawStdio::new(&core.handle()).unwrap();
    let (stdin, stdout, _) = stdio.split();
    let (commands, rl_writer) = async_readline::init(stdin, stdout);

    let done = commands.map(move |line| {
        let command = String::from_utf8(line.line.clone()).unwrap();
        if command == "exit" {
            std::process::exit(0);
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
        v
    }).forward(rl_writer);
    core.run(done).unwrap();
}
