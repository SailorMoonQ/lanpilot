//! Platform input injection for the LanPilot agent (spec section 5.2).
//!
//! Conventions shared by every backend:
//! - Keys are USB HID keyboard page usages ([`HidUsage`]).
//! - `scroll` takes notches; fractions allowed. Positive `dy` is wheel up,
//!   positive `dx` is wheel right.

mod keys;
#[cfg(target_os = "linux")]
pub mod linux;
pub mod recording;
pub mod wheel;
#[cfg(windows)]
pub mod windows;

pub use keys::{HidUsage, SUPPORTED_KEYS, usage_from_name};

/// Largest scroll, in notches per axis, one `scroll` call may produce. Larger
/// requests are clamped by the agent and by the wheel accumulator.
pub const MAX_SCROLL_NOTCHES_PER_CALL: f32 = 1000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKey {
    PlayPause,
    Next,
    Previous,
    VolumeUp,
    VolumeDown,
    Mute,
}

#[derive(Debug, thiserror::Error)]
pub enum InputError {
    #[error("not supported on this platform: {0}")]
    Unsupported(&'static str),
    #[error("unknown key usage 0x{0:02x}")]
    UnknownKey(u32),
    #[error("input injection failed: {0}")]
    Os(String),
}

/// One injector for the whole machine. Calls are short and synchronous.
pub trait InputBackend: Send {
    fn move_relative(&mut self, dx: i32, dy: i32) -> Result<(), InputError>;
    fn button(&mut self, button: MouseButton, down: bool) -> Result<(), InputError>;
    fn scroll(&mut self, dx: f32, dy: f32) -> Result<(), InputError>;
    fn key(&mut self, usage: HidUsage, down: bool) -> Result<(), InputError>;
    fn media(&mut self, key: MediaKey) -> Result<(), InputError>;
    fn supports_text(&self) -> bool;
    fn text(&mut self, text: &str) -> Result<(), InputError>;
}

/// The real backend for this platform.
pub fn open_default() -> Result<Box<dyn InputBackend>, InputError> {
    #[cfg(windows)]
    {
        Ok(Box::new(windows::SendInputBackend::new()))
    }
    #[cfg(target_os = "linux")]
    {
        Ok(Box::new(linux::UinputBackend::new()?))
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        Err(InputError::Unsupported(
            "no input backend for this platform",
        ))
    }
}
