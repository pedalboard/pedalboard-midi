# Pedalboard Display UI Design

## Hardware

- 2x SSD1327 128x128 OLED (4-bit grayscale)
- Left display: above Vol encoder
- Right display: above Gain encoder
- 6 buttons (A–F) between the displays
- 2 encoders with push-button (Vol, Gain)
- 12-LED rings around each encoder
- 2 single indicator LEDs (Mode, Mon)

## Display Modes

| Mode | Content | When |
|------|---------|------|
| Performance (default) | Button labels + states | Normal operation |
| Debug (MIDI log) | Scrolling MIDI messages | When SysEx session is active (handshake received) |

Keep MIDI log code as-is, just switch which renderer the display task uses.

## Design Goals

1. Performance-focused: at-a-glance status while playing
2. No configuration on-device — all config via web UI (SysEx/WebSocket)
3. Show what each button/encoder does in the current preset
4. Minimal distraction — large, readable, no menus

## Display Layout Proposal

### Left Display (Vol side)

**Default view — preset + buttons A–C:**
```
┌────────────────────┐
│  PRESET 1          │
│  "Clean + Delay"   │
│                    │
│  A: Drive    [ON]  │
│  B: Delay    [OFF] │
│  C: Reverb   [ON]  │
│                    │
│                    │
└────────────────────┘
```

### Right Display (Gain side)

**Default view — buttons D–F + status:**
```
┌────────────────────┐
│                    │
│  D: Looper   [REC] │
│  E: Tap      [---] │
│  F: Bank+    [---] │
│                    │
│                    │
│  BPM: 120          │
└────────────────────┘
```

### Contextual Overlays (temporary, ~1s)

**Encoder turn** — large value overlay on the respective display:
```
┌────────────────────┐
│                    │
│                    │
│       Vol          │
│        72          │
│                    │
│                    │
└────────────────────┘
```

**Preset/bank change** — flash on both displays (~2s), then revert to default view:
```
┌────────────────────┐
│                    │
│    PRESET 2        │
│   "Crunch Lead"    │
│                    │
└────────────────────┘
```

## Data Model

### Per-preset metadata (stored in flash, configured via web UI)

```rust
struct PresetMeta {
    name: String<16>,           // "Clean + Delay"
    button_labels: [String<8>; 6],  // ["Drive", "Delay", "Reverb", "Loop", "Tap", "Bank+"]
    encoder_labels: [String<8>; 2], // ["Vol", "Gain"]
}
```

### Display state (runtime, not persisted)

```rust
struct DisplayState {
    active_preset: usize,
    button_states: [bool; 6],      // from LED output state
    overlay: Option<Overlay>,       // temporary overlay (encoder value, preset change)
    overlay_timeout: u32,           // ms remaining
}

enum Overlay {
    EncoderValue { label: &str, value: u8 },
    PresetChange { name: &str },
    Reboot,
}
```

## Presets & Banks

### Concept
- A **bank** = an OpenDeck **preset** (each has its own button/encoder config)
- Switching bank changes what every button/encoder does
- Active bank shown on display with button labels

### Bank Switching Modes (configurable via web UI)

| Mode | Trigger | Notes |
|------|---------|-------|
| Long press | Hold any configured button >500ms | Button still sends normal MIDI on short press |
| Dedicated buttons | Button(s) assigned as Bank+/Bank- | Uses OpenDeckPresetChange message type |
| Encoder button | Push encoder to cycle banks | Encoder turn still controls CC |
| Encoder turn | Dedicated encoder for bank selection | Encoder doesn't send MIDI in this mode |

Only one mode active at a time. Configured per-device (not per-preset, since it's the mechanism to *reach* other presets).

### Open: Long press (future)
- Not currently supported in the opendeck library (buttons are momentary or latching)
- Likely needs a protocol extension (new button type or section)
- Would need a firmware-level timer that distinguishes short vs long press
- Short press = normal MIDI action, long press = bank change
- Threshold: ~500ms
- Parked until other modes are proven

## Open Questions

1. Where do button labels come from? (Set via web UI, stored in flash per-preset?)
2. Should encoder push toggle display mode or have a MIDI function?
3. How many presets/banks? Fixed count or dynamic?
4. What info matters most during a gig? (Preset name, button labels, encoder values, BPM?)
5. Should displays dim/sleep after inactivity?

## Next Steps

- [ ] Define data model for button labels (per-preset strings in flash)
- [ ] Prototype default view layout in pedalboard-graphics simulator
- [ ] Implement basic preset name + button state display
- [ ] Add contextual encoder value overlay
