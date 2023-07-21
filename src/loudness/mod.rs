use smart_leds::{colors::*, RGB8};

pub fn loudness_color(lufs: f32) -> RGB8 {
    if lufs < -100.0 {
        return LIGHT_BLUE;
    } else if lufs < -60.0 {
        return LIGHT_GREEN;
    } else if lufs < -30.0 {
        return GREEN;
    } else if lufs < -27.0 {
        return YELLOW;
    } else if lufs < -24.0 {
        return ORANGE;
    } else if lufs < -21.0 {
        return DARK_ORANGE;
    } else if lufs < -18.0 {
        return RED;
    }
    return DARK_RED;
}
