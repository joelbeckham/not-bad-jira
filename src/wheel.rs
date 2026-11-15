use std::time::{Duration, Instant};

#[derive(Default)]
pub struct RowWheel {
    remainder: f32,
    last_event: Option<Instant>,
}

impl RowWheel {
    pub fn advance(&mut self, delta: f32, selected: usize, count: usize) -> usize {
        if count == 0 || !delta.is_finite() {
            *self = Self::default();
            return selected.min(count.saturating_sub(1));
        }
        if delta == 0. {
            return selected.min(count - 1);
        }
        let now = Instant::now();
        if self
            .last_event
            .is_none_or(|last| now.duration_since(last) > Duration::from_millis(400))
            || self.remainder.signum() != delta.signum()
        {
            self.remainder = 0.;
        }
        self.last_event = Some(now);
        self.remainder += delta;
        let steps = self.remainder.trunc();
        self.remainder -= steps;
        // Windows applies its lines-per-notch setting before dispatch. A wheel
        // event still advances just one row; do not queue the excess lines.
        let steps = steps.clamp(-1., 1.);
        let next =
            (selected.min(count - 1) as f64 - steps as f64).clamp(0., (count - 1) as f64) as usize;
        if (next == 0 && delta > 0.) || (next == count - 1 && delta < 0.) {
            self.remainder = 0.;
        }
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_direction_and_limits() {
        let mut wheel = RowWheel::default();
        assert_eq!(wheel.advance(-3., 5, 20), 6);
        assert_eq!(wheel.advance(-3., 6, 20), 7);
        assert_eq!(wheel.advance(3., 7, 20), 6);
        assert_eq!(wheel.advance(100., 5, 20), 4);
        assert_eq!(wheel.advance(-100., 5, 20), 6);
        assert_eq!(wheel.advance(3., 0, 20), 0);
        assert_eq!(wheel.advance(-1., 19, 20), 19);
        assert_eq!(wheel.advance(1., 19, 20), 18);
    }

    #[test]
    fn fractional_motion_accumulates_and_reverses() {
        let mut wheel = RowWheel::default();
        assert_eq!(wheel.advance(-0.4, 0, 20), 0);
        assert_eq!(wheel.advance(-0.7, 0, 20), 1);
        wheel = RowWheel::default();
        assert_eq!(wheel.advance(-0.4, 5, 20), 5);
        assert_eq!(wheel.advance(-0.7, 5, 20), 6);
        assert_eq!(wheel.advance(0.6, 6, 20), 6);
        assert_eq!(wheel.advance(0.5, 6, 20), 5);
        wheel.last_event = Some(Instant::now() - Duration::from_secs(1));
        assert_eq!(wheel.advance(0.95, 5, 20), 5);
    }

    #[test]
    fn empty_and_shortened_lists_are_safe() {
        let mut wheel = RowWheel::default();
        assert_eq!(wheel.advance(-3., 8, 0), 0);
        assert_eq!(wheel.advance(-3., 8, 1), 0);
        assert_eq!(wheel.advance(1., 8, 3), 1);
    }
}
