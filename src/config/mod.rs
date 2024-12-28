use defmt::*;
use opendeck::{
    parser::OpenDeckParser,
    renderer::{Buffer, OpenDeckRenderer},
    FirmwareVersion, HardwareUid, MessageStatus, NrOfSupportedComponents, OpenDeckRequest,
    OpenDeckResponse, SpecialRequest, SpecialResponse, ValueSize,
};

const OPENDECK_UID: u32 = 0x12345677;
const OPENDECK_ANALOG: usize = 2;
const OPENDECK_ENCODERS: usize = 2;
const OPENDECK_LEDS: usize = 8;
const OPENDECK_BUTTONS: usize = 8;

use heapless::Vec;

/// Processes a SysEx request and returns an optional response.
pub fn process_sysex(request: &[u8]) -> Option<Buffer> {
    let parser = OpenDeckParser::new(ValueSize::TwoBytes);
    if let Ok(req) = parser.parse(request) {
        info!("opendeck-req: {}", req);
        let res = match req {
            OpenDeckRequest::Special(special) => match special {
                SpecialRequest::BootloaderMode => {
                    rp2040_hal::rom_data::reset_to_usb_boot(0, 0);
                    None
                }
                SpecialRequest::Reboot => {
                    cortex_m::peripheral::SCB::sys_reset();
                }
                SpecialRequest::Handshake => {
                    Some(OpenDeckResponse::Special(SpecialResponse::Handshake))
                }
                SpecialRequest::ValueSize => {
                    Some(OpenDeckResponse::Special(SpecialResponse::ValueSize))
                }
                SpecialRequest::ValuesPerMessage => Some(OpenDeckResponse::Special(
                    SpecialResponse::ValuesPerMessage(32),
                )),
                SpecialRequest::FirmwareVersion => Some(OpenDeckResponse::Special(
                    SpecialResponse::FirmwareVersion(firmware_version()),
                )),
                SpecialRequest::HardwareUID => Some(OpenDeckResponse::Special(
                    SpecialResponse::HardwareUID(HardwareUid(OPENDECK_UID)),
                )),
                SpecialRequest::FirmwareVersionAndHardwareUUID => Some(OpenDeckResponse::Special(
                    SpecialResponse::FirmwareVersionAndHardwareUUID(
                        firmware_version(),
                        HardwareUid(OPENDECK_UID),
                    ),
                )),
                SpecialRequest::BootloaderSupport => Some(OpenDeckResponse::Special(
                    SpecialResponse::BootloaderSupport(true),
                )),
                SpecialRequest::NrOfSupportedPresets => Some(OpenDeckResponse::Special(
                    SpecialResponse::NrOfSupportedPresets(10),
                )),
                SpecialRequest::NrOfSupportedComponents => Some(OpenDeckResponse::Special(
                    SpecialResponse::NrOfSupportedComponents(NrOfSupportedComponents {
                        buttons: OPENDECK_BUTTONS,
                        encoders: OPENDECK_ENCODERS,
                        analog: OPENDECK_ANALOG,
                        leds: OPENDECK_LEDS,
                        touchscreen_buttons: 0,
                    }),
                )),
                _ => None,
            },
            OpenDeckRequest::Configuration(wish, amount, block, index, value) => Some(
                OpenDeckResponse::Configuration(wish, amount, block, index, value, Vec::new()),
            ),

            _ => None,
        };
        if let Some(odr) = res {
            info!("opendeck-res: {}", odr);
            let r = OpenDeckRenderer::new(ValueSize::TwoBytes);
            return Some(r.render(odr, MessageStatus::Response));
        }
    }

    None
}

fn firmware_version() -> FirmwareVersion {
    FirmwareVersion {
        major: 1,
        minor: 0,
        revision: 0,
    }
}
