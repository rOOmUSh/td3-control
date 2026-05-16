#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
BIN="$ROOT/target/release/td3-control"

if [[ ! -f "$BIN" ]]; then
    echo "Release binary not found: $BIN" >&2
    echo "Build it first with: cargo build --release" >&2
    exit 1
fi

chmod +x "$BIN"

if [[ -e /dev/snd/seq && ! -r /dev/snd/seq ]]; then
    echo "MIDI access is blocked for user '$USER'." >&2
    echo "Run this once, then log out and back in:" >&2
    echo "  sudo usermod -aG audio $USER" >&2
    echo >&2
    echo "After that command, you can avoid logging out by starting a new audio-group shell:" >&2
    echo "  newgrp audio" >&2
    exit 1
fi

cd "$ROOT"
exec "$BIN" "$@"
