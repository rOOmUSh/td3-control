@echo off
setlocal EnableExtensions

cd /d "%~dp0..\.."
cargo test --no-fail-fast -- --test-threads=1
pause
