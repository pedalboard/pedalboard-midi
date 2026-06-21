use crate::events::{Edge, InputEvent, Pulse};
use crate::ledring::Animation as RingAnim;
use crate::leds::{Animation::Flash, Led, LedRings, Leds};
use midi2::BytesMessage;
use opendeck::button::handler::Action;
use opendeck::config::SysexResponseIterator;
use opendeck::encoder::handler::EncoderPulse;
use opendeck::handler::Messages;
use smart_leds::colors::*;

pub type OpenDeckConfig = opendeck::config::Config<2, 10, 2, 2, 10>;
pub type OpenDeckConfigResponses = SysexResponseIterator<2, 10, 2, 2, 10>;

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
        use opendeck::analog::AnalogSection;
        use opendeck::encoder::EncoderSection;
        use opendeck::{Amount, Block, OpenDeckRequest, Wish};

        let mut config =
            opendeck::config::Config::new(firmware_version, hardware_uid, reboot, bootloader);

        // Configure encoders: enabled, CC mode, pulses_per_step=1, CC#0-1
        for i in 0..2u16 {
            config.process_req(OpenDeckRequest::Configuration(
                Wish::Set,
                Amount::Single,
                Block::Encoder(i, EncoderSection::Enabled(true)),
            ));
            config.process_req(OpenDeckRequest::Configuration(
                Wish::Set,
                Amount::Single,
                Block::Encoder(
                    i,
                    EncoderSection::MessageType(
                        opendeck::encoder::EncoderMessageType::ControlChange,
                    ),
                ),
            ));
            config.process_req(OpenDeckRequest::Configuration(
                Wish::Set,
                Amount::Single,
                Block::Encoder(i, EncoderSection::PulsesPerStep(1)),
            ));
            config.process_req(OpenDeckRequest::Configuration(
                Wish::Set,
                Amount::Single,
                Block::Encoder(i, EncoderSection::UpperLimit(24)),
            ));
        }

        // Enable analog inputs with CC#2-3
        for i in 0..2u16 {
            config.process_req(OpenDeckRequest::Configuration(
                Wish::Set,
                Amount::Single,
                Block::Analog(i, AnalogSection::Enabled(true)),
            ));
            config.process_req(OpenDeckRequest::Configuration(
                Wish::Set,
                Amount::Single,
                Block::Analog(i, AnalogSection::MidiId(i + 2)),
            ));
            config.process_req(OpenDeckRequest::Configuration(
                Wish::Set,
                Amount::Single,
                Block::Analog(i, AnalogSection::UpperADCOffset(14)),
            ));
        }

        // Configure LED outputs 0-5 to react to buttons A-F (notes 2-7)
        use opendeck::led::{ControlType, LedSection};
        use opendeck::ChannelOrAll;
        for i in 0..6u16 {
            let note = (i + 2) as u8;
            config.process_req(OpenDeckRequest::Configuration(
                Wish::Set,
                Amount::Single,
                Block::Led(i, LedSection::ControlType(ControlType::LocalNoteSingleValue)),
            ));
            config.process_req(OpenDeckRequest::Configuration(
                Wish::Set,
                Amount::Single,
                Block::Led(i, LedSection::ActivationId(note)),
            ));
            config.process_req(OpenDeckRequest::Configuration(
                Wish::Set,
                Amount::Single,
                Block::Led(i, LedSection::ActivationValue(1)),
            ));
            config.process_req(OpenDeckRequest::Configuration(
                Wish::Set,
                Amount::Single,
                Block::Led(i, LedSection::Channel(ChannelOrAll::Channel(1))),
            ));
        }

        // Configure LED outputs 6-9 for CC visualization (LocalCcMultiValue)
        // 6=Vol(CC#0), 7=Gain(CC#1), 8=ExpA(CC#2), 9=ExpB(CC#3)
        for (idx, cc) in [(6u16, 0u8), (7, 1), (8, 2), (9, 3)] {
            config.process_req(OpenDeckRequest::Configuration(
                Wish::Set,
                Amount::Single,
                Block::Led(idx, LedSection::ControlType(ControlType::LocalCcMultiValue)),
            ));
            config.process_req(OpenDeckRequest::Configuration(
                Wish::Set,
                Amount::Single,
                Block::Led(idx, LedSection::ActivationId(cc)),
            ));
            config.process_req(OpenDeckRequest::Configuration(
                Wish::Set,
                Amount::Single,
                Block::Led(idx, LedSection::Channel(ChannelOrAll::Channel(1))),
            ));
        }

        // Reset encoder values to 0 (boot pin glitches may have incremented them)
        for i in 0..2u16 {
            config.process_req(OpenDeckRequest::Configuration(
                Wish::Set,
                Amount::Single,
                Block::Encoder(i, EncoderSection::RepeatedValue(0)),
            ));
        }

        OpenDeck {
            leds: Leds::default(),
            config,
        }
    }

    /// Reset encoder rings to empty after boot glitches settle.
    pub fn reset_encoder_rings(&mut self) {
        self.leds
            .set_ledring(RingAnim::Fill(CYAN, 0), LedRings::Vol);
        self.leds
            .set_ledring(RingAnim::Fill(CYAN, 0), LedRings::Gain);
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

    /// Call after consuming a locally-generated MIDI message to update LED outputs.
    pub fn notify_local_midi(&mut self, raw: &[u8]) {
        if raw.len() < 3 {
            return;
        }
        if let Some((channel, id, value, is_note_on)) = parse_midi_raw(raw) {
            let changed = self.config.notify_local_midi(channel, id, value, is_note_on);
            if changed > 0 {
                self.sync_output_leds();
            }
        }
    }

    /// Process an incoming external MIDI message and update LED outputs.
    pub fn handle_midi_input(&mut self, m: &BytesMessage<&[u8]>) {
        self.leds.set(Flash(DARK_BLUE), Led::Mon);
        if let Some((channel, id, value, is_note_on)) = parse_bytes_message(m) {
            let changed = self.config.notify_external_midi(channel, id, value, is_note_on);
            if changed > 0 {
                self.sync_output_leds();
            }
        }
    }

    pub fn process_sysex(&mut self, request: &[u8]) -> OpenDeckConfigResponses {
        self.config.process_sysex(request)
    }

    /// Sync opendeck output states to physical LED rings.
    fn sync_output_leds(&mut self) {
        // Outputs 0-5: buttons A-F (on/off green)
        const BTN_RINGS: [LedRings; 6] = [
            LedRings::A, LedRings::B, LedRings::C,
            LedRings::D, LedRings::E, LedRings::F,
        ];
        for i in 0..6 {
            let on = self.config.output_state(i);
            self.leds.set_ledring(
                if on { RingAnim::On(GREEN) } else { RingAnim::Off },
                BTN_RINGS[i],
            );
        }
        // Outputs 6-7: Encoder level → single LEDs (Mode=Vol, Mon=Gain)
        use crate::leds::Animation;
        let vol_level = self.config.output_level(6);
        let gain_level = self.config.output_level(7);
        self.leds.set(
            if vol_level > 0 { Animation::On(CYAN) } else { Animation::Off },
            Led::Mode,
        );
        self.leds.set(
            if gain_level > 0 { Animation::On(CYAN) } else { Animation::Off },
            Led::Mon,
        );
        // Outputs 8-9: Expression pedal level → rings D, F
        const CC_RINGS: [(LedRings, Option<usize>, u16); 2] = [
            (LedRings::D, Some(3), 127),
            (LedRings::F, Some(5), 127),
        ];
        for (i, &(ring, btn, max)) in CC_RINGS.iter().enumerate() {
            if let Some(b) = btn {
                if self.config.output_state(b) {
                    continue;
                }
            }
            let level = self.config.output_level(8 + i);
            let fill = ((level as u16 * 12) / max).min(12) as u8;
            self.leds.set_ledring(RingAnim::Fill(CYAN, fill), ring);
        }
    }
}

fn parse_midi_raw(raw: &[u8]) -> Option<(u8, u8, u8, bool)> {
    if raw.len() < 3 {
        return None;
    }
    let status = raw[0] & 0xF0;
    let channel = (raw[0] & 0x0F) + 1;
    let id = raw[1];
    let value = raw[2];
    match status {
        0x90 if value > 0 => Some((channel, id, value, true)),
        0x90 => Some((channel, id, value, false)), // velocity 0 = note off
        0x80 => Some((channel, id, value, false)),
        0xB0 => Some((channel, id, value, true)),
        _ => None,
    }
}

fn parse_bytes_message(m: &BytesMessage<&[u8]>) -> Option<(u8, u8, u8, bool)> {
    use midi2::prelude::*;
    match m {
        BytesMessage::ChannelVoice1(cv) => {
            use midi2::channel_voice1::ChannelVoice1;
            match cv {
                ChannelVoice1::NoteOn(n) => {
                    let ch: u8 = u4::from(n.channel()).into();
                    Some((ch + 1, n.note_number().into(), n.velocity().into(), true))
                }
                ChannelVoice1::NoteOff(n) => {
                    let ch: u8 = u4::from(n.channel()).into();
                    Some((ch + 1, n.note_number().into(), n.velocity().into(), false))
                }
                ChannelVoice1::ControlChange(cc) => {
                    let ch: u8 = u4::from(cc.channel()).into();
                    Some((ch + 1, cc.control().into(), cc.control_data().into(), true))
                }
                _ => None,
            }
        }
        _ => None,
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
            opendeck::config::FirmwareVersion {
                major: 1,
                minor: 0,
                revision: 0,
            },
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
        assert!(result.unwrap().is_some());
    }

    #[test]
    fn test_expression_pedal_produces_cc() {
        let mut od = test_config();
        let mut buf = [0u8; 6];
        let mut messages = od.handle_human_input(InputEvent::ExpressionPedalA(2048));
        let result = messages.next(&mut buf);
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }

    #[test]
    fn test_notify_local_midi_updates_output() {
        use opendeck::led::{ControlType, LedSection};
        use opendeck::ChannelOrAll;
        use opendeck::{Amount, Block, OpenDeckRequest, Wish};

        let mut od = test_config();

        // Configure LED output 0: LocalNoteSingleValue, note 60, value 127, channel 1
        od.config.process_req(OpenDeckRequest::Configuration(
            Wish::Set,
            Amount::Single,
            Block::Led(0, LedSection::ControlType(ControlType::LocalNoteSingleValue)),
        ));
        od.config.process_req(OpenDeckRequest::Configuration(
            Wish::Set,
            Amount::Single,
            Block::Led(0, LedSection::ActivationId(60)),
        ));
        od.config.process_req(OpenDeckRequest::Configuration(
            Wish::Set,
            Amount::Single,
            Block::Led(0, LedSection::ActivationValue(127)),
        ));
        od.config.process_req(OpenDeckRequest::Configuration(
            Wish::Set,
            Amount::Single,
            Block::Led(0, LedSection::Channel(ChannelOrAll::Channel(1))),
        ));

        assert!(!od.config.output_state(0));

        // Simulate local Note On: channel 1, note 60, velocity 127
        od.notify_local_midi(&[0x90, 60, 127]);
        assert!(od.config.output_state(0));

        // Simulate local Note Off
        od.notify_local_midi(&[0x80, 60, 0]);
        assert!(!od.config.output_state(0));
    }

}
