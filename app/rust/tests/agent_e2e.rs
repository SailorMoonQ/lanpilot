//! The bridge client against a real in-process agent with a recording input
//! backend, over loopback QUIC.

use lanpilot_agent::config::Config;
use lanpilot_agent::paths::Paths;
use lanpilot_agent::secret::store_password;
use lanpilot_agent::server::Agent;
use lanpilot_core::identity::Identity;
use lanpilot_input::recording::{Recorded, RecordingBackend, RecordingHandle};
use rust_lib_lanpilot::api::types::{ErrorKind, PairedServerInfo};
use rust_lib_lanpilot::client::Client;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

const PASSWORD: &str = "hunter22";

struct Harness {
    _dir: tempfile::TempDir,
    agent: Arc<Agent>,
    addr: SocketAddr,
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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
