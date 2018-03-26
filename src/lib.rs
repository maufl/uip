#![feature(conservative_impl_trait)]
#![feature(ip)]
#![feature(nll)]

extern crate byteorder;
extern crate bytes;
extern crate futures;
extern crate igd;
extern crate interfaces;
extern crate libc;
#[macro_use]
extern crate log;
extern crate mio;
extern crate nix;
extern crate openssl;
extern crate rand;
extern crate rmp_serde;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate tokio_core;
extern crate tokio_io;
extern crate tokio_openssl;
extern crate tokio_uds;

mod data;
mod unix;
mod configuration;
mod state;
mod network;
mod utils;

pub use configuration::Configuration;
pub use state::State;
pub use data::{Identifier, Identity};
pub use utils::Shared;
