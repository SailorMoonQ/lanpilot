//! rustls configuration: TLS 1.3 only, self-signed Ed25519 certificates on both
//! sides, handshake signatures verified, authorization left to the caller.

use crate::identity::{Identity, IdentityError, public_key_from_cert};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, WebPkiSupportedAlgorithms, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{DigitallySignedStruct, DistinguishedName, SignatureScheme};
use std::sync::Arc;

pub fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

#[derive(Debug)]
struct SelfSignedEd25519 {
    algorithms: WebPkiSupportedAlgorithms,
}

impl SelfSignedEd25519 {
    fn new(provider: &CryptoProvider) -> Arc<Self> {
        Arc::new(Self {
            algorithms: provider.signature_verification_algorithms,
        })
    }

    fn check_cert(end_entity: &CertificateDer<'_>) -> Result<(), rustls::Error> {
        public_key_from_cert(end_entity.as_ref())
            .map(|_| ())
            .map_err(|_| rustls::Error::InvalidCertificate(rustls::CertificateError::BadEncoding))
    }

    fn tls12_refused() -> Result<HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::General("TLS 1.2 is not supported".into()))
    }
}

impl ServerCertVerifier for SelfSignedEd25519 {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Self::check_cert(end_entity)?;
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Self::tls12_refused()
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SignatureScheme::ED25519]
    }
}

impl ClientCertVerifier for SelfSignedEd25519 {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        Self::check_cert(end_entity)?;
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Self::tls12_refused()
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SignatureScheme::ED25519]
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TlsError {
    #[error(transparent)]
    Identity(#[from] IdentityError),
    #[error(transparent)]
    Rustls(#[from] rustls::Error),
}

pub fn server_config(identity: &Identity, alpn: &[u8]) -> Result<rustls::ServerConfig, TlsError> {
    let provider = provider();
    let (cert, key) = identity.certificate()?;
    let mut config = rustls::ServerConfig::builder_with_provider(provider.clone())
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .with_client_cert_verifier(SelfSignedEd25519::new(&provider))
        .with_single_cert(vec![cert], key)?;
    config.alpn_protocols = vec![alpn.to_vec()];
    Ok(config)
}

pub fn client_config(identity: &Identity, alpn: &[u8]) -> Result<rustls::ClientConfig, TlsError> {
    let provider = provider();
    let (cert, key) = identity.certificate()?;
    let mut config = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .dangerous()
        .with_custom_certificate_verifier(SelfSignedEd25519::new(&provider))
        .with_client_auth_cert(vec![cert], key)?;
    config.alpn_protocols = vec![alpn.to_vec()];
    Ok(config)
}
