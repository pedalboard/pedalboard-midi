#[path = "../../src/preset_format.rs"]
mod preset_format;

use preset_format::*;

#[test]
fn empty_buffer_parses_nothing() {
    let buf = [0xFF; SECTOR_SIZE];
    let mut count = 0;
    parse(&buf, |_, _| count += 1);
    assert_eq!(count, 0);
}

#[test]
fn serialize_and_parse_roundtrip() {
    let mut buf = [0u8; SECTOR_SIZE];
    let presets: &[(u8, &[u8])] = &[(0, b"hello"), (1, b"world"), (2, b"test data here")];

    let written = serialize(&mut buf, presets);
    assert_eq!(written, 3);

    let mut parsed = Vec::new();
    parse(&buf, |idx, data| parsed.push((idx, data.to_vec())));

    assert_eq!(parsed.len(), 3);
    assert_eq!(parsed[0], (0, b"hello".to_vec()));
    assert_eq!(parsed[1], (1, b"world".to_vec()));
    assert_eq!(parsed[2], (2, b"test data here".to_vec()));
}

#[test]
fn find_one_existing() {
    let mut buf = [0u8; SECTOR_SIZE];
    serialize(&mut buf, &[(0, b"aaa"), (5, b"bbb"), (2, b"ccc")]);

    assert_eq!(find_one(&buf, 5), Some(b"bbb".as_slice()));
    assert_eq!(find_one(&buf, 2), Some(b"ccc".as_slice()));
    assert_eq!(find_one(&buf, 3), None);
}

#[test]
fn serialize_with_update_adds_new() {
    let mut src = [0u8; SECTOR_SIZE];
    serialize(&mut src, &[(0, b"existing")]);

    let mut dst = [0u8; SECTOR_SIZE];
    let count = serialize_with_update(&mut dst, &src, 1, b"new preset");
    assert_eq!(count, 2);
    assert_eq!(find_one(&dst, 0), Some(b"existing".as_slice()));
    assert_eq!(find_one(&dst, 1), Some(b"new preset".as_slice()));
}

#[test]
fn serialize_with_update_replaces_existing() {
    let mut src = [0u8; SECTOR_SIZE];
    serialize(&mut src, &[(0, b"old"), (1, b"keep")]);

    let mut dst = [0u8; SECTOR_SIZE];
    let count = serialize_with_update(&mut dst, &src, 0, b"replaced");
    assert_eq!(count, 2);
    assert_eq!(find_one(&dst, 0), Some(b"replaced".as_slice()));
    assert_eq!(find_one(&dst, 1), Some(b"keep".as_slice()));
}

#[test]
fn sector_full_stops_writing() {
    let mut buf = [0u8; SECTOR_SIZE];
    let big = [0x42u8; 2000];
    let count = serialize(&mut buf, &[(0, &big), (1, &big), (2, &big)]);
    assert_eq!(count, 2);
}

#[test]
fn realistic_preset_sizes() {
    let mut buf = [0u8; SECTOR_SIZE];
    let preset_a = [0xAA; 133];
    let preset_b = [0xBB; 123];
    let preset_c = [0xCC; 124];

    let count = serialize(&mut buf, &[(0, &preset_a), (1, &preset_b), (2, &preset_c)]);
    assert_eq!(count, 3);

    assert_eq!(find_one(&buf, 0).unwrap().len(), 133);
    assert_eq!(find_one(&buf, 1).unwrap().len(), 123);
    assert_eq!(find_one(&buf, 2).unwrap().len(), 124);
}

#[test]
fn corrupted_data_is_skipped() {
    let mut buf = [0u8; preset_format::SECTOR_SIZE];
    preset_format::serialize(&mut buf, &[(0, b"good"), (1, b"also good")]);

    // Corrupt preset 0's data (flip a byte after the header)
    buf[9] ^= 0xFF; // data byte of preset 0

    // Preset 0 should be skipped (bad checksum), preset 1 still valid
    assert_eq!(preset_format::find_one(&buf, 0), None);
    assert_eq!(preset_format::find_one(&buf, 1), Some(b"also good".as_slice()));
}

#[test]
fn totally_garbage_flash_returns_nothing() {
    // Simulate leftover data from a different format
    let mut buf = [0x42u8; preset_format::SECTOR_SIZE];
    buf[0..4].copy_from_slice(&0x5045_4442u32.to_le_bytes()); // valid magic
    buf[4] = 3; // claims 3 entries but data is garbage

    let mut count = 0;
    preset_format::parse(&buf, |_, _| count += 1);
    // Should not call back for any entry (checksums won't match)
    assert_eq!(count, 0);
}
