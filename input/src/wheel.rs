//! Converts fractional notches to whole 1/120-notch units (the Windows
//! `WHEEL_DELTA` and Linux `REL_WHEEL_HI_RES` scale), keeping the remainder.

/// Units per notch on both Windows and Linux hi-res wheels.
pub const UNITS_PER_NOTCH: f32 = 120.0;

#[derive(Debug, Default)]
pub struct WheelAccumulator {
    rem_x: f32,
    rem_y: f32,
}

impl WheelAccumulator {
    pub fn take(&mut self, dx: f32, dy: f32) -> (i32, i32) {
        let tx = self.rem_x + dx * UNITS_PER_NOTCH;
        let ty = self.rem_y + dy * UNITS_PER_NOTCH;
        let (ox, oy) = (tx.trunc(), ty.trunc());
        self.rem_x = tx - ox;
        self.rem_y = ty - oy;
        (ox as i32, oy as i32)
    }
}

/// Turns hi-res units into legacy whole notches (one notch per 120 units).
#[derive(Debug, Default)]
pub struct NotchCounter {
    rem: i32,
}

impl NotchCounter {
    pub fn feed(&mut self, units: i32) -> i32 {
        self.rem += units;
        let notches = self.rem / UNITS_PER_NOTCH as i32;
        self.rem -= notches * UNITS_PER_NOTCH as i32;
        notches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_notches_become_120_units() {
        let mut w = WheelAccumulator::default();
        assert_eq!(w.take(0.0, 1.0), (0, 120));
        assert_eq!(w.take(-2.0, 0.0), (-240, 0));
    }

    #[test]
    fn fractions_accumulate_without_loss() {
        let mut w = WheelAccumulator::default();
        let mut total = 0;
        for _ in 0..1000 {
            total += w.take(0.0, 0.001).1;
        }
        // 1000 * 0.001 notch = 1 notch = 120 units (allow f32 rounding of one unit).
        assert!((119..=120).contains(&total), "{total}");
    }

    #[test]
    fn negative_fractions_accumulate_symmetrically() {
        let mut w = WheelAccumulator::default();
        assert_eq!(w.take(0.0, -0.004), (0, 0));
        assert_eq!(w.take(0.0, -0.005), (0, -1));
    }

    #[test]
    fn notch_counter_emits_whole_notches() {
        let mut n = NotchCounter::default();
        assert_eq!(n.feed(60), 0);
        assert_eq!(n.feed(60), 1);
        assert_eq!(n.feed(-240), -2);
        assert_eq!(n.feed(-119), 0);
        assert_eq!(n.feed(-1), -1);
    }
}
