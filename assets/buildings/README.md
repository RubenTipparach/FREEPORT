# Buildings, as brushes in the field

A building is not a mesh here. It is a list of BRUSHES written into the
same density field the ground is made of, marched by the same marching
cubes, walked on by the same collision rule. Each `*.json` in this folder
is one recipe; `tools/bundle_buildings.py` bundles them into
`docs/mockups/buildings.js` for the mockup (and `--check` holds the bundle
to the recipes), and the core will read the JSON itself when the field
grows a material channel. `docs/mockups/kit.js` is the reference
implementation of everything below.

## The frame

x east, y north, z up, metres, origin at the middle of the footprint on the
ground (`u = 0` is the town's level). `footprint` is what the lot becomes,
`storey` is the height of one, `storeys` the range a lot may ask for, and
`door` is where along east the doorway is, for whoever walks in.

## A brush

```json
{ "op": "add", "shape": "box", "mat": "concrete", "at": [0, 0, 1.85], "size": [7, 6, 3] }
```

| key | what |
| --- | --- |
| `op` | `add` (union) or `cut` (subtraction), applied IN LIST ORDER: a cut after a wall goes through the wall, a flight added after a room stands in it |
| `shape` | `box`, `cyl` (axis `u` by default, or `n` or `e` for a vault), `sphere`, `stairs` (`steps`, `dir` `n` or `s`), `window` (an OPENING onto the room: the hole through the wall, `face` `n`, `s`, `e` or `w`; add `pane` for glass in it, `lit` true, false or absent for a hash) |
| `mat` | `concrete`, `plate`, `glass`, `lit` (a glowing pane), `lamp` (glows and is a light), `street` |
| `at`, `size` | centre and full size, metres, in the building frame; a cylinder's `size` is diameter, diameter, height along its axis; a flight's is width across, run along, rise up |
| `rot`, `tilt`, `pitch` | degrees about up, about north (east toward up) and about east (north toward up), for a gable, a ramp, a sloped rail |
| `clip` | `[lo, hi]` in the building's up: the brush exists only between those heights, so a vault is a cylinder from the wall line up and nothing under the floor |
| `blend` | a smooth union of that radius, for a plinth poured into the ground; the material still switches hard |
| `room` | on a cut: the inside of a building, lit by its lamps and named by the walker |
| `each` | repeat: `storey` (0 to S-1), `upper` (1 to S-1), `flight` (0 to S-2), `top` (once, at S storeys up); `at[2]` is offset a storey a time |
| `alternate` | with `each`: on odd repeats the brush is mirrored through the centre, so a flight is on the west wall rising north and then on the east wall rising south |

## The rules the field imposes

- **A sample's material is the DEEPEST solid at the point**, the one whose
  surface is farthest away. The mesher asks the field a hand inside each
  triangle's middle, which can be most of a cell inside the surface, so a
  thin thing on a thick one (a street on the ground, a lamp under a slab)
  is SUNK into its host by at least a cell or it draws as its host. A skin
  (a plate roof) is thicker than a cell or is the whole of the solid.
- **Nothing thinner than a cell's DIAGONAL exists.** The mockup dual
  contours towns at 0.22 m on a lattice that is the planet's and not the
  building's, so every plate lies oblique to it, and a cell that holds both
  faces of a plate puts its one vertex between them: a 0.3 m slab came out
  pitted, one pit a coarse cell. Walls, slabs, ramps, rails, eaves and
  lamps are 0.4 (the diagonal is 0.39) and the kit warns on anything under
  0.4. Windows are openings, so there is no pane to be thin. The game's
  near lattice is a cube sphere patch's, level and plumb where a town
  stands, so there a plate need only beat the cell and only a pitched
  thing the diagonal.
- **A cut leaves the material it cut through**, so a doorway's reveal is
  the wall's concrete.
- **Order is meaning.** Add the shell, cut the room, cut the door, add the
  flight, add the lamp. A flight added before the room is cut is cut away
  with it.
- **The walker walks the field, not the recipe.** Anything under 0.6 m is
  a step, anything higher is a wall, a slab above the head is a ceiling; a
  ramp is walkable at any pitch the field can hold, which is why the
  recipes climb on ramps (a pitched box) and not on the `stairs` shape: a
  flight of ten boxes is chunky at every lattice a browser can afford and a
  ramp is exact at all of them. The opening in the floor over a ramp runs
  from where a climber's head would meet the slab to the wall beyond, and
  is a body's width wider than the ramp. There is no collider to keep in
  step with the picture because the picture is the collider.
