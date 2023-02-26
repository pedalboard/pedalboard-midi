use crate::devices::MidiMessages;
use midi_types::{Channel, Control, MidiMessage, Value7};

const CHANNEL: Channel = Channel::new(2);

pub struct PedalboardAudio {}

pub enum PAEvent {
    #[allow(dead_code)]
    OutputLevel(Value7),
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
    pub fn midi_messages(&mut self, event: PAEvent) -> MidiMessages {
        match event {
            PAEvent::OutputLevel(value) => control_change(Control::new(7), value),
        }
    }
}

fn control_change(control: Control, value: Value7) -> MidiMessages {
    let mut messages = MidiMessages::none();
    messages.push(MidiMessage::ControlChange(CHANNEL, control, value));
    messages
}
