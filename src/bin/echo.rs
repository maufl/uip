extern crate byteorder;
extern crate bytes;

use std::os::unix::net::UnixStream;
use std::env::args;
use std::io::{Write, BufRead, stdin};
use byteorder::BigEndian;
use bytes::BytesMut;
use bytes::buf::BufMut;


fn main() {
    let socket_address = args().nth(1).expect("No socket address provided");
    let host_id = args().nth(2).expect("No host id provided");
    let channel_id = args()
        .nth(3)
        .expect("No channel id provided")
        .parse::<u16>()
        .expect("Invalid channel id");

    let mut connect = BytesMut::with_capacity(5 + host_id.len());
    connect.put_u8(1);
    connect.put_u16::<BigEndian>(host_id.len() as u16);
    connect.put_slice(host_id.as_bytes());
    connect.put_u16::<BigEndian>(channel_id);

    let mut socket =
        UnixStream::connect(&socket_address).expect("Unable to connect to unix socket");
    socket.write_all(&connect).expect(
        "Unable to write to socket",
    );
    let stdin = stdin();
    for line in stdin.lock().lines() {
        println!("> ");
        let line = match line {
            Ok(l) => l,
            Err(err) => return println!("Error while reading line: {}", err),
        };
        let mut data = BytesMut::with_capacity(3 + line.len());
        data.put_u8(2);
        data.put_u16::<BigEndian>(line.len() as u16);
        data.put_slice(line.as_bytes());
        socket.write_all(&data).expect("Unable to write to socket");
        //let mut buf = [0u8;1500];
        //socket.read(&mut buf).expect("Unable to read from socket");
    }
}
