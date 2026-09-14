#!/usr/bin/env bash
# Bake every material graph in materials/ to the PNG sets the game and the
# mockups read, with Material Maker itself, so the graph is the source and the
# PNG is its export and there is nothing to keep in step by hand.
#
#   tools/bake_materials.sh                 # every materials/*.ptex
#   tools/bake_materials.sh materials/ice.ptex
#   tools/bake_materials.sh --check         # re-export and compare against what is committed
#   tools/bake_materials.sh --size 512      # the maps at another size (1024 is the default)
#
# Each graph exports four maps in Material Maker's Godot layout, which is the
# glTF one Bevy reads without conversion: <name>_albedo.png, <name>_normal.png
# (OpenGL, green up), <name>_orm.png (occlusion in red, roughness in green,
# metallic in blue) and <name>_heightmap.png.
#
# Material Maker is a Godot app and renders its graphs on the GPU, so this
# needs a display and a Vulkan device. On a headless box that is Xvfb and
# Mesa's lavapipe (apt: mesa-vulkan-drivers), which is what this script uses
# when there is no DISPLAY, and the forward_plus renderer, because the mobile
# and GL ones crash under lavapipe while this one exports cleanly. The export
# does not exit on its own once its files are written, so this waits for every
# expected file to appear and hold still, then stops it.
#
# The export is written at 2048 whatever the graph's material node says
# (measured, on a graph at size 10 and one at 11), so every map is box
# filtered down to --size by tools/shrink_png.py on the way in: a smaller
# file, and the one resample that is bit exact on every machine.
#
# Bake ALL the graphs in one run, which is the default. Handed one graph on
# its own, Material Maker's command line loaded it and then sat idle for
# ever, three times out of three here, while every run given the whole set
# exported all of them inside four minutes. Nothing in its log says why.
#
# --check is a TOLERANCE and not `cmp`: a GPU render is not bit exact between
# drivers, so a map is judged by the share of its pixels that moved more than a
# little, off tools/pngdiff.py, against a floor a re-export on the same machine
# stays under.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
mm="${MATERIAL_MAKER:-$root/tools/material_maker/material_maker.x86_64}"
out="$root/assets/textures/terrain"
check=0
size=1024
files=()
while [ $# -gt 0 ]; do
    case "$1" in
        --check) check=1 ;;
        --size) size="$2"; shift ;;
        *) files+=("$1") ;;
    esac
    shift
done
[ ${#files[@]} -gt 0 ] || files=("$root"/materials/*.ptex)
[ -x "$mm" ] || { echo "bake_materials: no Material Maker at $mm; run tools/get_material_maker.sh or set MATERIAL_MAKER" >&2; exit 2; }

say() { printf '\033[36m==\033[0m %s\n' "$*"; }
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
export HOME="$work/home"; mkdir -p "$HOME"
runner=()
if [ -z "${DISPLAY:-}" ]; then
    command -v xvfb-run >/dev/null || { echo "bake_materials: no DISPLAY and no xvfb-run" >&2; exit 2; }
    runner=(xvfb-run -a -s "-screen 0 1600x900x24")
fi

expected=()
for f in "${files[@]}"; do
    n="$(basename "$f" .ptex)"
    for m in albedo normal orm heightmap; do expected+=("$work/${n}_${m}.png"); done
done

say "exporting ${#files[@]} graph(s) with Material Maker"
# In its own session, so the whole tree (xvfb-run, Xvfb, Material Maker) is
# one process group and one kill takes all of it: killing xvfb-run alone
# leaves Material Maker running at a quarter of a core for ever, which is how
# three of them came to be fighting over four cores the first time.
setsid "${runner[@]}" "$mm" --rendering-driver vulkan --rendering-method forward_plus \
    --export-material --target Godot -o "$work" "${files[@]}" > "$work/mm.log" 2>&1 &
pid=$!
deadline=$((SECONDS + 1200))
settled=0
last=""
while [ $SECONDS -lt $deadline ]; do
    sleep 3
    if ! kill -0 $pid 2>/dev/null; then break; fi
    all=1
    for e in "${expected[@]}"; do [ -s "$e" ] || { all=0; break; }; done
    if [ $all -eq 1 ]; then
        now="$(ls -l "${expected[@]}" | awk '{print $5}' | tr '\n' ' ')"
        if [ "$now" = "$last" ]; then settled=$((settled + 1)); else settled=0; fi
        last="$now"
        [ $settled -ge 2 ] && break
    fi
done
kill -TERM -- -$pid 2>/dev/null || kill $pid 2>/dev/null || true
wait $pid 2>/dev/null || true
for e in "${expected[@]}"; do
    [ -s "$e" ] || { echo "bake_materials: $e was never written; see:"; tail -20 "$work/mm.log"; exit 1; }
    python3 "$root/tools/shrink_png.py" "$e" "$e" "$size"
done

if [ $check -eq 1 ]; then
    bad=0
    for e in "${expected[@]}"; do
        b="$(basename "$e")"
        if [ ! -f "$out/$b" ]; then echo "$out/$b is missing; run the bake"; bad=1; continue; fi
        if ! python3 "$root/tools/pngdiff.py" "$out/$b" "$e" --max 0.5 > "$work/diff.txt"; then
            echo "$b has drifted from its graph: $(tail -1 "$work/diff.txt")"; bad=1
        fi
    done
    [ $bad -eq 0 ] && say "every map matches its graph" || exit 1
else
    mkdir -p "$out"
    for e in "${expected[@]}"; do cp "$e" "$out/"; done
    say "wrote $(( ${#expected[@]} )) maps to $out"
    ls -l "$out" | awk 'NR > 1 {printf "   %8d  %s\n", $5, $9}'
fi
