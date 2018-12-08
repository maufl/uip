use openssl::rsa::Rsa;
use openssl::pkey::{PKey, Private};
use openssl::x509::{X509, X509Builder, X509NameBuilder};
use openssl::error::ErrorStack;
use openssl::hash::MessageDigest;
use openssl::asn1::Asn1Time;
use std::ops::Deref;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use Identifier;

#[derive(Serialize, Deserialize)]
pub struct Identity {
    pub identifier: Identifier,
    #[serde(deserialize_with = "deserialize_key", serialize_with = "serialize_key")]
    pub key: PKey<Private>,
    #[serde(deserialize_with = "deserialize_certificate",
            serialize_with = "serialize_certificate")]
    pub cert: X509,
}

impl Clone for Identity {
    fn clone(&self) -> Identity {
        Identity {
            identifier: self.identifier,
            key: PKey::<Private>::private_key_from_pem(&self.key
                .private_key_to_pem_pkcs8()
                .expect("Unable to serialze key to pem"))
                .expect(
                "Unable to deserialize key from pem",
            ),
            cert: self.cert.clone(),
        }
    }
}

impl Identity {
    pub fn generate() -> Result<Identity, ErrorStack> {
        let key = Rsa::generate(2048)?;
        let private_key = PKey::<Private>::from_rsa(key)?;
        let identifier = Identifier::from_public_key(&private_key)?;
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
        builder.set_pubkey(private_key.deref())?;
        builder.sign(private_key.deref(), MessageDigest::sha256())?;
        let cert = builder.build();
        Ok(Identity {
            identifier: identifier,
            key: private_key,
            cert: cert,
        })
    }
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
fn serialize_key<S>(pkey: &PKey<Private>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let pem = pkey.private_key_to_pem_pkcs8().unwrap();
    let string = String::from_utf8(pem).unwrap();
    string.serialize(serializer)
}

fn deserialize_key<'de, D>(deserializer: D) -> Result<PKey<Private>, D::Error>
where
    D: Deserializer<'de>,
{
    let string = String::deserialize(deserializer)?;
    Ok(PKey::private_key_from_pem(string.as_bytes()).unwrap())
}
