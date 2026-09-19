#!/usr/bin/env python3
"""Bake the asphalt set: albedo, normal, ORM and height, tileable.

    python3 tools/make_asphalt_texture.py           # write assets/textures/terrain
    python3 tools/make_asphalt_texture.py --check   # re-bake and compare, byte for byte

Every other set in this project is a Material Maker graph in `materials/`
and `tools/bake_materials.sh` is what turns one into PNGs. This one is a
SCRIPT, and the reason is written down rather than hidden: Material Maker
does not export in the container this was built in. Measured, on the seven
COMMITTED graphs with nothing changed: it starts, reports its Vulkan
device, and then sits for five minutes having written none of its thirty
two maps, under forward_plus, mobile and gl_compatibility alike. The
graph is not the problem and neither is the new set; the rig is. Rather
than ship a `.ptex` nobody can bake and a PNG nobody can check against
it, the generator IS the source here, which is the rule this project
actually keeps ("every asset has a committed source") and the same shape
swarm-demo's own `make_chitin_texture.py` has.

It is deterministic to the BIT on any machine: integer hashes, no `sin`,
no random module, no GPU. That is what makes `--check` mean something,
and it is the same rule the core's own field noise keeps so that two
clients agree.

The maps are 1024 and the street's tile is 3 m (`CONCRETE_TILE` in
`terrain.rs`), so one pixel is 2.9 mm of road and a chip of aggregate is
about five pixels across, which is what asphalt actually is: stone
chips in bitumen, the binder near black and the chips the only part of it
that is not.
"""
import sys
import pathlib
import numpy as np
from PIL import Image

SIZE = 1024
#: Cells across the map for each scale. A chip of aggregate is about
#: 15 mm on a 3 m tile, which is 200 of them across a 1024 map.
CHIPS = 200
GRIT = 96
#: How broadly the road is worn: where the tyres have polished it, where
#: the sun has bleached it, where the oil has run. Metre scale, not
#: millimetre, so it is a handful of cells over the tile.
WEAR = 3

#: What asphalt reflects. Fresh bitumen is about a twentieth and weathered
#: asphalt about a fifth; the chips in it are ordinary grey stone. These
#: are LINEAR, because that is what the albedo map is read as.
BINDER = 0.060
CHIP_LO = 0.085
CHIP_HI = 0.210
#: How far the broad wear moves the colour either way.
WEAR_LO = 0.80
WEAR_HI = 1.30

#: Matte, because asphalt is, and the one thing on a road that shines is
#: the wheel track the traffic has polished.
ROUGH_DRY = 0.96
ROUGH_POLISHED = 0.74
#: How proud a chip stands over the binder, as a share of the height map.
RELIEF = 0.55
#: How hard the normal map leans. SHALLOW, which is this project's rule
#: for every set: at full strength a normal map on flat ground reads as
#: gravel rather than as a road, and swarm-demo runs its finishes at a
#: fifth for the same reason. The first render of this set was at 0.30
#: with the chips spanning 0.05 to 0.27 of albedo, and the street came
#: back as coarse gravel rather than tarmac: a chip is 12 mm on a 3 m
#: tile, so at walking distance it is a few pixels and every bit of
#: contrast on it is noise.
NORMAL_SCALE = 0.20
#: How dark the pits between the chips go.
AO_DEPTH = 0.40

MAPS = ("albedo", "normal", "orm", "heightmap")


def hash2(ix, iy, k):
    """A uniform [0, 1) from two integer lattice coordinates and a lane.

    Integer arithmetic only, wrapped to `u32` by hand, so this is the same
    number on every machine and in every Python. `sin` based hashes are
    the thing this avoids: they differ between libms and between a CPU and
    a GPU, which is the core's own rule for its field noise.
    """
    # Every product is taken modulo 2^32 by hand before it becomes a
    # `uint32`, because numpy refuses a Python integer that does not fit
    # rather than wrapping it, and the wrap is the whole point of a hash.
    h = (ix.astype(np.uint32) * np.uint32(0x27D4EB2D)) ^ (
        iy.astype(np.uint32) * np.uint32(0x165667B1)
    ) ^ np.uint32((k * 0x9E3779B1) & 0xFFFFFFFF)
    h ^= h >> np.uint32(15)
    h = (h * np.uint32(0x85EBCA6B)).astype(np.uint32)
    h ^= h >> np.uint32(13)
    h = (h * np.uint32(0xC2B2AE35)).astype(np.uint32)
    h ^= h >> np.uint32(16)
    return h.astype(np.float64) / 4294967296.0


def lattice(cells):
    """The cell index and the fraction inside it, for a map of `SIZE`."""
    t = (np.arange(SIZE, dtype=np.float64) + 0.5) * cells / SIZE
    i = np.floor(t).astype(np.int64)
    return i, t - i


def value_noise(cells, seed):
    """Tileable value noise: a hashed lattice, smoothstepped between.

    Tileable because the lattice index is taken modulo `cells`, so the
    map's left edge and its right edge read the same corner.
    """
    i, f = lattice(cells)
    ix, iy = np.meshgrid(i, i, indexing="ij")
    fx, fy = np.meshgrid(f, f, indexing="ij")
    sx, sy = fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy)
    out = np.zeros((SIZE, SIZE))
    for dx in (0, 1):
        for dy in (0, 1):
            v = hash2((ix + dx) % cells, (iy + dy) % cells, seed)
            wx = sx if dx else 1.0 - sx
            wy = sy if dy else 1.0 - sy
            out += v * wx * wy
    return out


def fbm(cells, octaves, persistence, seed):
    """Octaves of `value_noise`, each twice as fine and worth less."""
    out = np.zeros((SIZE, SIZE))
    amp, total = 1.0, 0.0
    for k in range(octaves):
        out += value_noise(cells * (1 << k), seed + k) * amp
        total += amp
        amp *= persistence
    return out / total


def worley(cells, seed):
    """The AGGREGATE: distance to the nearest scattered point, and which.

    A jittered grid rather than a Poisson process, and toroidal, so the
    map tiles and every cell is one chip of stone. Returns the distance in
    cell widths and the chip's own hash, which is what lets one chip be a
    darker stone than the next.
    """
    i, f = lattice(cells)
    ix, iy = np.meshgrid(i, i, indexing="ij")
    fx, fy = np.meshgrid(f, f, indexing="ij")
    best = np.full((SIZE, SIZE), 9.0)
    who = np.zeros((SIZE, SIZE))
    for dx in (-1, 0, 1):
        for dy in (-1, 0, 1):
            cx, cy = (ix + dx) % cells, (iy + dy) % cells
            px = dx + hash2(cx, cy, seed)
            py = dy + hash2(cx, cy, seed + 101)
            d = (px - fx) ** 2 + (py - fy) ** 2
            nearer = d < best
            best = np.where(nearer, d, best)
            who = np.where(nearer, hash2(cx, cy, seed + 202), who)
    return np.sqrt(best), who


def normal_from(height):
    """A normal map off the height's own gradient, wrapped so it tiles.

    OpenGL green up, which is the layout Bevy reads and the one Material
    Maker's Godot target writes, so this set drops in beside the others
    with nothing to convert.
    """
    dx = (np.roll(height, -1, 0) - np.roll(height, 1, 0)) * (0.5 * SIZE * NORMAL_SCALE)
    dy = (np.roll(height, -1, 1) - np.roll(height, 1, 1)) * (0.5 * SIZE * NORMAL_SCALE)
    n = np.stack([-dx, -dy, np.ones_like(height)], axis=-1)
    return n / np.linalg.norm(n, axis=-1, keepdims=True)


def blur(a, radius):
    """A box blur, wrapped, which is what the occlusion is measured against."""
    out = np.zeros_like(a)
    for d in range(-radius, radius + 1):
        out += np.roll(np.roll(a, d, 0), d, 1)
    return out / (2 * radius + 1)


def bake():
    """Every map of the set, as float arrays in nought to one."""
    # The chips, standing proud of the binder between them. `1 - d` is
    # domed at a chip's middle and nought at the border with the next,
    # which is a stone with mortar round it rather than a flat facet.
    d, stone = worley(CHIPS, 11)
    dome = np.clip(1.0 - d * 1.6, 0.0, 1.0) ** 0.6
    grit = fbm(GRIT, 3, 0.5, 31)
    height = np.clip(dome * RELIEF + grit * (1.0 - RELIEF), 0.0, 1.0)

    wear = fbm(WEAR, 4, 0.6, 51)
    wash = WEAR_LO + wear * (WEAR_HI - WEAR_LO)
    # A chip's own stone, the binder where there is no chip, and the broad
    # wear over both. Every chip is a different grey, because a road is
    # made of whatever was quarried nearby.
    chip = CHIP_LO + stone * (CHIP_HI - CHIP_LO)
    albedo = (BINDER + (chip - BINDER) * dome) * wash
    albedo = np.clip(albedo, 0.0, 1.0)

    normal = normal_from(height)
    # Occlusion: how far a point stands under the ground round it, which
    # is the gap between the chips and nowhere else.
    ao = np.clip(1.0 - (blur(height, 3) - height) * AO_DEPTH * 6.0, 0.0, 1.0)
    rough = ROUGH_DRY + wear * (ROUGH_POLISHED - ROUGH_DRY)
    orm = np.stack([ao, rough, np.zeros_like(ao)], axis=-1)
    return {
        "albedo": np.repeat(albedo[..., None], 3, axis=-1),
        "normal": normal * 0.5 + 0.5,
        "orm": orm,
        "heightmap": np.repeat(height[..., None], 3, axis=-1),
    }


def png(a):
    """A float array as eight bit RGB, rounded the one way everywhere."""
    return Image.fromarray(np.clip(a * 255.0 + 0.5, 0, 255).astype(np.uint8), "RGB")


def main():
    check = "--check" in sys.argv[1:]
    out = pathlib.Path(__file__).resolve().parent.parent / "assets/textures/terrain"
    maps = bake()
    bad = 0
    for name in MAPS:
        path = out / f"asphalt_{name}.png"
        img = png(maps[name])
        if not check:
            img.save(path, optimize=True)
            print(f"   {path.stat().st_size:8d}  {path.name}")
            continue
        if not path.exists():
            print(f"{path.name} is missing; run the generator")
            bad = 1
            continue
        have = np.asarray(Image.open(path).convert("RGB"))
        want = np.asarray(img)
        moved = int((np.abs(have.astype(int) - want.astype(int)) > 0).sum())
        if moved:
            print(f"{path.name} has drifted from the generator: {moved} bytes")
            bad = 1
    if check and not bad:
        print("== every asphalt map matches the generator")
    return bad


if __name__ == "__main__":
    sys.exit(main())
