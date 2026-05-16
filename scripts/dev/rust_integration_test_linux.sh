#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
TEST_CMD="cargo test tests::device_integration_test::device_ -- --ignored --test-threads=1 --nocapture"

cd "$ROOT"

if [[ -e /dev/snd/seq && ! -r /dev/snd/seq ]]; then
    if id -nG | tr ' ' '\n' | grep -qx audio; then
        echo "Running TD-3 integration tests through the audio group..."
        exec sg audio -c "$TEST_CMD"
    fi

    echo "MIDI access is blocked for user '$USER'." >&2
    echo "Run this once, then log out and back in:" >&2
    echo "  sudo usermod -aG audio $USER" >&2
    echo >&2
    echo "After that command, you can avoid logging out by starting a new audio-group shell:" >&2
    echo "  newgrp audio" >&2
    exit 1
fi

echo "Running TD-3 device integration tests."
echo "These tests require a connected TD-3 and will read/write all 64 device patterns."
echo

exec bash -lc "$TEST_CMD"
