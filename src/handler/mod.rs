use crate::handler::opendeck_handler::OpenDeck;
use crate::hmi::inputs::InputEvent;
use defmt::*;
use midi2::prelude::*;
use midi2::BytesMessage;
use opendeck::handler::Messages;
use opendeck_handler::{OpenDeckConfig, OpenDeckConfigResponses};
use pedalboard_midi::leds::{Animation::Flash, Led, LedRings, Leds};
use smart_leds::colors::*;

pub mod opendeck_handler;

pub trait Handler {
    fn handle_human_input(&mut self, e: InputEvent) -> Messages<'_>;
    fn handle_midi_input(&mut self, m: &BytesMessage<&[u8]>);
    fn process_sysex(&mut self, request: &[u8]) -> OpenDeckConfigResponses;
    fn leds(&mut self) -> &mut Leds;
    fn config(&mut self) -> &mut OpenDeckConfig;
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
    fn handle_human_input<'a>(&mut self, event: InputEvent) -> Messages<'_> {
        info!("handle input event {:?}", event);
        let r = self.opendeck.handle_human_input(event);
        // FIXME only flash when a message was received
        //        if let actions = Actions::MidiMessage {
        // MIDI-out indicator

        //        if has_midi_message(&r) {
        //            self.leds().set(Flash(DARK_GREEN), Led::Mon);
        //        }

        r
    }
    fn handle_midi_input(&mut self, m: &BytesMessage<&[u8]>) {
        let mut handled = false;
        // see https://github.com/pedalboard/db-meter.lv2
        if let BytesMessage::ChannelVoice1(midi2::channel_voice1::ChannelVoice1::NoteOn(m)) = m {
            if m.note_number() == u7::new(24) {
                handled = true;
                let v: u8 = m.velocity().into();
                let lufs = -(v as f32);

                debug!("loudness {}", lufs);
                self.leds().set_ledring(
                    pedalboard_midi::ledring::Animation::Loudness(lufs),
                    LedRings::Vol,
                );
            }
        }
        if !handled {
            // MIDI-in indicator
            self.leds().set(Flash(DARK_BLUE), Led::Mon);
            self.opendeck.handle_midi_input(m);
        }
    }

    fn process_sysex(&mut self, request: &[u8]) -> OpenDeckConfigResponses {
        self.opendeck.process_sysex(request)
    }

    fn leds(&mut self) -> &mut Leds {
        self.opendeck.leds()
    }
    fn config(&mut self) -> &mut OpenDeckConfig {
        self.opendeck.config()
    }
}
