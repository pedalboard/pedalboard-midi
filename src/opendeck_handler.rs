use crate::events::{Edge, InputEvent, Pulse};
use crate::leds::Leds;
use midi2::BytesMessage;
use opendeck::button::handler::Action;
use opendeck::config::SysexResponseIterator;
use opendeck::encoder::handler::EncoderPulse;
use opendeck::handler::Messages;

pub type OpenDeckConfig = opendeck::config::Config<2, 10, 2, 2, 8>;
pub type OpenDeckConfigResponses = SysexResponseIterator<2, 10, 2, 2, 8>;

pub struct OpenDeck {
    pub config: OpenDeckConfig,
    pub leds: Leds,
}

impl OpenDeck {
    pub fn new(
        firmware_version: opendeck::config::FirmwareVersion,
        hardware_uid: u32,
        reboot: fn(),
        bootloader: fn(),
    ) -> Self {
        OpenDeck {
            leds: Leds::default(),
            config: opendeck::config::Config::new(firmware_version, hardware_uid, reboot, bootloader),
        }
    }

    pub fn handle_human_input(&mut self, event: InputEvent) -> Messages<'_> {
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

    pub fn handle_midi_input(&mut self, _: &BytesMessage<&[u8]>) {}

    pub fn process_sysex(&mut self, request: &[u8]) -> OpenDeckConfigResponses {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn noop() {}

    fn test_config() -> OpenDeck {
        OpenDeck::new(
            opendeck::config::FirmwareVersion { major: 1, minor: 0, revision: 0 },
            0x123456,
            noop,
            noop,
        )
    }

    #[test]
    fn test_button_press_produces_message() {
        let mut od = test_config();
        let mut buf = [0u8; 6];
        let mut messages = od.handle_human_input(InputEvent::ButtonA(Edge::Activate));
        let result = messages.next(&mut buf);
        assert!(result.is_ok());
        // Default button config sends Note On
        assert!(result.unwrap().is_some());
    }

    #[test]
    fn test_expression_pedal_disabled_by_default() {
        let mut od = test_config();
        let mut buf = [0u8; 6];
        let mut messages = od.handle_human_input(InputEvent::ExpressionPedalA(2048));
        let result = messages.next(&mut buf);
        assert!(result.is_ok());
        // Analog is disabled by default, no message
        assert!(result.unwrap().is_none());
    }
}
