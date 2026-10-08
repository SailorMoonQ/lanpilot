//! The long-lived client behind the Dart API. All QUIC work happens here;
//! `api::bridge` only forwards calls to one global instance.

use crate::api::types::{BridgeError, DiscoveredInfo, ErrorKind, OsKind, PairedServerInfo};
use lanpilot_core::discovery::DiscoveredDevice;
use lanpilot_core::identity::{Identity, PublicKey};
use lanpilot_core::pairing::flow::{PairedServer, pair_with_invite, pair_with_password};
use lanpilot_core::pairing::invite::Invite;
use lanpilot_core::proto::v1::Os;
use lanpilot_core::quinn;
use lanpilot_core::transport::client_endpoint;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};

pub struct Client {
    identity: Identity,
    device_name: String,
    #[allow(dead_code)] // read by connect (Task 3)
    app_version: String,
    /// Created at the first network call, never at app start: iOS shows the
    /// Local Network prompt then, and a socket created before access was
    /// granted stays blocked (M0). `reset_endpoint` drops it.
    endpoint: tokio::sync::Mutex<Option<quinn::Endpoint>>,
}

impl Client {
    pub fn new(identity: Identity, device_name: &str, app_version: &str) -> Self {
        Self {
            identity,
            device_name: device_name.to_owned(),
            app_version: app_version.to_owned(),
            endpoint: tokio::sync::Mutex::new(None),
        }
    }

    pub fn short_id(&self) -> String {
        self.identity.public_key().short_id()
    }

    async fn endpoint(&self) -> Result<quinn::Endpoint, BridgeError> {
        let mut slot = self.endpoint.lock().await;
        if let Some(endpoint) = slot.as_ref() {
            return Ok(endpoint.clone());
        }
        let endpoint = client_endpoint(&self.identity)?;
        *slot = Some(endpoint.clone());
        Ok(endpoint)
    }

    /// Pairs with the computer in a `lanpilot://pair?d=...` link. Surrounding
    /// whitespace is ignored.
    pub async fn pair_with_uri(&self, uri: &str) -> Result<PairedServerInfo, BridgeError> {
        let invite = Invite::from_uri(uri.trim())
            .map_err(|e| BridgeError::new(ErrorKind::InvalidInput, e.to_string()))?;
        let endpoint = self.endpoint().await?;
        let paired = pair_with_invite(&endpoint, &invite, &self.device_name, Os::Ios).await?;
        let addrs = invite
            .addrs
            .iter()
            .map(|ip| SocketAddr::new(*ip, invite.port));
        Ok(paired_info(&paired, addrs))
    }

    /// Pairs with the computer at `addr` ("ip:port") using its pairing password.
    pub async fn pair_with_password(
        &self,
        addr: &str,
        password: &str,
    ) -> Result<PairedServerInfo, BridgeError> {
        let addr = parse_addr(addr)?;
        let endpoint = self.endpoint().await?;
        let paired = pair_with_password(
            &endpoint,
            addr,
            &self.identity,
            password,
            &self.device_name,
            Os::Ios,
        )
        .await?;
        Ok(paired_info(&paired, [addr]))
    }
}

pub fn os_kind(raw: i32) -> OsKind {
    match Os::try_from(raw).unwrap_or(Os::Unspecified) {
        Os::Windows => OsKind::Windows,
        Os::Linux => OsKind::Linux,
        Os::Macos => OsKind::Macos,
        Os::Ios => OsKind::Ios,
        Os::Android => OsKind::Android,
        Os::Unspecified => OsKind::Unknown,
    }
}

fn paired_info(
    paired: &PairedServer,
    addrs: impl IntoIterator<Item = SocketAddr>,
) -> PairedServerInfo {
    PairedServerInfo {
        public_key_hex: hex::encode(paired.public_key.as_bytes()),
        short_id: paired.public_key.short_id(),
        name: paired.name.clone(),
        os: os_kind(paired.os as i32),
        addrs: addrs.into_iter().map(|a| a.to_string()).collect(),
    }
}

pub fn parse_addr(addr: &str) -> Result<SocketAddr, BridgeError> {
    let parsed: SocketAddr = addr
        .trim()
        .parse()
        .map_err(|_| BridgeError::new(ErrorKind::InvalidInput, format!("bad address {addr:?}")))?;
    if !parsed.is_ipv4() || parsed.port() == 0 {
        return Err(BridgeError::new(
            ErrorKind::InvalidInput,
            format!("need an IPv4 address with a port, got {addr:?}"),
        ));
    }
    Ok(parsed)
}

pub fn parse_key(hex_key: &str) -> Result<PublicKey, BridgeError> {
    let bytes = hex::decode(hex_key.trim())
        .map_err(|_| BridgeError::new(ErrorKind::InvalidInput, "server key is not hex"))?;
    PublicKey::from_slice(&bytes)
        .map_err(|e| BridgeError::new(ErrorKind::InvalidInput, e.to_string()))
}

/// Validates one Bonjour result with core (spec 3.2). Only IPv4 addresses are
/// kept; a result with none is dropped.
pub fn validate_discovered(
    fullname: &str,
    txt: &HashMap<String, String>,
    addrs: &[String],
    port: u16,
) -> Option<DiscoveredInfo> {
    if port == 0 {
        return None;
    }
    let ips: Vec<IpAddr> = addrs
        .iter()
        .filter_map(|a| a.trim().parse::<IpAddr>().ok())
        .filter(IpAddr::is_ipv4)
        .collect();
    if ips.is_empty() {
        return None;
    }
    let device = DiscoveredDevice::from_resolved(fullname, txt, ips, port)?;
    Some(DiscoveredInfo {
        short_id: device.short_id,
        name: device.name,
        os: os_kind(device.os as i32),
        proto_min: device.proto_min,
        proto_max: device.proto_max,
        addrs: device
            .addrs
            .iter()
            .map(|ip| SocketAddr::new(*ip, device.port).to_string())
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn txt(id: &str) -> HashMap<String, String> {
        HashMap::from([
            ("id".to_owned(), id.to_owned()),
            ("name".to_owned(), "Desk".to_owned()),
            ("os".to_owned(), "windows".to_owned()),
            ("proto".to_owned(), "1-1".to_owned()),
        ])
    }

    const ID: &str = "0123456789abcdef";

    #[test]
    fn validates_a_good_result() {
        let fullname = format!("{ID}._lanpilot._udp.local.");
        let info = validate_discovered(
            &fullname,
            &txt(ID),
            &["192.168.1.9".into(), "fe80::1".into()],
            45810,
        )
        .unwrap();
        assert_eq!(info.short_id, ID);
        assert_eq!(info.os, OsKind::Windows);
        assert_eq!(info.addrs, vec!["192.168.1.9:45810".to_owned()]);
    }

    #[test]
    fn rejects_mismatched_id_bad_port_or_no_ipv4() {
        let fullname = format!("{ID}._lanpilot._udp.local.");
        let addrs = ["192.168.1.9".to_owned()];
        assert!(validate_discovered(&fullname, &txt("ffffffffffffffff"), &addrs, 45810).is_none());
        assert!(validate_discovered(&fullname, &txt(ID), &addrs, 0).is_none());
        assert!(validate_discovered(&fullname, &txt(ID), &["fe80::1".into()], 45810).is_none());
    }

    #[test]
    fn parses_addresses_and_keys() {
        assert_eq!(parse_addr(" 10.0.0.2:45810 ").unwrap().port(), 45810);
        assert_eq!(
            parse_addr("10.0.0.2").unwrap_err().kind,
            ErrorKind::InvalidInput
        );
        assert_eq!(
            parse_addr("[::1]:45810").unwrap_err().kind,
            ErrorKind::InvalidInput
        );
        assert_eq!(parse_key("zz").unwrap_err().kind, ErrorKind::InvalidInput);
        assert!(parse_key(&"ab".repeat(32)).is_ok());
    }
}
