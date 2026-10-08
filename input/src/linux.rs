//! Linux backend: a uinput virtual device (spec 5.2).

use crate::wheel::{NotchCounter, WheelAccumulator};
use crate::{HidUsage, InputBackend, InputError, MediaKey, MouseButton, SUPPORTED_KEYS};
use evdev::uinput::{VirtualDevice, VirtualDeviceBuilder};
use evdev::{AttributeSet, EventType, InputEvent, Key, RelativeAxisType};
use std::time::Duration;

pub const VIRTUAL_DEVICE_NAME: &str = "LanPilot Virtual Input";

const REL_WHEEL_HI_RES: RelativeAxisType = RelativeAxisType(0x0b);
const REL_HWHEEL_HI_RES: RelativeAxisType = RelativeAxisType(0x0c);

/// HID usage -> Linux KEY_* code (linux/input-event-codes.h).
pub fn linux_key_for(usage: u32) -> Option<u16> {
    const LETTERS: [u16; 26] = [
        30, 48, 46, 32, 18, 33, 34, 35, 23, 36, 37, 38, 50, // a..m
        49, 24, 25, 16, 19, 31, 20, 22, 47, 17, 45, 21, 44, // n..z
    ];
    Some(match usage {
        0x04..=0x1D => LETTERS[(usage - 0x04) as usize],
        0x1E..=0x26 => 2 + (usage - 0x1E) as u16, // 1..9 -> KEY_1..KEY_9
        0x27 => 11,                               // 0
        0x28 => 28,                               // enter
        0x29 => 1,                                // esc
        0x2A => 14,                               // backspace
        0x2B => 15,                               // tab
        0x2C => 57,                               // space
        0x2D => 12,                               // minus
        0x2E => 13,                               // equal
        0x2F => 26,                               // [
        0x30 => 27,                               // ]
        0x31 => 43,                               // backslash
        0x33 => 39,                               // ;
        0x34 => 40,                               // '
        0x35 => 41,                               // `
        0x36 => 51,                               // ,
        0x37 => 52,                               // .
        0x38 => 53,                               // /
        0x39 => 58,                               // caps lock
        0x3A..=0x43 => 59 + (usage - 0x3A) as u16, // f1..f10
        0x44 => 87,                               // f11
        0x45 => 88,                               // f12
        0x46 => 99,                               // print screen (KEY_SYSRQ)
        0x47 => 70,                               // scroll lock
        0x48 => 119,                              // pause
        0x49 => 110,                              // insert
        0x4A => 102,                              // home
        0x4B => 104,                              // page up
        0x4C => 111,                              // delete
        0x4D => 107,                              // end
        0x4E => 109,                              // page down
        0x4F => 106,                              // right
        0x50 => 105,                              // left
        0x51 => 108,                              // down
        0x52 => 103,                              // up
        0x65 => 127,                              // menu (KEY_COMPOSE)
        0xE0 => 29,                               // left ctrl
        0xE1 => 42,                               // left shift
        0xE2 => 56,                               // left alt
        0xE3 => 125,                              // left meta
        0xE4 => 97,                               // right ctrl
        0xE5 => 54,                               // right shift
        0xE6 => 100,                              // right alt
        0xE7 => 126,                              // right meta
        _ => return None,
    })
}

fn media_code(key: MediaKey) -> u16 {
    match key {
        MediaKey::PlayPause => 164,
        MediaKey::Next => 163,
        MediaKey::Previous => 165,
        MediaKey::VolumeUp => 115,
        MediaKey::VolumeDown => 114,
        MediaKey::Mute => 113,
    }
}

fn os_err(e: std::io::Error) -> InputError {
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        InputError::Os(format!(
            "{e}: no access to /dev/uinput; install packaging/linux/70-lanpilot-uinput.rules"
        ))
    } else {
        InputError::Os(e.to_string())
    }
}

pub struct UinputBackend {
    device: VirtualDevice,
    wheel: WheelAccumulator,
    notches_x: NotchCounter,
    notches_y: NotchCounter,
}

impl UinputBackend {
    pub fn new() -> Result<Self, InputError> {
        let mut keys = AttributeSet::<Key>::new();
        for k in [Key::BTN_LEFT, Key::BTN_RIGHT, Key::BTN_MIDDLE] {
            keys.insert(k);
        }
        for (_, usage) in SUPPORTED_KEYS {
            keys.insert(Key::new(
                linux_key_for(*usage).expect("table covers SUPPORTED_KEYS"),
            ));
        }
        for media in [
            MediaKey::PlayPause,
            MediaKey::Next,
            MediaKey::Previous,
            MediaKey::VolumeUp,
            MediaKey::VolumeDown,
            MediaKey::Mute,
        ] {
            keys.insert(Key::new(media_code(media)));
        }
        let mut axes = AttributeSet::<RelativeAxisType>::new();
        for a in [
            RelativeAxisType::REL_X,
            RelativeAxisType::REL_Y,
            RelativeAxisType::REL_WHEEL,
            RelativeAxisType::REL_HWHEEL,
            REL_WHEEL_HI_RES,
            REL_HWHEEL_HI_RES,
        ] {
            axes.insert(a);
        }
        let device = VirtualDeviceBuilder::new()
            .map_err(os_err)?
            .name(VIRTUAL_DEVICE_NAME)
            .with_keys(&keys)
            .map_err(os_err)?
            .with_relative_axes(&axes)
            .map_err(os_err)?
            .build()
            .map_err(os_err)?;
        // Give the compositor time to pick the new device up.
        std::thread::sleep(Duration::from_millis(200));
        Ok(Self {
            device,
            wheel: WheelAccumulator::default(),
            notches_x: NotchCounter::default(),
            notches_y: NotchCounter::default(),
        })
    }

    fn emit(&mut self, events: &[InputEvent]) -> Result<(), InputError> {
        if events.is_empty() {
            return Ok(());
        }
        // `emit` appends the SYN_REPORT that ends the frame.
        self.device.emit(events).map_err(os_err)
    }

    fn key_event(code: u16, down: bool) -> InputEvent {
        InputEvent::new(EventType::KEY, code, i32::from(down))
    }
}

impl InputBackend for UinputBackend {
    fn move_relative(&mut self, dx: i32, dy: i32) -> Result<(), InputError> {
        let mut ev = Vec::with_capacity(2);
        if dx != 0 {
            ev.push(InputEvent::new(
                EventType::RELATIVE,
                RelativeAxisType::REL_X.0,
                dx,
            ));
        }
        if dy != 0 {
            ev.push(InputEvent::new(
                EventType::RELATIVE,
                RelativeAxisType::REL_Y.0,
                dy,
            ));
        }
        self.emit(&ev)
    }

    fn button(&mut self, button: MouseButton, down: bool) -> Result<(), InputError> {
        let key = match button {
            MouseButton::Left => Key::BTN_LEFT,
            MouseButton::Right => Key::BTN_RIGHT,
            MouseButton::Middle => Key::BTN_MIDDLE,
        };
        self.emit(&[Self::key_event(key.code(), down)])
    }

    fn scroll(&mut self, dx: f32, dy: f32) -> Result<(), InputError> {
        let (ux, uy) = self.wheel.take(dx, dy);
        let (nx, ny) = (self.notches_x.feed(ux), self.notches_y.feed(uy));
        let rel = |axis: RelativeAxisType, v: i32| InputEvent::new(EventType::RELATIVE, axis.0, v);
        let mut ev = Vec::with_capacity(4);
        if uy != 0 {
            ev.push(rel(REL_WHEEL_HI_RES, uy));
        }
        if ny != 0 {
            ev.push(rel(RelativeAxisType::REL_WHEEL, ny));
        }
        if ux != 0 {
            ev.push(rel(REL_HWHEEL_HI_RES, ux));
        }
        if nx != 0 {
            ev.push(rel(RelativeAxisType::REL_HWHEEL, nx));
        }
        self.emit(&ev)
    }

    fn key(&mut self, usage: HidUsage, down: bool) -> Result<(), InputError> {
        let code = linux_key_for(usage.0).ok_or(InputError::UnknownKey(usage.0))?;
        self.emit(&[Self::key_event(code, down)])
    }

    fn media(&mut self, key: MediaKey) -> Result<(), InputError> {
        let code = media_code(key);
        self.emit(&[Self::key_event(code, true)])?;
        self.emit(&[Self::key_event(code, false)])
    }

    fn supports_text(&self) -> bool {
        false
    }

    fn text(&mut self, _text: &str) -> Result<(), InputError> {
        // Decided by the M0 spike (clipboard paste vs IBus), spec 9.
        Err(InputError::Unsupported("text input on Linux"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SUPPORTED_KEYS;

    #[test]
    fn every_supported_key_has_a_linux_code() {
        for (name, usage) in SUPPORTED_KEYS {
            assert!(
                linux_key_for(*usage).is_some(),
                "no KEY_* for {name} (0x{usage:02x})"
            );
        }
    }

    #[test]
    fn known_mappings() {
        assert_eq!(linux_key_for(0x04), Some(30)); // a -> KEY_A
        assert_eq!(linux_key_for(0x1D), Some(44)); // z -> KEY_Z
        assert_eq!(linux_key_for(0x27), Some(11)); // 0 -> KEY_0
        assert_eq!(linux_key_for(0x28), Some(28)); // enter
        assert_eq!(linux_key_for(0xE3), Some(125)); // left meta
        assert_eq!(linux_key_for(0x99), None);
    }

    /// Needs /dev/uinput access (install the udev rule). Run on Linux with
    /// `cargo test -p lanpilot-input -- --ignored`.
    #[test]
    #[ignore = "creates a real uinput device"]
    fn live_device_appears_and_accepts_events() {
        let mut b = UinputBackend::new().unwrap();
        let names: Vec<String> = std::fs::read_dir("/sys/class/input")
            .unwrap()
            .filter_map(|e| std::fs::read_to_string(e.ok()?.path().join("name")).ok())
            .map(|s| s.trim().to_owned())
            .collect();
        assert!(names.iter().any(|n| n == VIRTUAL_DEVICE_NAME), "{names:?}");
        b.move_relative(5, 0).unwrap();
        b.move_relative(-5, 0).unwrap();
        b.scroll(0.0, 0.5).unwrap();
        b.scroll(0.0, -0.5).unwrap();
    }
}
