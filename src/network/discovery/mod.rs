mod error;
mod upnp_igd;
mod interfaces;

pub use self::error::AddressDiscoveryError;
pub use self::upnp_igd::request_external_address;
pub use self::interfaces::discover_addresses;
