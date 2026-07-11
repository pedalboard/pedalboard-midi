// Host-side tests for src/pe_sysex.rs

/// Match the firmware's MAX_PRESET_SIZE constant.
pub const MAX_PRESET_SIZE: usize = 256;

#[path = "../../src/persist.rs"]
mod persist;

#[path = "../../src/pe_sysex.rs"]
mod pe_sysex;

use midi_controller::config::{GLOBAL_CONFIG_RESOURCE, SYSTEM_COMMAND_RESOURCE};
use midi_controller::property_exchange;
use pe_sysex::handle_set;
use persist::PersistCommand;

const SRC_MUID: [u8; 4] = [0x10, 0x20, 0x30, 0x40];
const DST_MUID: [u8; 4] = [0x01, 0x02, 0x03, 0x04];

#[test]
fn handle_set_reboot_command() {
    let body = [0x01u8]; // SystemCommand::Reboot
    let msg = property_exchange::build_set_inquiry(
        SRC_MUID,
        DST_MUID,
        0x01,
        SYSTEM_COMMAND_RESOURCE,
        &body,
    );
    let result = handle_set(&msg).expect("should parse valid set property");
    assert!(
        matches!(result.command, Some(PersistCommand::Reboot)),
        "expected Reboot, got {:?}",
        result.command
    );
}

#[test]
fn handle_set_bootloader_command() {
    let body = [0x02u8]; // SystemCommand::Bootloader
    let msg = property_exchange::build_set_inquiry(
        SRC_MUID,
        DST_MUID,
        0x01,
        SYSTEM_COMMAND_RESOURCE,
        &body,
    );
    let result = handle_set(&msg).expect("should parse valid set property");
    assert!(
        matches!(result.command, Some(PersistCommand::Bootloader)),
        "expected Bootloader, got {:?}",
        result.command
    );
}

#[test]
fn handle_set_factory_reset_command() {
    let body = [0x03u8]; // SystemCommand::FactoryReset
    let msg = property_exchange::build_set_inquiry(
        SRC_MUID,
        DST_MUID,
        0x01,
        SYSTEM_COMMAND_RESOURCE,
        &body,
    );
    let result = handle_set(&msg).expect("should parse valid set property");
    assert!(
        matches!(result.command, Some(PersistCommand::EraseAll)),
        "expected EraseAll, got {:?}",
        result.command
    );
}

#[test]
fn handle_set_preset_upload() {
    let preset_data: [u8; 8] = [0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80];
    let resource = 0u8;
    let msg =
        property_exchange::build_set_inquiry(SRC_MUID, DST_MUID, 0x01, resource, &preset_data);
    let result = handle_set(&msg).expect("should parse valid set property");
    match result.command {
        Some(PersistCommand::SavePreset(idx, ref blob)) => {
            assert_eq!(idx, 0);
            assert_eq!(blob.as_slice(), &preset_data);
        }
        other => panic!("expected SavePreset(0, ...), got {:?}", other),
    }
}

#[test]
fn handle_set_global_config() {
    let config_data: [u8; 5] = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE];
    let msg = property_exchange::build_set_inquiry(
        SRC_MUID,
        DST_MUID,
        0x01,
        GLOBAL_CONFIG_RESOURCE,
        &config_data,
    );
    let result = handle_set(&msg).expect("should parse valid set property");
    match result.command {
        Some(PersistCommand::SavePreset(idx, ref blob)) => {
            assert_eq!(idx, GLOBAL_CONFIG_RESOURCE);
            assert_eq!(blob.as_slice(), &config_data);
        }
        other => panic!(
            "expected SavePreset(GLOBAL_CONFIG_RESOURCE, ...), got {:?}",
            other
        ),
    }
}

#[test]
fn handle_set_invalid_sysex() {
    let garbage: [u8; 5] = [0x01, 0x02, 0x03, 0x04, 0x05];
    let result = handle_set(&garbage);
    assert!(result.is_none(), "garbage bytes should return None");
}

#[test]
fn handle_set_reply_contains_correct_request_id() {
    let req_id = 0x42u8;
    let body = [0x01u8]; // Reboot command
    let msg = property_exchange::build_set_inquiry(
        SRC_MUID,
        DST_MUID,
        req_id,
        SYSTEM_COMMAND_RESOURCE,
        &body,
    );
    let result = handle_set(&msg).expect("should parse valid set property");
    // request_id is at offset 14 in the reply SysEx
    let reply_req_id = property_exchange::request_id(&result.reply);
    assert_eq!(reply_req_id, req_id, "reply should echo the request_id");
}
