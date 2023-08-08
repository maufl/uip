#![feature(ip)]

#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;

mod configuration;
mod data;
mod network;
pub mod unix;
mod utils;

pub use configuration::Configuration;
pub use data::{Identifier, Identity, PeerInformationBase};
pub use network::NetworkState;
pub use unix::UnixState;
pub use utils::Shared;
