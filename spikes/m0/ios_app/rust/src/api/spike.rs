//! M0 iOS spike: pair, connect, draw a square and measure control-stream latency.
//!
//! All QUIC work runs on one long-lived tokio runtime owned here, so the
//! endpoint and connection survive between calls from Dart.

use anyhow::{Context, anyhow, bail};
use lanpilot_core::framing::{read_msg, write_msg};
use lanpilot_core::identity::{Identity, PublicKey};
use lanpilot_core::pairing::flow::pair_with_invite;
use lanpilot_core::pairing::invite::Invite;
use lanpilot_core::proto::v1::{
    ClientMessage, GestureState, Os, PointerDatagram, RunCommand, ServerMessage, client_message,
    server_message,
};
use lanpilot_core::quinn;
use lanpilot_core::session::{ClientSession, connect_paired, local_hello, open_session};
use lanpilot_core::transport::client_endpoint;
use prost::Message;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const DEVICE_NAME: &str = "iPhone spike";
const APP_VERSION: &str = "0.0.1";

struct Server {
    key: PublicKey,
    addrs: Vec<IpAddr>,
    port: u16,
    name: String,
}

struct Live {
    conn: quinn::Connection,
    session: ClientSession,
    next_id: u64,
    gesture: u32,
}

struct Client {
    dir: PathBuf,
    endpoint: quinn::Endpoint,
    server: Option<Server>,
    live: Option<Live>,
}

fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("tokio runtime")
    })
}

fn client() -> &'static Mutex<Option<Client>> {
    static CLIENT: OnceLock<Mutex<Option<Client>>> = OnceLock::new();
    CLIENT.get_or_init(|| Mutex::new(None))
}

/// Runs `f` on the spike runtime and awaits it from whatever executor Dart uses.
async fn on_rt<T, F>(f: F) -> anyhow::Result<T>
where
    T: Send + 'static,
    F: Future<Output = anyhow::Result<T>> + Send + 'static,
{
    runtime()
        .spawn(f)
        .await
        .map_err(|e| anyhow!("task failed: {e}"))?
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn load_server(dir: &PathBuf) -> anyhow::Result<Option<Server>> {
    let path = dir.join("server.txt");
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)?;
    let mut lines = text.lines();
    let mut next = |what: &str| {
        lines
            .next()
            .with_context(|| format!("server.txt: no {what}"))
    };
    let key = PublicKey::from_slice(&hex::decode(next("key")?)?)?;
    let port = next("port")?.parse()?;
    let addrs = next("addrs")?
        .split(',')
        .filter(|s| !s.is_empty())
        .map(str::parse)
        .collect::<Result<_, _>>()?;
    let name = next("name").unwrap_or_default().to_owned();
    Ok(Some(Server {
        key,
        addrs,
        port,
        name,
    }))
}

fn save_server(dir: &PathBuf, s: &Server) -> anyhow::Result<()> {
    let addrs: Vec<String> = s.addrs.iter().map(ToString::to_string).collect();
    let text = format!(
        "{}\n{}\n{}\n{}\n",
        hex::encode(s.key.as_bytes()),
        s.port,
        addrs.join(","),
        s.name
    );
    std::fs::write(dir.join("server.txt"), text)?;
    Ok(())
}

fn describe(s: &Option<Server>) -> String {
    match s {
        Some(s) => {
            let addrs: Vec<String> = s.addrs.iter().map(ToString::to_string).collect();
            format!(
                "paired with \"{}\" at [{}]:{} key {}",
                s.name,
                addrs.join(", "),
                s.port,
                s.key.short_id()
            )
        }
        None => "not paired".into(),
    }
}

/// Loads (or creates) the client identity and the paired server from `docs_dir`.
pub async fn init_client(docs_dir: String) -> anyhow::Result<String> {
    on_rt(async move {
        let mut guard = client().lock().await;
        if let Some(c) = guard.as_ref() {
            return Ok(describe(&c.server));
        }
        let dir = PathBuf::from(docs_dir);
        let id_path = dir.join("identity.bin");
        let identity = match std::fs::read(&id_path) {
            Ok(bytes) => {
                let secret: [u8; 32] = bytes
                    .as_slice()
                    .try_into()
                    .map_err(|_| anyhow!("identity.bin is {} bytes, not 32", bytes.len()))?;
                Identity::from_secret_bytes(&secret)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let id = Identity::generate();
                std::fs::write(&id_path, id.secret_bytes())?;
                id
            }
            Err(e) => return Err(e.into()),
        };
        let endpoint = client_endpoint(&identity).map_err(|e| anyhow!("endpoint: {e}"))?;
        let server = load_server(&dir)?;
        let status = format!(
            "client {}; {}",
            identity.public_key().short_id(),
            describe(&server)
        );
        *guard = Some(Client {
            dir,
            endpoint,
            server,
            live: None,
        });
        Ok(status)
    })
    .await
}

fn need(guard: &mut Option<Client>) -> anyhow::Result<&mut Client> {
    guard.as_mut().context("client not initialized")
}

/// Pairs with the agent from a `lanpilot://pair?d=...` URI. Returns a summary with the time taken.
pub async fn pair(uri: String) -> anyhow::Result<String> {
    on_rt(async move {
        let mut guard = client().lock().await;
        let c = need(&mut guard)?;
        let invite = Invite::from_uri(uri.trim()).map_err(|e| anyhow!("invite: {e}"))?;
        let start = Instant::now();
        let paired = pair_with_invite(&c.endpoint, &invite, DEVICE_NAME, Os::Ios)
            .await
            .map_err(|e| anyhow!("pair: {e} ({e:?})"))?;
        let took = start.elapsed();
        let server = Server {
            key: paired.public_key,
            addrs: invite.addrs.clone(),
            port: invite.port,
            name: paired.name,
        };
        save_server(&c.dir, &server)?;
        c.server = Some(server);
        c.live = None;
        Ok(format!(
            "paired in {:.1} ms; {}",
            ms(took),
            describe(&c.server)
        ))
    })
    .await
}

/// Connects to the paired agent and opens a session. Replaces any live session.
pub async fn connect() -> anyhow::Result<String> {
    on_rt(async move {
        let mut guard = client().lock().await;
        let c = need(&mut guard)?;
        if let Some(old) = c.live.take() {
            old.conn.close(0u32.into(), b"reconnect");
        }
        let server = c.server.as_ref().context("not paired")?;
        let start = Instant::now();
        let mut last = anyhow!("no address to try");
        let mut conn = None;
        for ip in &server.addrs {
            match connect_paired(&c.endpoint, SocketAddr::new(*ip, server.port), &server.key).await
            {
                Ok(x) => {
                    conn = Some(x);
                    break;
                }
                Err(e) => last = anyhow!("connect {ip}: {e} ({e:?})"),
            }
        }
        let conn = conn.ok_or(last)?;
        let connected = start.elapsed();
        let session = open_session(&conn, &local_hello(DEVICE_NAME, Os::Ios, APP_VERSION, &[]))
            .await
            .map_err(|e| anyhow!("session: {e} ({e:?})"))?;
        let total = start.elapsed();
        let report = format!(
            "connect {:.1} ms + session {:.1} ms = {:.1} ms; server \"{}\" proto v{} via {}",
            ms(connected),
            ms(total - connected),
            ms(total),
            session.server_hello.device_name,
            session.version,
            conn.remote_address()
        );
        c.live = Some(Live {
            conn,
            session,
            next_id: 0,
            gesture: 0,
        });
        Ok(report)
    })
    .await
}

fn live(c: &mut Client) -> anyhow::Result<&mut Live> {
    let reason = c
        .live
        .as_ref()
        .context("not connected")?
        .conn
        .close_reason();
    if let Some(reason) = reason {
        c.live = None;
        bail!("connection closed: {reason}");
    }
    Ok(c.live.as_mut().expect("checked above"))
}

/// Traces a 200 px square with pointer datagrams: 4 sides x 30 steps, 8 ms apart.
pub async fn draw_square() -> anyhow::Result<String> {
    on_rt(async move {
        let mut guard = client().lock().await;
        let c = need(&mut guard)?;
        let live = live(c)?;
        live.gesture += 1;
        let gesture_id = live.gesture;
        let corners = [
            (0.0, 0.0),
            (200.0, 0.0),
            (200.0, 200.0),
            (0.0, 200.0),
            (0.0, 0.0),
        ];
        let start = Instant::now();
        let mut seq = 0u32;
        for side in corners.windows(2) {
            let ((x0, y0), (x1, y1)) = (side[0], side[1]);
            for i in 1..=30u32 {
                let t = i as f32 / 30.0;
                seq += 1;
                let g = GestureState {
                    gesture_id,
                    seq,
                    total_dx: x0 + (x1 - x0) * t,
                    total_dy: y0 + (y1 - y0) * t,
                    total_scroll_x: 0.0,
                    total_scroll_y: 0.0,
                };
                live.conn
                    .send_datagram(PointerDatagram { gesture: Some(g) }.encode_to_vec().into())
                    .map_err(|e| anyhow!("datagram {seq}: {e}"))?;
                tokio::time::sleep(Duration::from_millis(8)).await;
            }
        }
        Ok(format!(
            "sent {seq} datagrams (gesture {gesture_id}) in {:.0} ms",
            ms(start.elapsed())
        ))
    })
    .await
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    let rank = ((p * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len());
    sorted[rank - 1]
}

/// Sends `rounds` RunCommand("ping") requests one at a time and reports the round trips.
pub async fn latency_test(rounds: u32) -> anyhow::Result<String> {
    on_rt(async move {
        let mut guard = client().lock().await;
        let c = need(&mut guard)?;
        let mut samples = Vec::with_capacity(rounds as usize);
        let result: anyhow::Result<()> = async {
            let live = live(c)?;
            for _ in 0..rounds {
                live.next_id += 1;
                let id = live.next_id;
                let msg = ClientMessage {
                    request_id: id,
                    body: Some(client_message::Body::RunCommand(RunCommand {
                        command_id: "ping".into(),
                    })),
                };
                let start = Instant::now();
                write_msg(&mut live.session.send, &msg)
                    .await
                    .map_err(|e| anyhow!("write {id}: {e}"))?;
                // read_msg is not cancel-safe; a timeout here desyncs the stream,
                // so the session is dropped below on any error.
                let reply: ServerMessage =
                    tokio::time::timeout(Duration::from_secs(5), read_msg(&mut live.session.recv))
                        .await
                        .map_err(|_| anyhow!("no reply to {id} within 5 s"))?
                        .map_err(|e| anyhow!("read {id}: {e}"))?
                        .ok_or_else(|| anyhow!("server closed the stream"))?;
                let rtt = start.elapsed();
                match reply.body {
                    Some(server_message::Body::Error(_) | server_message::Body::Ack(_))
                        if reply.request_id == id => {}
                    other => bail!("unexpected reply to {id}: {other:?}"),
                }
                samples.push(ms(rtt));
            }
            Ok(())
        }
        .await;
        if let Err(e) = result {
            c.live = None;
            return Err(e.context(format!("after {} rounds; session dropped", samples.len())));
        }
        samples.sort_by(f64::total_cmp);
        let mean = samples.iter().sum::<f64>() / samples.len().max(1) as f64;
        Ok(format!(
            "{} rounds: min {:.2} / median {:.2} / P95 {:.2} / max {:.2} ms (mean {:.2})",
            samples.len(),
            samples[0],
            percentile(&samples, 0.5),
            percentile(&samples, 0.95),
            samples[samples.len() - 1],
            mean
        ))
    })
    .await
}

/// Sends one UDP packet to `addr` ("ip:port"), which makes iOS show the local
/// network prompt (or fail if access was denied). The agent ignores it.
pub async fn probe_lan(addr: String) -> anyhow::Result<String> {
    on_rt(async move {
        let target: SocketAddr = addr
            .parse()
            .with_context(|| format!("bad address {addr}"))?;
        let sock = tokio::net::UdpSocket::bind("0.0.0.0:0").await?;
        let start = Instant::now();
        let sent = sock
            .send_to(b"lanpilot-probe", target)
            .await
            .map_err(|e| anyhow!("send to {target}: {e} ({e:?})"))?;
        Ok(format!(
            "sent {sent} bytes to {target} in {:.1} ms",
            ms(start.elapsed())
        ))
    })
    .await
}

/// Closes the live session, if any.
pub async fn disconnect() -> anyhow::Result<String> {
    on_rt(async move {
        let mut guard = client().lock().await;
        let c = need(&mut guard)?;
        match c.live.take() {
            Some(live) => {
                live.conn.close(0u32.into(), b"bye");
                Ok("disconnected".into())
            }
            None => Ok("was not connected".into()),
        }
    })
    .await
}

#[flutter_rust_bridge::frb(init)]
pub fn init_app() {
    flutter_rust_bridge::setup_default_user_utils();
}
