//! Long-term Ed25519 device identity and the self-signed certificate used for TLS.

use ed25519_dalek::SigningKey;
use ed25519_dalek::pkcs8::EncodePrivateKey;
use rand_core::OsRng;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error)]
pub enum IdentityError {
    #[error("invalid public key")]
    InvalidKey,
    #[error("certificate error: {0}")]
    Certificate(String),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PublicKey(pub [u8; 32]);

impl PublicKey {
    pub fn from_slice(bytes: &[u8]) -> Result<Self, IdentityError> {
        let arr: [u8; 32] = bytes.try_into().map_err(|_| IdentityError::InvalidKey)?;
        Ok(Self(arr))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn fingerprint(&self) -> String {
        hex::encode(Sha256::digest(self.0))
    }

    pub fn short_id(&self) -> String {
        self.fingerprint()[..16].to_owned()
    }
}

pub struct Identity {
    signing: SigningKey,
}

impl std::fmt::Debug for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Identity")
            .field("public_key", &self.public_key().short_id())
            .finish_non_exhaustive()
    }
}

impl Identity {
    pub fn generate() -> Self {
        Self {
            signing: SigningKey::generate(&mut OsRng),
        }
    }

    pub fn from_secret_bytes(secret: &[u8; 32]) -> Self {
        Self {
            signing: SigningKey::from_bytes(secret),
        }
    }

    pub fn secret_bytes(&self) -> [u8; 32] {
        self.signing.to_bytes()
    }

    pub fn public_key(&self) -> PublicKey {
        PublicKey(self.signing.verifying_key().to_bytes())
    }

    /// Self-signed certificate over this identity's key. Peers never validate
    /// the chain; they compare the embedded public key with their paired list.
    pub fn certificate(
        &self,
    ) -> Result<(CertificateDer<'static>, PrivateKeyDer<'static>), IdentityError> {
        let err = |e: &dyn std::fmt::Display| IdentityError::Certificate(e.to_string());
        let pkcs8 = self.signing.to_pkcs8_der().map_err(|e| err(&e))?;
        let pkcs8_bytes = pkcs8.as_bytes().to_vec();
        let key_pair = rcgen::KeyPair::try_from(pkcs8_bytes.as_slice()).map_err(|e| err(&e))?;
        let params =
            rcgen::CertificateParams::new(vec!["lanpilot".to_owned()]).map_err(|e| err(&e))?;
        let cert = params.self_signed(&key_pair).map_err(|e| err(&e))?;
        Ok((
            cert.der().clone(),
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(pkcs8_bytes)),
        ))
    }
}

/// Extracts the Ed25519 public key from a DER certificate.
pub fn public_key_from_cert(der: &[u8]) -> Result<PublicKey, IdentityError> {
    let (_, cert) = x509_parser::parse_x509_certificate(der)
        .map_err(|e| IdentityError::Certificate(e.to_string()))?;
    let spki = cert.public_key();
    if spki.algorithm.algorithm != x509_parser::oid_registry::OID_SIG_ED25519 {
        return Err(IdentityError::InvalidKey);
    }
    PublicKey::from_slice(&spki.subject_public_key.data)
}

#[cfg(test)]
mod tests {
    use super::*;

    // RFC 8032 section 7.1, test 1.
    const SECRET: &str = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";
    const PUBLIC: &str = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";

    fn rfc_identity() -> Identity {
        let mut secret = [0u8; 32];
        hex::decode_to_slice(SECRET, &mut secret).unwrap();
        Identity::from_secret_bytes(&secret)
    }

    #[test]
    fn derives_known_public_key() {
        assert_eq!(hex::encode(rfc_identity().public_key().as_bytes()), PUBLIC);
    }

    #[test]
    fn secret_round_trips() {
        let id = Identity::generate();
        let restored = Identity::from_secret_bytes(&id.secret_bytes());
        assert_eq!(id.public_key(), restored.public_key());
    }

    #[test]
    fn fingerprint_and_short_id() {
        let pk = rfc_identity().public_key();
        let fp = pk.fingerprint();
        assert_eq!(fp.len(), 64);
        assert!(
            fp.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
        assert_eq!(pk.short_id(), fp[..16]);
    }

    #[test]
    fn certificate_carries_public_key() {
        let id = Identity::generate();
        let (cert, _key) = id.certificate().unwrap();
        assert_eq!(
            public_key_from_cert(cert.as_ref()).unwrap(),
            id.public_key()
        );
    }

    #[test]
    fn rejects_garbage_cert() {
        assert!(public_key_from_cert(&[1, 2, 3]).is_err());
    }

    #[test]
    fn from_slice_checks_length() {
        assert!(PublicKey::from_slice(&[0u8; 31]).is_err());
        assert!(PublicKey::from_slice(&[0u8; 32]).is_ok());
    }

    #[test]
    fn debug_does_not_leak_secret() {
        let id = rfc_identity();
        assert!(!format!("{id:?}").contains(SECRET));
    }
}
