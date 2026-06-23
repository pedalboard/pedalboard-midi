use smart_leds::RGB8;
pub const LEDS_PER_RING: usize = 12;

pub type LedData = [RGB8; LEDS_PER_RING];

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum Animation {
    On(RGB8),
    Off,
    Fill(RGB8, u8), // color, count (0-12)
    Heatmap(u8),    // fill level (0-12), arc with dead zone at bottom, blue→green→red
}

#[derive(Copy, Clone)]
pub struct LedRing {
    rotation: u8,
    animation: Animation,
}

impl LedRing {
    pub fn new(rotation: u8) -> Self {
        LedRing {
            rotation,
            animation: Animation::Off,
        }
    }

    /// Render current state to pixel buffer (no side effects).
    pub fn render(&self) -> LedData {
        match self.animation {
            Animation::On(c) => [c; LEDS_PER_RING],
            Animation::Off => [RGB8::default(); LEDS_PER_RING],
            Animation::Fill(c, count) => {
                let mut data = [RGB8::default(); LEDS_PER_RING];
                for i in 0..(count as usize).min(LEDS_PER_RING) {
                    data[(self.rotation as usize + LEDS_PER_RING - i) % LEDS_PER_RING] = c;
                }
                data
            }
            Animation::Heatmap(fill) => {
                // Potentiometer arc: 7h (blue) → 8h → 9h → ... → 12h → ... → 5h (red).
                // Dead zone: 6 o'clock. Arc = 11 LEDs clockwise from 7h to 5h.
                const ARC_HOURS: [usize; 11] = [7, 8, 9, 10, 11, 0, 1, 2, 3, 4, 5];
                let mut data = [RGB8::default(); LEDS_PER_RING];
                let lit = ((fill as usize) * ARC_HOURS.len() / 12).min(ARC_HOURS.len());
                for i in 0..lit {
                    data[CLOCK[ARC_HOURS[i]]] = heatmap_color(i, ARC_HOURS.len());
                }
                data
            }
        }
    }

    pub fn set(&mut self, a: Animation) {
        self.animation = a;
    }
}

impl Default for LedRing {
    fn default() -> Self {
        Self::new(8)
    }
}

/// Physical LED index for each clock-hour position.
/// Derived from PCB layout (pedalboard-display): D1=3h, D2=2h, ..., D12=4h.
/// Indices are 0-based (D1=0, D2=1, ..., D12=11).
const CLOCK: [usize; 12] = [
    3,  //  0: 12 o'clock
    2,  //  1:  1 o'clock
    1,  //  2:  2 o'clock
    0,  //  3:  3 o'clock
    11, //  4:  4 o'clock
    10, //  5:  5 o'clock
    9,  //  6:  6 o'clock
    8,  //  7:  7 o'clock
    7,  //  8:  8 o'clock
    6,  //  9:  9 o'clock
    5,  // 10: 10 o'clock
    4,  // 11: 11 o'clock
];

/// Blue (pos=0) → Green (mid) → Red (pos=max-1)
fn heatmap_color(pos: usize, max: usize) -> RGB8 {
    if max <= 1 {
        return RGB8::new(0, 0, 255);
    }
    let t = (pos * 255) / (max - 1); // 0..255
    if t < 128 {
        // blue → green
        let g = (t * 2) as u8;
        RGB8::new(0, g, 255 - g)
    } else {
        // green → red
        let r = ((t - 128) * 2) as u8;
        RGB8::new(r, 255 - r, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ledring_default_off() {
        let ring = LedRing::default();
        let data = ring.render();
        for led in data.iter() {
            assert_eq!(*led, RGB8::default());
        }
    }

    #[test]
    fn test_ledring_on() {
        let mut ring = LedRing::default();
        let color = RGB8::new(255, 0, 0);
        ring.set(Animation::On(color));
        let data = ring.render();
        for led in data.iter() {
            assert_eq!(*led, color);
        }
    }

    #[test]
    fn test_ledring_fill() {
        let mut ring = LedRing::default();
        let color = RGB8::new(0, 255, 0);
        ring.set(Animation::Fill(color, 3));
        let data = ring.render();
        // With rotation=8, filled LEDs are at indices 8, 7, 6
        assert_eq!(data[8], color);
        assert_eq!(data[7], color);
        assert_eq!(data[6], color);
        assert_eq!(data[5], RGB8::default());
    }

    #[test]
    fn test_heatmap_zero_is_off() {
        let mut ring = LedRing::default();
        ring.set(Animation::Heatmap(0));
        let data = ring.render();
        for led in data.iter() {
            assert_eq!(*led, RGB8::default());
        }
    }

    #[test]
    fn test_heatmap_full_lights_11_leds() {
        let mut ring = LedRing::default();
        ring.set(Animation::Heatmap(12));
        let data = ring.render();
        let lit = data.iter().filter(|l| **l != RGB8::default()).count();
        assert_eq!(lit, 11);
    }

    #[test]
    fn test_heatmap_dead_zone_at_6_oclock() {
        let mut ring = LedRing::default();
        ring.set(Animation::Heatmap(12));
        let data = ring.render();
        // 6 o'clock = CLOCK[6] = idx 9
        assert_eq!(data[CLOCK[6]], RGB8::default());
    }

    #[test]
    fn test_heatmap_starts_at_7_oclock_ends_at_5_oclock() {
        let mut ring = LedRing::default();
        ring.set(Animation::Heatmap(12));
        let data = ring.render();
        // 7 o'clock = CLOCK[7] = idx 8, first in arc (blue)
        assert_eq!(data[CLOCK[7]], heatmap_color(0, 11));
        // 5 o'clock = CLOCK[5] = idx 10, last in arc (red)
        assert_eq!(data[CLOCK[5]], heatmap_color(10, 11));
    }

    #[test]
    fn test_heatmap_partial_fill() {
        let mut ring = LedRing::default();
        // fill=4 → 4*11/12=3 lit LEDs (7h, 8h, 9h)
        ring.set(Animation::Heatmap(4));
        let data = ring.render();
        let lit = data.iter().filter(|l| **l != RGB8::default()).count();
        assert_eq!(lit, 3);
        assert_ne!(data[CLOCK[7]], RGB8::default()); // 7h lit
        assert_ne!(data[CLOCK[8]], RGB8::default()); // 8h lit
        assert_ne!(data[CLOCK[9]], RGB8::default()); // 9h lit
        assert_eq!(data[CLOCK[10]], RGB8::default()); // 10h not yet
    }

    #[test]
    fn test_heatmap_color_gradient() {
        let c0 = heatmap_color(0, 11);
        assert_eq!(c0, RGB8::new(0, 0, 255)); // blue
        let c5 = heatmap_color(5, 11);
        assert!(c5.g > c5.r && c5.g > c5.b); // green-ish
        let c10 = heatmap_color(10, 11);
        assert!(c10.r > c10.g && c10.r > c10.b); // red-ish
    }
}
