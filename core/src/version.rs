//! Protocol version negotiation. Both peers send (min, max) in `Hello` and
//! independently pick the highest common version.

pub const PROTO_MIN: u32 = 1;
pub const PROTO_MAX: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum VersionMismatch {
    #[error("peer is too old")]
    PeerTooOld,
    #[error("peer is too new")]
    PeerTooNew,
}

pub fn negotiate(local: (u32, u32), remote: (u32, u32)) -> Result<u32, VersionMismatch> {
    let low = local.0.max(remote.0);
    let high = local.1.min(remote.1);
    if low <= high {
        Ok(high)
    } else if remote.1 < local.0 {
        Err(VersionMismatch::PeerTooOld)
    } else {
        Err(VersionMismatch::PeerTooNew)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_highest_common() {
        assert_eq!(negotiate((1, 3), (2, 5)), Ok(3));
        assert_eq!(negotiate((1, 1), (1, 1)), Ok(1));
    }

    #[test]
    fn reports_which_side_is_outdated() {
        assert_eq!(negotiate((3, 4), (1, 2)), Err(VersionMismatch::PeerTooOld));
        assert_eq!(negotiate((1, 2), (3, 4)), Err(VersionMismatch::PeerTooNew));
    }
}
