#![feature(conservative_impl_trait)]
#![feature(ip)]

extern crate interfaces;
extern crate igd;
extern crate bytes;
extern crate byteorder;
extern crate futures;
extern crate tokio_io;
extern crate tokio_uds;
extern crate tokio_core;
extern crate serde;
extern crate rmp_serde;
#[macro_use]
extern crate serde_derive;
extern crate openssl;
extern crate tokio_openssl;
#[macro_use]
extern crate log;
extern crate nix;
extern crate libc;
extern crate mio;

mod data;
mod unix;
mod configuration;
mod state;
mod network;

pub use configuration::Configuration;
pub use state::State;
pub use data::{Identity, Identifier};
