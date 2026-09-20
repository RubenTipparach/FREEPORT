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

import numpy as np

from texkit import fbm, grey, normal_from, occlusion, soften, worley, write_or_check

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
#:
#: 0.20 was still far too much and it took a NIGHT to see it. Measured
#: as the mean angle the normal leans off the surface, asphalt was 86.3
#: degrees against concrete's 21.1 and hull plate's 13.8: a chip's dome
#: is a sharp worley cone, so nearly every texel on the map stood on
#: end. Under the sun that reads as coarse gravel and is arguable;
#: under a STREET LAMP a few metres up it is a field of white specular
#: glints, and the first picture of the port at midnight was a road
#: covered in them. It is the mockups' own lesson about a lamp a hand
#: under a ceiling lighting every grain of a normal map at a grazing
#: angle, arriving outdoors. Swept against the mean lean with the height
#: low passed over `SMOOTH` first: 0.08 is 60 degrees, 0.05 is 49, 0.03
#: is 36 and 0.02 is 27, which is concrete's own 21 and hull plate's 14.
NORMAL_SCALE = 0.02
#: How far the height is low passed before the normal is read off it,
#: pixels. A chip is five pixels across, so its own edge is a cliff one
#: pixel wide: the map keeps it and the normal does not.
SMOOTH = 3
#: How dark the pits between the chips go.
AO_DEPTH = 0.40


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

    normal = normal_from(soften(height, SMOOTH), NORMAL_SCALE)
    # Occlusion: how far a point stands under the ground round it, which
    # is the gap between the chips and nowhere else.
    ao = occlusion(height, AO_DEPTH)
    rough = ROUGH_DRY + wear * (ROUGH_POLISHED - ROUGH_DRY)
    orm = np.stack([ao, rough, np.zeros_like(ao)], axis=-1)
    return {
        "albedo": grey(albedo),
        "normal": normal * 0.5 + 0.5,
        "orm": orm,
        "heightmap": grey(height),
    }


def main():
    """Write the set, or hold the committed PNGs to what it bakes."""
    return write_or_check({"asphalt": bake()}, "--check" in sys.argv[1:])


if __name__ == "__main__":
    sys.exit(main())
