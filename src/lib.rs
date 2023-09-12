#![feature(ip)]

#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;

mod configuration;
mod data;
mod network;
mod rpc;
pub mod unix;
mod utils;

mod uipd_capnp {
    include!(concat!(env!("OUT_DIR"), "/uipd_capnp.rs"));
}

pub use configuration::Configuration;
pub use data::{Identifier, Identity, PeerInformationBase};
pub use network::NetworkState;
pub use unix::UnixState;
pub use utils::Shared;
