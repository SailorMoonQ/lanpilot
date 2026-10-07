//! Shared core of LanPilot: protocol, identity, transport, pairing and discovery.

pub use quinn;
pub mod framing;
pub mod identity;
pub mod pairing;
pub mod pointer;
pub mod proto;
pub mod tls;
pub mod transport;
pub mod version;
