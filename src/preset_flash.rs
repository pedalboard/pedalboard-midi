//! Raw flash storage for PE presets.
//!
//! Uses a dedicated 4KB sector at the END of the storage region.
//! Layout: [magic: u32][count: u8][entries...]
//! Entry: [index: u8][len: u16][data: [u8; len]]
//!
//! On save: erase sector, write all presets sequentially.
//! On load: read sequentially until count exhausted.

const PRESET_SECTOR_OFFSET: u32 = 0x001F_0000 + (60 * 1024); // last 4KB of storage region
const SECTOR_SIZE: usize = 4096;
const MAGIC: u32 = 0x5045_4442; // "PEDB"

/// Save all preset blobs to flash. Each entry is (index, data).
/// Erases the sector and writes everything in one shot.
pub fn save_all(presets: &[(u8, &[u8])]) {
    let mut page = [0xFFu8; SECTOR_SIZE];

    // Header
    page[0..4].copy_from_slice(&MAGIC.to_le_bytes());
    page[4] = presets.len() as u8;

    let mut offset = 5;
    for &(idx, data) in presets {
        if offset + 3 + data.len() > SECTOR_SIZE {
            break; // sector full
        }
        page[offset] = idx;
        page[offset + 1] = (data.len() & 0xFF) as u8;
        page[offset + 2] = ((data.len() >> 8) & 0xFF) as u8;
        page[offset + 3..offset + 3 + data.len()].copy_from_slice(data);
        offset += 3 + data.len();
    }

    write_sector(&page);
}

/// Save or update a single preset. Reads existing presets from flash,
/// updates/adds the given one, and rewrites the sector.
/// Uses a static buffer — NOT reentrant.
pub fn save_one(preset_index: u8, data: &[u8]) {
    static mut PAGE: [u8; SECTOR_SIZE] = [0xFF; SECTOR_SIZE];

    // Safety: called only from persist task, single-threaded
    let page = unsafe { &mut *core::ptr::addr_of_mut!(PAGE) };
    page.fill(0xFF);

    let mut offset = 5usize;
    let mut count = 0u8;

    // Copy existing presets (except the one we're replacing)
    load_all(|idx, existing| {
        if idx == preset_index {
            return;
        }
        if offset + 3 + existing.len() > SECTOR_SIZE {
            return;
        }
        page[offset] = idx;
        page[offset + 1] = (existing.len() & 0xFF) as u8;
        page[offset + 2] = ((existing.len() >> 8) & 0xFF) as u8;
        page[offset + 3..offset + 3 + existing.len()].copy_from_slice(existing);
        offset += 3 + existing.len();
        count += 1;
    });

    // Write the new/updated preset
    if offset + 3 + data.len() <= SECTOR_SIZE {
        page[offset] = preset_index;
        page[offset + 1] = (data.len() & 0xFF) as u8;
        page[offset + 2] = ((data.len() >> 8) & 0xFF) as u8;
        page[offset + 3..offset + 3 + data.len()].copy_from_slice(data);
        count += 1;

        page[0..4].copy_from_slice(&MAGIC.to_le_bytes());
        page[4] = count;
        write_sector(page);
    }
}

fn write_sector(page: &[u8; SECTOR_SIZE]) {
    cortex_m::interrupt::free(|_| unsafe {
        rp2040_flash::flash::flash_range_erase(PRESET_SECTOR_OFFSET, SECTOR_SIZE as u32, true);
        for page_idx in 0..(SECTOR_SIZE / 256) {
            let page_offset = page_idx * 256;
            rp2040_flash::flash::flash_range_program(
                PRESET_SECTOR_OFFSET + page_offset as u32,
                &page[page_offset..page_offset + 256],
                true,
            );
        }
    });
}

/// Read preset blobs from flash. Calls the callback for each (index, data) found.
pub fn load_all(mut callback: impl FnMut(u8, &[u8])) {
    let flash_base = (0x1000_0000 + PRESET_SECTOR_OFFSET) as *const u8;

    // Read header
    let magic = u32::from_le_bytes(unsafe {
        [
            *flash_base,
            *flash_base.add(1),
            *flash_base.add(2),
            *flash_base.add(3),
        ]
    });
    if magic != MAGIC {
        return; // no valid data
    }
    let count = unsafe { *flash_base.add(4) };

    let mut offset = 5usize;
    for _ in 0..count {
        if offset + 3 > SECTOR_SIZE {
            break;
        }
        let idx = unsafe { *flash_base.add(offset) };
        let len = unsafe {
            (*flash_base.add(offset + 1) as usize) | ((*flash_base.add(offset + 2) as usize) << 8)
        };
        offset += 3;
        if offset + len > SECTOR_SIZE {
            break;
        }
        let data = unsafe { core::slice::from_raw_parts(flash_base.add(offset), len) };
        callback(idx, data);
        offset += len;
    }
}
