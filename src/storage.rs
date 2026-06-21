//! Flash-based persistent configuration storage for RP2040.
//!
//! Uses the last 64KB of flash (16 pages of 4KB each) with `sequential-storage`
//! for wear-leveled key-value storage.

use embedded_storage::nor_flash::{
    ErrorType, MultiwriteNorFlash, NorFlash, NorFlashError, NorFlashErrorKind, ReadNorFlash,
};

/// Flash storage starting at STORAGE_ORIGIN (last 64KB of 2MB flash).
/// The RP2040 XIP base is 0x10000000; flash offset = addr - 0x10000000.
const STORAGE_ORIGIN: u32 = 0x001F_0000; // offset from flash start (2MB - 64KB)
const STORAGE_SIZE: usize = 64 * 1024;
const SECTOR_SIZE: usize = 4096;
const PAGE_SIZE: usize = 256;

#[derive(Debug)]
pub struct FlashError;

impl NorFlashError for FlashError {
    fn kind(&self) -> NorFlashErrorKind {
        NorFlashErrorKind::Other
    }
}

/// Thin wrapper around RP2040 ROM flash functions implementing `NorFlash`.
pub struct FlashStorage {
    _private: (),
}

impl FlashStorage {
    pub fn new() -> Self {
        Self { _private: () }
    }
}

impl ErrorType for FlashStorage {
    type Error = FlashError;
}

impl ReadNorFlash for FlashStorage {
    const READ_SIZE: usize = 1;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        let addr = (0x1000_0000 + STORAGE_ORIGIN + offset) as *const u8;
        // Safety: reading from XIP-mapped flash memory
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = unsafe { core::ptr::read_volatile(addr.add(i)) };
        }
        Ok(())
    }

    fn capacity(&self) -> usize {
        STORAGE_SIZE
    }
}

impl NorFlash for FlashStorage {
    const WRITE_SIZE: usize = PAGE_SIZE;
    const ERASE_SIZE: usize = SECTOR_SIZE;

    fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        let flash_from = STORAGE_ORIGIN + from;
        let count = (to - from) as usize;
        // Safety: erasing flash in the reserved storage region.
        // Must run with interrupts disabled and from RAM (critical section).
        cortex_m::interrupt::free(|_| unsafe {
            rp2040_hal::rom_data::connect_internal_flash();
            rp2040_hal::rom_data::flash_exit_xip();
            rp2040_hal::rom_data::flash_range_erase(flash_from, count, SECTOR_SIZE as u32, 0xD8);
            rp2040_hal::rom_data::flash_flush_cache();
            rp2040_hal::rom_data::flash_enter_cmd_xip();
        });
        Ok(())
    }

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        let flash_offset = STORAGE_ORIGIN + offset;
        // Safety: writing flash in the reserved storage region.
        cortex_m::interrupt::free(|_| unsafe {
            rp2040_hal::rom_data::connect_internal_flash();
            rp2040_hal::rom_data::flash_exit_xip();
            rp2040_hal::rom_data::flash_range_program(flash_offset, bytes.as_ptr(), bytes.len());
            rp2040_hal::rom_data::flash_flush_cache();
            rp2040_hal::rom_data::flash_enter_cmd_xip();
        });
        Ok(())
    }
}

impl MultiwriteNorFlash for FlashStorage {}
