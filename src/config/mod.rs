use heapless::Vec;

pub const SYSEX_BUFFER_SIZE: usize = 64;

pub type SysexBuffer = Vec<u8, SYSEX_BUFFER_SIZE>;

/// Processes a SysEx request and returns an optional response.
pub fn process_sysex(request: &[u8]) -> Option<SysexBuffer> {
    /// Identity request message.
    ///
    /// See section *DEVICE INQUIRY* of the *MIDI 1.0 Detailed Specification* for further details.
    const IDENTITY_REQUEST: [u8; 6] = [0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7];

    if request == IDENTITY_REQUEST {
        let mut response = Vec::<u8, SYSEX_BUFFER_SIZE>::new();
        response
            .extend_from_slice(&[
                0xF0, 0x7E, 0x7F, 0x06, 0x02, // Header
                0x01, // Manufacturer ID
                0x02, // Family code
                0x03, // Family code
                0x04, // Family member code
                0x05, // Family member code
                0x00, // Software revision level
                0x00, // Software revision level
                0x00, // Software revision level
                0x00, // Software revision level
                0xF7,
            ])
            .ok();

        return Some(response);
    }

    None
}
