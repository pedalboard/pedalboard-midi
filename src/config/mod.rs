use defmt::*;
use opendeck::{
    parser::OpenDeckParser,
    renderer::{Buffer, OpenDeckRenderer},
    Block, FirmwareVersion, GlobalSection, HardwareUid, MessageStatus, NrOfSupportedComponents,
    OpenDeckRequest, OpenDeckResponse, PresetIndex, SpecialRequest, SpecialResponse, ValueSize,
    Wish,
};

const OPENDECK_UID: u32 = 0x12345677;
const OPENDECK_ANALOG: usize = 2;
const OPENDECK_ENCODERS: usize = 2;
const OPENDECK_LEDS: usize = 8;
const OPENDECK_BUTTONS: usize = 8;
const OPENDECK_NR_PRESETS: usize = 2;

use heapless::Vec;

#[derive(Default, Copy, Clone)]
pub struct Preset {}

#[derive(Default)]
pub struct Config {
    enabled: bool,
    current_preset: u16,
    //    presets: [Preset; OPENDECK_NR_PRESETS],
}

impl Config {
    pub fn new() -> Self {
        Config {
            enabled: false,
            current_preset: 0,
            //          presets: [Preset::default(); OPENDECK_NR_PRESETS],
        }
    }
    /// Processes a SysEx request and returns an optional response.
    pub fn process_sysex(&mut self, request: &[u8]) -> Option<Buffer> {
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
                        self.enabled = true;
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
                    SpecialRequest::FirmwareVersionAndHardwareUUID => Some(
                        OpenDeckResponse::Special(SpecialResponse::FirmwareVersionAndHardwareUUID(
                            firmware_version(),
                            HardwareUid(OPENDECK_UID),
                        )),
                    ),
                    SpecialRequest::BootloaderSupport => Some(OpenDeckResponse::Special(
                        SpecialResponse::BootloaderSupport(true),
                    )),
                    SpecialRequest::NrOfSupportedPresets => Some(OpenDeckResponse::Special(
                        SpecialResponse::NrOfSupportedPresets(OPENDECK_NR_PRESETS),
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
                OpenDeckRequest::Configuration(wish, amount, block) => {
                    let mut res_values = Vec::new();
                    match block {
                        Block::Global(GlobalSection::Midi(_, _)) => {}
                        Block::Global(GlobalSection::Presets(index, value)) => {
                            if let Ok(param) = PresetIndex::try_from(index) {
                                match param {
                                    PresetIndex::Active => match wish {
                                        Wish::Set => self.current_preset = value,
                                        Wish::Get | Wish::Backup => {
                                            res_values.push(self.current_preset).unwrap()
                                        }
                                    },
                                    // FIXME implement more preset features
                                    PresetIndex::Preservation => {}
                                    PresetIndex::EnableMideChange => {}
                                    PresetIndex::ForceValueRefresh => {}
                                }
                            }
                        }
                        Block::Button(_) => {}
                        Block::Encoder => {}
                        Block::Analog(_) => {}
                        Block::Display => {}
                        Block::Led => {}
                        Block::Touchscreen => {}
                    }
                    Some(OpenDeckResponse::Configuration(
                        wish, amount, block, res_values,
                    ))
                }

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
}

fn firmware_version() -> FirmwareVersion {
    FirmwareVersion {
        major: 1,
        minor: 0,
        revision: 0,
    }
}
