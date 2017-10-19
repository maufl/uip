use openssl::rsa::Rsa;
use openssl::pkey::PKey;
use openssl::x509::{X509,X509Builder,X509NameBuilder};
use openssl::error::ErrorStack;
use openssl::hash::{hash2,MessageDigest};
use openssl::asn1::{Asn1Time};
use std::ops::Deref;
use serde::{Serialize,Deserialize,Serializer,Deserializer};

#[derive(Serialize, Deserialize)]
pub struct Id {
    pub hash: String,
    #[serde(deserialize_with="deserialize_key",serialize_with="serialize_key")]
    pub key: PKey,
    #[serde(deserialize_with="deserialize_certificate",serialize_with="serialize_certificate")]
    pub cert: X509
}
fn serialize_certificate<S>(x509: &X509, serializer: S) -> Result<S::Ok, S::Error> where S: Serializer {
    let pem = x509.to_pem().unwrap();
    let string = String::from_utf8(pem).unwrap();
    string.serialize(serializer)
}

fn deserialize_certificate<'de, D>(deserializer: D) -> Result<X509, D::Error> where D: Deserializer< 'de> {
    let string = String::deserialize(deserializer)?;
    Ok(X509::from_pem(string.as_bytes()).unwrap())
}
fn serialize_key<S>(pkey: &PKey, serializer: S) -> Result<S::Ok, S::Error> where S: Serializer {
    let pem = pkey.private_key_to_pem().unwrap();
    let string = String::from_utf8(pem).unwrap();
    string.serialize(serializer)
}

fn deserialize_key<'de, D>(deserializer: D) -> Result<PKey, D::Error> where D: Deserializer< 'de> {
    let string = String::deserialize(deserializer)?;
    Ok(PKey::private_key_from_pem(string.as_bytes()).unwrap())
}

impl Clone for Id {
    fn clone(&self) -> Id {
        Id {
            hash: self.hash.clone(),
            key: PKey::private_key_from_pem(&self.key.private_key_to_pem().expect("Unable to serialze key to pem")).expect("Unable to deserialize key from pem"),
            cert: self.cert.clone()
        }
    }
}

impl Id {

    pub fn generate() -> Result<Id, ErrorStack> {
        let key = Rsa::generate(2048)?;
        let pubkey_pem = key.public_key_to_pem()?;
        let hash: String = hash2(MessageDigest::sha256(), &pubkey_pem)?.iter().map(|byte| format!("{:02X}", byte) ).collect::<Vec<String>>().join("");
        let pkey = PKey::from_rsa(key)?;
        let x509_name = {
            let mut builder = X509NameBuilder::new()?;
            builder.append_entry_by_text("CN", hash.as_ref())?;
            builder.build()
        };
        let not_before = Asn1Time::days_from_now(0)?;
        let not_after = Asn1Time::days_from_now(356*2)?;
        let mut builder = X509Builder::new()?;
        builder.set_version(2)?;
        builder.set_not_before(&not_before)?;
        builder.set_not_after(&not_after)?;
        builder.set_subject_name(&x509_name)?;
        builder.set_issuer_name(&x509_name)?;
        builder.set_pubkey(pkey.deref())?;
        builder.sign(pkey.deref(), MessageDigest::sha256())?;
        let cert = builder.build();
        Ok(Id{
            hash: hash,
            key: pkey,
            cert: cert
        })
    }
}
