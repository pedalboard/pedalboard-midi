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
    return DARK_RED;
}

pub fn loudness_step(step: usize) -> f32 {
    -72.0 + ((step * 6) as f32)
}
