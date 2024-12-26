use defmt::*;
use heapless::Vec;
use opendeck::{OpenDeckMsg, OpenDeckParser, SpecialMsg};
use rp2040_hal::rom_data::reset_to_usb_boot;

pub const SYSEX_BUFFER_SIZE: usize = 64;

pub type SysexBuffer = Vec<u8, SYSEX_BUFFER_SIZE>;

/// Processes a SysEx request and returns an optional response.
pub fn process_sysex(request: &[u8]) -> Option<SysexBuffer> {
    if let Ok(msg) = OpenDeckParser::parse(request) {
        match msg {
            OpenDeckMsg::Special(special) => match special {
                SpecialMsg::BootloaderMode => {
                    info!("reset to usb boot");
                    reset_to_usb_boot(0, 0)
                }
                SpecialMsg::Reboot => {}
                _ => {}
            },
            OpenDeckMsg::Configuration => {}
            _ => {}
        }
    }

    None
}
