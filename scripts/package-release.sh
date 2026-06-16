#!/usr/bin/env bash
set -euo pipefail

VERSION="$1"
PLATFORM="$2"
TARGET="$3"

NAME="td3-control-${VERSION}-${PLATFORM}"
STAGE="dist/${NAME}"
BIN_NAME="td3-control"
[[ "$PLATFORM" == windows* ]] && BIN_NAME="td3-control.exe"

rm -rf "$STAGE"
mkdir -p "$STAGE"

cp "target/${TARGET}/release/${BIN_NAME}" "$STAGE/"

mkdir -p "$STAGE/config"
cp config/default_env.template "$STAGE/config/"
cp README.md LICENSE "$STAGE/"
mkdir -p "$STAGE/docs"
cp docs/FAQ.md "$STAGE/docs/"
if [[ -f docs/images/startup-gui.png ]]; then
    mkdir -p "$STAGE/docs/images"
    cp docs/images/startup-gui.png "$STAGE/docs/images/"
fi

if [[ "$PLATFORM" == windows* ]]; then
    cat > "$STAGE/run.bat" << 'EOF'
@echo off
cd /d "%~dp0"
start "" td3-control.exe
EOF
else
    cat > "$STAGE/run.command" << 'EOF'
#!/usr/bin/env bash
cd "$(dirname "$0")"
./td3-control
EOF
    chmod +x "$STAGE/run.command"
    chmod +x "$STAGE/td3-control"
fi

cd dist
if [[ "$PLATFORM" == windows* ]]; then
    if command -v 7z >/dev/null 2>&1; then
        7z a -tzip "${NAME}.zip" "${NAME}/" >/dev/null
    else
        powershell -NoProfile -Command "Compress-Archive -Path '${NAME}' -DestinationPath '${NAME}.zip' -Force"
    fi
else
    zip -r "${NAME}.zip" "${NAME}/" >/dev/null
fi
echo "dist/${NAME}.zip"
