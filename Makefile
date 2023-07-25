
.PHONY: help install install-latest restart stop start status sushi-logs device

.DEFAULT_GOAL := help

RELEASE_LEVEL ?= patch

MOUNT_POINT = "/mnt/pico"

mount: ## mount the RP2040 in bootsel mode
	@while [ ! -L /dev/disk/by-label/RPI-RP2 ] ; \
	do \
		sleep 1; \
		echo -n '.'; \
	done
	@echo ""
	sudo mount -o uid=1000,gid=1000 /dev/disk/by-label/RPI-RP2 $(MOUNT_POINT)

run: ## build and run by using pico_probe
	cargo run --release

clean: ## clean build results
	cargo clean

build: ## build
	cargo build --release

lint: ## lint source code
	cargo clippy

debug: ## build and run by installing uf2 on the mounted pico
	cargo run --config 'runner = "probe-run --chip RP2040"' --release

device:
	$(eval DEVICE := $(shell amidi -l | grep pedalboard-midi |  awk '{ print $$2 }'))

bootsel: device ## restart the RP2040 in bootsel mode
	-aconnect -d 128:1 16:0 
	amidi -S '8F 00 00' -p "$(DEVICE)"
	-aconnect 128:1 16:0

install-latest:
	curl -L https://github.com/pedalboard/pedalboard-midi/releases/latest/download/pedalboard-midi.uf2 -o $(MOUNT_POINT)/pm.uf2

install: bootsel mount install-latest ## mount and install code to RP2040

log-midi: device ## log the midi traffic coming from USB
	@amidi -p "$(DEVICE)" -d	 

uf2: ## build uf2
	cargo build --release
	elf2uf2-rs ./target/thumbv6m-none-eabi/release/pedalboard-midi

release: ## create a new release (RELEASE_LEVEL=minor make release)
	cargo clean
	cargo release --no-publish --execute ${RELEASE_LEVEL}
	$(MAKE) uf2
	gh release create --latest --generate-notes $$(git describe --tags --abbrev=0) ./target/thumbv6m-none-eabi/release/pedalboard-midi.uf2

help:
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-30s\033[0m %s\n", $$1, $$2}'

