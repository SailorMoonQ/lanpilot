//! Functions exposed to Dart. Each forwards to the one global [`Client`].
//! Async work runs on the bridge's own tokio runtime, so the QUIC endpoint and
//! the connection outlive any single call.

use crate::api::types::{
    BridgeError, ConnectionEvent, DiscoveredInfo, ErrorKind, MediaKind, MouseButtonKind,
    PairedServerInfo, SessionInfo,
};
use crate::client::{self, Client};
use crate::frb_generated::StreamSink;
use lanpilot_core::identity::Identity;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

static CLIENT: OnceLock<Arc<Client>> = OnceLock::new();
static EVENTS: Mutex<Option<StreamSink<ConnectionEvent>>> = Mutex::new(None);

fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("lanpilot-bridge")
            .enable_all()
            .build()
            .expect("tokio runtime")
    })
}

fn client() -> Result<Arc<Client>, BridgeError> {
    CLIENT
        .get()
        .cloned()
        .ok_or_else(|| BridgeError::new(ErrorKind::Internal, "init_client was not called"))
}

/// Runs `work` on the bridge runtime and awaits it from Dart's executor.
async fn on_rt<T, F>(work: impl FnOnce(Arc<Client>) -> F) -> Result<T, BridgeError>
where
    T: Send + 'static,
    F: Future<Output = Result<T, BridgeError>> + Send + 'static,
{
    let client = client()?;
    runtime()
        .spawn(work(client))
        .await
        .map_err(|e| BridgeError::new(ErrorKind::Internal, e.to_string()))?
}

fn forward_event(event: ConnectionEvent) {
    let sink = EVENTS.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(sink) = sink.as_ref() {
        let _ = sink.add(event);
    }
}

/// A new 32-byte identity secret, for the keychain.
#[flutter_rust_bridge::frb(sync)]
pub fn generate_secret() -> Vec<u8> {
    Identity::generate().secret_bytes().to_vec()
}

/// Creates the global client from the keychain secret and returns this
/// phone's short id. Calling it again with the same secret is a no-op.
#[flutter_rust_bridge::frb(sync)]
pub fn init_client(
    secret: Vec<u8>,
    device_name: String,
    app_version: String,
) -> Result<String, BridgeError> {
    let secret: [u8; 32] = secret
        .as_slice()
        .try_into()
        .map_err(|_| BridgeError::new(ErrorKind::InvalidInput, "the secret must be 32 bytes"))?;
    let identity = Identity::from_secret_bytes(&secret);
    let short_id = identity.public_key().short_id();
    let client = CLIENT.get_or_init(|| {
        let client = Client::new(identity, &device_name, &app_version);
        client.set_event_sink(Arc::new(forward_event));
        Arc::new(client)
    });
    if client.short_id() != short_id {
        return Err(BridgeError::new(
            ErrorKind::Internal,
            "already initialized with another identity",
        ));
    }
    Ok(short_id)
}

/// Session ends, one event per session. Subscribing again replaces the
/// previous stream.
pub fn connection_events(sink: StreamSink<ConnectionEvent>) {
    *EVENTS.lock().unwrap_or_else(PoisonError::into_inner) = Some(sink);
}

#[flutter_rust_bridge::frb(sync)]
pub fn validate_discovered(
    fullname: String,
    txt: HashMap<String, String>,
    addrs: Vec<String>,
    port: u16,
) -> Option<DiscoveredInfo> {
    client::validate_discovered(&fullname, &txt, &addrs, port)
}

pub async fn pair_with_uri(uri: String) -> Result<PairedServerInfo, BridgeError> {
    on_rt(|c| async move { c.pair_with_uri(&uri).await }).await
}

pub async fn pair_with_password(
    addr: String,
    password: String,
) -> Result<PairedServerInfo, BridgeError> {
    on_rt(|c| async move { c.pair_with_password(&addr, &password).await }).await
}

pub async fn connect(
    server_key_hex: String,
    candidates: Vec<String>,
) -> Result<SessionInfo, BridgeError> {
    on_rt(|c| async move { c.connect(&server_key_hex, &candidates).await }).await
}

pub async fn disconnect() -> Result<(), BridgeError> {
    on_rt(|c| async move {
        c.disconnect().await;
        Ok(())
    })
    .await
}

pub async fn reset_endpoint() -> Result<(), BridgeError> {
    on_rt(|c| async move {
        c.reset_endpoint().await;
        Ok(())
    })
    .await
}

#[flutter_rust_bridge::frb(sync)]
pub fn begin_gesture() -> Result<(), BridgeError> {
    client()?.begin_gesture()
}

#[flutter_rust_bridge::frb(sync)]
pub fn send_pointer(dx: f64, dy: f64) -> Result<(), BridgeError> {
    client()?.send_pointer(dx, dy)
}

#[flutter_rust_bridge::frb(sync)]
pub fn send_scroll(dx: f64, dy: f64) -> Result<(), BridgeError> {
    client()?.send_scroll(dx, dy)
}

pub async fn button(button: MouseButtonKind, down: bool) -> Result<(), BridgeError> {
    on_rt(|c| async move { c.button(button, down).await }).await
}

pub async fn key_chord(usages: Vec<u32>) -> Result<(), BridgeError> {
    on_rt(|c| async move { c.key_chord(usages).await }).await
}

pub async fn media(action: MediaKind) -> Result<(), BridgeError> {
    on_rt(|c| async move { c.media(action).await }).await
}

pub async fn zoom(steps: f32) -> Result<(), BridgeError> {
    on_rt(|c| async move { c.zoom(steps).await }).await
}

pub async fn unpair() -> Result<(), BridgeError> {
    on_rt(|c| async move { c.unpair().await }).await
}

#[flutter_rust_bridge::frb(init)]
pub fn init_app() {
    flutter_rust_bridge::setup_default_user_utils();
}
