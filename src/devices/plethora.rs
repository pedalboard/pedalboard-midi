use crate::devices::{Direction, MidiMessages};
use midi_types::{Channel, Control, MidiMessage, Program, Value7};

const PLETHORA_CHANNEL: Channel = Channel::new(1);
const MAX_V7: Value7 = midi_types::Value7::new(127);
const MIN_V7: Value7 = midi_types::Value7::new(0);

#[allow(dead_code)]
pub enum PlethoraAction {
    GoToBoard(u8),
    Board(Direction),
    HotKnob(u8, Value7),
}

pub struct Plethora {}

impl Plethora {
    pub fn midi_messages(&self, act: PlethoraAction) -> MidiMessages {
        let mut messages: MidiMessages = MidiMessages::none();
        match act {
            PlethoraAction::Board(dir) => match dir {
                Direction::Up => {
                    messages.push(MidiMessage::ControlChange(
                        PLETHORA_CHANNEL,
                        Control::new(95),
                        MAX_V7,
                    ));
                }
                Direction::Down => {
                    messages.push(MidiMessage::ControlChange(
                        PLETHORA_CHANNEL,
                        Control::new(94),
                        MAX_V7,
                    ));
                }
            },
            PlethoraAction::GoToBoard(nr) => {
                messages.push(MidiMessage::ControlChange(
                    PLETHORA_CHANNEL,
                    Control::new(102),
                    MIN_V7,
                ));
                messages.push(MidiMessage::ControlChange(
                    PLETHORA_CHANNEL,
                    Control::new(103),
                    MIN_V7,
                ));
                messages.push(MidiMessage::ControlChange(
                    PLETHORA_CHANNEL,
                    Control::new(104),
                    MIN_V7,
                ));
                messages.push(MidiMessage::ProgramChange(
                    PLETHORA_CHANNEL,
                    Program::new(nr - 1),
                ));
            }
            PlethoraAction::HotKnob(nr, value) => messages.push(MidiMessage::ControlChange(
                PLETHORA_CHANNEL,
                Control::new(106 + nr),
                value,
            )),
        };
        messages
    }
}
