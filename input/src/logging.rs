//! A backend that only logs each call (at debug level). The agent uses it on
//! platforms without a real backend and for `run --mock-input`, so phone apps
//! can be developed against a live agent without driving a desktop.

use crate::{HidUsage, InputBackend, InputError, MediaKey, MouseButton};

#[derive(Debug, Default)]
pub struct LoggingBackend;

impl LoggingBackend {
    pub fn new() -> Self {
        Self
    }
}

impl InputBackend for LoggingBackend {
    fn move_relative(&mut self, dx: i32, dy: i32) -> Result<(), InputError> {
        tracing::debug!("mock input: move ({dx}, {dy})");
        Ok(())
    }

    fn button(&mut self, button: MouseButton, down: bool) -> Result<(), InputError> {
        let edge = if down { "down" } else { "up" };
        tracing::debug!("mock input: {button:?} button {edge}");
        Ok(())
    }

    fn scroll(&mut self, dx: f32, dy: f32) -> Result<(), InputError> {
        tracing::debug!("mock input: scroll ({dx}, {dy}) notches");
        Ok(())
    }

    fn key(&mut self, usage: HidUsage, down: bool) -> Result<(), InputError> {
        let edge = if down { "down" } else { "up" };
        tracing::debug!("mock input: key 0x{:02x} {edge}", usage.0);
        Ok(())
    }

    fn media(&mut self, key: MediaKey) -> Result<(), InputError> {
        tracing::debug!("mock input: media {key:?}");
        Ok(())
    }

    fn supports_text(&self) -> bool {
        true
    }

    fn text(&mut self, text: &str) -> Result<(), InputError> {
        tracing::debug!("mock input: text {text:?}");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_every_call() {
        let mut b = LoggingBackend::new();
        assert!(b.supports_text());
        b.move_relative(3, -4).unwrap();
        b.button(MouseButton::Left, true).unwrap();
        b.button(MouseButton::Left, false).unwrap();
        b.scroll(-1.5, 2.0).unwrap();
        b.key(HidUsage(0x04), true).unwrap();
        b.key(HidUsage(0x04), false).unwrap();
        b.media(MediaKey::Mute).unwrap();
        b.text("你好").unwrap();
    }
}
