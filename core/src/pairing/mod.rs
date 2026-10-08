//! Device pairing: QR invites, one-time tokens, password (SPAKE2) and the
//! wire flows that use them. See spec section 4.

pub mod flow;
pub mod invite;
pub mod password;
pub mod tokens;
