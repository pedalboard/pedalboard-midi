use crate::devices::MidiMessages;
use heapless::Vec;
use midi_types::{Channel, Control, MidiMessage, Program, Value7};

const PLETHORA_CHANNEL: Channel = Channel::new(1);
const MAX_VALUE: Value7 = midi_types::Value7::new(127);
const MIN_VALUE: Value7 = midi_types::Value7::new(0);

#[allow(dead_code)]
pub enum Plethora {
    Board(u8),
    BoardUp,
    BoardDown,
    HotKnob(u8, Value7),
}
impl Plethora {
    pub fn midi_messages(&self) -> MidiMessages {
        let mut messages: MidiMessages = Vec::new();
        match *self {
            Plethora::BoardUp => messages
                .push(MidiMessage::ControlChange(
                    PLETHORA_CHANNEL,
                    Control::new(95),
                    MAX_VALUE,
                ))
                .unwrap(),
            Plethora::BoardDown => messages
                .push(MidiMessage::ControlChange(
                    PLETHORA_CHANNEL,
                    Control::new(94),
                    MAX_VALUE,
                ))
                .unwrap(),
            Plethora::Board(nr) => {
                messages
                    .push(MidiMessage::ControlChange(
                        PLETHORA_CHANNEL,
                        Control::new(102),
                        MIN_VALUE,
                    ))
                    .unwrap();
                messages
                    .push(MidiMessage::ControlChange(
                        PLETHORA_CHANNEL,
                        Control::new(103),
                        MIN_VALUE,
                    ))
                    .unwrap();
                messages
                    .push(MidiMessage::ControlChange(
                        PLETHORA_CHANNEL,
                        Control::new(104),
                        MIN_VALUE,
                    ))
                    .unwrap();
                messages
                    .push(MidiMessage::ProgramChange(
                        PLETHORA_CHANNEL,
                        Program::new(nr - 1),
                    ))
                    .unwrap();
            }
            Plethora::HotKnob(nr, value) => messages
                .push(MidiMessage::ControlChange(
                    PLETHORA_CHANNEL,
                    Control::new(106 + nr),
                    value,
                ))
                .unwrap(),
        };
        messages
    }
}
