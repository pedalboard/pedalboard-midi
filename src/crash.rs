//! HardFault / panic crash recovery.
//!
//! - **Release builds**: writes crash info to a fixed RAM address (survives warm reset),
//!   then triggers an immediate system reset. The device recovers in <100ms.
//! - **Debug builds**: halts for probe inspection (unchanged `panic_probe` behavior).
//!
//! On boot, `check_crash_marker()` detects if the previous run crashed and logs
//! the fault info via defmt before clearing the marker.

/// Fixed RAM address for crash info (end of RAM, outside normal stack/heap).
/// RP2040 RAM: 0x20000000 - 0x2003FFFF (256KB).
/// We use the last 16 bytes: 0x2003FFF0 - 0x2003FFFF.
const CRASH_MARKER_ADDR: u32 = 0x2003_FFF0;
const CRASH_MAGIC: u32 = 0xDEAD_BEEF;

/// Crash info stored in RAM (survives warm reset).
#[repr(C)]
struct CrashInfo {
    magic: u32,
    pc: u32,
    lr: u32,
    xpsr: u32,
}

/// Check for crash marker on boot. If found, log the crash info and clear it.
/// Call this early in init, before any other RAM initialization could overwrite it.
pub fn check_crash_marker() {
    let info = unsafe { &*(CRASH_MARKER_ADDR as *const CrashInfo) };
    if info.magic == CRASH_MAGIC {
        defmt::error!(
            "⚡ Recovered from crash! PC={:08x} LR={:08x} xPSR={:08x}",
            info.pc,
            info.lr,
            info.xpsr
        );
        // Clear the marker so we don't log the same crash forever
        unsafe {
            core::ptr::write_volatile(CRASH_MARKER_ADDR as *mut u32, 0);
        }
    }
}

/// HardFault handler for release builds: save crash context and reset.
#[cfg(not(debug_assertions))]
#[cortex_m_rt::exception]
unsafe fn HardFault(frame: &cortex_m_rt::ExceptionFrame) -> ! {
    // Write crash info to fixed RAM (survives warm reset)
    let info = &mut *(CRASH_MARKER_ADDR as *mut CrashInfo);
    core::ptr::write_volatile(&mut info.magic, CRASH_MAGIC);
    core::ptr::write_volatile(&mut info.pc, frame.pc());
    core::ptr::write_volatile(&mut info.lr, frame.lr());
    core::ptr::write_volatile(&mut info.xpsr, frame.xpsr());

    // Trigger immediate system reset
    cortex_m::peripheral::SCB::sys_reset();
}

/// Panic handler for release builds: treat like HardFault — reset immediately.
/// In debug builds, `panic_probe` handles this (halts + prints via defmt).
#[cfg(not(debug_assertions))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    // Write a marker without detailed frame info (panics don't have ExceptionFrame)
    unsafe {
        let crash = &mut *(CRASH_MARKER_ADDR as *mut CrashInfo);
        core::ptr::write_volatile(&mut crash.magic, CRASH_MAGIC);
        core::ptr::write_volatile(&mut crash.pc, 0xFFFF_FFFF); // unknown
        core::ptr::write_volatile(&mut crash.lr, 0xFFFF_FFFF);
        core::ptr::write_volatile(&mut crash.xpsr, 0);
    }
    cortex_m::peripheral::SCB::sys_reset();
}
