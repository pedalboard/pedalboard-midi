//! LED ring rendering — thin re-export from midi-controller with smart_leds bridge.

use smart_leds::RGB8;

pub use midi_controller::led::{LedRing, Modifier, Renderer, Rgb, RingAnimation, LEDS_PER_RING};

pub type LedData = [RGB8; LEDS_PER_RING];

/// Legacy Animation enum — bridges old call sites to new RingAnimation.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum Animation {
    On(RGB8),
    Off,
    Fill(RGB8, u8),
    Heatmap(u8),
}

impl Animation {
    pub fn to_ring_animation(self) -> RingAnimation {
        match self {
            Animation::On(c) => RingAnimation::solid(rgb8_to_rgb(c)),
            Animation::Off => RingAnimation::off(),
            Animation::Fill(c, n) => RingAnimation {
                renderer: Renderer::Fill(rgb8_to_rgb(c), n),
                modifier: Modifier::Solid,
            },
            Animation::Heatmap(fill) => RingAnimation {
                renderer: Renderer::Heatmap(fill),
                modifier: Modifier::Solid,
            },
        }
    }
}

pub fn rgb_to_rgb8(c: Rgb) -> RGB8 {
    RGB8::new(c.r, c.g, c.b)
}

pub fn rgb8_to_rgb(c: RGB8) -> Rgb {
    Rgb::new(c.r, c.g, c.b)
}

/// Render a LedRing to smart_leds format.
pub fn render_ring(ring: &LedRing, tick: u16) -> LedData {
    let frame = ring.render(tick);
    let mut out = [RGB8::default(); LEDS_PER_RING];
    for (i, px) in frame.iter().enumerate() {
        out[i] = rgb_to_rgb8(*px);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_on_converts() {
        let anim = Animation::On(RGB8::new(255, 0, 0));
        let ra = anim.to_ring_animation();
        assert_eq!(ra.renderer, Renderer::Solid(Rgb::new(255, 0, 0)));
        assert_eq!(ra.modifier, Modifier::Solid);
    }

    #[test]
    fn legacy_off_converts() {
        let ra = Animation::Off.to_ring_animation();
        assert_eq!(ra.renderer, Renderer::Off);
    }

    #[test]
    fn render_ring_output_matches() {
        let mut ring = LedRing::default();
        ring.set(RingAnimation::solid(Rgb::new(100, 50, 25)));
        let data = render_ring(&ring, 0);
        assert!(data
            .iter()
            .all(|px| px.r == 100 && px.g == 50 && px.b == 25));
    }
}
