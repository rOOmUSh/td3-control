#!/usr/bin/env bash
# export RUST_LOG=info
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
cd "$ROOT"

cargo build --release
./target/release/td3-control control --scratch-pattern G1P1A
