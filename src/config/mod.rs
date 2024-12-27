use defmt::*;
use opendeck::{
    parser::OpenDeckParser,
    renderer::{Buffer, OpenDeckRenderer},
    FirmwareVersion, HardwareUid, MessageStatus, OpenDeckRequest, OpenDeckResponse, SpecialRequest,
    SpecialResponse,
    ValueSize::OneByte,
};

const OPENDECK_UID: u32 = 0x12345677;

/// Processes a SysEx request and returns an optional response.
pub fn process_sysex(request: &[u8]) -> Option<Buffer> {
    if let Ok(req) = OpenDeckParser::parse(request) {
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
                SpecialRequest::ValueSize => Some(OpenDeckResponse::Special(
                    SpecialResponse::ValueSize(OneByte),
                )),
                SpecialRequest::ValuesPerMessage => Some(OpenDeckResponse::Special(
                    SpecialResponse::ValuesPerMessage(32),
                )),
                SpecialRequest::FirmwareVersion => Some(OpenDeckResponse::Special(
                    SpecialResponse::FirmwareVersion(firmware_version()),
                )),
                SpecialRequest::HardwareUID => Some(OpenDeckResponse::Special(
                    SpecialResponse::HardwareUID(HardwareUid(OPENDECK_UID)),
                )),
                _ => None,
            },
            OpenDeckRequest::Configuration => None,
            _ => None,
        };
        if let Some(odr) = res {
            info!("opendeck-res: {}", odr);
            return Some(OpenDeckRenderer::render(odr, MessageStatus::Response));
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
