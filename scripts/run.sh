#!/usr/bin/env bash
# Flash the board (reboots it into BOOTSEL itself) and attach to its USB serial log.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "Flashing…"
cargo run --release

echo "Waiting for serial port…"
until port=$(compgen -G '/dev/cu.usbmodem*' | head -n1); do sleep 0.5; done

echo "Attaching to $port (quit: Ctrl-A then K)"
exec screen "$port" 115200
