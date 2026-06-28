# ADR 002: Unsafe Code Policy

## Status

Accepted (2026-06-28)

## Context

Embedded Rust on the RP2040 requires some `unsafe` for direct hardware access — flash ROM routines must execute with interrupts disabled, and ISR-context static buffers cannot use safe interior mutability patterns without allocation.

## Decision

Keep `unsafe` usage to the absolute minimum. Every `unsafe` block must have a clear soundness justification. No new `unsafe` is added without documenting why safe alternatives are insufficient.

### Current Inventory (5 blocks)

**`src/storage.rs` (3 blocks):**
1. `core::ptr::read_volatile` — read flash via XIP memory-mapped address (no safe flash-read API on RP2040)
2. `rp2040_flash::flash::flash_range_erase` — ROM call, must run with interrupts disabled (`cortex_m::interrupt::free`)
3. `rp2040_flash::flash::flash_range_program` — ROM call, same constraint

**`src/main.rs` (2 blocks):**
4. `&mut *core::ptr::addr_of_mut!(GET_BUF)` — access ISR-local static buffer for PE Get serialization (cannot use `RefCell` in ISR context without critical section overhead)
5. `&(*core::ptr::addr_of!(GET_BUF))[..len]` — read back the serialized slice from the same static buffer

## Consequences

- All unsafe blocks are confined to two files and are trivially auditable
- Flash operations are inherently unsafe on RP2040; this is an accepted platform constraint
- The static buffer pattern in `main.rs` is scoped within a single ISR and protected by the RTIC priority ceiling — no data race is possible
- Policy: any PR adding `unsafe` must include a `// SAFETY:` comment explaining the soundness argument
