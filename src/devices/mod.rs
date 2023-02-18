mod plethora;
mod rc500;

use heapless::Vec;
use midi_types::MidiMessage;

type MidiMessages = Vec<MidiMessage, 8>;
