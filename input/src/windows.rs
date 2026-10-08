//! Windows backend: `SendInput` (spec 5.2).

#![allow(unsafe_code)]

use crate::wheel::WheelAccumulator;
use crate::{HidUsage, InputBackend, InputError, MediaKey, MouseButton};
use std::mem::size_of;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN,
    MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL, MOUSEINPUT, SendInput,
};

/// HID usage -> (virtual key, extended flag).
pub fn vk_for(usage: u32) -> Option<(u16, bool)> {
    let plain = |vk: u16| Some((vk, false));
    let ext = |vk: u16| Some((vk, true));
    match usage {
        0x04..=0x1D => plain(0x41 + (usage - 0x04) as u16), // a..z
        0x1E..=0x26 => plain(0x31 + (usage - 0x1E) as u16), // 1..9
        0x27 => plain(0x30),                                // 0
        0x28 => plain(0x0D),                                // enter
        0x29 => plain(0x1B),                                // esc
        0x2A => plain(0x08),                                // backspace
        0x2B => plain(0x09),                                // tab
        0x2C => plain(0x20),                                // space
        0x2D => plain(0xBD),                                // minus (VK_OEM_MINUS)
        0x2E => plain(0xBB),                                // equal (VK_OEM_PLUS)
        0x2F => plain(0xDB),                                // [ (VK_OEM_4)
        0x30 => plain(0xDD),                                // ] (VK_OEM_6)
        0x31 => plain(0xDC),                                // \ (VK_OEM_5)
        0x33 => plain(0xBA),                                // ; (VK_OEM_1)
        0x34 => plain(0xDE),                                // ' (VK_OEM_7)
        0x35 => plain(0xC0),                                // ` (VK_OEM_3)
        0x36 => plain(0xBC),                                // ,
        0x37 => plain(0xBE),                                // .
        0x38 => plain(0xBF),                                // / (VK_OEM_2)
        0x39 => plain(0x14),                                // caps lock
        0x3A..=0x45 => plain(0x70 + (usage - 0x3A) as u16), // f1..f12
        0x46 => ext(0x2C),                                  // print screen
        0x47 => plain(0x91),                                // scroll lock
        0x48 => plain(0x13),                                // pause
        0x49 => ext(0x2D),                                  // insert
        0x4A => ext(0x24),                                  // home
        0x4B => ext(0x21),                                  // page up
        0x4C => ext(0x2E),                                  // delete
        0x4D => ext(0x23),                                  // end
        0x4E => ext(0x22),                                  // page down
        0x4F => ext(0x27),                                  // right
        0x50 => ext(0x25),                                  // left
        0x51 => ext(0x28),                                  // down
        0x52 => ext(0x26),                                  // up
        0x65 => ext(0x5D),                                  // menu (VK_APPS)
        0xE0 => plain(0xA2),                                // left ctrl
        0xE1 => plain(0xA0),                                // left shift
        0xE2 => plain(0xA4),                                // left alt
        0xE3 => ext(0x5B),                                  // left win
        0xE4 => ext(0xA3),                                  // right ctrl
        0xE5 => plain(0xA1),                                // right shift
        0xE6 => ext(0xA5),                                  // right alt
        0xE7 => ext(0x5C),                                  // right win
        _ => None,
    }
}

fn media_vk(key: MediaKey) -> u16 {
    match key {
        MediaKey::PlayPause => 0xB3,
        MediaKey::Next => 0xB0,
        MediaKey::Previous => 0xB1,
        MediaKey::VolumeUp => 0xAF,
        MediaKey::VolumeDown => 0xAE,
        MediaKey::Mute => 0xAD,
    }
}

fn mouse(dx: i32, dy: i32, data: i32, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                // windows-sys types this as u32; negative wheel deltas are
                // passed as their two's-complement bit pattern, as Win32 expects.
                mouseData: data as u32,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn keyboard(vk: u16, scan: u16, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn send(inputs: &[INPUT]) -> Result<(), InputError> {
    if inputs.is_empty() {
        return Ok(());
    }
    // SAFETY: `inputs` is a valid slice of initialized INPUT structs; the
    // length and element size passed match it exactly.
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            size_of::<INPUT>() as i32,
        )
    };
    if sent as usize == inputs.len() {
        Ok(())
    } else {
        Err(InputError::Os(std::io::Error::last_os_error().to_string()))
    }
}

#[derive(Debug, Default)]
pub struct SendInputBackend {
    wheel: WheelAccumulator,
}

impl SendInputBackend {
    pub fn new() -> Self {
        Self::default()
    }
}

impl InputBackend for SendInputBackend {
    fn move_relative(&mut self, dx: i32, dy: i32) -> Result<(), InputError> {
        send(&[mouse(dx, dy, 0, MOUSEEVENTF_MOVE)])
    }

    fn button(&mut self, button: MouseButton, down: bool) -> Result<(), InputError> {
        let flags = match (button, down) {
            (MouseButton::Left, true) => MOUSEEVENTF_LEFTDOWN,
            (MouseButton::Left, false) => MOUSEEVENTF_LEFTUP,
            (MouseButton::Right, true) => MOUSEEVENTF_RIGHTDOWN,
            (MouseButton::Right, false) => MOUSEEVENTF_RIGHTUP,
            (MouseButton::Middle, true) => MOUSEEVENTF_MIDDLEDOWN,
            (MouseButton::Middle, false) => MOUSEEVENTF_MIDDLEUP,
        };
        send(&[mouse(0, 0, 0, flags)])
    }

    fn scroll(&mut self, dx: f32, dy: f32) -> Result<(), InputError> {
        let (ux, uy) = self.wheel.take(dx, dy);
        let mut inputs = Vec::with_capacity(2);
        if uy != 0 {
            inputs.push(mouse(0, 0, uy, MOUSEEVENTF_WHEEL));
        }
        if ux != 0 {
            inputs.push(mouse(0, 0, ux, MOUSEEVENTF_HWHEEL));
        }
        send(&inputs)
    }

    fn key(&mut self, usage: HidUsage, down: bool) -> Result<(), InputError> {
        let (vk, extended) = vk_for(usage.0).ok_or(InputError::UnknownKey(usage.0))?;
        let mut flags = if extended { KEYEVENTF_EXTENDEDKEY } else { 0 };
        if !down {
            flags |= KEYEVENTF_KEYUP;
        }
        send(&[keyboard(vk, 0, flags)])
    }

    fn media(&mut self, key: MediaKey) -> Result<(), InputError> {
        let vk = media_vk(key);
        send(&[
            keyboard(vk, 0, KEYEVENTF_EXTENDEDKEY),
            keyboard(vk, 0, KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP),
        ])
    }

    fn supports_text(&self) -> bool {
        true
    }

    fn text(&mut self, text: &str) -> Result<(), InputError> {
        let inputs: Vec<INPUT> = text
            .encode_utf16()
            .flat_map(|unit| {
                [
                    keyboard(0, unit, KEYEVENTF_UNICODE),
                    keyboard(0, unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP),
                ]
            })
            .collect();
        send(&inputs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SUPPORTED_KEYS;

    #[test]
    fn every_supported_key_has_a_virtual_key() {
        for (name, usage) in SUPPORTED_KEYS {
            assert!(vk_for(*usage).is_some(), "no VK for {name} (0x{usage:02x})");
        }
    }

    #[test]
    fn known_mappings() {
        assert_eq!(vk_for(0x04), Some((0x41, false))); // a -> 'A'
        assert_eq!(vk_for(0x27), Some((0x30, false))); // 0 -> '0'
        assert_eq!(vk_for(0x28), Some((0x0D, false))); // enter
        assert_eq!(vk_for(0x50), Some((0x25, true))); // left arrow, extended
        assert_eq!(vk_for(0xE6), Some((0xA5, true))); // right alt, extended
        assert_eq!(vk_for(0xE0), Some((0xA2, false))); // left ctrl
        assert_eq!(vk_for(0x99), None);
    }

    /// Moves the real cursor. Run manually: `cargo test -p lanpilot-input -- --ignored`.
    #[test]
    #[ignore = "moves the real mouse cursor"]
    fn live_move_changes_cursor_position() {
        use windows_sys::Win32::Foundation::POINT;
        use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
        let pos = || {
            let mut p = POINT { x: 0, y: 0 };
            // SAFETY: `p` is a valid, writable POINT for the duration of the call.
            unsafe { GetCursorPos(&mut p) };
            p.x
        };
        let mut b = SendInputBackend::new();
        b.move_relative(-50, 0).unwrap();
        let before = pos();
        b.move_relative(20, 0).unwrap();
        assert!(pos() > before, "cursor did not move right");
    }
}
