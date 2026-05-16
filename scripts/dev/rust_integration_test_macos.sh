#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
cd "$ROOT"

echo "Running TD-3 device integration tests."
echo "These tests require a connected TD-3 and will read/write all 64 device patterns."
echo

cargo test tests::device_integration_test::device_ -- --ignored --test-threads=1 --nocapture
