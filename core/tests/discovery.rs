//! Real multicast on this machine. CI runners often block multicast, so this is
//! `#[ignore]`d there and run on developer machines with `-- --ignored`; it is
//! part of the per-milestone manual E2E checklist.

use lanpilot_core::discovery::{Advertisement, Advertiser, Browser, DiscoveryEvent};
use lanpilot_core::proto::v1::Os;
use std::time::Duration;

#[tokio::test]
#[ignore = "needs working local multicast; run with --ignored"]
async fn browser_finds_and_loses_advertiser() {
    let ad = Advertisement {
        short_id: "feedfacecafebeef".into(),
        name: "Test PC".into(),
        os: Os::Linux,
        proto_min: 1,
        proto_max: 1,
        port: 45999,
    };
    let mut browser = Browser::start().unwrap();
    let advertiser = Advertiser::start(&ad).unwrap();

    let found = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(DiscoveryEvent::Found(d)) = browser.next().await
                && d.short_id == ad.short_id
            {
                return d;
            }
        }
    })
    .await
    .expect("found within 10 s");
    assert_eq!(found.name, "Test PC");
    assert_eq!(found.port, 45999);
    assert!(!found.addrs.is_empty());

    drop(advertiser);
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(DiscoveryEvent::Lost { short_id }) = browser.next().await
                && short_id == ad.short_id
            {
                return;
            }
        }
    })
    .await
    .expect("lost within 10 s");
}
