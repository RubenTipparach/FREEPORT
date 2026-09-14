#!/usr/bin/env bash
# Fetch Material Maker (the Linux build) into tools/material_maker, which is
# gitignored, so `tools/bake_materials.sh` has something to run.
#
#   tools/get_material_maker.sh            # 1.7, from itch.io
#
# GitHub's release page is the canonical home and is the first place to look
# by hand (https://github.com/RodZill4/material-maker/releases). This script
# goes to itch.io instead because the sandboxes this project is built in
# cannot reach github.com downloads, and itch's own download flow is a page
# fetch, a POST for a download key, and a POST for the file URL. If itch
# changes that flow the script says so and the fix is to download by hand and
# unpack into tools/material_maker.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dest="$root/tools/material_maker"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
ua="Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/128 Safari/537.36"
page="https://rodzilla.itch.io/material-maker"
upload=18327586   # material_maker_1_7_linux.tar.gz

say() { printf '\033[36m==\033[0m %s\n' "$*"; }
say "asking itch.io for a download key"
curl -sS -L -c "$work/cj" -b "$work/cj" -A "$ua" "$page" -o "$work/page.html"
csrf=$(grep -o 'csrf_token" value="[^"]*"' "$work/page.html" | head -1 | sed 's/.*value="//;s/"$//')
curl -sS -L -c "$work/cj" -b "$work/cj" -A "$ua" -X POST --data-urlencode "csrf_token=$csrf" "$page/download_url" -o "$work/dl.json"
dlurl=$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['url'])" "$work/dl.json")
curl -sS -L -c "$work/cj" -b "$work/cj" -A "$ua" "$dlurl" -o "$work/dlpage.html"
csrf2=$(grep -o 'csrf_token" value="[^"]*"' "$work/dlpage.html" | head -1 | sed 's/.*value="//;s/"$//')
curl -sS -L -c "$work/cj" -b "$work/cj" -A "$ua" -X POST --data-urlencode "csrf_token=${csrf2:-$csrf}" "$page/file/$upload?source=game_download" -o "$work/file.json"
url=$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['url'])" "$work/file.json") || {
    echo "get_material_maker: itch.io did not hand back a file URL; download material_maker_1_7_linux.tar.gz by hand from $page and unpack it into $dest" >&2
    exit 1
}
say "downloading (about 94 MB)"
curl -sS -L "$url" -o "$work/mm.tar.gz"
mkdir -p "$dest"
tar xzf "$work/mm.tar.gz" -C "$work"
rm -rf "$dest"
mv "$work"/material_maker_*_linux "$dest"
say "Material Maker is at $dest/material_maker.x86_64"
