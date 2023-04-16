
.PHONY: help install install-latest restart stop start status sushi-logs device

.DEFAULT_GOAL := help

MOUNT_POINT = "/mnt/pico"

mount: ## mount the RP2040 in bootsel mode
	@while [ ! -L /dev/disk/by-label/RPI-RP2 ] ; \
	do \
		sleep 1; \
		echo -n '.'; \
	done
	@echo ""
	sudo mkdir -p $(MOUNT_POINT)
	sudo mount -o uid=1000,gid=1000 /dev/disk/by-label/RPI-RP2 $(MOUNT_POINT)

run: ## build and run
	cargo run --release

device:
	$(eval DEVICE := $(shell amidi -l | grep pedalboard-midi |  awk '{ print $$2 }'))

bootsel: device ## restart the RP2040 in bootsel mode
	-aconnect -d 128:1 16:0 
	amidi -S '8F 00 00' -p "$(DEVICE)"
	aconnect 128:1 16:0

install-latest:
	curl -L https://github.com/pedalboard/pedalboard-midi/releases/latest/download/pedalboard-midi.uf2 -o $(MOUNT_POINT)/pm.uf2

install: bootsel mount install-latest ## mount and install code to RP2040

log-midi: device ## log the midi traffic coming from USB
	@amidi -p "$(DEVICE)" -d	 

release:
	cargo clean
	cargo release --no-publish --execute patch
	cargo build --release
	elf2uf2-rs ./target/thumbv6m-none-eabi/release/pedalboard-midi
	gh release create --latest --generate-notes $$(git describe --tags --abbrev=0) ./target/thumbv6m-none-eabi/release/pedalboard-midi.uf2

help:
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-30s\033[0m %s\n", $$1, $$2}'

