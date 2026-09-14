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
the eye is near enough to see, meshed by marching cubes, because a height map
cannot overhang and a column of hex prisms can only fake it.

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
| terrain | density field plus marching cubes in the core, chunked on a cube sphere quadtree | overhangs, caves, arches and craters with lips; the alternative is tenebris's Goldberg hex prisms, and `docs/mockups` is where the two are compared on the same seed before the call is final |
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
anywhere and nothing of any size may point in.

**Still to build, and the order it comes in:**

1. **The chunk streamer.** `select` gives a leaf set; the app diffs it against
   last frame's, samples and marches new leaves on a worker, and despawns the
   rest with hysteresis (tenebris held a chunk to one and a half times its
   load distance for three seconds, because a full reload was a two second
   stall and the boundary flaps). One cached verdict per body per frame that
   every pass reads, so terrain, water and the impostor cannot disagree mid
   flap. LOD is anchored to the PLAYER, not the camera: a map view that pulls
   the eye to twenty kilometres must not unload the ground under the ship.
2. **Seams between levels.** A finer leaf beside a coarser one leaves a crack
   along the shared edge. The answer is the mesher's, not the streamer's:
   a skirt per chunk first (cheap, invisible from above, the thing every
   shipped voxel planet does), transvoxel if the skirts ever show.
3. **The impostor tier.** Past the streaming radius a body is tenebris's
   baked equirect on an icosphere, lit per body from its own star.
4. **Materials on the field.** Triplanar, blended by slope and height off the
   Material Maker sets, which `docs/mockups/common.js` already does in GLSL
   and the WGSL transcribes.

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
| a building | a block kit: slab, panels, panes, pillars, parapet or gable, a doorway, a floor a storey, a flight of stairs, a lamp, placed free on the lot, each building plumb on its own patch of the sphere | the lot's tiles raised by the storeys in half metre blocks and tagged concrete, windows lit by the shader; the doorway, the floors, the stairs and the lamps are RUNS of blocks in a column, nothing placed and nothing to align |
| a wall on foot | a box the body is pushed out of along the face it came in least by | any tile round the body standing higher than a step, pushed off along the line from its middle |
| a room | the sun casts a shadow map over the port, so a room is dark because the walls are between it and the sky, and a point light per lamp warms it | a face toward the inside is TAGGED inside and the shader lights it from the building's lamps and nothing else, with the sun ignored |
| the concrete | box UVs in metres | UVs in the TOWN's frame: a cap on east and north, a wall along the town axis it faces across and up from its base, so the panel seams meet the blocks and the floors |

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
cargo test -p freeport_core                       # 22, the core
python3 tools/shape.py --check                    # no file over 900 lines, no function over 100
cargo fmt --all -- --check                        # the format
cargo clippy -p freeport_core -- -D warnings      # the core's lints
tools/bake_materials.sh --check                   # the maps match their graphs
python3 tools/pngdiff.py before.png after.png     # a refactor's pictures, against the scene's own floor
cargo build --release -p freeport_app             # the harness (needs libwayland-dev libxkbcommon-dev libudev-dev libasound2-dev on Linux)
./target/release/freeport_app                     # a window: one marched planetoid, a light, a camera
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

- `freeport_core`: 22 tests in 80 ms. A 6 m sphere on a 32^3 lattice at half
  a metre marches to 5,288 triangles, a closed shell within 3% of the
  sphere's area.
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
  decimetres; then in at a door, up two flights to floor two, 6.35 m over
  the base on the kit and 6.04 on the blocks.
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
