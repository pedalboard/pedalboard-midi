# ADR 003: Simplified Property Exchange Protocol

## Status

Accepted (2026-07-01)

## Context

The pedalboard needs a bidirectional protocol for configuration upload, read-back,
and system commands between the CLI (host) and firmware (RP2040). Requirements:

- Works over USB MIDI (SysEx transport)
- Minimal firmware footprint (no JSON parsing, no heap allocation)
- Reliable framing with request/reply semantics
- Passes through MIDI bridges and routers untouched

Options considered:

1. **Custom SysEx with manufacturer ID** — Own framing from scratch. Full flexibility
   but requires designing and documenting a complete message format.
2. **Full MIDI-CI Property Exchange** — Spec-compliant JSON headers, standard resource
   names. The RP2040 could handle this (264KB RAM, `serde_json_core` exists for
   no_std), but full compliance requires implementing resource discovery, MUID
   negotiation, subscription semantics, and pagination — weeks of engineering
   effort for zero practical benefit since we control both endpoints.
3. **MIDI-CI envelope with simplified addressing** — Use the standard MIDI-CI PE
   SysEx structure (sub-ID2, MUIDs, request IDs, chunk headers) but replace the
   JSON resource header with a single byte.

The deciding factor is **engineering ROI**, not hardware capability. Full MIDI-CI
compliance would only matter for third-party interop (DAW auto-discovery,
generic MIDI-CI editors, multi-device buses). None of these apply to a
self-contained pedalboard controller with its own CLI.

## Decision

Use option 3: MIDI-CI Property Exchange framing with a 1-byte resource identifier
instead of JSON headers.

### Message structure (follows MIDI-CI 1.2 §7)

```
F0 7E <device_id> 0D <sub_id2> <ci_version>
<source_muid: 4B> <dest_muid: 4B>
<request_id: 1B>
<header_len: 2B (7-bit LSB)>
<header: resource_id byte>
<num_chunks: 2B> <chunk_num: 2B>
<body_len: 2B (7-bit LSB)>
<body: mcoded7-encoded payload>
F7
```

### Sub-ID2 values (standard MIDI-CI)

| Sub-ID2 | Direction | Meaning |
|---------|-----------|---------|
| 0x34 | Host → Device | Get Property Inquiry |
| 0x35 | Device → Host | Get Property Reply |
| 0x36 | Host → Device | Set Property Inquiry |
| 0x37 | Device → Host | Set Property Reply (ACK) |

### Resource ID allocation

| ID | Purpose | Body format |
|----|---------|-------------|
| 0x00–0x1F | Preset slots (32 max) | postcard-serialized `Preset` |
| 0x7E | System commands (reserved) | Command enum (future) |
| 0x7F | Global config | postcard-serialized `GlobalConfig` |

### Body encoding

The body carries arbitrary binary data (postcard serialization produces bytes with
bit 7 set). Since SysEx only allows 7-bit bytes, the body is encoded using
**mcoded7** (MIDI-CI spec §5.5.2): every 7 input bytes become 8 output bytes,
with a leading byte carrying the high bits.

### Constraints

All values in the SysEx envelope (resource ID, request_id, MUIDs) must be 7-bit
safe (0x00–0x7F). Only the mcoded7-encoded body section can represent 8-bit data.

## Consequences

**Advantages:**
- Standard MIDI-CI envelope — bridges, routers, and monitors pass messages through
- No JSON parsing on firmware — resource is a single byte comparison
- No heap allocation — fixed-size buffers for encode/decode
- Request/reply with IDs — supports concurrent operations if needed
- mcoded7 is spec-compliant encoding for binary payloads

**Trade-offs:**
- Not interoperable with standard MIDI-CI hosts (DAWs, editors) — they expect
  JSON resource headers. This is acceptable since we control both ends.
- Resource namespace is limited to 128 values. Sufficient for this project.
- If future interop is needed, a translation layer could map our byte IDs to
  JSON resource names without changing the firmware.

**Who speaks this protocol:**
- `pedalboard-cli` (Rust, host) — builds and parses PE messages
- `pedalboard-midi` (Rust, RP2040) — handles PE Set/Get in USB RX ISR
- `pedalboard-bridge` (Go, CM5) — transparent passthrough (treats as opaque SysEx)
- `pedalboard-protocol` (Rust, shared) — framing, encode/decode, types
