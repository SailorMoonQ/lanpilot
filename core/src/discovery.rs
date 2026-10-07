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
    Found(DiscoveredDevice),
    Lost { short_id: String },
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

pub fn parse_txt(props: &HashMap<String, String>) -> Option<(String, String, Os, u32, u32)> {
    let id = props.get("id").filter(|s| !s.is_empty())?.clone();
    let name = props.get("name").cloned().unwrap_or_default();
    let os = os_from_str(props.get("os").map(String::as_str).unwrap_or_default());
    let (min, max) = props.get("proto")?.split_once('-')?;
    Some((id, name, os, min.parse().ok()?, max.parse().ok()?))
}

fn short_id_from_fullname(fullname: &str) -> Option<String> {
    fullname
        .strip_suffix(SERVICE_TYPE)
        .and_then(|s| s.strip_suffix('.'))
        .map(str::to_owned)
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
                    let Some((short_id, name, os, proto_min, proto_max)) = parse_txt(&props) else {
                        continue;
                    };
                    let mut addrs: Vec<IpAddr> = info.get_addresses().iter().copied().collect();
                    addrs.sort();
                    return Some(DiscoveryEvent::Found(DiscoveredDevice {
                        short_id,
                        name,
                        os,
                        proto_min,
                        proto_max,
                        addrs,
                        port: info.get_port(),
                    }));
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
    fn short_id_parses_fullname() {
        assert_eq!(
            short_id_from_fullname("0123456789abcdef._lanpilot._udp.local."),
            Some("0123456789abcdef".to_owned())
        );
        assert_eq!(short_id_from_fullname("other._http._tcp.local."), None);
    }
}
