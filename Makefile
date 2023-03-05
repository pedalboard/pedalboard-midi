
.PHONY: help install restart stop start status sushi-logs

.DEFAULT_GOAL := help


mount: ## mount the RP2040 in bootsel mode
	@while [ ! -L /dev/disk/by-label/RPI-RP2 ] ; \
	do \
		sleep 1; \
		echo -n '.'; \
	done
	@echo ""
	sudo mount -o uid=1000,gid=1000 /dev/disk/by-label/RPI-RP2 /mnt/pico/

run: ## build and run
	cargo run --release

device:
	$(eval DEVICE := $(shell amidi -l | grep pedalboard-midi |  awk '{ print $$2 }'))

bootsel: device ## restart the RP2040 in bootsel mode
	amidi -S '8F 00 00' -p "$(DEVICE)"

install-latest:
	curl -L https://github.com/pedalboard/pedalboard-midi/releases/latest/download/pedalboard-midi.uf2 -o /mnt/pico/pm.uf3

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

