//! lpctl: a development client for the LanPilot agent.

pub mod store;

use lanpilot_core::framing::{read_msg, write_msg};
use lanpilot_core::pairing::flow::{PairedServer, pair_with_invite, pair_with_password};
use lanpilot_core::pairing::invite::Invite;
use lanpilot_core::proto::v1::{
    ClientMessage, GestureState, KeyChord, Media, MediaAction, MouseButton, Os, PointerButton,
    PointerDatagram, ServerMessage, Text, Unpair, client_message, server_message,
};
use lanpilot_core::quinn;
use lanpilot_core::session::{ClientSession, connect_paired, local_hello, open_session};
use lanpilot_core::transport::client_endpoint;
use prost::Message;
use std::net::SocketAddr;
use std::time::Duration;
use store::{ClientStore, KnownServer};

#[derive(Debug, thiserror::Error)]
pub enum LpctlError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Usage(String),
}

fn core<E: std::fmt::Display>(e: E) -> LpctlError {
    LpctlError::Usage(e.to_string())
}

pub enum Action {
    Move { dx: f32, dy: f32, steps: u32 },
    Click(MouseButton),
    Scroll(f32),
    Keys(Vec<u32>),
    Media(MediaAction),
    Text(String),
    Unpair,
    Square,
}

pub fn parse_media(name: &str) -> Option<MediaAction> {
    Some(match name {
        "play-pause" | "play" | "pause" => MediaAction::PlayPause,
        "next" => MediaAction::Next,
        "prev" | "previous" => MediaAction::Previous,
        "vol-up" => MediaAction::VolumeUp,
        "vol-down" => MediaAction::VolumeDown,
        "mute" => MediaAction::Mute,
        _ => return None,
    })
}

pub fn parse_button(name: &str) -> Option<MouseButton> {
    Some(match name {
        "left" => MouseButton::Left,
        "right" => MouseButton::Right,
        "middle" => MouseButton::Middle,
        _ => return None,
    })
}

pub fn move_steps(dx: f32, dy: f32, steps: u32) -> Vec<(f32, f32)> {
    let n = steps.max(1);
    (1..=n)
        .map(|i| {
            if i == n {
                (dx, dy)
            } else {
                (dx * i as f32 / n as f32, dy * i as f32 / n as f32)
            }
        })
        .collect()
}

fn known(p: PairedServer, addrs: Vec<std::net::IpAddr>, port: u16) -> KnownServer {
    KnownServer {
        public_key: p.public_key,
        name: p.name,
        addrs,
        port,
    }
}

pub async fn pair_uri(
    store: &mut ClientStore,
    uri: &str,
    device_name: &str,
) -> Result<KnownServer, LpctlError> {
    let invite = Invite::from_uri(uri.trim()).map_err(core)?;
    let endpoint = client_endpoint(store.identity()).map_err(core)?;
    let paired = pair_with_invite(&endpoint, &invite, device_name, Os::Unspecified)
        .await
        .map_err(core)?;
    let server = known(paired, invite.addrs.clone(), invite.port);
    store.upsert(server.clone())?;
    Ok(server)
}

pub async fn pair_password(
    store: &mut ClientStore,
    addr: SocketAddr,
    password: &str,
    device_name: &str,
) -> Result<KnownServer, LpctlError> {
    let endpoint = client_endpoint(store.identity()).map_err(core)?;
    let paired = pair_with_password(
        &endpoint,
        addr,
        store.identity(),
        password,
        device_name,
        Os::Unspecified,
    )
    .await
    .map_err(core)?;
    let server = known(paired, vec![addr.ip()], addr.port());
    store.upsert(server.clone())?;
    Ok(server)
}

/// Accepts only an Ack or Error answering request `id`.
fn check_reply(reply: ServerMessage, id: u64) -> Result<(), LpctlError> {
    match reply.body {
        Some(server_message::Body::Ack(_)) if reply.request_id == id => Ok(()),
        Some(server_message::Body::Error(e)) if reply.request_id == id => {
            Err(LpctlError::Usage(e.message))
        }
        _ => Err(LpctlError::Usage("unexpected reply".into())),
    }
}

struct Live {
    conn: quinn::Connection,
    session: ClientSession,
    next_id: u64,
    gesture: u32,
}

impl Live {
    async fn request(&mut self, body: client_message::Body) -> Result<(), LpctlError> {
        self.next_id += 1;
        let id = self.next_id;
        write_msg(
            &mut self.session.send,
            &ClientMessage {
                request_id: id,
                body: Some(body),
            },
        )
        .await
        .map_err(core)?;
        let reply: ServerMessage =
            tokio::time::timeout(Duration::from_secs(5), read_msg(&mut self.session.recv))
                .await
                .map_err(|_| LpctlError::Usage("no reply within 5 s".into()))?
                .map_err(core)?
                .ok_or_else(|| LpctlError::Usage("server closed the stream".into()))?;
        check_reply(reply, id)
    }

    async fn motion(&mut self, totals: &[(f32, f32)], scroll: bool) -> Result<(), LpctlError> {
        self.gesture += 1;
        for (seq, (x, y)) in totals.iter().enumerate() {
            let g = GestureState {
                gesture_id: self.gesture,
                seq: seq as u32 + 1,
                total_dx: if scroll { 0.0 } else { *x },
                total_dy: if scroll { 0.0 } else { *y },
                total_scroll_x: 0.0,
                total_scroll_y: if scroll { *y } else { 0.0 },
            };
            self.conn
                .send_datagram(PointerDatagram { gesture: Some(g) }.encode_to_vec().into())
                .map_err(core)?;
            tokio::time::sleep(Duration::from_millis(8)).await;
        }
        Ok(())
    }
}

pub async fn perform(
    store: &mut ClientStore,
    server: &KnownServer,
    addr_override: Option<SocketAddr>,
    action: Action,
) -> Result<(), LpctlError> {
    let endpoint = client_endpoint(store.identity()).map_err(core)?;
    let candidates: Vec<SocketAddr> = addr_override
        .into_iter()
        .chain(
            server
                .addrs
                .iter()
                .map(|ip| SocketAddr::new(*ip, server.port)),
        )
        .collect();
    let mut last = LpctlError::Usage("no address to try".into());
    let mut conn = None;
    for addr in candidates {
        match connect_paired(&endpoint, addr, &server.public_key).await {
            Ok(c) => {
                conn = Some(c);
                break;
            }
            Err(e) => last = core(e),
        }
    }
    let conn = conn.ok_or(last)?;
    let session = open_session(
        &conn,
        &local_hello("lpctl", Os::Unspecified, env!("CARGO_PKG_VERSION"), &[]),
    )
    .await
    .map_err(core)?;
    let mut live = Live {
        conn: conn.clone(),
        session,
        next_id: 0,
        gesture: 0,
    };

    // Errors inside this block must not skip the close below.
    let result = async {
        match action {
            Action::Move { dx, dy, steps } => live.motion(&move_steps(dx, dy, steps), false).await,
            Action::Square => {
                for (dx, dy) in [(200.0, 0.0), (0.0, 200.0), (-200.0, 0.0), (0.0, -200.0)] {
                    live.motion(&move_steps(dx, dy, 30), false).await?;
                }
                Ok(())
            }
            Action::Scroll(notches) => live.motion(&move_steps(0.0, notches, 10), true).await,
            Action::Click(button) => {
                for down in [true, false] {
                    live.request(client_message::Body::PointerButton(PointerButton {
                        button: button as i32,
                        down,
                        gesture: None,
                    }))
                    .await?;
                }
                Ok(())
            }
            Action::Keys(usages) => {
                live.request(client_message::Body::KeyChord(KeyChord { usages }))
                    .await
            }
            Action::Media(action) => {
                live.request(client_message::Body::Media(Media {
                    action: action as i32,
                }))
                .await
            }
            Action::Text(text) => {
                live.request(client_message::Body::Text(Text { text }))
                    .await
            }
            Action::Unpair => {
                live.request(client_message::Body::Unpair(Unpair {}))
                    .await?;
                store.remove(&server.public_key)
            }
        }
    }
    .await;
    // Let the last datagrams leave before closing.
    tokio::time::sleep(Duration::from_millis(50)).await;
    conn.close(0u32.into(), b"done");
    endpoint.wait_idle().await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_steps_end_exactly_at_target() {
        let s = move_steps(100.0, -30.0, 7);
        assert_eq!(s.len(), 7);
        assert_eq!(*s.last().unwrap(), (100.0, -30.0));
        assert!(s.windows(2).all(|w| w[1].0 >= w[0].0));
        assert_eq!(
            move_steps(5.0, 5.0, 0),
            vec![(5.0, 5.0)],
            "zero steps still moves"
        );
    }

    #[test]
    fn replies_must_answer_the_request() {
        use lanpilot_core::proto::v1::{Ack, Error as ProtoError};
        let ack = |request_id| ServerMessage {
            request_id,
            body: Some(server_message::Body::Ack(Ack {})),
        };
        let error = |request_id| ServerMessage {
            request_id,
            body: Some(server_message::Body::Error(ProtoError {
                code: 1,
                message: "boom".into(),
            })),
        };
        let msg = |r: Result<(), LpctlError>| r.unwrap_err().to_string();
        assert!(check_reply(ack(3), 3).is_ok());
        assert_eq!(msg(check_reply(error(3), 3)), "boom");
        assert_eq!(msg(check_reply(ack(2), 3)), "unexpected reply");
        assert_eq!(msg(check_reply(error(2), 3)), "unexpected reply");
        assert_eq!(msg(check_reply(error(0), 3)), "unexpected reply");
    }

    #[test]
    fn parses_names() {
        assert_eq!(parse_media("play-pause"), Some(MediaAction::PlayPause));
        assert_eq!(parse_media("vol-down"), Some(MediaAction::VolumeDown));
        assert_eq!(parse_media("x"), None);
        assert_eq!(parse_button("right"), Some(MouseButton::Right));
        assert_eq!(parse_button("x"), None);
    }
}
