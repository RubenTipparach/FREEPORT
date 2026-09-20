#!/usr/bin/env python3
"""The bits every baked texture set here is made of: hashes, noise, maps.

Material Maker is the source for a material in this project and
`tools/bake_materials.sh` is what turns a `.ptex` into PNGs. Two sets are
SCRIPTS instead, and the reason is written down rather than hidden in
`make_asphalt_texture.py`: Material Maker does not export in the container
this was built in. Rather than write the same hash, the same tileable
noise and the same normal-off-a-height twice in two generators, they are
here, which is the same divergent path rule the Rust side keeps and the
same shape swarm-demo's own `tools/texkit` has.

Everything is deterministic to the BIT on any machine: integer hashes, no
`sin`, no random module, no GPU. That is what makes a generator's
`--check` mean anything, and it is the core's own rule for its field noise
so that two clients agree.
"""
import pathlib

import numpy as np
from PIL import Image

#: Pixels a side. Every set is this, because the layers of one array
#: texture have to agree and `terrain.rs` stacks them all into three.
SIZE = 1024

#: The maps a set ships, in the layout Bevy reads and Material Maker's
#: Godot target writes: albedo, normal in OpenGL green up, ORM with
#: occlusion in red, roughness in green and metallic in blue, and the
#: height the normal came off.
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
    h = (
        (ix.astype(np.uint32) * np.uint32(0x27D4EB2D))
        ^ (iy.astype(np.uint32) * np.uint32(0x165667B1))
        ^ np.uint32((k * 0x9E3779B1) & 0xFFFFFFFF)
    )
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


def stretched(across, along, octaves, persistence, seed):
    """`fbm` with its two axes on different scales, for a GRAIN.

    Wood and marble are the same noise pulled a long way along one axis:
    a plank's figure and a vein of calcite both run one way and are fine
    across it. Written as one function because two sets need it.
    """
    i_a, f_a = lattice(across)
    i_b, f_b = lattice(along)
    out = np.zeros((SIZE, SIZE))
    amp, total = 1.0, 0.0
    for k in range(octaves):
        step = 1 << k
        ix, iy = np.meshgrid(i_a * step, i_b * step, indexing="ij")
        fx, fy = np.meshgrid(f_a, f_b, indexing="ij")
        sx, sy = fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy)
        layer = np.zeros((SIZE, SIZE))
        for dx in (0, 1):
            for dy in (0, 1):
                v = hash2(
                    (ix + dx) % (across * step),
                    (iy + dy) % (along * step),
                    seed + k,
                )
                wx = sx if dx else 1.0 - sx
                wy = sy if dy else 1.0 - sy
                layer += v * wx * wy
        out += layer * amp
        total += amp
        amp *= persistence
    return out / total


def worley(cells, seed):
    """Distance to the nearest scattered point, and WHICH point it is.

    A jittered grid rather than a Poisson process, and toroidal, so the
    map tiles and every cell is one feature: a chip of road aggregate, a
    pit in a stone face. Returns the distance in cell widths and the
    feature's own hash, which is what lets one chip be a darker stone
    than the next.
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


def normal_from(height, scale):
    """A normal map off a height's own gradient, wrapped so it tiles.

    OpenGL green up, which is the layout Bevy reads and the one Material
    Maker's Godot target writes, so a generated set drops in beside the
    others with nothing to convert.

    The scale is SHALLOW on every set here, which is this project's rule:
    at full strength a normal map on a flat surface reads as gravel rather
    than as the thing it is, and swarm-demo runs its finishes at a fifth.
    """
    dx = (np.roll(height, -1, 0) - np.roll(height, 1, 0)) * (0.5 * SIZE * scale)
    dy = (np.roll(height, -1, 1) - np.roll(height, 1, 1)) * (0.5 * SIZE * scale)
    n = np.stack([-dx, -dy, np.ones_like(height)], axis=-1)
    return n / np.linalg.norm(n, axis=-1, keepdims=True)


def blur(a, radius):
    """A box blur, wrapped, which is what occlusion is measured against."""
    out = np.zeros_like(a)
    for d in range(-radius, radius + 1):
        out += np.roll(np.roll(a, d, 0), d, 1)
    return out / (2 * radius + 1)


def soften(a, radius):
    """A separable box blur, wrapped: a real low pass, unlike `blur`.

    What it is FOR is the NORMAL. A map at 1024 over a three metre tile
    is 2.9 mm a pixel, and a feature a few pixels across (a chip of road
    aggregate, a brick's own grain) has a gradient of a tenth of the
    height map per pixel, which `normal_from` turns into a normal
    standing on end. Measured as the mean angle a normal leans off its
    surface: asphalt was 86 degrees against concrete's 21, so nearly
    every texel of a road was a cliff. Under the sun that reads as
    coarse gravel; under a STREET LAMP it is a field of white specular
    glints, which is what the first picture of the port at midnight came
    back as. Low passing the height first keeps the map's own detail and
    takes the cliff out of the normal it is read through.
    """
    out = a
    for axis in (0, 1):
        acc = np.zeros_like(out)
        for d in range(-radius, radius + 1):
            acc += np.roll(out, d, axis)
        out = acc / (2 * radius + 1)
    return out


def occlusion(height, depth, radius=3):
    """How far a point stands UNDER the ground round it: a mortar joint,
    the gap between two chips, the groove between two planks."""
    return np.clip(1.0 - (blur(height, radius) - height) * depth * 6.0, 0.0, 1.0)


def grey(a):
    """One channel as three, for a map whose colour is its value."""
    return np.repeat(a[..., None], 3, axis=-1)


def png(a):
    """A float array as eight bit RGB, rounded the one way everywhere."""
    return Image.fromarray(np.clip(a * 255.0 + 0.5, 0, 255).astype(np.uint8), "RGB")


def terrain_dir():
    """Where a baked set lives, beside the ones Material Maker writes."""
    return pathlib.Path(__file__).resolve().parent.parent / "assets/textures/terrain"


def write_or_check(sets, check):
    """Write every map of every set, or hold the committed PNGs to them.

    BYTE for byte, unlike `bake_materials.sh --check`'s half a per cent:
    a GPU render is not bit exact across drivers and this is arithmetic,
    so anything but equality here is a generator that has drifted from
    what is committed. Returns nought when everything agrees.
    """
    out, bad = terrain_dir(), 0
    for name, maps in sets.items():
        for kind in MAPS:
            path = out / f"{name}_{kind}.png"
            img = png(maps[kind])
            if not check:
                img.save(path, optimize=True)
                print(f"   {path.stat().st_size:8d}  {path.name}")
                continue
            if not path.exists():
                print(f"{path.name} is missing; run the generator")
                bad = 1
                continue
            have = np.asarray(Image.open(path).convert("RGB")).astype(int)
            moved = int((np.abs(have - np.asarray(img).astype(int)) > 0).sum())
            if moved:
                print(f"{path.name} has drifted from the generator: {moved} bytes")
                bad = 1
    if check and not bad:
        print(f"== every map of {', '.join(sets)} matches the generator")
    return bad
