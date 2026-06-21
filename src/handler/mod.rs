use defmt::*;
use midi2::prelude::*;
use midi2::BytesMessage;
use opendeck::handler::Messages;
use pedalboard_midi::events::InputEvent;
use pedalboard_midi::leds::{Animation::Flash, Led, LedRings, Leds};
use pedalboard_midi::opendeck_handler::{OpenDeck, OpenDeckConfig, OpenDeckConfigResponses};
use smart_leds::colors::*;

pub trait Handler {
    fn handle_human_input(&mut self, e: InputEvent) -> Messages<'_>;
    fn handle_midi_input(&mut self, m: &BytesMessage<&[u8]>);
    fn process_sysex(&mut self, request: &[u8]) -> OpenDeckConfigResponses;
    fn leds(&mut self) -> &mut Leds;
    fn config(&mut self) -> &mut OpenDeckConfig;
}

/// The router (dispatcher) for human input and midi input
pub struct Handlers {
    opendeck: OpenDeck,
}

impl Handlers {
    pub fn new() -> Self {
        Handlers {
            opendeck: OpenDeck::new(
                opendeck::config::FirmwareVersion {
                    major: 1,
                    minor: 0,
                    revision: 0,
                },
                0x123456,
                reboot,
                bootloader,
            ),
        }
    }
}

impl Default for Handlers {
    fn default() -> Self {
        Self::new()
    }
}

impl Handler for Handlers {
    fn handle_human_input(&mut self, event: InputEvent) -> Messages<'_> {
        use pedalboard_midi::events::Edge;
        use pedalboard_midi::ledring::Animation as RingAnim;
        use smart_leds::colors::*;

        // Toggle LED ring when a button is pressed
        match event {
            InputEvent::ButtonA(Edge::Activate) => self.leds().set_ledring(RingAnim::Toggle(GREEN, false), LedRings::A),
            InputEvent::ButtonB(Edge::Activate) => self.leds().set_ledring(RingAnim::Toggle(GREEN, false), LedRings::B),
            InputEvent::ButtonC(Edge::Activate) => self.leds().set_ledring(RingAnim::Toggle(GREEN, false), LedRings::C),
            InputEvent::ButtonD(Edge::Activate) => self.leds().set_ledring(RingAnim::Toggle(GREEN, false), LedRings::D),
            InputEvent::ButtonE(Edge::Activate) => self.leds().set_ledring(RingAnim::Toggle(GREEN, false), LedRings::E),
            InputEvent::ButtonF(Edge::Activate) => self.leds().set_ledring(RingAnim::Toggle(GREEN, false), LedRings::F),
            _ => {}
        }

        info!("handle input event");
        self.opendeck.handle_human_input(event)
    }

    fn handle_midi_input(&mut self, m: &BytesMessage<&[u8]>) {
        let mut handled = false;
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
            self.opendeck.leds.set(Flash(DARK_BLUE), Led::Mon);
            self.opendeck.handle_midi_input(m);
        }
    }

    fn process_sysex(&mut self, request: &[u8]) -> OpenDeckConfigResponses {
        self.opendeck.process_sysex(request)
    }

    fn leds(&mut self) -> &mut Leds {
        &mut self.opendeck.leds
    }

    fn config(&mut self) -> &mut OpenDeckConfig {
        &mut self.opendeck.config
    }
}

fn reboot() {
    warn!("Rebooting...");
    cortex_m::peripheral::SCB::sys_reset();
}

fn bootloader() {
    warn!("Rebooting to bootloader...");
    rp2040_hal::rom_data::reset_to_usb_boot(0, 0);
}
