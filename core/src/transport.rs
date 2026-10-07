//! QUIC endpoints with LanPilot's TLS and timing settings.

use crate::identity::{Identity, IdentityError, PublicKey, public_key_from_cert};
use crate::tls::{TlsError, client_config, server_config};
use quinn::crypto::rustls::{QuicClientConfig, QuicServerConfig};
use rustls::pki_types::CertificateDer;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

pub const ALPN: &[u8] = b"lanpilot";
pub const DEFAULT_PORT: u16 = 45810;
pub const KEEP_ALIVE: Duration = Duration::from_secs(1);
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(3);
/// TLS server name used on connect. Certificates are not checked against it.
const SERVER_NAME: &str = "lanpilot";

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error(transparent)]
    Identity(#[from] IdentityError),
    #[error(transparent)]
    Tls(#[from] rustls::Error),
    #[error("quic config: {0}")]
    QuicConfig(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Connect(#[from] quinn::ConnectError),
    #[error(transparent)]
    Connection(#[from] quinn::ConnectionError),
    #[error("peer presented no certificate")]
    NoPeerIdentity,
}

impl From<TlsError> for TransportError {
    fn from(e: TlsError) -> Self {
        match e {
            TlsError::Identity(e) => Self::Identity(e),
            TlsError::Rustls(e) => Self::Tls(e),
        }
    }
}

fn transport_config() -> Arc<quinn::TransportConfig> {
    let mut t = quinn::TransportConfig::default();
    t.keep_alive_interval(Some(KEEP_ALIVE));
    t.max_idle_timeout(Some(
        quinn::IdleTimeout::try_from(IDLE_TIMEOUT).expect("3 s is a valid idle timeout"),
    ));
    Arc::new(t)
}

pub fn server_endpoint(
    identity: &Identity,
    addr: SocketAddr,
) -> Result<quinn::Endpoint, TransportError> {
    let tls = server_config(identity, ALPN)?;
    let quic =
        QuicServerConfig::try_from(tls).map_err(|e| TransportError::QuicConfig(e.to_string()))?;
    let mut config = quinn::ServerConfig::with_crypto(Arc::new(quic));
    config.transport_config(transport_config());
    Ok(quinn::Endpoint::server(config, addr)?)
}

pub fn client_endpoint(identity: &Identity) -> Result<quinn::Endpoint, TransportError> {
    let tls = client_config(identity, ALPN)?;
    let quic =
        QuicClientConfig::try_from(tls).map_err(|e| TransportError::QuicConfig(e.to_string()))?;
    let mut config = quinn::ClientConfig::new(Arc::new(quic));
    config.transport_config(transport_config());
    let mut endpoint = quinn::Endpoint::client(SocketAddr::from(([0, 0, 0, 0], 0)))?;
    endpoint.set_default_client_config(config);
    Ok(endpoint)
}

pub async fn connect(
    endpoint: &quinn::Endpoint,
    addr: SocketAddr,
) -> Result<quinn::Connection, TransportError> {
    Ok(endpoint.connect(addr, SERVER_NAME)?.await?)
}

/// How long [`finish_and_confirm`] waits for the peer to acknowledge a stream.
pub const DELIVERY_TIMEOUT: Duration = Duration::from_secs(5);

/// Finishes `send` and waits (at most [`DELIVERY_TIMEOUT`]) until the peer has
/// acknowledged all of its data. Returns `true` only if it did.
///
/// Dropping or closing a connection discards stream data that is still unsent,
/// so a side that writes a final message and then hangs up must call this
/// first; afterwards the connection may be dropped at once.
pub async fn finish_and_confirm(send: &mut quinn::SendStream) -> bool {
    if send.finish().is_err() {
        return false;
    }
    // `Ok(None)`: finished and fully acknowledged. `Ok(Some(_))`: the peer
    // stopped the stream before reading everything.
    matches!(
        tokio::time::timeout(DELIVERY_TIMEOUT, send.stopped()).await,
        Ok(Ok(None))
    )
}

pub fn peer_public_key(conn: &quinn::Connection) -> Result<PublicKey, TransportError> {
    let identity = conn.peer_identity().ok_or(TransportError::NoPeerIdentity)?;
    let certs = identity
        .downcast::<Vec<CertificateDer<'static>>>()
        .map_err(|_| TransportError::NoPeerIdentity)?;
    let first = certs.first().ok_or(TransportError::NoPeerIdentity)?;
    Ok(public_key_from_cert(first.as_ref())?)
}
