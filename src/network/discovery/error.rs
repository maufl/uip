use std::error::Error;
use std::fmt::{self, Display};

#[derive(Clone, Debug)]
pub enum AddressDiscoveryError {
    InterfacesError(String),
}

impl Display for AddressDiscoveryError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.description())
    }
}

impl Error for AddressDiscoveryError {
    fn description(&self) -> &str {
        match *self {
            AddressDiscoveryError::InterfacesError(ref string) => string,
        }
    }
}
