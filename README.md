# Pedalboard MIDI Firmware

RP2040 firmware (Rust / RTIC) for the Open Pedalboard MIDI controller.

## What It Does

Handles all real-time MIDI control: button input, encoder rotation, expression pedals, LED ring rendering, OLED display, DIN + USB MIDI output, and MIDI Clock. Starts instantly on power-up — no boot delay.

## Architecture

- **RTIC v2** async tasks on single core (RP2040)
- **PE (Property Exchange)** for preset config via MIDI-CI SysEx
- **OpenDeck SysEx** for legacy/hardware configuration (being phased out)
- **Flash persistence** via `sequential-storage` (64KB, wear-leveled)
- **EEPROM state** (AT24CS01) for runtime toggle/encoder state across power cycles

See [docs/architecture.md](docs/architecture.md) for task structure, priority levels, and storage design.

For the full multi-module system overview (CLI → Bridge → Firmware), see the [Software Architecture](https://github.com/pedalboard/.github/blob/main/docs/software-architecture.md) doc.

## Configuration

Use [pedalboard-cli](https://github.com/pedalboard/pedalboard-cli) to configure the device:

```bash
pedalboard-cli upload setlist.yaml   # upload presets + global config
pedalboard-cli read 0                # read back a preset
pedalboard-cli monitor               # real-time MIDI output
pedalboard-cli flash firmware.uf2    # flash new firmware
```

Configuration is defined as YAML setlists — version-controlled, diffable, one file per rig.

## Building

```bash
make build       # release build (thumbv6m-none-eabi)
make lint        # clippy
make run         # build + flash via probe-rs (SWD)
```

## Flashing

```bash
make flash         # UF2 via bridge (over network, no probe needed)
make flash-probe   # SWD via local probe-rs
```

## Testing

```bash
# Host tests (protocol + pe_handler + display logic, no hardware needed)
cd tests-host && cargo test

# Integration tests (requires device connected via bridge)
cd ../pedalboard-cli && ./tests/integration.sh
```

## Dependencies

```bash
rustup target add thumbv6m-none-eabi
cargo install flip-link elf2uf2-rs probe-rs-tools
```

## Hardware

See [pedalboard-hw](https://github.com/pedalboard/pedalboard-hw) for the PCB design.

## Acknowledgments

This firmware builds upon the [OpenDeck](https://github.com/shanteacontrols/OpenDeck) platform by [Igor Petrović / Shantea Controls](https://shanteacontrols.com) for its SysEx configuration protocol.

## License

[GPL-3.0](LICENSE)
