use crate::handler::opendeck_handler::OpenDeck;
use crate::hmi::inputs::InputEvent;
use crate::hmi::leds::{Animation::Flash, Led, LedRings, Leds};
use defmt::*;
use midi2::prelude::*;
use midi2::{error::BufferOverflow, BytesMessage};
use smart_leds::colors::*;

mod opendeck_handler;

pub trait Handler {
    fn handle_human_input<'a>(
        &mut self,
        e: InputEvent,
        buffer: &'a mut [u8],
    ) -> Result<Option<BytesMessage<&'a mut [u8]>>, BufferOverflow>;
    fn handle_midi_input(&mut self, m: BytesMessage<&[u8]>);
    fn process_sysex(&mut self, request: &[u8]) -> opendeck::config::Responses;
    fn leds(&mut self) -> &mut Leds;
}

/// The router (dispatcher) for human input and midi input
#[derive(Default)]
pub struct Handlers {
    opendeck: OpenDeck,
}

impl Handlers {
    pub fn new() -> Self {
        Handlers {
            opendeck: OpenDeck::new(),
        }
    }
}

impl Handler for Handlers {
    fn handle_human_input<'a>(
        &mut self,
        event: InputEvent,
        buffer: &'a mut [u8],
    ) -> Result<Option<BytesMessage<&'a mut [u8]>>, BufferOverflow> {
        info!("handle input event {:?}", event);
        let actions = self.opendeck.handle_human_input(event, buffer);
        // FIXME only flash when a message was received
        //        if let actions = Actions::MidiMessage {
        // MIDI-out indicator
        self.leds().set(Flash(DARK_GREEN), Led::Mon);
        //      }

        actions
    }
    fn handle_midi_input(&mut self, m: BytesMessage<&[u8]>) {
        let mut handled = false;
        match m {
            // see https://github.com/pedalboard/db-meter.lv2
            BytesMessage::ChannelVoice1(m) => match m {
                midi2::channel_voice1::ChannelVoice1::NoteOn(m) => {
                    if m.note_number() == u7::new(24) {
                        handled = true;
                        let v: u8 = m.velocity().into();
                        let lufs = -(v as f32);

                        debug!("loudness {}", lufs);
                        self.leds().set_ledring(
                            super::hmi::ledring::Animation::Loudness(lufs),
                            LedRings::Vol,
                        );
                    }
                }
                _ => {}
            },
            _ => {}
        }
        if handled {
            // MIDI-in indicator
            self.leds().set(Flash(DARK_BLUE), Led::Mon);
        }
    }

    fn process_sysex(&mut self, request: &[u8]) -> opendeck::config::Responses {
        self.opendeck.process_sysex(request)
    }

    fn leds(&mut self) -> &mut Leds {
        self.opendeck.leds()
    }
}
