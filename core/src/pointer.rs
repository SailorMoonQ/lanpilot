//! Receiver side of cumulative pointer motion. See spec section 3.6.

use crate::proto::v1::GestureState;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PointerDelta {
    pub dx: i32,
    pub dy: i32,
    pub scroll_x: f32,
    pub scroll_y: f32,
}

impl PointerDelta {
    pub fn is_zero(&self) -> bool {
        self.dx == 0 && self.dy == 0 && self.scroll_x == 0.0 && self.scroll_y == 0.0
    }
}

#[derive(Debug)]
struct Current {
    gesture_id: u32,
    last_seq: u32,
    emitted_x: i64,
    emitted_y: i64,
    scroll_x: f32,
    scroll_y: f32,
}

#[derive(Debug, Default)]
pub struct PointerApplier {
    current: Option<Current>,
}

impl PointerApplier {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply(&mut self, state: &GestureState) -> Option<PointerDelta> {
        let is_new = match &self.current {
            Some(c) if state.gesture_id < c.gesture_id => return None,
            Some(c) if state.gesture_id == c.gesture_id => {
                if state.seq <= c.last_seq {
                    return None;
                }
                false
            }
            _ => true,
        };
        if is_new {
            self.current = Some(Current {
                gesture_id: state.gesture_id,
                last_seq: 0,
                emitted_x: 0,
                emitted_y: 0,
                scroll_x: 0.0,
                scroll_y: 0.0,
            });
        }
        let c = self.current.as_mut().expect("set above");
        c.last_seq = state.seq;

        let target_x = f64::from(state.total_dx).floor() as i64;
        let target_y = f64::from(state.total_dy).floor() as i64;
        let delta = PointerDelta {
            dx: (target_x - c.emitted_x) as i32,
            dy: (target_y - c.emitted_y) as i32,
            scroll_x: state.total_scroll_x - c.scroll_x,
            scroll_y: state.total_scroll_y - c.scroll_y,
        };
        c.emitted_x = target_x;
        c.emitted_y = target_y;
        c.scroll_x = state.total_scroll_x;
        c.scroll_y = state.total_scroll_y;
        Some(delta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

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

    #[test]
    fn emits_differences_between_totals() {
        let mut a = PointerApplier::new();
        assert_eq!(a.apply(&gs(1, 1, 3.0, 4.0)).unwrap().dx, 3);
        let d = a.apply(&gs(1, 2, 10.0, 1.0)).unwrap();
        assert_eq!((d.dx, d.dy), (7, -3));
    }

    #[test]
    fn lost_sample_is_recovered_by_next() {
        let mut a = PointerApplier::new();
        a.apply(&gs(1, 1, 5.0, 0.0));
        // seq 2 (total 9) is lost
        let d = a.apply(&gs(1, 3, 12.0, 0.0)).unwrap();
        assert_eq!(d.dx, 7);
    }

    #[test]
    fn stale_seq_is_ignored() {
        let mut a = PointerApplier::new();
        a.apply(&gs(1, 5, 20.0, 0.0));
        assert_eq!(a.apply(&gs(1, 4, 15.0, 0.0)), None);
        assert_eq!(a.apply(&gs(1, 5, 20.0, 0.0)), None);
    }

    #[test]
    fn older_gesture_is_ignored_and_newer_resets() {
        let mut a = PointerApplier::new();
        a.apply(&gs(2, 1, 5.0, 0.0));
        assert_eq!(a.apply(&gs(1, 9, 50.0, 0.0)), None);
        let d = a.apply(&gs(3, 1, 2.0, 0.0)).unwrap();
        assert_eq!(d.dx, 2);
    }

    #[test]
    fn sub_pixel_motion_accumulates() {
        let mut a = PointerApplier::new();
        let d1 = a.apply(&gs(1, 1, 0.4, 0.0)).unwrap();
        let d2 = a.apply(&gs(1, 2, 0.8, 0.0)).unwrap();
        let d3 = a.apply(&gs(1, 3, 1.2, 0.0)).unwrap();
        assert_eq!((d1.dx, d2.dx, d3.dx), (0, 0, 1));
        assert!(d1.is_zero());
    }

    #[test]
    fn negative_motion_floors_consistently() {
        let mut a = PointerApplier::new();
        assert_eq!(a.apply(&gs(1, 1, -0.5, 0.0)).unwrap().dx, -1);
        assert_eq!(a.apply(&gs(1, 2, -1.0, 0.0)).unwrap().dx, 0);
        assert_eq!(a.apply(&gs(1, 3, -2.5, 0.0)).unwrap().dx, -2);
    }

    #[test]
    fn scroll_is_fractional_difference() {
        let mut a = PointerApplier::new();
        let mut s = gs(1, 1, 0.0, 0.0);
        s.total_scroll_y = 1.5;
        assert_eq!(a.apply(&s).unwrap().scroll_y, 1.5);
        s.seq = 2;
        s.total_scroll_y = 2.0;
        assert_eq!(a.apply(&s).unwrap().scroll_y, 0.5);
    }

    /// Tiny deterministic RNG so the property test controls loss and order.
    fn xorshift(state: &mut u64) -> u64 {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        *state
    }

    proptest! {
        #[test]
        fn converges_despite_loss_and_reorder(
            moves in prop::collection::vec((-50.0f32..50.0, -50.0f32..50.0), 1..200),
            seed in 1u64..u64::MAX,
        ) {
            // Build the samples the phone would send (f32 accumulation like the app).
            let (mut tx, mut ty) = (0.0f32, 0.0f32);
            let mut samples = Vec::new();
            for (i, (dx, dy)) in moves.iter().enumerate() {
                tx += dx;
                ty += dy;
                samples.push(gs(1, i as u32 + 1, tx, ty));
            }
            let last = *samples.last().unwrap();

            // Drop about a third, then shuffle the rest.
            let mut rng = seed;
            let mut delivered: Vec<GestureState> =
                samples.into_iter().filter(|_| !xorshift(&mut rng).is_multiple_of(3)).collect();
            for i in (1..delivered.len()).rev() {
                let j = (xorshift(&mut rng) % (i as u64 + 1)) as usize;
                delivered.swap(i, j);
            }
            // The final state always arrives eventually (next datagram or a click).
            delivered.push(last);

            let mut a = PointerApplier::new();
            let (mut sx, mut sy) = (0i64, 0i64);
            for s in &delivered {
                if let Some(d) = a.apply(s) {
                    sx += d.dx as i64;
                    sy += d.dy as i64;
                }
            }
            prop_assert_eq!(sx, (last.total_dx as f64).floor() as i64);
            prop_assert_eq!(sy, (last.total_dy as f64).floor() as i64);
        }
    }
}
