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

echo The hex world: a disc of Goldberg columns round the eye and Planet-LOD
echo past it, both made in the vertex stage. Pass --chunks for the dual
echo contoured world instead, with its towns and its builder.
echo.
echo Left click takes the mouse, Escape gives it back. On foot: WASD, Shift
echo runs, Space jumps, and the feet stand on a column's own flat top. F
echo swaps to the fly camera and back, Tab toggles the wire.
echo.
target\release\freeport_app.exe %*
pause
