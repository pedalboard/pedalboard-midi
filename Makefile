
.PHONY: help install install-latest-release device mount clean run build lint debug device log-midi uf2 deploy

.DEFAULT_GOAL := help

RELEASE_LEVEL ?= patch

MOUNT_POINT ?= "/mnt/pico"

mount: ## mount the RP2040 in bootsel mode
	@while [ ! -L /dev/disk/by-label/RPI-RP2 ] ; \
	do \
		sleep 1; \
		echo -n '.'; \
	done
	@echo ""
	sudo mount -o uid=1000,gid=1000 /dev/disk/by-label/RPI-RP2 $(MOUNT_POINT)

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

flash-probe: build ## flash via debug probe (SWD)
	probe-rs download --chip RP2040 --protocol swd target/thumbv6m-none-eabi/release/pedalboard-midi
	probe-rs reset --chip RP2040 --protocol swd

flash-bridge: uf2 ## flash via bridge DFU (over network)
	ssh laenzi@cm5-dev.home "amidi -p hw:2,0,0 -S 'F0 00 53 43 00 00 01 F7' -d -t 2 && amidi -p hw:2,0,0 -S 'F0 00 53 43 00 00 55 F7'"
	@echo "Waiting for UF2 mount..."
	ssh laenzi@cm5-dev.home "while [ ! -f /media/laenzi/RPI-RP2/INFO_UF2.TXT ]; do sleep 1; done"
	scp target/thumbv6m-none-eabi/release/pedalboard-midi.uf2 laenzi@cm5-dev.home:/media/laenzi/RPI-RP2/
	ssh laenzi@cm5-dev.home "sync"
	@echo "Flashed via bridge."

device:
	$(eval DEVICE := $(shell amidi -l | grep pedalboard |  awk '{ print $$2 }'))

bootsel: device ## restart the RP2040 in bootsel mode
	-aconnect -d 128:1 16:0
	amidi -S 'F0 00 53 43 00 00 55 F7' -p "$(DEVICE)"
	-aconnect 128:1 16:0

reboot: device ## reboot the RP2040
	-aconnect -d 128:1 16:0
	amidi -S 'F0 00 53 43 00 00 7F F7' -p "$(DEVICE)"
	-aconnect 128:1 16:0


install-latest-release: bootsel mount ## install the latest release from github
	curl -L https://github.com/pedalboard/pedalboard-midi/releases/latest/download/pedalboard-midi.uf2 -o $(MOUNT_POINT)/pm.uf2

install: ## mount and install built uf2 to RP2040
	cp ./target/thumbv6m-none-eabi/release/pedalboard-midi.uf2 $(MOUNT_POINT)

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

usbip-host: ## run usbip host on the device
	pgrep usbipd || sudo usbipd -D
	sudo modprobe usbip_host
	sudo usbip bind --busid 1-1.1

usbip-attach: ## attach the device to the host
	sudo modprobe vhci-hcd
	sudo usbip attach -r pi-dev -b 1-1.1

deploy: ## build, copy, and flash to cm5-dev (stops/restarts bridge)
	./deploy.sh

help:
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-30s\033[0m %s\n", $$1, $$2}'
