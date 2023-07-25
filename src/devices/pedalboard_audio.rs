use crate::handler::MidiMessages;
use midi_types::{Channel, Control, MidiMessage, Value7};

const CHANNEL: Channel = Channel::new(2);
const MAX_PROCESSORS: usize = 10;

pub struct PedalboardAudio {
    bypass_status: [bool; MAX_PROCESSORS],
}

#[allow(dead_code)]
pub enum PAAction {
    OutputLevel(Value7),
    BypassProcessor(u8),
}

impl Default for PedalboardAudio {
    fn default() -> Self {
        Self::new()
    }
}

impl PedalboardAudio {
    pub fn new() -> Self {
        Self {
            bypass_status: [false; MAX_PROCESSORS],
        }
    }
    pub fn midi_messages(&mut self, act: PAAction) -> MidiMessages {
        match act {
            PAAction::OutputLevel(value) => control_change(Control::new(100), value),
            PAAction::BypassProcessor(processor) => {
                let i = (processor as usize) % MAX_PROCESSORS;
                self.bypass_status[i] = !self.bypass_status[i];
                let value = match self.bypass_status[i] {
                    true => 0,
                    false => 127,
                };
                control_change(Control::new(i as u8), Value7::new(value))
            }
        }
    }
}

fn control_change(control: Control, value: Value7) -> MidiMessages {
    let mut messages = MidiMessages::none();
    messages.push(MidiMessage::ControlChange(CHANNEL, control, value));
    messages
}
