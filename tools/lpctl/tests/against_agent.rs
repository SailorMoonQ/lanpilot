//! lpctl driving a real in-process Agent over loopback.

use lanpilot_agent::paths::Paths;
use lanpilot_agent::server::Agent;
use lanpilot_input::recording::{Recorded, RecordingBackend};
use lanpilot_input::{HidUsage, MouseButton as InMouse};
use lpctl::store::ClientStore;
use lpctl::{Action, pair_uri, perform};
use std::time::Duration;

#[tokio::test]
async fn pair_and_drive_the_agent() {
    let agent_dir = tempfile::tempdir().unwrap();
    let (backend, events) = RecordingBackend::new(true);
    let agent = Agent::new(&Paths::under(agent_dir.path()), Box::new(backend)).unwrap();
    let endpoint = agent.bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let addr = endpoint.local_addr().unwrap();
    tokio::spawn(agent.clone().serve(endpoint));

    let client_dir = tempfile::tempdir().unwrap();
    let mut store = ClientStore::open(client_dir.path().to_owned()).unwrap();
    let uri = agent.invite(addr.port(), vec![addr.ip()]).to_uri();
    let server = pair_uri(&mut store, &uri, "lpctl-test").await.unwrap();
    assert_eq!(server.public_key, agent.public_key());

    perform(
        &mut store,
        &server,
        None,
        Action::Move {
            dx: 30.0,
            dy: 0.0,
            steps: 3,
        },
    )
    .await
    .unwrap();
    perform(&mut store, &server, None, Action::Keys(vec![0xE0, 0x06]))
        .await
        .unwrap();
    perform(
        &mut store,
        &server,
        None,
        Action::Click(lanpilot_core::proto::v1::MouseButton::Left),
    )
    .await
    .unwrap();

    // Datagrams are unreliable but loopback does not drop; give them a moment.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let e = events.events();
    let moved: i32 = e
        .iter()
        .filter_map(|r| {
            if let Recorded::Move(dx, _) = r {
                Some(*dx)
            } else {
                None
            }
        })
        .sum();
    assert_eq!(moved, 30);
    let keys: Vec<&Recorded> = e
        .iter()
        .filter(|r| matches!(r, Recorded::Key(..)))
        .collect();
    assert_eq!(
        keys,
        vec![
            &Recorded::Key(HidUsage(0xE0), true),
            &Recorded::Key(HidUsage(0x06), true),
            &Recorded::Key(HidUsage(0x06), false),
            &Recorded::Key(HidUsage(0xE0), false),
        ]
    );
    let down = e
        .iter()
        .position(|r| *r == Recorded::Button(InMouse::Left, true))
        .expect("button down");
    let up = e
        .iter()
        .position(|r| *r == Recorded::Button(InMouse::Left, false))
        .expect("button up");
    assert!(down < up);

    perform(&mut store, &server, None, Action::Unpair)
        .await
        .unwrap();
    assert!(store.servers().is_empty());
    // The agent forgets the device right after delivering the Ack.
    let me = store.identity().public_key();
    for _ in 0..100 {
        if !agent.store().contains(&me) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("agent still lists the unpaired device");
}
