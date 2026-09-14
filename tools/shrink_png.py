#!/usr/bin/env python3
"""Box filter a PNG down to a size, in place or to another file.

    python3 tools/shrink_png.py in.png out.png 1024

Material Maker's command line export writes every map at 2048 whatever the
graph's material node says (measured: a graph at size 10 and one at 11 both
came out 2048), and sixty megabytes of terrain maps is not a thing to commit
for five materials. A box filter is the one resample that is exactly the same
on every machine, which is what lets `bake_materials.sh --check` compare.
"""
import sys
from PIL import Image

src, dst, size = sys.argv[1], sys.argv[2], int(sys.argv[3])
img = Image.open(src)
if img.width != size or img.height != size:
    img = img.resize((size, size), Image.BOX)
img.save(dst, optimize=True)
