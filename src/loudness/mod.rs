use smart_leds::{colors::*, RGB8};

pub fn loudness_color(lufs: f32) -> RGB8 {
    if lufs < -100.0 {
        return WHITE;
    } else if lufs < -60.0 {
        return CYAN;
    } else if lufs < -24.0 {
        return GREEN;
    } else if lufs < -18.0 {
        return YELLOW;
    } else if lufs < -12.0 {
        return ORANGE_RED;
    } else if lufs < -6.0 {
        return RED;
    }
    DARK_RED
}

pub fn loudness_step(step: usize) -> f32 {
    -72.0 + ((step * 6) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loudness_step_increases() {
        for i in 0..11 {
            assert!(loudness_step(i + 1) > loudness_step(i));
        }
    }

    #[test]
    fn test_loudness_color_ranges() {
        assert_eq!(loudness_color(-110.0), WHITE);
        assert_eq!(loudness_color(-70.0), CYAN);
        assert_eq!(loudness_color(-30.0), GREEN);
        assert_eq!(loudness_color(-20.0), YELLOW);
        assert_eq!(loudness_color(-15.0), ORANGE_RED);
        assert_eq!(loudness_color(-8.0), RED);
        assert_eq!(loudness_color(0.0), DARK_RED);
    }
}
