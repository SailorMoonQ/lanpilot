//! Windows backend: `SendInput` (spec 5.2).

#![allow(unsafe_code)]

use crate::wheel::WheelAccumulator;
use crate::{HidUsage, InputBackend, InputError, MediaKey, MouseButton};
use std::mem::size_of;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_HWHEEL,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP,
    MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK,
    MOUSEEVENTF_WHEEL, MOUSEINPUT, SendInput,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VirtualDesk {
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
}

pub(crate) fn absolute_target(
    cursor: (i32, i32),
    delta: (i32, i32),
    desk: &VirtualDesk,
) -> (i32, i32) {
    let clamp = |v: i32, origin: i32, size: i32| v.clamp(origin, origin + size - 1);
    (
        clamp(cursor.0.saturating_add(delta.0), desk.left, desk.width),
        clamp(cursor.1.saturating_add(delta.1), desk.top, desk.height),
    )
}

/// The position a relative move starts from.
///
/// `last` is the cursor position read before the previous absolute move and
/// the target it was sent to. `GetCursorPos` can lag behind `SendInput`, so
/// if the cursor still reads where it was before that move (and the move was
/// not a no-op), the move has not landed yet and its target is the base.
/// Otherwise the move landed, or something else moved the cursor, and the
/// read position is the base.
pub(crate) fn base_position(
    read: (i32, i32),
    last: Option<((i32, i32), (i32, i32))>,
) -> (i32, i32) {
    match last {
        Some((prev_read, target)) if read == prev_read && prev_read != target => target,
        _ => read,
    }
}

/// Pixel -> 0..=65535 so that Windows' mapping `origin + n * size / 65536`
/// (floored) lands exactly on the pixel: n = ceil(offset * 65536 / size).
pub(crate) fn normalize(p: (i32, i32), desk: &VirtualDesk) -> (i32, i32) {
    let axis = |v: i32, origin: i32, size: i32| {
        let offset = (v - origin) as i64;
        let size = size as i64;
        ((offset * 65536 + size - 1) / size).clamp(0, 65535) as i32
    };
    (
        axis(p.0, desk.left, desk.width),
        axis(p.1, desk.top, desk.height),
    )
}

fn virtual_desk() -> Option<VirtualDesk> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN,
    };
    // SAFETY: GetSystemMetrics has no pointer arguments and no preconditions.
    let (left, top, width, height) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    (width > 1 && height > 1).then_some(VirtualDesk {
        left,
        top,
        width,
        height,
    })
}

fn cursor_pos() -> Option<(i32, i32)> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut p = POINT { x: 0, y: 0 };
    // SAFETY: `p` is a valid, writable POINT for the duration of the call.
    (unsafe { GetCursorPos(&mut p) } != 0).then_some((p.x, p.y))
}

#[derive(Debug)]
pub struct SendInputBackend {
    wheel: WheelAccumulator,
    /// Cursor position read before the last absolute move, and its target.
    last: Option<((i32, i32), (i32, i32))>,
}

impl SendInputBackend {
    pub fn new() -> Self {
        use windows_sys::Win32::UI::HiDpi::{
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
        };
        // Without per-monitor awareness, GetCursorPos reports virtualized
        // coordinates on scaled displays.
        // SAFETY: takes a constant context handle and no pointers.
        let set =
            unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        if set == 0 {
            // FALSE when awareness was already set for this process. That is
            // harmless if it was this same mode, but if a different mode was
            // set (for example by a manifest), coordinates may be virtualized
            // on mixed-DPI setups and absolute moves can land off target.
            tracing::debug!(
                "SetProcessDpiAwarenessContext failed: {}",
                std::io::Error::last_os_error()
            );
        }
        Self {
            wheel: WheelAccumulator::default(),
            last: None,
        }
    }
}

/// Same as `new`: the DPI awareness setup must not be skippable.
impl Default for SendInputBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl InputBackend for SendInputBackend {
    fn move_relative(&mut self, dx: i32, dy: i32) -> Result<(), InputError> {
        if dx == 0 && dy == 0 {
            return Ok(());
        }
        match (cursor_pos(), virtual_desk()) {
            (Some(read), Some(desk)) => {
                let base = base_position(read, self.last);
                let target = absolute_target(base, (dx, dy), &desk);
                let (nx, ny) = normalize(target, &desk);
                send(&[mouse(
                    nx,
                    ny,
                    0,
                    MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
                )])?;
                self.last = Some((read, target));
                Ok(())
            }
            // Fall back to a relative move (subject to system acceleration).
            _ => {
                self.last = None;
                send(&[mouse(dx, dy, 0, MOUSEEVENTF_MOVE)])
            }
        }
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

    fn inverse(n: i32, origin: i32, size: i32) -> i32 {
        origin + ((n as i64 * size as i64) / 65536) as i32
    }

    #[test]
    fn normalize_round_trips_every_pixel() {
        for desk in [
            VirtualDesk {
                left: 0,
                top: 0,
                width: 1920,
                height: 1080,
            },
            VirtualDesk {
                left: 0,
                top: 0,
                width: 3840,
                height: 2160,
            },
            VirtualDesk {
                left: -1920,
                top: -300,
                width: 5760,
                height: 1740,
            },
            VirtualDesk {
                left: 0,
                top: 0,
                width: 7680,
                height: 4320,
            },
        ] {
            for x in desk.left..desk.left + desk.width {
                let (nx, _) = normalize((x, desk.top), &desk);
                assert!((0..=65535).contains(&nx));
                assert_eq!(inverse(nx, desk.left, desk.width), x, "x={x} desk={desk:?}");
            }
            for y in desk.top..desk.top + desk.height {
                let (_, ny) = normalize((desk.left, y), &desk);
                assert!((0..=65535).contains(&ny));
                assert_eq!(inverse(ny, desk.top, desk.height), y, "y={y} desk={desk:?}");
            }
        }
    }

    #[test]
    fn target_adds_delta_and_clamps_to_the_desk() {
        let desk = VirtualDesk {
            left: -1920,
            top: 0,
            width: 3840,
            height: 1080,
        };
        assert_eq!(absolute_target((100, 100), (5, -7), &desk), (105, 93));
        assert_eq!(absolute_target((1900, 10), (500, 0), &desk), (1919, 10));
        assert_eq!(absolute_target((-1900, 5), (-500, -50), &desk), (-1920, 0));
        assert_eq!(
            absolute_target((0, 0), (i32::MAX, i32::MIN), &desk),
            (1919, 0)
        );
    }

    #[test]
    fn default_sets_per_monitor_dpi_awareness() {
        use windows_sys::Win32::UI::HiDpi::{
            AreDpiAwarenessContextsEqual, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
            GetThreadDpiAwarenessContext,
        };
        let _ = SendInputBackend::default();
        // SAFETY: both calls take no pointers; the context handles are
        // returned by the system or are system constants.
        let equal = unsafe {
            AreDpiAwarenessContextsEqual(
                GetThreadDpiAwarenessContext(),
                DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
            )
        };
        assert_ne!(equal, 0, "Default skipped the DPI awareness setup");
    }

    /// Counts DEBUG events from this module.
    struct DebugEvents(std::sync::Arc<std::sync::atomic::AtomicUsize>);

    impl tracing::Subscriber for DebugEvents {
        fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
            true
        }
        fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
            tracing::span::Id::from_u64(1)
        }
        fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
        fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
        fn event(&self, e: &tracing::Event<'_>) {
            if *e.metadata().level() == tracing::Level::DEBUG
                && e.metadata().target() == module_path!().trim_end_matches("::tests")
            {
                self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        }
        fn enter(&self, _: &tracing::span::Id) {}
        fn exit(&self, _: &tracing::span::Id) {}
    }

    #[test]
    fn failed_dpi_awareness_setup_is_logged() {
        let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        tracing::subscriber::with_default(DebugEvents(count.clone()), || {
            // Awareness can be set once per process, so the second call fails.
            let _ = SendInputBackend::new();
            let _ = SendInputBackend::new();
        });
        assert!(count.load(std::sync::atomic::Ordering::SeqCst) >= 1);
    }

    #[test]
    fn base_without_history_is_the_read_position() {
        assert_eq!(base_position((10, 20), None), (10, 20));
    }

    #[test]
    fn base_after_a_landed_move_is_the_read_position() {
        // The cursor reached the previous target.
        assert_eq!(
            base_position((11, 20), Some(((10, 20), (11, 20)))),
            (11, 20)
        );
    }

    #[test]
    fn base_before_the_previous_move_lands_is_its_target() {
        // The cursor still reads where it was before the previous move.
        assert_eq!(
            base_position((10, 20), Some(((10, 20), (11, 20)))),
            (11, 20)
        );
    }

    #[test]
    fn base_after_the_user_moved_the_mouse_is_the_read_position() {
        assert_eq!(
            base_position((300, 400), Some(((10, 20), (11, 20)))),
            (300, 400)
        );
    }

    #[test]
    fn base_after_a_clamped_no_op_move_is_the_read_position() {
        assert_eq!(base_position((0, 0), Some(((0, 0), (0, 0)))), (0, 0));
    }

    /// Moves the real cursor by 100 px. Run manually:
    /// `cargo test -p lanpilot-input -- --ignored live_burst_of_small_moves_is_exact`.
    #[test]
    #[ignore = "moves the real mouse cursor"]
    fn live_burst_of_small_moves_is_exact() {
        let mut b = SendInputBackend::new();
        // Make room on the right, wherever the cursor starts.
        b.move_relative(-150, 0).unwrap();
        let before = cursor_pos().expect("GetCursorPos failed");
        for _ in 0..100 {
            b.move_relative(1, 0).unwrap();
        }
        let after = cursor_pos().expect("GetCursorPos failed");
        assert_eq!(after.0 - before.0, 100, "x displacement not exact");
        assert_eq!(after.1, before.1, "y changed");
    }

    /// Moves the real cursor. Run manually: `cargo test -p lanpilot-input -- --ignored`.
    #[test]
    #[ignore = "moves the real mouse cursor"]
    fn live_move_changes_cursor_position() {
        let mut b = SendInputBackend::new();
        b.move_relative(-50, 0).unwrap();
        let before = cursor_pos().expect("GetCursorPos failed");
        b.move_relative(37, 0).unwrap();
        let after = cursor_pos().expect("GetCursorPos failed");
        assert_eq!(after.0 - before.0, 37, "x displacement not exact");
        assert_eq!(after.1, before.1, "y changed");
    }
}
