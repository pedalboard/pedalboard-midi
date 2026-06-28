use crate::events::{Edge, InputEvent, Pulse};
use crate::ledring::Animation as RingAnim;
use crate::leds::{Led, LedData, LedRings, Leds};
use core::cell::RefCell;
use midi2::BytesMessage;
use opendeck::button::handler::Action;
use opendeck::config::SysexResponseIterator;
use opendeck::encoder::handler::EncoderPulse;
use opendeck::handler::Messages;
use opendeck::led::ControlType;
use rtic_sync::channel::Sender;
use smart_leds::colors::*;
use smart_leds::RGB8;

pub const PERSIST_CAPACITY: usize = 4;

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum PersistCommand {
    Save(u8, u8, u8, u16),
    SavePreset(u8, heapless::Vec<u8, 192>),
    SaveActivePreset(u8),
    EraseAll,
}

pub struct PedalboardHandler {
    persist_sender: RefCell<Sender<'static, PersistCommand, PERSIST_CAPACITY>>,
}

impl PedalboardHandler {
    pub fn new(persist_sender: Sender<'static, PersistCommand, PERSIST_CAPACITY>) -> Self {
        Self {
            persist_sender: RefCell::new(persist_sender),
        }
    }
}

impl opendeck::SystemHandler for PedalboardHandler {
    fn reboot(&self) {
        cortex_m::peripheral::SCB::sys_reset();
    }
    fn bootloader(&self) {
        rp2040_hal::rom_data::reset_to_usb_boot(0, 0);
    }
    fn factory_reset(&self) {
        self.persist_sender
            .borrow_mut()
            .try_send(PersistCommand::EraseAll)
            .ok();
    }
}

pub type OpenDeckConfig = opendeck::config::Config<32, 10, 2, 2, 10, PedalboardHandler>;
pub type OpenDeckConfigResponses = SysexResponseIterator<32, 10, 2, 2, 10>;

pub struct OpenDeck {
    pub config: OpenDeckConfig,
    leds: Leds,
}

impl OpenDeck {
    pub fn new(
        firmware_version: opendeck::config::FirmwareVersion,
        hardware_uid: u32,
        persist_sender: Sender<'static, PersistCommand, PERSIST_CAPACITY>,
    ) -> Self {
        use opendeck::analog::AnalogSection;
        use opendeck::encoder::EncoderSection;
        use opendeck::{Amount, Block, OpenDeckRequest, Wish};

        let mut config = opendeck::config::Config::new_with_adc_max(
            firmware_version,
            hardware_uid,
            PedalboardHandler::new(persist_sender),
            4095,
        );

        // Configure encoders: enabled, CC mode, pulses_per_step=1
        // (fallback when no PE config — generates CC#0-1 on ch1)
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
                Block::Encoder(i, EncoderSection::UpperLimit(127)),
            ));
            config.process_req(OpenDeckRequest::Configuration(
                Wish::Set,
                Amount::Single,
                Block::Encoder(
                    i,
                    EncoderSection::Accelleration(opendeck::encoder::Accelleration::Medium),
                ),
            ));
        }

        // Set encoder button velocity to 127
        use opendeck::button::{ButtonSection, ButtonType};
        for i in 0..2u16 {
            config.process_req(OpenDeckRequest::Configuration(
                Wish::Set,
                Amount::Single,
                Block::Button(i, ButtonSection::Value(127)),
            ));
        }

        // Set buttons D-F (indices 5-7) to latching mode
        for i in 5..8u16 {
            config.process_req(OpenDeckRequest::Configuration(
                Wish::Set,
                Amount::Single,
                Block::Button(i, ButtonSection::Type(ButtonType::Latching)),
            ));
        }

        // Enable analog inputs (fallback when no PE config — generates CC#2-3 on ch1)
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

        // Configure LED outputs for buttons A-C, E (notes 2-4, 6)
        use opendeck::led::LedSection;
        use opendeck::ChannelOrAll;
        for (idx, note) in [(0u16, 2u8), (1, 3), (2, 4), (4, 6)] {
            config.process_req(OpenDeckRequest::Configuration(
                Wish::Set,
                Amount::Single,
                Block::Led(
                    idx,
                    LedSection::ControlType(ControlType::LocalNoteSingleValue),
                ),
            ));
            config.process_req(OpenDeckRequest::Configuration(
                Wish::Set,
                Amount::Single,
                Block::Led(idx, LedSection::ActivationId(note)),
            ));
            config.process_req(OpenDeckRequest::Configuration(
                Wish::Set,
                Amount::Single,
                Block::Led(idx, LedSection::ActivationValue(1)),
            ));
            config.process_req(OpenDeckRequest::Configuration(
                Wish::Set,
                Amount::Single,
                Block::Led(idx, LedSection::Channel(ChannelOrAll::Channel(1))),
            ));
        }

        // Configure LED outputs 3(D) and 5(F) for expression pedals CC#2-3
        for (idx, cc) in [(3u16, 2u8), (5, 3)] {
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

        // Configure LED outputs 6-7: encoder CC → rings Vol/Gain
        for (idx, cc) in [(6u16, 0u8), (7, 1)] {
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

        // Configure LED outputs 8-9: encoder buttons → single LEDs
        for (idx, note) in [(8u16, 0u8), (9, 1)] {
            config.process_req(OpenDeckRequest::Configuration(
                Wish::Set,
                Amount::Single,
                Block::Led(
                    idx,
                    LedSection::ControlType(ControlType::LocalNoteSingleValue),
                ),
            ));
            config.process_req(OpenDeckRequest::Configuration(
                Wish::Set,
                Amount::Single,
                Block::Led(idx, LedSection::ActivationId(note)),
            ));
            config.process_req(OpenDeckRequest::Configuration(
                Wish::Set,
                Amount::Single,
                Block::Led(idx, LedSection::ActivationValue(127)),
            ));
            config.process_req(OpenDeckRequest::Configuration(
                Wish::Set,
                Amount::Single,
                Block::Led(idx, LedSection::Channel(ChannelOrAll::Channel(1))),
            ));
        }

        // Set LED colors - each ring a different color
        use opendeck::led::Color;
        let colors = [
            Color::Red,     // 0 - A
            Color::Green,   // 1 - B
            Color::Blue,    // 2 - C
            Color::Yellow,  // 3 - D
            Color::Magenta, // 4 - E
            Color::Cyan,    // 5 - F
            Color::White,   // 6 - Vol
            Color::Red,     // 7 - Gain
        ];
        for (i, &c) in colors.iter().enumerate() {
            config.set_output_color(i, c);
        }
        config.set_output_color(8, Color::Green);
        config.set_output_color(9, Color::Blue);

        // Reset encoder values to 0
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
            .set_ledring(RingAnim::Fill(RGB8::default(), 0), LedRings::Vol);
        self.leds
            .set_ledring(RingAnim::Fill(RGB8::default(), 0), LedRings::Gain);
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

    /// Render a component info SysEx message for the given input event.
    /// Returns None if SysEx session is not active.
    pub fn component_info(&self, event: &InputEvent, buf: &mut [u8]) -> Option<usize> {
        use midi2::prelude::*;
        use opendeck::renderer::OpenDeckRenderer;
        use opendeck::{BlockId, ValueSize};

        if !self.config.sysex_enabled() {
            return None;
        }
        let (block, index) = match event {
            InputEvent::Vol(_) => (BlockId::Encoder, 0u16),
            InputEvent::Gain(_) => (BlockId::Encoder, 1),
            InputEvent::VolButton(_) => (BlockId::Button, 0),
            InputEvent::GainButton(_) => (BlockId::Button, 1),
            InputEvent::ButtonA(_) => (BlockId::Button, 2),
            InputEvent::ButtonB(_) => (BlockId::Button, 3),
            InputEvent::ButtonC(_) => (BlockId::Button, 4),
            InputEvent::ButtonD(_) => (BlockId::Button, 5),
            InputEvent::ButtonE(_) => (BlockId::Button, 6),
            InputEvent::ButtonF(_) => (BlockId::Button, 7),
            InputEvent::ExpressionPedalA(_) => (BlockId::Analog, 0),
            InputEvent::ExpressionPedalB(_) => (BlockId::Analog, 1),
        };
        let renderer = OpenDeckRenderer::new(ValueSize::TwoBytes, buf);
        if let Ok(Some(sysex)) = renderer.render_component_info(block, index) {
            Some(sysex.data().len())
        } else {
            None
        }
    }

    /// Process local MIDI and update LED state. Returns rendered LED data.
    pub fn notify_local_midi(&mut self, raw: &[u8]) -> LedData {
        if raw.len() >= 3 {
            let is_cc = (raw[0] & 0xF0) == 0xB0;
            if let Some((channel, id, value, is_note_on)) = parse_midi_raw(raw) {
                self.config
                    .notify_local_midi(channel, id, value, is_note_on, is_cc);
            }
        }
        self.update_leds();
        self.leds.render()
    }

    /// Process an incoming external MIDI message and update LED state. Returns rendered LED data.
    pub fn handle_midi_input(&mut self, m: &BytesMessage<&[u8]>) -> LedData {
        // Flash Mon LED on external MIDI
        self.leds.set_single(Led::Mon, Some(DARK_BLUE));
        if let Some((channel, id, value, is_note_on, is_cc)) = parse_bytes_message(m) {
            self.config
                .notify_external_midi(channel, id, value, is_note_on, is_cc);
        }
        self.update_leds();
        self.leds.render()
    }

    pub fn process_sysex(&mut self, request: &[u8]) -> OpenDeckConfigResponses {
        self.config.process_sysex(request)
    }

    /// Whether locally-generated MIDI should be sent on DIN MIDI out.
    pub fn din_midi_enabled(&self) -> bool {
        self.config.global_midi().din_midi_enabled()
    }

    /// Whether incoming DIN MIDI should be forwarded to USB MIDI out.
    pub fn din_to_usb_thru(&self) -> bool {
        self.config.global_midi().din_to_usb_thru()
    }

    /// Whether incoming USB MIDI should be forwarded to DIN MIDI out.
    pub fn usb_to_din_thru(&self) -> bool {
        self.config.global_midi().usb_to_din_thru()
    }

    /// Whether incoming USB MIDI should be forwarded back to USB MIDI out.
    pub fn usb_to_usb_thru(&self) -> bool {
        self.config.global_midi().usb_to_usb_thru()
    }

    /// Render current LED state (for use after sysex config changes).
    pub fn render_leds(&self) -> LedData {
        self.leds.render()
    }

    /// Turn off Mon LED (called by flash timeout).
    pub fn clear_mon(&mut self) -> LedData {
        self.leds.set_single(Led::Mon, None);
        self.leds.render()
    }

    /// Update LED structs from config output state.
    fn update_leds(&mut self) {
        const RINGS: [LedRings; 8] = [
            LedRings::A,
            LedRings::B,
            LedRings::C,
            LedRings::D,
            LedRings::E,
            LedRings::F,
            LedRings::Vol,
            LedRings::Gain,
        ];
        const SINGLE_LEDS: [Led; 2] = [Led::Mode, Led::Mon];

        for (i, ring) in RINGS
            .iter()
            .enumerate()
            .take(self.config.output_count().min(8))
        {
            let ct = self.config.output_control_type(i);
            let rgb = color_to_rgb(self.config.output_color(i));
            let is_multi = matches!(
                ct,
                ControlType::LocalCcMultiValue
                    | ControlType::MidiInCcMultiValue
                    | ControlType::LocalNoteMultiValue
                    | ControlType::MidiInNoteMultiValue
            );
            if is_multi {
                let level = self.config.output_level(i);
                let fill = ((level as u16 * 12) / 127).min(12) as u8;
                self.leds.set_ledring(RingAnim::Heatmap(fill), *ring);
            } else {
                let is_static = ct == ControlType::Static;
                let on = is_static || self.config.output_state(i);
                self.leds
                    .set_ledring(if on { RingAnim::On(rgb) } else { RingAnim::Off }, *ring);
            }
        }
        // Outputs 8-9 → single LEDs
        for (i, single_led) in SINGLE_LEDS.iter().enumerate() {
            let idx = 8 + i;
            if idx < self.config.output_count() {
                let on = self.config.output_state(idx);
                let rgb = color_to_rgb(self.config.output_color(idx));
                self.leds
                    .set_single(*single_led, if on { Some(rgb) } else { None });
            }
        }
    }
}

fn color_to_rgb(c: opendeck::led::Color) -> RGB8 {
    use opendeck::led::Color;
    match c {
        Color::Off => RGB8::new(0, 0, 0),
        Color::Red => RGB8::new(255, 0, 0),
        Color::Green => RGB8::new(0, 255, 0),
        Color::Yellow => RGB8::new(255, 255, 0),
        Color::Blue => RGB8::new(0, 0, 255),
        Color::Magenta => RGB8::new(255, 0, 255),
        Color::Cyan => RGB8::new(0, 255, 255),
        Color::White => RGB8::new(255, 255, 255),
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
        0x90 => Some((channel, id, value, false)),
        0x80 => Some((channel, id, value, false)),
        0xB0 => Some((channel, id, value, true)),
        _ => None,
    }
}

fn parse_bytes_message(m: &BytesMessage<&[u8]>) -> Option<(u8, u8, u8, bool, bool)> {
    use midi2::prelude::*;
    match m {
        BytesMessage::ChannelVoice1(cv) => {
            use midi2::channel_voice1::ChannelVoice1;
            match cv {
                ChannelVoice1::NoteOn(n) => {
                    let ch: u8 = n.channel().into();
                    Some((
                        ch + 1,
                        n.note_number().into(),
                        n.velocity().into(),
                        true,
                        false,
                    ))
                }
                ChannelVoice1::NoteOff(n) => {
                    let ch: u8 = n.channel().into();
                    Some((
                        ch + 1,
                        n.note_number().into(),
                        n.velocity().into(),
                        false,
                        false,
                    ))
                }
                ChannelVoice1::ControlChange(cc) => {
                    let ch: u8 = cc.channel().into();
                    Some((
                        ch + 1,
                        cc.control().into(),
                        cc.control_data().into(),
                        true,
                        true,
                    ))
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

    fn test_config() -> OpenDeck {
        OpenDeck::new(
            opendeck::config::FirmwareVersion {
                major: 1,
                minor: 0,
                revision: 0,
            },
            0x123456,
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
        use opendeck::led::LedSection;
        use opendeck::ChannelOrAll;
        use opendeck::{Amount, Block, OpenDeckRequest, Wish};

        let mut od = test_config();

        od.config.process_req(OpenDeckRequest::Configuration(
            Wish::Set,
            Amount::Single,
            Block::Led(
                0,
                LedSection::ControlType(ControlType::LocalNoteSingleValue),
            ),
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

        od.notify_local_midi(&[0x90, 60, 127]);
        assert!(od.config.output_state(0));

        od.notify_local_midi(&[0x80, 60, 0]);
        assert!(!od.config.output_state(0));
    }

    /// Routing defaults: DIN off, all thru off
    #[test]
    fn test_routing_defaults() {
        let od = test_config();
        // Per wiki: all routing defaults are 0 (disabled)
        assert!(!od.din_midi_enabled());
        assert!(!od.din_to_usb_thru());
        assert!(!od.usb_to_din_thru());
        assert!(!od.usb_to_usb_thru());
    }

    /// Routing can be enabled via SysEx configuration
    #[test]
    fn test_routing_enable_via_config() {
        use opendeck::global::{GlobalSection, MidiIndex};
        use opendeck::{Amount, Block, OpenDeckRequest, Wish};

        let mut od = test_config();

        // Enable DIN MIDI state
        od.config.process_req(OpenDeckRequest::Configuration(
            Wish::Set,
            Amount::Single,
            Block::Global(GlobalSection::Midi(MidiIndex::DINMIDIstate, 1)),
        ));
        assert!(od.din_midi_enabled());

        // Enable DIN→USB thru
        od.config.process_req(OpenDeckRequest::Configuration(
            Wish::Set,
            Amount::Single,
            Block::Global(GlobalSection::Midi(MidiIndex::DINtoUSBthru, 1)),
        ));
        assert!(od.din_to_usb_thru());

        // Enable USB→DIN thru
        od.config.process_req(OpenDeckRequest::Configuration(
            Wish::Set,
            Amount::Single,
            Block::Global(GlobalSection::Midi(MidiIndex::USBtoDINthru, 1)),
        ));
        assert!(od.usb_to_din_thru());

        // Enable USB→USB thru
        od.config.process_req(OpenDeckRequest::Configuration(
            Wish::Set,
            Amount::Single,
            Block::Global(GlobalSection::Midi(MidiIndex::USBtoUSBthru, 1)),
        ));
        assert!(od.usb_to_usb_thru());
    }
}
