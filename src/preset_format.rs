//! Preset flash format: platform-independent serialization logic.
//!
//! Layout: [magic: u32 LE][version: u8][count: u8][entries...]
//! Entry: [index: u8][len_lo: u8][len_hi: u8][checksum: u8][data: [u8; len]]

pub const SECTOR_SIZE: usize = 4096;
pub const MAGIC: u32 = 0x5045_4442; // "PEDB"
pub const FORMAT_VERSION: u8 = 1;
const HEADER_SIZE: usize = 6; // magic(4) + version(1) + count(1)
const ENTRY_HEADER_SIZE: usize = 4; // index + len_lo + len_hi + checksum

/// Serialize presets into a sector buffer. Returns number of presets written.
pub fn serialize(buf: &mut [u8; SECTOR_SIZE], presets: &[(u8, &[u8])]) -> u8 {
    buf.fill(0xFF);
    buf[0..4].copy_from_slice(&MAGIC.to_le_bytes());
    buf[4] = FORMAT_VERSION;

    let mut offset = HEADER_SIZE;
    let mut count = 0u8;

    for &(idx, data) in presets {
        if offset + ENTRY_HEADER_SIZE + data.len() > SECTOR_SIZE {
            break;
        }
        buf[offset] = idx;
        buf[offset + 1] = (data.len() & 0xFF) as u8;
        buf[offset + 2] = ((data.len() >> 8) & 0xFF) as u8;
        buf[offset + 3] = checksum(data);
        buf[offset + 4..offset + 4 + data.len()].copy_from_slice(data);
        offset += ENTRY_HEADER_SIZE + data.len();
        count += 1;
    }

    buf[5] = count;
    count
}

/// Parse presets from a sector buffer. Calls callback for each (index, data) found.
/// Entries with invalid checksums are silently skipped.
/// Returns immediately if version doesn't match (stale format).
pub fn parse(buf: &[u8], mut callback: impl FnMut(u8, &[u8])) {
    if buf.len() < HEADER_SIZE {
        return;
    }
    let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    if magic != MAGIC {
        return;
    }
    if buf[4] != FORMAT_VERSION {
        return;
    }
    let count = buf[5];

    let mut offset = HEADER_SIZE;
    for _ in 0..count {
        if offset + ENTRY_HEADER_SIZE > buf.len().min(SECTOR_SIZE) {
            break;
        }
        let idx = buf[offset];
        let len = (buf[offset + 1] as usize) | ((buf[offset + 2] as usize) << 8);
        let expected_csum = buf[offset + 3];
        offset += ENTRY_HEADER_SIZE;
        if offset + len > buf.len().min(SECTOR_SIZE) {
            break;
        }
        let data = &buf[offset..offset + len];
        if checksum(data) == expected_csum {
            callback(idx, data);
        }
        offset += len;
    }
}

/// Find a single preset in a sector buffer. Returns the data slice or None.
pub fn find_one(buf: &[u8], preset_index: u8) -> Option<&[u8]> {
    if buf.len() < HEADER_SIZE {
        return None;
    }
    let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    if magic != MAGIC {
        return None;
    }
    if buf[4] != FORMAT_VERSION {
        return None;
    }
    let count = buf[5];

    let mut offset = HEADER_SIZE;
    for _ in 0..count {
        if offset + ENTRY_HEADER_SIZE > buf.len().min(SECTOR_SIZE) {
            break;
        }
        let idx = buf[offset];
        let len = (buf[offset + 1] as usize) | ((buf[offset + 2] as usize) << 8);
        let expected_csum = buf[offset + 3];
        offset += ENTRY_HEADER_SIZE;
        if offset + len > buf.len().min(SECTOR_SIZE) {
            break;
        }
        if idx == preset_index && checksum(&buf[offset..offset + len]) == expected_csum {
            return Some(&buf[offset..offset + len]);
        }
        offset += len;
    }
    None
}

/// Serialize with one preset updated/added. Reads existing entries from `src`,
/// skips the one being replaced, appends the new one.
pub fn serialize_with_update(
    buf: &mut [u8; SECTOR_SIZE],
    src: &[u8],
    preset_index: u8,
    data: &[u8],
) -> u8 {
    buf.fill(0xFF);

    let mut offset = HEADER_SIZE;
    let mut count = 0u8;

    // Copy existing (except the one we're replacing)
    parse(src, |idx, existing| {
        if idx == preset_index {
            return;
        }
        if offset + ENTRY_HEADER_SIZE + existing.len() > SECTOR_SIZE {
            return;
        }
        buf[offset] = idx;
        buf[offset + 1] = (existing.len() & 0xFF) as u8;
        buf[offset + 2] = ((existing.len() >> 8) & 0xFF) as u8;
        buf[offset + 3] = checksum(existing);
        buf[offset + 4..offset + 4 + existing.len()].copy_from_slice(existing);
        offset += ENTRY_HEADER_SIZE + existing.len();
        count += 1;
    });

    // Append new/updated preset
    if offset + ENTRY_HEADER_SIZE + data.len() <= SECTOR_SIZE {
        buf[offset] = preset_index;
        buf[offset + 1] = (data.len() & 0xFF) as u8;
        buf[offset + 2] = ((data.len() >> 8) & 0xFF) as u8;
        buf[offset + 3] = checksum(data);
        buf[offset + 4..offset + 4 + data.len()].copy_from_slice(data);
        count += 1;
    }

    buf[0..4].copy_from_slice(&MAGIC.to_le_bytes());
    buf[4] = FORMAT_VERSION;
    buf[5] = count;
    count
}

/// Simple XOR checksum over data bytes.
fn checksum(data: &[u8]) -> u8 {
    let mut csum: u8 = 0xA5; // seed to avoid all-zeros matching
    for &b in data {
        csum ^= b;
    }
    csum
}
