# ADR 001: Use sequential-storage for Preset Persistence

## Status

Accepted (2026-06-28)

## Context

Presets were originally stored in a single raw 4KB flash sector with a manual format:

```
[magic: 4B][version: 1B][count: 1B][entries: variable...]
```

This approach had several problems:
- No wear leveling — the same sector was erased on every save (~100K cycle limit)
- No integrity checking — a power loss during write corrupted the entire sector
- Separate codepath from the existing OpenDeck config storage

Meanwhile, the OpenDeck hardware config already used `sequential-storage` (map mode) with full wear leveling across 16 sectors.

## Decision

Unify preset storage with the existing `ConfigStore` backed by `sequential-storage`. Presets are stored as variable-length postcard-serialized blobs under keys `0x8000 | index`, sharing the same 64KB flash region and wear-leveling pool as OpenDeck config entries.

## Consequences

- **Wear leveling:** writes distribute across all 16 sectors instead of hammering one
- **CRC integrity:** sequential-storage validates entries on read; corrupt entries are skipped
- **Unified code:** single `ConfigStore` handles both config values and preset blobs
- **Async-only access:** sequential-storage's async API means all flash I/O runs in the `persist` task; callers send commands via channel
- **Trade-off:** slightly higher per-entry overhead (key + length + CRC headers) vs raw format, but well within the 64KB budget given ~130 bytes per serialized preset
