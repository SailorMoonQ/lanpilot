//! Maps core errors and QUIC close codes to the plain types Dart sees.

use crate::api::types::{BridgeError, CloseReason, ErrorKind};
use lanpilot_core::pairing::flow::PairingFlowError;
use lanpilot_core::proto::v1::PairRejectReason;
use lanpilot_core::quinn::ConnectionError;
use lanpilot_core::session::{CLOSE_NOT_PAIRED, CLOSE_UNEXPECTED_PEER, SessionError};
use lanpilot_core::transport::TransportError;
use lanpilot_core::version::VersionMismatch;
use std::fmt;

/// Close codes the agent sends (agent/src/session.rs). Duplicated here because
/// the bridge does not depend on the agent crate.
pub const CLOSE_DEVICE_REMOVED: u32 = 3;
pub const CLOSE_UNPAIRED: u32 = 4;

impl BridgeError {
    pub(crate) fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            retry_after_secs: 0,
        }
    }
}

impl fmt::Display for BridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for BridgeError {}

pub fn close_reason(e: &ConnectionError) -> CloseReason {
    match e {
        ConnectionError::ApplicationClosed(close) => {
            match u32::try_from(close.error_code.into_inner()).unwrap_or(u32::MAX) {
                CLOSE_NOT_PAIRED => CloseReason::NotPaired,
                CLOSE_DEVICE_REMOVED => CloseReason::DeviceRemoved,
                CLOSE_UNPAIRED => CloseReason::Unpaired,
                CLOSE_UNEXPECTED_PEER => CloseReason::UnexpectedPeer,
                _ => CloseReason::ServerClosed,
            }
        }
        ConnectionError::LocallyClosed => CloseReason::Local,
        ConnectionError::TimedOut => CloseReason::TimedOut,
        _ => CloseReason::Lost,
    }
}

pub fn from_connection(e: &ConnectionError) -> BridgeError {
    let kind = match close_reason(e) {
        CloseReason::NotPaired => ErrorKind::NotPaired,
        CloseReason::DeviceRemoved => ErrorKind::DeviceRemoved,
        CloseReason::Unpaired => ErrorKind::Unpaired,
        CloseReason::UnexpectedPeer => ErrorKind::ServerKeyMismatch,
        CloseReason::TimedOut => ErrorKind::Timeout,
        CloseReason::Local | CloseReason::ServerClosed | CloseReason::Lost => ErrorKind::Closed,
    };
    BridgeError::new(kind, e.to_string())
}

/// How much an error says about the computer we want. When several candidate
/// addresses fail, the highest-ranked error is reported.
pub fn rank(kind: ErrorKind) -> u8 {
    match kind {
        ErrorKind::NotPaired
        | ErrorKind::DeviceRemoved
        | ErrorKind::Unpaired
        | ErrorKind::ServerTooOld
        | ErrorKind::AppTooOld => 4,
        ErrorKind::Timeout => 3,
        ErrorKind::Unreachable => 2,
        ErrorKind::ServerKeyMismatch => 1,
        _ => 0,
    }
}

impl From<TransportError> for BridgeError {
    fn from(e: TransportError) -> Self {
        match &e {
            TransportError::Connection(c) => from_connection(c),
            TransportError::Connect(_) | TransportError::Io(_) => {
                Self::new(ErrorKind::Unreachable, e.to_string())
            }
            TransportError::NoPeerIdentity => {
                Self::new(ErrorKind::ServerKeyMismatch, e.to_string())
            }
            _ => Self::new(ErrorKind::Internal, e.to_string()),
        }
    }
}

impl From<SessionError> for BridgeError {
    fn from(e: SessionError) -> Self {
        match e {
            SessionError::Connection(c) => from_connection(&c),
            SessionError::Transport(t) => t.into(),
            SessionError::Version(VersionMismatch::PeerTooOld) => {
                Self::new(ErrorKind::ServerTooOld, "the computer needs an update")
            }
            SessionError::Version(VersionMismatch::PeerTooNew) => {
                Self::new(ErrorKind::AppTooOld, "the app needs an update")
            }
            SessionError::UnexpectedPeer => Self::new(ErrorKind::ServerKeyMismatch, e.to_string()),
            SessionError::NotPaired => Self::new(ErrorKind::NotPaired, e.to_string()),
            SessionError::Frame(_) | SessionError::Closed => {
                Self::new(ErrorKind::Closed, e.to_string())
            }
            SessionError::Unexpected(_) => Self::new(ErrorKind::Internal, e.to_string()),
        }
    }
}

impl From<PairingFlowError> for BridgeError {
    fn from(e: PairingFlowError) -> Self {
        match e {
            PairingFlowError::Transport(t) => t.into(),
            PairingFlowError::Connection(c) => from_connection(&c),
            PairingFlowError::ServerKeyMismatch => {
                Self::new(ErrorKind::ServerKeyMismatch, e.to_string())
            }
            PairingFlowError::WrongPassword => Self::new(ErrorKind::WrongPassword, e.to_string()),
            PairingFlowError::NoAddress => Self::new(ErrorKind::Unreachable, e.to_string()),
            PairingFlowError::Rejected {
                reason,
                retry_after,
            } => {
                let kind = match reason {
                    PairRejectReason::BadToken => ErrorKind::BadToken,
                    PairRejectReason::BadPassword => ErrorKind::WrongPassword,
                    PairRejectReason::Locked => ErrorKind::PasswordLocked,
                    PairRejectReason::Disabled => ErrorKind::PasswordDisabled,
                    PairRejectReason::Denied | PairRejectReason::Unspecified => ErrorKind::Denied,
                };
                Self {
                    kind,
                    message: e.to_string(),
                    retry_after_secs: u32::try_from(retry_after.as_secs()).unwrap_or(u32::MAX),
                }
            }
            PairingFlowError::Frame(_) | PairingFlowError::Closed => {
                Self::new(ErrorKind::Closed, e.to_string())
            }
            PairingFlowError::Unexpected(_) => Self::new(ErrorKind::Internal, e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lanpilot_core::quinn::{ApplicationClose, VarInt};
    use std::time::Duration;

    fn app_close(code: u32) -> ConnectionError {
        ConnectionError::ApplicationClosed(ApplicationClose {
            error_code: VarInt::from_u32(code),
            reason: Vec::new().into(),
        })
    }

    #[test]
    fn close_codes_map_to_reasons() {
        assert_eq!(close_reason(&app_close(0)), CloseReason::ServerClosed);
        assert_eq!(close_reason(&app_close(1)), CloseReason::UnexpectedPeer);
        assert_eq!(close_reason(&app_close(2)), CloseReason::NotPaired);
        assert_eq!(close_reason(&app_close(3)), CloseReason::DeviceRemoved);
        assert_eq!(close_reason(&app_close(4)), CloseReason::Unpaired);
        assert_eq!(close_reason(&app_close(99)), CloseReason::ServerClosed);
        assert_eq!(
            close_reason(&ConnectionError::TimedOut),
            CloseReason::TimedOut
        );
        assert_eq!(
            close_reason(&ConnectionError::LocallyClosed),
            CloseReason::Local
        );
        assert_eq!(close_reason(&ConnectionError::Reset), CloseReason::Lost);
    }

    #[test]
    fn version_mismatch_names_the_side_to_update() {
        let older: BridgeError = SessionError::Version(VersionMismatch::PeerTooOld).into();
        let newer: BridgeError = SessionError::Version(VersionMismatch::PeerTooNew).into();
        assert_eq!(older.kind, ErrorKind::ServerTooOld);
        assert_eq!(newer.kind, ErrorKind::AppTooOld);
    }

    #[test]
    fn rejections_keep_the_retry_delay() {
        let e: BridgeError = PairingFlowError::Rejected {
            reason: PairRejectReason::Locked,
            retry_after: Duration::from_secs(120),
        }
        .into();
        assert_eq!(
            (e.kind, e.retry_after_secs),
            (ErrorKind::PasswordLocked, 120)
        );
        let e: BridgeError = PairingFlowError::Rejected {
            reason: PairRejectReason::BadToken,
            retry_after: Duration::ZERO,
        }
        .into();
        assert_eq!(e.kind, ErrorKind::BadToken);
    }

    #[test]
    fn definitive_errors_outrank_timeouts() {
        assert!(rank(ErrorKind::DeviceRemoved) > rank(ErrorKind::Timeout));
        assert!(rank(ErrorKind::Timeout) > rank(ErrorKind::Unreachable));
        assert!(rank(ErrorKind::Unreachable) > rank(ErrorKind::ServerKeyMismatch));
    }
}
