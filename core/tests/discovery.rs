//! Real multicast on this machine. CI runners often block multicast, so these
//! are `#[ignore]`d there and run on developer machines with `-- --ignored`;
//! they are part of the per-milestone manual E2E checklist.

use lanpilot_core::discovery::{
    Advertisement, Advertiser, Browser, DiscoveredDevice, DiscoveryEvent,
};
use lanpilot_core::proto::v1::Os;
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

fn ad(short_id: &str, port: u16, addrs: Vec<IpAddr>) -> Advertisement {
    Advertisement {
        short_id: short_id.into(),
        name: "Test PC".into(),
        os: Os::Linux,
        proto_min: 1,
        proto_max: 1,
        port,
        addrs,
    }
}

async fn wait_found(browser: &mut Browser, short_id: &str) -> DiscoveredDevice {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(DiscoveryEvent::Found(d)) = browser.next().await
                && d.short_id == short_id
            {
                return d;
            }
        }
    })
    .await
    .expect("found within 10 s")
}

async fn wait_lost(browser: &mut Browser, short_id: &str) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(DiscoveryEvent::Lost { short_id: id }) = browser.next().await
                && id == short_id
            {
                return;
            }
        }
    })
    .await
    .expect("lost within 10 s");
}

/// A usable LAN IPv4 of this machine: not loopback, link-local or the
/// 198.18.0.0/15 benchmarking range that TUN proxies use.
fn lan_ipv4() -> Ipv4Addr {
    if_addrs::get_if_addrs()
        .unwrap()
        .into_iter()
        .filter_map(|i| match i.ip() {
            IpAddr::V4(v4) => Some(v4),
            IpAddr::V6(_) => None,
        })
        .find(|v4| {
            !v4.is_loopback()
                && !v4.is_link_local()
                && !(v4.octets()[0] == 198 && (v4.octets()[1] & 0xfe) == 18)
        })
        .expect("a LAN IPv4 address")
}

#[tokio::test]
#[ignore = "needs working local multicast; run with --ignored"]
async fn browser_finds_and_loses_advertiser() {
    let ad = ad("feedfacecafebeef", 45999, vec![]);
    let mut browser = Browser::start().unwrap();
    let advertiser = Advertiser::start(&ad).unwrap();

    let found = wait_found(&mut browser, &ad.short_id).await;
    assert_eq!(found.name, "Test PC");
    assert_eq!(found.port, 45999);
    assert!(!found.addrs.is_empty());

    advertiser.stop().await;
    wait_lost(&mut browser, &ad.short_id).await;
}

#[tokio::test]
#[ignore = "needs working local multicast; run with --ignored"]
async fn explicit_addrs_are_the_only_ones_advertised() {
    let ip = IpAddr::V4(lan_ipv4());
    let ad = ad("deadbeefcafef00d", 45998, vec![ip]);
    let mut browser = Browser::start().unwrap();
    let advertiser = Advertiser::start(&ad).unwrap();

    let found = wait_found(&mut browser, &ad.short_id).await;
    assert_eq!(found.addrs, vec![ip]);

    advertiser.stop().await;
    wait_lost(&mut browser, &ad.short_id).await;
}
