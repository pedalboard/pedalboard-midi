//! PE (Property Exchange) SysEx message handling.
//!
//! Extracted from the USB IRQ handler to keep interrupt context thin.
//! The firmware calls these functions and dispatches the results.

use crate::persist::PersistCommand;
use defmt::debug;
use heapless::Vec;
use midi_controller::config;
use midi_controller::property_exchange;

/// Result of handling a PE Set Property message.
pub struct SetResult {
    /// Persist command to execute (save preset, system command, etc.)
    pub command: Option<PersistCommand>,
    /// ACK reply SysEx to send back via USB.
    pub reply: Vec<u8, 256>,
}

/// Handle a PE Set Property SysEx message.
/// Returns None if the message is not a valid Set Property.
pub fn handle_set(sysex: &[u8]) -> Option<SetResult> {
    if !property_exchange::is_set_property(sysex) {
        return None;
    }

    let data = property_exchange::extract_set_property(sysex)?;

    let mut decoded = [0u8; crate::MAX_PRESET_SIZE];
    let dec_len = property_exchange::decode_mcoded7(data.body, &mut decoded);

    let command = if data.resource == config::SYSTEM_COMMAND_RESOURCE {
        // System command (reboot, bootloader, factory reset)
        if dec_len > 0 {
            config::SystemCommand::from_byte(decoded[0]).map(|cmd| {
                debug!("PE System command: {}", cmd as u8);
                match cmd {
                    config::SystemCommand::Reboot => PersistCommand::Reboot,
                    config::SystemCommand::Bootloader => PersistCommand::Bootloader,
                    config::SystemCommand::FactoryReset => PersistCommand::EraseAll,
                }
            })
        } else {
            None
        }
    } else if data.resource == config::GLOBAL_CONFIG_RESOURCE {
        debug!("PE Set GlobalConfig body len={}", dec_len);
        Vec::from_slice(&decoded[..dec_len])
            .ok()
            .map(|blob| PersistCommand::SavePreset(config::GLOBAL_CONFIG_RESOURCE, blob))
    } else {
        debug!(
            "PE Set Property preset={} body len={}",
            data.resource,
            data.body.len()
        );
        Vec::from_slice(&decoded[..dec_len])
            .ok()
            .map(|blob| PersistCommand::SavePreset(data.resource, blob))
    };

    // Build ACK reply
    let req_id = property_exchange::request_id(sysex);
    let src_muid = property_exchange::source_muid(sysex);
    let reply_data = property_exchange::build_set_reply(
        [0x01, 0x02, 0x03, 0x04],
        src_muid,
        req_id,
        property_exchange::PeStatus::Ok,
    );
    let mut reply = Vec::new();
    reply.extend_from_slice(&reply_data).ok();

    Some(SetResult { command, reply })
}
