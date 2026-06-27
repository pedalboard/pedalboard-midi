
.PHONY: help clean run build lint attach flash-probe flash-bridge uf2 deploy release log-midi device reboot

.DEFAULT_GOAL := help

RELEASE_LEVEL ?= patch

run: ## build and run by using pico_probe
	cargo run --release $(PATCHES)

clean: ## clean build results
	cargo clean

OPENDECK_PATCH := --config 'patch."https://github.com/pedalboard/opendeck".opendeck.path="../opendeck"'
PROTOCOL_PATCH := --config 'patch."https://github.com/pedalboard/pedalboard-protocol".pedalboard-protocol.path="../pedalboard-protocol"'
PATCHES := $(OPENDECK_PATCH) $(PROTOCOL_PATCH)

build: ## build
	cargo build --release $(PATCHES)

lint: ## lint source code
	cargo clippy --all-features $(PATCHES)

attach: ## attach to the running program
	probe-rs attach --chip RP2040  ./target/thumbv6m-none-eabi/release/pedalboard-midi

flash: uf2 ## flash firmware via bridge DFU (over network)
	ssh laenzi@cm5-dev.home "amidi -p hw:2,0,0 -S 'F0 00 53 43 00 00 01 F7' -d -t 2 && amidi -p hw:2,0,0 -S 'F0 00 53 43 00 00 55 F7'"
	@echo "Waiting for UF2 mount..."
	ssh laenzi@cm5-dev.home "while [ ! -f /media/laenzi/RPI-RP2/INFO_UF2.TXT ]; do sleep 1; done"
	scp target/thumbv6m-none-eabi/release/pedalboard-midi.uf2 laenzi@cm5-dev.home:/media/laenzi/RPI-RP2/
	ssh laenzi@cm5-dev.home "sync"
	@echo "Flashed via bridge."

flash-probe: build ## flash via debug probe (SWD, development only)
	probe-rs download --chip RP2040 --protocol swd target/thumbv6m-none-eabi/release/pedalboard-midi
	probe-rs reset --chip RP2040 --protocol swd

device:
	$(eval DEVICE := $(shell amidi -l | grep pedalboard |  awk '{ print $$2 }'))

reboot: device ## reboot the RP2040
	-aconnect -d 128:1 16:0
	amidi -S 'F0 00 53 43 00 00 7F F7' -p "$(DEVICE)"
	-aconnect 128:1 16:0

log-midi: device ## log the midi traffic coming from USB
	@amidi -p "$(DEVICE)" -d

uf2: ## build uf2
	cargo build --release $(PATCHES)
	elf2uf2-rs ./target/thumbv6m-none-eabi/release/pedalboard-midi

release: ## create a new release (RELEASE_LEVEL=minor make release)
	cargo clean
	cargo release --no-publish --execute ${RELEASE_LEVEL}
	$(MAKE) uf2
	gh release create --latest --generate-notes $$(git describe --tags --abbrev=0) ./target/thumbv6m-none-eabi/release/pedalboard-midi.uf2

help:
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-30s\033[0m %s\n", $$1, $$2}'
