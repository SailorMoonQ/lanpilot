//! The bridge client against a real in-process agent with a recording input
//! backend, over loopback QUIC.

use lanpilot_agent::config::Config;
use lanpilot_agent::paths::Paths;
use lanpilot_agent::secret::store_password;
use lanpilot_agent::server::Agent;
use lanpilot_core::identity::Identity;
use lanpilot_input::recording::{Recorded, RecordingBackend, RecordingHandle};
use lanpilot_input::{HidUsage, MediaKey, MouseButton as InMouse};
use rust_lib_lanpilot::api::types::{CloseReason, ConnectionEvent, MediaKind, MouseButtonKind};
use rust_lib_lanpilot::api::types::{ErrorKind, PairedServerInfo};
use rust_lib_lanpilot::client::Client;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

const PASSWORD: &str = "hunter22";

struct Harness {
    _dir: tempfile::TempDir,
    agent: Arc<Agent>,
    addr: SocketAddr,
    events: RecordingHandle,
}

impl Harness {
    async fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::under(dir.path());
        let mut config = Config::load_or_default(&paths.config_file).unwrap();
        config.pairing.password_enabled = true;
        config.save(&paths.config_file).unwrap();
        store_password(&paths.password_file(), PASSWORD).unwrap();
        let (backend, events) = RecordingBackend::new(true);
        let agent = Agent::new(&paths, Box::new(backend)).unwrap();
        let endpoint = agent.bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let addr = endpoint.local_addr().unwrap();
        tokio::spawn(agent.clone().serve(endpoint));
        Self {
            _dir: dir,
            agent,
            addr,
            events,
        }
    }

    fn uri(&self) -> String {
        self.agent
            .invite(self.addr.port(), vec![self.addr.ip()])
            .to_uri()
    }

    fn addr(&self) -> String {
        self.addr.to_string()
    }

    /// Polls the recorded input until `done` holds, failing after 5 s.
    async fn wait_for(&self, what: &str, done: impl Fn(&[Recorded]) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if done(&self.events.events()) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {what}: {:?}",
                self.events.events()
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

fn new_client() -> Client {
    Client::new(Identity::generate(), "iPhone test", "0.2.0")
}

async fn paired(h: &Harness) -> (Client, PairedServerInfo) {
    let client = new_client();
    let info = client.pair_with_uri(&h.uri()).await.unwrap();
    (client, info)
}

#[tokio::test(flavor = "multi_thread")]
async fn pair_with_uri_returns_the_server() {
    let h = Harness::start().await;
    let (_client, info) = paired(&h).await;
    assert_eq!(info.short_id, h.agent.public_key().short_id());
    assert_eq!(info.public_key_hex.len(), 64);
    assert_eq!(info.addrs, vec![h.addr()]);
    assert_eq!(info.name, h.agent.config().general.name);
}

#[tokio::test(flavor = "multi_thread")]
async fn pair_with_uri_tolerates_whitespace() {
    let h = Harness::start().await;
    let info = new_client()
        .pair_with_uri(&format!("  \n{}\t\n", h.uri()))
        .await
        .unwrap();
    assert_eq!(info.short_id, h.agent.public_key().short_id());
}

#[tokio::test(flavor = "multi_thread")]
async fn pair_with_uri_rejects_junk() {
    let client = new_client();
    for junk in ["", "hello", "https://example.com", "lanpilot://pair?d=@@@"] {
        let e = client.pair_with_uri(junk).await.unwrap_err();
        assert_eq!(e.kind, ErrorKind::InvalidInput, "{junk:?}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn reused_invite_is_a_bad_token() {
    let h = Harness::start().await;
    let uri = h.uri();
    new_client().pair_with_uri(&uri).await.unwrap();
    let e = new_client().pair_with_uri(&uri).await.unwrap_err();
    assert_eq!(e.kind, ErrorKind::BadToken);
}

#[tokio::test(flavor = "multi_thread")]
async fn password_pairing() {
    let h = Harness::start().await;
    let client = new_client();
    let e = client
        .pair_with_password(&h.addr(), "wrong-password")
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::WrongPassword);
    let info = client
        .pair_with_password(&h.addr(), PASSWORD)
        .await
        .unwrap();
    assert_eq!(info.short_id, h.agent.public_key().short_id());
    assert_eq!(info.addrs, vec![h.addr()]);
}

fn sink(client: &Client) -> mpsc::UnboundedReceiver<ConnectionEvent> {
    let (tx, rx) = mpsc::unbounded_channel();
    client.set_event_sink(Arc::new(move |e| {
        let _ = tx.send(e);
    }));
    rx
}

async fn next_event(rx: &mut mpsc::UnboundedReceiver<ConnectionEvent>) -> ConnectionEvent {
    tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("no connection event within 5 s")
        .expect("sink dropped")
}

/// A UDP socket that never answers: QUIC handshakes to it time out.
fn black_hole() -> (std::net::UdpSocket, String) {
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let addr = socket.local_addr().unwrap().to_string();
    (socket, addr)
}

fn moved_x(events: &[Recorded]) -> i32 {
    events
        .iter()
        .map(|e| match e {
            Recorded::Move(dx, _) => *dx,
            _ => 0,
        })
        .sum()
}

#[test]
fn close_codes_match_the_agent() {
    assert_eq!(
        rust_lib_lanpilot::error::CLOSE_DEVICE_REMOVED,
        lanpilot_agent::session::CLOSE_DEVICE_REMOVED
    );
    assert_eq!(
        rust_lib_lanpilot::error::CLOSE_UNPAIRED,
        lanpilot_agent::session::CLOSE_UNPAIRED
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn connect_reports_the_session() {
    let h = Harness::start().await;
    let (client, info) = paired(&h).await;
    let session = client
        .connect(&info.public_key_hex, &info.addrs)
        .await
        .unwrap();
    assert_eq!(session.generation, 1);
    assert_eq!(session.version, 1);
    assert_eq!(session.addr, h.addr());
    assert_eq!(session.server_name, info.name);
    assert!(session.capabilities.iter().any(|c| c == "zoom"));
}

#[tokio::test(flavor = "multi_thread")]
async fn pointer_and_buttons_reach_the_agent() {
    let h = Harness::start().await;
    let (client, info) = paired(&h).await;
    client
        .connect(&info.public_key_hex, &info.addrs)
        .await
        .unwrap();
    client.begin_gesture().unwrap();
    for _ in 0..3 {
        client.send_pointer(10.0, 0.0).unwrap();
    }
    h.wait_for("30 px of motion", |e| moved_x(e) == 30).await;
    client.button(MouseButtonKind::Left, true).await.unwrap();
    client.button(MouseButtonKind::Left, false).await.unwrap();
    h.wait_for("left click", |e| {
        e.contains(&Recorded::Button(InMouse::Left, true))
            && e.contains(&Recorded::Button(InMouse::Left, false))
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn scroll_keys_media_and_zoom_reach_the_agent() {
    let h = Harness::start().await;
    let (client, info) = paired(&h).await;
    client
        .connect(&info.public_key_hex, &info.addrs)
        .await
        .unwrap();
    client.begin_gesture().unwrap();
    client.send_scroll(0.0, 1.5).unwrap();
    h.wait_for("scroll", |e| {
        e.iter()
            .any(|x| matches!(x, Recorded::Scroll(_, y) if *y > 1.4))
    })
    .await;
    client.key_chord(vec![0xE0, 0x06]).await.unwrap();
    client.media(MediaKind::PlayPause).await.unwrap();
    client.zoom(2.0).await.unwrap();
    h.wait_for("chord, media and zoom", |e| {
        e.contains(&Recorded::Key(HidUsage(0x06), true))
            && e.contains(&Recorded::Media(MediaKey::PlayPause))
            && e.contains(&Recorded::Scroll(0.0, 2.0))
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn requests_report_agent_errors() {
    let h = Harness::start().await;
    let (client, info) = paired(&h).await;
    client
        .connect(&info.public_key_hex, &info.addrs)
        .await
        .unwrap();
    let e = client.zoom(f32::NAN).await.unwrap_err();
    assert_eq!(e.kind, ErrorKind::RequestFailed);
    // The control stream is still in sync afterwards.
    client.media(MediaKind::Mute).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn input_without_a_session_is_not_connected() {
    let client = new_client();
    assert_eq!(
        client.send_pointer(1.0, 1.0).unwrap_err().kind,
        ErrorKind::NotConnected
    );
    assert_eq!(
        client.begin_gesture().unwrap_err().kind,
        ErrorKind::NotConnected
    );
    let e = client.media(MediaKind::Mute).await.unwrap_err();
    assert_eq!(e.kind, ErrorKind::NotConnected);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_live_address_wins_the_race() {
    let h = Harness::start().await;
    let (client, info) = paired(&h).await;
    let (_hole, dead) = black_hole();
    let started = Instant::now();
    let session = client
        .connect(&info.public_key_hex, &[dead, h.addr()])
        .await
        .unwrap();
    assert_eq!(session.addr, h.addr());
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dead_address_times_out() {
    let (_hole, dead) = black_hole();
    let client = new_client();
    let e = client.connect(&"ab".repeat(32), &[dead]).await.unwrap_err();
    assert_eq!(e.kind, ErrorKind::Timeout);
}

#[tokio::test(flavor = "multi_thread")]
async fn connect_rejects_bad_input() {
    let client = new_client();
    let e = client
        .connect("not hex", &["10.0.0.1:1".into()])
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::InvalidInput);
    let e = client.connect(&"ab".repeat(32), &[]).await.unwrap_err();
    assert_eq!(e.kind, ErrorKind::InvalidInput);
}

#[tokio::test(flavor = "multi_thread")]
async fn wrong_server_key_is_a_mismatch() {
    let h = Harness::start().await;
    let (client, _info) = paired(&h).await;
    let other = Identity::generate().public_key();
    let e = client
        .connect(&hex::encode(other.as_bytes()), &[h.addr()])
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::ServerKeyMismatch);
}

#[tokio::test(flavor = "multi_thread")]
async fn reconnect_replaces_the_session() {
    let h = Harness::start().await;
    let (client, info) = paired(&h).await;
    let mut events = sink(&client);
    client
        .connect(&info.public_key_hex, &info.addrs)
        .await
        .unwrap();
    let second = client
        .connect(&info.public_key_hex, &info.addrs)
        .await
        .unwrap();
    assert_eq!(second.generation, 2);
    let first_closed = next_event(&mut events).await;
    assert_eq!(
        (first_closed.generation, first_closed.reason),
        (1, CloseReason::Local)
    );
    client.media(MediaKind::Next).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn disconnect_waits_for_an_in_flight_connect() {
    let h = Harness::start().await;
    let (client, info) = paired(&h).await;
    // `biased` polls the connect first: it takes the free ops lock and parks
    // on the network, so the disconnect queues behind it (FIFO mutex).
    let (connected, ()) = tokio::join!(
        biased;
        client.connect(&info.public_key_hex, &info.addrs),
        client.disconnect(),
    );
    assert!(connected.is_ok(), "{connected:?}");
    // The disconnect ran after the connect, so nothing is left connected.
    assert_eq!(
        client.send_pointer(1.0, 0.0).unwrap_err().kind,
        ErrorKind::NotConnected
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn unpair_closes_with_unpaired() {
    let h = Harness::start().await;
    let (client, info) = paired(&h).await;
    let mut events = sink(&client);
    let session = client
        .connect(&info.public_key_hex, &info.addrs)
        .await
        .unwrap();
    client.unpair().await.unwrap();
    let event = next_event(&mut events).await;
    assert_eq!(
        (event.generation, event.reason),
        (session.generation, CloseReason::Unpaired)
    );
    assert!(h.agent.store().list().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn removal_on_the_computer_is_reported() {
    let h = Harness::start().await;
    let client = new_client();
    let info = client.pair_with_uri(&h.uri()).await.unwrap();
    let mut events = sink(&client);
    client
        .connect(&info.public_key_hex, &info.addrs)
        .await
        .unwrap();
    let phone_key = h.agent.store().list()[0].public_key;
    h.agent.store().remove(&phone_key).unwrap();
    let event = next_event(&mut events).await;
    assert_eq!(event.reason, CloseReason::DeviceRemoved);
    let e = client
        .connect(&info.public_key_hex, &info.addrs)
        .await
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::NotPaired);
}

#[tokio::test(flavor = "multi_thread")]
async fn reset_endpoint_then_connect_works() {
    let h = Harness::start().await;
    let (client, info) = paired(&h).await;
    let mut events = sink(&client);
    client
        .connect(&info.public_key_hex, &info.addrs)
        .await
        .unwrap();
    client.reset_endpoint().await;
    let session = client
        .connect(&info.public_key_hex, &info.addrs)
        .await
        .unwrap();
    assert_eq!(session.generation, 2);
    // The connect, not the reset, closed the first session.
    let replaced = next_event(&mut events).await;
    assert_eq!(
        (replaced.generation, replaced.reason),
        (1, CloseReason::Local)
    );
    client.media(MediaKind::Next).await.unwrap();
}

/// A pairing retry resets the endpoint while a session is live (spec 4.4).
/// The live session must keep working: nothing would report its loss.
#[tokio::test(flavor = "multi_thread")]
async fn reset_endpoint_keeps_the_live_session() {
    let h = Harness::start().await;
    let (client, info) = paired(&h).await;
    let mut events = sink(&client);
    client
        .connect(&info.public_key_hex, &info.addrs)
        .await
        .unwrap();
    client.reset_endpoint().await;
    client.media(MediaKind::PlayPause).await.unwrap();
    h.wait_for("media after the reset", |e| {
        e.contains(&Recorded::Media(MediaKey::PlayPause))
    })
    .await;
    let quiet = tokio::time::timeout(Duration::from_millis(500), events.recv()).await;
    assert!(quiet.is_err(), "the live session closed: {quiet:?}");
    let session = client
        .connect(&info.public_key_hex, &info.addrs)
        .await
        .unwrap();
    assert_eq!(session.generation, 2);
    client.media(MediaKind::Next).await.unwrap();
}
