use defmt::*;
use midi_types::{Channel, Value14, Value7};
use opendeck::{
    parser::{OpenDeckParseError, OpenDeckParser},
    renderer::{Buffer, OpenDeckRenderer},
    Accelleration, Amount, Block, ButtonSection, ButtonType, ChannelOrAll, EncoderMessageType,
    EncoderSection, FirmwareVersion, GlobalSection, HardwareUid, MessageStatus, MessageType,
    NewValues, NrOfSupportedComponents, OpenDeckRequest, OpenDeckResponse, PresetIndex,
    SpecialRequest, SpecialResponse, ValueSize, Wish,
};

const OPENDECK_UID: u32 = 0x12345677;
const OPENDECK_ANALOG: usize = 2;
const OPENDECK_ENCODERS: usize = 2;
const OPENDECK_LEDS: usize = 8;
const OPENDECK_BUTTONS: usize = 8;
const OPENDECK_NR_PRESETS: usize = 2;
const OPENDECK_MAX_NR_MESSAGES: usize = 2;

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
    fn set(&mut self, section: &ButtonSection) {
        match section {
            ButtonSection::Type(t) => self.button_type = *t,
            ButtonSection::Value(v) => self.value = *v,
            ButtonSection::MidiId(id) => self.midi_id = *id,
            ButtonSection::MessageType(t) => self.message_type = *t,
            ButtonSection::Channel(c) => self.channel = c.clone(),
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

#[derive(Debug, Format, Clone)]
pub struct Encoder {
    enabled: bool,
    invert_state: bool,
    message_type: EncoderMessageType,
    midi_id: Value14,
    channel: ChannelOrAll,
    pulses_per_step: u8,
    accelleration: Accelleration,
    remote_sync: bool,
    upper_limit: Value14,
    lower_limit: Value14,
    second_midi_id: Value14,
}

impl Encoder {
    fn new(midi_id: Value14) -> Self {
        Encoder {
            enabled: true,
            invert_state: false,
            message_type: EncoderMessageType::ControlChange,
            channel: ChannelOrAll::Channel(Channel::C1),
            pulses_per_step: 2,
            midi_id,
            accelleration: Accelleration::None,
            remote_sync: false,
            lower_limit: Value14::new(0),
            upper_limit: Value14::new(0),
            second_midi_id: Value14::new(0),
        }
    }
    fn set(&mut self, section: &EncoderSection) {
        match section {
            EncoderSection::MessageType(v) => self.message_type = *v,
            EncoderSection::Channel(v) => self.channel = v.clone(),
            EncoderSection::Enabled(v) => self.enabled = *v,
            EncoderSection::MidiIdLSB(v) => self.midi_id = *v,
            EncoderSection::InvertState(v) => self.invert_state = *v,
            EncoderSection::PulsesPerStep(v) => self.pulses_per_step = *v,
            EncoderSection::RemoteSync(v) => self.remote_sync = *v,
            EncoderSection::Accelleration(v) => self.accelleration = *v,
            EncoderSection::LowerLimit(v) => self.lower_limit = *v,
            EncoderSection::UpperLimit(v) => self.upper_limit = *v,
            EncoderSection::SecondMidiId(v) => self.second_midi_id = *v,
            EncoderSection::MidiIdMSB(_) => {}
        }
    }
    fn get(&self, section: &EncoderSection) -> u16 {
        match section {
            EncoderSection::MessageType(_) => self.message_type as u16,
            EncoderSection::Channel(_) => self.channel.clone().into(),
            EncoderSection::Enabled(_) => self.enabled as u16,
            EncoderSection::MidiIdLSB(_) => self.midi_id.into(),
            EncoderSection::InvertState(_) => self.invert_state as u16,
            EncoderSection::PulsesPerStep(_) => self.pulses_per_step as u16,
            EncoderSection::RemoteSync(_) => self.remote_sync as u16,
            EncoderSection::Accelleration(_) => self.accelleration as u16,
            EncoderSection::LowerLimit(_) => self.lower_limit.into(),
            EncoderSection::UpperLimit(_) => self.upper_limit.into(),
            EncoderSection::SecondMidiId(_) => self.second_midi_id.into(),
            EncoderSection::MidiIdMSB(_) => 0x00,
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
    encoders: Vec<Encoder, OPENDECK_ENCODERS>,
}

impl Default for Preset {
    fn default() -> Self {
        let mut buttons = Vec::new();
        for i in 0..OPENDECK_BUTTONS {
            buttons.push(Button::new(Value7::new(i as u8))).unwrap();
        }
        let mut encoders = Vec::new();
        for i in 0..OPENDECK_ENCODERS {
            encoders.push(Encoder::new(Value14::new(i as i16))).unwrap();
        }
        Preset { buttons, encoders }
    }
}

impl Preset {
    fn button_mut(&mut self, index: &u16) -> Option<&mut Button> {
        self.buttons.get_mut(*index as usize)
    }
    fn button(&mut self, index: &u16) -> Option<&Button> {
        self.buttons.get(*index as usize)
    }
    fn encoder_mut(&mut self, index: &u16) -> Option<&mut Encoder> {
        self.encoders.get_mut(*index as usize)
    }
    fn encoder(&mut self, index: &u16) -> Option<&Encoder> {
        self.encoders.get(*index as usize)
    }
}

#[derive(Default)]
pub struct Config {
    enabled: bool,
    current_preset: usize,
    presets: Vec<Preset, OPENDECK_NR_PRESETS>,
}

type Responses = Vec<Buffer, OPENDECK_MAX_NR_MESSAGES>;

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
    /// Processes a SysEx request and returns an optional responses.
    pub fn process_sysex(&mut self, request: &[u8]) -> Responses {
        let parser = OpenDeckParser::new(ValueSize::TwoBytes);
        let renderer = OpenDeckRenderer::new(ValueSize::TwoBytes);
        let mut responses = Vec::new();
        match parser.parse(request) {
            Ok(req) => {
                if let Some(odr) = self.process_req(&req) {
                    info!("opendeck-res: {}", odr);
                    responses
                        .push(renderer.render(odr, MessageStatus::Response))
                        .unwrap();

                    if let OpenDeckRequest::Configuration(wish, Amount::All(0x7E), block) = req {
                        let end = OpenDeckResponse::Configuration(
                            wish,
                            Amount::All(0x7E),
                            block,
                            Vec::new(),
                        );
                        responses
                            .push(renderer.render(end, MessageStatus::Response))
                            .unwrap();
                    }
                }
            }
            Err(OpenDeckParseError::StatusError(status)) => {
                responses
                    .push(renderer.render(
                        OpenDeckResponse::Special(SpecialResponse::Handshake),
                        status,
                    ))
                    .unwrap();
            }
            Err(err) => {
                error!("error parsing sysex message: {}", err)
            }
        }
        responses
    }

    fn process_req(&mut self, req: &OpenDeckRequest) -> Option<OpenDeckResponse> {
        info!("opendeck-req: {}", req);
        match req {
            OpenDeckRequest::Special(special) => {
                if let Some(spec_res) = self.process_special_req(special) {
                    return Some(OpenDeckResponse::Special(spec_res));
                }
                None
            }
            OpenDeckRequest::Configuration(wish, amount, block) => {
                let (res_values, for_amount) = self.process_config(wish, amount, block);
                Some(OpenDeckResponse::Configuration(
                    wish.clone(),
                    for_amount,
                    block.clone(),
                    res_values,
                ))
            }
            _ => None,
        }
    }

    fn process_special_req(&mut self, special: &SpecialRequest) -> Option<SpecialResponse> {
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

    fn process_config(
        &mut self,
        wish: &Wish,
        amount: &Amount,
        block: &Block,
    ) -> (NewValues, Amount) {
        let mut res_values = Vec::new();
        let mut for_amount = amount.clone();

        if let Some(preset) = self.current_preset_mut() {
            match block {
                Block::Global(GlobalSection::Midi(_, _)) => {}
                Block::Global(GlobalSection::Presets(pi, value)) => {
                    match pi {
                        PresetIndex::Active => match wish {
                            Wish::Set => self.current_preset = *value as usize,
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
                Block::Button(index, section) => match wish {
                    Wish::Set => {
                        if let Some(b) = preset.button_mut(index) {
                            b.set(section)
                        }
                    }
                    Wish::Get | Wish::Backup => match amount {
                        Amount::Single => {
                            if let Some(b) = preset.button(index) {
                                res_values.push(b.get(section)).unwrap();
                            }
                        }
                        Amount::All(_) => {
                            for b in preset.buttons.iter() {
                                res_values.push(b.get(section)).unwrap();
                            }
                            for_amount = Amount::All(0)
                        }
                    },
                },
                Block::Encoder(index, section) => match wish {
                    Wish::Set => {
                        if let Some(b) = preset.encoder_mut(index) {
                            b.set(section)
                        }
                    }
                    Wish::Get | Wish::Backup => match amount {
                        Amount::Single => {
                            if let Some(b) = preset.encoder(index) {
                                res_values.push(b.get(section)).unwrap();
                            }
                        }
                        Amount::All(_) => {
                            for b in preset.encoders.iter() {
                                res_values.push(b.get(section)).unwrap();
                            }
                            for_amount = Amount::All(0)
                        }
                    },
                },
                Block::Analog(_, _) => {}
                Block::Display => {}
                Block::Led => {}
                Block::Touchscreen => {}
            };
        };

        (res_values, for_amount)
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
