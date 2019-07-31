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
extern crate tokio;
extern crate tokio_openssl;

mod data;
pub mod unix;
mod configuration;
mod state;
mod network;
mod utils;

pub use configuration::Configuration;
pub use state::State;
pub use data::{Identifier, Identity, PeerInformationBase};
pub use utils::Shared;
pub use network::NetworkState;
pub use unix::UnixState;
