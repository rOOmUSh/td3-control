REM set RUST_LOG=info
cd /d "%~dp0..\.."
cargo build --release
target\release\td3-control control --scratch-pattern G1P1A
