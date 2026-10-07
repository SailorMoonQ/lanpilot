//! mDNS advertisement (PC) and browsing (phone). See spec section 3.1.

use crate::proto::v1::Os;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::collections::HashMap;
use std::net::IpAddr;

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
    pub fn txt(&self) -> Vec<(String, String)> {
        vec![
            ("id".into(), self.short_id.clone()),
            ("name".into(), self.name.clone()),
            ("os".into(), os_to_str(self.os).into()),
            (
                "proto".into(),
                format!("{}-{}", self.proto_min, self.proto_max),
            ),
        ]
    }
}

const MAX_NAME_CHARS: usize = 64;

/// A short ID is exactly 16 lowercase hex characters.
fn is_valid_short_id(s: &str) -> bool {
    s.len() == 16 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

pub fn parse_txt(props: &HashMap<String, String>) -> Option<(String, String, Os, u32, u32)> {
    let id = props.get("id").filter(|s| is_valid_short_id(s))?.clone();
    let name: String = props
        .get("name")
        .map(|n| {
            n.chars()
                .filter(|c| !c.is_control())
                .take(MAX_NAME_CHARS)
                .collect()
        })
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

/// Builds a device from a resolved service, dropping it unless the TXT `id`
/// equals the mDNS instance name (otherwise a LAN peer could claim any identity).
fn resolved_to_event(
    fullname: &str,
    props: &HashMap<String, String>,
    mut addrs: Vec<IpAddr>,
    port: u16,
) -> Option<DiscoveredDevice> {
    let instance = short_id_from_fullname(fullname)?;
    let (short_id, name, os, proto_min, proto_max) = parse_txt(props)?;
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

pub struct Advertiser {
    daemon: ServiceDaemon,
    fullname: String,
}

impl Advertiser {
    pub fn start(ad: &Advertisement) -> Result<Self, DiscoveryError> {
        let daemon = ServiceDaemon::new()?;
        let props: HashMap<String, String> = ad.txt().into_iter().collect();
        let info = ServiceInfo::new(
            SERVICE_TYPE,
            &ad.short_id,
            &format!("{}.local.", ad.short_id),
            "",
            ad.port,
            props,
        )?
        .enable_addr_auto();
        let fullname = info.get_fullname().to_owned();
        daemon.register(info)?;
        Ok(Self { daemon, fullname })
    }
}

impl Drop for Advertiser {
    fn drop(&mut self) {
        let _ = self.daemon.unregister(&self.fullname);
        let _ = self.daemon.shutdown();
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
                    if let Some(device) =
                        resolved_to_event(info.get_fullname(), &props, addrs, info.get_port())
                    {
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
        }
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
            resolved_to_event("fedcba9876543210._lanpilot._udp.local.", &map, vec![], 1).is_none()
        );
        assert!(resolved_to_event("evil._lanpilot._udp.local.", &map, vec![], 1).is_none());
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
        let d = resolved_to_event(
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
        map.insert("name".into(), format!("a\u{0}b\nc{}", "x".repeat(100)));
        let name = parse_txt(&map).unwrap().1;
        assert!(name.starts_with("abcx"));
        assert_eq!(name.chars().count(), 64);
        assert!(!name.chars().any(char::is_control));
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
