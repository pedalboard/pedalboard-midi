# Pedalboard Midi Controller

The firmware is currently in avery prototype state and specific to a use case:

```
    MIDI Controller => Plethora X3 (Channel 1) => RC500 (Channel 2) 
```

The project currenly can be used as a starting point for your own pedalboard.

It is planned to split the reusable parts into a library or make the firmware more configurable.

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
