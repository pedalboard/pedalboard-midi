use crate::ledring::LedRing;
use crate::ledring::LEDS_PER_RING;
use smart_leds::RGB8;

const NUM_LEDS: usize = 2;
const NUM_LED_RINGS: usize = 8;
pub const LED_OUTPUTS: usize = NUM_LEDS + NUM_LED_RINGS * LEDS_PER_RING;

pub type LedData = [RGB8; LED_OUTPUTS];

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum Led {
    Mode,
    Mon,
}

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum LedRings {
    Gain,
    F,
    C,
    B,
    E,
    Vol,
    D,
    A,
}

pub struct Leds {
    singles: [Option<RGB8>; NUM_LEDS],
    ledrings: [LedRing; NUM_LED_RINGS],
}

impl Leds {
    pub fn new() -> Self {
        Leds {
            singles: [None; NUM_LEDS],
            ledrings: [LedRing::default(); NUM_LED_RINGS],
        }
    }

    /// Render current state into a pixel buffer.
    pub fn render(&self) -> LedData {
        let mut data: LedData = [RGB8::default(); LED_OUTPUTS];

        for (ring_index, ring) in self.ledrings.iter().enumerate() {
            for (led_index, pixel) in ring.render().into_iter().enumerate() {
                data[ring_index * LEDS_PER_RING + led_index] = pixel;
            }
        }

        for (i, color) in self.singles.iter().enumerate() {
            let led = NUM_LED_RINGS * LEDS_PER_RING + i;
            data[led] = color.unwrap_or_default();
        }

        data
    }

    pub fn set_single(&mut self, l: Led, color: Option<RGB8>) {
        self.singles[l as usize] = color;
    }

    pub fn set_ledring(&mut self, a: crate::ledring::Animation, r: LedRings) {
        self.ledrings[r as usize].set(a);
    }
}

impl Default for Leds {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smart_leds::colors::*;

    #[test]
    fn test_leds_default_all_off() {
        let leds = Leds::new();
        let data = leds.render();
        for led in data.iter() {
            assert_eq!(*led, RGB8::default());
        }
    }

    #[test]
    fn test_set_single_on() {
        let mut leds = Leds::new();
        leds.set_single(Led::Mon, Some(RED));
        let data = leds.render();
        let mon_index = NUM_LED_RINGS * LEDS_PER_RING + Led::Mon as usize;
        assert_eq!(data[mon_index], RED);
    }

    #[test]
    fn test_set_single_off() {
        let mut leds = Leds::new();
        leds.set_single(Led::Mon, Some(RED));
        leds.set_single(Led::Mon, None);
        let data = leds.render();
        let mon_index = NUM_LED_RINGS * LEDS_PER_RING + Led::Mon as usize;
        assert_eq!(data[mon_index], RGB8::default());
    }
}
