@echo off
REM Simple double-click launcher for freeport on Windows, swarm-demo's.
REM Builds the release binary if needed, then runs it from the checkout,
REM which is where the baked sets are found.

cd /d "%~dp0"

echo Building. The last step is LINKING freeport_app.exe, which is a very
echo large link and is the slow part: it can sit on the last line of the build
echo for a while with no output. That is the linker working, not a hang. See
echo .cargo\config.toml for how to make it much faster.
echo.

cargo build --release -p freeport_app
if errorlevel 1 (
    echo Build failed.
    pause
    exit /b 1
)

echo Left click takes the mouse, Escape gives it back. WASD walks, Shift runs,
echo Space jumps, F swaps between walking and flying, Tab toggles the wire.
echo.
target\release\freeport_app.exe %*
pause
