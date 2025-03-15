use midi2::BytesMessage;
use opendeck::button::handler::Action;

use crate::handler::Handler;
use crate::hmi::{
    inputs::{Edge, InputEvent},
    leds::Leds,
};
use opendeck::handler::Messages;

pub type OpenDeckConfig = opendeck::config::Config<2, 8, 2, 2, 8>;

pub struct OpenDeck {
    config: OpenDeckConfig,
    leds: Leds,
}

impl Handler for OpenDeck {
    fn handle_human_input<'a>(&mut self, event: InputEvent) -> Messages {
        if let Some(preset) = self.config.current_preset_mut() {
            match event {
                InputEvent::ButtonA(a) => {
                    if let Some(button) = preset.button_mut(&0) {
                        return Messages::Button(button.handle(a.into()));
                    }
                    Messages::None
                }
                _ => Messages::None,
            }
        } else {
            Messages::None
        }
    }
    fn handle_midi_input(&mut self, _: &BytesMessage<&[u8]>) {}
    fn leds(&mut self) -> &mut Leds {
        &mut self.leds
    }
    fn process_sysex(&mut self, request: &[u8]) -> opendeck::config::Responses {
        self.config.process_sysex(request)
    }
}

impl From<Edge> for Action {
    fn from(edge: Edge) -> Self {
        match edge {
            Edge::Activate => Action::Pressed,
            Edge::Deactivate => Action::Released,
        }
    }
}

impl OpenDeck {
    pub fn new() -> Self {
        let leds = Leds::default();
        let config =
            opendeck::config::Config::new(firmware_version(), 0x123456, reboot, bootloader);

        OpenDeck { leds, config }
    }
}

impl Default for OpenDeck {
    fn default() -> Self {
        OpenDeck::new()
    }
}

fn firmware_version() -> opendeck::config::FirmwareVersion {
    opendeck::config::FirmwareVersion {
        major: 1,
        minor: 0,
        revision: 0,
    }
}

fn reboot() {
    cortex_m::peripheral::SCB::sys_reset();
}
fn bootloader() {
    rp2040_hal::rom_data::reset_to_usb_boot(0, 0);
}
