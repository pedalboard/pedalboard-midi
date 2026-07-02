//! Persistence command types shared between OpenDeck and PE paths.

pub const PERSIST_CAPACITY: usize = 32;

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum PersistCommand {
    /// Save an OpenDeck config entry (block, section, index, value).
    #[cfg(feature = "opendeck")]
    Save(u8, u8, u8, u16),
    /// Save a preset or global config blob (resource index, data).
    SavePreset(u8, heapless::Vec<u8, { crate::MAX_PRESET_SIZE }>),
    /// Persist the active preset index.
    SaveActivePreset(u8),
    /// Persist runtime state to EEPROM.
    SaveState(heapless::Vec<u8, 128>),
    /// Factory reset: erase all flash + EEPROM.
    EraseAll,
    /// Reboot the device.
    Reboot,
    /// Enter UF2 bootloader mode.
    Bootloader,
}
