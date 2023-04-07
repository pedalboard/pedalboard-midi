# Pedalboard Midi Controller

This project implements a MIDI controller for my pedalboard

The midi devices are daisy chained with MIDI cables in the following order:

```
    MIDI Controller => Plethora X3 (Channel 1) => RC500 (Channel 2) 
```

## Open Hardware

see [Pedalboard HW](https://github.com/pedalboard/pedalboard-hw) for more details.

![Schematic](https://pedalboard.github.io/pedalboard-hw-site/Schematic/pedalboard-hw-MIDI.svg)

## Development
This project was generated with the [RP2040 Project Template](https://github.com/rp-rs/rp2040-project-template)

### Dependencies

```
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add thumbv6m-none-eabi
cargo install flip-link
sudo apt-get install libudev-dev
cargo install elf2uf2-rs
```


## Deployment (from Linux Host)

The firmware supports No-Button-Boot (nbb) bootsel mode via USB midi interface.

see `make bootsel` for how to send a midi message to reset the device into bootsel mode.

1. Connect USB C cable to the Pico
4. Run `make deploy`
5. Remove USB C cable from the Pico
