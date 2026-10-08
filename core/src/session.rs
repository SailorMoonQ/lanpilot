//! Opening the control stream: the client sends `StreamOpen` (Hello or a pair
//! request); for sessions the server answers with its own Hello and both sides
//! negotiate the protocol version independently.
//!
//! Authorization: servers accept streams with [`accept_authorized`] and
//! clients connect to paired servers with [`connect_paired`]. Both compare
//! the peer's TLS key, which is the only identity check in LanPilot.

use crate::framing::{FrameError, read_msg, write_msg};
use crate::identity::PublicKey;
use crate::proto::v1::{Hello, Os, PairRequest, StreamOpen, stream_open};
use crate::text::sanitize_display_name;
use crate::transport::{TransportError, connect, finish_and_confirm, peer_public_key};
use crate::version::{PROTO_MAX, PROTO_MIN, VersionMismatch, negotiate};
use std::net::SocketAddr;

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error(transparent)]
    Connection(#[from] quinn::ConnectionError),
    #[error(transparent)]
    Frame(#[from] FrameError),
    #[error("stream closed before the expected message")]
    Closed,
    #[error("unexpected message: {0}")]
    Unexpected(&'static str),
    #[error(transparent)]
    Version(#[from] VersionMismatch),
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error("the server is not the paired device")]
    UnexpectedPeer,
    #[error("the client is not paired")]
    NotPaired,
}

/// Application close code: the server's key is not the expected one.
pub const CLOSE_UNEXPECTED_PEER: u32 = 1;
/// Application close code: an unpaired client tried to open a session.
pub const CLOSE_NOT_PAIRED: u32 = 2;

/// Client side: connects to a paired server and checks that its TLS key is
/// `expected` before anything is sent. On a mismatch the connection is closed
/// with [`CLOSE_UNEXPECTED_PEER`] and `SessionError::UnexpectedPeer` returned.
pub async fn connect_paired(
    endpoint: &quinn::Endpoint,
    addr: SocketAddr,
    expected: &PublicKey,
) -> Result<quinn::Connection, SessionError> {
    let conn = connect(endpoint, addr).await?;
    match peer_public_key(&conn) {
        Ok(key) if key == *expected => Ok(conn),
        Ok(_) => {
            conn.close(CLOSE_UNEXPECTED_PEER.into(), b"unexpected peer");
            Err(SessionError::UnexpectedPeer)
        }
        Err(e) => {
            conn.close(CLOSE_UNEXPECTED_PEER.into(), b"no peer identity");
            Err(e.into())
        }
    }
}

/// Server side: the intended entry point for every incoming stream. Accepts
/// the next stream like [`accept_open`], but if the first message is a Hello
/// from a peer for which `is_paired` returns false, closes the connection with
/// [`CLOSE_NOT_PAIRED`] and returns `SessionError::NotPaired`. Pairing
/// requests are always allowed, since they are how a device becomes paired.
///
/// This only guards streams. Callers must also ignore datagrams (pointer
/// input) on connections that have not opened an authorized session.
pub async fn accept_authorized(
    conn: &quinn::Connection,
    is_paired: impl Fn(&PublicKey) -> bool,
) -> Result<Opened, SessionError> {
    let peer = peer_public_key(conn)?;
    let opened = accept_open(conn).await?;
    if matches!(opened, Opened::Session { .. }) && !is_paired(&peer) {
        conn.close(CLOSE_NOT_PAIRED.into(), b"not paired");
        return Err(SessionError::NotPaired);
    }
    Ok(opened)
}

/// Our Hello. `device_name` is sanitized like any name a peer would display.
pub fn local_hello(device_name: &str, os: Os, app_version: &str, capabilities: &[&str]) -> Hello {
    Hello {
        proto_min: PROTO_MIN,
        proto_max: PROTO_MAX,
        app_version: app_version.to_owned(),
        capabilities: capabilities.iter().map(|c| (*c).to_owned()).collect(),
        device_name: sanitize_display_name(device_name),
        os: os as i32,
    }
}

pub enum Opened {
    Session {
        send: quinn::SendStream,
        recv: quinn::RecvStream,
        hello: Hello,
    },
    Pair {
        send: quinn::SendStream,
        recv: quinn::RecvStream,
        request: PairRequest,
    },
}

/// Low-level: accepts the next stream and reads its `StreamOpen` without any
/// authorization. Servers should use [`accept_authorized`] instead.
pub async fn accept_open(conn: &quinn::Connection) -> Result<Opened, SessionError> {
    let (send, mut recv) = conn.accept_bi().await?;
    let open: StreamOpen = read_msg(&mut recv).await?.ok_or(SessionError::Closed)?;
    match open.kind {
        Some(stream_open::Kind::Hello(mut hello)) => {
            hello.device_name = sanitize_display_name(&hello.device_name);
            Ok(Opened::Session { send, recv, hello })
        }
        Some(stream_open::Kind::Pair(request)) => Ok(Opened::Pair {
            send,
            recv,
            request,
        }),
        None => Err(SessionError::Unexpected("empty StreamOpen")),
    }
}

/// Sends the server Hello and negotiates the version with the client's Hello.
///
/// On a version mismatch the Hello is still sent (so the client can tell the
/// user which side to update), then the stream is finished and its delivery
/// awaited (bounded by [`DELIVERY_TIMEOUT`](crate::transport::DELIVERY_TIMEOUT))
/// before `Err(SessionError::Version(..))` is returned, so the caller may drop
/// the connection immediately.
pub async fn answer_hello(
    send: &mut quinn::SendStream,
    local: &Hello,
    remote: &Hello,
) -> Result<u32, SessionError> {
    write_msg(send, local).await?;
    match negotiate(
        (local.proto_min, local.proto_max),
        (remote.proto_min, remote.proto_max),
    ) {
        Ok(version) => Ok(version),
        Err(mismatch) => {
            // Best effort: the mismatch is the error worth reporting either way.
            finish_and_confirm(send).await;
            Err(mismatch.into())
        }
    }
}

pub struct ClientSession {
    pub send: quinn::SendStream,
    pub recv: quinn::RecvStream,
    pub server_hello: Hello,
    pub version: u32,
}

pub async fn open_session(
    conn: &quinn::Connection,
    local: &Hello,
) -> Result<ClientSession, SessionError> {
    let (mut send, mut recv) = conn.open_bi().await?;
    let open = StreamOpen {
        kind: Some(stream_open::Kind::Hello(local.clone())),
    };
    write_msg(&mut send, &open).await?;
    let mut server_hello: Hello = read_msg(&mut recv).await?.ok_or(SessionError::Closed)?;
    server_hello.device_name = sanitize_display_name(&server_hello.device_name);
    let version = negotiate(
        (local.proto_min, local.proto_max),
        (server_hello.proto_min, server_hello.proto_max),
    )?;
    Ok(ClientSession {
        send,
        recv,
        server_hello,
        version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_hello_sanitizes_device_name() {
        let hello = local_hello(" \u{202E}Desk\n", Os::Linux, "0.1.0", &[]);
        assert_eq!(hello.device_name, "Desk");
    }
}
