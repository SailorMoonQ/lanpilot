//! Converts fractional notches to whole 1/120-notch units (the Windows
//! `WHEEL_DELTA` and Linux `REL_WHEEL_HI_RES` scale), keeping the remainder.

/// Units per notch on both Windows and Linux hi-res wheels.
pub const UNITS_PER_NOTCH: f32 = 120.0;

#[derive(Debug, Default)]
pub struct WheelAccumulator {
    rem_x: f32,
    rem_y: f32,
}

/// Largest magnitude, in units, one `take` call produces per axis.
const MAX_UNITS_PER_CALL: f32 = crate::MAX_SCROLL_NOTCHES_PER_CALL * UNITS_PER_NOTCH;

impl WheelAccumulator {
    /// Adds `dx`/`dy` notches and returns the whole units to emit. Non-finite
    /// input is ignored; each axis is clamped to
    /// [`crate::MAX_SCROLL_NOTCHES_PER_CALL`] notches.
    pub fn take(&mut self, dx: f32, dy: f32) -> (i32, i32) {
        (
            Self::axis(&mut self.rem_x, dx),
            Self::axis(&mut self.rem_y, dy),
        )
    }

    fn axis(rem: &mut f32, notches: f32) -> i32 {
        if !notches.is_finite() {
            return 0;
        }
        let total =
            (*rem + notches * UNITS_PER_NOTCH).clamp(-MAX_UNITS_PER_CALL, MAX_UNITS_PER_CALL);
        let out = total.trunc();
        *rem = total - out;
        out as i32
    }
}

/// Turns hi-res units into legacy whole notches (one notch per 120 units).
#[derive(Debug, Default)]
pub struct NotchCounter {
    rem: i64,
}

impl NotchCounter {
    /// Never overflows: the remainder is kept in `i64` and always stays
    /// below one notch in magnitude.
    pub fn feed(&mut self, units: i32) -> i32 {
        const PER_NOTCH: i64 = UNITS_PER_NOTCH as i64;
        self.rem += i64::from(units);
        let notches = self.rem / PER_NOTCH;
        self.rem -= notches * PER_NOTCH;
        // |notches| <= (i32::MAX + 119) / 120, far inside i32.
        i32::try_from(notches).unwrap_or(if notches < 0 { i32::MIN } else { i32::MAX })
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

    const MAX_UNITS: i32 = (crate::MAX_SCROLL_NOTCHES_PER_CALL * UNITS_PER_NOTCH) as i32;

    /// Gesture totals as the agent receives them; each call scrolls by the
    /// difference to the previous total, like the pointer applier emits.
    #[test]
    fn extreme_totals_never_overflow_and_stay_bounded() {
        let mut w = WheelAccumulator::default();
        let mut n = NotchCounter::default();
        let mut prev = 0.0f32;
        let mut last = (0, 0);
        for total in [-8.0e6f32, -7_999_999.5, 1.0e7] {
            let (_, units) = w.take(0.0, total - prev);
            prev = total;
            assert!(units.abs() <= MAX_UNITS, "{units}");
            last = (units, n.feed(units));
        }
        // The final step is a huge positive scroll: clamped, still positive.
        assert_eq!(last.0, MAX_UNITS);
        assert!(last.1 > 0 && last.1 <= 1001, "{}", last.1);
    }

    #[test]
    fn notch_counter_never_overflows() {
        let mut n = NotchCounter::default();
        assert_eq!(n.feed(60), 0);
        assert!(n.feed(i32::MAX) > 0);
        assert!(n.feed(i32::MAX) > 0);
        let mut n = NotchCounter::default();
        assert_eq!(n.feed(-60), 0);
        assert!(n.feed(i32::MIN) < 0);
        assert!(n.feed(i32::MIN) < 0);
    }

    #[test]
    fn non_finite_input_is_ignored_and_does_not_poison() {
        let mut w = WheelAccumulator::default();
        assert_eq!(w.take(0.0, 0.5), (0, 60));
        assert_eq!(w.take(f32::NAN, f32::NAN), (0, 0));
        assert_eq!(w.take(f32::INFINITY, f32::NEG_INFINITY), (0, 0));
        assert_eq!(w.take(1.0, 1.0), (120, 120));
    }
}
