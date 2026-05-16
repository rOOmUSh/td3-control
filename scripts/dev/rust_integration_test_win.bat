@echo off
setlocal EnableExtensions

cd /d "%~dp0..\.."

echo Running TD-3 device integration tests.
echo These tests require a connected TD-3 and will read/write all 64 device patterns.
echo.

cargo test tests::device_integration_test::device_ -- --ignored --test-threads=1 --nocapture
set "STATUS=%ERRORLEVEL%"

pause
exit /b %STATUS%
