use crate::handler::{Actions, Handler};
use crate::hmi::{inputs::InputEvent, leds::Leds};

pub struct OpenDeck {
    leds: Leds,
}

impl Handler for OpenDeck {
    fn handle_human_input(&mut self, event: InputEvent) -> Actions {
        match event {
            InputEvent::ButtonA(_) => Actions::none(),
            _ => Actions::none(),
        }
    }
    fn leds(&mut self) -> &mut Leds {
        &mut self.leds
    }
}

impl OpenDeck {
    pub fn new() -> Self {
        let leds = Leds::default();

        OpenDeck { leds }
    }
}

impl Default for OpenDeck {
    fn default() -> Self {
        Self::new()
    }
}
