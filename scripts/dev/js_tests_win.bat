@echo off
setlocal EnableExtensions DisableDelayedExpansion

set "ROOT=%~dp0..\.."
for %%I in ("%ROOT%") do set "ROOT=%%~fI"
set "UI_DIR=%ROOT%\ui"
set "TOTAL=0"
set "FAILED=0"

where node >nul 2>nul
if errorlevel 1 (
    echo ERROR: node was not found on PATH.
    exit /b 1
)

if not exist "%UI_DIR%\" (
    echo ERROR: ui directory was not found: "%UI_DIR%"
    exit /b 1
)

echo Running UI JavaScript tests...
echo.

for /r "%UI_DIR%" %%F in (*.test.js) do (
    set /a TOTAL+=1
    echo == %%F ==
    node "%%F"
    if errorlevel 1 (
        set /a FAILED+=1
        echo FAILED: %%F
    )
    echo.
)

echo UI test summary: %TOTAL% files run, %FAILED% failed.

if "%TOTAL%"=="0" (
    echo ERROR: no UI test files were found.
    exit /b 1
)

if not "%FAILED%"=="0" (
    exit /b 1
)

pause
