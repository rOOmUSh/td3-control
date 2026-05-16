#!/usr/bin/env bash
set -u

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
UI_DIR="$ROOT/ui"
TOTAL=0
FAILED=0

if ! command -v node >/dev/null 2>&1; then
    echo "ERROR: node was not found on PATH."
    exit 1
fi

if [ ! -d "$UI_DIR" ]; then
    echo "ERROR: ui directory was not found: \"$UI_DIR\""
    exit 1
fi

echo "Running UI JavaScript tests..."
echo

while IFS= read -r -d '' f; do
    TOTAL=$((TOTAL + 1))
    echo "== $f =="
    if ! node "$f"; then
        FAILED=$((FAILED + 1))
        echo "FAILED: $f"
    fi
    echo
done < <(find "$UI_DIR" -type f -name '*.test.js' -print0)

echo "UI test summary: $TOTAL files run, $FAILED failed."

if [ "$TOTAL" -eq 0 ]; then
    echo "ERROR: no UI test files were found."
    exit 1
fi

if [ "$FAILED" -ne 0 ]; then
    exit 1
fi

exit 0
