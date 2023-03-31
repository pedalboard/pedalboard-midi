# Pedalboard Midi Controller

This project implements a MIDI controller for my pedalboard

The midi devices are daisy chained with MIDI cables in the following order:

```
MIDI Controller => Plethora X3 (Channel 1) => RC500 (Channel 2)
```

A prototype can be easyly assembled

## Prototype Hardware 

To test and prototype the firmware, the following setup can be used:

### BOM

* 1 x Rasperry PI [Pico](https://www.raspberrypi.com/products/raspberry-pi-pico/)
* 1 x Adafruit [MIDI Feather Wing](https://www.adafruit.com/product/4740) 
* 2 x SensorKit [KY-40 Rotary Encoder](https://sensorkit.joy-it.net/en/sensors/ky-040)
* 1 x Adafruit [NeoPixel Strand](https://www.adafruit.com/product/3631) (only 10 pixels are used)

### Wiring (Frizing)

![Breadboard Wiring](doc/wiring.png)


## Open Hardware
After building a prototype some improvments and more professional hardware is built as part of the 
[Pedalboard HW](https://github.com/pedalboard/pedalboard-hw) project

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
