//! Cumulative totals of the current pointer gesture (spec 3.6). Dart sends
//! per-frame deltas; the agent wants totals since the gesture started, so a
//! lost datagram is repaired by the next one.

use lanpilot_core::proto::v1::GestureState;

#[derive(Debug, Default)]
pub struct GestureAcc {
    id: u32,
    seq: u32,
    dx: f64,
    dy: f64,
    sx: f64,
    sy: f64,
    active: bool,
}

impl GestureAcc {
    /// Starts a new gesture (a new finger down). Ids strictly increase.
    pub fn begin(&mut self) {
        self.id = self.id.wrapping_add(1).max(1);
        self.seq = 0;
        self.dx = 0.0;
        self.dy = 0.0;
        self.sx = 0.0;
        self.sy = 0.0;
        self.active = true;
    }

    pub fn motion(&mut self, dx: f64, dy: f64) -> GestureState {
        self.ensure_active();
        self.dx += dx;
        self.dy += dy;
        self.advance()
    }

    pub fn scroll(&mut self, sx: f64, sy: f64) -> GestureState {
        self.ensure_active();
        self.sx += sx;
        self.sy += sy;
        self.advance()
    }

    /// The latest state, attached to button events so the agent can catch up
    /// on motion before clicking. `None` before the first gesture.
    pub fn current(&self) -> Option<GestureState> {
        self.active.then(|| self.state())
    }

    fn ensure_active(&mut self) {
        if !self.active || self.seq == u32::MAX {
            self.begin();
        }
    }

    fn advance(&mut self) -> GestureState {
        self.seq += 1;
        self.state()
    }

    fn state(&self) -> GestureState {
        GestureState {
            gesture_id: self.id,
            seq: self.seq,
            total_dx: self.dx as f32,
            total_dy: self.dy as f32,
            total_scroll_x: self.sx as f32,
            total_scroll_y: self.sy as f32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn totals_accumulate_within_a_gesture() {
        let mut g = GestureAcc::default();
        assert!(g.current().is_none());
        g.begin();
        g.motion(1.5, -2.0);
        let s = g.motion(0.5, 1.0);
        assert_eq!((s.gesture_id, s.seq), (1, 2));
        assert_eq!((s.total_dx, s.total_dy), (2.0, -1.0));
        let s = g.scroll(0.0, 0.25);
        assert_eq!((s.seq, s.total_scroll_y), (3, 0.25));
    }

    #[test]
    fn begin_resets_totals_and_bumps_the_id() {
        let mut g = GestureAcc::default();
        g.begin();
        g.motion(10.0, 0.0);
        g.begin();
        let s = g.motion(1.0, 0.0);
        assert_eq!((s.gesture_id, s.seq, s.total_dx), (2, 1, 1.0));
    }

    #[test]
    fn motion_without_begin_starts_a_gesture() {
        let mut g = GestureAcc::default();
        let s = g.motion(1.0, 1.0);
        assert_eq!((s.gesture_id, s.seq), (1, 1));
        assert_eq!(g.current().unwrap().seq, 1);
    }
}
