use crate::devices::{Direction, MidiMessages};
use heapless::Vec;
use midi_types::{Channel, Control, MidiMessage, Program, Value7};

const PLETHORA_CHANNEL: Channel = Channel::new(1);
const MAX_VALUE: Value7 = midi_types::Value7::new(127);
const MIN_VALUE: Value7 = midi_types::Value7::new(0);

#[allow(dead_code)]
pub enum PlethoraEvent {
    GoToBoard(u8),
    Board(Direction),
    HotKnob(u8, Value7),
}

pub struct Plethora {}

impl Plethora {
    pub fn midi_messages(&self, event: PlethoraEvent) -> MidiMessages {
        let mut messages: MidiMessages = Vec::new();
        match event {
            PlethoraEvent::Board(dir) => match dir {
                Direction::Up => {
                    messages
                        .push(MidiMessage::ControlChange(
                            PLETHORA_CHANNEL,
                            Control::new(95),
                            MAX_VALUE,
                        ))
                        .unwrap();
                }
                Direction::Down => {
                    messages
                        .push(MidiMessage::ControlChange(
                            PLETHORA_CHANNEL,
                            Control::new(94),
                            MAX_VALUE,
                        ))
                        .unwrap();
                }
            },
            PlethoraEvent::GoToBoard(nr) => {
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
            PlethoraEvent::HotKnob(nr, value) => messages
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
