use std::error::Error;
use std::fmt::{self, Display};
use igd::{SearchError, AddAnyPortError};

#[derive(Clone, Debug)]
pub enum AddressDiscoveryError {
    IgdError(String),
    InterfacesError(String)
}

impl Display for AddressDiscoveryError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.description())
    }
}

impl Error for AddressDiscoveryError {
    fn description(&self) -> &str {
        match *self {
            AddressDiscoveryError::IgdError(ref string) |
            AddressDiscoveryError::InterfacesError(ref string) => string,
        }
    }
}

impl From<SearchError> for AddressDiscoveryError {
    fn from(err: SearchError) -> AddressDiscoveryError {
        AddressDiscoveryError::IgdError(format!(
            "Error while searching internet gateway device: {}",
            err
        ))
    }
}

impl From<AddAnyPortError> for AddressDiscoveryError {
    fn from(err: AddAnyPortError) -> AddressDiscoveryError {
        AddressDiscoveryError::IgdError(format!("Error while setting up port mapping: {}", err))
    }
}
