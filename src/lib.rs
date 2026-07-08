#![no_std]

/// Maximum serialized size of a single preset (postcard-encoded).
/// Used for PE receive/send buffers and the persist channel Vec.
///
/// # Preset data flow and buffer chain
///
/// ```text
/// Upload (PE SET):
///   USB SysEx [Vec<u8, 256>] → decode_mcoded7 [u8; N] → SavePreset [Vec<u8, N>]
///     → persist channel → save_preset → flash (via 256-byte page writes)
///
/// Read-back (PE GET):
///   flash → load_all_presets [u8; 4096] → Preset (RAM)
///   pe-read → postcard::to_slice [u8; N] → encode_mcoded7 → Vec<u8, 256> SysEx reply
///     → USB out channel (capacity >= reply_size / 3)
/// ```
///
/// `N` = `MAX_PRESET_SIZE`. When increasing, also update:
/// - `pedalboard-protocol`: `build_get_reply` Vec capacity and `encoded_body` buffer
/// - `USB_OUT_CAPACITY` (the static assert will catch this)
pub const MAX_PRESET_SIZE: usize = 256;

/// Flash format version byte prepended to every stored preset/config blob.
///
/// Bump when the postcard-serialized layout of `Preset` or `GlobalConfig` changes
/// (reordering, removing, or changing field types). Adding a new `#[serde(default)]`
/// field at the end does NOT require a bump (postcard tolerates trailing data).
///
/// See: `dotgithub/docs/adr-versioning.md`
pub const FLASH_FORMAT_VERSION: u8 = pedalboard_protocol::config::PRESET_SCHEMA_VERSION;

/// Maximum PE GET reply message size (capped by protocol Vec<u8, 350>).
pub const MAX_PE_REPLY_SIZE: usize = 350;

/// Minimum USB out channel capacity needed to send a full PE reply (3 bytes per packet).
pub const MIN_USB_OUT_CAPACITY: usize = MAX_PE_REPLY_SIZE / 3 + 1;

pub mod action;
pub mod display;
pub mod events;
pub mod ledring;
pub mod leds;
#[cfg(all(target_arch = "arm", feature = "opendeck"))]
pub mod opendeck_handler;
pub mod pe_handler;
pub mod persist;
#[cfg(target_arch = "arm")]
pub mod storage;
pub mod system_status;
pub mod views;
