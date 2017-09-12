#![feature(conservative_impl_trait)]

extern crate interfaces;
extern crate igd;
extern crate bytes;
extern crate byteorder;
extern crate rustls;
extern crate futures;
extern crate tokio_io;
extern crate tokio_uds;
extern crate tokio_core;
extern crate tokio_rustls;
extern crate async_readline;
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;
extern crate openssl;

mod transport;
mod peer_information_base;
mod configuration;
use configuration::Configuration;
mod state;
use state::{State};

use rustls::{Certificate};
use rustls::internal::pemfile::{ certs };
use std::net::ToSocketAddrs;
use std::io::{ BufReader };
use tokio_core::reactor::{Core};
use futures::{Stream,Sink,Future};
use futures::future;
use bytes::BytesMut;
use bytes::buf::FromBuf;
use std::fs::File;

fn main() {
    let mut core = Core::new().unwrap();

    let config_file = match File::open("config-a.json") {
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

    let state = State::from_configuration(config, core.handle());

    let state2 = state.clone();
    core.handle().spawn(state);

    let stdio = async_readline::RawStdio::new(&core.handle()).unwrap();
    let (stdin, stdout, _) = stdio.split();
    let (commands, rl_writer) = async_readline::init(stdin, stdout);

    let done = commands.for_each(|line|{ state2.send_frame("testserver.com".to_string(), 1u16, BytesMut::from_buf(line.line)); future::ok(()) });
    core.run(done).unwrap();
}

