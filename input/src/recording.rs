//! A backend that records calls instead of injecting input. Used by tests in
//! this crate and by the agent's integration tests.

use crate::{HidUsage, InputBackend, InputError, MediaKey, MouseButton};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq)]
pub enum Recorded {
    Move(i32, i32),
    Button(MouseButton, bool),
    Scroll(f32, f32),
    Key(HidUsage, bool),
    Media(MediaKey),
    Text(String),
}

#[derive(Debug, Clone, Default)]
pub struct RecordingHandle {
    events: Arc<Mutex<Vec<Recorded>>>,
}

impl RecordingHandle {
    pub fn events(&self) -> Vec<Recorded> {
        self.events.lock().expect("recording lock poisoned").clone()
    }
}

#[derive(Debug)]
pub struct RecordingBackend {
    handle: RecordingHandle,
    supports_text: bool,
}

impl RecordingBackend {
    pub fn new(supports_text: bool) -> (Self, RecordingHandle) {
        let handle = RecordingHandle::default();
        (
            Self {
                handle: handle.clone(),
                supports_text,
            },
            handle,
        )
    }

    fn push(&self, event: Recorded) {
        self.handle
            .events
            .lock()
            .expect("recording lock poisoned")
            .push(event);
    }
}

impl InputBackend for RecordingBackend {
    fn move_relative(&mut self, dx: i32, dy: i32) -> Result<(), InputError> {
        self.push(Recorded::Move(dx, dy));
        Ok(())
    }

    fn button(&mut self, button: MouseButton, down: bool) -> Result<(), InputError> {
        self.push(Recorded::Button(button, down));
        Ok(())
    }

    fn scroll(&mut self, dx: f32, dy: f32) -> Result<(), InputError> {
        self.push(Recorded::Scroll(dx, dy));
        Ok(())
    }

    fn key(&mut self, usage: HidUsage, down: bool) -> Result<(), InputError> {
        self.push(Recorded::Key(usage, down));
        Ok(())
    }

    fn media(&mut self, key: MediaKey) -> Result<(), InputError> {
        self.push(Recorded::Media(key));
        Ok(())
    }

    fn supports_text(&self) -> bool {
        self.supports_text
    }

    fn text(&mut self, text: &str) -> Result<(), InputError> {
        if !self.supports_text {
            return Err(InputError::Unsupported("text input"));
        }
        self.push(Recorded::Text(text.to_owned()));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_calls_in_order() {
        let (mut b, handle) = RecordingBackend::new(true);
        b.move_relative(3, -4).unwrap();
        b.button(MouseButton::Left, true).unwrap();
        b.scroll(0.0, 1.5).unwrap();
        b.key(HidUsage(0x04), true).unwrap();
        b.media(MediaKey::Mute).unwrap();
        b.text("hi").unwrap();
        assert_eq!(
            handle.events(),
            vec![
                Recorded::Move(3, -4),
                Recorded::Button(MouseButton::Left, true),
                Recorded::Scroll(0.0, 1.5),
                Recorded::Key(HidUsage(0x04), true),
                Recorded::Media(MediaKey::Mute),
                Recorded::Text("hi".into()),
            ]
        );
    }

    #[test]
    fn text_unsupported_when_disabled() {
        let (mut b, handle) = RecordingBackend::new(false);
        assert!(!b.supports_text());
        assert!(matches!(b.text("x"), Err(InputError::Unsupported(_))));
        assert!(handle.events().is_empty());
    }
}
