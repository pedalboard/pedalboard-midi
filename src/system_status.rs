/// System-level status messages shown on both displays.
/// These are shown as full-screen overlays before destructive operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemStatus {
    /// Device is about to reboot.
    Rebooting,
    /// Device is entering UF2 bootloader for firmware update.
    Bootloader,
    /// Factory reset in progress (erasing flash + EEPROM).
    FactoryReset,
}

impl SystemStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Rebooting => "Rebooting...",
            Self::Bootloader => "Entering\nBootloader\n...",
            Self::FactoryReset => "Factory\nReset...",
        }
    }
}
