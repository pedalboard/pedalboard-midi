#!/bin/bash
set -e

HOST="laenzi@cm5-dev.home"
UF2="./target/thumbv6m-none-eabi/release/pedalboard-midi.uf2"
REMOTE_UF2="/tmp/pedalboard-midi.uf2"
MOUNT="/media/laenzi/RPI-RP2"

# 1. Build UF2
echo "==> Building UF2..."
make uf2

# 2. Copy UF2 to cm5-dev
echo "==> Copying UF2 to $HOST..."
scp "$UF2" "$HOST:$REMOTE_UF2"

# 3. Stop bridge, send bootloader SysEx, flash, restart bridge
echo "==> Flashing on $HOST..."
ssh "$HOST" bash -s <<'EOF'
set -e
MOUNT="/media/laenzi/RPI-RP2"
UF2="/tmp/pedalboard-midi.uf2"

# Stop bridge to release MIDI port
sudo systemctl stop opendeck-bridge

# Send handshake + bootloader command
amidi -p hw:0,0,0 -S 'F0 00 53 43 00 00 01 F7' -d -t 2 || true
sleep 0.5
amidi -p hw:0,0,0 -S 'F0 00 53 43 00 00 55 F7'

# Wait for UF2 mount
echo -n "Waiting for RPI-RP2 mount"
for i in $(seq 1 30); do
    if [ -d "$MOUNT" ]; then
        echo " OK"
        break
    fi
    echo -n "."
    sleep 1
done

if [ ! -d "$MOUNT" ]; then
    echo " TIMEOUT - mount not found"
    sudo systemctl start opendeck-bridge
    exit 1
fi

# Flash
cp "$UF2" "$MOUNT/" && sync
echo "==> Flashed. Waiting for reboot..."
sleep 3

# Restart bridge
sudo systemctl start opendeck-bridge
echo "==> Bridge restarted. Done!"
EOF
