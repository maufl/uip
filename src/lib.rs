#![feature(ip)]
#![feature(nll)]

#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;

mod data;
pub mod unix;
mod configuration;
mod network;
mod utils;

pub use configuration::Configuration;
pub use data::{Identifier, Identity, PeerInformationBase};
pub use utils::Shared;
pub use network::NetworkState;
pub use unix::UnixState;
