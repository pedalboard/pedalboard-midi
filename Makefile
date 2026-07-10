
.PHONY: help clean run build lint attach flash flash-probe uf2 monitor reboot deploy release

.DEFAULT_GOAL := help

RELEASE_LEVEL ?= patch

run: ## build and run by using pico_probe
	cargo run --release $(PATCHES)

clean: ## clean build results
	cargo clean

PROTOCOL_PATCH := --config 'patch."https://github.com/pedalboard/midi-controller".midi-controller.path="../midi-controller"'
PATCHES := $(PROTOCOL_PATCH)

build: ## build
	cargo build --release $(PATCHES)

lint: ## lint source code
	cargo clippy --all-features $(PATCHES)

attach: ## attach to the running program
	probe-rs attach --chip RP2040  ./target/thumbv6m-none-eabi/release/pedalboard-midi

flash: uf2 ## flash firmware via CLI (bootloader + HTTP upload to bridge)
	cd ../pedalboard-cli && cargo run -q $(PROTOCOL_PATCH) -- flash \
		../pedalboard-midi/target/thumbv6m-none-eabi/release/pedalboard-midi.uf2

flash-probe: build ## flash via debug probe on CM5 (SWD)
	scp target/thumbv6m-none-eabi/release/pedalboard-midi laenzi@cm5-dev.home:/tmp/
	ssh laenzi@cm5-dev.home "probe-rs download --chip RP2040 --protocol swd /tmp/pedalboard-midi"

monitor: ## monitor MIDI output in real-time
	cd ../pedalboard-cli && cargo run -q $(PROTOCOL_PATCH) -- monitor

reboot: ## reboot the device
	cd ../pedalboard-cli && cargo run -q $(PROTOCOL_PATCH) -- reboot

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
