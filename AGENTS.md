# freeport

An open world space game about WORKING in a space economy: a few thousand
stars, tens of thousands of planets, and a player who flies between them,
lands, gets out, walks, and earns a living hauling, mining, trading and
building. The scale is real: planets hundreds to thousands of kilometres
across, drawn from orbit to the ground with no loading screen and no seam,
and a ship you can climb out of on the ramp of a station or on a beach.

It is built the way [swarm-demo](https://github.com/RubenTipparach/swarm-demo)
is built, in Bevy, with that project's code organisation, documentation habit
and content tooling, and it takes its planet, orbit and floating origin
lessons from [tenebris](https://github.com/RubenTipparach/tenebris), which
solved those on a Mario Galaxy scale (a three hundred metre planet) and wrote
down what it learned. The design page is `docs/freeport.html`, published at
https://claude.ai/code/artifact/7822c376-33d9-4391-9907-a958426efc29; this file is the rules and the reasons.

No em dashes or en dashes anywhere, the same rule swarm-demo keeps.

## What is being built, in one table

| the thing | the number | why that number |
| --- | --- | --- |
| stars | about 3,000 | a map a player can learn regions of, and never all of |
| planets and moons | 10,000 to 30,000 | every star has a few, most never visited, all landable |
| a planet's radius | 200 km to 3,000 km | big enough that the horizon is flat on foot and curved from orbit |
| the smallest thing that matters | a bolt on a hull, 1 cm | the walker's world |
| the biggest distance in one frame | a star at 10^11 m | the sky's world |
| the walker's eye height | 1.7 m | everything about the FPS controller is measured off it |
| the ship | 8 m to 120 m long | a thing you walk through, not a point with a camera on it |

That table is what decides the two things this file is mostly about: how a
position is stored, and how ground is drawn. A single precision float at a
planet's radius steps in tens of centimetres (`pos::f32_step` is the
function, and the test that pins it), so the world frame is `f64` and the
renderer draws in an `f32` frame measured from a floating origin that follows
the eye. And a planet's surface is a density field sampled at whatever detail
the eye is near enough to see, dual contoured on one lattice at two levels,
because a height map cannot overhang, a column of hex prisms can only fake
it, and marching cubes cannot make a corner.

## The two crates and the line between them

`crates/freeport_core` is the game's own reasoning and depends on nothing but
`std` and `glam` (at Bevy's own version, so a `DVec3` here is a `DVec3` there
with no conversion at the boundary): world positions and the floating origin,
the cube sphere and its quadtree, density fields, the mesher, and everything
that comes after them (orbits, the economy, the star map, the damage model).
`crates/freeport_app` is the Bevy harness: it draws what the core says, runs
the physics, streams the chunks and collects input. This is swarm-demo's
boundary between `swarm_core` and `swarm_app`, and tenebris's between
`tenebris-core` and `tenebris-client`, kept for the same reasons: a rule with
one implementation cannot be changed in only one of two places, a crate with
no engine in it is tested without one, and Bevy is 0.x, so when the render
graph moves again the game is untouched.

The test is the same one: if two clients computed this differently, would the
world diverge? Then it belongs in the core. Where a ship IS, what the ground is
made of at a point, what a cargo is worth: core. Which chunk mesh is on the
GPU, how the camera eases, what a button looks like: app.

## The stack, and why each piece

| layer | choice | why, and what it displaces |
| --- | --- | --- |
| engine | Bevy 0.18.1 | ECS that proves systems disjoint from their filters; wgpu, so one shader language on every platform; what swarm-demo already runs and what this toolchain (Rust 1.94) builds. Bevy 0.19 needs Rust 1.95 and neither physics nor floating origin has followed it yet; the engine moves when all three do, in one commit that carries nothing else |
| physics | avian3d 0.6 | ECS native, `f64` build available, deterministic enough for a client authoritative game; Rapier through a plugin is the alternative and the extra layer is the reason not |
| floating origin | our own `pos::Origin` in the core, `big_space` 0.12 evaluated for the app side | the RULE (f64 world, f32 render frame, rebase past a radius, snap to a grid) lives in the core where a test holds it; whether the app's transforms are rebased by hand or by big_space's grid cells is an app decision, and big_space is the one to beat because it has already met the traps |
| terrain | a density field dual contoured in the core (`dc.rs`) on one lattice at two levels, chunked on a cube sphere quadtree | overhangs, caves, arches and craters with lips, and a corner that is a corner wherever something is built; marching cubes (`march.rs`) is the reference the surface table is derived from, tenebris's Goldberg hex prisms the alternative, and `docs/mockups` is where the two were compared on the same seed |
| planets from far off | a baked equirect impostor per body, tenebris's `distant.fs` ported to WGSL | the whole disk from a texture and an icosphere, with the rim fresnel and the terminator faded normal detail that made tenebris's planets read |
| atmosphere | single scatter ray march, tenebris's `atmosphere.fs` ported, with the CPU mirror | sky and distance fog agree by construction when the same march runs on both sides |
| orbits | closed form on rails (`orbit.rs` ported) | a solar system that cannot drift, blow up or need integrating; a player's ship is the one body that integrates, in a patched conic frame |
| textures | Material Maker, `materials/*.ptex` baked by `tools/bake_materials.sh` | the graph is the source and the PNG is its export; the Godot target writes the glTF layout (albedo, normal, ORM) Bevy reads without conversion |
| models | Blender, exported to glTF into `assets/models`, generated where a generator exists | a hull, a station module, a crate: authored once, with the rules below |
| mockups | three.js, in `docs/mockups`, published as artifacts | a new screen, a new mechanic's feel, a new mesher: rendered, linked, approved, then built |

## How the code is written

Ported from swarm-demo's `CLAUDE.md`, which ported it from redux-tribes'
`GUIDELINES.md`. These are rules, and the first three are checks:
`.claude/skills/tidy/SKILL.md` runs them all.

- **A file is under 900 lines and a function under 100.** `python3
  tools/shape.py --check` fails on either. The list it prints is work, never a
  reason to raise the limit.
- **rustfmt is the format**, `cargo fmt --all -- --check` in the suites. A
  formatting commit carries nothing else and goes in `.git-blame-ignore-revs`.
- **clippy is clean at `-D warnings` in the core**, and its count in the app
  never rises.
- **No em dashes or en dashes anywhere**, checked by
  `git ls-files -z | LC_ALL=C.UTF-8 xargs -0 grep -lIP '[\x{2013}\x{2014}]'`.
  The locale goes on the grep, not on git.
- **Single responsibility.** A module owns one thing and its first line says
  what; a Bevy system does one thing and is named as a verb phrase
  (`stream_chunks`, `rebase_origin`), a component is a noun, a marker is an
  adjective. A function that needs a section comment inside it is two
  functions, and `#[allow(clippy::too_many_arguments)]` is the smell that
  says a struct is missing.
- **Divergent paths for like functionality are a defect.** Two places that
  need one behaviour call one function. The allowed duplicates are the ones
  across a boundary the machine cannot cross, and each is under the rule that
  one side is the reference and the other its transcription: the marching
  cubes tables are extracted by a script into `tables.rs` and `mc_tables.js`
  from one source; the field in `docs/mockups/common.js` is `field.rs` line
  for line, on `Math.imul` so a JS number behaves as a `u32`; a WGSL
  transcription of a core function names the function it transcribes.
- **Open for extension, closed for modification.** A new material is a
  `.ptex` in `materials/`, a new hull is a glTF in `assets/models`, a new
  commodity is a row, and lists a player picks from are read off a manifest
  rather than typed. Tuning numbers are data in `assets/config/*.yaml` with
  the zero sentinel (nought means "use the default"), never inline in a
  system; the checklist for a new tunable is tenebris's: a field on the
  struct, a line in the YAML's own schema comment, the parser, the consumer.
- **Liskov.** Anything standing in for a `Body` keeps every invariant a body
  has. A moon is a body; a station is a body with a frame of its own; an
  asteroid is a body with no atmosphere and no gravity worth the name, and no
  query pretends otherwise.
- **Interface segregation.** A query names exactly the components it reads,
  and its filters ARE the interface: a rule a system must not see is a marker
  and a filter, not an `if` in twelve systems. Resources stay narrow, one
  fact each.
- **Dependency inversion.** The core depends on nothing but `std` and `glam`;
  the app depends on the core's public functions; that direction never
  reverses.
- **Rust, specifically.** `f64` for anything in the world frame and `f32` for
  anything in the render frame, and the type says which (`WorldPos` is never
  a bare `DVec3` in a signature). No `HashMap` in anything that will be
  hashed or replayed. No `unwrap` past startup, and every `expect` says what
  was assumed. Every `pub fn` in the core has a doc comment and a test. A
  constant lives beside the one system that reads it, with its unit in the
  comment. Guard every expression at the point it can leave its domain: a NaN
  in a transform is every frame wrong from then on. `single()` only where
  exactly one can exist.
- **A mockup before a large feature.** A new screen, a mechanic's feel, a
  mesher, a hull: rendered in three.js under `docs/mockups`, published,
  approved, then built. swarm-demo's move order was rebuilt as a prototype
  after the first cut shipped four defects, and that is the rule here from
  the first commit rather than the second.
- **Measure, then decide.** Numbers in the commit message, a clock rather
  than the engine's delta, and never "faster" without a before and an after.
  A frame time is a `std::time::Instant` and never Bevy's `Time`, whose delta
  is clamped and lies exactly when it matters.
- **Before a push**: `/simplify` on the diff for reuse and altitude,
  `/code-review` for correctness, then the suites (`/tidy` runs them).

## The world frame is f64 and the renderer's is measured from a floating origin

`pos.rs` is the whole rule and `an_f32_at_ten_thousand_kilometres_cannot_tell_a_millimetre`
is why: at 10,000 km from the origin the next representable `f32` is half a
metre away. `WorldPos` is an `f64` position in metres and it is what every
component that persists, is sent, or is reasoned about carries. `Origin` is
where the renderer's `f32` frame is measured from: it follows the eye once the
eye is `REBASE_RADIUS` (two kilometres) away, and it snaps to `ORIGIN_CELL`
(one kilometre) when it moves, so an origin is one of a countable set of
positions and a rebase is a function of where the eye is and never of which
frame it happened on. Within two kilometres of the origin an `f32` holds a
quarter of a millimetre, which is under anything a renderer can show.

The rules tenebris paid for, kept here in the same words:

- **Rebase in f64, then cast.** A body's render position is
  `(body.world - origin) as f32`, never `body.world as f32 - origin as f32`.
  The subtraction is the precise step and it happens once, in the core.
- **Shaders take PLANET LOCAL coordinates.** A water, atmosphere or terrain
  shader is handed the camera as `camera_world - body.world`, in the body's
  own frame, never the world camera. Tenebris's "Sequoia water saga" was a
  body at x = 20,000 handed the world camera: every view dependent term
  resolved against a garbage vector, the fog distance read twenty kilometres
  for every fragment, and the sheet went the colour of the sky. It was
  mis-fixed by turning the atmosphere off. The root cause was one subtraction
  in the wrong frame.
- **LOD is measured off the ABSOLUTE position, never the rebased one.**
  Mixing the two read every body as the same distance away the moment the
  world went heliocentric, and dumped the near mesh into the impostor.
- **Meshes are chunk local.** A chunk's vertices are `f32` metres from the
  chunk's own `f64` corner, and the chunk's entity is placed from that corner
  through the origin. Tenebris's hex mesher emits planet world `f32` and its
  own shader comments warn that a gradient in it "must exceed planet scale
  f32 quantisation or the derivative degrades to noise"; at a thousand
  kilometres that quantisation is six centimetres and the warning is the
  whole surface.
- **The far field is compressed, not clipped.** Tenebris's `far_shell` slides
  anything past fourteen kilometres inward at identical apparent size, order
  preserving so occlusion holds, because a sun at 150 km does not fit a 20 km
  far plane. The port here is a logarithmic depth buffer or a depth split,
  and `far_shell` is the fallback if either costs more than it buys; either
  way the sky's bodies are drawn where they LOOK, and the number that decides
  is measured in a picture.

## The ground is a field, sampled where the eye is

`field.rs` is what the ground is made of at a point: positive in rock,
negative in air, nought on the surface. A planet is a sphere plus fractal
relief on its direction plus a little three dimensional noise, and that last
term is the whole reason for a field: it is what lets a cliff undercut, a
crater keep a lip and a cave exist. The noise is value noise on an integer
hash, on nothing but add, multiply, floor and compare, because a field two
clients evaluate has to come out bit for bit the same on both and `sin` does
not (`tools/texkit` in swarm-demo has the long form of that rule).

`sphere.rs` is where to sample. A planet is six faces of a cube inflated to a
sphere, tangent warped so the corner cells are within a half of the centre
ones (1.42 measured, against 5.2 plain), each face a quadtree, each node a
patch of ground. `select` walks the tree for an eye and returns the leaves:
a node splits while it is wider than `ratio` times its distance from the eye.
**That distance is to the patch's NEAREST point, by angle, and not to its
middle**: the first cut measured the middle, and an eye ten metres over the
corner of a patch a thousand kilometres wide was five hundred kilometres from
its middle, so the ground under the eye stayed at the coarsest level there
is. `a_patch_is_as_far_as_its_nearest_point` holds it.

`march.rs` is the mesher: marching cubes on Bourke's tables, shared vertices
through an edge cache indexed by lattice point and axis (the one name for an
edge that two cells agree on), and normals off the field's gradient pointed
out of the rock. A sphere marches to a closed shell whose area is the
sphere's within three percent, and `normals_point_out_of_the_rock_and_the_winding_agrees`
holds the winding to the normals by area, because a sliver can point
anywhere and nothing of any size may point in. It is the reference now
rather than the mesher: `dc.rs` is what the game draws with, and it derives
which crossings are one surface from `march.rs`'s tables. The section on
the two levels below is the whole of it.

**Still to build, and the order it comes in:**

1. **The chunk streamer.** `select` gives a leaf set; the app diffs it against
   last frame's, samples and contours new leaves on a worker, and despawns
   the rest with hysteresis (tenebris held a chunk to one and a half times
   its load distance for three seconds, because a full reload was a two
   second stall and the boundary flaps). One cached verdict per body per
   frame that every pass reads, so terrain, water and the impostor cannot
   disagree mid flap. LOD is anchored to the PLAYER, not the camera: a map
   view that pulls the eye to twenty kilometres must not unload the ground
   under the ship. A leaf beside a leaf one level up joins it by the rule
   `dc.rs` already applies between the two levels of one lattice: the seam
   is polygons whose corners are cells of both sizes, and nothing is
   skirted. The skirt was the first answer here and the mockup's picture
   retired it before the streamer existed.
2. **The impostor tier.** Past the streaming radius a body is tenebris's
   baked equirect on an icosphere, lit per body from its own star.
3. **The rest of the materials.** The sets are on the field in the harness
   (the section on the sets below); what is left is the sand band by height
   once there is a sea, concrete panels in a building's own frame rather
   than the planet's, and mipmaps for the array textures.

## A town is where a mesher is judged, and the walker is the judge

The two mockups carry the same planet (sixty four metres, a sea half a metre
under the mean radius, sand to a metre and a half above it, grass beyond),
the same three towns and the same first person walker, because a slope, a
kerb, a wall and a doorway are what a terrain system is FOR in a game about
walking out of a ship. The town plan is shared (`planTowns` in
`docs/mockups/common.js`: sites where the land is nearly level a little
above the sea, the port first, a grid of ten metre blocks and four metre
streets, a building on most lots, taller near the middle) and what differs
is what a building is and what the ground does under it:

| | the field, marched | the tiles, stacked |
| --- | --- | --- |
| the ground under a town | the field is FLATTENED under it (`Planet.surface` blends the relief to the site's height and fades the volumetric term), so the plateau has a smooth skirt cut by the same field | the tiles are LEVELLED, so the plateau has a wall of blocks wherever the hill was higher |
| a building | a list of BRUSHES in the same field (`assets/buildings/*.json`, the kit in `docs/mockups/kit.js`): boxes, cylinders, spheres and flights of steps added or cut in list order, in concrete, plate, glass or lamp, marched on a lattice six times finer under the town, each building plumb on its own patch of the sphere | the lot's tiles raised by the storeys in half metre blocks and tagged concrete, windows lit by the shader; the doorway, the floors, the stairs and the lamps are RUNS of blocks in a column, nothing placed and nothing to align |
| a wall on foot | the FIELD: a ring of points round the body between its step and its head, each pushed out along the field's own gradient, sideways only | any tile round the body standing higher than a step, pushed off along the line from its middle |
| a room | a face whose air side is in a room cut is TAGGED inside; both pages light an inside face from the building's lamps and nothing else, with the sun ignored | the same tag, on the wall faces toward an interior tile |
| the concrete | three planes in the BUILDING's own frame: its anchor and its east ride the vertex, so the position from the anchor is the recipe's e, n and u, panels of 3 m level and plumb on each building whatever the planet's axes do, and the material rides the triangle FLAT, so concrete meets rock on a line | UVs in the TOWN's frame: a cap on east and north, a wall along the town axis it faces across and up from its base, so the panel seams meet the blocks and the floors |

**The walker is one class and the page supplies three functions.** `Walker`
is a direction on the sphere and a height off the ground, a heading carried
as a tangent vector and squared to the local up every frame (the surface
walker as a basis), velocity with acceleration and friction, a body radius
of 35 cm, a step of 60 cm, a head at 1.85 m, a jump that clears a metre,
and the sea holding it at wading depth. The page answers `ground(dir)` (the
highest floor, step or roof no more than a step above the feet, else the
field's first crossing from space; on the hex page the highest run top in
the tile under the direction, found by a greedy walk from the last one,
tenebris's `find_tile`), `ceiling(dir)` (the lowest solid above the feet,
which is what stops a jump under a slab and clamps the head under a
lintel) and `resolve(dir, foot)` (the direction pushed out of whatever solid
the body overlaps between its step and its head). A floor with a floor
over it is a floor, and a step is a wall from the front and a floor from
above, which is what a step is. **Resolve, never "may I".** The first cut asked `blocked(from, to)` and a walker that
touched a wall stood glued to it, because every step from a touching
position touches. A walker that is pushed OUT of what it overlaps slides
along a wall for free, and the two pages' walkers then do the same thing on
the same seed to a few decimetres: seven metres up the port's main street
to the first face, nineteen along it, a jump of 1.4 m. Driven headless,
with the numbers printed, because a walker that feels right is a walker
whose numbers a second person can check.

**Three pictures found what no number did.** The block kit drew black but
for its windows: the lot frame was built as east, up, north, which is left
handed, so every box wound inside out and only the emissive panes survived
the cull. A hex walker could stand in a column's corner and see the inside
of the world, because the body was held off the hexagon's flats and not its
corners; the reach is the circumradius now. And the owner caught the third
off the published page: the streets hung out past the horizon. A street was
one straight bar the length of the town placed at its own middle, which is a
CHORD, and on a sixty four metre planet a fifty six metre chord sags six
metres at its ends, while every building, placed on its own patch, sat
down. A street is laid in three and a half metre pieces now, each on its
own patch. On a thousand kilometre world the same chord sags under a
millimetre, which is why this is a rule about the MOCKUP's planet and not
about the game's: anything long enough that the ground curves under it is
placed in pieces, and how long that is depends on the radius. All three are
the kind of defect a screenshot catches and a test suite does not, which is
the reason the mockups have a screenshot harness at all, and the third is
the kind only a second pair of eyes catches, which is the reason they are
published.

**Inside, and up the stairs, on both.** Every building has a doorway on the
street side, a floor a storey, a flight along one wall (west rising north,
then east rising south, so the next flight is across the room), a hole in
the slab over the flight, and a lamp under every ceiling. The block kit
builds them as boxes; the hex page GROWS them, because on a column world a
step, a floor, a lintel and a wall are all the same thing, a run of blocks
in a column: a ring tile is wall to the parapet with a gap of four blocks
where the door is, an interior tile is the ground, a slab a storey and, on
the stair strip, a step of k blocks, with the slab above cut where a
climber's head would meet it. The walker climbs both the same way, and the
harness proves it: in at the door, west to the wall, north up the flight to
floor one (3.35 m over the base on the kit, 3.04 on the blocks), east
across, south up the second flight to floor two, and `where` naming the
building and the floor at every stage. Four things the numbers found before
a picture could:

- **The flight topped out over a drop.** The slab's hole was cut a stride
  past the top step on both pages, so a walker stepping off the flight fell
  through its own stairwell. The hole ends at the top step's far edge and
  starts a little short of the first step whose climber's head would meet
  the slab, and nowhere else.
- **A flight against the wall it faces cannot be got onto.** The hex flight
  started flush with the south wall and the kit's a step from it with the
  handrail running to the floor, so the only way on was from the side, over
  a metre of riser. A flight stands a stride clear of the wall it faces
  (`GAP`, `STAIR_GAP`) so a walker gets on at the bottom step, and the rail
  starts at the third step, because the two low ones are climbable from the
  side and a rail to the floor is a wall to go round the end of.
- **The exterior floor line was a box the size of the lot.** Drawn, it was
  hidden inside the walls and the slab; as a collider it caught a climber's
  head at the fourth step and, pushing out along the least penetrated face,
  shoved the walker through the west wall and out of the building. It is
  four bars round the outside now and decoration never collides.
- **Two risers are exactly a step.** 0.3 twice against 0.6, and on the
  frames the sum rounded up the second step was a wall. The collider gives
  the same centimetre of slack the ground query already had. A tie on a
  threshold is a coin toss, and the coin is the rounding mode.

A lamp is the one thing the shader is handed: up to `MAX_LAMPS` (96) of
them as position and reach, a fixed uniform array because GLSL has no
other kind, and only a fragment tagged inside pays for the loop. The lamp's
own diffuse is wrapped, because a lamp a hand under a ceiling lights the
whole ceiling at a grazing angle and a plain cosine there lights every
grain of the normal map on one side: the ceiling came out as gravel and the
map is at half strength inside for the same reason.

**Six material sets are three samplers.** A set is three maps, and six
sets bound one map to a sampler each is eighteen, past the sixteen a
fragment shader is promised on a real GPU: the plate set took the mockups
from fifteen to eighteen, they drew on swiftshader, which allows more, and
the owner's browser refused the program with `MAX_TEXTURE_IMAGE_UNITS(16)`.
The maps are stacked a kind at a time into three array textures
(`stackMaps` in `common.js`, a layer a set, the rows turned over because
pixel data is not flipped on upload the way an image is), so the shader
binds three whatever the count of sets, and the pictures before and after
differ by the HUD's digits alone (0.075% of pixels). The Bevy port has the
same limit and the same answer.

## A building is a field too, on a finer lattice, and the material is flat

The question was whether a building can be built out of the same marching
cubes as the planet, with the two textures kept apart rather than blended.
It can, and the marched mockup now does it, which answers three things at
once: what a building IS in a field, how a fine thing lives in a coarse
lattice, and how a material stays hard.

**A building is a list of brushes.** A recipe (`assets/buildings/*.json`,
the schema in that folder's README, bundled by `tools/bundle_buildings.py`
into `docs/mockups/buildings.js` under the one source rule the marching
cubes tables live under) is a list of signed distance brushes in the
building's frame: a box, a cylinder on any axis, a sphere, a flight of
steps (kept in the kit, unused: the recipes climb on RAMPS, one pitched box
each, exact under the walker at any lattice where a flight of ten boxes
was chunky at every lattice a browser can afford), a window, which is an
OPENING onto the room and nothing in it. Each is ADDED (a union,
or a smooth one with `blend`, for a plinth poured into the ground) or CUT,
in list order, so a door cut after a wall goes through the wall and a
flight added after a room stands in it; `each` repeats a brush a storey at
a time and `alternate` mirrors it through the centre on odd storeys, so a
ramp is on the west wall rising north and then the east rising south, with
the opening in the floor above it cut from where a climber's head would
meet the slab to the wall beyond, a body's width wider than the ramp;
`clip` bounds a brush in the building's up, so a vault is a cylinder from
the wall line and nothing under the floor; a cut that is a `room` names the
inside. `kit.js` is the reference implementation, and it knows nothing of
planets: the page hands it local coordinates and takes back a density and
a material, which is what lets the core evaluate the same list one day.
Six recipes ship: a house and a cottage with a gable, a round tower, a
hangar with a barrel vault and an arched door, a dome with an oculus, and a
dugout with a trench of steps down into the ground. The last three are
what a block kit cannot make and the reason to want a field.

**The material is the DEEPEST solid at a sample**, the one whose surface is
farthest away. The mesher asks the field a hand inside each triangle's
middle for its material and a hand outside for its room, and hands both to
the triangle FLAT (`flat varying`, and EVERY vertex of a triangle carries
its values, a vertex its triangles disagree on being split, because a flat
value is read off whichever vertex a driver calls provoking: the last on
paper, and the first cut leaned on the last, and the owner's GPU hatched
every wall along the quads), so a triangle is one material, a material
boundary runs along triangle edges, and concrete meets rock on a line however the
geometry blends. The rule has a price the recipes pay: a hand inside can be
most of a cell inside the surface, so a thin thing on a thick one (a street
on the ground, a lamp under a slab) is SUNK into its host by at least a
cell or it draws as its host, and a skin is thicker than a cell or is the
whole of the solid. And nothing thinner than a cell's DIAGONAL exists,
because the lattice is the planet's and not the building's, so every
plate lies oblique to it, and a dual contoured cell that holds both faces
of a plate puts its one vertex between them: a 0.3 m slab on the 0.22 m
lattice came out pitted, one pit a coarse cell, and every pit was a
vertex measured at the slab's mid plane. Walls, slabs, ramps, rails,
eaves and lamps are 0.4 (the diagonal is 0.39) and the kit warns on
anything under 0.4; since the mesher holds a vertex a surface the diagonal
is a margin and the CELL is the limit, and it is the exposed STEP that
must beat it, not the box: a lamp 0.4 deep sunk 0.25 into its slab hangs
0.15 and smears, so lamps hang 0.25, parapets and a vault stand 0.25 out
of their walls, and a street stands 0.25 proud of the ground. In the game
a chunk's lattice is a cube sphere patch's, level and plumb where a town
stands, so there a plate need only beat the cell and only a pitched thing
the diagonal. A box
face is shaded FLAT on its own normal (`dFdx`, the curved brushes flagged
to stay smooth), because a normal interpolated across a corner rounds it
over a cell.

**A fine lattice under each town, DUAL CONTOURED, in chunks, meeting the
coarse one on cell faces.** The planet is marched at 1.33 m with no
buildings in its field. Every coarse cell a structure's box touches is
REPLACED by its subdivision (four, six or eight ways, the page's select;
six is 0.22 m) with the buildings in the field, in chunks of four coarse
cells a side, so an edit remeshes the chunk or two its brush touches. The
fine lattice is not marched: marching cubes bevels every edge across the
cell that holds it and can never make a right angle, and a building is
right angles. It is dual contoured: one vertex per SURFACE in a fine cell,
at the least squares point of that surface's edge crossings' positions and
normals held a little toward their middle, and a quad for every crossing
edge round the four cells that share it, each cell giving the vertex of
the surface the edge is on, so a box's corner is a corner and its wall a
plane at any lattice, and a curved brush stays curved. Which crossings are
one surface is what the marching cubes case already says: its triangles
for the cell's corner signs, joined where they share an edge, so a plate's
two faces in one cell, or a step and the wall it stands out from, are two
vertices and not one between them. One vertex a cell was the first cut,
and it put a pit in every ceiling and a slit of sky in every pillar,
because the pillars stand 0.27 m out of the walls. The crossings are found by
bisection along the edge where a structure is involved (a box's distance
curves round its corners and the straight line's guess beaded every edge),
their normals are the field's gradient there, and a chunk computes a shell
of its neighbours' cells so the quads on its border are the same from
either side. The join is on coarse cell faces where the field is the
terrain alone on both sides, because a structure's box is inside its
cells, so the two surfaces differ there by the coarse lattice's own
interpolation error, and the fine mesh dives under the coarse one there:
the cells outside the fine region get their vertex sunk along the normal
by however far it stands above the coarse surface (the coarse field,
interpolated) and three centimetres more, so the rim is never seen; a
fixed three centimetres fell short wherever the gap was ten. The gap is
MEASURED anyway: every crossing on a join face is checked against the
bilinear coarse field on that face and the page reports the mean and the
worst. On this seed 9,339 crossings lie on the join, 5 mm apart on average
and 108 mm at worst, the worst where a street runs out of the levelled site
into the skirt, which is the chord sag this file already knows about. The
mockup keeps that skirt. The game does not, and the section on the two
levels below is why.

**A dual contouring mesh is watertight by construction, and the page
proves that its own is.** Every crossing edge gets one quad and every quad
shares its vertices with its neighbours, so the only ways to lose that are
an edge with two owners or none, and a triangle wound to face into the
rock, which is culled and is a hole with the room's dark through it. Both
happened, and each is closed by a rule: an edge is owned by the lowest
chunk among the masked cells round it, which both chunks can tell from
the mask alone, where before a chunk border went to the low chunk outright
and where the low chunk had no cells the fine region ended on an open rim;
a chunk samples two points of margin round itself rather than one, so a
terrain crossing's lattice gradient is the same from both chunks that hold
its cell, where before a chunk fell back to the field on its shell and
placed the shared vertex a hair from its neighbour's, 7,093 edges' worth;
and each HALF of a quad is wound by the field's own gradient at its middle,
where one test for both halves let a folded quad's second half face in.
Two cheaper windings were measured and refused: the crossing's normal left
259 halves facing in, and one sample a hand off the face flipped 5,713,
every floor triangle within a hand of a wall, because at a concave corner
both sides of a face are rock. `checkMesh` on the page is the proof: the
fine mesh welded by position across the chunks, every edge counted, an
edge in one triangle a hole unless both its ends are on the skirt's rim,
and every triangle tested against the gradient at its middle. It reads
nought holes and one facing triangle in 898,432, before and after a house
is built with the tools below, and it is what the commit message carries.
What it does not count is a pinch, a cell whose marching cubes case joins
two surfaces through an ambiguous face and puts one vertex between them:
splitting such a component on its normals was tried, and the neighbours,
which had not split, left 3,316 edges open along the cuts.

**The walker walks the field and nothing else.** The ground is the first
solid under the feet going down from a step above them, the ceiling the
first solid going up, and a wall is any solid a ring of points round the
body meets between its step and its head, pushed out along the field's own
gradient, sideways only. There is no collider list to keep in step with
the picture because the picture is the collider, a ramp is walkable at any
pitch the ground query can follow, and a sculpted block is walkable the
frame it is placed. Four things the numbers found: the push
was scaled by the gradient's DIFFERENCE rather than the gradient, so a one
centimetre contact threw the body fourteen and a pane threw it out through
the glass; the local panel frame was built at the fragment, and a point on
a sphere projected on its own tangent plane is nought everywhere, so the
panels were float noise until they were measured as arcs from the pole,
and now from the structure's own anchor, which is exact and needs no trig;
a body that steps DOWN by less than a step was airborne every frame of
a downslope, flickering down every plinth's fillet, until a small drop
became a step; and a walker that climbs anything rising less than a step
per stride walks up the END of a ramp's rail, which leans back a third,
so ground steeper than fifty degrees is refused, and refused outright it
stopped at every door, because a plinth's fillet is a kerb with a
shoulder. A steep face is climbed only where it tops out within a step of
the feet two body widths on: the fillet does, and a rail's end, a cliff
and a wall do not.

**Building is the same list, longer.** On foot, B: a brush at the point
the crosshair meets the field (a march along the look ray, the field's
gradient for the face), snapped to half a metre, added with the left
button and cut with the right, R and F lifting where it lands by half a
metre, Z taking it back, and a button exporting what was built. The
shapes are a block, a slab, a pillar, a ball, a ramp, a WALL laid from one
click to the next (0.4 thick, 2.6 high, its foot sunk 0.3 so a wall to
eight metres keeps its foot in the ground where the ground curves away, the
street pieces' chord lesson again), a PAD, which is flat ground (a fill of
terrain up to the aimed height and a clearing above it, so a hill is cut
and a dip is filled and the top is a plane), a ROOM, which is a cut that
is tagged inside and so is lamp lit like a recipe's, and a door and a
window, which are the cuts a wall wants; the materials are concrete, plate,
glass, lamp and terrain. That is constructive solid geometry in the field:
add and cut in the order made, and a wall with a room cut behind it, a
door through it and a lamp under a slab is a house the walker walks into.
Edits are built on SITES: the first edit on a spot fixes a frame, its patch
of the sphere, and every edit within eight metres is placed in that frame,
because two edits on their own patches lean against each other by the
angle between the patches, 1.8 degrees at two metres on this planet, so a
room cut meant to share a face with a wall leaned 8 cm into it at the top,
thinned it under a cell, and pinched it; in one frame a shared face is one
face and a pad is exactly flat. The same reading is the frame's inverse:
a site is found where the crosshair is by the map a lot frame is built
with, inverted, where the first cut read it off the tangent plane and a
brush aimed 27 m from the town's middle landed 2.5 m short. An edit is a
structure like any other, so it lives in the same buckets, the same fine
region and the same collision, and the export is one recipe a site, its
brushes in the site's frame, which is a recipe somebody has not written
down yet.

## The two levels are one dual contour, and the core proves it closed

The mockup's join was a skirt: the fine mesh's rim sunk three centimetres
under the coarse surface so the crack between the two could not show. The
owner's picture said it did: a line of dark slits along the join, seen from
a pad at a grazing angle. Rendered each mesh alone, the coarse mesh's edge
is a chord of a 1.33 m cell and the fine mesh follows the true surface, and
wherever the chord stands ABOVE the fine surface a line of sight passes
under it into the interior of the planet, which nothing meshes, and out the
far side to the sea. A skirt hides the case where the fine stands above the
coarse and cannot close a lip from below: under a lip, sinking the rim only
opens the slit. Measured on every fine crossing along the mockup's join: 622
of 6,542 have the fine above the coarse by over a centimetre and 139 the
coarse above the fine, by up to 7.5 cm; with a pad built on a slope, 850,
236 and 16 cm. Every lip is a slit at some angle, and the page's hole count
exempted the rim, which is exactly why it read nought while the join leaked.

**The answer is the octree one, and it is in the core.** `lattice.rs` is one
lattice at two levels: a coarse grid of cells, a mask of the cells that are
subdivided `sub` ways, and every position computed from a FINE index through
one function, so a coarse corner and the fine point under it are the same
bits and two chunks agree on every sample. `dc.rs` contours it a chunk at a
time: a MINIMAL edge is a fine edge wherever a subdivided cell is round it
and a coarse edge everywhere else, and the polygon on a crossing minimal
edge joins the vertices of the LEAVES round it, which are fine cells on one
side of a join and the coarse cell on the other. Nothing dives under
anything and nothing is sunk: the seam is polygons whose corners are cells
of two sizes, every mesh edge is shared by exactly two polygons, and the
mesh is closed by construction. The rules that make that hold:

- **A leaf's vertex is a function of the field and the leaf alone**, one per
  surface (the marching cubes case's triangles joined where they share an
  edge, `components`), at the least squares point of that surface's
  crossings, each crossing bisected on the field and given the field's
  gradient there. Every chunk that needs a leaf's vertex computes it the
  same way from the same samples, so a shared vertex lands on the same bits
  from either side and the audit's weld finds it once.
- **An edge has one owner.** The lowest chunk among the subdivided cells
  round a fine edge, among the cells round a coarse edge, which every chunk
  can tell from the mask alone.
- **The coarse corner of a seam polygon** is the coarse cell's vertex for the
  surface of the coarse edge the fine edge lies on, else for the one surface
  crossing the face it lies in, else the nearest of the cell's. And the mask
  is GROWN first (`Lattice::grow`): an unsubdivided cell whose face toward a
  subdivided one has four coarse corners of one sign while a fine point on
  it has the other is subdivided too, and again until nothing changes, so a
  coarse cell the fine surface crosses always has a vertex to end on. The
  count of corners that had none is reported and is nought.
- **The least squares point is the pseudo inverse from the crossings'
  middle** (`qef.rs`), constraining only the directions the planes
  constrain: onto a face, onto an edge, into a corner, and never off the
  crease. The mockup's solve pulled every vertex a little toward the middle
  of its cell, and on the pad's rim that put a zigzag strip of slivers along
  every box edge.
- **Nought is rock, and a build is half a fine cell off the lattice.** A
  box face laid exactly on a lattice plane samples nought all over it.
  Called air, the crease where the ground met the pad fell on a lattice
  edge with a crossing on the crease itself and no normal to give it, and
  the foot of the pad grew a shelf. Called rock, the two cells either side
  of that edge both solve to the same point on the crease: two vertices in
  one place, which the audit counts as a pinch, 97 of them round the pad.
  The rule is that what is built snaps to half a metre and the lattice's
  corner sits half a fine cell off that grid, so no face ever lies on a
  lattice plane, and `a_face_on_a_lattice_plane_pinches_and_half_a_cell_of_offset_does_not`
  holds it: the same pad pinches on a lattice it coincides with and is
  clean on one offset by half a cell.
- **`audit.rs` measures what the construction claims.** The chunks welded by
  position (half a millimetre, searching the neighbouring bins too, because
  two chunks place a shared vertex a float's rounding apart and a rounding
  that straddles a bin edge read as four holes), every edge counted, every
  triangle tested against the field's gradient at its middle. A sphere at
  one level, a sphere with a blob of fine cells over its top, and a planet
  with a slab and a wall built on it all come out with nought open edges,
  nought pinches and nought triangles facing in, and the slab's top is a
  plane to two millimetres, which is what dual contouring is for.

`freeport_app` draws it: a 40 m planetoid on 96 cells of a metre, a site of
fine cells at a quarter under a pad, a wall and a step, a fly camera (click
takes the mouse, Escape gives it back, WASD and Q E, Shift, Tab for the
wireframe, `--fly` to start in the air) and `--shot out.png` for a picture
taken headless under Xvfb and lavapipe, which is how the join was looked at
here from the owner's angle before anybody flew round it; the first cut
coloured coarse vertices sand and fine ones blue so a seam polygon blended
the two, and those pictures are the design page's. Vertices are split per
triangle for Bevy, and a corner whose smooth normal disagrees with its
triangle's face by more than a crease takes the face's, so a box is shaded
flat on each face, the ground stays round, and where the ground meets a
wall only the corner on the crease changes. A first cut flattened the whole
triangle and the shading jumped along every crease.

## The sets on the field, and the walker on it, in Bevy

**A triangle is made of what the field says a hand inside its middle.**
`Density::material` answers `TERRAIN` or `CONCRETE` at a point, `Built`
says concrete wherever a block's density beats the ground's (the deepest
solid, the mockup's rule), and `dc.rs` asks it half a fine cell inside every
triangle's middle (`HAND`) and carries the answer per triangle; the built
planet test holds the slab's top concrete and the ground terrain. The app
puts it in the vertex colour's red, every corner of the triangle the same,
so no driver's choice of provoking vertex can change it, which is the
mockup's hatched walls not happening twice.

**The shader is the mockup's, transcribed.** `terrain.wgsl` is an
extension on Bevy's standard material: `tri` and `triN` line for line
(three planes weighted by the normal's fourth power, a normal map read on
each and turned into the world), rock on the steep and grass on the flat
by the same smoothstep, concrete where the triangle says so, and Bevy's
own PBR lighting after. Every sample is taken whatever the material and
blended by weight, because a texture sample under a branch is not in
uniform control flow and the compiler refuses it; the mockup's GLSL was
allowed the branch. The sets are three array textures, a layer a set,
which is the mockup's answer to a real GPU's sixteen samplers, decoded
straight off the checkout at startup (`terrain.rs`; `FREEPORT_ASSETS` or
the checkout the binary was built from), and a missing map is a flat layer
with a warning so the harness runs anywhere. Every coordinate the shader
reasons in is planet local, from a centre it is handed.

**The walker is the mockup's, in the core.** `walker.rs` is the `Walker`
class and the marched page's three rules ported number for number: an eye
at 1.7 m, a body of 35 cm, a step of 60 cm, a head at 1.85 m, five metres
a second walking and eight and a half running, a jump at 5.3 m/s that
clears a metre; `ground` (the highest solid no more than a step over the
feet, else the first solid going down, else the first crossing from space
when the feet are not yet known), `ceiling`, `resolve` (a ring of twelve
points at three heights pushed out along the field's gradient, sideways
only, three passes) and `can_stand` (fifty degrees, unless it tops out
within a step two body widths on). The field it walks is the same `Built`
the mesher contoured, so the picture is the collider. The harness is
`walk.rs`: WASD, Shift, Space, the mouse, F to swap with the fly camera
from wherever it is, and a line of text saying where the feet are and what
they stand on. Four walks are the tests: two seconds on a ball walks 8 to
10 m and running further; a jump peaks between 1.0 and 1.6 m and lands
inside 1.3 s; a 0.4 m kerb is walked up and a 1.2 m wall stops the body
its own radius short; a wall walked into diagonally is slid along, and a
lintel a stride ahead holds a jump under it to 0.6 m. One lesson from
writing them: on a 20 m ball a flat block three metres from the pole stood
0.22 m higher than the curving ground, so a 0.4 kerb was a 0.63 wall and a
wall's far end stood clear of the ground; the test ball is two kilometres,
and a real block on a planet is built plumb on its own patch, which is the
mockup's lot frame and the streets in pieces again.

## Bodies orbit on rails, ships integrate, and a station is a frame

Every planet, moon and station's position is a closed form function of the
world clock (tenebris's `orbit.rs`): no integration, no drift, nothing to
save. A ship is the one thing that integrates, under avian, in the frame of
the body whose gravity dominates (tenebris's `gravity_at` picks the STRONGEST
pull, not the nearest body, because a moon's well overlaps its planet's), and
a patched conic is how the nav display draws where it is going.

An orbit's speed is DERIVED from the same gravity the integrator applies,
never configured: tenebris's station orbits at `sqrt(g R^2 / r)` for the
same `g` the walker feels, so a ship coasting beside it is velocity matched
by construction and docking is a rendezvous rather than a snap.

**Inside a station's field the station is the fixed frame and the world moves
round it.** A walker on a deck lives in station local coordinates, integrates
there, jumps straight up and lands on the deck, and the world position is
COMPOSED from the station's pose every frame, never integrated. A player
integrated in the world frame inherits the station's fifteen metres a second
as leaked velocity, and the moment their feet leave the deck the station
curves away under them. Any approach that keeps the player a free world body
plus per frame corrections leaks. The same rule applies to a ship's interior
while it flies, to an elevator, to anything that moves with a floor on it.

**Free rotation is a quaternion; a surface walker is a basis.** A ship's
attitude is a `DQuat`, integrated as `q = q * from_axis_angle(body_axis, a)`
and normalised once, and its basis is DERIVED from it each frame. The
tangent basis representation (`up`, `fwd`, `right`, rebuilt with `up` as the
boss) is a gimbal: cross the pole and it flips and drops the roll. It is the
right model ONLY for a body glued to a surface, where up really is radial and
roll does not exist, which is the walker. Tenebris's `steer_rocket_attitude`
is the port.

**Getting out of the ship leaves the ship.** Exit parks it, pose and fuel and
all, where it is; it keeps drawing; walking up to it and pressing the key
boards it again. A ship that vanished on exit was tenebris's longest open
bug and it is the first thing to test in a build, in flight AND after a
reload, and only in the running game: the headless tests passed the whole
time it was broken.

## Content: what is generated, what is baked, and where it lives

- **Textures are Material Maker's.** A material is a graph in
  `materials/<name>.ptex` and nothing else is its source. `tools/bake_materials.sh`
  exports every graph through Material Maker's own command line (Godot
  target, which is the glTF layout Bevy reads: `<name>_albedo.png`,
  `<name>_normal.png` in OpenGL green up, `<name>_orm.png` with occlusion in
  red, roughness in green and metallic in blue, and `<name>_heightmap.png`),
  box filters them to 1024 because the export writes 2048 whatever the graph
  says (measured), and puts them in `assets/textures/terrain`. `--check`
  re-exports and holds the committed maps within half a percent of pixels,
  a tolerance rather than `cmp` because a GPU render is not bit exact across
  drivers. `tools/get_material_maker.sh` fetches the tool; it runs headless
  under Xvfb and lavapipe with the forward_plus renderer, and only that one:
  the mobile and GL renderers crash under lavapipe while this one exports
  cleanly, which took an afternoon to find and is written here so nobody
  finds it twice. Seven sets ship: basalt, regolith, ice, dunes, hull
  plate, grass and concrete, all authored SHALLOW, because a normal map at
  full strength on a flat quad reads as gravel (swarm-demo runs its finishes
  at a fifth).
- **Models are Blender's, or a generator's, never hand edited in a text
  editor.** A baked model is a `.blend` beside its glTF export in
  `assets/models/<thing>/`, metres, Y up, origin at the pivot the game will
  turn it about (a ship's centre of mass, a module's docking face), one
  material per Material Maker set by name, and a `README.md` in the folder
  saying what it is and what generated it. Anything the game can make from
  parameters (a rock, a terrain, a station ring from a radius and a count) is
  generated in code and never baked, which is tenebris's rule: a procedural
  game's default for new geometry is procedural, and baking is for the things
  only a hand can draw.
- **Every asset has a committed source.** A PNG without a `.ptex`, a glTF
  without a `.blend` or a generator, a table without the script that wrote it
  is a file nobody can regenerate and nobody can review a change to.
- **Screenshots prove things.** A change that claims to change nothing is
  proved by rendering the same scene on the binary from before and after and
  running `tools/pngdiff.py` on the pair, against that scene's own floor. A
  camera for a picture is SOLVED, never hand aimed: tenebris's LODCAM solves
  yaw and pitch against the live basis every frame after guessed angles
  stared at empty sky for a week.

## Suites

```sh
cargo test -p freeport_core                       # 39, the core
python3 tools/shape.py --check                    # no file over 900 lines, no function over 100
cargo fmt --all -- --check                        # the format
cargo clippy -p freeport_core -- -D warnings      # the core's lints
tools/bake_materials.sh --check                   # the maps match their graphs
python3 tools/bundle_buildings.py --check         # the mockup's recipes match assets/buildings
python3 tools/pngdiff.py before.png after.png     # a refactor's pictures, against the scene's own floor
cargo build --release -p freeport_app             # the harness (needs libwayland-dev libxkbcommon-dev libudev-dev libasound2-dev on Linux)
./target/release/freeport_app                     # a window: on foot at the site, the pad, the wall and the step in the baked sets; F flies, Tab wires, Esc frees the mouse
./run.sh --test                                   # the core suite and the shape and recipe checks, then the build and the window; run.bat is the Windows twin, --shot out.png takes the picture below with no display
./target/release/freeport_app --fly --sub 6 --wire --eye 9.5,41.6,0.5 --look 0,40.6,0 --shot join.png   # a picture, headless under xvfb-run with VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json
```

The mockups are `docs/mockups/marching-cubes.html` and
`docs/mockups/hex-terrain.html`, opened straight off the checkout (they find
the baked textures in `assets/textures/terrain` from there) or served from
the repository root, and published:

- marching cubes: https://claude.ai/code/artifact/342b7f52-5a1f-4000-94b3-1d3967b527d1
- hex terrain: https://claude.ai/code/artifact/87905fd8-d47b-4f2e-8cc3-8c226251a799

`docs/freeport.html` is the design page and links both. **A published page
is republished in the same change as the file it was made from**, which is
swarm-demo's registry rule: a mockup a version behind is worse than none,
because a reader cannot tell which.

A headless Bevy run needs a Vulkan device; on a box with no GPU that is
`mesa-vulkan-drivers` for lavapipe and Xvfb, the same rig Material Maker
bakes on here. Numbers from lavapipe are not GPU numbers and are for
correctness and A/B only.

## Measure, then decide

Numbers in the commit message. What is measured so far:

- `freeport_core`: 39 tests in 0.27 s. A 6 m sphere on a 32^3 lattice at half
  a metre marches to 5,288 triangles, a closed shell within 3% of the
  sphere's area, and dual contours to one at both one level and two.
- The harness's planetoid, in release: 96^3 cells of a metre, 1,283 of them
  subdivided four ways under the site and 2 grown, 1,835 chunks with
  triangles in them, 69,682 triangles of which 248 polygons are seams,
  contoured in 836 ms of which 208 is the coarse samples, audited in 142 ms:
  nought open edges, nought pinches, nought facing in, nought missing
  corners, 21,154 m^2 of surface; the three sets decoded and stacked at
  startup, 1024 a side, three layers each.
- The walker, headless in the core, on a 2 km ball: 8 to 10 m in two
  seconds walking and over 14 running, a jump to between 1.0 and 1.6 m
  landing inside 1.3 s, a 0.4 m kerb climbed, a 1.2 m wall stopping the
  body 35 cm short, a diagonal walk sliding over 4 m along it, and a jump
  under a lintel held to under 0.6 m.
- The mockup's join, measured at every fine crossing on it: 622 of 6,542
  with the fine surface over a centimetre above the coarse, 139 with the
  coarse above the fine, by up to 7.5 cm; a pad on a slope makes those 850,
  236 and 16 cm. A skirt hides the first and cannot close the second.
- The cube sphere's corner to centre cell area ratio: 1.42 warped, 5.2
  plain, on a 16 by 16 grid.
- The marching cubes mockup: a 64 m planet with a sea and three towns on a
  112^3 lattice of 1.33 m cells, 95,200 triangles of ground marched in 0.9 s
  in Chromium on swiftshader, the towns planned in 52 ms and their twenty
  buildings built from the kit in 76 ms (25,080 triangles with the floors,
  stairs and lamps; 59 lamps, the port's 23 as point lights). The hex mockup
  on the same seed: 163,842 tiles at 0.53 m, 153,009 walls, 1,363,296
  triangles, 2,876 of the tiles built (56 door tiles, 636 steps, 59 lamps),
  in about 6.6 s of which the plan (a first crossing per tile, then the
  runs) is 2.5 to 2.9 and the mesh 0.8 to 2.4. Fourteen times the triangles
  for the same ground, which is the cost of a terrace: a flat plain is cheap
  and every step is a wall, and a room is walls all the way round.
- The walker, both pages, headless: 7 m up the port's main street to the
  first face, 19.4 m along it, a jump to 1.4 m, the same to a few
  decimetres; then in at a door, up two flights to floor two, 6.04 m over
  the base on the blocks; and on the marched page, in at the house's door,
  up the ramp to floor one (3.39 m over the base) and the second to floor
  two (6.38 m), the page naming the recipe, the building and the floor at
  every stage, and never a fall through the opening over either ramp.
- The towns in the field: 284 structures (twenty buildings, the streets in
  pieces) in 33,466 coarse cells replaced by 0.22 m ones over 929 chunks,
  dual contoured from 17.4 million samples into 885,862 triangles in 8.2 to
  9.9 s over three runs
  (marched, the same cells were 11.5 million samples, 908,807 triangles and
  5.5 s: the crossings are bisected now, and a quad per crossing edge is
  about as many triangles as marching cubes' one to five per cell), against
  84,174 triangles and 0.9 s for the rest of the planet. The join between
  the lattices: 9,404 vertices on it, 5 mm apart on average, 108 mm at the
  worst. A sculpted slab remarches the eight chunks it touches or borders
  in 335 ms and a cut doorway its eight in 545 ms, and the walker steps up
  onto the slab (49 cm) or walks through the doorway into the house, the
  same frame; taking both back is 859 ms. (A chunk's neighbours are remeshed
  with it since border edges have one owner, which is why a slab that
  touched one chunk now touches eight.) The fine mesh with its vertices welded by position: edges
  shared by more than two triangles, which is where a quad is twisted
  through a cell holding two surfaces, were 905 with one vertex a cell and
  are 11 with one a surface, on 898,432 triangles, since a quad is one per
  crossing edge either way; open edges off the skirt's rim, which are
  holes, 7,093 before the chunks agreed on a shell cell's normal, 655
  before a border edge had one owner, nought since, and one triangle in
  898,432 facing into the rock, before and after a house is built. The
  towns load in 13.0 s now against 9.9, the gradient at the middle of every
  half quad being most of the difference.
- The builder, headless: a 6 m pad of flat ground remarches 48 chunks in
  1.4 s, a wall from two clicks 1.3 s, a room cut, a door and a lamp about
  0.3 s each, and the walker walks up onto the pad and in through the door
  the same frame; the export is one recipe of nine brushes at the site.
- Material Maker under lavapipe: seven graphs, twenty eight maps at 2048,
  exported in about six minutes on four cores, and byte identical on a
  re-export of the same graphs on the same machine (five sets unchanged when
  two were added), so `--check`'s half a percent is slack for another driver
  and not for this one. Handed a single graph the command line stalls, and
  two of the new graphs' first settings (a voronoi with a stretch, a bricks
  with no rounding) crashed it at load; every graph is on settings the
  working ones already use, and the bake is always the whole set.
- Tenebris, for scale: 300 m planets, 163,842 hex tiles a body, 21 MB of
  voxels and a 2.7 s full remesh, one flat 5 km detail cutoff and an impostor
  past it. Every number in that project's LOD is a number this one replaces,
  and every RULE in it is one this one keeps.
