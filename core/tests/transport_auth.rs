//! Negative tests: the TLS layer must reject impostors and non-conforming certificates.

use lanpilot_core::identity::Identity;
use lanpilot_core::quinn::crypto::rustls::{QuicClientConfig, QuicServerConfig};
use lanpilot_core::quinn::{self, Endpoint};
use lanpilot_core::tls::provider;
use lanpilot_core::transport::{ALPN, client_endpoint, connect, server_endpoint};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::sign::CertifiedKey;
use rustls::{DigitallySignedStruct, DistinguishedName, SignatureScheme};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

fn localhost() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}

/// Accepts any certificate and any signature. Test-only.
#[derive(Debug)]
struct AcceptAll;

const SCHEMES: [SignatureScheme; 2] = [
    SignatureScheme::ED25519,
    SignatureScheme::ECDSA_NISTP256_SHA256,
];

impl ServerCertVerifier for AcceptAll {
    fn verify_server_cert(
        &self,
        _: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &CertificateDer<'_>,
        _: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }
    fn verify_tls13_signature(
        &self,
        _: &[u8],
        _: &CertificateDer<'_>,
        _: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        SCHEMES.to_vec()
    }
}

impl ClientCertVerifier for AcceptAll {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }
    fn verify_client_cert(
        &self,
        _: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: UnixTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        Ok(ClientCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &CertificateDer<'_>,
        _: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }
    fn verify_tls13_signature(
        &self,
        _: &[u8],
        _: &CertificateDer<'_>,
        _: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        SCHEMES.to_vec()
    }
}

/// Certificate of `cert_of`, signing key of `key_of`.
fn mismatched_certified_key(cert_of: &Identity, key_of: &Identity) -> Arc<CertifiedKey> {
    let (cert, _) = cert_of.certificate().unwrap();
    let (_, key) = key_of.certificate().unwrap();
    let signing = provider().key_provider.load_private_key(key).unwrap();
    Arc::new(CertifiedKey::new(vec![cert], signing))
}

#[derive(Debug)]
struct FixedCert(Arc<CertifiedKey>);

impl rustls::client::ResolvesClientCert for FixedCert {
    fn resolve(&self, _: &[&[u8]], _: &[SignatureScheme]) -> Option<Arc<CertifiedKey>> {
        Some(self.0.clone())
    }
    fn has_certs(&self) -> bool {
        true
    }
}

impl rustls::server::ResolvesServerCert for FixedCert {
    fn resolve(&self, _: rustls::server::ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        Some(self.0.clone())
    }
}

fn raw_client(mut config: rustls::ClientConfig) -> Endpoint {
    config.alpn_protocols = vec![ALPN.to_vec()];
    let quic = QuicClientConfig::try_from(config).unwrap();
    let mut endpoint = Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
    endpoint.set_default_client_config(quinn::ClientConfig::new(Arc::new(quic)));
    endpoint
}

fn client_builder() -> rustls::ConfigBuilder<rustls::ClientConfig, rustls::client::WantsClientCert>
{
    rustls::ClientConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .unwrap()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAll))
}

/// Drives a raw client against a lanpilot server and asserts that the server never
/// yields an established connection. (TLS 1.3 client auth is verified by the server
/// after the client considers the handshake done, so the client side may briefly succeed.)
async fn assert_server_rejects(client: Endpoint) {
    let server = server_endpoint(&Identity::generate(), localhost()).unwrap();
    let addr = server.local_addr().unwrap();
    let server_task = tokio::spawn(async move {
        let incoming = server.accept().await.unwrap();
        let result = incoming.await;
        (result.is_ok(), server)
    });

    let attempt = client.connect(addr, "lanpilot").unwrap().await;
    if let Ok(conn) = attempt {
        // The server must tear the connection down.
        tokio::time::timeout(Duration::from_secs(5), conn.closed())
            .await
            .expect("server closed the unauthenticated connection");
    }
    let (established, _server) = tokio::time::timeout(Duration::from_secs(5), server_task)
        .await
        .expect("server finished the handshake attempt")
        .unwrap();
    assert!(!established, "server accepted an unauthenticated client");
}

#[tokio::test]
async fn server_rejects_client_impostor() {
    let (a, b) = (Identity::generate(), Identity::generate());
    let config = client_builder()
        .with_client_cert_resolver(Arc::new(FixedCert(mismatched_certified_key(&a, &b))));
    assert_server_rejects(raw_client(config)).await;
}

#[tokio::test]
async fn client_rejects_server_impostor() {
    let (a, b) = (Identity::generate(), Identity::generate());
    let mut config = rustls::ServerConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .unwrap()
        .with_client_cert_verifier(Arc::new(AcceptAll))
        .with_cert_resolver(Arc::new(FixedCert(mismatched_certified_key(&a, &b))));
    config.alpn_protocols = vec![ALPN.to_vec()];
    let quic = QuicServerConfig::try_from(config).unwrap();
    let server = Endpoint::server(
        quinn::ServerConfig::with_crypto(Arc::new(quic)),
        localhost(),
    )
    .unwrap();
    let addr = server.local_addr().unwrap();
    let server_task = tokio::spawn(async move {
        if let Some(incoming) = server.accept().await {
            let _ = incoming.await;
        }
        server
    });

    let client = client_endpoint(&Identity::generate()).unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), connect(&client, addr))
        .await
        .expect("handshake finished");
    assert!(
        result.is_err(),
        "client accepted a server that cannot sign for its certificate"
    );
    let _ = server_task.await;
}

#[tokio::test]
async fn server_rejects_non_ed25519_certificate() {
    let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
    let cert = rcgen::CertificateParams::new(vec!["lanpilot".to_string()])
        .unwrap()
        .self_signed(&key)
        .unwrap();
    let der = CertificateDer::from(cert.der().to_vec());
    let key_der = PrivateKeyDer::try_from(key.serialize_der()).unwrap();
    let config = client_builder()
        .with_client_auth_cert(vec![der], key_der)
        .unwrap();
    assert_server_rejects(raw_client(config)).await;
}

#[tokio::test]
async fn server_rejects_client_without_certificate() {
    let config = client_builder().with_no_client_auth();
    assert_server_rejects(raw_client(config)).await;
}
