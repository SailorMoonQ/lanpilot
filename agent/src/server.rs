//! Accept loop and per-connection dispatch (spec 3.2, 5.1).

use crate::AgentError;
use crate::config::Config;
use crate::devices::DeviceStore;
use crate::pairing::{AgentPairing, current_os};
use crate::paths::Paths;
use crate::secret::load_or_create_identity;
use crate::session::{InputHub, SharedInput, run_session};
use lanpilot_core::discovery::{Advertisement, Advertiser};
use lanpilot_core::identity::{Identity, PublicKey};
use lanpilot_core::pairing::flow::serve_pairing;
use lanpilot_core::pairing::invite::Invite;
use lanpilot_core::quinn;
use lanpilot_core::session::{Opened, SessionError, accept_authorized, answer_hello, local_hello};
use lanpilot_core::transport::{peer_public_key, server_endpoint};
use lanpilot_core::version::{PROTO_MAX, PROTO_MIN};
use lanpilot_input::InputBackend;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use tokio_util::task::TaskTracker;

pub const OPEN_TIMEOUT: Duration = Duration::from_secs(10);
pub const PAIRING_TIMEOUT: Duration = Duration::from_secs(30);

pub struct Agent {
    identity: Identity,
    config: Config,
    store: Arc<DeviceStore>,
    pairing: Arc<AgentPairing>,
    input: SharedInput,
    supports_text: bool,
    tasks: TaskTracker,
    _watcher: notify::RecommendedWatcher,
}

impl Agent {
    pub fn new(paths: &Paths, input: Box<dyn InputBackend>) -> Result<Arc<Self>, AgentError> {
        let config = Config::load_or_default(&paths.config_file)?;
        let identity = load_or_create_identity(&paths.identity_file())?;
        let store = DeviceStore::open(paths.devices_file())?;
        let watcher = store.watch()?;
        let pairing = Arc::new(AgentPairing::new(
            config.general.name.clone(),
            config.pairing.password_enabled,
            paths.password_file(),
            store.clone(),
        ));
        let supports_text = input.supports_text();
        Ok(Arc::new(Self {
            identity,
            config,
            store,
            pairing,
            input: InputHub::shared(input),
            supports_text,
            tasks: TaskTracker::new(),
            _watcher: watcher,
        }))
    }

    pub fn public_key(&self) -> PublicKey {
        self.identity.public_key()
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn store(&self) -> &Arc<DeviceStore> {
        &self.store
    }

    pub fn capabilities(&self) -> Vec<&'static str> {
        if self.supports_text {
            vec!["text", "zoom"]
        } else {
            vec!["zoom"]
        }
    }

    pub fn bind(&self, addr: SocketAddr) -> Result<quinn::Endpoint, AgentError> {
        server_endpoint(&self.identity, addr).map_err(|e| AgentError::Core(e.to_string()))
    }

    pub fn invite(&self, port: u16, addrs: Vec<IpAddr>) -> Invite {
        self.pairing.issue_invite(self.public_key(), port, addrs)
    }

    pub fn advertise(&self, port: u16) -> Result<Advertiser, AgentError> {
        Advertiser::start(&Advertisement {
            short_id: self.public_key().short_id(),
            name: self.config.general.name.clone(),
            os: current_os(),
            proto_min: PROTO_MIN,
            proto_max: PROTO_MAX,
            port,
        })
        .map_err(|e| AgentError::Core(e.to_string()))
    }

    pub async fn serve(self: Arc<Self>, endpoint: quinn::Endpoint) {
        while let Some(incoming) = endpoint.accept().await {
            let agent = self.clone();
            self.tasks
                .spawn(async move { agent.handle(incoming).await });
        }
    }

    /// Closes the endpoint and waits for live connections to finish, so each
    /// session releases held input and the phones see the close.
    pub async fn shutdown(&self, endpoint: &quinn::Endpoint) {
        endpoint.close(0u32.into(), b"agent shutting down");
        self.tasks.close();
        if timeout(Duration::from_secs(5), self.tasks.wait())
            .await
            .is_err()
        {
            tracing::warn!("timed out waiting for sessions to end");
        }
        let _ = timeout(Duration::from_secs(2), endpoint.wait_idle()).await;
    }

    async fn handle(self: Arc<Self>, incoming: quinn::Incoming) {
        let conn = match timeout(OPEN_TIMEOUT, incoming).await {
            Ok(Ok(c)) => c,
            Ok(Err(e)) => {
                tracing::debug!("handshake failed: {e}");
                return;
            }
            Err(_) => {
                tracing::debug!("handshake timed out");
                return;
            }
        };
        let peer = match peer_public_key(&conn) {
            Ok(p) => p,
            Err(e) => {
                tracing::debug!("no usable peer key: {e}");
                return;
            }
        };
        let who = peer.short_id();
        let store = self.store.clone();
        let opened = match timeout(
            OPEN_TIMEOUT,
            accept_authorized(&conn, |k| store.contains(k)),
        )
        .await
        {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => {
                tracing::debug!("{who}: rejected: {e}");
                return;
            }
            Err(_) => {
                tracing::debug!("{who}: no stream opened in time");
                return;
            }
        };
        match opened {
            Opened::Pair {
                send,
                recv,
                request,
            } => {
                let server_key = self.public_key();
                match timeout(
                    PAIRING_TIMEOUT,
                    serve_pairing(&conn, &server_key, send, recv, request, &*self.pairing),
                )
                .await
                {
                    Ok(Ok(Some(d))) => tracing::debug!("{who}: paired as {}", d.name),
                    Ok(Ok(None)) => tracing::info!("{who}: pairing rejected"),
                    // An error or timeout can also come after `approve()`,
                    // when only the result delivery failed (core's
                    // "uncertain" case). The device then stays paired on
                    // purpose: the token or SPAKE2 check passed, so only a
                    // holder of the secret got this far. The phone can still
                    // connect, or be removed with `devices remove`.
                    Ok(Err(e)) => tracing::debug!("{who}: pairing failed: {e}"),
                    Err(_) => tracing::debug!("{who}: pairing timed out"),
                }
            }
            Opened::Session {
                mut send,
                recv,
                hello,
            } => {
                let local = local_hello(
                    &self.config.general.name,
                    current_os(),
                    env!("CARGO_PKG_VERSION"),
                    &self.capabilities(),
                );
                match answer_hello(&mut send, &local, &hello).await {
                    Ok(_version) => {
                        tracing::info!("{who}: session started ({})", hello.device_name);
                        run_session(
                            conn,
                            peer,
                            send,
                            recv,
                            self.input.clone(),
                            self.store.clone(),
                        )
                        .await;
                        tracing::info!("{who}: session ended");
                    }
                    Err(e @ SessionError::Version(_)) => tracing::info!("{who}: {e}"),
                    Err(e) => tracing::debug!("{who}: hello failed: {e}"),
                }
            }
        }
    }
}
