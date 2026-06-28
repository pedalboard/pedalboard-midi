#![no_std]

/// Maximum serialized size of a single preset (postcard-encoded).
/// Used for PE receive/send buffers and the persist channel Vec.
pub const MAX_PRESET_SIZE: usize = 256;

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
