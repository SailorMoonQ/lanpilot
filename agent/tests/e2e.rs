//! Real QUIC on loopback against a real Agent with a recording input backend.

use lanpilot_agent::devices::DeviceStore;
use lanpilot_agent::paths::Paths;
use lanpilot_agent::server::Agent;
use lanpilot_agent::session::{CLOSE_DEVICE_REMOVED, CLOSE_UNPAIRED};
use lanpilot_core::framing::{read_msg, write_msg};
use lanpilot_core::identity::{Identity, PublicKey};
use lanpilot_core::pairing::flow::pair_with_invite;
use lanpilot_core::proto::v1::*;
use lanpilot_core::quinn;
use lanpilot_core::session::{ClientSession, connect_paired, local_hello, open_session};
use lanpilot_core::transport::client_endpoint;
use lanpilot_input::recording::{Recorded, RecordingBackend, RecordingHandle};
use lanpilot_input::{HidUsage, MouseButton as InMouse};
use prost::Message;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

struct Harness {
    _dir: tempfile::TempDir,
    paths: Paths,
    agent: Arc<Agent>,
    addr: SocketAddr,
    events: RecordingHandle,
}

async fn start(text: bool) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::under(dir.path());
    let (backend, events) = RecordingBackend::new(text);
    let agent = Agent::new(&paths, Box::new(backend)).unwrap();
    let endpoint = agent.bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let addr = endpoint.local_addr().unwrap();
    tokio::spawn(agent.clone().serve(endpoint));
    Harness {
        _dir: dir,
        paths,
        agent,
        addr,
        events,
    }
}

struct Client {
    id: Identity,
    endpoint: quinn::Endpoint,
    server: PublicKey,
}

async fn pair(h: &Harness) -> Client {
    let id = Identity::generate();
    let endpoint = client_endpoint(&id).unwrap();
    let invite = h.agent.invite(h.addr.port(), vec![h.addr.ip()]);
    let paired = pair_with_invite(&endpoint, &invite, "iPhone", Os::Ios)
        .await
        .unwrap();
    Client {
        id,
        endpoint,
        server: paired.public_key,
    }
}

async fn session(h: &Harness, c: &Client) -> (quinn::Connection, ClientSession) {
    let conn = connect_paired(&c.endpoint, h.addr, &c.server)
        .await
        .unwrap();
    let s = open_session(&conn, &local_hello("iPhone", Os::Ios, "0.1.0", &[]))
        .await
        .unwrap();
    (conn, s)
}

async fn request(s: &mut ClientSession, id: u64, body: client_message::Body) -> ServerMessage {
    write_msg(
        &mut s.send,
        &ClientMessage {
            request_id: id,
            body: Some(body),
        },
    )
    .await
    .unwrap();
    let reply: ServerMessage = tokio::time::timeout(Duration::from_secs(5), read_msg(&mut s.recv))
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(reply.request_id, id);
    reply
}

async fn wait_for(events: &RecordingHandle, pred: impl Fn(&[Recorded]) -> bool) {
    for _ in 0..100 {
        if pred(&events.events()) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("condition not met; events: {:?}", events.events());
}

fn gs(gesture_id: u32, seq: u32, dx: f32, dy: f32) -> GestureState {
    GestureState {
        gesture_id,
        seq,
        total_dx: dx,
        total_dy: dy,
        total_scroll_x: 0.0,
        total_scroll_y: 0.0,
    }
}

#[tokio::test]
async fn pair_then_control_the_pointer_and_keys() {
    let h = start(true).await;
    let c = pair(&h).await;
    assert!(h.agent.store().contains(&c.id.public_key()));

    let (conn, mut s) = session(&h, &c).await;
    assert_eq!(s.server_hello.capabilities, vec!["text".to_owned()]);

    conn.send_datagram(
        PointerDatagram {
            gesture: Some(gs(1, 1, 10.0, -2.0)),
        }
        .encode_to_vec()
        .into(),
    )
    .unwrap();
    wait_for(&h.events, |e| e.contains(&Recorded::Move(10, -2))).await;

    let r = request(
        &mut s,
        1,
        client_message::Body::PointerButton(PointerButton {
            button: MouseButton::Left as i32,
            down: true,
            gesture: Some(gs(1, 2, 12.0, -2.0)),
        }),
    )
    .await;
    assert!(matches!(r.body, Some(server_message::Body::Ack(_))));
    request(
        &mut s,
        2,
        client_message::Body::PointerButton(PointerButton {
            button: MouseButton::Left as i32,
            down: false,
            gesture: None,
        }),
    )
    .await;
    request(
        &mut s,
        3,
        client_message::Body::KeyChord(KeyChord {
            usages: vec![0xE0, 0x06],
        }),
    )
    .await;
    request(
        &mut s,
        4,
        client_message::Body::Text(Text {
            text: "你好".into(),
        }),
    )
    .await;
    let cmd = request(
        &mut s,
        5,
        client_message::Body::RunCommand(RunCommand {
            command_id: "x".into(),
        }),
    )
    .await;
    assert!(
        matches!(cmd.body, Some(server_message::Body::Error(e)) if e.code == ErrorCode::Unsupported as i32)
    );

    let events = h.events.events();
    let tail: Vec<_> = events
        .iter()
        .skip_while(|e| **e != Recorded::Move(2, 0))
        .cloned()
        .collect();
    assert_eq!(
        tail,
        vec![
            Recorded::Move(2, 0),
            Recorded::Button(InMouse::Left, true),
            Recorded::Button(InMouse::Left, false),
            Recorded::Key(HidUsage(0xE0), true),
            Recorded::Key(HidUsage(0x06), true),
            Recorded::Key(HidUsage(0x06), false),
            Recorded::Key(HidUsage(0xE0), false),
            Recorded::Text("你好".into()),
        ]
    );
}

#[tokio::test]
async fn held_button_is_released_when_the_phone_vanishes() {
    let h = start(true).await;
    let c = pair(&h).await;
    let (conn, mut s) = session(&h, &c).await;
    request(
        &mut s,
        1,
        client_message::Body::PointerButton(PointerButton {
            button: MouseButton::Left as i32,
            down: true,
            gesture: None,
        }),
    )
    .await;
    conn.close(0u32.into(), b"bye");
    wait_for(&h.events, |e| {
        e.last() == Some(&Recorded::Button(InMouse::Left, false))
    })
    .await;
}

#[tokio::test]
async fn unpaired_client_cannot_open_a_session() {
    let h = start(true).await;
    let stranger = Identity::generate();
    let endpoint = client_endpoint(&stranger).unwrap();
    let conn = connect_paired(&endpoint, h.addr, &h.agent.public_key())
        .await
        .unwrap();
    let result = open_session(&conn, &local_hello("x", Os::Ios, "0.1.0", &[])).await;
    assert!(result.is_err());
    // Datagrams from an unauthorized connection never reach the backend.
    let _ = conn.send_datagram(
        PointerDatagram {
            gesture: Some(gs(1, 1, 50.0, 0.0)),
        }
        .encode_to_vec()
        .into(),
    );
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(h.events.events().is_empty());
}

#[tokio::test]
async fn removing_the_device_from_the_cli_disconnects_it() {
    let h = start(true).await;
    let c = pair(&h).await;
    let (conn, _s) = session(&h, &c).await;
    // A second process (the CLI) edits the devices file.
    let cli = DeviceStore::open(h.paths.devices_file()).unwrap();
    cli.remove_by_short_id(&c.id.public_key().short_id())
        .unwrap()
        .unwrap();
    let reason = tokio::time::timeout(Duration::from_secs(5), conn.closed())
        .await
        .unwrap();
    assert!(
        matches!(&reason, quinn::ConnectionError::ApplicationClosed(a) if a.error_code == CLOSE_DEVICE_REMOVED.into()),
        "{reason:?}"
    );
}

#[tokio::test]
async fn unpair_message_acks_then_forgets_the_device() {
    let h = start(true).await;
    let c = pair(&h).await;
    let (conn, mut s) = session(&h, &c).await;
    let r = request(&mut s, 1, client_message::Body::Unpair(Unpair {})).await;
    assert!(matches!(r.body, Some(server_message::Body::Ack(_))));
    let reason = tokio::time::timeout(Duration::from_secs(5), conn.closed())
        .await
        .unwrap();
    assert!(
        matches!(&reason, quinn::ConnectionError::ApplicationClosed(a) if a.error_code == CLOSE_UNPAIRED.into()),
        "{reason:?}"
    );
    assert!(!h.agent.store().contains(&c.id.public_key()));
}

#[tokio::test]
async fn no_text_capability_without_text_support() {
    let h = start(false).await;
    let c = pair(&h).await;
    let (_conn, s) = session(&h, &c).await;
    assert!(s.server_hello.capabilities.is_empty());
}
