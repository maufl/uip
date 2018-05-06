extern crate clap;
extern crate fern;
#[macro_use]
extern crate log;
extern crate rmp_serde;
extern crate serde;
extern crate uip;

use serde::Serialize;
use rmp_serde::{from_slice, Serializer};

use std::os::unix::net::UnixStream;
use std::io::{stdin, Read, Write};
use clap::{App, Arg};

use uip::unix::Frame;
use uip::Identifier;

fn main() {
    fern::Dispatch::new()
        .level_for("uip", log::LevelFilter::Debug)
        .chain(std::io::stdout())
        .apply()
        .expect("Unable to initialize logger");

    let matches = app().get_matches();

    let socket_path = matches
        .value_of("socket")
        .expect("A socket argument is required");
    let port: u16 = matches
        .value_of("port")
        .expect("A port argument is required")
        .parse()
        .expect("Port must be a number");
    if matches.is_present("listen") {
        listen(socket_path, port);
    } else {
        let remote_id: Identifier = matches
            .value_of("remote_id")
            .expect("A peer id is required when connecting")
            .parse()
            .expect("Peer id is invalid");
        connect(socket_path, remote_id, port);
    }
}

fn listen(socket_path: &str, port: u16) {
    let mut conn = UnixStream::connect(socket_path).expect("Unable to connect to UIP daemon");
    let msg = Frame::Listen(port);
    let mut buf = Vec::new();
    msg.serialize(&mut Serializer::new(&mut buf))
        .expect("Unable to serialize listen message");
    conn.write(&buf).expect("Unable to send listen message");
    let mut buf = [0; 1500];
    let n = conn.read(&mut buf[..]).expect("Unable to read message");
    let frame: Frame = from_slice(&buf[..n]).expect("Unable to parse incoming connection message");
    let (remote_id, src_port) = match frame {
        Frame::IncomingConnection(id, port) => (id, port),
        _ => return error!("Unexpected message"),
    };
    info!("New incoming connection from {}:{}", remote_id, src_port);
    let mut conn =
        UnixStream::connect(socket_path).expect("Unable to connect Unix socket to UIP daemon");
    let msg = Frame::Accept(remote_id, port, src_port);
    let mut buf = Vec::new();
    msg.serialize(&mut Serializer::new(&mut buf))
        .expect("Unable to serialize connect message");
    conn.write(&buf).expect("Unable to send connect message");
    loop {
        let mut buf = [0; 1500];
        let n = conn.read(&mut buf[..]).expect("Unable to read message");
        let frame: Frame = from_slice(&buf[..n]).expect("Unable to parse data message");
        match frame {
            Frame::Data(data) => info!("Received new data: {:?}", data),
            _ => return error!("Unexpected message"),
        };
    }
}

fn connect(socket_path: &str, remote_id: Identifier, port: u16) {
    let mut conn =
        UnixStream::connect(socket_path).expect("Unable to connect Unix socket to UIP daemon");
    let msg = Frame::Connect(remote_id, port);
    let mut buf = Vec::new();
    msg.serialize(&mut Serializer::new(&mut buf))
        .expect("Unable to serialize connect message");
    conn.write(&buf).expect("Unable to send connect message");
    loop {
        let mut input = String::new();
        match stdin().read_line(&mut input) {
            Ok(_) => {
                let mut buf = Vec::new();
                Frame::Data(input.as_bytes().to_vec())
                    .serialize(&mut Serializer::new(&mut buf))
                    .expect("Unable to serialize data message");
                conn.write(&buf).expect("Unable to send connect message");
            }
            Err(err) => return error!("Error reading from stdin: {}", err),
        }
    }
}

fn app() -> App<'static, 'static> {
    App::new("Netcat for UIP")
        .version("0.1")
        .author("Felix K. Maurer <maufl@maufl.de>")
        .about("A simple utility that connects to the UIP daemon and can be used to send or receive data.")
        .arg(Arg::with_name("socket")
             .short("s")
             .long("socket")
             .help("The socket of the UIP daemon to connect to.")
             .takes_value(true)
             .required(true))
        .arg(Arg::with_name("listen")
             .short("l")
             .long("listen"))
        .arg(Arg::with_name("remote_id")
             .short("i")
             .long("id")
             .help("The id of the peer to connect to")
             .takes_value(true)
             .required_unless("listen"))
        .arg(Arg::with_name("port")
             .short("p")
             .long("port")
             .help("The port to connect to or listen on")
             .takes_value(true)
             .required(true))
}
