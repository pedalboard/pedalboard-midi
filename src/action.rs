//! Re-export action executor from pedalboard-protocol.
pub use pedalboard_protocol::action::{
    action_to_midi, analog_cc, encoder_cc, execute_button_press, EncoderDirection, MidiMessage,
};
