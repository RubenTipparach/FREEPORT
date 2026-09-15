#!/usr/bin/env bash
#
# Build and run freeport on Linux or macOS. run.bat is the Windows twin.
#
#   ./run.sh                                     a window, release profile
#   ./run.sh --debug                             the dev profile
#   ./run.sh --build-only                        build, do not run
#   ./run.sh --test                              the suites, then build and run
#   ./run.sh --shot out.png                      no display needed: a picture under Xvfb and lavapipe, then exit
#   ./run.sh -- --fly --wire
#   ./run.sh --shot out.png -- --octaves 14 --eye 0,1030000,0 --look 0,1000000,120000
#
# Everything after -- goes to freeport_app itself: --levels, --wire, --fly,
# --eye, --look, --shot, --frames, --fps, --octaves, --walk.
# The first token this script does not know starts the passthrough too.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$root"

profile=release
run=1
suites=0
shot=""
app_args=()

usage() {
    awk 'NR <= 2 { next } /^#/ { sub(/^# ?/, ""); print; next } { exit }' "${BASH_SOURCE[0]}"
}

while [ $# -gt 0 ]; do
    case "$1" in
        --debug)      profile=debug ;;
        --release)    profile=release ;;
        --build-only) run=0 ;;
        --test)       suites=1 ;;
        --shot)       shot="${2:-}"; [ -n "$shot" ] || { echo "run.sh: --shot needs a file name" >&2; exit 2; }; shift ;;
        -h|--help)    usage; exit 0 ;;
        --)           shift; while [ $# -gt 0 ]; do app_args+=("$1"); shift; done; break ;;
        *)            while [ $# -gt 0 ]; do app_args+=("$1"); shift; done; break ;;
    esac
    shift
done

say()  { printf '\033[36m==\033[0m %s\n' "$*"; }
warn() { printf '\033[33m!!\033[0m %s\n' "$*" >&2; }

# ------------------------------------------------------------ the toolchain --

if ! command -v cargo >/dev/null 2>&1; then
    echo "run.sh: no cargo on PATH. Install Rust from https://rustup.rs and reopen the shell." >&2
    exit 1
fi

os="$(uname -s)"
case "$os" in
    Linux)
        # Bevy links alsa and udev on Linux and wants a Vulkan driver at run
        # time. Missing headers fail the build with a linker error that does
        # not name the package, so say the package here instead.
        if command -v pkg-config >/dev/null 2>&1; then
            missing=""
            for lib in alsa libudev; do
                pkg-config --exists "$lib" || missing="$missing $lib"
            done
            if [ -n "$missing" ]; then
                warn "pkg-config cannot find:$missing"
                warn "  Debian, Ubuntu: sudo apt install libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev mesa-vulkan-drivers"
                warn "  Fedora:         sudo dnf install alsa-lib-devel systemd-devel wayland-devel libxkbcommon-devel mesa-vulkan-drivers"
                warn "  Arch:           sudo pacman -S alsa-lib systemd-libs wayland libxkbcommon vulkan-radeon"
            fi
        fi
        ;;
    Darwin)
        # Metal through wgpu, nothing to install past the command line tools.
        if ! xcode-select -p >/dev/null 2>&1; then
            warn "no Xcode command line tools. Run: xcode-select --install"
        fi
        ;;
    *)
        warn "unrecognised uname '$os'. Carrying on, but this script is written for Linux and macOS."
        ;;
esac

# ------------------------------------------------------------- the suites --

if [ "$suites" = 1 ]; then
    say "cargo test -p freeport_core"
    cargo test -p freeport_core
    if command -v python3 >/dev/null 2>&1; then
        say "the shape of the code"
        python3 tools/shape.py --check
    else
        warn "no python3, skipping the shape check"
    fi
fi

# -------------------------------------------------------------- the build --

build=(cargo build -p freeport_app)
[ "$profile" = release ] && build+=(--release)

say "${build[*]}"
"${build[@]}"

out="target/$profile/freeport_app"
say "built $out"

# ---------------------------------------------------------------- the run --

if [ "$run" = 0 ]; then
    exit 0
fi

cmd=("$out")
cmd+=(${app_args[@]+"${app_args[@]}"})

if [ -n "$shot" ]; then
    # A picture with no display: Xvfb for the window and lavapipe for the
    # device, the rig Material Maker bakes on. Slow, and for looking, not
    # for a frame rate.
    if ! command -v xvfb-run >/dev/null 2>&1; then
        warn "no xvfb-run: sudo apt install xvfb mesa-vulkan-drivers, or run without --shot on a display"
        exit 1
    fi
    icd=/usr/share/vulkan/icd.d/lvp_icd.json
    [ -f "$icd" ] && export VK_ICD_FILENAMES="$icd"
    export WGPU_BACKEND=vulkan
    cmd+=(--shot "$shot")
    say "xvfb-run -a -s '-screen 0 1600x900x24' ${cmd[*]}"
    exec xvfb-run -a -s "-screen 0 1600x900x24" "${cmd[@]}"
fi

say "${cmd[*]}"
exec "${cmd[@]}"
