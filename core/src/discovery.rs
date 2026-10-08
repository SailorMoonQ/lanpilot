//! mDNS advertisement (PC) and browsing. See spec section 3.1.
//!
//! [`Browser`] does raw multicast and is meant for desktop and tests. iOS
//! needs a restricted entitlement for raw multicast and Android needs a
//! MulticastLock, so phones should browse via NWBrowser / NsdManager and call
//! [`DiscoveredDevice::from_resolved`]; Found/Lost are hints, the only
//! identity check is the TLS key.

use crate::proto::v1::Os;
use crate::text::sanitize_display_name;
use mdns_sd::{IfKind, ServiceDaemon, ServiceEvent, ServiceInfo};
use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Duration;

pub const SERVICE_TYPE: &str = "_lanpilot._udp.local.";

#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error(transparent)]
    Mdns(#[from] mdns_sd::Error),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Advertisement {
    pub short_id: String,
    pub name: String,
    pub os: Os,
    pub proto_min: u32,
    pub proto_max: u32,
    pub port: u16,
    /// Addresses to publish. Empty lets mdns-sd choose (every non-loopback
    /// interface, addresses tracked automatically). Non-empty publishes exactly
    /// these addresses, and only on the interfaces that own them, so virtual
    /// adapters (TUN, VPN) never carry the record.
    pub addrs: Vec<IpAddr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredDevice {
    pub short_id: String,
    pub name: String,
    pub os: Os,
    pub proto_min: u32,
    pub proto_max: u32,
    pub addrs: Vec<IpAddr>,
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DiscoveryEvent {
    /// Consumers must treat this as an upsert keyed by `short_id`: mDNS
    /// re-announcements produce repeated `Found` events for the same device.
    Found(DiscoveredDevice),
    Lost {
        short_id: String,
    },
}

fn os_to_str(os: Os) -> &'static str {
    match os {
        Os::Windows => "windows",
        Os::Linux => "linux",
        Os::Macos => "macos",
        Os::Ios => "ios",
        Os::Android => "android",
        Os::Unspecified => "unknown",
    }
}

fn os_from_str(s: &str) -> Os {
    match s {
        "windows" => Os::Windows,
        "linux" => Os::Linux,
        "macos" => Os::Macos,
        "ios" => Os::Ios,
        "android" => Os::Android,
        _ => Os::Unspecified,
    }
}

impl Advertisement {
    /// TXT properties. The name is sanitized with [`sanitize_display_name`]
    /// and clipped (on a char boundary) so `name=<value>` fits one TXT entry.
    pub fn txt(&self) -> Vec<(String, String)> {
        let name = sanitize_display_name(&self.name);
        let name = clip_utf8(&name, MAX_TXT_ENTRY_BYTES - "name=".len());
        vec![
            ("id".into(), self.short_id.clone()),
            ("name".into(), name.to_owned()),
            ("os".into(), os_to_str(self.os).into()),
            (
                "proto".into(),
                format!("{}-{}", self.proto_min, self.proto_max),
            ),
        ]
    }
}

/// A TXT record entry (`key=value`) is at most 255 bytes.
const MAX_TXT_ENTRY_BYTES: usize = 255;

/// Longest prefix of `s` that fits in `max` bytes without splitting a char.
fn clip_utf8(s: &str, max: usize) -> &str {
    let mut end = s.len().min(max);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// A short ID is exactly 16 lowercase hex characters.
fn is_valid_short_id(s: &str) -> bool {
    s.len() == 16 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

pub fn parse_txt(props: &HashMap<String, String>) -> Option<(String, String, Os, u32, u32)> {
    let id = props.get("id").filter(|s| is_valid_short_id(s))?.clone();
    let name = props
        .get("name")
        .map(|n| sanitize_display_name(n))
        .unwrap_or_default();
    let os = os_from_str(props.get("os").map(String::as_str).unwrap_or_default());
    let (min, max) = props.get("proto")?.split_once('-')?;
    let (min, max): (u32, u32) = (min.parse().ok()?, max.parse().ok()?);
    if min > max {
        return None;
    }
    Some((id, name, os, min, max))
}

/// Instance name of a LanPilot service fullname, only if it is a valid short ID.
fn short_id_from_fullname(fullname: &str) -> Option<String> {
    fullname
        .strip_suffix(SERVICE_TYPE)
        .and_then(|s| s.strip_suffix('.'))
        .filter(|s| is_valid_short_id(s))
        .map(str::to_owned)
}

impl DiscoveredDevice {
    /// Builds a device from a resolved service: `fullname` is the full service
    /// name (`<short_id>._lanpilot._udp.local.`), `txt` its TXT properties.
    /// Returns `None` unless the TXT is valid and its `id` equals the mDNS
    /// instance name (otherwise a LAN peer could claim any identity). The name
    /// is sanitized and `addrs` are sorted.
    ///
    /// Phones resolve with platform APIs and call this; [`Browser`] uses it too.
    pub fn from_resolved(
        fullname: &str,
        txt: &HashMap<String, String>,
        mut addrs: Vec<IpAddr>,
        port: u16,
    ) -> Option<DiscoveredDevice> {
        let instance = short_id_from_fullname(fullname)?;
        let (short_id, name, os, proto_min, proto_max) = parse_txt(txt)?;
        if short_id != instance {
            return None;
        }
        addrs.sort();
        Some(DiscoveredDevice {
            short_id,
            name,
            os,
            proto_min,
            proto_max,
            addrs,
            port,
        })
    }
}

pub struct Advertiser {
    daemon: ServiceDaemon,
    /// `None` once withdrawn, so `Drop` never unregisters twice.
    fullname: Option<String>,
}

impl Advertiser {
    /// Starts advertising `ad`. The advertised name is sanitized (see
    /// [`Advertisement::txt`]); see [`Advertisement::addrs`] for interface
    /// selection. Loopback stays disabled (the mdns-sd default).
    pub fn start(ad: &Advertisement) -> Result<Self, DiscoveryError> {
        let info = service_info(ad)?;
        let daemon = ServiceDaemon::new()?;
        if !ad.addrs.is_empty() {
            daemon.disable_interface(IfKind::All)?;
            for ip in &ad.addrs {
                daemon.enable_interface(IfKind::Addr(*ip))?;
            }
        }
        let fullname = info.get_fullname().to_owned();
        daemon.register(info)?;
        Ok(Self {
            daemon,
            fullname: Some(fullname),
        })
    }

    /// Withdraws the record and stops the daemon, waiting (up to
    /// [`GOODBYE_TIMEOUT`]) until the goodbye has been sent so browsers see
    /// the device go away promptly. Prefer this over dropping, which cannot
    /// wait.
    pub async fn stop(mut self) {
        let Some(fullname) = self.fullname.take() else {
            return;
        };
        if let Ok(status) = self.daemon.unregister(&fullname) {
            let _ = tokio::time::timeout(GOODBYE_TIMEOUT, status.recv_async()).await;
        }
        let _ = self.daemon.shutdown();
    }
}

/// How long [`Advertiser::stop`] waits for the daemon to send the goodbye.
pub const GOODBYE_TIMEOUT: Duration = Duration::from_secs(1);

/// The service record for `ad`: exactly `ad.addrs` when non-empty, otherwise
/// no addresses and mdns-sd's automatic address tracking.
fn service_info(ad: &Advertisement) -> Result<ServiceInfo, DiscoveryError> {
    let props: HashMap<String, String> = ad.txt().into_iter().collect();
    let info = ServiceInfo::new(
        SERVICE_TYPE,
        &ad.short_id,
        &format!("{}.local.", ad.short_id),
        ad.addrs.as_slice(),
        ad.port,
        props,
    )?;
    Ok(if ad.addrs.is_empty() {
        info.enable_addr_auto()
    } else {
        info
    })
}

/// Best effort for panics and early returns: the goodbye is queued but not
/// awaited. Does nothing after [`Advertiser::stop`].
impl Drop for Advertiser {
    fn drop(&mut self) {
        if let Some(fullname) = self.fullname.take() {
            let _ = self.daemon.unregister(&fullname);
            let _ = self.daemon.shutdown();
        }
    }
}

pub struct Browser {
    daemon: ServiceDaemon,
    events: mdns_sd::Receiver<ServiceEvent>,
}

impl Browser {
    pub fn start() -> Result<Self, DiscoveryError> {
        let daemon = ServiceDaemon::new()?;
        let events = daemon.browse(SERVICE_TYPE)?;
        Ok(Self { daemon, events })
    }

    /// Next relevant event; `None` once the daemon has stopped.
    pub async fn next(&mut self) -> Option<DiscoveryEvent> {
        loop {
            match self.events.recv_async().await.ok()? {
                ServiceEvent::ServiceResolved(info) => {
                    let props: HashMap<String, String> = info
                        .get_properties()
                        .iter()
                        .map(|p| (p.key().to_owned(), p.val_str().to_owned()))
                        .collect();
                    let addrs: Vec<IpAddr> = info.get_addresses().iter().copied().collect();
                    if let Some(device) = DiscoveredDevice::from_resolved(
                        info.get_fullname(),
                        &props,
                        addrs,
                        info.get_port(),
                    ) {
                        return Some(DiscoveryEvent::Found(device));
                    }
                }
                ServiceEvent::ServiceRemoved(_, fullname) => {
                    if let Some(short_id) = short_id_from_fullname(&fullname) {
                        return Some(DiscoveryEvent::Lost { short_id });
                    }
                }
                _ => {}
            }
        }
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        let _ = self.daemon.stop_browse(SERVICE_TYPE);
        let _ = self.daemon.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ad() -> Advertisement {
        Advertisement {
            short_id: "0123456789abcdef".into(),
            name: "书房台式机".into(),
            os: Os::Windows,
            proto_min: 1,
            proto_max: 2,
            port: 45810,
            addrs: vec![],
        }
    }

    #[test]
    fn explicit_addrs_are_published_exactly() {
        let a: IpAddr = "192.168.50.203".parse().unwrap();
        let b: IpAddr = "10.0.0.7".parse().unwrap();
        let mut ad = ad();
        ad.addrs = vec![a, b];
        let info = service_info(&ad).unwrap();
        assert!(!info.is_addr_auto());
        let got: std::collections::HashSet<IpAddr> = info.get_addresses().clone();
        assert_eq!(got, [a, b].into_iter().collect());
        assert_eq!(
            info.get_fullname(),
            "0123456789abcdef._lanpilot._udp.local."
        );
        assert_eq!(info.get_port(), 45810);
    }

    #[test]
    fn empty_addrs_keep_auto_mode() {
        let info = service_info(&ad()).unwrap();
        assert!(info.is_addr_auto());
        assert!(info.get_addresses().is_empty());
    }

    #[test]
    fn txt_round_trips() {
        let map: HashMap<String, String> = ad().txt().into_iter().collect();
        let (id, name, os, min, max) = parse_txt(&map).unwrap();
        assert_eq!(id, "0123456789abcdef");
        assert_eq!(name, "书房台式机");
        assert_eq!(os, Os::Windows);
        assert_eq!((min, max), (1, 2));
    }

    #[test]
    fn txt_requires_id_and_valid_proto() {
        let mut map: HashMap<String, String> = ad().txt().into_iter().collect();
        map.insert("proto".into(), "x".into());
        assert!(parse_txt(&map).is_none());
        map.insert("proto".into(), "1-1".into());
        map.remove("id");
        assert!(parse_txt(&map).is_none());
    }

    #[test]
    fn unknown_os_is_unspecified() {
        let mut map: HashMap<String, String> = ad().txt().into_iter().collect();
        map.insert("os".into(), "plan9".into());
        assert_eq!(parse_txt(&map).unwrap().2, Os::Unspecified);
    }

    #[test]
    fn mismatched_instance_and_txt_id_is_dropped() {
        let map: HashMap<String, String> = ad().txt().into_iter().collect();
        assert!(
            DiscoveredDevice::from_resolved(
                "fedcba9876543210._lanpilot._udp.local.",
                &map,
                vec![],
                1
            )
            .is_none()
        );
        assert!(
            DiscoveredDevice::from_resolved("evil._lanpilot._udp.local.", &map, vec![], 1)
                .is_none()
        );
    }

    #[test]
    fn non_hex_or_wrong_length_id_is_rejected() {
        for bad in [
            "0123456789ABCDEF",
            "0123456789abcde",
            "0123456789abcdeg",
            "",
        ] {
            let mut map: HashMap<String, String> = ad().txt().into_iter().collect();
            map.insert("id".into(), bad.into());
            assert!(parse_txt(&map).is_none(), "{bad:?}");
            assert_eq!(
                short_id_from_fullname(&format!("{bad}._lanpilot._udp.local.")),
                None
            );
        }
    }

    #[test]
    fn matching_instance_and_id_yields_device() {
        let map: HashMap<String, String> = ad().txt().into_iter().collect();
        let a: IpAddr = "192.168.1.9".parse().unwrap();
        let b: IpAddr = "10.0.0.2".parse().unwrap();
        let d = DiscoveredDevice::from_resolved(
            "0123456789abcdef._lanpilot._udp.local.",
            &map,
            vec![a, b],
            45810,
        )
        .unwrap();
        assert_eq!(d.short_id, "0123456789abcdef");
        assert_eq!(d.addrs, vec![b, a]);
        assert_eq!(d.port, 45810);
    }

    #[test]
    fn inverted_proto_range_is_rejected() {
        let mut map: HashMap<String, String> = ad().txt().into_iter().collect();
        map.insert("proto".into(), "3-2".into());
        assert!(parse_txt(&map).is_none());
    }

    #[test]
    fn name_is_sanitized() {
        let mut map: HashMap<String, String> = ad().txt().into_iter().collect();
        map.insert(
            "name".into(),
            format!(" a\u{0}b\u{202E}\nc\u{200B}{}", "x".repeat(100)),
        );
        let name = parse_txt(&map).unwrap().1;
        assert!(name.starts_with("abcx"), "{name:?}");
        assert_eq!(name.chars().count(), 64);
    }

    #[test]
    fn advertised_name_is_sanitized_and_fits_a_txt_entry() {
        let mut a = ad();
        a.name = format!("\u{202E}{}", "\u{1D54F}".repeat(100));
        let txt = a.txt();
        let (key, value) = txt.iter().find(|(k, _)| k == "name").unwrap();
        assert!(key.len() + 1 + value.len() <= MAX_TXT_ENTRY_BYTES);
        assert!(!value.contains('\u{202E}'));
        assert!(value.chars().all(|c| c == '\u{1D54F}'));
        assert!(!value.is_empty());
    }

    #[test]
    fn short_id_parses_fullname() {
        assert_eq!(
            short_id_from_fullname("0123456789abcdef._lanpilot._udp.local."),
            Some("0123456789abcdef".to_owned())
        );
        assert_eq!(short_id_from_fullname("other._http._tcp.local."), None);
    }
}
