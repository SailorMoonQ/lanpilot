//! One-time pairing tokens embedded in QR invites.

use rand_core::{OsRng, RngCore};
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub const TOKEN_TTL: Duration = Duration::from_secs(120);

#[derive(Default)]
pub struct TokenStore {
    expiry_by_token: HashMap<[u8; 16], Instant>,
}

/// Tokens are secrets, so `Debug` shows only how many are outstanding.
impl std::fmt::Debug for TokenStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenStore")
            .field("count", &self.expiry_by_token.len())
            .finish()
    }
}

impl TokenStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn issue(&mut self, now: Instant) -> [u8; 16] {
        self.prune(now);
        let mut token = [0u8; 16];
        OsRng.fill_bytes(&mut token);
        self.expiry_by_token.insert(token, now + TOKEN_TTL);
        token
    }

    /// Returns true exactly once for a token issued within the last `TOKEN_TTL`.
    pub fn consume(&mut self, token: &[u8], now: Instant) -> bool {
        self.prune(now);
        let Ok(key) = <[u8; 16]>::try_from(token) else {
            return false;
        };
        self.expiry_by_token.remove(&key).is_some()
    }

    pub fn len(&self) -> usize {
        self.expiry_by_token.len()
    }

    pub fn is_empty(&self) -> bool {
        self.expiry_by_token.is_empty()
    }

    fn prune(&mut self, now: Instant) {
        self.expiry_by_token.retain(|_, expiry| *expiry >= now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_single_use() {
        let now = Instant::now();
        let mut store = TokenStore::new();
        let t = store.issue(now);
        assert!(store.consume(&t, now));
        assert!(!store.consume(&t, now));
    }

    #[test]
    fn token_expires() {
        let now = Instant::now();
        let mut store = TokenStore::new();
        let t = store.issue(now);
        assert!(!store.consume(&t, now + TOKEN_TTL + Duration::from_millis(1)));
    }

    #[test]
    fn unknown_token_is_rejected() {
        let mut store = TokenStore::new();
        assert!(!store.consume(&[0u8; 16], Instant::now()));
        assert!(!store.consume(&[1, 2, 3], Instant::now()));
    }

    #[test]
    fn tokens_are_random() {
        let now = Instant::now();
        let mut store = TokenStore::new();
        assert_ne!(store.issue(now), store.issue(now));
    }

    #[test]
    fn debug_shows_only_count() {
        let mut store = TokenStore::new();
        let t = store.issue(Instant::now());
        let dbg = format!("{store:?}");
        assert_eq!(dbg, "TokenStore { count: 1 }");
        assert!(!dbg.contains(&hex::encode(t)));
    }

    #[test]
    fn expired_tokens_are_pruned() {
        let now = Instant::now();
        let mut store = TokenStore::new();
        store.issue(now);
        store.issue(now + TOKEN_TTL * 2);
        assert_eq!(store.len(), 1);
    }
}
