#!/usr/bin/env python3
"""Bake the six sets a BUILDING is made of: wood, brick, vinyl, marble, stone, glass.

    python3 tools/make_building_textures.py           # write assets/textures/terrain
    python3 tools/make_building_textures.py --check   # re-bake and compare, byte for byte

A town used to be one grey: every house and every office wore the concrete
set, so a suburb and a downtown were the same wall at two heights. The
owner's brief is the trades' own list, a house out of wood, red brick or
vinyl and an office out of red brick, concrete, marble, glass or stone
blocks, and `freeport_core::field` carries one material byte a triangle,
so what is missing is the TEXTURE at the other end of that byte.

These are SCRIPTS and not Material Maker graphs, for the reason
`make_asphalt_texture.py` writes down at length: Material Maker does not
export in the container this was built in, measured on the seven committed
graphs with nothing changed. A generator nobody can run is worse than a
generator in the wrong language, and this one is deterministic to the bit
(integer hashes out of `texkit`, no `sin`, no random module, no GPU), so
its `--check` means something that `bake_materials.sh --check`'s half a
per cent cannot.

EVERY NUMBER IS IN METRES FIRST. A built thing's tile is `CONCRETE_TILE`
in `terrain.rs`, three metres, and the map is 1024, so a pixel is 2.93 mm
of wall. A brick is 215 by 65 mm with a 10 mm joint, a clapboard's reveal
is 200 mm, an ashlar block is 500 by 300: those are the sizes, and the
cell counts below are what they come to on a three metre tile. A texture
authored by eye instead comes out at whatever scale it came out at, and
a wall of it reads as a doll's house or as a cliff.
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import numpy as np

from texkit import (
    SIZE,
    fbm,
    grey,
    hash2,
    normal_from,
    occlusion,
    soften,
    stretched,
    worley,
    write_or_check,
)

#: The tile a built thing is mapped at, metres: `terrain::CONCRETE_TILE`.
#: Every count below is a real size divided by this, which is what keeps a
#: brick a brick when the tile moves.
TILE = 3.0


#: The finest LATTICE any octave here is allowed, cells across the map.
#: A 1024 map at four pixels a cell: past that an octave is noise under
#: the sampler rather than detail, the mip chain throws all of it away,
#: and what is left in the normal map is the speckle this project keeps
#: turning down. The first cut of these ran octaves out to 1,760 cells,
#: which is under two pixels each, and the stone came back as gravel.
FINEST = 256


def units(metres):
    """How many of something `metres` across fit the tile, rounded to a
    whole number so the map still TILES: a course that did not divide the
    tile would show its own seam down every wall."""
    return max(1, int(round(TILE / metres)))


def step(edge0, edge1, x):
    """`smoothstep`, numpy's missing one."""
    t = np.clip((x - edge0) / (edge1 - edge0), 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


def cell(rows, cols):
    """Where a pixel is in a grid of `rows` up by `cols` across, as two
    arrays in unit widths. Axis nought is UP a wall and axis one is
    along it, which is what makes a course horizontal."""
    v = (np.arange(SIZE, dtype=np.float64) + 0.5) * rows / SIZE
    u = (np.arange(SIZE, dtype=np.float64) + 0.5) * cols / SIZE
    return np.meshgrid(v, u, indexing="ij")


def bond(rows, cols, seed):
    """A RUNNING BOND: courses of units, every other course shifted half a
    unit, which is how a brick or a stone wall is actually laid and the
    reason a real one has no continuous vertical joint in it.

    Returns where the pixel stands inside its own unit (nought to one on
    each axis) and the unit's own hash, which is what lets one brick be
    fired darker than the next. `rows` is EVEN so the stagger repeats
    within the tile.
    """
    v, u = cell(rows, cols)
    row = np.floor(v).astype(np.int64)
    shifted = u + np.where(row % 2 == 1, 0.5, 0.0)
    col = np.floor(shifted).astype(np.int64)
    who = hash2(col % cols, row % rows, seed)
    return shifted - col, v - row, who


def joint(fu, fv, wide_u, wide_v):
    """One in the FACE of a unit and nought in the joint round it, the
    joint being `wide_u` and `wide_v` of a unit on each axis."""
    across = step(0.0, wide_u, np.minimum(fu, 1.0 - fu))
    along = step(0.0, wide_v, np.minimum(fv, 1.0 - fv))
    return np.minimum(across, along)


def finish(height, albedo, rough, normal_scale, ao_depth, smooth=2, metal=0.0):
    """The three maps a set ships, off a height, a colour and a gloss.

    One function because every set here ends the same way and this is the
    project's own divergent path rule: five copies of a normal, an
    occlusion and an ORM pack is five places for a channel to end up in
    the wrong lane.

    The normal is read off a LOW PASSED height (`smooth` pixels), and the
    map itself keeps every bit of its detail. A feature a few pixels
    across has a gradient of a tenth of the map per pixel, which is a
    normal standing on end, and what that looks like is a wall of white
    specular glints under a street lamp. The number to judge it by is the
    mean angle the normal leans off its surface: concrete is 21 degrees
    and hull plate 14, and every set here is measured against those.
    """
    return {
        "albedo": np.clip(albedo, 0.0, 1.0),
        "normal": normal_from(soften(height, smooth), normal_scale) * 0.5 + 0.5,
        "orm": np.stack(
            [
                occlusion(height, ao_depth),
                np.clip(rough, 0.04, 1.0),
                np.full_like(height, metal),
            ],
            axis=-1,
        ),
        "heightmap": grey(np.clip(height, 0.0, 1.0)),
    }


def tint(value, colour):
    """A one channel map wearing a linear rgb colour."""
    return value[..., None] * np.asarray(colour, dtype=np.float64)


# ---------------------------------------------------------------- wood

#: A board 150 mm wide with a 6 mm gap between it and the next, which is
#: ordinary board and batten siding. LINEAR albedo: the first render of
#: this came back as CHARRED timber, a black wall with white speckle,
#: because 0.075 to 0.165 is what a creosoted fence is and this camera's
#: own exposure then took what was left of it. Stained cedar is a fifth
#: to a third in red and about half that in green, which is sRGB 0.44 to
#: 0.58: a warm brown wall rather than a burnt one.
BOARD_WIDE = 0.15
BOARD_GAP = 0.006
WOOD_DARK = (0.160, 0.094, 0.050)
WOOD_LIGHT = (0.300, 0.196, 0.106)


def wood():
    """Vertical boards with the grain running up them, and knots.

    The grain is `stretched` noise pulled fifty to one along the board,
    which is what a plank's figure IS: growth rings cut lengthwise. A
    knot is a `worley` cell, dark and with the grain swept round it,
    sparse enough that a wall has a few rather than a rash.
    """
    boards = units(BOARD_WIDE)
    v, u = cell(1, boards)
    fu = u - np.floor(u)
    board = hash2(np.floor(u).astype(np.int64) % boards, np.zeros_like(fu, np.int64), 7)
    # The gap between two boards, as a share of a board's own width.
    gap = BOARD_GAP / BOARD_WIDE
    face = step(0.0, gap, np.minimum(fu, 1.0 - fu))

    grain = stretched(3, 32, 4, 0.55, 13)
    fine = stretched(6, 64, 2, 0.5, 29)
    knot_d, _ = worley(14, 41)
    # A knot is 60 mm across on a three metre tile, which is a dozen
    # cells with a tight dome inside each. The first cut had five cells
    # across the whole tile and a loose dome, so every knot was 600 mm
    # and the wall came back blotched like scorched plywood.
    knot = np.clip(1.0 - knot_d * 3.4, 0.0, 1.0) ** 2.0

    figure = np.clip(grain * 0.72 + fine * 0.28, 0.0, 1.0)
    # Each board is cut from its own tree, so it is its own shade.
    shade = np.clip(figure * 0.78 + board * 0.22, 0.0, 1.0)
    colour = tint(shade, np.subtract(WOOD_LIGHT, WOOD_DARK)) + np.asarray(WOOD_DARK)
    colour *= (1.0 - knot * 0.42)[..., None]
    colour *= (0.35 + 0.65 * face)[..., None]

    height = np.clip(face * (0.70 + 0.20 * figure) - knot * 0.22, 0.0, 1.0)
    rough = 0.62 + (1.0 - figure) * 0.16 + knot * 0.1
    return finish(height, colour, rough, 0.045, 0.45)


# --------------------------------------------------------------- brick

#: A standard brick: 215 by 65 mm with a 10 mm joint, so a course is
#: 75 mm and a stretcher course repeats every 225. Linear albedo: the
#: first render of this came back BLACK rather than red, because a mean
#: of 0.083 over three channels is what a wall in shadow is and this
#: camera's exposure took the rest. A fired red brick is a fifth to a
#: third in red with about a third of that in green, which reads as red
#: brick at noon and still as brick at dusk; lime mortar is a light
#: grey. It took two lifts: at a mean of 0.105 against concrete's 0.449
#: a brick block beside a concrete one still read as a black slab, which
#: is four times the difference real brick and real concrete have.
BRICK_LONG = 0.225
BRICK_COURSE = 0.075
BRICK_JOINT = 0.010
BRICK_DARK = (0.115, 0.042, 0.032)
BRICK_LIGHT = (0.290, 0.112, 0.082)
MORTAR = (0.230, 0.220, 0.205)


def brick():
    """Red brick in a running bond, each brick fired its own shade."""
    cols = units(BRICK_LONG)
    rows = units(BRICK_COURSE)
    rows += rows % 2  # even, so the stagger repeats inside the tile
    fu, fv, who = bond(rows, cols, 3)
    face = joint(fu, fv, BRICK_JOINT / BRICK_LONG, BRICK_JOINT / BRICK_COURSE)

    rash = fbm(64, 3, 0.5, 71)
    sand = fbm(128, 2, 0.5, 97)
    fired = np.clip(who * 0.7 + rash * 0.3, 0.0, 1.0)
    body = tint(fired, np.subtract(BRICK_LIGHT, BRICK_DARK)) + np.asarray(BRICK_DARK)
    body *= (0.86 + 0.28 * sand)[..., None]
    colour = body * face[..., None] + np.asarray(MORTAR) * (1.0 - face)[..., None]

    # The joint is RAKED: the mortar stands back from the brick's face,
    # which is what gives a brick wall its shadow line.
    height = face * (0.86 + 0.14 * sand) + (1.0 - face) * 0.12
    rough = 0.86 + (1.0 - face) * 0.10 + sand * 0.04
    # 0.022 is 31 degrees of mean lean, against concrete's 21; at the
    # 0.10 the first cut carried it was 55, which is basalt's, and a
    # brick wall under a street lamp came back as a field of glints.
    return finish(height, colour, rough, 0.022, 0.65)


# --------------------------------------------------------------- vinyl

#: Lap siding with a 200 mm reveal: the visible face of one course. It is
#: PLASTIC, so it is pale, nearly flat, and glossier than anything else
#: on a house.
LAP_REVEAL = 0.20
VINYL = (0.300, 0.298, 0.262)


def vinyl():
    """Horizontal lap siding: a run of shallow steps, each course
    overlapping the one below, and almost no texture on the face.

    What says vinyl rather than painted board is the LACK of grain and
    the regular step: the height is a saw tooth with a rolled lip at the
    bottom of each course, and everything else on it is the faintest
    mottle so a whole wall is not one flat value.
    """
    rows = units(LAP_REVEAL)
    v, _ = cell(rows, 1)
    fv = v - np.floor(v)
    # A course stands proudest at its lower lip and falls back to the top
    # of the one above: the lip is the bottom eighth of the reveal.
    lip = step(0.0, 0.10, fv) * (1.0 - step(0.86, 1.0, fv))
    # The course's own swell, falling from its lip to the top of it. A
    # SMOOTHSTEP and not a cosine, which is this file's determinism rule
    # rather than a taste: `--check` is byte for byte and a libm's cosine
    # is not promised to the last bit on another machine, which is the
    # same reason the core's field noise has no trigonometry in it.
    swell = 0.85 - 0.60 * step(0.0, 1.0, np.clip((fv - 0.1) / 0.8, 0.0, 1.0))
    height = np.clip(lip * swell, 0.0, 1.0)

    mottle = fbm(9, 3, 0.5, 131)
    colour = np.asarray(VINYL) * (0.94 + 0.12 * mottle)[..., None]
    # Extruded plastic is semi gloss and the same gloss everywhere; what
    # little variation there is comes off the weathering, not the surface.
    rough = 0.40 + mottle * 0.06 + (1.0 - lip) * 0.05
    # The shallowest normal of any set here but marble's, measured
    # rather than chosen: at 0.06 the lap step leaned a mean of 42
    # degrees, which is basalt's own roughness on a sheet of extruded
    # plastic. Concrete is 21 and hull plate 14, and vinyl belongs with
    # those and not with the quarried sets.
    return finish(height, colour, rough, 0.018, 0.30)


# -------------------------------------------------------------- marble

#: Polished white marble: bright, cold and almost mirror smooth. The
#: veins are calcite and iron, so they run darker and a little warmer.
MARBLE_BODY = (0.620, 0.612, 0.585)
MARBLE_VEIN = (0.130, 0.125, 0.140)


def marble():
    """Veined and POLISHED: a bright body, thin dark veins running one
    way, and a roughness low enough that a lobby reflects the street.

    A vein is where a turbulent field crosses nought, which is why
    `abs(noise - 0.5)` is the whole of it: that is a ridge, and a ridge
    of a warped fbm is a vein. It is warped by a second noise so the
    veins wander rather than running parallel.
    """
    warp = fbm(5, 3, 0.55, 211) - 0.5
    body = stretched(4, 26, 4, 0.58, 223)
    turb = np.abs(body + warp * 0.35 - 0.5)
    vein = 1.0 - step(0.0, 0.055, turb)
    hair = 1.0 - step(0.0, 0.018, np.abs(stretched(7, 32, 3, 0.5, 251) - 0.5))
    cloud = fbm(6, 4, 0.5, 277)

    ink = np.clip(vein + hair * 0.55, 0.0, 1.0)
    colour = np.asarray(MARBLE_BODY) * (0.92 + 0.14 * cloud)[..., None]
    colour = colour * (1.0 - ink)[..., None] + np.asarray(MARBLE_VEIN) * ink[..., None]

    # Polished stone is FLAT: the height is here so the normal is not
    # exactly one everywhere, and it is worn to almost nothing.
    height = np.clip(0.5 + cloud * 0.5 - ink * 0.35, 0.0, 1.0)
    rough = 0.11 + ink * 0.16 + cloud * 0.03
    return finish(height, colour, rough, 0.008, 0.12)


# --------------------------------------------------------------- stone

#: Ashlar: a dressed block 500 by 300 mm with a 12 mm joint. Limestone,
#: so a light warm grey with a lot of block to block variation, because
#: a course is whatever came out of the quarry that week.
ASHLAR_LONG = 0.50
ASHLAR_COURSE = 0.30
ASHLAR_JOINT = 0.012
STONE_DARK = (0.105, 0.098, 0.086)
STONE_LIGHT = (0.290, 0.278, 0.252)


def stone():
    """Ashlar blocks in a running bond, pitted and weathered.

    The FACE is what makes it stone rather than brick: a worley field of
    pits with a broad fbm under it, so the surface is uneven at the size
    of a hand, and the joint is deep enough to carry a real shadow.
    """
    cols = units(ASHLAR_LONG)
    rows = units(ASHLAR_COURSE)
    rows += rows % 2
    fu, fv, who = bond(rows, cols, 5)
    face = joint(fu, fv, ASHLAR_JOINT / ASHLAR_LONG, ASHLAR_JOINT / ASHLAR_COURSE)

    pit_d, _ = worley(64, 61)
    pits = np.clip(1.0 - pit_d * 1.8, 0.0, 1.0) ** 1.4
    coarse = fbm(16, 4, 0.55, 83)
    weather = fbm(4, 4, 0.6, 109)

    quarried = np.clip(who * 0.55 + coarse * 0.45, 0.0, 1.0)
    colour = tint(quarried, np.subtract(STONE_LIGHT, STONE_DARK)) + np.asarray(STONE_DARK)
    colour *= (0.80 + 0.30 * weather)[..., None]
    colour *= (1.0 - pits * 0.30)[..., None]
    colour = colour * face[..., None] + np.asarray(MORTAR) * 0.7 * (1.0 - face)[..., None]

    height = face * (0.78 + 0.16 * coarse - pits * 0.10) + (1.0 - face) * 0.06
    rough = 0.88 + pits * 0.08 - weather * 0.04
    return finish(np.clip(height, 0.0, 1.0), colour, rough, 0.055, 0.80)


# ------------------------------------------------------------- glazing

#: A curtain wall: vertical mullions at 1.5 m and a transom at every
#: floor, which on a three metre tile is two bays across and one storey
#: up. The glass is DARK, because glass is: a tinted pane reflects a
#: tenth and passes most of the rest, so what a facade is made of is the
#: reflection and not the albedo. The mullions are anodised aluminium.
BAY = 1.5
STOREY = 3.0
MULLION = 0.06
GLAZED = (0.022, 0.033, 0.040)
MULLION_GREY = (0.210, 0.213, 0.220)


def glazing():
    """A glass office facade: dark panes in a grid of aluminium mullions.

    It exists because the owner's list says an office can be GLASS and
    the pane material was the wrong answer to it. `field::GLASS` is what
    a WINDOW is drawn with, a flat dark colour at a roughness of 0.08,
    and a whole building of it is a mirror: the first render of one came
    back as a tower that reflected the sky so exactly that it vanished
    against it, leaving its own floor slabs and window frames hanging in
    the air. A facade is a GRID, and the grid is what makes it read as a
    building: mullions catch the light where the glass does not.
    """
    v, u = cell(units(STOREY), units(BAY))
    fu, fv = u - np.floor(u), v - np.floor(v)
    wide = MULLION / BAY
    tall = MULLION / STOREY
    frame = 1.0 - joint(fu, fv, wide, tall)
    # The spandrel: the opaque band at each floor, behind which the slab
    # is. It is glass too, and a little lighter than the vision panel.
    spandrel = 1.0 - step(0.0, 0.18, np.minimum(fv, 1.0 - fv))
    smear = fbm(12, 3, 0.5, 307)

    glass = np.asarray(GLAZED) * (0.85 + 0.30 * smear)[..., None]
    glass *= (1.0 + spandrel * 0.55)[..., None]
    colour = glass * (1.0 - frame)[..., None] + np.asarray(MULLION_GREY) * frame[..., None]

    height = frame * 0.9 + (1.0 - frame) * (0.25 + 0.08 * spandrel)
    # Glass is SMOOTH and aluminium is not, which is the whole of what
    # tells the two apart at a distance; and the mullion is METAL, so its
    # reflection takes the frame's own colour and the pane's does not.
    rough = 0.16 + smear * 0.03 + frame * 0.24
    maps = finish(height, colour, rough, 0.030, 0.55)
    maps["orm"][..., 2] = frame * 0.85
    return maps


def main():
    """Write the six sets, or hold the committed PNGs to them."""
    sets = {
        "wood": wood(),
        "brick": brick(),
        "vinyl": vinyl(),
        "marble": marble(),
        "stone": stone(),
        "glazing": glazing(),
    }
    return write_or_check(sets, "--check" in sys.argv[1:])


if __name__ == "__main__":
    sys.exit(main())
