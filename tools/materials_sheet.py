#!/usr/bin/env python3
"""The design page's sheet of the baked sets: a row a set, a column a map.

Reads assets/textures/terrain/<set>_{albedo,normal,orm,heightmap}.png and
writes docs/img/materials.jpg, so the sheet is regenerated whenever a graph
is rebaked and never falls behind the maps it shows.
"""
import sys
from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
SETS = ["basalt", "regolith", "ice", "dunes", "hull_plate", "grass", "concrete"]
MAPS = ["albedo", "normal", "orm", "heightmap"]
CELL = 256
LABEL = 22


def main():
    out = ROOT / "docs" / "img" / "materials.jpg"
    if len(sys.argv) > 1:
        out = Path(sys.argv[1])
    src = ROOT / "assets" / "textures" / "terrain"
    sheet = Image.new("RGB", (len(MAPS) * CELL, len(SETS) * (CELL + LABEL)), (18, 29, 41))
    draw = ImageDraw.Draw(sheet)
    for row, name in enumerate(SETS):
        y = row * (CELL + LABEL)
        draw.text((6, y + 4), name.replace("_", " "), fill=(227, 234, 240))
        for col, kind in enumerate(MAPS):
            path = src / f"{name}_{kind}.png"
            if not path.exists():
                print(f"missing {path}", file=sys.stderr)
                continue
            tile = Image.open(path).convert("RGB").resize((CELL, CELL), Image.LANCZOS)
            sheet.paste(tile, (col * CELL, y + LABEL))
    out.parent.mkdir(parents=True, exist_ok=True)
    sheet.save(out, quality=82)
    print(f"wrote {out}: {sheet.size[0]}x{sheet.size[1]}")


if __name__ == "__main__":
    main()
