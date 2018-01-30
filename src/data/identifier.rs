use openssl::pkey::PKey;
use openssl::x509::X509;
use openssl::error::ErrorStack;
use openssl::sha::sha256;
use std::fmt;
use std::str::FromStr;
use std::default::Default;
use serde::{Serialize, Deserialize, Serializer, Deserializer};
use serde::de::{Error, Unexpected};

const IDENTIFIER_LENGTH: usize = 32;

#[derive(Copy, Clone, Default, PartialEq, Eq, Hash)]
pub struct Identifier([u8; IDENTIFIER_LENGTH]);

impl Identifier {
    pub fn from_x509_certificate(x509: &X509) -> Result<Identifier, ErrorStack> {
        let pub_key = x509.public_key()?;
        Identifier::from_public_key(&pub_key)
    }

    pub fn from_public_key(key: &PKey) -> Result<Identifier, ErrorStack> {
        let pub_key_der = key.public_key_to_der()?;
        let identifier = sha256(&pub_key_der);
        Ok(Identifier(identifier))
    }

    pub fn copy_from_slice(slice: &[u8]) -> Result<Identifier, String> {
        if slice.len() < IDENTIFIER_LENGTH {
            return Err("Slice length insufficient".to_string());
        }
        let mut identifier = [0u8; IDENTIFIER_LENGTH];
        identifier.copy_from_slice(&slice[..IDENTIFIER_LENGTH]);
        Ok(Identifier(identifier))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl FromStr for Identifier {
    type Err = String;

    fn from_str(string: &str) -> Result<Identifier, String> {
        if string.len() != 2 * IDENTIFIER_LENGTH {
            return Err("String length insufficient".to_string());
        }
        let mut ident = [0u8; 32];
        for i in 0..32 {
            ident[i] = u8::from_str_radix(&string[i * 2..i * 2 + 2], 16).map_err(
                |_err| "Invalid characters in string",
            )?;
        }
        Ok(Identifier(ident))
    }
}

impl fmt::Display for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        for byte in &self.0 {
            write!(f, "{:02X}", byte)?;
        }
        Ok(())
    }
}

impl fmt::Debug for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        fmt::Display::fmt(self, f)
    }
}

impl Serialize for Identifier {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_string().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Identifier {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Identifier, D::Error> {
        let string = String::deserialize(deserializer)?;
        if string.len() != 2 * IDENTIFIER_LENGTH {
            return Err(D::Error::invalid_length(string.len(), &"64 characters"));
        }
        let mut ident = [0u8; IDENTIFIER_LENGTH];
        for i in 0..IDENTIFIER_LENGTH {
            ident[i] = u8::from_str_radix(&string[i * 2..i * 2 + 2], 16).map_err(
                |_err| {
                    D::Error::invalid_value(
                        Unexpected::Str(&string[i * 2..i * 2 + 2]),
                        &"a valid hex value",
                    )
                },
            )?;
        }
        Ok(Identifier(ident))
    }
}
