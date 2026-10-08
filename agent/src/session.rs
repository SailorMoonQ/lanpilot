//! One authorized session: control stream, pointer datagrams and cleanup.

use crate::devices::DeviceStore;
use lanpilot_core::framing::{read_msg, write_msg};
use lanpilot_core::identity::PublicKey;
use lanpilot_core::pointer::{PointerApplier, PointerDelta};
use lanpilot_core::proto::v1::{
    Ack, ClientMessage, Error as ProtoError, ErrorCode, GestureState, KeyChord, Media, MediaAction,
    MouseButton, PointerButton, PointerDatagram, RunCommand, ServerMessage, Text, Unpair,
    client_message, server_message,
};
use lanpilot_core::quinn;
use lanpilot_core::transport::finish_and_confirm;
use lanpilot_input::{HidUsage, InputBackend, InputError, MediaKey, MouseButton as InMouse};
use prost::Message;
use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};
use tokio::sync::{broadcast, mpsc};

pub type SharedInput = Arc<Mutex<Box<dyn InputBackend>>>;

pub const CLOSE_DEVICE_REMOVED: u32 = 3;
pub const CLOSE_UNPAIRED: u32 = 4;
pub const MAX_CHORD_KEYS: usize = 8;

pub struct SessionState {
    applier: PointerApplier,
    held: HashSet<InMouse>,
}

impl SessionState {
    pub fn new() -> Self {
        Self {
            applier: PointerApplier::new(),
            held: HashSet::new(),
        }
    }
}

impl Default for SessionState {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Handled {
    pub reply: Option<ServerMessage>,
    pub unpair: bool,
}

type Failure = (ErrorCode, String);

fn from_input(e: InputError) -> Failure {
    let code = match e {
        InputError::UnknownKey(_) => ErrorCode::BadRequest,
        InputError::Unsupported(_) => ErrorCode::Unsupported,
        InputError::Os(_) => ErrorCode::Internal,
    };
    (code, e.to_string())
}

fn bad(msg: &str) -> Failure {
    (ErrorCode::BadRequest, msg.to_owned())
}

fn apply_delta(input: &mut dyn InputBackend, d: PointerDelta) {
    if (d.dx != 0 || d.dy != 0)
        && let Err(e) = input.move_relative(d.dx, d.dy)
    {
        tracing::debug!("pointer move failed: {e}");
    }
    if (d.scroll_x != 0.0 || d.scroll_y != 0.0)
        && let Err(e) = input.scroll(d.scroll_x, d.scroll_y)
    {
        tracing::debug!("scroll failed: {e}");
    }
}

fn apply_gesture(input: &mut dyn InputBackend, state: &mut SessionState, g: &GestureState) {
    if let Some(d) = state.applier.apply(g) {
        apply_delta(input, d);
    }
}

fn button(
    input: &mut dyn InputBackend,
    state: &mut SessionState,
    b: PointerButton,
) -> Result<(), Failure> {
    if let Some(g) = &b.gesture {
        apply_gesture(input, state, g);
    }
    let which = match MouseButton::try_from(b.button) {
        Ok(MouseButton::Left) => InMouse::Left,
        Ok(MouseButton::Right) => InMouse::Right,
        Ok(MouseButton::Middle) => InMouse::Middle,
        _ => return Err(bad("unknown mouse button")),
    };
    input.button(which, b.down).map_err(from_input)?;
    if b.down {
        state.held.insert(which);
    } else {
        state.held.remove(&which);
    }
    Ok(())
}

fn chord(input: &mut dyn InputBackend, c: KeyChord) -> Result<(), Failure> {
    if c.usages.is_empty() || c.usages.len() > MAX_CHORD_KEYS {
        return Err(bad("a key chord needs 1 to 8 keys"));
    }
    let mut pressed: Vec<HidUsage> = Vec::with_capacity(c.usages.len());
    for u in c.usages.iter().map(|u| HidUsage(*u)) {
        if let Err(e) = input.key(u, true) {
            for p in pressed.iter().rev() {
                let _ = input.key(*p, false);
            }
            return Err(from_input(e));
        }
        pressed.push(u);
    }
    for p in pressed.iter().rev() {
        input.key(*p, false).map_err(from_input)?;
    }
    Ok(())
}

fn media(input: &mut dyn InputBackend, m: Media) -> Result<(), Failure> {
    let key = match MediaAction::try_from(m.action) {
        Ok(MediaAction::PlayPause) => MediaKey::PlayPause,
        Ok(MediaAction::Next) => MediaKey::Next,
        Ok(MediaAction::Previous) => MediaKey::Previous,
        Ok(MediaAction::VolumeUp) => MediaKey::VolumeUp,
        Ok(MediaAction::VolumeDown) => MediaKey::VolumeDown,
        Ok(MediaAction::Mute) => MediaKey::Mute,
        _ => return Err(bad("unknown media action")),
    };
    input.media(key).map_err(from_input)
}

fn text(input: &mut dyn InputBackend, t: Text) -> Result<(), Failure> {
    if !input.supports_text() {
        return Err((
            ErrorCode::Unsupported,
            "text input is not supported on this computer".into(),
        ));
    }
    input.text(&t.text).map_err(from_input)
}

pub fn handle_client_message(
    msg: ClientMessage,
    input: &mut dyn InputBackend,
    state: &mut SessionState,
) -> Handled {
    let mut unpair = false;
    let result: Result<(), Failure> = match msg.body {
        Some(client_message::Body::PointerButton(b)) => button(input, state, b),
        Some(client_message::Body::KeyChord(c)) => chord(input, c),
        Some(client_message::Body::Text(t)) => text(input, t),
        Some(client_message::Body::Media(m)) => media(input, m),
        Some(client_message::Body::RunCommand(RunCommand { .. })) => Err((
            ErrorCode::Unsupported,
            "shortcut commands are not available yet".into(),
        )),
        Some(client_message::Body::Unpair(Unpair {})) => {
            unpair = true;
            Ok(())
        }
        None => Err(bad("empty message")),
    };
    let reply = (msg.request_id != 0).then_some(ServerMessage {
        request_id: msg.request_id,
        body: Some(match result {
            Ok(()) => server_message::Body::Ack(Ack {}),
            Err((code, message)) => server_message::Body::Error(ProtoError {
                code: code as i32,
                message,
            }),
        }),
    });
    Handled { reply, unpair }
}

pub fn apply_datagram(bytes: &[u8], input: &mut dyn InputBackend, state: &mut SessionState) {
    if let Ok(PointerDatagram { gesture: Some(g) }) = PointerDatagram::decode(bytes) {
        apply_gesture(input, state, &g);
    }
}

pub fn release_held(input: &mut dyn InputBackend, state: &mut SessionState) {
    for b in state.held.drain() {
        if let Err(e) = input.button(b, false) {
            tracing::warn!("releasing {b:?} failed: {e}");
        }
    }
}

/// Runs one authorized session until the connection ends, the client stops,
/// or the device is unpaired or removed. Always releases held buttons.
pub async fn run_session(
    conn: quinn::Connection,
    peer: PublicKey,
    mut send: quinn::SendStream,
    mut recv: quinn::RecvStream,
    input: SharedInput,
    store: Arc<DeviceStore>,
) {
    // Subscribe first, then check membership, so a removal can never slip
    // between authorization and the watcher.
    let mut removed = store.subscribe_removed();
    if !store.contains(&peer) {
        conn.close(CLOSE_DEVICE_REMOVED.into(), b"device removed");
        return;
    }

    let state = Arc::new(Mutex::new(SessionState::new()));
    let (tx, mut rx) = mpsc::channel::<ServerMessage>(64);

    // Writer: owns the send stream; finishes it once every reply is out.
    let writer = tokio::spawn(async move {
        while let Some(m) = rx.recv().await {
            if write_msg(&mut send, &m).await.is_err() {
                return;
            }
        }
        finish_and_confirm(&mut send).await;
    });

    // Reader: owns the receive stream; never cancelled mid-frame.
    let mut reader = {
        let (state, input) = (state.clone(), input.clone());
        tokio::spawn(async move {
            loop {
                let msg = match read_msg::<ClientMessage, _>(&mut recv).await {
                    Ok(Some(m)) => m,
                    Ok(None) => return false,
                    Err(e) => {
                        tracing::debug!("control stream read failed: {e}");
                        return false;
                    }
                };
                let handled = {
                    let mut input = input.lock().unwrap_or_else(PoisonError::into_inner);
                    let mut state = state.lock().unwrap_or_else(PoisonError::into_inner);
                    handle_client_message(msg, input.as_mut(), &mut state)
                };
                if let Some(reply) = handled.reply
                    && tx.send(reply).await.is_err()
                {
                    return false;
                }
                if handled.unpair {
                    return true; // dropping `tx` lets the writer finish
                }
            }
        })
    };

    let mut datagrams = {
        let (state, input, conn) = (state.clone(), input.clone(), conn.clone());
        tokio::spawn(async move {
            while let Ok(bytes) = conn.read_datagram().await {
                let mut input = input.lock().unwrap_or_else(PoisonError::into_inner);
                let mut state = state.lock().unwrap_or_else(PoisonError::into_inner);
                apply_datagram(&bytes, input.as_mut(), &mut state);
            }
        })
    };

    let mut removal = {
        let (conn, store) = (conn.clone(), store.clone());
        tokio::spawn(async move {
            loop {
                match removed.recv().await {
                    Ok(key) if key == peer => {}
                    Ok(_) => continue,
                    Err(broadcast::error::RecvError::Lagged(_)) if store.contains(&peer) => {
                        continue;
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => return,
                }
                conn.close(CLOSE_DEVICE_REMOVED.into(), b"device removed");
                return;
            }
        })
    };

    // Biased: a reader result (notably an acknowledged Unpair) wins over a
    // simultaneous connection close.
    let (reader_result, closed_first) = tokio::select! {
        biased;
        r = &mut reader => (Some(r.unwrap_or(false)), false),
        _ = conn.closed() => {
            if reader.is_finished() {
                (Some((&mut reader).await.unwrap_or(false)), true)
            } else {
                (None, true)
            }
        }
    };
    let unpaired = reader_result == Some(true);

    // No more pointer input and no watcher racing the close code below.
    datagrams.abort();
    removal.abort();
    let _ = (&mut datagrams).await;
    let _ = (&mut removal).await;

    if unpaired {
        let _ = writer.await; // the Ack is delivered before the device disappears
        if let Err(e) = store.remove(&peer) {
            tracing::error!("cannot remove unpaired device: {e}");
        }
        conn.close(CLOSE_UNPAIRED.into(), b"unpaired");
    } else if closed_first {
        reader.abort();
        writer.abort();
        let _ = (&mut reader).await;
        let _ = writer.await;
    } else {
        let _ = writer.await; // drains replies and finishes the stream
        conn.close(0u32.into(), b"session ended");
    }

    // Every task has terminated; nothing can press after this release.
    let mut input = input.lock().unwrap_or_else(PoisonError::into_inner);
    let mut state = state.lock().unwrap_or_else(PoisonError::into_inner);
    release_held(input.as_mut(), &mut state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use lanpilot_input::recording::{Recorded, RecordingBackend};

    fn msg(id: u64, body: client_message::Body) -> ClientMessage {
        ClientMessage {
            request_id: id,
            body: Some(body),
        }
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

    fn error_code(h: &Handled) -> Option<ErrorCode> {
        match h.reply.as_ref()?.body.as_ref()? {
            server_message::Body::Error(e) => ErrorCode::try_from(e.code).ok(),
            _ => None,
        }
    }

    fn is_ack(h: &Handled, id: u64) -> bool {
        matches!(h.reply.as_ref(), Some(ServerMessage { request_id, body: Some(server_message::Body::Ack(_)) }) if *request_id == id)
    }

    #[test]
    fn button_with_gesture_catches_up_then_clicks() {
        let (mut b, h) = RecordingBackend::new(true);
        let mut s = SessionState::new();
        apply_datagram(
            &PointerDatagram {
                gesture: Some(gs(1, 1, 5.0, 0.0)),
            }
            .encode_to_vec(),
            &mut b,
            &mut s,
        );
        let out = handle_client_message(
            msg(
                7,
                client_message::Body::PointerButton(PointerButton {
                    button: MouseButton::Left as i32,
                    down: true,
                    gesture: Some(gs(1, 2, 9.0, 1.0)),
                }),
            ),
            &mut b,
            &mut s,
        );
        assert!(is_ack(&out, 7));
        assert_eq!(
            h.events(),
            vec![
                Recorded::Move(5, 0),
                Recorded::Move(4, 1),
                Recorded::Button(InMouse::Left, true)
            ]
        );
    }

    #[test]
    fn no_reply_for_request_id_zero() {
        let (mut b, _h) = RecordingBackend::new(true);
        let mut s = SessionState::new();
        let out = handle_client_message(
            msg(
                0,
                client_message::Body::Media(Media {
                    action: MediaAction::Mute as i32,
                }),
            ),
            &mut b,
            &mut s,
        );
        assert!(out.reply.is_none());
    }

    #[test]
    fn chord_presses_in_order_and_releases_in_reverse() {
        let (mut b, h) = RecordingBackend::new(true);
        let mut s = SessionState::new();
        let out = handle_client_message(
            msg(
                1,
                client_message::Body::KeyChord(KeyChord {
                    usages: vec![0xE0, 0xE1, 0x29],
                }),
            ),
            &mut b,
            &mut s,
        );
        assert!(is_ack(&out, 1));
        assert_eq!(
            h.events(),
            vec![
                Recorded::Key(HidUsage(0xE0), true),
                Recorded::Key(HidUsage(0xE1), true),
                Recorded::Key(HidUsage(0x29), true),
                Recorded::Key(HidUsage(0x29), false),
                Recorded::Key(HidUsage(0xE1), false),
                Recorded::Key(HidUsage(0xE0), false),
            ]
        );
    }

    #[test]
    fn chord_limits() {
        let (mut b, _h) = RecordingBackend::new(true);
        let mut s = SessionState::new();
        let empty = handle_client_message(
            msg(
                1,
                client_message::Body::KeyChord(KeyChord { usages: vec![] }),
            ),
            &mut b,
            &mut s,
        );
        assert_eq!(error_code(&empty), Some(ErrorCode::BadRequest));
        let long = handle_client_message(
            msg(
                2,
                client_message::Body::KeyChord(KeyChord {
                    usages: vec![0x04; MAX_CHORD_KEYS + 1],
                }),
            ),
            &mut b,
            &mut s,
        );
        assert_eq!(error_code(&long), Some(ErrorCode::BadRequest));
    }

    #[test]
    fn text_unsupported_and_run_command_unsupported() {
        let (mut b, _h) = RecordingBackend::new(false);
        let mut s = SessionState::new();
        let t = handle_client_message(
            msg(1, client_message::Body::Text(Text { text: "hi".into() })),
            &mut b,
            &mut s,
        );
        assert_eq!(error_code(&t), Some(ErrorCode::Unsupported));
        let r = handle_client_message(
            msg(
                2,
                client_message::Body::RunCommand(RunCommand {
                    command_id: "x".into(),
                }),
            ),
            &mut b,
            &mut s,
        );
        assert_eq!(error_code(&r), Some(ErrorCode::Unsupported));
    }

    #[test]
    fn text_and_media_reach_the_backend() {
        let (mut b, h) = RecordingBackend::new(true);
        let mut s = SessionState::new();
        handle_client_message(
            msg(
                1,
                client_message::Body::Text(Text {
                    text: "你好".into(),
                }),
            ),
            &mut b,
            &mut s,
        );
        handle_client_message(
            msg(
                2,
                client_message::Body::Media(Media {
                    action: MediaAction::Next as i32,
                }),
            ),
            &mut b,
            &mut s,
        );
        assert_eq!(
            h.events(),
            vec![
                Recorded::Text("你好".into()),
                Recorded::Media(MediaKey::Next)
            ]
        );
    }

    #[test]
    fn bad_requests() {
        let (mut b, _h) = RecordingBackend::new(true);
        let mut s = SessionState::new();
        let none = handle_client_message(
            ClientMessage {
                request_id: 1,
                body: None,
            },
            &mut b,
            &mut s,
        );
        assert_eq!(error_code(&none), Some(ErrorCode::BadRequest));
        let media = handle_client_message(
            msg(
                2,
                client_message::Body::Media(Media {
                    action: MediaAction::Unspecified as i32,
                }),
            ),
            &mut b,
            &mut s,
        );
        assert_eq!(error_code(&media), Some(ErrorCode::BadRequest));
        let button = handle_client_message(
            msg(
                3,
                client_message::Body::PointerButton(PointerButton {
                    button: 0,
                    down: true,
                    gesture: None,
                }),
            ),
            &mut b,
            &mut s,
        );
        assert_eq!(error_code(&button), Some(ErrorCode::BadRequest));
    }

    #[test]
    fn unpair_is_acked_and_flagged() {
        let (mut b, _h) = RecordingBackend::new(true);
        let mut s = SessionState::new();
        let out = handle_client_message(
            msg(9, client_message::Body::Unpair(Unpair {})),
            &mut b,
            &mut s,
        );
        assert!(is_ack(&out, 9));
        assert!(out.unpair);
    }

    #[test]
    fn held_buttons_are_released() {
        let (mut b, h) = RecordingBackend::new(true);
        let mut s = SessionState::new();
        for button in [MouseButton::Left, MouseButton::Right] {
            handle_client_message(
                msg(
                    0,
                    client_message::Body::PointerButton(PointerButton {
                        button: button as i32,
                        down: true,
                        gesture: None,
                    }),
                ),
                &mut b,
                &mut s,
            );
        }
        handle_client_message(
            msg(
                0,
                client_message::Body::PointerButton(PointerButton {
                    button: MouseButton::Right as i32,
                    down: false,
                    gesture: None,
                }),
            ),
            &mut b,
            &mut s,
        );
        release_held(&mut b, &mut s);
        let events = h.events();
        assert_eq!(events.last(), Some(&Recorded::Button(InMouse::Left, false)));
        assert_eq!(
            events
                .iter()
                .filter(|e| **e == Recorded::Button(InMouse::Left, false))
                .count(),
            1
        );
        release_held(&mut b, &mut s);
        assert_eq!(h.events().len(), events.len(), "second release is a no-op");
    }

    #[test]
    fn garbage_datagram_is_ignored() {
        let (mut b, h) = RecordingBackend::new(true);
        let mut s = SessionState::new();
        apply_datagram(&[0xff, 0xff, 0xff], &mut b, &mut s);
        assert!(h.events().is_empty());
    }

    #[tokio::test]
    async fn session_for_removed_device_closes_immediately() {
        use lanpilot_core::identity::Identity;
        use lanpilot_core::transport::{client_endpoint, connect, server_endpoint};

        let dir = tempfile::tempdir().unwrap();
        let store = DeviceStore::open(dir.path().join("devices.json")).unwrap();
        let server_id = Identity::generate();
        let client_id = Identity::generate();
        // The client key is never added, as if it was removed after authorization.
        let server = server_endpoint(&server_id, ([127, 0, 0, 1], 0).into()).unwrap();
        let addr = server.local_addr().unwrap();
        let client = client_endpoint(&client_id).unwrap();

        let client_task = tokio::spawn(async move {
            let conn = connect(&client, addr).await.unwrap();
            let (mut send, _recv) = conn.open_bi().await.unwrap();
            send.write_all(&[0]).await.unwrap();
            let err = conn.closed().await;
            (client, err)
        });

        let conn = server.accept().await.unwrap().await.unwrap();
        let (send, recv) = conn.accept_bi().await.unwrap();
        let (backend, handle) = RecordingBackend::new(true);
        let input: SharedInput = Arc::new(Mutex::new(Box::new(backend)));
        run_session(conn, client_id.public_key(), send, recv, input, store).await;

        let (_client, err) = client_task.await.unwrap();
        match err {
            quinn::ConnectionError::ApplicationClosed(c) => {
                assert_eq!(u64::from(c.error_code), u64::from(CLOSE_DEVICE_REMOVED));
            }
            other => panic!("unexpected close: {other:?}"),
        }
        assert!(handle.events().is_empty());
    }
}
