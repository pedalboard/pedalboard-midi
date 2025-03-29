use midi2::BytesMessage;
use opendeck::config::SysExResponseIterator;

use crate::handler::Handler;
use crate::hmi::{
    inputs::{Edge, InputEvent, Pulse},
    leds::Leds,
};
use defmt::*;
use opendeck::button::handler::Action;
use opendeck::encoder::handler::EncoderPulse;
use opendeck::handler::Messages;

pub type OpenDeckConfig = opendeck::config::Config<2, 10, 2, 2, 8>;
pub type OpenDeckConfigResponses<'a> = SysExResponseIterator<'a, 2, 10, 2, 2, 8>;

pub struct OpenDeck {
    config: OpenDeckConfig,
    leds: Leds,
}

impl Handler for OpenDeck {
    fn handle_human_input<'a>(&mut self, event: InputEvent) -> Messages {
        match event {
            InputEvent::Vol(pulse) => self.config.handle_encoder(0, pulse.into()),
            InputEvent::Gain(pulse) => self.config.handle_encoder(1, pulse.into()),
            InputEvent::VolButton(a) => self.config.handle_button(0, a.into()),
            InputEvent::GainButton(a) => self.config.handle_button(1, a.into()),
            InputEvent::ButtonA(a) => self.config.handle_button(2, a.into()),
            InputEvent::ButtonB(a) => self.config.handle_button(3, a.into()),
            InputEvent::ButtonC(a) => self.config.handle_button(4, a.into()),
            InputEvent::ButtonD(a) => self.config.handle_button(5, a.into()),
            InputEvent::ButtonE(a) => self.config.handle_button(6, a.into()),
            InputEvent::ButtonF(a) => self.config.handle_button(7, a.into()),
            InputEvent::ExpressionPedalA(value) => self.config.handle_analog(0, value),
            InputEvent::ExpressionPedalB(value) => self.config.handle_analog(1, value),
        }
    }
    fn handle_midi_input(&mut self, _: &BytesMessage<&[u8]>) {}
    fn leds(&mut self) -> &mut Leds {
        &mut self.leds
    }
    fn process_sysex<'a>(&mut self, request: &[u8]) -> OpenDeckConfigResponses {
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

impl From<Pulse> for EncoderPulse {
    fn from(pulse: Pulse) -> Self {
        match pulse {
            Pulse::Clockwise => EncoderPulse::Clockwise,
            Pulse::CounterClockwise => EncoderPulse::CounterClockwise,
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
    warn!("Rebooting...");
    cortex_m::peripheral::SCB::sys_reset();
}
fn bootloader() {
    warn!("Rebooting to bootloader...");
    rp2040_hal::rom_data::reset_to_usb_boot(0, 0);
}
