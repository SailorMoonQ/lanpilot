//! One authorized session: control stream, pointer datagrams and cleanup.

use crate::devices::DeviceStore;
use lanpilot_core::framing::{read_msg, write_msg};
use lanpilot_core::identity::PublicKey;
use lanpilot_core::pointer::{PointerApplier, PointerDelta};
use lanpilot_core::proto::v1::{
    Ack, ClientMessage, Error as ProtoError, ErrorCode, GestureState, KeyChord, Media, MediaAction,
    MouseButton, PointerButton, PointerDatagram, RunCommand, ServerMessage, Text, Unpair, Zoom,
    client_message, server_message,
};
use lanpilot_core::quinn;
use lanpilot_core::transport::{DELIVERY_TIMEOUT, finish_and_confirm};
use lanpilot_input::{
    HidUsage, InputBackend, InputError, MAX_SCROLL_NOTCHES_PER_CALL, MediaKey,
    MouseButton as InMouse,
};
use prost::Message;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, PoisonError};
use tokio::sync::{broadcast, mpsc};

/// The machine's one input backend, shared by every session.
pub type SharedInput = Arc<Mutex<InputHub>>;

/// Wraps the backend so sessions share it safely: a mouse button held by
/// several phones goes up only when the last of them releases it (spec 3.7).
pub struct InputHub {
    backend: Box<dyn InputBackend>,
    button_holds: HashMap<InMouse, u32>,
}

impl InputHub {
    pub fn new(backend: Box<dyn InputBackend>) -> Self {
        Self {
            backend,
            button_holds: HashMap::new(),
        }
    }

    pub fn shared(backend: Box<dyn InputBackend>) -> SharedInput {
        Arc::new(Mutex::new(Self::new(backend)))
    }

    /// Adds one session's hold on `button`; only the first hold presses it.
    pub fn press(&mut self, button: InMouse) -> Result<(), InputError> {
        let holds = self.button_holds.entry(button).or_insert(0);
        if *holds == 0 {
            self.backend.button(button, true)?;
        }
        *holds += 1;
        Ok(())
    }

    /// Drops one session's hold on `button`; the last hold releases it.
    /// Callers only release what their session holds (`SessionState::held`).
    pub fn release(&mut self, button: InMouse) -> Result<(), InputError> {
        let Some(holds) = self.button_holds.get_mut(&button) else {
            return Ok(());
        };
        *holds -= 1;
        if *holds > 0 {
            return Ok(());
        }
        self.button_holds.remove(&button);
        self.backend.button(button, false)
    }

    pub fn move_relative(&mut self, dx: i32, dy: i32) -> Result<(), InputError> {
        self.backend.move_relative(dx, dy)
    }

    pub fn scroll(&mut self, dx: f32, dy: f32) -> Result<(), InputError> {
        self.backend.scroll(dx, dy)
    }

    pub fn key(&mut self, usage: HidUsage, down: bool) -> Result<(), InputError> {
        self.backend.key(usage, down)
    }

    pub fn media(&mut self, key: MediaKey) -> Result<(), InputError> {
        self.backend.media(key)
    }

    pub fn supports_text(&self) -> bool {
        self.backend.supports_text()
    }

    pub fn text(&mut self, text: &str) -> Result<(), InputError> {
        self.backend.text(text)
    }
}

pub const CLOSE_DEVICE_REMOVED: u32 = 3;
pub const CLOSE_UNPAIRED: u32 = 4;
pub const MAX_CHORD_KEYS: usize = 8;
pub const MAX_TEXT_CHARS: usize = 4096;

pub struct SessionState {
    applier: PointerApplier,
    held: HashSet<InMouse>,
    /// Keys whose release failed; retried when the session ends.
    held_keys: HashSet<HidUsage>,
}

impl SessionState {
    pub fn new() -> Self {
        Self {
            applier: PointerApplier::new(),
            held: HashSet::new(),
            held_keys: HashSet::new(),
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

fn apply_delta(input: &mut InputHub, d: PointerDelta) {
    if (d.dx != 0 || d.dy != 0)
        && let Err(e) = input.move_relative(d.dx, d.dy)
    {
        tracing::debug!("pointer move failed: {e}");
    }
    // Bound every backend, not only the ones that clamp internally.
    let limit = MAX_SCROLL_NOTCHES_PER_CALL;
    let (sx, sy) = (
        d.scroll_x.clamp(-limit, limit),
        d.scroll_y.clamp(-limit, limit),
    );
    if (sx != 0.0 || sy != 0.0)
        && let Err(e) = input.scroll(sx, sy)
    {
        tracing::debug!("scroll failed: {e}");
    }
}

const ZOOM_MODIFIER: HidUsage = HidUsage(0xE0); // left Ctrl

fn zoom(input: &mut InputHub, state: &mut SessionState, z: Zoom) -> Result<(), Failure> {
    if !z.steps.is_finite() {
        return Err(bad("zoom steps must be finite"));
    }
    if z.steps == 0.0 {
        return Ok(());
    }
    let limit = MAX_SCROLL_NOTCHES_PER_CALL;
    input.key(ZOOM_MODIFIER, true).map_err(from_input)?;
    let scrolled = input.scroll(0.0, z.steps.clamp(-limit, limit));
    let released = release_keys(input, state, &[ZOOM_MODIFIER]);
    scrolled.map_err(from_input)?;
    released.map_err(from_input)
}

fn apply_gesture(input: &mut InputHub, state: &mut SessionState, g: &GestureState) {
    if let Some(d) = state.applier.apply(g) {
        apply_delta(input, d);
    }
}

fn button(input: &mut InputHub, state: &mut SessionState, b: PointerButton) -> Result<(), Failure> {
    if let Some(g) = &b.gesture {
        apply_gesture(input, state, g);
    }
    let which = match MouseButton::try_from(b.button) {
        Ok(MouseButton::Left) => InMouse::Left,
        Ok(MouseButton::Right) => InMouse::Right,
        Ok(MouseButton::Middle) => InMouse::Middle,
        _ => return Err(bad("unknown mouse button")),
    };
    // Each session holds a button at most once; the hub counts sessions.
    if b.down {
        if !state.held.contains(&which) {
            input.press(which).map_err(from_input)?;
            state.held.insert(which);
        }
    } else if state.held.remove(&which) {
        input.release(which).map_err(from_input)?;
    }
    Ok(())
}

/// Releases `keys` in reverse order, attempting every one even if some fail.
/// Keys whose release failed are kept in `held_keys` for `release_held`.
/// Returns the first error.
fn release_keys(
    input: &mut InputHub,
    state: &mut SessionState,
    keys: &[HidUsage],
) -> Result<(), InputError> {
    let mut first = None;
    for k in keys.iter().rev() {
        if let Err(e) = input.key(*k, false) {
            state.held_keys.insert(*k);
            first.get_or_insert(e);
        }
    }
    first.map_or(Ok(()), Err)
}

fn chord(input: &mut InputHub, state: &mut SessionState, c: KeyChord) -> Result<(), Failure> {
    if c.usages.is_empty() || c.usages.len() > MAX_CHORD_KEYS {
        return Err(bad("a key chord needs 1 to 8 keys"));
    }
    let mut pressed: Vec<HidUsage> = Vec::with_capacity(c.usages.len());
    for u in c.usages.iter().map(|u| HidUsage(*u)) {
        if let Err(e) = input.key(u, true) {
            let _ = release_keys(input, state, &pressed);
            return Err(from_input(e));
        }
        pressed.push(u);
    }
    release_keys(input, state, &pressed).map_err(from_input)
}

fn media(input: &mut InputHub, m: Media) -> Result<(), Failure> {
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

fn text(input: &mut InputHub, t: Text) -> Result<(), Failure> {
    if t.text.chars().nth(MAX_TEXT_CHARS).is_some() {
        return Err((
            ErrorCode::BadRequest,
            format!("text is limited to {MAX_TEXT_CHARS} characters"),
        ));
    }
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
    input: &mut InputHub,
    state: &mut SessionState,
) -> Handled {
    let mut unpair = false;
    let result: Result<(), Failure> = match msg.body {
        Some(client_message::Body::PointerButton(b)) => button(input, state, b),
        Some(client_message::Body::KeyChord(c)) => chord(input, state, c),
        Some(client_message::Body::Text(t)) => text(input, t),
        Some(client_message::Body::Media(m)) => media(input, m),
        Some(client_message::Body::Zoom(z)) => zoom(input, state, z),
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

pub fn apply_datagram(bytes: &[u8], input: &mut InputHub, state: &mut SessionState) {
    if let Ok(PointerDatagram { gesture: Some(g) }) = PointerDatagram::decode(bytes) {
        apply_gesture(input, state, &g);
    }
}

pub fn release_held(input: &mut InputHub, state: &mut SessionState) {
    for b in state.held.drain() {
        if let Err(e) = input.release(b) {
            tracing::warn!("releasing {b:?} failed: {e}");
        }
    }
    for k in state.held_keys.drain() {
        if let Err(e) = input.key(k, false) {
            tracing::warn!("releasing key {k:?} failed: {e}");
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ExitPlan {
    /// The reader saw an Unpair.
    Unpair,
    /// The reader ended by itself (stream finished or failed).
    ReaderDone,
    /// The connection closed before the reader finished.
    ConnectionClosed,
}

fn exit_plan(reader_result: Option<bool>) -> ExitPlan {
    match reader_result {
        Some(true) => ExitPlan::Unpair,
        Some(false) => ExitPlan::ReaderDone,
        None => ExitPlan::ConnectionClosed,
    }
}

/// Waits for the writer to flush and finish, but not longer than the
/// delivery timeout: a peer that stops reading must not hold the session.
async fn drain_writer(mut writer: tokio::task::JoinHandle<()>) {
    let drained = tokio::time::timeout(DELIVERY_TIMEOUT, &mut writer).await;
    if drained.is_err() {
        writer.abort();
        let _ = writer.await;
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
                    handle_client_message(msg, &mut input, &mut state)
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
                apply_datagram(&bytes, &mut input, &mut state);
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
    // simultaneous connection close. `reader_result` is `Some` exactly when
    // the reader's output was taken, so the handle is never awaited twice.
    let reader_result: Option<bool> = tokio::select! {
        biased;
        r = &mut reader => Some(r.unwrap_or(false)),
        _ = conn.closed() => {
            if reader.is_finished() {
                Some((&mut reader).await.unwrap_or(false))
            } else {
                None
            }
        }
    };

    // No more pointer input and no watcher racing the close code below.
    datagrams.abort();
    removal.abort();
    let _ = (&mut datagrams).await;
    let _ = (&mut removal).await;

    match exit_plan(reader_result) {
        ExitPlan::Unpair => {
            drain_writer(writer).await; // the Ack is delivered before the device disappears
            if let Err(e) = store.remove(&peer) {
                tracing::error!("cannot remove unpaired device: {e}");
            }
            conn.close(CLOSE_UNPAIRED.into(), b"unpaired");
        }
        ExitPlan::ReaderDone => {
            drain_writer(writer).await; // drains replies and finishes the stream
            conn.close(0u32.into(), b"session ended");
        }
        ExitPlan::ConnectionClosed => {
            reader.abort();
            writer.abort();
            let _ = (&mut reader).await;
            let _ = writer.await;
        }
    }

    // Every task has terminated; nothing can press after this release.
    let mut input = input.lock().unwrap_or_else(PoisonError::into_inner);
    let mut state = state.lock().unwrap_or_else(PoisonError::into_inner);
    release_held(&mut input, &mut state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use lanpilot_input::recording::{Recorded, RecordingBackend, RecordingHandle};

    fn hub(supports_text: bool) -> (InputHub, RecordingHandle) {
        let (b, h) = RecordingBackend::new(supports_text);
        (InputHub::new(Box::new(b)), h)
    }

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
    fn zoom_holds_ctrl_around_the_scroll() {
        let (mut b, h) = hub(true);
        let mut s = SessionState::new();
        let out = handle_client_message(
            msg(3, client_message::Body::Zoom(Zoom { steps: 2.0 })),
            &mut b,
            &mut s,
        );
        assert!(is_ack(&out, 3));
        assert_eq!(
            h.events(),
            vec![
                Recorded::Key(HidUsage(0xE0), true),
                Recorded::Scroll(0.0, 2.0),
                Recorded::Key(HidUsage(0xE0), false),
            ]
        );
    }

    #[test]
    fn zoom_validates_steps() {
        let (mut b, h) = hub(true);
        let mut s = SessionState::new();
        let nan = handle_client_message(
            msg(1, client_message::Body::Zoom(Zoom { steps: f32::NAN })),
            &mut b,
            &mut s,
        );
        assert_eq!(error_code(&nan), Some(ErrorCode::BadRequest));
        let zero = handle_client_message(
            msg(2, client_message::Body::Zoom(Zoom { steps: 0.0 })),
            &mut b,
            &mut s,
        );
        assert!(is_ack(&zero, 2));
        assert!(h.events().is_empty(), "NaN and zero touch nothing");
        handle_client_message(
            msg(3, client_message::Body::Zoom(Zoom { steps: 1.0e9 })),
            &mut b,
            &mut s,
        );
        assert!(
            h.events()
                .contains(&Recorded::Scroll(0.0, MAX_SCROLL_NOTCHES_PER_CALL))
        );
    }

    #[test]
    fn button_with_gesture_catches_up_then_clicks() {
        let (mut b, h) = hub(true);
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
        let (mut b, _h) = hub(true);
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
        let (mut b, h) = hub(true);
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

    /// Records like `RecordingBackend` but fails to release one chosen key.
    struct FailRelease {
        inner: RecordingBackend,
        fail: HidUsage,
    }

    impl InputBackend for FailRelease {
        fn move_relative(&mut self, dx: i32, dy: i32) -> Result<(), InputError> {
            self.inner.move_relative(dx, dy)
        }
        fn button(&mut self, button: InMouse, down: bool) -> Result<(), InputError> {
            self.inner.button(button, down)
        }
        fn scroll(&mut self, dx: f32, dy: f32) -> Result<(), InputError> {
            self.inner.scroll(dx, dy)
        }
        fn key(&mut self, usage: HidUsage, down: bool) -> Result<(), InputError> {
            // Record the attempt, then fail it.
            self.inner.key(usage, down)?;
            if usage == self.fail && !down {
                return Err(InputError::Os("release failed".into()));
            }
            Ok(())
        }
        fn media(&mut self, key: MediaKey) -> Result<(), InputError> {
            self.inner.media(key)
        }
        fn supports_text(&self) -> bool {
            self.inner.supports_text()
        }
        fn text(&mut self, text: &str) -> Result<(), InputError> {
            self.inner.text(text)
        }
    }

    #[test]
    fn chord_releases_every_key_even_if_one_release_fails() {
        let (inner, h) = RecordingBackend::new(true);
        let mut b = InputHub::new(Box::new(FailRelease {
            inner,
            fail: HidUsage(0xE1),
        }));
        let mut s = SessionState::new();
        let out = handle_client_message(
            msg(
                1,
                client_message::Body::KeyChord(KeyChord {
                    usages: vec![0xE0, 0xE1, 0x04],
                }),
            ),
            &mut b,
            &mut s,
        );
        assert_eq!(error_code(&out), Some(ErrorCode::Internal));
        assert_eq!(
            h.events(),
            vec![
                Recorded::Key(HidUsage(0xE0), true),
                Recorded::Key(HidUsage(0xE1), true),
                Recorded::Key(HidUsage(0x04), true),
                Recorded::Key(HidUsage(0x04), false),
                Recorded::Key(HidUsage(0xE1), false),
                Recorded::Key(HidUsage(0xE0), false),
            ]
        );
        assert_eq!(s.held_keys, HashSet::from([HidUsage(0xE1)]));

        let before = h.events().len();
        release_held(&mut b, &mut s);
        assert_eq!(
            h.events()[before..],
            [Recorded::Key(HidUsage(0xE1), false)],
            "the failed release is retried at session end"
        );
        assert!(s.held_keys.is_empty());
    }

    #[test]
    fn chord_limits() {
        let (mut b, _h) = hub(true);
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
        let (mut b, _h) = hub(false);
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
    fn text_longer_than_the_limit_is_rejected() {
        let (mut b, h) = hub(true);
        let mut s = SessionState::new();
        let at_limit = "你".repeat(MAX_TEXT_CHARS);
        let ok = handle_client_message(
            msg(1, client_message::Body::Text(Text { text: at_limit })),
            &mut b,
            &mut s,
        );
        assert!(is_ack(&ok, 1));
        let long = handle_client_message(
            msg(
                2,
                client_message::Body::Text(Text {
                    text: "a".repeat(MAX_TEXT_CHARS + 1),
                }),
            ),
            &mut b,
            &mut s,
        );
        assert_eq!(error_code(&long), Some(ErrorCode::BadRequest));
        match long.reply.unwrap().body.unwrap() {
            server_message::Body::Error(e) => {
                assert_eq!(e.message, "text is limited to 4096 characters")
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(h.events().len(), 1, "only the text at the limit is typed");
    }

    #[test]
    fn text_and_media_reach_the_backend() {
        let (mut b, h) = hub(true);
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
        let (mut b, _h) = hub(true);
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
        let (mut b, _h) = hub(true);
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
        let (mut b, h) = hub(true);
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
    fn huge_scroll_is_clamped_before_the_backend() {
        let (mut b, h) = hub(true);
        let mut s = SessionState::new();
        let mut g = gs(1, 1, 0.0, 0.0);
        g.total_scroll_x = -5.0e6;
        g.total_scroll_y = 5.0e6;
        apply_datagram(
            &PointerDatagram { gesture: Some(g) }.encode_to_vec(),
            &mut b,
            &mut s,
        );
        assert_eq!(
            h.events(),
            vec![Recorded::Scroll(
                -MAX_SCROLL_NOTCHES_PER_CALL,
                MAX_SCROLL_NOTCHES_PER_CALL
            )]
        );
    }

    fn left(down: bool) -> ClientMessage {
        msg(
            0,
            client_message::Body::PointerButton(PointerButton {
                button: MouseButton::Left as i32,
                down,
                gesture: None,
            }),
        )
    }

    fn count(h: &RecordingHandle, e: Recorded) -> usize {
        h.events().iter().filter(|x| **x == e).count()
    }

    #[test]
    fn a_button_held_by_two_sessions_goes_up_with_the_last() {
        let (mut hub, h) = hub(true);
        let (mut a, mut b) = (SessionState::new(), SessionState::new());
        handle_client_message(left(true), &mut hub, &mut a);
        handle_client_message(left(true), &mut hub, &mut b);
        assert_eq!(count(&h, Recorded::Button(InMouse::Left, true)), 1);

        handle_client_message(left(false), &mut hub, &mut b);
        assert_eq!(count(&h, Recorded::Button(InMouse::Left, false)), 0);

        release_held(&mut hub, &mut a);
        assert_eq!(count(&h, Recorded::Button(InMouse::Left, false)), 1);
    }

    #[test]
    fn releasing_a_button_this_session_does_not_hold_is_a_no_op() {
        let (mut hub, h) = hub(true);
        let (mut a, mut b) = (SessionState::new(), SessionState::new());
        handle_client_message(left(true), &mut hub, &mut a);
        handle_client_message(left(false), &mut hub, &mut b);
        handle_client_message(left(false), &mut hub, &mut b);
        assert_eq!(count(&h, Recorded::Button(InMouse::Left, false)), 0);
        // A repeated press by the holder does not add a second hold.
        handle_client_message(left(true), &mut hub, &mut a);
        handle_client_message(left(false), &mut hub, &mut a);
        assert_eq!(count(&h, Recorded::Button(InMouse::Left, false)), 1);
    }

    #[test]
    fn garbage_datagram_is_ignored() {
        let (mut b, h) = hub(true);
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
        let input = InputHub::shared(Box::new(backend));
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

    #[test]
    fn exit_plan_covers_every_reader_state() {
        assert_eq!(exit_plan(Some(true)), ExitPlan::Unpair);
        assert_eq!(exit_plan(Some(false)), ExitPlan::ReaderDone);
        assert_eq!(exit_plan(None), ExitPlan::ConnectionClosed);
    }
}
