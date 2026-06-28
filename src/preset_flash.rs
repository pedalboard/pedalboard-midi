//! Raw flash storage for PE presets.
//!
//! Uses a dedicated 4KB sector at the END of the storage region.
//! Format handled by `preset_format` module.
//! Flash I/O via `rp2040-flash` (runs from RAM, interrupts disabled).

use crate::preset_format::{self, SECTOR_SIZE};

const PRESET_SECTOR_OFFSET: u32 = 0x001F_0000 + (60 * 1024); // last 4KB of storage region

/// Read all presets from flash. Calls the callback for each (index, data) found.
pub fn load_all(callback: impl FnMut(u8, &[u8])) {
    let buf = flash_sector_slice();
    preset_format::parse(buf, callback);
}

/// Read a single preset's raw bytes from flash. Returns a slice into XIP flash (zero-copy).
pub fn load_one(preset_index: u8) -> Option<&'static [u8]> {
    let buf = flash_sector_slice();
    preset_format::find_one(buf, preset_index)
}

/// Save or update a single preset. Reads existing, updates one, rewrites sector.
/// Uses a static buffer — NOT reentrant.
pub fn save_one(preset_index: u8, data: &[u8]) {
    static mut PAGE: [u8; SECTOR_SIZE] = [0xFF; SECTOR_SIZE];

    let page = unsafe { &mut *core::ptr::addr_of_mut!(PAGE) };
    let src = flash_sector_slice();
    preset_format::serialize_with_update(page, src, preset_index, data);
    write_sector(page);
}

/// Erase the entire preset sector (factory reset).
pub fn erase_all() {
    cortex_m::interrupt::free(|_| unsafe {
        rp2040_flash::flash::flash_range_erase(PRESET_SECTOR_OFFSET, SECTOR_SIZE as u32, true);
    });
}

fn flash_sector_slice() -> &'static [u8] {
    let ptr = (0x1000_0000 + PRESET_SECTOR_OFFSET) as *const u8;
    unsafe { core::slice::from_raw_parts(ptr, SECTOR_SIZE) }
}

fn write_sector(page: &[u8; SECTOR_SIZE]) {
    cortex_m::interrupt::free(|_| unsafe {
        rp2040_flash::flash::flash_range_erase(PRESET_SECTOR_OFFSET, SECTOR_SIZE as u32, true);
        for i in 0..(SECTOR_SIZE / 256) {
            let offset = i * 256;
            rp2040_flash::flash::flash_range_program(
                PRESET_SECTOR_OFFSET + offset as u32,
                &page[offset..offset + 256],
                true,
            );
        }
    });
}
