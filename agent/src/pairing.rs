//! The agent's side of pairing (spec 4.2 to 4.4).

use crate::devices::DeviceStore;
use crate::secret::load_password;
use lanpilot_core::identity::PublicKey;
use lanpilot_core::pairing::flow::{NewDevice, PairingAuthority};
use lanpilot_core::pairing::invite::Invite;
use lanpilot_core::pairing::password::{LOCKOUT, PasswordAttempts, Reserved};
use lanpilot_core::pairing::tokens::TokenStore;
use lanpilot_core::proto::v1::Os;
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub fn current_os() -> Os {
    if cfg!(windows) {
        Os::Windows
    } else if cfg!(target_os = "linux") {
        Os::Linux
    } else if cfg!(target_os = "macos") {
        Os::Macos
    } else {
        Os::Unspecified
    }
}

const VIRTUAL_NAME_WORDS: &[&str] = &[
    "mihomo",
    "clash",
    "wintun",
    "tun",
    "tap",
    "wireguard",
    "tailscale",
    "zerotier",
    "vpn",
    "vethernet",
    "hyper-v",
    "docker",
    "vmware",
    "virtualbox",
    "vbox",
    "utun",
    "wsl",
];

/// VPN, proxy and virtual adapters, recognized by address range or adapter name.
/// Their addresses are not reachable from the phone's LAN.
pub fn is_virtual_adapter(name: &str, ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    if a == 198 && (b == 18 || b == 19) {
        return true;
    }
    let name = name.to_lowercase();
    let wireguard = name == "wg"
        || name
            .strip_prefix("wg")
            .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()));
    wireguard || VIRTUAL_NAME_WORDS.iter().any(|w| name.contains(w))
}

/// Picks the IPv4 addresses to advertise in a pairing invite. Falls back to every
/// non-loopback, non-link-local address when filtering would leave nothing.
pub fn select_invite_addrs(ifaces: &[(String, Ipv4Addr)]) -> Vec<IpAddr> {
    let usable: Vec<&(String, Ipv4Addr)> = ifaces
        .iter()
        .filter(|(_, ip)| !ip.is_loopback() && !ip.is_link_local())
        .collect();
    let preferred: Vec<IpAddr> = usable
        .iter()
        .filter(|(name, ip)| !is_virtual_adapter(name, *ip))
        .map(|(_, ip)| IpAddr::V4(*ip))
        .collect();
    let mut addrs = if preferred.is_empty() {
        usable.iter().map(|(_, ip)| IpAddr::V4(*ip)).collect()
    } else {
        preferred
    };
    addrs.sort();
    addrs.dedup();
    addrs
}

/// IPv4 only: the core client binds IPv4 (see `transport::client_endpoint`).
pub fn lan_ipv4_addrs() -> Vec<IpAddr> {
    let ifaces: Vec<(String, Ipv4Addr)> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|iface| match iface.ip() {
            IpAddr::V4(v4) => Some((iface.name, v4)),
            IpAddr::V6(_) => None,
        })
        .collect();
    select_invite_addrs(&ifaces)
}

pub fn render_qr(text: &str) -> String {
    match qrcode::QrCode::new(text.as_bytes()) {
        Ok(code) => code
            .render::<qrcode::render::unicode::Dense1x2>()
            .quiet_zone(true)
            .build(),
        Err(e) => format!("(cannot render QR code: {e})"),
    }
}

pub struct AgentPairing {
    server_name: String,
    password_enabled: bool,
    password_file: PathBuf,
    store: Arc<DeviceStore>,
    tokens: Mutex<TokenStore>,
    attempts: Mutex<PasswordAttempts>,
}

impl AgentPairing {
    pub fn new(
        server_name: String,
        password_enabled: bool,
        password_file: PathBuf,
        store: Arc<DeviceStore>,
    ) -> Self {
        Self {
            server_name,
            password_enabled,
            password_file,
            store,
            tokens: Mutex::new(TokenStore::new()),
            attempts: Mutex::new(PasswordAttempts::new()),
        }
    }

    pub fn issue_invite(&self, server_key: PublicKey, port: u16, addrs: Vec<IpAddr>) -> Invite {
        let token = self
            .tokens
            .lock()
            .expect("token lock poisoned")
            .issue(Instant::now());
        Invite {
            addrs,
            port,
            server_public_key: server_key,
            token,
            server_name: self.server_name.clone(),
        }
    }
}

impl PairingAuthority for AgentPairing {
    fn server_name(&self) -> String {
        self.server_name.clone()
    }

    fn server_os(&self) -> Os {
        current_os()
    }

    fn consume_token(&self, token: &[u8]) -> bool {
        self.tokens
            .lock()
            .expect("token lock poisoned")
            .consume(token, Instant::now())
    }

    fn password(&self) -> Option<String> {
        if !self.password_enabled {
            return None;
        }
        match load_password(&self.password_file) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("cannot read the pairing password: {e}");
                None
            }
        }
    }

    fn password_begin(&self) -> Result<Reserved, Duration> {
        let result = self
            .attempts
            .lock()
            .expect("attempts lock poisoned")
            .begin(Instant::now());
        if let Ok(Reserved { now_locked: true }) = result {
            tracing::warn!(
                "password pairing locked for {} s after repeated failures",
                LOCKOUT.as_secs()
            );
        }
        result
    }

    fn password_succeeded(&self) {
        self.attempts
            .lock()
            .expect("attempts lock poisoned")
            .record_success();
    }

    fn approve(&self, device: &NewDevice) -> impl Future<Output = bool> + Send {
        let ok = match self.store.add(device.public_key, &device.name, device.os) {
            Ok(()) => {
                tracing::info!(
                    "new device paired: {} ({})",
                    device.name,
                    device.public_key.short_id()
                );
                true
            }
            Err(e) => {
                tracing::error!("cannot persist paired device: {e}");
                false
            }
        };
        async move { ok }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lanpilot_core::pairing::password::MAX_FAILURES;

    fn setup(enabled: bool) -> (tempfile::TempDir, AgentPairing, Arc<DeviceStore>) {
        let dir = tempfile::tempdir().unwrap();
        let store = DeviceStore::open(dir.path().join("devices.json")).unwrap();
        let p = AgentPairing::new(
            "Desk".into(),
            enabled,
            dir.path().join("pairing-password"),
            store.clone(),
        );
        (dir, p, store)
    }

    #[test]
    fn invite_token_is_consumable_once() {
        let (_d, p, _s) = setup(false);
        let invite = p.issue_invite(
            PublicKey([7; 32]),
            45810,
            vec!["192.168.1.20".parse().unwrap()],
        );
        assert_eq!(invite.server_name, "Desk");
        assert_eq!(invite.port, 45810);
        assert!(p.consume_token(&invite.token));
        assert!(!p.consume_token(&invite.token));
    }

    #[test]
    fn password_requires_enabled_flag() {
        let (d, p, _s) = setup(false);
        crate::secret::store_password(&d.path().join("pairing-password"), "hunter22").unwrap();
        assert_eq!(p.password(), None);

        let (d, p, _s) = setup(true);
        assert_eq!(p.password(), None, "enabled but no password set");
        crate::secret::store_password(&d.path().join("pairing-password"), "hunter22").unwrap();
        assert_eq!(
            p.password().as_deref(),
            Some("hunter22"),
            "picked up without restart"
        );
    }

    #[test]
    fn password_begin_reports_lock_on_last_attempt() {
        let (_d, p, _s) = setup(true);
        for i in 1..=MAX_FAILURES {
            let r = p.password_begin().unwrap();
            assert_eq!(r.now_locked, i == MAX_FAILURES);
        }
        assert!(p.password_begin().is_err());
        p.password_succeeded();
    }

    #[tokio::test]
    async fn approve_persists_device() {
        let (_d, p, store) = setup(false);
        let device = NewDevice {
            public_key: PublicKey([9; 32]),
            name: "iPhone".into(),
            os: Os::Ios,
        };
        assert!(p.approve(&device).await);
        assert!(store.contains(&PublicKey([9; 32])));
    }

    #[test]
    fn qr_renders_as_multiline_blocks() {
        let qr = render_qr("lanpilot://pair?d=abc");
        assert!(qr.lines().count() > 10);
        assert!(qr.contains('█') || qr.contains('▀') || qr.contains('▄'));
    }

    #[test]
    fn lan_addrs_exclude_loopback_and_link_local() {
        for ip in lan_ipv4_addrs() {
            let IpAddr::V4(v4) = ip else {
                panic!("IPv6 returned: {ip}")
            };
            assert!(!v4.is_loopback() && !v4.is_link_local(), "{v4}");
        }
    }

    #[test]
    fn virtual_adapters_are_recognized() {
        let ip = |s: &str| s.parse::<Ipv4Addr>().unwrap();
        assert!(is_virtual_adapter("Mihomo", ip("198.18.0.1")));
        assert!(
            is_virtual_adapter("Ethernet", ip("198.19.255.1")),
            "198.18.0.0/15 by address"
        );
        assert!(is_virtual_adapter("vEthernet (WSL)", ip("172.20.0.1")));
        assert!(is_virtual_adapter("wg0", ip("10.8.0.2")));
        assert!(is_virtual_adapter("Tailscale", ip("100.64.0.3")));
        assert!(!is_virtual_adapter("Ethernet", ip("192.168.50.203")));
        assert!(!is_virtual_adapter("Wi-Fi 6", ip("192.168.1.10")));
        assert!(!is_virtual_adapter("wlan0", ip("192.168.1.11")));
        assert!(!is_virtual_adapter("eno1", ip("10.0.0.5")));
    }

    #[test]
    fn invite_addrs_drop_virtual_adapters_but_never_end_empty() {
        let ip = |s: &str| s.parse::<Ipv4Addr>().unwrap();
        let ifaces = vec![
            ("Ethernet".to_owned(), ip("192.168.50.203")),
            ("Mihomo".to_owned(), ip("198.18.0.1")),
            ("Loopback".to_owned(), ip("127.0.0.1")),
            ("Ethernet 2".to_owned(), ip("169.254.10.1")),
        ];
        assert_eq!(
            select_invite_addrs(&ifaces),
            vec![IpAddr::V4(ip("192.168.50.203"))]
        );
        let only_virtual = vec![("Mihomo".to_owned(), ip("198.18.0.1"))];
        assert_eq!(
            select_invite_addrs(&only_virtual),
            vec![IpAddr::V4(ip("198.18.0.1"))]
        );
    }
}
