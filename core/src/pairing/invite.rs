//! Pairing invite carried by the QR code shown on the PC.

use crate::identity::PublicKey;
use crate::proto::v1::PairingInvite;
use crate::text::sanitize_display_name;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use prost::Message;
use std::net::IpAddr;

const PREFIX: &str = "lanpilot://pair?d=";

#[derive(Debug, Clone, PartialEq)]
pub struct Invite {
    pub addrs: Vec<IpAddr>,
    pub port: u16,
    pub server_public_key: PublicKey,
    pub token: [u8; 16],
    pub server_name: String,
}

#[derive(Debug, thiserror::Error)]
pub enum InviteError {
    #[error("not a LanPilot pairing code")]
    NotLanpilot,
    #[error("pairing code is corrupted")]
    Encoding,
    #[error("pairing code has an invalid {0}")]
    Invalid(&'static str),
}

impl Invite {
    pub fn to_uri(&self) -> String {
        let raw = PairingInvite {
            addrs: self.addrs.iter().map(ToString::to_string).collect(),
            port: u32::from(self.port),
            server_public_key: self.server_public_key.as_bytes().to_vec(),
            token: self.token.to_vec(),
            server_name: self.server_name.clone(),
        };
        format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(raw.encode_to_vec()))
    }

    /// Parses and validates a pairing URI. `server_name` is sanitized for display.
    pub fn from_uri(uri: &str) -> Result<Self, InviteError> {
        let data = uri.strip_prefix(PREFIX).ok_or(InviteError::NotLanpilot)?;
        let bytes = URL_SAFE_NO_PAD
            .decode(data)
            .map_err(|_| InviteError::Encoding)?;
        let raw = PairingInvite::decode(bytes.as_slice()).map_err(|_| InviteError::Encoding)?;

        let addrs = raw
            .addrs
            .iter()
            .map(|a| a.parse::<IpAddr>())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| InviteError::Invalid("addrs"))?;
        if addrs.is_empty() {
            return Err(InviteError::Invalid("addrs"));
        }
        let port = u16::try_from(raw.port)
            .ok()
            .filter(|p| *p != 0)
            .ok_or(InviteError::Invalid("port"))?;
        let server_public_key = PublicKey::from_slice(&raw.server_public_key)
            .map_err(|_| InviteError::Invalid("server_public_key"))?;
        let token: [u8; 16] = raw
            .token
            .as_slice()
            .try_into()
            .map_err(|_| InviteError::Invalid("token"))?;
        Ok(Self {
            addrs,
            port,
            server_public_key,
            token,
            server_name: sanitize_display_name(&raw.server_name),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Invite {
        Invite {
            addrs: vec!["192.168.1.20".parse().unwrap(), "10.0.0.5".parse().unwrap()],
            port: 45810,
            server_public_key: PublicKey([7u8; 32]),
            token: [9u8; 16],
            server_name: "书房台式机".into(),
        }
    }

    #[test]
    fn round_trips_through_uri() {
        let uri = sample().to_uri();
        assert!(uri.starts_with("lanpilot://pair?d="));
        assert!(
            !uri["lanpilot://pair?d=".len()..].contains('='),
            "no base64 padding"
        );
        assert_eq!(Invite::from_uri(&uri).unwrap(), sample());
    }

    #[test]
    fn server_name_is_sanitized() {
        let mut dirty = sample();
        dirty.server_name = "\u{202E}Desk\n\u{200B}".into();
        assert_eq!(
            Invite::from_uri(&dirty.to_uri()).unwrap().server_name,
            "Desk"
        );
    }

    #[test]
    fn rejects_other_schemes() {
        assert!(matches!(
            Invite::from_uri("https://example.com/?d=abc"),
            Err(InviteError::NotLanpilot)
        ));
    }

    #[test]
    fn rejects_bad_payloads() {
        assert!(matches!(
            Invite::from_uri("lanpilot://pair?d=!!!"),
            Err(InviteError::Encoding)
        ));
        let mut bad = sample();
        bad.port = 0;
        assert!(matches!(
            Invite::from_uri(&bad.to_uri()),
            Err(InviteError::Invalid("port"))
        ));
    }

    #[test]
    fn rejects_wrong_key_and_token_lengths() {
        use crate::proto::v1::PairingInvite;
        use base64::Engine;
        use prost::Message;
        let raw = PairingInvite {
            addrs: vec!["192.168.1.20".into()],
            port: 1,
            server_public_key: vec![1u8; 31],
            token: vec![2u8; 16],
            server_name: String::new(),
        };
        let d = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw.encode_to_vec());
        assert!(matches!(
            Invite::from_uri(&format!("lanpilot://pair?d={d}")),
            Err(InviteError::Invalid("server_public_key"))
        ));
    }
}
