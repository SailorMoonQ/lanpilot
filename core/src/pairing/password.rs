//! Password pairing: SPAKE2 plus key confirmation bound to both TLS keys.

use crate::identity::PublicKey;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use spake2::{Ed25519Group, Identity as SpakeIdentity, Password, Spake2};
use std::time::{Duration, Instant};
use unicode_normalization::UnicodeNormalization;
use zeroize::Zeroizing;

pub const MIN_PASSWORD_CHARS: usize = 6;
pub const MAX_FAILURES: u32 = 5;
pub const LOCKOUT: Duration = Duration::from_secs(300);

const ID_CLIENT: &[u8] = b"lanpilot-pair-client";
const ID_SERVER: &[u8] = b"lanpilot-pair-server";

#[derive(Debug, thiserror::Error)]
pub enum PasswordError {
    #[error("password must have at least {MIN_PASSWORD_CHARS} characters")]
    TooShort,
    #[error("password pairing protocol error")]
    Protocol,
}

pub fn validate_password(password: &str) -> Result<(), PasswordError> {
    if password.nfc().count() < MIN_PASSWORD_CHARS {
        Err(PasswordError::TooShort)
    } else {
        Ok(())
    }
}

pub struct SharedKey(Zeroizing<Vec<u8>>);

impl std::fmt::Debug for SharedKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SharedKey(..)")
    }
}

fn start(password: &str, a: bool) -> (Spake2<Ed25519Group>, Vec<u8>) {
    let normalized = Zeroizing::new(password.nfc().collect::<String>());
    let pw = Password::new(normalized.as_bytes());
    let (ida, idb) = (SpakeIdentity::new(ID_CLIENT), SpakeIdentity::new(ID_SERVER));
    if a {
        Spake2::<Ed25519Group>::start_a(&pw, &ida, &idb)
    } else {
        Spake2::<Ed25519Group>::start_b(&pw, &ida, &idb)
    }
}

pub struct ClientHandshake {
    state: Spake2<Ed25519Group>,
}

impl ClientHandshake {
    pub fn start(password: &str) -> (Self, Vec<u8>) {
        let (state, msg) = start(password, true);
        (Self { state }, msg)
    }

    pub fn finish(self, server_msg: &[u8]) -> Result<SharedKey, PasswordError> {
        self.state
            .finish(server_msg)
            .map(|key| SharedKey(Zeroizing::new(key)))
            .map_err(|_| PasswordError::Protocol)
    }
}

pub fn server_handshake(
    password: &str,
    client_msg: &[u8],
) -> Result<(Vec<u8>, SharedKey), PasswordError> {
    let (state, msg) = start(password, false);
    let key = state
        .finish(client_msg)
        .map_err(|_| PasswordError::Protocol)?;
    Ok((msg, SharedKey(Zeroizing::new(key))))
}

#[derive(Debug, Clone, Copy)]
pub enum Role {
    Client,
    Server,
}

fn mac(key: &SharedKey, role: Role, client: &PublicKey, server: &PublicKey) -> Hmac<Sha256> {
    let mut mac = Hmac::<Sha256>::new_from_slice(&key.0).expect("HMAC accepts any key length");
    mac.update(match role {
        Role::Client => b"lanpilot-confirm-client",
        Role::Server => b"lanpilot-confirm-server",
    });
    mac.update(client.as_bytes());
    mac.update(server.as_bytes());
    mac
}

pub fn confirmation(
    key: &SharedKey,
    role: Role,
    client: &PublicKey,
    server: &PublicKey,
) -> Vec<u8> {
    mac(key, role, client, server)
        .finalize()
        .into_bytes()
        .to_vec()
}

pub fn verify_confirmation(
    key: &SharedKey,
    role: Role,
    client: &PublicKey,
    server: &PublicKey,
    tag: &[u8],
) -> bool {
    mac(key, role, client, server).verify_slice(tag).is_ok()
}

/// Online-guess limiter. Callers hold one shared instance for the whole server
/// under one lock, call `begin` before sending any SPAKE2 message, and call
/// `record_success` only after the client's confirmation verifies.
#[derive(Debug, Default)]
pub struct PasswordAttempts {
    failures: u32,
    locked_until: Option<Instant>,
}

impl PasswordAttempts {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reserves one attempt. If locked, returns the time left without counting.
    /// Otherwise the attempt is counted as a failure immediately and `Ok` is
    /// returned; only `record_success` undoes it.
    pub fn begin(&mut self, now: Instant) -> Result<(), Duration> {
        match self.locked_until {
            Some(until) if now < until => return Err(until - now),
            Some(_) => {
                self.locked_until = None;
                self.failures = 0;
            }
            None => {}
        }
        self.failures += 1;
        if self.failures >= MAX_FAILURES {
            self.locked_until = Some(now + LOCKOUT);
        }
        Ok(())
    }

    pub fn record_success(&mut self) {
        self.failures = 0;
        self.locked_until = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const C: PublicKey = PublicKey([1u8; 32]);
    const S: PublicKey = PublicKey([2u8; 32]);

    fn run(client_pw: &str, server_pw: &str) -> (SharedKey, SharedKey) {
        let (client, client_msg) = ClientHandshake::start(client_pw);
        let (server_msg, server_key) = server_handshake(server_pw, &client_msg).unwrap();
        let client_key = client.finish(&server_msg).unwrap();
        (client_key, server_key)
    }

    #[test]
    fn same_password_confirms_both_ways() {
        let (ck, sk) = run("hunter22", "hunter22");
        let server_tag = confirmation(&sk, Role::Server, &C, &S);
        assert!(verify_confirmation(&ck, Role::Server, &C, &S, &server_tag));
        let client_tag = confirmation(&ck, Role::Client, &C, &S);
        assert!(verify_confirmation(&sk, Role::Client, &C, &S, &client_tag));
    }

    #[test]
    fn wrong_password_fails_confirmation() {
        let (ck, sk) = run("hunter22", "hunter23");
        let server_tag = confirmation(&sk, Role::Server, &C, &S);
        assert!(!verify_confirmation(&ck, Role::Server, &C, &S, &server_tag));
    }

    #[test]
    fn mismatched_tls_keys_fail_confirmation() {
        // Man in the middle: client sees M as the server key, server sees M as the client key.
        let m = PublicKey([3u8; 32]);
        let (ck, sk) = run("hunter22", "hunter22");
        let server_tag = confirmation(&sk, Role::Server, &m, &S);
        assert!(!verify_confirmation(&ck, Role::Server, &C, &m, &server_tag));
    }

    #[test]
    fn roles_are_not_interchangeable() {
        let (ck, sk) = run("hunter22", "hunter22");
        let server_tag = confirmation(&sk, Role::Server, &C, &S);
        assert!(!verify_confirmation(&ck, Role::Client, &C, &S, &server_tag));
    }

    #[test]
    fn garbage_client_message_is_a_protocol_error() {
        assert!(matches!(
            server_handshake("hunter22", &[1, 2, 3]),
            Err(PasswordError::Protocol)
        ));
    }

    #[test]
    fn validates_length_in_characters() {
        assert!(matches!(
            validate_password("12345"),
            Err(PasswordError::TooShort)
        ));
        assert!(validate_password("123456").is_ok());
        assert!(validate_password("密码六个字符").is_ok());
    }

    #[test]
    fn begin_counts_attempts_without_outcome() {
        let t0 = Instant::now();
        let mut a = PasswordAttempts::new();
        for _ in 0..MAX_FAILURES {
            assert!(a.begin(t0).is_ok());
        }
        assert_eq!(a.begin(t0).unwrap_err(), LOCKOUT);
    }

    #[test]
    fn locks_after_max_attempts_and_unlocks_later() {
        let t0 = Instant::now();
        let mut a = PasswordAttempts::new();
        for _ in 0..MAX_FAILURES {
            a.begin(t0).unwrap();
        }
        assert_eq!(a.begin(t0).unwrap_err(), LOCKOUT);
        assert!(a.begin(t0 + LOCKOUT).is_ok());
    }

    #[test]
    fn single_failure_after_expiry_does_not_relock() {
        let t0 = Instant::now();
        let mut a = PasswordAttempts::new();
        for _ in 0..MAX_FAILURES {
            a.begin(t0).unwrap();
        }
        let t1 = t0 + LOCKOUT;
        assert!(a.begin(t1).is_ok());
        assert!(a.begin(t1).is_ok());
    }

    #[test]
    fn success_clears_reserved_attempts() {
        let t0 = Instant::now();
        let mut a = PasswordAttempts::new();
        for _ in 0..MAX_FAILURES - 1 {
            a.begin(t0).unwrap();
        }
        a.record_success();
        for _ in 0..MAX_FAILURES {
            assert!(a.begin(t0).is_ok());
        }
    }

    #[test]
    fn nfc_and_nfd_forms_agree() {
        let (ck, sk) = run("caf\u{e9}12", "cafe\u{301}12");
        let tag = confirmation(&sk, Role::Server, &C, &S);
        assert!(verify_confirmation(&ck, Role::Server, &C, &S, &tag));
        assert!(validate_password("cafe\u{301}12").is_ok());
        assert!(validate_password("cafe\u{301}1").is_err());
    }

    #[test]
    fn truncated_tag_is_rejected() {
        let (ck, sk) = run("hunter22", "hunter22");
        let tag = confirmation(&sk, Role::Server, &C, &S);
        assert!(!verify_confirmation(&ck, Role::Server, &C, &S, &tag[..16]));
        assert!(!verify_confirmation(&ck, Role::Server, &C, &S, &[]));
    }

    #[test]
    fn client_finish_rejects_garbage() {
        let (client, _) = ClientHandshake::start("hunter22");
        assert!(matches!(
            client.finish(&[1, 2, 3]),
            Err(PasswordError::Protocol)
        ));
    }
}
