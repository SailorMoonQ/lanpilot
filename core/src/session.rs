//! Opening the control stream: the client sends `StreamOpen` (Hello or a pair
//! request); for sessions the server answers with its own Hello and both sides
//! negotiate the protocol version independently.

use crate::framing::{FrameError, read_msg, write_msg};
use crate::proto::v1::{Hello, Os, PairRequest, StreamOpen, stream_open};
use crate::version::{PROTO_MAX, PROTO_MIN, VersionMismatch, negotiate};

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
}

pub fn local_hello(device_name: &str, os: Os, app_version: &str, capabilities: &[&str]) -> Hello {
    Hello {
        proto_min: PROTO_MIN,
        proto_max: PROTO_MAX,
        app_version: app_version.to_owned(),
        capabilities: capabilities.iter().map(|c| (*c).to_owned()).collect(),
        device_name: device_name.to_owned(),
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

pub async fn accept_open(conn: &quinn::Connection) -> Result<Opened, SessionError> {
    let (send, mut recv) = conn.accept_bi().await?;
    let open: StreamOpen = read_msg(&mut recv).await?.ok_or(SessionError::Closed)?;
    match open.kind {
        Some(stream_open::Kind::Hello(hello)) => Ok(Opened::Session { send, recv, hello }),
        Some(stream_open::Kind::Pair(request)) => Ok(Opened::Pair {
            send,
            recv,
            request,
        }),
        None => Err(SessionError::Unexpected("empty StreamOpen")),
    }
}

pub async fn answer_hello(
    send: &mut quinn::SendStream,
    local: &Hello,
    remote: &Hello,
) -> Result<u32, SessionError> {
    write_msg(send, local).await?;
    Ok(negotiate(
        (local.proto_min, local.proto_max),
        (remote.proto_min, remote.proto_max),
    )?)
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
    let server_hello: Hello = read_msg(&mut recv).await?.ok_or(SessionError::Closed)?;
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
