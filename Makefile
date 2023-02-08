
.PHONY: help install restart stop start status sushi-logs

.DEFAULT_GOAL := help

mount: ## mount the RP2040 in bootsel mode
	@while [ ! -L /dev/disk/by-label/RPI-RP2 ] ; \
	do \
		sleep 1; \
		echo -n '.'; \
	done
	sudo mount -o uid=1000,gid=1000 /dev/disk/by-label/RPI-RP2 /mnt/pico/

run: ## build and run
	cargo run --release

bootsel: ## restart the RP2040 in bootsel mode
	echo "z" > /dev/ttyACM0

deploy: bootsel mount run ## mount and deploy to RP2040

help:
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-30s\033[0m %s\n", $$1, $$2}'

