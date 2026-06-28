//! Encoder acceleration: converts tick intervals to step multipliers.
//! Hardware-specific timing — depends on 1ms poll rate.

/// Compute acceleration steps from ticks since last encoder pulse.
/// Faster turning = more steps per pulse.
pub fn accel_steps(ticks_since_last: u16) -> u8 {
    if ticks_since_last < 20 {
        8 // very fast
    } else if ticks_since_last < 50 {
        4 // fast
    } else if ticks_since_last < 100 {
        2 // moderate
    } else {
        1 // slow/normal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slow_turn_gives_one_step() {
        assert_eq!(accel_steps(200), 1);
        assert_eq!(accel_steps(100), 1);
    }

    #[test]
    fn fast_turn_gives_more_steps() {
        assert_eq!(accel_steps(10), 8);
        assert_eq!(accel_steps(30), 4);
        assert_eq!(accel_steps(80), 2);
    }
}
