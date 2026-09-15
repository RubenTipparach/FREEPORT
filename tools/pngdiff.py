#!/usr/bin/env python3
"""How different two screenshots are, in pixels, with nothing but the standard
library, because the container this runs in has no image library and a
refactor is judged by its pictures.

    python3 tools/pngdiff.py before.png after.png            # the numbers
    python3 tools/pngdiff.py before.png after.png --max 0.5  # exit 1 over half a percent
    python3 tools/pngdiff.py before.png after.png --grid     # and WHERE, on a 4 by 4 map

Two fixed step renders of the same scene are NOT byte identical: the spark
ring is claimed with atomics on the GPU and a software rasteriser's thread
order differs run to run, so a handful of pixels move every time (measured:
19 of 1024000 on the order picture). The check is therefore a SHARE of pixels
that differ by more than a threshold, held against that noise floor, and not
`cmp`. A refactor that moves code and changes no behaviour stays under the
floor; one that changed a colour, a position or a count does not.

Only the PNG shapes Bevy's screenshot and Material Maker's export write are handled: 8 bit RGB or RGBA,
no interlace, one IDAT stream. Anything else is refused by name.
"""
import struct
import sys
import zlib


def read_png(path):
    data = open(path, "rb").read()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise SystemExit(f"{path}: not a PNG")
    pos = 8
    width = height = None
    idat = b""
    colour = bitdepth = None
    while pos < len(data):
        length, kind = struct.unpack(">I4s", data[pos:pos + 8])
        body = data[pos + 8:pos + 8 + length]
        pos += 12 + length
        if kind == b"IHDR":
            width, height, bitdepth, colour, _, _, interlace = struct.unpack(">IIBBBBB", body)
            if bitdepth != 8 or colour not in (2, 6) or interlace != 0:
                raise SystemExit(f"{path}: only 8 bit RGB or RGBA without interlace is handled (depth {bitdepth}, colour {colour}, interlace {interlace})")
        elif kind == b"IDAT":
            idat += body
        elif kind == b"IEND":
            break
    channels = 3 if colour == 2 else 4
    raw = zlib.decompress(idat)
    stride = width * channels
    rows = []
    prev = bytearray(stride)
    at = 0
    for _ in range(height):
        filt = raw[at]
        line = bytearray(raw[at + 1:at + 1 + stride])
        at += 1 + stride
        if filt == 1:
            for i in range(channels, stride):
                line[i] = (line[i] + line[i - channels]) & 255
        elif filt == 2:
            for i in range(stride):
                line[i] = (line[i] + prev[i]) & 255
        elif filt == 3:
            for i in range(stride):
                left = line[i - channels] if i >= channels else 0
                line[i] = (line[i] + ((left + prev[i]) >> 1)) & 255
        elif filt == 4:
            for i in range(stride):
                a = line[i - channels] if i >= channels else 0
                b = prev[i]
                c = prev[i - channels] if i >= channels else 0
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pred = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
                line[i] = (line[i] + pred) & 255
        elif filt != 0:
            raise SystemExit(f"{path}: unknown filter {filt}")
        rows.append(bytes(line))
        prev = line
    return width, height, channels, rows


def main():
    # A flag's own value is not a file name: `--max 0.5` left "0.5" in the
    # positional list and every call with a limit exited on the usage text,
    # which read as a drift the bake check could never clear.
    argv = sys.argv[1:]
    args = []
    limit = None
    i = 0
    while i < len(argv):
        a = argv[i]
        if a == "--max" and i + 1 < len(argv):
            limit = float(argv[i + 1])
            i += 2
            continue
        if not a.startswith("--"):
            args.append(a)
        i += 1
    if len(args) != 2:
        raise SystemExit(__doc__)
    step = 8
    a = read_png(args[0])
    b = read_png(args[1])
    if a[:3] != b[:3]:
        raise SystemExit(f"shapes differ: {a[:3]} against {b[:3]}")
    width, height, channels, _ = a
    differ = 0
    worst = 0
    cells = [[0] * 4 for _ in range(4)]
    for y, (ra, rb) in enumerate(zip(a[3], b[3])):
        row = cells[y * 4 // height]
        for x in range(0, width * channels, channels):
            d = max(abs(ra[x + c] - rb[x + c]) for c in range(3))
            if d > step:
                differ += 1
                row[x // channels * 4 // width] += 1
            if d > worst:
                worst = d
    total = width * height
    share = 100.0 * differ / total
    print(f"{differ} of {total} pixels differ by more than {step} of 255 ({share:.3f}%), worst {worst}")
    if "--grid" in sys.argv:
        # Where the moved pixels are: a swarm that diverged run to run is
        # spread over the cloud, a HUD that changed is one cell.
        for row in cells:
            print("  " + " ".join(f"{n:6d}" for n in row))
    if limit is not None and share > limit:
        print(f"pngdiff: FAILED, over {limit}%")
        sys.exit(1)


if __name__ == "__main__":
    main()
