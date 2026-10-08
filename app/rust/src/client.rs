//! The long-lived client behind the Dart API. All QUIC work happens here;
//! `api::bridge` only forwards calls to one global instance.

use crate::api::types::{
    BridgeError, ConnectionEvent, DiscoveredInfo, ErrorKind, MediaKind, MouseButtonKind, OsKind,
    PairedServerInfo, SessionInfo,
};
use crate::control::Control;
use crate::error::{close_reason, from_connection, rank};
use crate::gesture::GestureAcc;
use lanpilot_core::discovery::DiscoveredDevice;
use lanpilot_core::identity::{Identity, PublicKey};
use lanpilot_core::pairing::flow::{PairedServer, pair_with_invite, pair_with_password};
use lanpilot_core::pairing::invite::Invite;
use lanpilot_core::proto::v1::Os;
use lanpilot_core::proto::v1::{
    GestureState, KeyChord, Media, MediaAction, MouseButton, PointerButton, PointerDatagram,
    Unpair, Zoom, client_message,
};
use lanpilot_core::quinn;
use lanpilot_core::session::{connect_paired, local_hello, open_session};
use lanpilot_core::transport::client_endpoint;
use prost::Message;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tokio::task::JoinSet;

/// Overall bound on one connect attempt. QUIC's own handshake timeout (3 s
/// idle) normally fires first; this is the safety net.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

pub type EventSink = Arc<dyn Fn(ConnectionEvent) + Send + Sync>;

struct Live {
    conn: quinn::Connection,
    control: Control,
    gesture: Mutex<GestureAcc>,
}

pub struct Client {
    identity: Identity,
    device_name: String,
    app_version: String,
    /// Created at the first network call, never at app start: iOS shows the
    /// Local Network prompt then, and a socket created before access was
    /// granted stays blocked (M0). `reset_endpoint` drops it.
    endpoint: tokio::sync::Mutex<Option<quinn::Endpoint>>,
    /// Serializes connect, disconnect and reset_endpoint.
    ops: tokio::sync::Mutex<()>,
    live: Mutex<Option<Arc<Live>>>,
    generation: AtomicU32,
    events: Arc<Mutex<Option<EventSink>>>,
}

impl Client {
    pub fn new(identity: Identity, device_name: &str, app_version: &str) -> Self {
        Self {
            identity,
            device_name: device_name.to_owned(),
            app_version: app_version.to_owned(),
            endpoint: tokio::sync::Mutex::new(None),
            ops: tokio::sync::Mutex::new(()),
            live: Mutex::new(None),
            generation: AtomicU32::new(0),
            events: Arc::new(Mutex::new(None)),
        }
    }

    pub fn short_id(&self) -> String {
        self.identity.public_key().short_id()
    }

    async fn endpoint(&self) -> Result<quinn::Endpoint, BridgeError> {
        let mut slot = self.endpoint.lock().await;
        if let Some(endpoint) = slot.as_ref() {
            return Ok(endpoint.clone());
        }
        let endpoint = client_endpoint(&self.identity)?;
        *slot = Some(endpoint.clone());
        Ok(endpoint)
    }

    /// Pairs with the computer in a `lanpilot://pair?d=...` link. Surrounding
    /// whitespace is ignored.
    pub async fn pair_with_uri(&self, uri: &str) -> Result<PairedServerInfo, BridgeError> {
        let invite = Invite::from_uri(uri.trim())
            .map_err(|e| BridgeError::new(ErrorKind::InvalidInput, e.to_string()))?;
        let endpoint = self.endpoint().await?;
        let paired = pair_with_invite(&endpoint, &invite, &self.device_name, Os::Ios).await?;
        let addrs = invite
            .addrs
            .iter()
            .filter(|ip| ip.is_ipv4())
            .map(|ip| SocketAddr::new(*ip, invite.port));
        Ok(paired_info(&paired, addrs))
    }

    /// Pairs with the computer at `addr` ("ip:port") using its pairing password.
    pub async fn pair_with_password(
        &self,
        addr: &str,
        password: &str,
    ) -> Result<PairedServerInfo, BridgeError> {
        let addr = parse_addr(addr)?;
        let endpoint = self.endpoint().await?;
        let paired = pair_with_password(
            &endpoint,
            addr,
            &self.identity,
            password,
            &self.device_name,
            Os::Ios,
        )
        .await?;
        Ok(paired_info(&paired, [addr]))
    }

    pub fn set_event_sink(&self, sink: EventSink) {
        *self.events.lock().unwrap_or_else(PoisonError::into_inner) = Some(sink);
    }

    /// Connects to the paired computer `server_key_hex`, trying every
    /// candidate address at once (spec 4.2), and opens a session.
    pub async fn connect(
        &self,
        server_key_hex: &str,
        candidates: &[String],
    ) -> Result<SessionInfo, BridgeError> {
        let key = parse_key(server_key_hex)?;
        let addrs = parse_candidates(candidates)?;
        let _op = self.ops.lock().await;
        self.close_live(b"replaced").await;
        let endpoint = self.endpoint().await?;
        let (conn, addr) = tokio::time::timeout(CONNECT_TIMEOUT, race(&endpoint, &key, &addrs))
            .await
            .map_err(|_| BridgeError::new(ErrorKind::Timeout, "connect timed out"))??;
        let hello = local_hello(&self.device_name, Os::Ios, &self.app_version, &[]);
        let session = match tokio::time::timeout(CONNECT_TIMEOUT, open_session(&conn, &hello)).await
        {
            Ok(Ok(session)) => session,
            Ok(Err(e)) => {
                let error = match conn.close_reason() {
                    Some(reason) => from_connection(&reason),
                    None => e.into(),
                };
                conn.close(0u32.into(), b"session failed");
                return Err(error);
            }
            Err(_) => {
                conn.close(0u32.into(), b"session timeout");
                return Err(BridgeError::new(
                    ErrorKind::Timeout,
                    "no Hello from the computer",
                ));
            }
        };
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let info = SessionInfo {
            generation,
            server_name: session.server_hello.device_name.clone(),
            server_os: os_kind(session.server_hello.os),
            version: session.version,
            capabilities: session.server_hello.capabilities.clone(),
            addr: addr.to_string(),
        };
        let live = Arc::new(Live {
            conn: conn.clone(),
            control: Control::start(session.send, session.recv),
            gesture: Mutex::new(GestureAcc::default()),
        });
        *self.live.lock().unwrap_or_else(PoisonError::into_inner) = Some(live);
        self.watch(conn, generation);
        Ok(info)
    }

    /// Ends the current session normally (close code 0).
    pub async fn disconnect(&self) {
        let _op = self.ops.lock().await;
        self.close_live(b"bye").await;
    }

    /// Ends the session and drops the QUIC endpoint, so the next call binds a
    /// fresh socket (spec 4.4).
    pub async fn reset_endpoint(&self) {
        let _op = self.ops.lock().await;
        self.close_live(b"reset").await;
        if let Some(endpoint) = self.endpoint.lock().await.take() {
            endpoint.close(0u32.into(), b"reset");
        }
    }

    pub fn begin_gesture(&self) -> Result<(), BridgeError> {
        let live = self.live()?;
        live.gesture
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .begin();
        Ok(())
    }

    /// Adds a pointer delta (pixels) to the current gesture and sends the totals.
    pub fn send_pointer(&self, dx: f64, dy: f64) -> Result<(), BridgeError> {
        let live = self.live()?;
        let state = live
            .gesture
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .motion(dx, dy);
        send_datagram(&live.conn, state)
    }

    /// Adds a scroll delta (notches) to the current gesture and sends the totals.
    pub fn send_scroll(&self, dx: f64, dy: f64) -> Result<(), BridgeError> {
        let live = self.live()?;
        let state = live
            .gesture
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .scroll(dx, dy);
        send_datagram(&live.conn, state)
    }

    pub async fn button(&self, button: MouseButtonKind, down: bool) -> Result<(), BridgeError> {
        let live = self.live()?;
        let gesture = live
            .gesture
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .current();
        let button = match button {
            MouseButtonKind::Left => MouseButton::Left,
            MouseButtonKind::Right => MouseButton::Right,
            MouseButtonKind::Middle => MouseButton::Middle,
        };
        live.control
            .request(client_message::Body::PointerButton(PointerButton {
                button: button as i32,
                down,
                gesture,
            }))
            .await
    }

    pub async fn key_chord(&self, usages: Vec<u32>) -> Result<(), BridgeError> {
        let live = self.live()?;
        live.control
            .request(client_message::Body::KeyChord(KeyChord { usages }))
            .await
    }

    pub async fn media(&self, action: MediaKind) -> Result<(), BridgeError> {
        let live = self.live()?;
        let action = match action {
            MediaKind::PlayPause => MediaAction::PlayPause,
            MediaKind::Next => MediaAction::Next,
            MediaKind::Previous => MediaAction::Previous,
            MediaKind::VolumeUp => MediaAction::VolumeUp,
            MediaKind::VolumeDown => MediaAction::VolumeDown,
            MediaKind::Mute => MediaAction::Mute,
        };
        live.control
            .request(client_message::Body::Media(Media {
                action: action as i32,
            }))
            .await
    }

    /// Ctrl + wheel on the computer; positive `steps` zoom in (spec 6.1).
    pub async fn zoom(&self, steps: f32) -> Result<(), BridgeError> {
        let live = self.live()?;
        live.control
            .request(client_message::Body::Zoom(Zoom { steps }))
            .await
    }

    /// Asks the computer to forget this phone. The computer then closes the
    /// connection with code 4, reported as `CloseReason::Unpaired`.
    pub async fn unpair(&self) -> Result<(), BridgeError> {
        let live = self.live()?;
        live.control
            .request(client_message::Body::Unpair(Unpair {}))
            .await
    }

    fn live(&self) -> Result<Arc<Live>, BridgeError> {
        let live = self
            .live
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or_else(|| BridgeError::new(ErrorKind::NotConnected, "not connected"))?;
        if let Some(reason) = live.conn.close_reason() {
            return Err(from_connection(&reason));
        }
        Ok(live)
    }

    async fn close_live(&self, reason: &'static [u8]) {
        let live = self
            .live
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(live) = live {
            live.control.finish().await;
            live.conn.close(0u32.into(), reason);
        }
    }

    /// Reports the end of a session, whatever closes it.
    fn watch(&self, conn: quinn::Connection, generation: u32) {
        let events = self.events.clone();
        tokio::spawn(async move {
            let error = conn.closed().await;
            let event = ConnectionEvent {
                generation,
                reason: close_reason(&error),
                message: error.to_string(),
            };
            let sink = events
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone();
            if let Some(sink) = sink {
                sink(event);
            }
        });
    }
}

pub fn os_kind(raw: i32) -> OsKind {
    match Os::try_from(raw).unwrap_or(Os::Unspecified) {
        Os::Windows => OsKind::Windows,
        Os::Linux => OsKind::Linux,
        Os::Macos => OsKind::Macos,
        Os::Ios => OsKind::Ios,
        Os::Android => OsKind::Android,
        Os::Unspecified => OsKind::Unknown,
    }
}

fn paired_info(
    paired: &PairedServer,
    addrs: impl IntoIterator<Item = SocketAddr>,
) -> PairedServerInfo {
    PairedServerInfo {
        public_key_hex: hex::encode(paired.public_key.as_bytes()),
        short_id: paired.public_key.short_id(),
        name: paired.name.clone(),
        os: os_kind(paired.os as i32),
        addrs: addrs.into_iter().map(|a| a.to_string()).collect(),
    }
}

pub fn parse_addr(addr: &str) -> Result<SocketAddr, BridgeError> {
    let parsed: SocketAddr = addr
        .trim()
        .parse()
        .map_err(|_| BridgeError::new(ErrorKind::InvalidInput, format!("bad address {addr:?}")))?;
    if !parsed.is_ipv4() || parsed.port() == 0 {
        return Err(BridgeError::new(
            ErrorKind::InvalidInput,
            format!("need an IPv4 address with a port, got {addr:?}"),
        ));
    }
    Ok(parsed)
}

pub fn parse_key(hex_key: &str) -> Result<PublicKey, BridgeError> {
    let bytes = hex::decode(hex_key.trim())
        .map_err(|_| BridgeError::new(ErrorKind::InvalidInput, "server key is not hex"))?;
    PublicKey::from_slice(&bytes)
        .map_err(|e| BridgeError::new(ErrorKind::InvalidInput, e.to_string()))
}

fn parse_candidates(candidates: &[String]) -> Result<Vec<SocketAddr>, BridgeError> {
    let mut addrs: Vec<SocketAddr> = Vec::new();
    for candidate in candidates {
        if let Ok(addr) = parse_addr(candidate)
            && !addrs.contains(&addr)
        {
            addrs.push(addr);
        }
    }
    if addrs.is_empty() {
        return Err(BridgeError::new(
            ErrorKind::InvalidInput,
            "no address to try",
        ));
    }
    Ok(addrs)
}

/// Connects to every address at once; the first that answers with the right
/// key wins and the others are cancelled (dropping the JoinSet aborts them).
async fn race(
    endpoint: &quinn::Endpoint,
    key: &PublicKey,
    addrs: &[SocketAddr],
) -> Result<(quinn::Connection, SocketAddr), BridgeError> {
    let mut attempts = JoinSet::new();
    for &addr in addrs {
        let endpoint = endpoint.clone();
        let key = *key;
        attempts.spawn(async move { (addr, connect_paired(&endpoint, addr, &key).await) });
    }
    let mut best: Option<BridgeError> = None;
    while let Some(joined) = attempts.join_next().await {
        let Ok((addr, result)) = joined else { continue };
        match result {
            Ok(conn) => return Ok((conn, addr)),
            Err(e) => {
                let e = BridgeError::from(e);
                if best.as_ref().is_none_or(|b| rank(e.kind) > rank(b.kind)) {
                    best = Some(e);
                }
            }
        }
    }
    Err(best.unwrap_or_else(|| BridgeError::new(ErrorKind::Unreachable, "no address answered")))
}

fn send_datagram(conn: &quinn::Connection, state: GestureState) -> Result<(), BridgeError> {
    let bytes = PointerDatagram {
        gesture: Some(state),
    }
    .encode_to_vec();
    conn.send_datagram(bytes.into())
        .map_err(|e| BridgeError::new(ErrorKind::Closed, e.to_string()))
}

/// Validates one Bonjour result with core (spec 3.2). Only IPv4 addresses are
/// kept; a result with none is dropped.
pub fn validate_discovered(
    fullname: &str,
    txt: &HashMap<String, String>,
    addrs: &[String],
    port: u16,
) -> Option<DiscoveredInfo> {
    if port == 0 {
        return None;
    }
    let ips: Vec<IpAddr> = addrs
        .iter()
        .filter_map(|a| a.trim().parse::<IpAddr>().ok())
        .filter(IpAddr::is_ipv4)
        .collect();
    if ips.is_empty() {
        return None;
    }
    let device = DiscoveredDevice::from_resolved(fullname, txt, ips, port)?;
    Some(DiscoveredInfo {
        short_id: device.short_id,
        name: device.name,
        os: os_kind(device.os as i32),
        proto_min: device.proto_min,
        proto_max: device.proto_max,
        addrs: device
            .addrs
            .iter()
            .map(|ip| SocketAddr::new(*ip, device.port).to_string())
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn txt(id: &str) -> HashMap<String, String> {
        HashMap::from([
            ("id".to_owned(), id.to_owned()),
            ("name".to_owned(), "Desk".to_owned()),
            ("os".to_owned(), "windows".to_owned()),
            ("proto".to_owned(), "1-1".to_owned()),
        ])
    }

    const ID: &str = "0123456789abcdef";

    #[test]
    fn validates_a_good_result() {
        let fullname = format!("{ID}._lanpilot._udp.local.");
        let info = validate_discovered(
            &fullname,
            &txt(ID),
            &["192.168.1.9".into(), "fe80::1".into()],
            45810,
        )
        .unwrap();
        assert_eq!(info.short_id, ID);
        assert_eq!(info.os, OsKind::Windows);
        assert_eq!(info.addrs, vec!["192.168.1.9:45810".to_owned()]);
    }

    #[test]
    fn rejects_mismatched_id_bad_port_or_no_ipv4() {
        let fullname = format!("{ID}._lanpilot._udp.local.");
        let addrs = ["192.168.1.9".to_owned()];
        assert!(validate_discovered(&fullname, &txt("ffffffffffffffff"), &addrs, 45810).is_none());
        assert!(validate_discovered(&fullname, &txt(ID), &addrs, 0).is_none());
        assert!(validate_discovered(&fullname, &txt(ID), &["fe80::1".into()], 45810).is_none());
    }

    #[test]
    fn parses_addresses_and_keys() {
        assert_eq!(parse_addr(" 10.0.0.2:45810 ").unwrap().port(), 45810);
        assert_eq!(
            parse_addr("10.0.0.2").unwrap_err().kind,
            ErrorKind::InvalidInput
        );
        assert_eq!(
            parse_addr("[::1]:45810").unwrap_err().kind,
            ErrorKind::InvalidInput
        );
        assert_eq!(parse_key("zz").unwrap_err().kind, ErrorKind::InvalidInput);
        assert!(parse_key(&"ab".repeat(32)).is_ok());
    }
}
