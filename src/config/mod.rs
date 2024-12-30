use defmt::*;
use midi_types::{Channel, Value7};
use opendeck::{
    parser::OpenDeckParser,
    renderer::{Buffer, OpenDeckRenderer},
    Amount, Block, ButtonSection, ButtonType, ChannelOrAll, FirmwareVersion, GlobalSection,
    HardwareUid, MessageStatus, MessageType, NrOfSupportedComponents, OpenDeckRequest,
    OpenDeckResponse, PresetIndex, SpecialRequest, SpecialResponse, ValueSize, Wish,
};

const OPENDECK_UID: u32 = 0x12345677;
const OPENDECK_ANALOG: usize = 2;
const OPENDECK_ENCODERS: usize = 2;
const OPENDECK_LEDS: usize = 8;
const OPENDECK_BUTTONS: usize = 8;
const OPENDECK_NR_PRESETS: usize = 2;

use heapless::Vec;

#[derive(Debug, Format, Clone)]
pub struct Button {
    button_type: ButtonType,
    value: Value7,
    midi_id: Value7,
    message_type: MessageType,
    channel: ChannelOrAll,
}

impl Button {
    fn new(midi_id: Value7) -> Self {
        Button {
            button_type: ButtonType::default(),
            value: Value7::new(0x01),
            midi_id,
            message_type: MessageType::default(),
            channel: ChannelOrAll::Channel(Channel::C1),
        }
    }
    fn set(&mut self, section: ButtonSection) {
        match section {
            ButtonSection::Type(t) => self.button_type = t,
            ButtonSection::Value(v) => self.value = v,
            ButtonSection::MidiId(id) => self.midi_id = id,
            ButtonSection::MessageType(t) => self.message_type = t,
            ButtonSection::Channel(c) => self.channel = c,
        }
    }
    fn get(&self, section: &ButtonSection) -> u16 {
        match section {
            ButtonSection::Type(_) => self.button_type as u16,
            ButtonSection::MessageType(_) => self.message_type as u16,
            ButtonSection::Value(_) => {
                let v: u8 = self.value.into();
                v as u16
            }
            ButtonSection::MidiId(_) => {
                let v: u8 = self.midi_id.into();
                v as u16
            }
            ButtonSection::Channel(_) => self.channel.clone().into(),
        }
    }
}

impl Default for Button {
    fn default() -> Self {
        Button::new(Value7::new(0x00))
    }
}

#[derive(Format, Debug)]
pub struct Preset {
    buttons: Vec<Button, OPENDECK_BUTTONS>,
}

impl Default for Preset {
    fn default() -> Self {
        let mut buttons = Vec::new();
        for i in 0..OPENDECK_BUTTONS {
            buttons.push(Button::new(Value7::new(i as u8))).unwrap();
        }
        Preset { buttons }
    }
}

impl Preset {
    fn button_mut(&mut self, index: u16) -> Option<&mut Button> {
        self.buttons.get_mut(index as usize)
    }
    fn button(&mut self, index: u16) -> Option<&Button> {
        self.buttons.get(index as usize)
    }
}

#[derive(Default)]
pub struct Config {
    enabled: bool,
    current_preset: usize,
    presets: Vec<Preset, OPENDECK_NR_PRESETS>,
}

impl Config {
    pub fn new() -> Self {
        let mut presets = Vec::new();
        for _ in 0..OPENDECK_NR_PRESETS {
            presets.push(Preset::default()).unwrap();
        }

        Config {
            enabled: false,
            current_preset: 0,
            presets,
        }
    }
    /// Processes a SysEx request and returns an optional response.
    pub fn process_sysex(&mut self, request: &[u8]) -> Option<Buffer> {
        let parser = OpenDeckParser::new(ValueSize::TwoBytes);
        if let Ok(req) = parser.parse(request) {
            if let Some(odr) = self.process_req(req) {
                info!("opendeck-res: {}", odr);
                let r = OpenDeckRenderer::new(ValueSize::TwoBytes);
                return Some(r.render(odr, MessageStatus::Response));
            }
        }

        None
    }

    fn process_req(&mut self, req: OpenDeckRequest) -> Option<OpenDeckResponse> {
        info!("opendeck-req: {}", req);
        match req {
            OpenDeckRequest::Special(special) => {
                if let Some(spec_res) = self.process_special_req(special) {
                    return Some(OpenDeckResponse::Special(spec_res));
                }
                None
            }
            OpenDeckRequest::Configuration(wish, amount, block) => {
                let mut res_values = Vec::new();
                let bc = block.clone();

                if let Some(preset) = self.current_preset_mut() {
                    match block {
                        Block::Global(_, GlobalSection::Midi(_)) => {}
                        Block::Global(index, GlobalSection::Presets(value)) => {
                            if let Ok(param) = PresetIndex::try_from(index) {
                                match param {
                                    PresetIndex::Active => match wish {
                                        Wish::Set => self.current_preset = value as usize,
                                        Wish::Get | Wish::Backup => {
                                            res_values.push(self.current_preset as u16).unwrap()
                                        }
                                    },
                                    // FIXME implement more preset features
                                    PresetIndex::Preservation => {}
                                    PresetIndex::EnableMideChange => {}
                                    PresetIndex::ForceValueRefresh => {}
                                }
                            }
                        }
                        Block::Button(index, section) => match wish {
                            Wish::Set => {
                                if let Some(b) = preset.button_mut(index) {
                                    b.set(section)
                                }
                            }
                            Wish::Get | Wish::Backup => match amount {
                                Amount::Single => {
                                    if let Some(b) = preset.button(index) {
                                        res_values.push(b.get(&section)).unwrap();
                                    }
                                }
                                Amount::All => {
                                    for b in preset.buttons.iter() {
                                        res_values.push(b.get(&section)).unwrap();
                                    }
                                }
                            },
                        },
                        Block::Encoder => {}
                        Block::Analog(_, _) => {}
                        Block::Display => {}
                        Block::Led => {}
                        Block::Touchscreen => {}
                    };
                };
                Some(OpenDeckResponse::Configuration(
                    wish, amount, bc, res_values,
                ))
            }
            _ => None,
        }
    }

    fn process_special_req(&mut self, special: SpecialRequest) -> Option<SpecialResponse> {
        match special {
            SpecialRequest::BootloaderMode => {
                rp2040_hal::rom_data::reset_to_usb_boot(0, 0);
                None
            }
            SpecialRequest::Reboot => {
                cortex_m::peripheral::SCB::sys_reset();
            }
            SpecialRequest::Handshake => {
                self.enabled = true;
                Some(SpecialResponse::Handshake)
            }
            SpecialRequest::ValueSize => Some(SpecialResponse::ValueSize),
            SpecialRequest::ValuesPerMessage => Some(SpecialResponse::ValuesPerMessage(32)),
            SpecialRequest::FirmwareVersion => {
                Some(SpecialResponse::FirmwareVersion(firmware_version()))
            }
            SpecialRequest::HardwareUID => {
                Some(SpecialResponse::HardwareUID(HardwareUid(OPENDECK_UID)))
            }
            SpecialRequest::FirmwareVersionAndHardwareUUID => {
                Some(SpecialResponse::FirmwareVersionAndHardwareUUID(
                    firmware_version(),
                    HardwareUid(OPENDECK_UID),
                ))
            }
            SpecialRequest::BootloaderSupport => Some(SpecialResponse::BootloaderSupport(true)),
            SpecialRequest::NrOfSupportedPresets => {
                Some(SpecialResponse::NrOfSupportedPresets(OPENDECK_NR_PRESETS))
            }
            SpecialRequest::NrOfSupportedComponents => Some(
                SpecialResponse::NrOfSupportedComponents(NrOfSupportedComponents {
                    buttons: OPENDECK_BUTTONS,
                    encoders: OPENDECK_ENCODERS,
                    analog: OPENDECK_ANALOG,
                    leds: OPENDECK_LEDS,
                    touchscreen_buttons: 0,
                }),
            ),
            _ => None,
        }
    }
    fn current_preset_mut(&mut self) -> Option<&mut Preset> {
        self.presets.get_mut(self.current_preset)
    }
}

fn firmware_version() -> FirmwareVersion {
    FirmwareVersion {
        major: 1,
        minor: 0,
        revision: 0,
    }
}
