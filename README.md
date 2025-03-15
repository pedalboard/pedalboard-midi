# Pedalboard Midi Controller

Firmware for the Open Pedalboard MIDI controller.

The firmware is compatible with the [OpenDeck MIDI platform](https://github.com/shanteacontrols/OpenDeck)
and can therefore be configured using the [OpenDeck configurator](https://config.shanteacontrols.com/#/).

More details about the OpenDeck configuration can be found [in the OpenDeck Wiki](https://github.com/shanteacontrols/OpenDeck/wiki/)

## Open Hardware

See [Pedalboard HW](https://github.com/pedalboard/pedalboard-hw) for more details.

![Schematic](https://pedalboard.github.io/pedalboard-hw-site/latest/Schematic/pedalboard-hw-MIDI.svg)

## Development

This project was generated with the [RP2040 Project Template](https://github.com/rp-rs/rp2040-project-template)

### Dependencies

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add thumbv6m-none-eabi
cargo install flip-link
sudo apt-get install libudev-dev
cargo install elf2uf2-rs
cargo install probe-rs --features cli
```

## Installation (from Linux Host)

The firmware supports No-Button-Boot (nbb) bootsel mode via USB midi interface.

See `make bootsel` for how to send a midi message to reset the device into
bootsel mode.

1. Connect USB C cable to the Pico
2. Run `make uf2 install`
3. Remove USB C cable from the Pico
