//! Password pairing: SPAKE2 plus key confirmation bound to both TLS keys.

use crate::identity::PublicKey;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use spake2::{Ed25519Group, Identity as SpakeIdentity, Password, Spake2};
use std::time::{Duration, Instant};

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
    if password.chars().count() < MIN_PASSWORD_CHARS {
        Err(PasswordError::TooShort)
    } else {
        Ok(())
    }
}

pub struct SharedKey(Vec<u8>);

impl std::fmt::Debug for SharedKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SharedKey(..)")
    }
}

fn start(password: &str, a: bool) -> (Spake2<Ed25519Group>, Vec<u8>) {
    let pw = Password::new(password.as_bytes());
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
            .map(SharedKey)
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
    Ok((msg, SharedKey(key)))
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

#[derive(Debug, Default)]
pub struct PasswordAttempts {
    failures: u32,
    locked_until: Option<Instant>,
}

impl PasswordAttempts {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn check(&self, now: Instant) -> Result<(), Duration> {
        match self.locked_until {
            Some(until) if now < until => Err(until - now),
            _ => Ok(()),
        }
    }

    pub fn record_failure(&mut self, now: Instant) {
        if self.locked_until.is_some_and(|until| now >= until) {
            self.locked_until = None;
            self.failures = 0;
        }
        self.failures += 1;
        if self.failures >= MAX_FAILURES {
            self.locked_until = Some(now + LOCKOUT);
        }
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
    fn locks_after_max_failures_and_unlocks_later() {
        let t0 = Instant::now();
        let mut a = PasswordAttempts::new();
        for _ in 0..MAX_FAILURES - 1 {
            a.record_failure(t0);
            assert!(a.check(t0).is_ok());
        }
        a.record_failure(t0);
        let left = a.check(t0).unwrap_err();
        assert_eq!(left, LOCKOUT);
        assert!(a.check(t0 + LOCKOUT).is_ok());
    }

    #[test]
    fn success_resets_failures() {
        let t0 = Instant::now();
        let mut a = PasswordAttempts::new();
        for _ in 0..MAX_FAILURES - 1 {
            a.record_failure(t0);
        }
        a.record_success();
        a.record_failure(t0);
        assert!(a.check(t0).is_ok());
    }
}
