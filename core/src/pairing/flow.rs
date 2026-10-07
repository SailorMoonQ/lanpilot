//! Pairing over a QUIC connection. See spec sections 4.2 and 4.3.

use crate::framing::{FrameError, read_msg, write_msg};
use crate::identity::PublicKey;
use crate::pairing::invite::Invite;
use crate::pairing::password::{
    ClientHandshake, Role, confirmation, server_handshake, verify_confirmation,
};
use crate::proto::v1::{
    Os, PairChallenge, PairConfirm, PairRejectReason, PairRequest, PairResult, PairServerMessage,
    PasswordPairing, QrPairing, StreamOpen, pair_request, pair_server_message, stream_open,
};
use crate::session::CLOSE_UNEXPECTED_PEER;
use crate::text::sanitize_display_name;
use crate::transport::{TransportError, connect, finish_and_confirm, peer_public_key};
use std::net::SocketAddr;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub struct NewDevice {
    pub public_key: PublicKey,
    pub name: String,
    pub os: Os,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PairedServer {
    pub public_key: PublicKey,
    pub name: String,
    pub os: Os,
}

#[derive(Debug, thiserror::Error)]
pub enum PairingFlowError {
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error(transparent)]
    Connection(#[from] quinn::ConnectionError),
    #[error(transparent)]
    Frame(#[from] FrameError),
    #[error("stream closed early")]
    Closed,
    #[error("unexpected message: {0}")]
    Unexpected(&'static str),
    #[error("the device that answered is not the one in the pairing code")]
    ServerKeyMismatch,
    #[error("wrong password")]
    WrongPassword,
    #[error("pairing rejected: {reason:?}")]
    Rejected {
        reason: PairRejectReason,
        retry_after: Duration,
    },
    #[error("pairing code has no reachable address")]
    NoAddress,
}

pub trait PairingAuthority: Send + Sync {
    fn server_name(&self) -> String;
    fn server_os(&self) -> Os;
    fn consume_token(&self, token: &[u8]) -> bool;
    fn password(&self) -> Option<String>;
    /// Reserves and counts one password attempt. `Err` means locked, with the time left.
    fn password_begin(&self) -> Result<(), Duration>;
    fn password_succeeded(&self);
    fn approve(&self, device: &NewDevice) -> impl Future<Output = bool> + Send;
}

fn os_from(raw: i32) -> Os {
    Os::try_from(raw).unwrap_or(Os::Unspecified)
}

fn finish(send: &mut quinn::SendStream) -> Result<(), PairingFlowError> {
    send.finish().map_err(|_| PairingFlowError::Closed)
}

/// Rejection to send: reason plus optional retry delay.
type Reject = (PairRejectReason, Duration);

/// Serves one pairing request received on `send`/`recv`.
///
/// Returns `Ok(Some(device))` when the device was approved and the accepted
/// `PairResult` was delivered, `Ok(None)` when a rejection was delivered.
///
/// The result stream is finished and its delivery confirmed (bounded by
/// [`DELIVERY_TIMEOUT`](crate::transport::DELIVERY_TIMEOUT)) before this
/// returns, so the caller may drop the connection immediately afterwards.
/// If delivery cannot be confirmed this returns `Err(PairingFlowError::Closed)`.
///
/// `approve()` runs before delivery; if delivery fails the caller should treat
/// the pairing as uncertain and may remove the device.
pub async fn serve_pairing<A: PairingAuthority>(
    conn: &quinn::Connection,
    server_key: &PublicKey,
    mut send: quinn::SendStream,
    mut recv: quinn::RecvStream,
    request: PairRequest,
    authority: &A,
) -> Result<Option<NewDevice>, PairingFlowError> {
    let device = NewDevice {
        public_key: peer_public_key(conn)?,
        name: sanitize_display_name(&request.device_name),
        os: os_from(request.os),
    };

    let verdict: Result<(), Reject> = match request.method {
        Some(pair_request::Method::Qr(qr)) => {
            if authority.consume_token(&qr.token) {
                Ok(())
            } else {
                Err((PairRejectReason::BadToken, Duration::ZERO))
            }
        }
        Some(pair_request::Method::Password(pw)) => {
            serve_password(
                &mut send,
                &mut recv,
                &pw,
                &device.public_key,
                server_key,
                authority,
            )
            .await?
        }
        None => Err((PairRejectReason::Unspecified, Duration::ZERO)),
    };

    let verdict = match verdict {
        Ok(()) if authority.approve(&device).await => Ok(()),
        Ok(()) => Err((PairRejectReason::Denied, Duration::ZERO)),
        Err(r) => Err(r),
    };

    let result = match verdict {
        Ok(()) => PairResult {
            accepted: true,
            reason: PairRejectReason::Unspecified as i32,
            server_name: authority.server_name(),
            server_os: authority.server_os() as i32,
            retry_after_secs: 0,
        },
        Err((reason, retry)) => PairResult {
            accepted: false,
            reason: reason as i32,
            server_name: String::new(),
            server_os: Os::Unspecified as i32,
            retry_after_secs: u32::try_from(retry.as_secs()).unwrap_or(u32::MAX),
        },
    };
    let accepted = result.accepted;
    write_msg(
        &mut send,
        &PairServerMessage {
            body: Some(pair_server_message::Body::Result(result)),
        },
    )
    .await?;
    if !finish_and_confirm(&mut send).await {
        return Err(PairingFlowError::Closed);
    }
    Ok(accepted.then_some(device))
}

async fn serve_password<A: PairingAuthority>(
    send: &mut quinn::SendStream,
    recv: &mut quinn::RecvStream,
    request: &PasswordPairing,
    client_key: &PublicKey,
    server_key: &PublicKey,
    authority: &A,
) -> Result<Result<(), Reject>, PairingFlowError> {
    let Some(password) = authority.password() else {
        return Ok(Err((PairRejectReason::Disabled, Duration::ZERO)));
    };
    if let Err(left) = authority.password_begin() {
        return Ok(Err((PairRejectReason::Locked, left)));
    }
    // The attempt is already counted as a failure by `password_begin`.
    let Ok((spake_msg, key)) = server_handshake(&password, &request.spake_msg) else {
        return Ok(Err((PairRejectReason::BadPassword, Duration::ZERO)));
    };
    let challenge = PairChallenge {
        spake_msg,
        server_confirm: confirmation(&key, Role::Server, client_key, server_key),
    };
    write_msg(
        send,
        &PairServerMessage {
            body: Some(pair_server_message::Body::Challenge(challenge)),
        },
    )
    .await?;

    let confirm: PairConfirm = read_msg(recv).await?.ok_or(PairingFlowError::Closed)?;
    if verify_confirmation(
        &key,
        Role::Client,
        client_key,
        server_key,
        &confirm.client_confirm,
    ) {
        authority.password_succeeded();
        Ok(Ok(()))
    } else {
        Ok(Err((PairRejectReason::BadPassword, Duration::ZERO)))
    }
}

async fn read_result(recv: &mut quinn::RecvStream) -> Result<PairResult, PairingFlowError> {
    let msg: PairServerMessage = read_msg(recv).await?.ok_or(PairingFlowError::Closed)?;
    match msg.body {
        Some(pair_server_message::Body::Result(r)) => Ok(r),
        _ => Err(PairingFlowError::Unexpected("expected PairResult")),
    }
}

fn into_paired(
    result: PairResult,
    server_key: PublicKey,
) -> Result<PairedServer, PairingFlowError> {
    if result.accepted {
        Ok(PairedServer {
            public_key: server_key,
            name: sanitize_display_name(&result.server_name),
            os: os_from(result.server_os),
        })
    } else {
        Err(PairingFlowError::Rejected {
            reason: PairRejectReason::try_from(result.reason)
                .unwrap_or(PairRejectReason::Unspecified),
            retry_after: Duration::from_secs(u64::from(result.retry_after_secs)),
        })
    }
}

async fn send_open(
    conn: &quinn::Connection,
    request: PairRequest,
) -> Result<(quinn::SendStream, quinn::RecvStream), PairingFlowError> {
    let (mut send, recv) = conn.open_bi().await?;
    let open = StreamOpen {
        kind: Some(stream_open::Kind::Pair(request)),
    };
    write_msg(&mut send, &open).await?;
    Ok((send, recv))
}

pub async fn pair_with_invite(
    endpoint: &quinn::Endpoint,
    invite: &Invite,
    device_name: &str,
    os: Os,
) -> Result<PairedServer, PairingFlowError> {
    let mut last_err = PairingFlowError::NoAddress;
    let mut conn = None;
    for ip in &invite.addrs {
        match connect(endpoint, SocketAddr::new(*ip, invite.port)).await {
            Ok(c) => {
                conn = Some(c);
                break;
            }
            Err(e) => last_err = e.into(),
        }
    }
    let conn = conn.ok_or(last_err)?;

    let server_key = peer_public_key(&conn)?;
    if server_key != invite.server_public_key {
        conn.close(CLOSE_UNEXPECTED_PEER.into(), b"server key mismatch");
        return Err(PairingFlowError::ServerKeyMismatch);
    }

    let request = PairRequest {
        device_name: device_name.to_owned(),
        os: os as i32,
        method: Some(pair_request::Method::Qr(QrPairing {
            token: invite.token.to_vec(),
        })),
    };
    let (_send, mut recv) = send_open(&conn, request).await?;
    let result = read_result(&mut recv).await;
    conn.close(0u32.into(), b"paired");
    into_paired(result?, server_key)
}

pub async fn pair_with_password(
    endpoint: &quinn::Endpoint,
    addr: SocketAddr,
    client_key: &PublicKey,
    password: &str,
    device_name: &str,
    os: Os,
) -> Result<PairedServer, PairingFlowError> {
    let conn = connect(endpoint, addr).await?;
    let server_key = peer_public_key(&conn)?;
    let outcome =
        password_exchange(&conn, client_key, &server_key, password, device_name, os).await;
    conn.close(0u32.into(), b"done");
    into_paired(outcome?, server_key)
}

async fn password_exchange(
    conn: &quinn::Connection,
    client_key: &PublicKey,
    server_key: &PublicKey,
    password: &str,
    device_name: &str,
    os: Os,
) -> Result<PairResult, PairingFlowError> {
    let (handshake, spake_msg) = ClientHandshake::start(password);
    let request = PairRequest {
        device_name: device_name.to_owned(),
        os: os as i32,
        method: Some(pair_request::Method::Password(PasswordPairing {
            spake_msg,
        })),
    };
    let (mut send, mut recv) = send_open(conn, request).await?;

    let first: PairServerMessage = read_msg(&mut recv).await?.ok_or(PairingFlowError::Closed)?;
    let challenge = match first.body {
        Some(pair_server_message::Body::Challenge(c)) => c,
        Some(pair_server_message::Body::Result(r)) => return Ok(r),
        None => return Err(PairingFlowError::Unexpected("empty PairServerMessage")),
    };

    let key = handshake
        .finish(&challenge.spake_msg)
        .map_err(|_| PairingFlowError::WrongPassword)?;
    if !verify_confirmation(
        &key,
        Role::Server,
        client_key,
        server_key,
        &challenge.server_confirm,
    ) {
        return Err(PairingFlowError::WrongPassword);
    }
    let confirm = PairConfirm {
        client_confirm: confirmation(&key, Role::Client, client_key, server_key),
    };
    write_msg(&mut send, &confirm).await?;
    finish(&mut send)?;
    read_result(&mut recv).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_name_from_result_is_sanitized() {
        let result = PairResult {
            accepted: true,
            server_name: "\u{202E}Desk\u{0}".into(),
            ..Default::default()
        };
        let paired = into_paired(result, PublicKey([1u8; 32])).unwrap();
        assert_eq!(paired.name, "Desk");
    }
}
