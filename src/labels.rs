//! Pedalboard-specific component label protocol.
//!
//! Uses M_ID_2 = 0x44 (distinct from OpenDeck's 0x43) with identical message structure.
//! Designed for easy migration to upstream OpenDeck if the extension is accepted.
//!
//! Labels are per-component properties stored as one byte per index (mDNS-style):
//! - BLOCK = 1 (Switch) or 2 (Encoder)
//! - SECTION = 5 (Switch label) or 13 (Encoder label) — next available section IDs
//! - INDEX = component_index * 16 + char_position
//! - VALUE = ASCII byte (0x00-0x7F), zero-terminated
//!
//! Message structure (same as OpenDeck configuration messages):
//!   F0 00 53 44 STATUS PART WISH AMOUNT BLOCK SECTION INDEX_H INDEX_L VALUE_H VALUE_L F7

use heapless::String;

pub const SYSEX_START: u8 = 0xF0;
pub const SYSEX_END: u8 = 0xF7;
pub const M_ID_0: u8 = 0x00;
pub const M_ID_1: u8 = 0x53;
pub const M_ID_2: u8 = 0x44;

const STATUS_REQUEST: u8 = 0x00;
const STATUS_ACK: u8 = 0x01;

const WISH_GET: u8 = 0x00;
const WISH_SET: u8 = 0x01;

const AMOUNT_SINGLE: u8 = 0x00;

const BLOCK_SWITCH: u8 = 0x01;
const BLOCK_ENCODER: u8 = 0x02;
const BLOCK_ANALOG: u8 = 0x03;
const BLOCK_GLOBAL: u8 = 0x00;

const SECTION_SWITCH_LABEL: u8 = 0x05;
const SECTION_ENCODER_LABEL: u8 = 0x0D;
const SECTION_ANALOG_LABEL: u8 = 0x0C;
const SECTION_PRESET_LABEL: u8 = 0x06;

pub const MAX_LABEL_LEN: usize = 16;
pub const LABEL_CHARS_PER_COMPONENT: usize = 16;

/// Max components per type — used for INDEX encoding.
/// INDEX = preset * MAX_COMPONENTS * 16 + component_index * 16 + char_pos
pub const MAX_SWITCH_COMPONENTS: u16 = 10;
pub const MAX_ENCODER_COMPONENTS: u16 = 2;
pub const MAX_ANALOG_COMPONENTS: u16 = 2;
pub const MAX_PRESETS: u16 = 32;

/// Check if a SysEx message is a pedalboard label message (M_ID_2 = 0x44).
pub fn is_label_message(data: &[u8]) -> bool {
    data.len() >= 9
        && data[0] == SYSEX_START
        && data[1] == M_ID_0
        && data[2] == M_ID_1
        && data[3] == M_ID_2
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentType {
    Switch,
    Encoder,
    Analog,
    Preset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LabelRequest {
    Get {
        component: ComponentType,
        preset: u8,
        index: u8,
        char_pos: u8,
    },
    Set {
        component: ComponentType,
        preset: u8,
        index: u8,
        char_pos: u8,
        value: u8,
    },
}

/// Decode a 14-bit two-byte value (OpenDeck split14bit encoding).
fn merge14bit(high: u8, low: u8) -> u16 {
    let mut low = low;
    if high & 0x01 != 0 {
        low |= 0x80;
    } else {
        low &= !0x80;
    }
    let high = high >> 1;
    ((high as u16) << 8) | low as u16
}

/// Encode a 14-bit value into two 7-bit bytes (OpenDeck split14bit encoding).
fn split14bit(value: u16) -> (u8, u8) {
    let mut high = ((value >> 8) & 0xFF) as u8;
    let mut low = (value & 0xFF) as u8;
    high = (high << 1) & 0x7F;
    if (low >> 7) & 0x01 != 0 {
        high |= 0x01;
    } else {
        high &= !0x01;
    }
    low &= 0x7F;
    (high, low)
}

/// Parse a label SysEx message. Returns None if invalid.
pub fn parse(data: &[u8]) -> Option<LabelRequest> {
    if !is_label_message(data) {
        return None;
    }
    // Minimum: F0 00 53 44 STATUS PART WISH AMOUNT BLOCK SECTION IDX_H IDX_L VAL_H VAL_L F7 = 15
    if data.len() < 15 {
        return None;
    }
    if data[4] != STATUS_REQUEST {
        return None;
    }
    // data[5] = PART (ignored for SINGLE)
    let wish = data[6];
    let amount = data[7];
    if amount != AMOUNT_SINGLE {
        return None; // only SINGLE supported for now
    }

    let component = match (data[8], data[9]) {
        (BLOCK_SWITCH, SECTION_SWITCH_LABEL) => ComponentType::Switch,
        (BLOCK_ENCODER, SECTION_ENCODER_LABEL) => ComponentType::Encoder,
        (BLOCK_ANALOG, SECTION_ANALOG_LABEL) => ComponentType::Analog,
        (BLOCK_GLOBAL, SECTION_PRESET_LABEL) => ComponentType::Preset,
        _ => return None,
    };

    let raw_index = merge14bit(data[10], data[11]);

    // Decode INDEX based on component type
    // For Switch/Encoder/Analog: INDEX = preset * max_components * 16 + component * 16 + char_pos
    // For Preset: INDEX = preset_index * 16 + char_pos
    let (preset, component_index, char_pos) = match component {
        ComponentType::Preset => {
            let preset_idx = (raw_index / LABEL_CHARS_PER_COMPONENT as u16) as u8;
            let cpos = (raw_index % LABEL_CHARS_PER_COMPONENT as u16) as u8;
            (0, preset_idx, cpos)
        }
        ComponentType::Switch => {
            let stride = MAX_SWITCH_COMPONENTS * LABEL_CHARS_PER_COMPONENT as u16;
            let preset_idx = (raw_index / stride) as u8;
            let remainder = raw_index % stride;
            let comp_idx = (remainder / LABEL_CHARS_PER_COMPONENT as u16) as u8;
            let cpos = (remainder % LABEL_CHARS_PER_COMPONENT as u16) as u8;
            (preset_idx, comp_idx, cpos)
        }
        ComponentType::Encoder => {
            let stride = MAX_ENCODER_COMPONENTS * LABEL_CHARS_PER_COMPONENT as u16;
            let preset_idx = (raw_index / stride) as u8;
            let remainder = raw_index % stride;
            let comp_idx = (remainder / LABEL_CHARS_PER_COMPONENT as u16) as u8;
            let cpos = (remainder % LABEL_CHARS_PER_COMPONENT as u16) as u8;
            (preset_idx, comp_idx, cpos)
        }
        ComponentType::Analog => {
            let stride = MAX_ANALOG_COMPONENTS * LABEL_CHARS_PER_COMPONENT as u16;
            let preset_idx = (raw_index / stride) as u8;
            let remainder = raw_index % stride;
            let comp_idx = (remainder / LABEL_CHARS_PER_COMPONENT as u16) as u8;
            let cpos = (remainder % LABEL_CHARS_PER_COMPONENT as u16) as u8;
            (preset_idx, comp_idx, cpos)
        }
    };

    match wish {
        WISH_GET => Some(LabelRequest::Get {
            component,
            preset,
            index: component_index,
            char_pos,
        }),
        WISH_SET => {
            let value = merge14bit(data[12], data[13]) as u8;
            Some(LabelRequest::Set {
                component,
                preset,
                index: component_index,
                char_pos,
                value,
            })
        }
        _ => None,
    }
}

/// Render a label response (ACK with value).
pub fn render_response(
    component: ComponentType,
    preset: u8,
    component_index: u8,
    char_pos: u8,
    value: u8,
    buf: &mut [u8],
) -> usize {
    if buf.len() < 15 {
        return 0;
    }
    let (block, section) = match component {
        ComponentType::Switch => (BLOCK_SWITCH, SECTION_SWITCH_LABEL),
        ComponentType::Encoder => (BLOCK_ENCODER, SECTION_ENCODER_LABEL),
        ComponentType::Analog => (BLOCK_ANALOG, SECTION_ANALOG_LABEL),
        ComponentType::Preset => (BLOCK_GLOBAL, SECTION_PRESET_LABEL),
    };
    let raw_index = match component {
        ComponentType::Preset => {
            component_index as u16 * LABEL_CHARS_PER_COMPONENT as u16 + char_pos as u16
        }
        ComponentType::Switch => {
            preset as u16 * MAX_SWITCH_COMPONENTS * LABEL_CHARS_PER_COMPONENT as u16
                + component_index as u16 * LABEL_CHARS_PER_COMPONENT as u16
                + char_pos as u16
        }
        ComponentType::Encoder => {
            preset as u16 * MAX_ENCODER_COMPONENTS * LABEL_CHARS_PER_COMPONENT as u16
                + component_index as u16 * LABEL_CHARS_PER_COMPONENT as u16
                + char_pos as u16
        }
        ComponentType::Analog => {
            preset as u16 * MAX_ANALOG_COMPONENTS * LABEL_CHARS_PER_COMPONENT as u16
                + component_index as u16 * LABEL_CHARS_PER_COMPONENT as u16
                + char_pos as u16
        }
    };
    let (idx_h, idx_l) = split14bit(raw_index);
    let (val_h, val_l) = split14bit(value as u16);

    buf[0] = SYSEX_START;
    buf[1] = M_ID_0;
    buf[2] = M_ID_1;
    buf[3] = M_ID_2;
    buf[4] = STATUS_ACK;
    buf[5] = 0x00; // PART
    buf[6] = WISH_SET; // echo as SET response
    buf[7] = AMOUNT_SINGLE;
    buf[8] = block;
    buf[9] = section;
    buf[10] = idx_h;
    buf[11] = idx_l;
    buf[12] = val_h;
    buf[13] = val_l;
    buf[14] = SYSEX_END;
    15
}

/// Helper: extract a full label string from a byte array of char values.
pub fn bytes_to_label(chars: &[u8]) -> String<MAX_LABEL_LEN> {
    let mut s = String::new();
    for &b in chars {
        if b == 0 {
            break;
        }
        s.push(b as char).ok();
    }
    s
}

/// Helper: convert a label string to a zero-terminated byte array.
pub fn label_to_bytes(label: &str) -> [u8; MAX_LABEL_LEN] {
    let mut bytes = [0u8; MAX_LABEL_LEN];
    for (i, b) in label.as_bytes().iter().take(MAX_LABEL_LEN - 1).enumerate() {
        bytes[i] = *b;
    }
    bytes
}

/// Storage key for persistence: encodes component type, index, and char position.
/// Storage key for persistence.
/// Uses block=7, section encodes component_type + preset, index encodes comp*16+char_pos.
pub fn storage_key(
    component: ComponentType,
    preset: u8,
    component_index: u8,
    char_pos: u8,
) -> (u8, u8, u8) {
    let base_section = match component {
        ComponentType::Switch => SECTION_SWITCH_LABEL,
        ComponentType::Encoder => SECTION_ENCODER_LABEL,
        ComponentType::Analog => SECTION_ANALOG_LABEL,
        ComponentType::Preset => SECTION_PRESET_LABEL,
    };
    // Encode preset into section: base + preset * 4 (fits in 5-bit section field for 3 presets)
    let section = base_section + preset * 4;
    let idx = component_index * LABEL_CHARS_PER_COMPONENT as u8 + char_pos;
    (7, section, idx)
}

/// Runtime label storage for buttons and encoders.
pub struct LabelStore<const B: usize, const E: usize, const A: usize, const P: usize> {
    pub buttons: [[[u8; MAX_LABEL_LEN]; B]; P],
    pub encoders: [[[u8; MAX_LABEL_LEN]; E]; P],
    pub analogs: [[[u8; MAX_LABEL_LEN]; A]; P],
    pub presets: [[u8; MAX_LABEL_LEN]; P],
    pub dirty: bool,
}

impl<const B: usize, const E: usize, const A: usize, const P: usize> LabelStore<B, E, A, P> {
    pub const fn new() -> Self {
        Self {
            buttons: [[[0u8; MAX_LABEL_LEN]; B]; P],
            encoders: [[[0u8; MAX_LABEL_LEN]; E]; P],
            analogs: [[[0u8; MAX_LABEL_LEN]; A]; P],
            presets: [[0u8; MAX_LABEL_LEN]; P],
            dirty: false,
        }
    }

    pub fn set_char(
        &mut self,
        component: ComponentType,
        preset: u8,
        index: u8,
        char_pos: u8,
        value: u8,
    ) {
        let pos = char_pos as usize;
        if pos >= MAX_LABEL_LEN {
            return;
        }
        let p = preset as usize;
        match component {
            ComponentType::Switch => {
                if let Some(preset_labels) = self.buttons.get_mut(p) {
                    if let Some(label) = preset_labels.get_mut(index as usize) {
                        label[pos] = value;
                        self.dirty = true;
                    }
                }
            }
            ComponentType::Encoder => {
                if let Some(preset_labels) = self.encoders.get_mut(p) {
                    if let Some(label) = preset_labels.get_mut(index as usize) {
                        label[pos] = value;
                        self.dirty = true;
                    }
                }
            }
            ComponentType::Analog => {
                if let Some(preset_labels) = self.analogs.get_mut(p) {
                    if let Some(label) = preset_labels.get_mut(index as usize) {
                        label[pos] = value;
                        self.dirty = true;
                    }
                }
            }
            ComponentType::Preset => {
                if let Some(label) = self.presets.get_mut(index as usize) {
                    label[pos] = value;
                    self.dirty = true;
                }
            }
        }
    }

    pub fn get_char(&self, component: ComponentType, preset: u8, index: u8, char_pos: u8) -> u8 {
        let pos = char_pos as usize;
        if pos >= MAX_LABEL_LEN {
            return 0;
        }
        let p = preset as usize;
        match component {
            ComponentType::Switch => self
                .buttons
                .get(p)
                .and_then(|pl| pl.get(index as usize))
                .map(|l| l[pos])
                .unwrap_or(0),
            ComponentType::Encoder => self
                .encoders
                .get(p)
                .and_then(|pl| pl.get(index as usize))
                .map(|l| l[pos])
                .unwrap_or(0),
            ComponentType::Analog => self
                .analogs
                .get(p)
                .and_then(|pl| pl.get(index as usize))
                .map(|l| l[pos])
                .unwrap_or(0),
            ComponentType::Preset => self
                .presets
                .get(index as usize)
                .map(|l| l[pos])
                .unwrap_or(0),
        }
    }

    pub fn button_label(&self, preset: usize, index: usize) -> String<MAX_LABEL_LEN> {
        self.buttons
            .get(preset)
            .and_then(|pl| pl.get(index))
            .map(|b| bytes_to_label(b))
            .unwrap_or_default()
    }

    pub fn encoder_label(&self, preset: usize, index: usize) -> String<MAX_LABEL_LEN> {
        self.encoders
            .get(preset)
            .and_then(|pl| pl.get(index))
            .map(|b| bytes_to_label(b))
            .unwrap_or_default()
    }

    pub fn analog_label(&self, preset: usize, index: usize) -> String<MAX_LABEL_LEN> {
        self.analogs
            .get(preset)
            .and_then(|pl| pl.get(index))
            .map(|b| bytes_to_label(b))
            .unwrap_or_default()
    }

    pub fn preset_label(&self, index: usize) -> String<MAX_LABEL_LEN> {
        self.presets
            .get(index)
            .map(|b| bytes_to_label(b))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_merge_roundtrip() {
        for v in [0u16, 1, 127, 128, 255, 1000, 16383] {
            let (h, l) = split14bit(v);
            assert!(h < 128);
            assert!(l < 128);
            assert_eq!(merge14bit(h, l), v);
        }
    }

    #[test]
    fn test_parse_set_switch_label() {
        let (idx_h, idx_l) = split14bit(2 * 16 + 0); // switch 2, char 0
        let (val_h, val_l) = split14bit(b'H' as u16);
        let msg = [
            0xF0, 0x00, 0x53, 0x44, 0x00, 0x00, 0x01, 0x00, 0x01, 0x05, idx_h, idx_l, val_h, val_l,
            0xF7,
        ];
        let req = parse(&msg).unwrap();
        assert_eq!(
            req,
            LabelRequest::Set {
                component: ComponentType::Switch,
                index: 2,
                char_pos: 0,
                value: b'H',
            }
        );
    }

    #[test]
    fn test_parse_get_encoder_label() {
        let (idx_h, idx_l) = split14bit(1 * 16 + 3); // encoder 1, char 3
        let (val_h, val_l) = split14bit(0);
        let msg = [
            0xF0, 0x00, 0x53, 0x44, 0x00, 0x00, 0x00, 0x00, 0x02, 0x0D, idx_h, idx_l, val_h, val_l,
            0xF7,
        ];
        let req = parse(&msg).unwrap();
        assert_eq!(
            req,
            LabelRequest::Get {
                component: ComponentType::Encoder,
                index: 1,
                char_pos: 3,
            }
        );
    }

    #[test]
    fn test_render_response() {
        let mut buf = [0u8; 20];
        let len = render_response(ComponentType::Switch, 2, 0, b'H', &mut buf);
        assert_eq!(len, 15);
        // Parse it back as if it were a request (change status to 0)
        assert_eq!(buf[0], 0xF0);
        assert_eq!(buf[3], 0x44);
        assert_eq!(buf[4], 0x01); // ACK
        assert_eq!(buf[14], 0xF7);
    }

    #[test]
    fn test_is_label_message() {
        let label_msg = [0xF0, 0x00, 0x53, 0x44, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF7];
        let opendeck_msg = [0xF0, 0x00, 0x53, 0x43, 0x00, 0x00, 0x01, 0xF7, 0x00, 0x00];
        assert!(is_label_message(&label_msg));
        assert!(!is_label_message(&opendeck_msg));
    }

    #[test]
    fn test_bytes_to_label() {
        let bytes = [b'V', b'o', b'l', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(bytes_to_label(&bytes).as_str(), "Vol");
    }

    #[test]
    fn test_label_to_bytes() {
        let bytes = label_to_bytes("Gain");
        assert_eq!(&bytes[..5], &[b'G', b'a', b'i', b'n', 0]);
    }

    #[test]
    fn test_storage_key() {
        let (block, section, index) = storage_key(ComponentType::Switch, 2, 3);
        assert_eq!(block, 7);
        assert_eq!(section, SECTION_SWITCH_LABEL);
        assert_eq!(index, 2 * 16 + 3);
    }
}
