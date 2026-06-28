#![no_std]

/// Maximum serialized size of a single preset (postcard-encoded).
/// Used for PE receive/send buffers and the persist channel Vec.
///
/// When increasing this value, also update:
/// - `pedalboard-protocol`: `build_get_reply` Vec capacity and `encoded_body` buffer
/// - Below: `USB_OUT_CAPACITY` (the static assert will catch this)
pub const MAX_PRESET_SIZE: usize = 256;

/// Maximum PE GET reply message size (capped by protocol Vec<u8, 256>).
pub const MAX_PE_REPLY_SIZE: usize = 256;

/// Minimum USB out channel capacity needed to send a full PE reply (3 bytes per packet).
pub const MIN_USB_OUT_CAPACITY: usize = MAX_PE_REPLY_SIZE / 3 + 1;

pub mod action;
pub mod display;
pub mod events;
pub mod ledring;
pub mod leds;
pub mod long_press;
#[cfg(target_arch = "arm")]
pub mod opendeck_handler;
pub mod pe_handler;
#[cfg(target_arch = "arm")]
pub mod storage;
pub mod views;
