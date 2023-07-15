use crate::devices::MidiMessages;
use midi_types::{Channel, Control, MidiMessage, Value7};

const CHANNEL: Channel = Channel::new(2);

pub struct PedalboardAudio {}

#[allow(dead_code)]
pub enum PAAction {
    OutputLevel(Value7),
    BypassProcessor(Control, bool),
}

impl Default for PedalboardAudio {
    fn default() -> Self {
        Self::new()
    }
}

impl PedalboardAudio {
    pub fn new() -> Self {
        Self {}
    }
    pub fn midi_messages(&mut self, act: PAAction) -> MidiMessages {
        match act {
            PAAction::OutputLevel(value) => control_change(Control::new(100), value),
            PAAction::BypassProcessor(control, true) => control_change(control, Value7::new(0)),
            PAAction::BypassProcessor(control, false) => control_change(control, Value7::new(127)),
        }
    }
}

fn control_change(control: Control, value: Value7) -> MidiMessages {
    let mut messages = MidiMessages::none();
    messages.push(MidiMessage::ControlChange(CHANNEL, control, value));
    messages
}
