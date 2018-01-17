use openssl::rsa::Rsa;
use openssl::pkey::PKey;
use openssl::x509::{X509, X509Builder, X509NameBuilder};
use openssl::error::ErrorStack;
use openssl::hash::MessageDigest;
use openssl::sha::sha256;
use openssl::asn1::Asn1Time;
use std::ops::Deref;
use std::fmt;
use serde::{Serialize, Deserialize, Serializer, Deserializer};
use serde::de::{Error, Unexpected};

#[derive(Debug, Copy, Clone, Default, PartialEq, Eq)]
pub struct Identifier([u8; 32]);

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
}

impl fmt::Display for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        for byte in &self.0 {
            write!(f, "{:02X}", byte)?;
        }
        Ok(())
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
        if string.len() != 64 {
            return Err(D::Error::invalid_length(string.len(), &"64 characters"));
        }
        let mut ident = [0u8; 32];
        for i in 0..32 {
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

#[derive(Serialize, Deserialize)]
pub struct Identity {
    pub identifier: Identifier,
    #[serde(deserialize_with = "deserialize_key", serialize_with = "serialize_key")]
    pub key: PKey,
    #[serde(deserialize_with = "deserialize_certificate", serialize_with = "serialize_certificate")]
    pub cert: X509,
}
fn serialize_certificate<S>(x509: &X509, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let pem = x509.to_pem().unwrap();
    let string = String::from_utf8(pem).unwrap();
    string.serialize(serializer)
}

fn deserialize_certificate<'de, D>(deserializer: D) -> Result<X509, D::Error>
where
    D: Deserializer<'de>,
{
    let string = String::deserialize(deserializer)?;
    Ok(X509::from_pem(string.as_bytes()).unwrap())
}
fn serialize_key<S>(pkey: &PKey, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let pem = pkey.private_key_to_pem().unwrap();
    let string = String::from_utf8(pem).unwrap();
    string.serialize(serializer)
}

fn deserialize_key<'de, D>(deserializer: D) -> Result<PKey, D::Error>
where
    D: Deserializer<'de>,
{
    let string = String::deserialize(deserializer)?;
    Ok(PKey::private_key_from_pem(string.as_bytes()).unwrap())
}

impl Clone for Identity {
    fn clone(&self) -> Identity {
        Identity {
            identifier: self.identifier,
            key: PKey::private_key_from_pem(&self.key.private_key_to_pem().expect(
                "Unable to serialze key to pem",
            )).expect("Unable to deserialize key from pem"),
            cert: self.cert.clone(),
        }
    }
}

impl Identity {
    pub fn generate() -> Result<Identity, ErrorStack> {
        let key = Rsa::generate(2048)?;
        let pkey = PKey::from_rsa(key)?;
        let identifier = Identifier::from_public_key(&pkey)?;
        let x509_name = {
            let mut builder = X509NameBuilder::new()?;
            builder.append_entry_by_text("CN", &identifier.to_string())?;
            builder.build()
        };
        let not_before = Asn1Time::days_from_now(0)?;
        let not_after = Asn1Time::days_from_now(356 * 2)?;
        let mut builder = X509Builder::new()?;
        builder.set_version(2)?;
        builder.set_not_before(&not_before)?;
        builder.set_not_after(&not_after)?;
        builder.set_subject_name(&x509_name)?;
        builder.set_issuer_name(&x509_name)?;
        builder.set_pubkey(pkey.deref())?;
        builder.sign(pkey.deref(), MessageDigest::sha256())?;
        let cert = builder.build();
        Ok(Identity {
            identifier: identifier,
            key: pkey,
            cert: cert,
        })
    }
}
