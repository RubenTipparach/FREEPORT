# freeport

Flight performance update: the finest terrain cell is 0.5 m, with ten levels
retaining the previous outer streaming extent. Planning runs off-thread, noise
has an exact AVX2 path, and GPU readback uses nonblocking polling so its worker
cannot hold rendering's device locks across a wait. See `docs/flight-performance.md`
for current measurements, diagnostics and the limits of this optimization pass.

Current implementation update: terrain sampling is now batched in a compute
shader, with f64 crossings and dual-contouring LOD seams retained on CPU. Mesh
conversion runs on workers and asset installation has a real frame budget. Rings
adapt to altitude and wait for replacement coverage when changing levels.
Buildings are authored by headless Blender with editable Exact Boolean cutters,
then baked into static meshes, collision boxes and three visual LODs, and past
those a building is a solid block drawn per city block (see "A town is its
BLOCKS" below). Windows
have actual openings and transparent glazing. The parametric source is
`assets/config/buildings.json`; `tools/bake_buildings.py` regenerates the library.
`docs/buildings-and-streaming.md` describes this workflow and supersedes the older
runtime building-generation and streamer timeout descriptions below.

The playable system is FREEPORT and nothing else. `assets/config/planets.json`
still exists and ships EMPTY: it carried Ember, Pelagos and Rime, and they were
copies, for a reason that is a field nothing reads (the section on the planets
below). Flight uses a wheel-selected cruise speed, a smooth atmospheric limit,
and continuous collision against each body's density field.
`docs/flight-and-planets.md` documents controls, body-local terrain streaming,
stale-job rejection and the current coarse distant-body rendering limitation.

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
the lattice at every level and the rings of chunks round an eye, the cube
sphere and its quadtree, density fields, the mesher and its audit, the sea,
towns and the parametric models their buildings and streets are made of,
the walker, and everything that comes after them (orbits, the economy, the
star map, the damage model).
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
| floating origin | our own `pos::Origin` in the core, applied by hand in the app (`rebase_origin` in `stream.rs`) | the RULE (f64 world, f32 render frame, rebase past a radius, snap to a grid) lives in the core where a test holds it; the app has one system that moves anything, every chunk, every lamp and the planet's centre in both materials, and that was small enough that `big_space` (evaluated at 0.12) would have been a dependency for one function |
| terrain | a density field dual contoured in the core (`dc.rs`), a chunk at a time on one lattice at every level (`lattice.rs`), the chunks in rings round the eye and streamed by the app (`stream.rs`) | overhangs, caves, arches and craters with lips, and a corner that is a corner wherever something is built; marching cubes (`march.rs`) is the reference the surface table is derived from, tenebris's Goldberg hex prisms the alternative, and `docs/mockups` is where the two were compared on the same seed; the cube sphere quadtree (`sphere.rs`) is kept for the far tier, where a planet is patches on a sphere and not cells in a lattice |
| water | a sea LEVEL in the core (`water.rs`), its surface contoured by the same mesher and clipped per triangle by what is under it, tenebris's water shader ported to WGSL on Bevy's own transmission | a surface is what the owner asked for now and a volume pinched: two surfaces contoured in one cell put two vertices a rounding apart along every shoreline. Voxel water comes back with the thing that digs |
| buildings and streets | parametric MODELS in the core (`model.rs`), triangles built from numbers, beside the field rather than in it | the owner's ask, and it leaves one lattice carrying terrain: no second lattice under a town, no massing rule, no chunk tested against every structure. A wall is one oriented box that is both drawn and collided, so the picture and the collider are still one set of numbers |
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
  reason to raise the limit. It counts braces from a `fn` line, so a brace in
  a byte or a string literal is a constant (`json.rs` writes `OPEN` and
  `CLOSE`) and never a literal, or the count is wrong for the rest of the
  file.
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
- **A ROAD GENERATION QUESTION GOES TO `docs/civil-engineering.md`
  FIRST.** How tight a bend may be, how steep a grade, how wide a lane
  or a shoulder or a verge, how steep a batter, how high an embankment:
  every one of those is a real discipline's number with a formula behind
  it, and that file carries the formula, the table, what this world
  picks and every place this world deliberately departs. A constant in
  `road.rs` or `road/ribbon.rs` that is not traceable to a row in it is
  a number somebody guessed. The owner's own instruction, and it holds
  until the owner says otherwise.
- **Measure, then decide.** Numbers in the commit message, a clock rather
  than the engine's delta, and never "faster" without a before and an after.
  A frame time is a `std::time::Instant` and never Bevy's `Time`, whose delta
  is clamped and lies exactly when it matters.
- **Before a push**: `/simplify` on the diff for reuse and altitude,
  `/code-review` for correctness, then the suites (`/tidy` runs them).
- **A session never RE-ARMS itself.** No scheduled check-in, no timer, no
  routine that wakes the session up to look at a pull request again, and
  nothing that arms its own successor. A push and the pull request are the
  handover, and after them the work is the owner's to look at; a loop that
  wakes every hour to report that nothing changed is noise that costs
  tokens and says nothing. If something really has to be watched, it is
  watched by whatever GitHub already sends, and the session ends its turn.
  This is the owner's own instruction and it holds until the owner says
  otherwise.

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
- **The DEPTH buffer needed neither, and that was measured rather than
  assumed.** Bevy's perspective is INFINITE REVERSE Z: the near plane maps to
  one, infinity to nought, and an `f32`'s mantissa is dense exactly where the
  precision is wanted. At a near plane of a tenth of a metre the step at ten
  metres is a hundredth of a millimetre and at a thousand kilometres it is
  under a tenth of a metre, which is finer than anything this world draws
  there. A logarithmic depth buffer is what a program with a FINITE far plane
  needs and this one has none, so the owner's ask for one is answered by
  saying which problem it would have solved. What a thousand kilometres
  actually costs is the VERTEX, not the depth, and the next rule is that.
- **A vertex is an OFFSET from an anchor, so the shader never forms a planet
  scale unit vector.** `pos::unit_offset(anchor, step)` is
  `normalize(anchor + step) - anchor` in closed form: `s = 2 a.step +
  step.step` and then `anchor * (-s / (root (1 + root))) + step / root`, which
  is algebraically the same thing and never differences two numbers near one.
  The naive form in `f32` loses a vertex 6.5 cm at a thousand kilometres
  (`a_small_step_keeps_its_metres_at_a_thousand_kilometres` prints both: 0.0650 m
  naive against 0.000018 m stable), which is a hex tile visibly out of place
  under the feet. Every tier's vertex is built this way now: one anchor
  differenced in `f64` on the CPU, and a small offset added in `f32` on the
  GPU. The same rule sends Planet-LOD's leaves as offsets from the anchor
  rather than as directions.

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

**What streams is decided here, and what is still to build:**

1. **The chunk streamer is built**, and the section on the rings below is
   the whole of it: `select` and the quadtree are not what it streams. A
   planet's near field is one lattice at every level and the chunks round
   the eye are boxes at each level, one inside the next, so a chunk beside
   a chunk one level up joins it by the rule `dc.rs` applies between any
   two levels: the seam is polygons whose corners are cells of both sizes,
   and nothing is skirted. The quadtree stays for the far tier, where a
   planet is patches on a sphere and not cells in a lattice, and `select`'s
   nearest point rule is the one that tier will need.
2. **The impostor tier is BUILT**, and the section on the chart below is
   the whole of it: past the rings a body is a displaced icosphere
   painted from an equirectangular chart baked off this same field.
3. **The rest of the materials.** The sets are on the field in the harness
   (the section on the sets below), concrete is in the town's frame, the
   array textures carry their mip chains and the sand band is on the shore;
   what is left is a second rock by latitude and ice at the poles.
4. **Floors and stairs in a building**, which the brushes had and the
   models do not: a building is a shell with a doorway and one room to the
   roof, and a slab a storey with a flight up it is geometry nobody has
   written yet.

## A planet is a FEW terms, and one fractal is one hillside

`biome.rs` is what the ground is made of at the size of a WORLD. A planet
whose surface is a single `fbm3` has lumps of one size everywhere, so it
reads as the same hillside from pole to pole however far you walk, and
that is what this world was. Real ground is a few terms of very different
characters laid over each other, and the variety is in the composition
rather than in any one of them:

| the term | what it decides | its size |
| --- | --- | --- |
| continent | where the sea is, and so every coast and island | 0.30 of the planet's lumps, three octaves, through a SHELF |
| belt | whether a range is pushed up through a continent | 0.44, four octaves, a smoothstep between 0.85 and 1.75 standard deviations of its own noise |
| ridge | the range itself, RIDGED so it is crests with flanks | 1.15, five octaves, the fold raised to 1.6 |
| hills | the ground under a walker's feet | 3.1, and the planet's OWN octave count |
| channel | a network cut down through whatever stands over the sea | 2.4, five octaves, a gorge inside a broad valley |

**A term written as a share of the relief and handed a RAW fbm is worth
about a fifth of what it says.** Measured over twenty thousand
directions (`measure_the_fbm_spread` prints it): a sum of halving octaves
is a sum of many small numbers, so it piles up near its middle, and
`fbm3`'s mean is 0.498 with a standard deviation of 0.106 whatever the
octave count past four, 99 in 100 samples inside 0.262 to 0.736. That is
why the first cut of this was eight kilometres of relief that never left
plus or minus 1,615 m. Everything is stretched by the MEASURED spread
first (`signed`), and the same planet then spans -3,554 to 4,158 m.

**Three limits, each found by a test rather than reasoned about:**

- **A gorge is capped by the body's own relief.** It was written in
  absolute metres, which is right for a river and wrong for a planetoid:
  a hundred metre test ball with four metres of relief was handed a
  thirty four metre gorge, most of the way to its own centre, and the
  mesher pinched on the wall of it. An absolute number is a number that
  is wrong on some body, so it is a number with a share beside it.
- **Every term is capped by the PLANET's octave count.** `octaves` is
  what ties a body's detail to its size (eighteen at a thousand
  kilometres, three on a twenty metre ball), and a term at a flat five
  octaves puts features under the lattice's own cell on anything small.
- **And nothing is steeper than the mesher can close.** `MAX_SLOPE` is
  20 and every term is scaled back to fit it, measured against the mesher
  rather than chosen: the dual contoured seam closes at a slope bound of
  18.7 (the rough test ball as it was) and does not at 94.7 (the same
  ball stretched), which came back as 32 open edges, 81 pinches and 1,192
  triangles facing in. The thousand kilometre planet asks for 9.95 and is
  untouched; it is the balls with a fifth of their radius in relief that
  this catches.

**Climate is two numbers and everything else is derived.** `temp` is the
latitude warped by noise less what altitude takes off it, and `wet` is
its own noise, drier in the middle of a continent and wetter in the low
ground and along a channel. A `Kind` follows from the pair with the
height over the sea and the slope: ocean, ice, beach, desert, savanna,
grass, forest, marsh, rock, snow and city.

**The lapse rate is in METRES and that was the difference between a
planet and a snowball.** It was a share of the RELIEF, so a body with
eight kilometres of it lost most of a unit of temperature over its own
mountains, and the first chart came back white from the poles to the
tropics with no desert anywhere on it. Altitude is not a share of
anything: Earth's lapse rate is about 6.5 degrees a kilometre and this
range spans about sixty, so it is 6,100 m to a unit, which puts the
equator's snow line near five thousand metres and a temperate mountain's
near two and a half thousand. A planet with five hundred metres of relief
then has snow by LATITUDE alone, which is also right.

**And the body is MOSTLY WATER, which is a percentile and not a
number.** A sea level means nothing on its own: it is where it falls in
the body's own height distribution. This planet's relief spans -2,920 to
4,358 m with its median at +351, so a sea 400 m UNDER the mean radius
left the world 26.3% water, which is a continent with lakes in it. At
+1,000 m it is 62.1%, an ocean with continents in it, and
`the_harness_planet_is_mostly_water` holds the half the owner asked for
rather than the constant that happens to give it.

Of 8,000 directions this planet grows 4,239 ocean, 1,709 forest, 719
ice, 452 grass, 392 snow, 313 desert, 122 savanna, 49 marsh and 5 beach.

## A continent needs a SHELF, or the hills decide where the sea is

`shelf` is the term that turns this body from three hundred middling
blobs into a map. The owner asked for the land to group into six or
seven continents with a lot of small islands off them, and no setting of
the continent term's frequency, octaves or amplitude gave one: the count
of pieces over a per cent of the body moved between one and ten and they
were all the same size.

**The reason is a gradient, and it is measurable.** A continent term
that is a smooth `fbm` swings 2,480 m over about 220 km here, which is
22 m a kilometre; the HILLS riding it swing 440 m over 13 km, which is
34. The hills are steeper than the continents, so everywhere the swell
came within a few hundred metres of the sea it was the hills that
decided land from water, and on a body whose sea sits near the middle of
the swell that is most of it. There was no coastLINE anywhere on the
planet, only a wide band of noise, and every "continent" was whatever
that noise happened to join up.

**Real crust is bimodal and that is the fix.** An abyssal plain, a
continental plateau, and a steep margin a few tens of kilometres wide
between them: a `smoothstep` across the stretched continent noise is
exactly that shape. Inside the margin the ground falls faster than the
hills can lift it, so the shore FOLLOWS the shelf and the hills only
fray it, which is what a coastline with bays, spits and islands off it
is made of. `SHELF_SHARE` is how much of the term the step takes, and it
is well under one on purpose: at one a plateau is dead flat and a
continent is a mesa, so the rest is left as the swell it was and is what
gives a continent an interior.

Measured on the same body before and after: 6 pieces over a per cent of
it, the biggest 15.3%, with 331 islands, against 7 continents of 17.9,
5.3, 4.1, 2.2, 2.1, 2.0 and 1.2% with 217 islands.

## How BIG a continent is, is the swell's own size, and the count is arithmetic

The shelf decided that there ARE continents. What decides how big one is
is `freq::CONTINENT`, the size of the swell the shelf cuts, and the owner
read the answer off the chart: the land masses were too small. Seven
pieces of 17.9, 5.3, 4.1, 2.2, 2.1, 2.0 and 1.2% is one continent and six
scraps, and the chart of it is a lace of middling blobs with no open
ocean anywhere.

It is 0.18 of the planet's lumps now rather than 0.30, with the sea 100 m
higher to hold the water share: **57.8% water in 3 continents of 17.9,
11.0 and 10.1% with 381 islands**. The biggest is the same and the second
is TWICE what it was, the third two and a half times, and there are 75%
more islands. A chunk of ocean you could sail across for a day is the
thing that appeared, and it is what a continent needs to be a continent.

**The COUNT falls when the size rises, and that is arithmetic rather than
a setting anybody can have both ways.** Land is a level set of a fractal,
so a swell twice as wide crosses the sea half as often; the sweep
(`measure_the_land_at_each_sea_level`) says it plainly, and no setting in
it gives seven pieces that are each big. The owner asked for six or seven
earlier and for bigger now, and bigger is the one that was taken, because
it is the newer word and because the earlier one was reaching for the same
thing: a map with real places on it.

**EARTH is the scale to read that at.** Its seven named continents are
four contiguous land masses of 16.6, 8.2, 2.7 and 1.5% of the globe at
71% water. This body's three are each bigger than any of them and it is
58% water, so what the count went down to is a MORE continental world than
Earth rather than a less divided one, and "three continents" and "seven
continents" are the same kind of statement about the same kind of planet.

**A town's level is tested against the window now, and it was not.**
`plan` tests the CANDIDATE's own direction against the habitable window
and `settle` then gives the site the LOWEST of forty nine marches round
it, which is a different number by up to the `LEVEL` fall the site just
passed: on this world's first run a town came out 2.96 m over the sea
where the floor is 3, and nothing said so. What that is on the way to is a
town accepted on a shore whose settled level falls UNDER the sea, and what
would be built there is a levelled plateau with water over it. `settle`
asks `window` itself and refuses, rather than being handed a floor by two
callers one of which would get it wrong.

**Three fixtures moved with the shape, and each is worth the line.** The
road test's little planet carried 900 m of relief on a 40 km radius, which
is nearly three times what this world carries, so its roads ran over
ground too rough for a village sized site to be level anywhere: it grew
two villages by luck before and nought after, and at 400 m of relief it
grows ten. Its village size assertion was wrong in principle and passed
for the same reason, holding a village smaller than the SMALLEST city when
size is how near the sea a place stands: a village on a beach is 0.42 of a
coastal size where a market town up a valley is the 0.32 floor, so the
village is legitimately the bigger, and what `WAYSIDE` actually promises
is that a village is smaller than a CITY ON ITS OWN GROUND. And the chart
test read the water mask back through a `> 8` of its own against an
`over_sea < -FULL_DEPTH * 0.03`, which are -7.3 m and -6.6 m of water; it
agreed for as long as no sampled texel fell in the 0.7 m between them, and
it compares the chart's byte with the byte the field encodes now.

**And how many continents there are is a fact about how much LAND there
is.** Land is a level set of a fractal, so past about four tenths of the
body it PERCOLATES and the seven become one: at a sea of +700 m this
world is 54.6% water and nearly all of its land is a single mass, and at
+1,300 m it is 72.8% water and the biggest piece left is 4.7% of the
body, with 415 islands off it. 37.8% land is where they are seven,
which is why the sea and
the shelf were picked together off one sweep
(`measure_the_land_at_each_sea_level`) rather than one at a time.

**Nothing is BUILT on frozen ground.** `Climate::frozen` is the one
place the freezing line is read, so the ice a chart paints, the ground a
town may stand on and the ground a road may cross are one answer: no
city on a glacier, and no road over one. A latitude would be a second
number to get wrong and would still allow a city on an equatorial ice
cap. Measured on the chart: every one of the 160 cities and every road
texel is inside 65 degrees of the equator, where the caps run from about
70.

**`sampling.wgsl` never compiled, and nothing said so.** `signed` is a
RESERVED KEYWORD in WGSL, so a shader declaring `fn signed` is refused
outright at `create_shader_module` and this whole compute sampler had
been dead since it was written; the core's function is `signed` and the
transcription is `stretch` now. What hid it is that the sampler is
optional (`Compute(enabled.then(...))`) and the CPU path is what every
picture was drawn from, so the only thing that ever saw the error was
`gpu_preserves_cpu_signs_and_lod_seams`, which is `#[ignore]`d because it
needs a device. An ignored test is a test nobody runs. Working, it agrees
with the CPU to identical mesh geometry and does 74,088 samples in 13.97
ms against the CPU's 37.82 on lavapipe.

**`sampling.wgsl` transcribes `landform` and `cut`, and the GPU is why
the split is where it is.** Terrain sampling is a compute pass, so a
shape change that landed only on the CPU would be ground the walker
stands on and the mesher never drew. The shader carries the SHAPE of the
function and every constant in it arrives in a uniform (`biome::Gpu`), so
a threshold cannot be tuned on one side and left stale on the other. The
landform and the cut are evaluated in full, because they are few octaves
each and there is nothing worth stopping early for; the HILLS keep the
sampler's own octave interval, and `soft` is monotone, so an interval
on the partial sum is still an interval on the height.

## The hills are never CLIPPED, and the GPU cuts where the CPU does

The owner looked at the map and asked why the port was all flat. It was,
and it was not the port's own levelling: seven kilometres of plain round
it stood DEAD flat, with a knife edge where the relief switched back on
and a coast running as one straight line. The hills term there read
exactly -440.00 m across the middle four kilometres.

**`signed` CLAMPS, and past the clamp every octave of the hills goes
together.** A stretch to minus one through one is right for a term that
goes through a smoothstep of its own afterwards, the shelf and the belt,
and wrong for the hills, which are the finest term and the ground under a
walker's feet: on the top and bottom hundredth of the body the whole sum
is pinned at its amplitude, which is a plateau or a basin with no detail
in it at all. The comment on `FBM_REACH` called that no part of a picture
anybody looks at twice. The port is the LOWEST ground on the body
(`town::in_order`), so it stands in one of those basins by construction,
and it is the one place everybody looks at: measured, the port's basin
runs 23 to 32% past the clamp (a stretched value of -1.23 to -1.32), and
2.00% of the whole body is pinned.

**`biome::soft` is the hills' own stretch**: the identity out to `KNEE`
(0.75), then `KNEE + (CEILING - KNEE) u / (1 + u)`, which leaves the
identity with the same slope and eases toward `CEILING` (1.3) without
reaching it, so the deepest basin keeps a share of its small hills rather
than none. A rational and not a `tanh`, because the field is add,
multiply, divide and compare, bit for bit the same on every client.
`band` carries the ceiling; the slope bound needs nothing, because the
curve never rises faster than the stretch. Only the hills take it: the
continent and the belt still clamp, since their flat ends are the abyssal
plain and the plateau and a smoothstep follows them anyway.

Measured on the harness body, before and after: **2.00% of the body
pinned at the clamp against nought; the port's middle four kilometres
0.00 m of hills against 23.01 m; the water 57.85% against 57.84%; and
land that rises under five centimetres over twenty metres 4.81% against
4.60%**. `the_hills_are_never_clipped_flat` holds the curve's own shape
and finds nought of 20,000 directions pinned, against 391 on the clamp.
It moved the bare ground, so the ATLAS was re-baked:
**786 settlements and 369 roads over 55,452 km joining 157 of the 160
cities, planned in 472.7 s**, against 832, 365, 60,651 km and 153. The
port is the same town on the same site, 6 km from the sea.

**And the GPU sampler never took the min that makes a town CUT.**
`Planet::surface` is `min(graded, bare)` with a road's `fill` blended
over it, which is this file's rule that a city flattens ground and never
adds any; `sampling.wgsl` added `bias + bare * keep` and never took the
min, and skipped the relief outright wherever `keep` was nought. So on a
real GPU, where the compute sampler is on by default, a town filled every
dip in its plateau and stood on its old pedestal down its downhill skirt,
drawn where the walker and the car, which collide against the CPU's
field, found nothing: ground drawn in the air. It never showed here,
because lavapipe samples on the CPU, and
`gpu_preserves_cpu_signs_and_lod_seams` is ignored for want of a device
and passed by luck, since no dip fell in its chunks. The new hills put
one there: the site's level 1392.82 m against the bare 1390.77, the GPU
calling rock (1.81) where the CPU has air (-0.25). The shader
transcribes the rule now (`levelled`, which rises in the bare ground
whatever the site does, so the octave loop's interval still bounds it),
a point carries `[bias, 1 - fill]` in a fifth lane, and the relief is
skipped only on a road's own flat. Measured on lavapipe: identical signs
and mesh geometry over 74,088 samples, 22.75 ms against the CPU's 75.18.

## A planet from orbit is a CHART, not a colour a vertex

`chart.rs` bakes two equirectangular pictures of a body off the same
field and the same biome rules the chunks are contoured from, and
`distant.rs` draws them. It is tenebris's answer ported
(`build_distant_textures_for_body` and `distant.glsl`).

What it replaces was one colour a VERTEX on a smooth icosphere sitting at
the BOTTOM of the relief band, four kilometres under the lowest ground.
On a 46,000 triangle sphere that is a colour every thirty kilometres:
continents came out as soft blobs with no coast anywhere on them, and
from thirty kilometres up the picture was a green smear on a blue ball
with the streamed chunks floating somewhere over it. The join between the
two is the thing the owner could see.

- **The albedo** is the biome's colour with the water blended over the
  shallows and a height shade on top, and its ALPHA is the water mask the
  shader gates a sun glint on, so an ocean catches the light where the
  land beside it stays matte.
- **The slope map** is the drawn surface's own gradient across a texel,
  which is what puts a mountain range on a sphere no mesh at that size
  could hold one on. It is normalised to the body's own 99th percentile,
  MEASURED rather than set: a fixed number came out flat, a hundredth of
  the relief across a kilometre of texel encoding as 7 of a possible 127,
  and the whole world shading as a smooth ball.
- **The sea is FLAT, because the sea is what is drawn.** The slope came
  off the raw altitude, which runs NEGATIVE under an ocean, so every
  ocean was shaded with the relief of its own sea BED: 13,844 sea texels
  at a mean bend of 45 of 127 and a worst of 128, against the land's own
  50. `distant::sphere` displaces its vertices to `ground.max(sea)`, so
  that is the only surface there is to have a slope, and the map and the
  mesh disagreed everywhere the ground fell under the water. The owner
  saw it as water with normals on it.
- **A slope leans its normal AWAY from what it climbs**, on BOTH axes.
  The shader built a tangent frame and ADDED the north term: on the
  steepest northward texel of the test planet the normal came out 0.41
  along north where it had to be negative, so every body was lit from
  the wrong side along one axis and a ridge read as a gully. It is
  -0.42 now, which is the same number the other way up. The frame is
  the CHART's own too, east where u grows and north where v shrinks,
  built from the derivative of `pixel_dir`: what it replaces swapped its
  reference axis wherever `|y| >= 0.9` to keep a cross product well
  conditioned, which is 64 degrees of latitude, and everything poleward
  of that was shaded in a frame that was not east and north at all with
  a hard ring where it switched.
- **A difference is not a SLOPE until it is divided by its own run**,
  and on an equirectangular chart that run is a function of latitude. A
  raw east difference understates the slope by the cosine of the
  latitude: the polar band measured 0.121 where the true gradient is
  0.297, two and a half times too flat, while the equator's 0.146 and
  0.148 agreed. It is the same number the steepness handed to `Kind` is
  read through.
- **And both axes are measured over the same GROUND**, which is what
  `east_span` is for. Dividing by the run alone swaps one bias for
  another: at eighty degrees two neighbouring texels are a kilometre
  apart where the rows above and below are still six, ground is
  fractal, so the shorter baseline is the steeper gradient and the
  polar band read 0.203 against the equator's 0.103 with all of it in
  east. That drew as horizontal streaks across both ice caps, which a
  picture found and no number had. Reaching `1 / cos(latitude)` texels
  holds the run at about one texel of ground at every latitude.
- **A body from orbit is DIFFUSE, and the sun's own GLINT is a term of
  its own.** The water was a smooth, reflective PBR material
  (`perceptual_roughness` 0.12, `reflectance` 0.35) taken through Bevy's
  whole lighting path, and that path includes the camera's ENVIRONMENT
  MAP: at that roughness over an ocean the size of a hemisphere it is a
  mirror the size of a hemisphere, and what it mirrors is the sky cubemap
  `sky::bake_env` baked for wherever the eye last was. The owner read it
  off a picture of a night side as a broad pale sheen swept across the
  whole disk and named what is wrong with it: **a planet's water does not
  carry a reflection map.** There is nothing out there for an ocean to
  reflect except the sun.
  So F0 is nought and the roughness is one, which takes every specular
  term in `apply_pbr_lighting` to zero, the lights' and the
  environment's alike, and the only shine left on the body is a Blinn
  Phong lobe about the half vector between the eye and the sun, gated on
  the chart's own water mask and on the day side. It is WIDE, a power of
  90 rather than of thousands, because the sun's own half degree is
  spread by the waves: a glint on an ocean from orbit is a soft patch
  about seventeen degrees across and not a point. And it is added BEFORE
  `main_pass_post_lighting_processing`, so it is in the same linear HDR
  the lighting is in and the tone mapper sees it, and scaled by
  `view.exposure` for the same reason the fog is: `distant.sea.x` is in
  nits and the frame is not.
- **The albedo is written sRGB.** `Kind::colour` is linear, because that
  is what a shader does arithmetic in, and the chart binds as an sRGB
  texture, because most of a planet is dark and that is where sRGB spends
  its bytes. Writing the linear value into an sRGB texture decodes it a
  SECOND time on the way out: a forest at 0.10 came back at 0.0095 and
  the whole planet drew nearly black.
- **The sphere is DISPLACED and sunk a hundredth of the relief**, 80 m on
  this planet, so the streamed chunks always win the depth test and the
  join is under a thousandth of the radius rather than four kilometres.
- **Three levels picked by distance**, 24, 48 and 79 subdivisions. 79 is
  Bevy's own icosphere cap, measured by asking for 144 and getting
  `TooManyVertices` with 210,252 points back. Going past it means writing
  an icosphere here, which buys a finer SILHOUETTE and nothing else,
  since the shading is the map's.
- **`FREEPORT_DUMP_CHARTS=1` writes them out** beside the assets. A
  planet's own texture is a picture, and a picture nobody can open is a
  buffer nobody can check.

## A CLIMB is what finds a LOD cliff, and a mark has to be able to go away

The owner asked to fly up a kilometre at a time and see a seamless
transition, with the roads on the chart the width of the roads on the
ground and a city's outline the city's. The climb is `--over N`, which
stands N metres over the port and looks straight down, and it found three
things, none of which any number in this file had.

**THE HAND OVER WAS A CLIFF, and it was twenty four fold.** At ten levels
a ring reaches 16.4 km from the eye and its coarsest cell is 256 m; the
chart's texel was 6,136 m. From 33 km up the picture was a sharp 32 km
ISLAND of terrain floating over a blur, and from 49 km the blur was its
own texels at a hundred and nine pixels each with the island shrinking
in the middle of them. Nothing in between existed.

**Fourteen levels is what closes it.** A level's box doubles, so four
more take the ring from 16.4 km to 262 km and the coarsest cell from
256 m to 4,096 m, which is within one and a half of a chart texel: a step
an eye cannot find. Measured at 49 km: 282 chunks and 177,897 triangles
against 148 and 54,625, and the frame is terrain to its edges. The floor
of the climb is unchanged, because `Rings::follow` drops the fine levels
by height anyway: at 2 km it is 2,050 chunks, which is what ten levels
built there too.

**The chart POKED THROUGH the terrain, and the sink was never the
problem.** From 150 km up the coarse ground was riddled with pale
patches. The sphere is sunk `SINK` (a hundredth of the relief, 80 m)
under the ground and that is plenty; what was wrong is that it sampled
the ground at its own VERTICES, which at Bevy's icosphere cap stand
13 km apart on this body, and strung a triangle between them. The relief's
HILLS term has a wavelength of 13 km, so the chord aliases it completely:
measured as a curvature, an interpolated chord over 13 km of that term
stands up to 3.3 km ABOVE the ground it spans, which is forty times the
sink.

`distant::floor` is the answer and it is one word: the sphere is displaced
to the LOWEST ground a vertex can see rather than the ground at it, a ring
of eight bearings at the vertex spacing. That makes the impostor an
envelope UNDER the body rather than a surface through it, so it cannot
poke through whatever the terrain does between two of its vertices. What
it costs is the SILHOUETTE, which sits at the local low ground rather than
at the ridge: four kilometres of a thousand, 0.4% of the limb. It is asked
of a BARE planet, which is `Chart::bake_bare`'s own rule and for the same
two reasons: a town is 483 m across against a 13 km vertex, and a planet
carrying 190,000 corridor sites would be walked at every one of the
560,000 samples.

**And the chart was HALF the width its own comment claimed.** The doc said
"a thousand kilometre planet at 2,048 wide is a texel every three
kilometres" and the constant said 1,024. It is 2,048 now: a texel is
3,068 m rather than 6,136, the bake is 3.3 s rather than 1.0, and the
marks on it lie by half as much, since both are measured in TEXELS.

**A MARK HAS TO BE ABLE TO GO AWAY, so it is not painted into the
albedo.** A city is drawn `CITY_TEXELS` (2.2) across and a road
`ROAD_TEXELS` (0.9), which is 6.7 km and 2.8 km of ground against a town
483 m across and a road 6.9 m wide: **14 and 400 times over**. This file
already called that "a LIE about the ground and a deliberate one", and it
is: from orbit a texel is about a pixel and a true width is nothing at
all. It is a lie that SHOWS the moment the streamed chunks draw the same
ground beside it, which is what the owner asked to have fixed.

A colour painted into the albedo cannot be taken back, so `Chart::blot`
paints no colour. It writes how much of a texel a mark COVERS into the
slope map's own spare lane, with the night light beside it, and
`distant.wgsl` composites the two over the biome colour and fades them
out where the chart is being magnified past what it knows. The lane was
filled with 255, which every `max` a mark took lost against, so a bare
body's coverage is nought now.

**And the fade's two numbers are read off the RING rather than chosen.**
The coarsest box reaches 262 km, so looking down a 45 degree frame the
chart first appears beside the streamed ground at 356 km up, where a
texel is 7.5 px: `MARK_FAT` is 6, so there is no altitude at which a fat
mark and the real ground are in one picture. From two and a half radii,
which is what a body from orbit is framed at, a texel is 1.78 px and the
marks have to be whole: `MARK_HONEST` is 1.5. At 30 km a texel is 89 px
and at 5 km it is 533, so the whole of the climb the owner asked about is
mark free and what the chart shows there is the same biome colour the
ground beside it is carrying.

**And the SMALLER derivative is what says how fat a mark is.** A round
mark foreshortened to a pixel one way and ten the other is a ten pixel
streak, and what decides that is the axis whose own pixel covers the
least ground. Taking the LARGER read a limb texel as sub pixel, so the
marks came back along the whole edge of the disk in a picture from 400 km
up where the streamed ground beside them had none: 12.7% of that limb
band moved when it was changed, against 0.03% of the middle, which is
what says the fix landed where the defect was and nowhere else.

**What is MISSING, named rather than hidden.** A city and a road share ONE
grey on the chart now (0.30/0.29/0.27, between `Kind::City`'s and
`Kind::Road`'s), because telling them apart would want a second number in
a lane there is not one of: the obvious one, the light over the coverage,
is not it, since a city's light is scaled by how big the city IS and a
wayside village's share is a road's own 0.34. And a mark still does not
MATCH the ground it stands for, it merely never contradicts it: at the
ranges it survives, the true city is half a pixel and the true road is a
five hundredth of one, so what the chart says is THAT there is a city and
where the roads run, which is what a map says.

**A camera for a picture is SOLVED, and `--sunward N` is that.** Three
renders here were aimed by hand at a planet that turned out to be a
different one in its own night, and a fourth used a shore read off an
`--octaves 6` run in an `--octaves 14` one: octaves change the terrain,
which moves the towns, which moves where the sun stands, so the log's own
numbers only mean anything for the settings that printed them.

## The other planets were COPIES, and the reason is a field nothing reads

`assets/config/planets.json` carried Ember, Pelagos and Rime, each with
its own centre, radius, relief, sea offset, seed and `colour`, and the
owner's picture of the system showed four bodies that were plainly one
body at four sizes: green continents on a blue ocean, every one of them.

**They are copies.** Every body is the same `biome::Shape` with a
different seed and radius, so what differs between two of them is which
noise they happened to roll and nothing about what KIND of world they
are. And the one field in that file that was supposed to tell them
apart does nothing at all: `colour` is written into `DistantMaterial`'s
`palette` uniform by `planet_view::spawn` and `distant.wgsl` never
reads it. A body from orbit is painted from its CHART, and a chart is
baked off `Kind::colour`, which is one table for every world, so a rust
red planet and an ice planet drew the same greens.

So the file ships EMPTY, which is the owner's own word for what to do
with them, and it stays the extension point it always was. A body goes
back into it the day a body can DIFFER, and what that wants is named
rather than guessed: a per body palette the chart is baked THROUGH, in
`chart.rs` beside `Kind::colour`, not a uniform beside a shader that
ignores it. A uniform nothing reads is worse than no uniform, because
it reads as a knob somebody has already turned.

It also takes the body out of the city's own sky, which is the other
half of the same picture: a planet hanging over the end of a street,
drawn from that same chart at a size nothing had measured.

## A town is where a mesher is judged, and the walker is the judge

This is the MOCKUPS' record, and it is what decided the terrain: the game
took the marched side of it and then moved its buildings out of the field
and into models, which is two decisions rather than one and both are
written down where they were made. The two mockups carry the same planet (sixty four metres, a sea half a metre
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

## A building WAS a field too, and what that proved

The marched mockup answered whether a building can be built out of the
same marching cubes as the planet, with the two textures kept apart rather
than blended. It can, and `docs/mockups/marching-cubes.html` still does
it: a recipe of signed distance brushes (`kit.js`) in the building's own
frame, added or cut in list order, evaluated on a lattice six times finer
under each town, dual contoured, meeting the coarse lattice on cell faces.
The game did that too for a while and does not any more: a building is a
MODEL now (the section on the cities below), because one lattice carrying
terrain and geometry beside it is cheaper in every direction than two
lattices and a field with a city in it. What the exercise proved is worth
keeping, because most of it is a rule about ANY mesher:

- **The material is the DEEPEST solid at a sample**, the one whose surface
  is farthest away, asked a hand inside each triangle's middle and handed
  to the triangle FLAT. Every vertex of a triangle carries the value, a
  vertex its triangles disagree on being split, because a flat value is
  read off whichever vertex a driver calls provoking: the first cut leaned
  on the last, which is what the paper says, and the owner's GPU hatched
  every wall along the quads. `dc.rs` still asks that question and the
  vertex colour still carries the answer.
- **Nothing thinner than a cell's DIAGONAL exists.** A plate lies oblique
  to a planet's lattice, and a dual contoured cell holding both faces of
  it puts its one vertex between them: a 0.3 m slab on the 0.22 m lattice
  came out pitted, one pit a coarse cell, every pit a vertex at the slab's
  mid plane. It is the exposed STEP that must beat the cell and not the
  box, so a lamp 0.4 deep sunk 0.25 into its slab hangs 0.15 and smears.
  That rule is what a model does not have to obey, which is a large part
  of why the game has models.
- **A dual contouring mesh is watertight by construction, and the page
  proves its own is**: every crossing edge gets one quad, an edge has one
  owner, a chunk samples two points of margin so two chunks agree on a
  shared vertex to the bit, and each HALF of a quad is wound by the
  field's own gradient at its middle. `checkMesh` reads nought holes and
  one facing triangle in 898,432. Those four rules are `dc.rs`'s and the
  section below is them at every level.
- **A box face is shaded FLAT on its own normal**, because a normal
  interpolated across a corner rounds it over a cell; the game's own
  `to_mesh` keeps that as the crease rule.
- **And the join between two lattices leaks at a lip.** Measured on every
  fine crossing along the mockup's join: 622 of 6,542 have the fine
  surface over a centimetre above the coarse and 139 the coarse above the
  fine, by up to 7.5 cm. A skirt hides the first and cannot close the
  second. That is why the game's levels are one lattice and its seam is a
  polygon rule, which is the next section.

## Every level is one lattice, and a chunk is contoured against the levels round it

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
lattice at every level: a cell at level L is 2^L fine cells a side, a
lattice point at any level is a fine point, and every position is computed
from a FINE index through one function (`point`), so a coarse corner and the
fine point under it are the same bits and two chunks at two levels agree on
every sample they share (`a_coarse_corner_is_the_fine_point_under_it_to_the_bit`).
A chunk (`ChunkId`) is `CH` (sixteen) cells of its own level a side, and the
chunks round an eye are RINGS: a box of `2 * HALF` (eight) chunks a side at
each level, each box inside the next coarser's, snapped to the coarser's
chunks and kept a whole coarser chunk inside it, so no two chunks that touch
differ by more than one level (`a_box_is_a_whole_chunk_inside_the_next`
measures it); a box moves only when the eye is `DRIFT` (one and a quarter)
chunks from its middle, so a boundary does not flap under a walker. The first
cut was one coarse grid with a MASK of subdivided cells, grown until every
coarse cell the fine surface crossed had a vertex; that is a planetoid's
answer and it does not scale to a planet, because the mask is the size of
the planet and every chunk read all of it. There is no mask now, and no
growing: a level is a set of boxes and a chunk reads its own neighbours.

`dc.rs` contours one chunk at a time against the levels round it (`Levels`,
which the rings implement): a MINIMAL edge is an edge of the finest level
round it, and the polygon on a crossing minimal edge joins the vertices of
the LEAVES round it, which are this chunk's cells on one side of a join and
a coarser neighbour's cell on the other. Nothing dives under anything and
nothing is sunk: the seam is polygons whose corners are cells of two sizes,
every mesh edge is shared by exactly two polygons, and the mesh is closed by
construction. The rules that make that hold:

- **A leaf's vertex is a function of the field and the leaf alone**, one per
  surface (the marching cubes case's triangles joined where they share an
  edge, `components`), at the least squares point of that surface's
  crossings, each crossing bisected on the field (`BISECT`, eight halvings)
  and given the field's gradient there, stepped at the EDGE's own scale and
  never the chunk's: stepped by the chunk's cell, a fine edge and the coarse
  edge over it gave one crossing two normals, two chunks solved a shared
  vertex to two places, and the seam leaked open edges. Every chunk that
  needs a leaf's vertex computes it the same way from the same fine points,
  so a shared vertex lands on the same bits from either side and the audit's
  weld finds it once.
- **An edge has one owner.** A chunk skips every edge with a finer chunk's
  cell round it, because that chunk owns it, and among chunks of one level
  the lowest owns an edge they share, which every chunk can tell from the
  levels alone. The finer chunk owns the seam, so the seam's polygons are in
  the mesh with the fine detail and a coarse chunk is never rebuilt for a
  fine neighbour arriving.
- **A coarse cell the fine surface crosses only on a face** has no crossing
  on any edge of its own and so no vertex, and the seam polygons on that
  face would have nothing to end on. It is given one at the least squares
  point of the fine crossings on its faces, which every fine chunk beside it
  computes alike (`seam_vertex`), so the seam closes on it
  (`a_coarse_cell_the_fine_surface_crosses_only_on_a_face_still_closes`).
  The mask was grown for this case before; the vertex is synthesised now.
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
  The rule is that the lattice's corner sits half a fine cell off the half
  metre grid anything built would snap to, so no face of a box in the field
  ever lies on a lattice plane, and
  `a_face_on_a_lattice_plane_pinches_and_half_a_cell_of_offset_does_not`
  holds it. Nothing is in the field but terrain today, so this is a rule
  about the TEST spheres and about whatever puts a box in the field next: a
  sphere whose radius put its surface through lattice points pinched the
  same way, and the test lattices sit half a fine cell off.
- **A chunk with no surface in it is never sampled.** `Density::solid` rules
  a box wholly rock or wholly air, for a planet from the band its surface
  stays in (`Planet::band`) and for a field with boxes in it from their own
  bounds, and where the field cannot rule, `Density::slope` bounds how fast
  it can change, so a few samples across the chunk rule it out anyway
  (`a_box_is_ruled_rock_or_air_only_where_the_band_allows`). An answer must
  hold on the box's closed boundary, because the chunk beside a skipped one
  relies on the shared face having no crossing. That is why the planet was
  2,245 chunks and not the rings' 5,632 when it was ten kilometres across.
  `Density::slope` is what `town::surface_radius` sphere traces on too,
  which is how a march down to the ground stopped costing one step per half
  metre of relief. The bound counts a site's
  skirt too, where the relief blends to the town's level over eleven
  metres: without it a chunk on the apron whose surface fell between two
  samples could be ruled empty and left a hole at the town's edge
  (`the_slope_bound_holds_across_a_sites_skirt` measures the field's
  gradient across one against the bound).
- **`audit.rs` measures what the construction claims.** The chunks welded by
  position (half a millimetre, searching the neighbouring bins too, because
  two chunks place a shared vertex a float's rounding apart and a rounding
  that straddles a bin edge read as four holes), every edge counted, every
  triangle tested against the field's gradient at its middle, and the area
  of what faces in (`facing_area`), because six slivers in the gap under a
  slab's overhang face wherever they like and are nothing, and one triangle
  of any size facing in is a hole. A sphere at one level, a sphere across
  four levels, a sphere the water's level cuts and a planet with a slab and
  a wall built on it all come out with nought open edges and nought
  pinches, and the slab's top is a plane to two millimetres, which is what
  dual contouring is for.

`freeport_app` draws it: vertices are split per triangle for Bevy, and a
corner whose smooth normal disagrees with its triangle's face by more than a
crease takes the face's, so a box is shaded flat on each face, the ground
stays round, and where the ground meets a wall only the corner on the crease
changes. A first cut flattened the whole triangle and the shading jumped
along every crease. The pictures the design page carries of a planetoid with
its coarse vertices sand and its fine ones blue are from the mask's day, and
the seam they show is the same polygon rule.

## The world is streamed in rings of chunks round the eye

`stream.rs` is the streamer. Between completed layout builds the rings follow the eye
(`Rings::follow`; the eye is the walker's or the fly camera's, whichever is
driving) and, when a box moves, the wanted set is recomputed: every chunk
of every level's box that the field cannot rule wholly rock or air, each
with the SIGNATURE of its twenty six neighbours' levels (`Rings::signature`,
two bits each: finer, the same, coarser), which is everything its mesh
depends on besides the field. The ruling is against the GROUND
(`World::ground`), which is the planet and nothing else: a building is a
model beside the field rather than a brush in it, so a chunk under a city
is ruled exactly the way a chunk in the wilderness is and costs the same.
The first cut of the old buildings built one field of every structure on
the planet and asked it about every chunk, which was eight thousand box
tests a chunk for five thousand chunks every time a box moved, a hitch
every five metres of walking; there is nothing to test now.

A wanted chunk that is not ready with that signature is a job, nearest
first and new before rebuilt, contoured on a worker from the same field
and frozen target rings. Initial loading draws progressively. Subsequent
layouts upload hidden and publish together only after every wanted chunk
and changed neighbor seam is ready. Spatial overlap alone cannot prove a
seam matches. There is no linger timeout. Moving catches up after publishing,
and a floating-origin rebase moves hidden staged meshes as well as visible ones.
L enables terrain-only wireframe colored by LOD with a cell-size legend;
K freezes the rings so the camera can inspect fixed joins. `--lod-wire`
starts in that view. Terrain keeps shared field normals at LOD joins;
architectural materials retain the normal crease rule.

- **Draining has time and count budgets.** `assets/config/render.json`
  controls upload milliseconds, maximum chunks per frame, worker count,
  and queued lookahead. Mesh conversion happens on workers; hidden staging
  uses the same upload budget as the first load.
- **A mesh is chunk local and placed through the origin.** A chunk's
  vertices are `f32` metres from the chunk's own `f64` corner, and its
  entity is placed from that corner through `pos::Origin`. `rebase_origin`
  is the one system that moves anything: every chunk, every sheet of sea,
  every lamp, every town's models and the planet's centre in the terrain
  and the water materials, which reason in planet local coordinates from
  it, the tenebris rule, so a rebase is a frame in which nothing on screen
  moves. `Anchored` is what says an entity is placed that way.
- **Nothing dirties the ground any more.** The streamer used to carry a
  generation on every job so a result contoured before an edit was dropped
  when it landed, and a box an edit dirtied so every chunk reaching into it
  was contoured again. The builder is gone, so the world is fixed once it
  is built, and the machinery went with it rather than sitting there for a
  caller that no longer exists.
- **The harness has no flag for which world**, because there is one.
  `freeport_app` is the planet (`RADIUS` 1,000,000 m, the sea 1,000 m OVER
  the mean radius, which is what leaves it 62.1% water, eleven levels of
  0.25 m to 256 m cells), 521 settlements of which the nearest eight are
  BUILT (`TOWN_RADIUS` 537 m, ten times the ground a 170 m town covers,
  which is the square root of ten on the radius), and the walker on a
  street of the port facing the middle of town. F
  swaps to the fly camera from wherever the walker is and back, and
  back is a FALL: the walker is let go with its feet an eye's height
  under the camera (`Walker::enter_at`) and comes down under the same
  gravity a jump does, which is the owner's ask, and never snapped to
  the ground under it. Flight uses
  quaternion orientation: Q/E rolls, Space/Ctrl moves along camera up/down,
  the mouse turns without a pitch limit, either Shift boosts, the wheel
  adjusts speed and R levels to the local horizon. Flight settings live in
  `assets/config/flight.json`. Tab wires,
  Esc frees the mouse; `--fly` starts in the air, `--eye` and `--look`
  place the camera, `--levels` sets the count, `--octaves` buys a picture
  down on a software rasteriser, `--walk N` drives the walker forward N
  frames at a fixed sixtieth, and `--frames N --shot out.png` takes a
  picture once the streamer is idle and N frames have run and quits. The
  log says where the port is and what its first lot is, so a picture can be
  aimed at it.
- **The rings follow the eye's own GROUND at altitude, not the eye.**
  A box is `2 * HALF` chunks a side centred on what it follows, which is
  16 km either way at the coarsest on this planet. Followed on the EYE,
  the ground drops out of the box entirely the moment the eye is higher
  than that: the owner's picture from 49.4 km up read `0 chunks, 0
  triangles` over a blurred smear, and the smear was the CHART at six
  kilometres a texel because there was nothing else left to draw the
  world with. Measured, the ground was 32,192 m outside a 16,384 m box.
  `Rings::focus` pulls the centre down toward the eye's own ground past
  half the coarsest box's width, so what streams is the patch you are
  looking DOWN at, and
  `the_ground_is_inside_the_coarsest_box_at_every_altitude` holds 0, 50,
  1,200, 12,000, 49,400 and 400,000 m up. Nothing about the nesting
  moved: every level follows the same point, so a finer box is inside
  its coarser one exactly as it was.
- **A FAST EYE GETS A COARSER FINEST RING, and that is what a drive
  down a highway needed.** `Rings::adapt` dropped the fine levels by
  HEIGHT and by nothing else, and on the ground at 160 km/h the finest
  box is 32 m either way: it is crossed in under a second, every
  crossing is a whole layout re-planned, and the streamer publishes a
  layout only once every chunk in it is ready, so a finest box gone
  before it is built is a layout that never lands and a world stuck at
  the last one that did. That is the owner's "it takes a long time for
  it to load as I start driving down the highways". The rule is the same
  one the height term keeps, through ONE demand:
  `height.max(pace * DWELL)` with `DWELL` four seconds, so a box too
  small for what the eye is DOING is a box not worth streaming.
  Measured: standing, walking (5 m/s) and running (8.5) all keep the
  half metre cell they always had, and the car's own 44.4 m/s drops the
  finest ring to level 2, a 2 m cell reaching 128 m, with the hysteresis
  bringing it back the moment the car stops.
- **And the boxes stand AHEAD of a moving eye.** A box is centred on
  what it follows, so half of the finest one is always behind, which is
  ground already driven over. `Rings::focus` leads by `LEAD` (1.5
  seconds of travel), capped at half the finest active box's own reach
  so the eye can never be outside its own finest ring: at the car's top
  speed that is 64 m of road arriving before the bonnet does, the cap
  binding on a lead of 66.6. The pace
  it reads is EASED over `PACE_EASE` (0.75 s), because a rule that read
  one frame's own step would drop and re-raise the finest ring every
  time a car touched the brake.
- **The coarsest box no longer holds the planet, and the CHART is what is
  behind it.** At 5,000 m of radius the 32 km box held the whole world; at
  1,000,000 m it is a patch 16 km either side of the eye. On foot that is
  still far past the horizon, which is `sqrt(2 R h)` and 1.8 km from an eye
  1.7 m up, so a walker sees no edge; from the air the world ended at the
  box with nothing behind it. What draws there now is the displaced
  icosphere painted from the body's own equirectangular chart (the section
  on the chart above), which is tenebris's answer ported. Planet-LOD, which
  drew the rest of the planet for the hex world in 44 leaves from four
  radii up, was the other and is not what this took.

## Water is a SURFACE for now, and the sea is clipped by what is under it

**The sea is a level and its surface is the sphere at that radius**,
contoured by the same `dc.rs` as the ground, at the same levels, in the
same chunks, so the two meet at a shore with the same cells. What clips it
is the MATERIAL, per triangle: a triangle of the sphere whose middle
stands in the ground's rock is `BURIED` and never drawn, and one over air
is `SURFACE`; the depth test settles the shoreline to the pixel, since the
ground is drawn first. Water is not in the ground's field and takes
nothing off a chunk: a chunk's sea is a second mesh from the same lattice,
and a box the level does not cross has none (`Density::solid` on the sea),
so a chunk of dry land pays nothing for the sea.
`the_sea_contours_to_a_closed_shell_and_only_the_surface_over_air_is_drawn`
audits the sheet: the whole sphere closed, the drawn part the part over
air.

**The first cut was a water VOLUME and it pinched.** A field of its own,
positive in water, contoured as a second surface, met the ground's surface
along every shoreline in the same cells, and two dual contoured surfaces
in one cell put two vertices a rounding apart, which the audit counts as a
pinch and the eye sees as a flicker along the whole shore. A level and a
material per triangle is the answer, and it is the one the sheet keeps.

**VOXEL water is what comes back with the thing that digs.** The owner's
older ask was that a hole dug below the sea's level away from the sea be
dry and one dug from the shore flood, and the answer built for it was a
rule rather than a second field: a cut is DRY unless it touched water when
it was made, the surface is buried inside a dry cut, and a cut that
touches water and reaches a dry one wets it and every dry cut that one
reaches. That machinery went out with the builder, because nothing digs
this ground now and a rule with no caller is a rule that rots. It is
written down here so it can come back whole with whatever digs next, and
`git log` has it.

**The walker wades.** The sea holds the feet no deeper than `WADE` (1.2 m)
under its level: past that the body floats, standing on nothing, and walks
(`Walker::float`, `Bounds.sea`, and
`the_sea_holds_a_walker_at_wading_depth_and_it_walks_on`).

**The sheet is tenebris's water shader on Bevy's own transmission.**
`water.rs` in the app and `water.wgsl`: a material extension on the
standard material with screen space specular transmission on, so what is
seen through the sheet is the ground behind it refracted and attenuated
over the THICKNESS of water the view ray crosses, which the fragment stage
reads off the depth prepass (the camera carries `DepthPrepass`, one
transmission step, and the sheet opts out of the prepass so it never reads
its own depth as the sea floor). The extension adds tenebris's: the swell
in the vertex stage along the radial (`water.vs.glsl`), the ripples as the
gradient of its `fbm` of its gradient noise bending the radial normal, the
fresnel sky and the foam on the crests (`water.fs.glsl`, named in the WGSL
line by line), and every coordinate PLANET LOCAL from a centre the
material is handed and `rebase_origin` moves, which is the Sequoia lesson
kept where it was learned.

**The NUMBERS are pale-blue-dot's, and they are tenebris's own re-measured
under a tone mapper.** That project is the same water shader carried into
the same engine, its settings are `assets/config/water.ron` and its
`openspec/changes/water-look/design.md` is the ablation that picked them:
every variant rendered on one binary, the sea's mean sRGB measured over a
fixed band, against a photograph of open ocean. Two of its results are
worth keeping whatever the sheet is drawn by:

- **Every shine knob in the shader together is worth about one per cent of
  the colour of a sea frame, and ABSORPTION is worth thirty times that.**
  Turning the sun's glint off entirely moved its shore frame by ONE level
  of 255; doubling the absorption moved it by 31 and its saturation by 17.
  What a sheet of water over a seabed is made of is the transmitted path,
  so the only terms with authority over it are how much water stands in
  front of the sand and how hard a metre of that water tints. A look
  complaint names a symptom, and the first thing to measure is whether the
  term about to be tuned has any authority over the pixels in question.
- **Tenebris's own numbers are authored for a renderer that CLIPS.** Its
  composite ends on a bare write to an eight bit buffer, so its near white
  horizon reflection (0.85, 0.92, 0.98) and the 0.02 of red in its deep
  colour are right there and wrong here: under Bevy's default
  `TonyMcMapface` they lift and desaturate into a pale sheet. This sea was
  carrying both, so the fix was not a new term, it was the two constants
  the tone mapper had been quietly ruining.

So `sheet_ext` is that file: the deep colour 0/0.12/0.28 with NO RED IN IT
AT ALL, which is what a saturated sea needs under a tone mapper; the sky a
clear day blue (0.10/0.36/0.72 to 0.03/0.18/0.55) rather than near white;
the fresnel floor 0.22 rather than 0.5, which is the one shine knob with
measurable authority, worth 22 levels of red on deep water at a grazing
angle; the waves calmed to a steepness of 0.45 and a slope cap of 0.7; and
absorption per metre of 0.90, 0.25, 0.08, which is hardest in red because
the sand under a metre of water is red and only the water in front of it
can take that out. Measured at the port's own shore, an eye at the
waterline looking three kilometres out: 142, 152, 162 at saturation 20,
which is a grey sheet, against 65, 120, 173 at saturation 108. 17.1% of
the picture moved.

**There is ONE absorption and ONE path.** Bevy's transmission attenuates
the refracted ray by `attenuation_color ^ (thickness / attenuation_distance)`,
so `water::attenuation` DERIVES that colour from the absorption vector at
a distance of one metre rather than authoring a second one beside it, and
the shader adds back `deep * (1 - exp(-absorption * path))`, which is what
Bevy's attenuation cannot do: left alone it takes the seabed to NOUGHT
over a long path, and deep water is not black, it is its own colour. The
two halves are `mix(deep, scene, exp(-a * path))` and they read the same
number. The path is the prepass thickness CAPPED at `MAX_PATH` (200 m), so
a ray that meets no floor at all is the sheet's own colour.

**The ripples are faded by their own FOOTPRINT, not by distance.** They
were worn out from 30 m to 160 m of range, and that is the right idea with
the wrong variable: what a fade is for is stopping once a ripple falls
under a pixel, which is a function of the angle and the field of view. At
a grazing angle a ripple thirty metres off already covers a pixel and was
still being point sampled, and fresnel and foam turn that sampling into
the white sparkle the before picture is full of. It is
`1 / (1 + |fwidth(world)| * scale * DETAIL_FADE)` now, on the WORLD
position because that is continuous across a chunk seam where the ripple
cell is a step, and it fades the height as well as the gradient so the
foam stops sparkling with them.

**The night side is DARK, and the sheet mirrors the sky the DOME is
painting.** The reflection was an authored day gradient at every hour, so
the sea at midnight was a lit blue sheet under a black sky, brighter than
the land beside it. `sky.rs` writes the sun's direction into the sheet
every frame, the fragment measures the terminator on the body's own
RADIAL (the same `DUSK_TO` and `DUSK_FROM` `distant.wgsl` uses, so one
line falls in one place), and past it the reflection is `water.fog.rgb`,
which is `atmos::horizon` and is already in this uniform because the fog
reads it. No authored night colour and no second sky model: the one the
atmosphere computed is the one the sea mirrors, which is this file's own
"the fog IS the sky" rule arriving at the other surface. The body and the
foam take a `NIGHT_FLOOR` of 0.18; the REFLECTION never does, because it
carries the sky's own level already and dimmed twice the sea went black
at the horizon, where a mirror should be closest to the sky it mirrors.

**And the murk darkens with the DIVE.** Seen from under the sheet it was
the bare deep colour at half a metre and at eight, while the seabed under
it was already attenuated by the water over it: a lit blue room with a
darkening floor. It is `deep * exp(-absorption * eye_depth) * lit` now, on
the same absorption everything else reads, so a dive gets darker and a
night dive gets darker still.

**What was NOT taken from pale-blue-dot, named rather than hidden.** Its
water is a render graph NODE with three sub passes (a composite that fogs
and blurs the whole frame when the camera is under, the cap, and a lens
pass of rain droplets and emerge drips), and it decides the camera's side
of the surface on the CPU as a tri state. This sheet is still a material
on Bevy's own transmission, so there is no underwater composite, no lens,
no rain ripples and no flow field; the underwater view is the cap's own
back face and nothing else. Its explicit Blinn Phong specular is
deliberately not taken either: Bevy's PBR already lights this sheet, a
second highlight is a term drawn twice, and that project measured its own
at one level of 255. What the standard material carries instead is water's
real F0 (Bevy `reflectance` 0.25 against 0.02 of reflectance) and a
roughness of 0.12, which is a sea rather than the mirror 0.06 was.

**THE SPECKLE ON THE SEA IS BEVY'S OWN SCREEN SPACE TRANSMISSION**, and
what says so is that it is flat in SCREEN space over a hundredfold range
of world footprint. The owner sent a picture of a sea covered in per
pixel salt and pepper and named four candidates: floating point
precision, a shader not working in the body's local frame, world space
UVs on one giant mesh, and a voxelised sea. Measured, it is none of
them, and each of those four was already answered in this file: the
ripples take a lattice CELL and a FRACTION so nothing is a planet scale
float, every coordinate is planet local from a centre `rebase_origin`
moves, the sheet has no UVs at all, and it is a contoured SURFACE with
its buried triangles dropped.

**A speckle is energy at the PIXEL, so the measure is the mean absolute
Laplacian of the luminance in a band.** On the owner's own framing, an
eye at the waterline, it reads 8.20 just under the horizon where a
ripple is kilometres of ground to a pixel, 7.48 in the middle distance,
7.67 near and 6.83 at the feet, against 4.65 for the SKY in the same
frame, which is the dome's own deliberate per pixel dither. A defect
that came from a coordinate, a frame or a mesh would grow with distance
from the origin or with the footprint. This one does not move at all.

**The A/B names it**, on a solved `--shore 2` camera: with Bevy's
`specular_transmission` taken to nought the same frame drops from
**5.09 to 3.38** in the middle band, 4.78 to 3.16 near, 3.81 to 2.76 at
the horizon, and the SKY band is unchanged to three decimals, which is
what says the measure caught the water and nothing else. The crop is
salt and pepper on one side and smooth on the other.

**And two more ablations say what it is NOT, which is the useful half.**
Taking the transmission's own blur to nought steps, so the centre tap is
all there is, is no better at all (5.25 against 5.09): the noise is not
the randomly rotated spiral of taps that fetches the background. And
zeroing the RIPPLE gradient moves the water band by **one level of 255
over 1% of its pixels**, because at a grazing view `detail` has already
faded the ripples to nothing there: the ripple normal is not the source
in this frame either.

So what is left is the refraction's own OFFSET. Bevy fetches the
background at a screen position displaced along the refracted ray and
scaled by `material.thickness`, and this sheet hands it the PREPASS
thickness, the path through water to the seabed, capped at `MAX_PATH`
(200 m). Over open sea at a grazing angle that path swings across its
whole range between neighbouring pixels, so two neighbours fetch
background hundreds of metres apart, and the background is terrain with
its own grain. That is named rather than fixed, because the fix is a
choice between a thickness that varies smoothly and a transmission that
does not blur, and neither has been measured yet.

**And a `--shore` camera could not find the sea at all**, which is what
this cost to measure and is a finding of its own. `aim::shore` scanned
three kilometres on the reasoning that the port stands on a shore by
construction, and `town::coastal` sizes a town by how HIGH over the sea
it stands rather than how NEAR the water it is. Once `town::CUT` capped
how deep a site may cut, the flattest big sites are inland basins and
plateaus: **not one of this body's 521 settlements has open sea within
three kilometres**, and the port's own nearest water is 15,725 m off, so
every `--shore` render came back as a main street with no sea in the
frame. The scan is coarse to fifty kilometres and then fine over the one
step that found water, on the BARE planet because a town's plateau
cannot make sea, and it aims at the biggest settlement rather than at
town 0, which is what its own doc always claimed.

**And the night sea is not PHOTOGRAPHED**, which is this file's own camera
rule catching me out: two hand aimed cameras at the antipode and at the
terminator both landed on dry land, and the atlas cannot solve a third
because it is baked at eighteen octaves and a picture on this rasteriser
is bought down to fourteen, which is a different world with different
coasts. The tool that is missing is a `--nightwater` that solves a shore
at a given sun elevation the way `--sunward` solves an orbit, and until it
exists the night path is code with a test behind it and no picture.

## Cities are MODELS on the planet, and the ground under them is levelled

The mockup's towns were where a mesher was judged; the game's are the
reason to land. `town.rs` is `planTowns` ported and grown: candidates come
off a golden angle spiral round the planet (`CANDIDATES`, four thousand),
a candidate qualifies on land between three and forty metres over the sea,
nearly level across the town's width, and apart from every town already
placed, and the first that qualifies is the port, because a port is the
town this game is about. A town is a local grid, blocks of `BLOCK` (10 m)
on a `PITCH` of 18.5 with `STREET` (8.5 m) between, a lot per block,
taller near the middle, a few blocks left as plazas, and every street's
RUN cut into pieces about `PIECE` (3.5 m) long, each on its own patch of
the sphere, which is the mockup's chord lesson at the game's radius: on a
5 km planet a fifty six metre chord sags 8 cm, and a piece never has to.

**A city FLATTENS ground and never adds any.** `Planet::surface` takes
whichever is lower of a site's own blend and the bare relief, and
`town::settle` gives a site the LOWEST ground its own survey found rather
than the height at its middle. Either alone is not enough: a level taken
at the middle of a sloping site lifts the downhill half, and a blend that
averages toward the level lifts every dip on the skirt. Together a site
can only ever cut, which is what grading is and what the owner asked for
after reading a town on a pedestal off a picture.

The survey is forty nine marches, twelve bearings on four rings, so the
level is the lowest of them; a dip between two neighbouring samples is
the only ground a site can still fill, and how deep that can be is
bounded by the `LEVEL` fall the site had to pass to be accepted.

**A CITY IS TEN TIMES BIGGER now and that made a QUARRY of every one of
them, which is the constant's own doc condemning itself.** `TOWN_RADIUS`
is 537 m rather than 170, the square root of ten on the radius for ten
times the ground the owner asked for. `LEVEL` is a SLOPE across the
site, 0.10, and its own doc already said what that is for: "at a tenth a
hundred and fifty metre city is cut fifteen metres into its own hill,
which reads as a terrace, and much more than that reads as a quarry". A
town of 537 m reaches 1,106 m at its outline, so the share allows a
**105 m cut** and the body's mean was 18 m, ramped over a
`field::site_skirt` of 24.6 m at up to **641%**. That is a cliff round
every city, and a road arriving at one crossed it.

**So a cut is CAPPED in metres**, `town::CUT` (15), which is a number a
share cannot express: what says terrace or quarry is the WALL against
the buildings beside it, and fifteen metres is five storeys. It is a
MAXIMUM, so a body whose towns are small never meets it and the share
binds first. It is also what makes an apron CLIMBABLE, which is the
other half: a smoothstep climbs at one and a half its own average, so
15 m over 24.6 is 0.91 against `walker::STAND`'s own 1.19, and a walker
gets up it and so does a car, which is this game's rule that the two are
one answer.

Measured on the body, before and after: **a town cuts up to 105 m at its
own edge (18 m on the mean) at up to 641%, against 17 m (9 m on the
mean) at up to 101%.** What it COST is the count, because a city of a
kilometre needs a plain: **521 settlements and 265 roads over 55,105 km
joining 122 of them, against 1,241 and 308 over 63,806 km joining 152**,
and 130 of the 160 city sites qualify rather than all of them. A body
that carries 130 cities on genuinely level ground is the trade, and it
is the one this takes.

**A town is `OUTLINE` (2.06) radii across and THREE numbers said one.**
The owner's picture was a suburb buried to its eaves with only the roofs
and the driveways showing, and it is one mistake made three times:

- `site_of` built a site whose band `site_band` then HALVED, so the
  ground was levelled right across to about one nominal radius.
- `site_ground` surveyed out to 1.05 of one, so the LEVEL was the lowest
  of the middle of the town and the ground the suburbs stood on had
  never been looked at.
- `lay` emits lots all the way out to 2.06 of one, which is right.

So every lot past about half a town stood on BARE RELIEF with its base
at the town's level, and the level is the lowest of the survey, so the
relief out there is HIGHER and the building is under it. Measured on the
fixture planet: **1.13 m into the ground, and 0.00 m after**, which
`a_lot_stands_on_its_own_ground_and_is_not_buried` holds. `site.r` is a
full-level RADIUS now (it was a diameter and one caller thought it was a
radius), the survey's rings are shares of `OUTLINE`, the levelness bound
is a SLOPE rather than a fall so widening the survey asks the same
steepness of more ground, and two towns stand apart by their OUTLINES
rather than their radii, because two sites that overlap cut into one
another.

**And the ATLAS had to be re-baked for any of that to reach the game**,
because what it stores is each town's own LEVEL. A fix to how a level is
computed that leaves the baked levels alone is a fix nobody sees: the
six things the atlas is refused on (the name, the seed, the radius, the
town size, the sea and the octaves) do not include how a site settles,
and the PROBE only samples bare ground, which this did not move.

**It cost the mesher's own fast path, and that is where the hole was.**
`local_solid` answered a box wholly inside a site's level region straight
off the level, rock below and air above, which was exactly right while a
site's ground WAS its level. With a cut it calls rock the ground the cut
has taken out from under it, and a chunk ruled rock is never meshed: a
hole in the world. The air half survives, because the level is still an
upper bound on the surface; the rock half is gone and what rules those
boxes now is the general path's own sample and bound, which is sound
inside a site because the min of a constant and the relief is never
steeper than the relief. Three fixtures in this repository had a site
standing OVER their own ground, which is now a site that does nothing at
all, and each had to be given a level that cuts before it tested
anything again.

**The ground under a town is LEVELLED and that is the field's job**
(`Planet.sites`: one right across the town and nought past an `APRON` of
12 m, the relief the site's height and the noise not asked), so a plateau
has a smooth skirt cut by the same field, and every building stands plumb
on its own lot's frame (`lot_frame`: the lot's direction, east and north
there keeping the town's heading, the town's level as its base).
`towns_stand_on_level_land_over_the_sea_and_apart` and
`a_levelled_site_flattens_the_ground_to_the_towns_height` hold it. That is
the ONE thing a town still does to the terrain, and it is a term in the
planet's own field rather than a second lattice.

**And that plateau FOLLOWS THE TOWN'S OUTLINE, because a town is not a
disc.** `site_of` levelled `radius * OUTLINE + APRON` right round, which
is the furthest a town can EVER reach in any direction, and a town
reaches that far in one direction: `demand` stretches it by `STRETCH`
(1.45) along its own shore and squeezes it by the same across, so along
the squeezed axis the disc stood more than twice as far out as anything
built on it. Measured on the fixture port: the disc is 109 m and the town
is 40 m out at its narrowest and 84 at its widest, so **72% of that
plateau was bare flat ground**. The owner read it off the climb as big
flat discs, and it was one, a hundred and sixty times.

`Site::outline` is the fix and it is the design's own rule about `demand`
arriving at the one reader that had not asked: how far a town REACHES
and how far its ground is LEVELLED are one function (`town::edge`) rather
than two that have to agree. `demand` is `1 - dist / edge(bearing)` now
and `Site::level_r` is `edge(bearing) + APRON`, so every lot is on
levelled ground by construction
(`a_towns_plateau_follows_its_outline_and_not_a_disc` walks both halves,
and `a_lot_stands_on_its_own_ground_and_is_not_buried` still measures
0.00 m into the ground).

**The lobes are read at the NOMINAL edge and not at the query point**,
which is what makes the outline a function of its own bearing: a ray out
of the middle of a town then crosses it exactly once, so there is an edge
to level up to rather than a level set somebody has to root find.

**And a skirt past a boundary that is not radial is STEEPER than one
past a boundary that is**, by `hypot(1, WOBBLE)`, which is the whole
price of an outline. `town::WOBBLE` is how fast the edge can move as you
walk round it, metres of edge a metre of arc, MEASURED over every bearing
of a thousand towns rather than reasoned about (the worst is 1.571 and
the constant is 2; reasoned from the lobes' own noise gradient it is
4.65, which is a fifty metre apron for a swing no town ever takes).
`field::site_skirt` widens a town's own skirt by exactly that, so the
blend's gradient is what it always was and the planet's slope bound
(`steepest`, which divides by the NARROWEST skirt on the body, a road's)
never moved. `Site::level_floor` is the same number the other way up:
what a BOX spanning some arc can be ruled against, because a bound that
ignored the wobble would call a chunk air over ground the town never
levelled, which is a hole.

**And the tarmac comes IN to the town now.** `road::open` measured
against the town's whole site band, which is its widest: a road out along
the squeezed axis stopped three hundred metres short of anything the town
had levelled and the highway ended in a field, which is the owner's "the
main city I spawn at doesn't appear to have a highway leaving out of it".
It reads `Site::level_r` at the road's own point, which is exactly where
the town's own site starts winning.

**A building is a MODEL, which is the owner's ask.** It used to be a
recipe of signed distance brushes evaluated in the ground's field on a
lattice six times finer under each town. That bought one thing, which is
that the picture and the collider were the same field, and it cost a
second lattice, a massing rule for the far chunks, a chunk test against
every structure on the planet, and a remesh for every edit. `model.rs` is
the answer now: a building is triangles built from numbers, beside the
field rather than in it, and the ground is ONE lattice carrying terrain
and nothing else.

**A wall is one oriented box which is DRAWN and COLLIDED**, which is how
the old guarantee is kept without the field. `Model::solid` pushes the
box's six faces into the mesh and the box itself onto a list; `Model::trim`
and `Model::quad` only draw. So the picture and the collider are the same
numbers and cannot drift, and anything a body should pass through, a
parapet, an eave, a pane, a lamp, is trim and never stops a body, which is
the mockup's own lesson about an exterior floor line the size of a lot
catching a climber's head at the fourth step.

**Five kinds, and a kind is a match arm.** `Kind::Block` is a rectangular
tower of four to eight storeys with plate pillars at its corners and a
parapet, `House` one or two under a gable, `Bungalow` one with a flat
roof, `Tower` a round drum of twelve boxes in a ring, and `Hangar` a shed
under a barrel vault. `town::choose` puts the towers in the middle, the one
floor houses at the edge and the odd hangar among them. The recipes these
replace were JSON files read at startup with a reader written for them
(`recipe.rs`, `json.rs`, 1,064 lines and eight files in `assets`); a kind
is a variant and an arm, and there is nothing to ship beside the binary.

**A building stands on its own BLOCK and never in the street**, which
is the owner's picture of a wall standing on a pavement. A block is
`BLOCK` (10 m) across and the street's own inner kerb is exactly
`BLOCK / 2` from its middle, so a lot may be moved within five metres
of its block's centre and no further. What it was moved by instead was
a JITTER of `BLOCK - 8` downtown and twice `SUBURB_SETBACK` in the
suburbs, up to 4.5 m either way, on a building that already covered the
whole block: measured on the fixture port, **14 of 15 lots stood in
their own street and the worst was 3.72 m in**, which is a suburban
hangar with its far wall past the centreline of the road.

`Kind::covers` is the one number that closes it: how much of its block
a kind's walls cover, so the room a lot has is `BLOCK * (1 - covers) / 2`
either way and `town::plot` bounds every offset by it. It is ONE for
every kind today, and that is a fact about the LIBRARY rather than a
knob nobody turned: `assets/config/buildings.json` bakes all thirteen
variants at the full block and `Library` REFUSES a bake wider than its
kind claims, so there is no room for a setback to be in and every
building sits square on its block, flush with the back of its own
pavement, which is what a terrace is. `SUBURB_SETBACK` is what a suburb
WANTS and the block is what it gets; the day a house is baked at six
metres the setback appears with it and with nothing else changed.

`a_building_stands_on_its_own_block_and_never_in_the_street` measures
it, off the building's own SOLID boxes rather than its mesh, because an
eave, a parapet and a pane are `Model::trim` and a body passes through
them: **0 of 15 lots, worst 0.00 m**.

**Every building is a shell with a doorway**, and the walker walks in:
four walls, the south one two piers and a lintel, a floor slab, a roof,
panes on every storey of every face but where the door is, and a lamp over
the door and one under each storey's ceiling.
`a_wall_stops_a_body_and_the_doorway_does_not` walks a line from two
metres outside the door to the middle of the room and holds every point of
it air, then holds the piers, the lintel and the other three walls solid.
What is MISSING is floors and stairs: a building is one room to the roof,
and that is named here rather than hidden, because on a field they were
brushes and here they are geometry nobody has written yet.

**A walk is what proves a town, and a picture cannot.**
`a_walker_walks_a_street_and_is_stopped_by_a_wall` sets a walker down on a
street of a planned town, walks it two and a half seconds up the street and
holds it on the ground within a few decimetres of the town's own level,
then sets it three metres east of a lot's east face and holds it short of
the wall by its own body. It found its own first defect: aimed east from
the street it walked eleven metres, because a lot is ten metres on a pitch
of fourteen and the gap between two lots is a way through, which is the
town's plan being right and the test being wrong. A render of this world on
a software rasteriser is seventeen minutes, so the walk is measured where
it costs nothing.

**A street is a CROSS SECTION, and that is the whole of `town/street.rs`.**
It was ONE QUAD four metres wide, and mining-mike's `city_road.gd` (in
`godot-sandbox`) says in its own note what is wrong with that: a box has
no cross section, so the road has no kerb, no pavement and no markings,
its corners meet at hard mitres, and a dead end stops mid cell with an
open edge. Four metres is also too narrow for two cars to pass. So a
street is `LANE` (2.75 m) of carriageway each way with `WALK` (1.5 m) of
raised pavement either side of it, which is 8.5 m of `STREET`, and the
`PITCH` is that plus the block, because those are not three numbers that
have to agree, they are one number said three ways. A car is 1.6 m
across, so two pass with better than a metre between them; a person is
0.45, so two pass on the pavement too.

**A CROSSING is a piece of its own, and it owns its whole square.** A run
spans the BLOCK it serves and stops at the crossing squares either end of
it, which is what leaves a crossing somewhere to be, and `streets_of`
emits one wherever a street could leave a node, carrying the ARMS it
actually has. Its square is three bands each way, nine cells: the middle
is always carriageway, the four corners never are, and each of the four
bands between is carriageway exactly when its own arm is there. A
crossroads is a plus of tarmac with four kerbed corners, a bend's kerb
turns the corner as an L, and a dead end closes with a pavement across it.
`town::paved` is that one decision, asked by the mesh that draws a
crossing AND by the traffic that has to know where a pedestrian steps
down off the kerb: a second copy of those nine cells would be a person
walking a hand over the tarmac at one junction in a hundred, which nothing
would say.

**And that is what killed the BARE CORNER the last commit named.** A run
used to reach only as far as a crossing's own centre line, so the square
where two streets met was covered from the sides a street arrived on and
no further: at an L bend the far quadrant was bare and the lane turning
left crossed it, 1.62 m of levelled verge. With the square paved whole,
`nobody_steps_off_the_paving_at_all` measures 0.000 m.

**A marking is PAINT, which is a material and not a mesh.** One dash of
the centreline a piece and a solid line down each side of the
carriageway, four millimetres over the tarmac in `field::PAINT`, which
`terrain.wgsl` draws as the concrete set BRIGHTENED the same way a street
is that set darkened. So a marking costs no texture, no second draw and
no shader of its own, and the whole town is still one shader and the five
sets it already binds.

**And a wider street COSTS a town its density, which is arithmetic and
is reported rather than compensated for.** The `PITCH` is the block plus
the street, so at a fixed town radius the count of blocks falls as the
square of it: 14 m to 18.5 is (14/18.5)^2, and measured on one binary
either side of that one constant, the 8 built towns are 400 buildings
against 695, 1,644,942 triangles against 2,791,902, and 300 people and
76 cars out against 522 and 132. The ratio is 0.575 against the 0.573
the area predicts, which is what says nothing else moved. Getting the
buildings back means growing the BLOCK or the town's own radius, and
neither is what was asked for: a street you can pass two cars on is
wider, and a town of a given size has fewer buildings on it when its
streets are.

**The carriageway stops nothing and the PAVEMENT does**, and that
difference is what a body does with each. The site under a town is
levelled, so the ground there is a plane and dual contouring holds it to
two millimetres (the audit's own number): five centimetres of paving
clears that by twenty five times and is under anything, so a walker
stands on the ground through it and it needs no collider, which is what
this always said. A `KERB` of 12 cm is not under anything: it is ANKLE
DEEP, so a pavement that only drew would be a pavement a walker waded
along. It is one `Model::solid` a strip, which is this file's own rule
that a box is DRAWN and COLLIDED from one set of numbers, and 12 cm is
well under the walker's 60 cm step, so he steps up onto it rather than
being stopped: `resolve`'s ring of points starts AT the step, so it never
sees a kerb at all. Measured underfoot across a run, at the middle of
each of eighteen bands: flat to 2.72 m out and 0.172 m up from 2.95 to
4.13. It cost 6,576 boxes on the eight built towns, 87,691 against
81,115, and not one triangle, because a `solid` draws what a `trim` did.
A pavement's slab is SUNK 15 cm into the ground, because an underside
lying exactly on that plane would fleck along its whole length.

**A town is its BLOCKS, and a far block is a SOLID BLOCK.** The owner's
ask was whole cities of thousands of buildings in one picture, with the
far ones as solid blocks, and a town drawn as one mesh cannot give it: a
town at one grade is every building at that grade, and standing in the
port that is 1,522 buildings at the nearest bake, **2.72 million
triangles and 551 ms of one frame** to raise. So `city/tiles.rs` cuts a
town into TILES a block each (`PITCH`, 48.5 m; 326 of them in the port),
and each tile is drawn at the grade its OWN distance is worth: the
library's three bakes to `building_lod_detail`, `_near` and `_far` (80,
250 and 1,200 m) and a solid block past that.

**A block is read OFF THE MODEL** (`model::massing`): the footprint and
the eaves are the model's own SOLIDS, the walls a body is stopped by,
and the top is its own mesh, so a variant re-baked taller comes back
taller at every range with nothing edited. A box to the eaves in the
building's own skin, a gable where a house has one, a drum of eight
faces where a tower is round, sunk 40 cm so a coarse chunk far off
cannot show sky under it. A street at that grade is one quad
(`street_graded`), and at the grade before it the kerb sides and the
paint go (`flat`), because a kerb is 12 cm and at 250 m that is under a
pixel. Measured on the port, a tile at a time and whole: **2,718,442
triangles at the nearest bake, 1,870,606 at the next, 253,160 at the
farthest and 32,686 as blocks**, which is 21 a lot against 1,786, and
building every tile as blocks is **23 ms** against 551.

**Every tile always HAS its blocks, and detail is swapped over them.**
A town is raised on a WORKER (`detail::raise`: its tiles and every
tile's blocks), stood in one frame (`detail::stand`), and from then on
`stream_tiles` asks each tile what its distance wants and puts what is
missing on `AsyncComputeTaskPool`, nearest first, `building_jobs` (3) at
a time. A grade lands HIDDEN and the block under it is hidden in the
same frame, so a tile is never a hole for a frame, which is the
terrain streamer's own rule. What a tile is DRAWN as and what it STOPS a
body with are two jobs: a wall is the same wall at every grade and only
a tile a body can reach needs one, so its boxes and lamps are built
within `STOPS_NEAR` (180 m) and dropped past `STOPS_FAR` (260), and a
job for boxes goes before a job for a picture, because walking into a
wall that is not there yet is worse than a block that is still a block.
The port is 152,899 boxes whole; standing in it the eight built towns
hold **30,506**.

**One mesh a TILE, in the town's own frame.** `model::fabric_part` is
`fabric` over the lots and pieces of one tile, each carried from its own
lot's frame in `f64` and then cast: a town is a kilometre across, so an
`f32` in its frame holds a tenth of a millimetre, which is the chunk
local rule at a town's scale. The town is one parent entity placed from
its world position through the floating origin like a chunk, the tiles
are its children with identity transforms and their own bounds, and
`rebase_origin` moves the parent with everything else, which is what
`Anchored` is. `a_town_built_in_parts_is_the_town_built_whole` holds
the tiles to the town to the triangle.

Measured on lavapipe from 650 m up and 1.9 km off the port, the same
2,435 terrain chunks either side: **554 tiles at 49,698 triangles, a
median frame of 699 ms against 2,601, a worst of 1,611 against 10,042,
and the terrain settled in 115.6 s against 419.5**, because the frame
the streamer drains in is no longer a frame spent drawing every window
in eight towns. On the port's main street: 13 tiles at the nearest
bake, 67 at the next, 246 at the farthest and 228 as blocks, 761,998
triangles for all eight towns.

**A lamp is a light near the eye and a number everywhere else.** Every
building's lamps are known to the world (2,771 of them on this seed), and
`lamps.rs` makes point lights of the nearest `MOST` (forty eight) within
`REACH` (60 m), spawned as the eye comes within reach, despawned as it
leaves, placed through the origin like a chunk. A city of thousands of
lamps is not thousands of lights.

**Concrete is mapped in the town's frame, and the owner saw why.** The
first render put panel seams across every wall at the angle between the
building and the planet's axes, because the triplanar mapping was in the
planet's frame, and the owner read it off the picture as UVs out of
alignment. `terrain.wgsl` is handed every town's frame (`FRAMES`, sixteen,
three lanes each: the direction with the ground's radius in w, east,
north) and maps concrete, plate and a street in the nearest one: east and
north measured on the sphere the town's ground is at, from the town's
centre, and height off that sphere, so a wall plumb on its lot has
constant east or north up its height and a floor constant height, and a
panel is level and plumb whatever the planet's axes do. From the town's
CENTRE and never the fragment's own foot, because a point on a sphere
projected on its own tangent plane is nought everywhere, which was the
mockup's float noise. The ground stays in the planet's frame, where a seam
in grass is nothing. The models wear the same material the ground does, so
there is one shader and one set of sets for the whole world.

**Cities are ALL OVER the planet, and a city is planned, levelled and
painted long before it is built.** 160 towns are planned rather than 8.
Every one levels its own ground and is stamped on the body's chart, and
the nearest `TOWNS_BUILT` (eight) to where the world starts are BUILT: a
town is about 375,000 triangles of baked buildings, so eight are 2.85
million and 160 would be sixty. It is a COUNT rather than a distance,
because it is the count that bounds the cost: at 160 towns the mean
spacing here is 280 km, and a first cut written as a 90 km reach built
exactly one of them.

**A city is drawn on the chart at its own SIZE.** The biggest
settlement on a body is `CITY_TEXELS` across and every other is the
square root of its share of that one's ground, because a mark's AREA is
what reads as how big a place is; the floor is `SMALLEST_CITY` (one
texel), since a village at a fifth of a city is half a texel and half a
texel is a place the chart does not say is there. Measured against the
body's own biggest rather than a constant, because there is no longer one
figure a town is and a chart cannot know the next body's.

**A town is smaller than a chart texel, so it is STAMPED.** Eighty metres
against six thousand: asking `surface` about a texel's own middle finds a
town one time in five thousand, so the cities were invisible on the chart
and the sites cost every one of half a million texels a walk over every
town on the planet, 84 million tests, which took the bake from 954 ms to
1,617. Baked bare and stamped after, the cities are ON it, one texel each,
and the bake is back to 991 ms.

**A town's height window is the PLANET's.** A candidate qualified between
3 and 40 m over the sea, which on a world with eight kilometres of relief
is the coastal fringe and nothing else, so every town came out on a beach
and the interior of every continent was empty. It is 3 m to three tenths
of the relief now, and level ground at two thousand metres is a plateau.

**Every town was the SAME TOWN, and the owner asked why.** One
`TOWN_RADIUS` was handed to all hundred and sixty of them, and `lay` cut
its blocks out of a circle (`hypot(x, z) > radius`), so a body carried a
hundred and sixty copies of one disc. Three things replace it, and they
are one number rather than three rules:

- **SIZE is how near the SEA a town stands.** `town::coastal` falls from
  one at the shore to `SMALLEST` inland over `COAST` of the body's own
  habitable window: a port trades with the whole world and an inland town
  with its own valley, which is the owner's observation and most of
  economic geography. Measured on this planet, the biggest quarter of its
  towns stand 124 m over the sea and the smallest quarter 998.
  What it replaces was ZIPF on the town's RANK. That gives the right
  SPREAD of sizes and puts them nowhere in particular: the biggest city
  on the body was wherever the hash happened to accept first. The spread
  survives, because the heights do: most ground is inland, so most towns
  are small and the few on the shore are the cities.
  The fall off is measured in the WINDOW and never in the relief, which
  is this file's own "an absolute number is wrong on some body" rule a
  third time: a town may stand between 3 m and three tenths of the
  relief, so on a two kilometre test ball that is 3 m to 12, and a length
  written as a twentieth of the relief is two metres. Every town on that
  ball came out one size.
- **SHAPE is a demand FIELD**, and everything is read off it. `demand` is
  one at a town's middle, nought at its nominal edge and negative outside,
  with two octaves of the core's own value noise pushing that edge in and
  out by `REACH` (0.42) of the radius. A town is what grew where growing
  was easy, so it runs a long way down one side and stops short on
  another. Measured on the test planet's own port, whose nominal radius
  is 27 m: it reaches 35 m at its furthest and its nearest edge is 14 m
  out, which is what "not a disc" means at a resolution a block grid can
  express.
- **And it grows ALONG ITS OWN SHORE.** `settle` reads which way the land
  falls out of the same forty nine samples it levels the site from, and
  hands the town the direction ACROSS that, because the sea stops a town
  one way and the hill behind it stops it the other. `demand` stretches
  by `STRETCH` (1.45) along it and squeezes by the same across, so the
  ground a town covers is unchanged and its plan is a long one: a coastal
  town runs up and down its own beach. A site with no slope under it gets
  no direction and stays round.
- **And the ZONES are that same number's own thresholds.** Over `CORE_AT`
  is downtown and its towers, over `TOWN_AT` is the town proper at two to
  four storeys, and everything out to nought is SUBURB: one storey
  whatever the hash says, `SUBURB_FILL` of the blocks carrying a house at
  all, and a setback three times the town's own jitter. What makes a
  suburb a suburb is the SPACE rather than the house, which is why the
  fill and the setback are there and not just a shorter building: the
  same grid with low buildings on it is downtown with the towers taken
  away. Measured: 5.0 storeys inside a third of the way out against 1.0
  past four fifths, and 0.0045 lots a square metre against 0.0007,
  which is six times the density.

**A quarter of a town is TALL and three quarters is not, and the two
thresholds were where that was set.** The owner's number is a quarter
towers and three quarters small houses, and the town was 51.7% tall.
`CORE_AT` and `TOWN_AT` are shares of the demand field, and demand
falls off from a town's middle, so what decides the mix is where the
second of them sits: a town is the DISC out to `TOWN_AT` of its own
demand, and the share of its lots inside that goes as the area. It is
0.55 rather than 0.38 now, swept rather than guessed, and `CORE_AT`
moved with it so downtown is still the inner part of what is tall. It
was 0.54 for a commit and the GRAIN below moved it, because a bite
that takes ground off the FRINGE is a bite that takes one storey
houses: re-swept, 0.50 gives 29.9%, 0.52 gives 27.3 and 28.2, 0.54
gives 25.5 and 26.4, and 0.55 gives **25.5% on a 170 m town and 25.5%
on a 537 m one**. That the two town sizes agree to a tenth of a per
cent is what says the mix is a fact about the field and not about the
radius.

**A CITY IS NOT AN OVAL, which is the owner's own word for it, and
what it wanted is a GRAIN.** `town/shape.rs` is where a town's shape
lives now, split out when `town.rs` went over this file's own nine
hundred lines, and the whole of the change is one term in `demand`.

What a town was is a stretched ellipse with two gentle dents in it.
`edge` is two octaves of value noise on the BEARING, which is ONE
scale and ONE closed curve, so from a kilometre up a hundred and sixty
of them read as one smooth blob each; and the only other thing
deciding where a town is not was a flat coin toss a block
(`SUBURB_FILL`), which has no structure at any scale. A fractal
outline round a field of static is what that is.

A real city is ragged at every scale it HAS, which is the thing Batty
and Longley measured and called a fractal city: a built up area is a
cluster with holes in it, arms down its roads and outlying pieces off
its fringe, and its boundary is crinkled from the district down to the
block. **A level set of a fractal IS a fractal**, which this file
already says about continents, so the grain is an octave sum on the
block's own PLACE rather than on its bearing.

**The MEASURE is the boundary's own box counting dimension**, which is
Batty and Longley's and is the one number that tells an oval from a
city: count the boxes of each size that hold any of the outline and
fit the slope. A smooth closed curve is one whatever the ladder,
because halving the box doubles the count; a crinkled one more than
doubles it. `town::fractal` is that, on a ladder from one `PITCH` to
half a town, because finer than a block a plan has nothing to say (a
lot is on its block or it is not) and coarser than half a town there
are no boxes left to count.

**Measured: the outline goes from 0.987 to 1.173**, and it climbs at
every step of the bite (0.987, 1.105, 1.173, 1.212 at nought, a
quarter, the shipped 0.55 and 0.8), which is what says the number is
reading the grain and not the lobes underneath it.

**The PERSISTENCE is the whole of it, and `fbm3` could not have done
this.** A sum that halves its amplitude every octave has a Hurst
exponent of ONE, and a level set of a field with `H` = 1 has dimension
`2 - H` = 1: a smooth curve however many octaves are piled on it.
Grained by an `fbm3` a town measures **1.007** against the bare
ellipse's 0.987, which is a picture nobody could tell from the oval,
and that was the first cut of this. `noise::fbm3_rough` is the same
sum with the persistence handed in and `fbm3` is it at a half, which
is one implementation and a parameter rather than a second function.
SWEPT with the stretch HELD so only the roughness moves: 0.50 gives
1.007, 0.60 gives 1.056, 0.70 gives 1.114, **0.80 gives 1.173** and
0.88 gives 1.223, while the share of the frame the plan covers stays
between 16.2 and 16.6% throughout. That last column is the control: it
says the dimension is reading how ROUGH the bite is and not how much
of the town it took.

**And the SPREAD had to be measured again, which is this file's oldest
lesson arriving at a second sum.** A share handed a raw octave sum is
worth about a fifth of what it says, because a sum of many small
numbers piles up near its middle. How much it piles up is a function
of BOTH the persistence and the count, so the grain cannot borrow
`fbm3`'s 0.498 and 0.106: it is a TABLE by octave count (0.184 at one
octave down to 0.073 at eight) and every row is re-measured by
`measure_the_grains_own_spread`. Every row is also NARROWER than
`fbm3`'s at the same count (0.080 against 0.108 at six), which is not
what a rougher field sounds like and is what the normalisation does:
an octave kept at 0.80 rather than halved means more nearly equal
INDEPENDENT terms in one average, so the sum piles up harder. What
makes a field rough is the SHARE of its variance the fine octaves
carry, and the number that reads that is its level set's dimension and
never its spread.

**The octave count is read off the TOWN's own size**, because the
finest octave is one block and no finer: past the block grid an octave
is noise a plan cannot express, and a hamlet of four blocks has one
scale and should have one lump. A 537 m town runs to six octaves, a
170 m one to four and a 23 m one to one. It is this file's own "every
term is capped by the planet's octave count" rule arriving at a city.

**It only ever takes demand AWAY, and that is the whole of what keeps
it affordable.** `Site::level_r` is `edge(bearing) + APRON` and the
ground a town levels follows it, so a grain that could ADD demand
would put blocks outside that plateau, which is the buried suburb this
file measured at 1.13 m once already. Subtracting leaves the built set
a SUBSET of the star shaped region the outline bounds, so
`Site::level_r`, `field::site_skirt`, `town::WOBBLE` and the planet's
own slope bound are exactly what they were, not one chunk is contoured
differently, and **the atlas needs no re-bake**: what it stores is
where a town STANDS and `lay` derives its grid again in milliseconds.
`a_towns_middle_is_never_bitten_through` measures both halves on
sixty four seeds and three sizes: nought points outside the outline,
and the least demand anywhere inside 0.645 of the edge is 0.0010.

**And a town always has a SOLID MIDDLE, which is arithmetic rather
than a coincidence.** The bite is scaled by `1 - want`, nought at the
middle and one at the rim, so it can only take a block out where
`want < GRAIN / (1 + GRAIN)`, which is 0.645 of the way out to that
bearing's own edge. A city's core is solid and its fringe is shredded,
which is what the density of a real built up area does as you walk out
of one; a uniform bite would eat the towers at the same rate and leave
a town that is merely thinner everywhere, and a one block hamlet would
be shredded to nothing.

**And `plot` reads the same bite**, which is the divergent path rule
arriving at a town's empty ground: what FRAYS a town's edge and what
THINS the ground inside it are one number now (`shape::bite`) rather
than a fractal outline round a coin toss. A suburb still has its own
base emptiness, because what makes a suburb a suburb is the space, but
the holes in it are the shape of the holes in its own edge.

**And the jitter is a TOWN's own and not a suburb's.** `plot` moved a
lot by `SUBURB_SETBACK` everywhere, which is a suburban house standing
back from its own street applied to a terrace; `TOWN_JITTER` is what a
town proper gets, and every offset is still bounded by the room the
lot's own block leaves it (`Kind::covers`), so nothing stands in the
street whatever either number says.

**A street runs where somebody built and nowhere else.** The grid used to
be laid over the whole disc whatever the town came out as, so a town's
paving was a perfect circle with the town's own ragged outline hidden
under it. `streets_of` lays a piece along a block's frontage only when
that block or its neighbour carries a lot, so the network takes the
town's shape for free and the suburbs get the sparse roads they should.

**And a block says WHICH of its four sides it fronts.** Every built block
fronted all four at first, which downtown is right about, because its
neighbours front the same street from the other side and the four merge
into a grid. A lone suburban house has no neighbours, so it stood in a
square ring of its own tarmac: a moat, which the picture showed at once
and the counts did not. The port went from 2,564 pieces of street to
1,540 for the same 249 lots.

**Then it fronted the side FACING the middle of town, and the street on
that side RUNS ACROSS THE WAY HOME.** A house far out along east fronts
west, which paves a NORTH SOUTH street beside it, and nothing on that
street leads west: every suburban house came out at an isolated
rectangle of tarmac joining nothing, which is the owner's second
picture of the same suburb. `faces` fronts the side whose street runs
ALONG the axis the middle of town is down, and `home_run` then paves
that street all the way IN, as an L: out along the axis the block stands
furthest down, then in along the other. The union of those over every
built block is the collector network a suburb hangs off, with the two
axes through the middle as its trunks, and in the dense core it merges
into the grid that was there anyway.

**A road that serves one house and joins nothing is not a road**, and
that is a test rather than a picture.
`every_house_is_on_one_connected_road_network` floods the paving from
the middle of town on a grid half a street wide and holds every lot
within half a block of reachable tarmac: **0 stranded**, against 1 on
the same fixture with `home_run` neutered.

**A road grows VILLAGES along it.** `road::waysides` walks each road's
own line and drops a settlement wherever it has run `EVERY` (25 km, about
a day with a cart) since the last one and the ground will take a town.
It is the owner's ask and it is also the only honest order, because a
road has to be routed before anything can stand beside it: they come
AFTER the cities in the list, so every road's own `from` and `to` still
name the towns they named. Their size is the same coastal law cut by
`WAYSIDE` (0.42), since a place that grew because the road goes past it
is a village whatever its shore. This body's 267 roads grew 544 of them,
against 160 cities.

**And a survey is filtered to ONE direction, which is this file's oldest
performance rule arriving at its newest caller.** The wayside pass asked
`settle` of a planet carrying all hundred and sixty town sites, and every
one of its forty nine marches walked all of them: the bake went from
nineteen seconds to three hundred and seventeen. `Planet::around` at the
candidate's own direction leaves nought or one, and it is 23.8 s.

**And the levelness test moved to ACCEPTANCE.** A site has to be level
across the town that will actually stand on it, and which town that is
depends on its rank, which depends on what has been accepted already; the
scan cannot know it. It is also far cheaper, because the scan asked six
`surface_radius` marches of all twenty thousand candidates and the
acceptance loop asks them of the few hundred it looks at: the bake is
19.0 s against 23.7.

**A town's ORDER is what put every city on a shore, and the window was
never the thing.** Candidates were sorted by height and the first hundred
and sixty taken, so the lowest hundred and sixty won: measured, all 160
towns stood between 3 and 35 m over the sea on a body with eight
kilometres of relief, which means the ceiling this window was widened to
had never once applied and the interior of every continent was still
empty. `town::in_order` is the rule now: the PORT is the lowest ground on
the body, because a port is the town this game is about, and every other
town is taken on a HASH of its own candidate index. The spiral's own
order is no better than the height's, because its index is monotone in
latitude by construction and its first hundred and sixty are a cap round
the north pole; a hash is an even sample of whatever qualified. The towns
span 3 m to 2,332 m now, with a quarter of them over 820 m up, and 32 of
the 160 stand on an ISLAND rather than a continent, 5 of those on one
under a thousandth of the body. `towns_stand_inland_and_on_islands` holds
all three.

**A planet's sites are filtered once per CHUNK, never per sample.**
`Planet::around` keeps only the sites whose levelling can reach a chunk,
and `surface_blend` is asked for every one of a chunk's seven thousand
sample points: walking 160 towns in it is a million tests for a chunk
nowhere near a town. It is what makes a planet with cities all over it
cost a chunk what a planet with eight does, and `flight.rs` was already
doing it by hand for its own sweep.

**A town is BUILT when the EYE comes near it**, which is `city::stream`,
and it was picked once at startup. Every town on the body is planned
from the first frame: it levels its own ground in the planet's field and
the chart paints it. What streams is the BUILDINGS, one town raised on
a worker at a time and one dropped a frame, which is the rule `stream.rs` keeps for
the ground and `lamps.rs` for the lights. The owner read the gap off
two pictures side by side: a chart with a hundred and sixty cities on it
and ground with nothing standing on it, and driving to the next town
arriving at an empty field.

It cost a split that was owed anyway. `World` is behind an `Arc` the
mesher's workers hold and it carried the collision boxes, the lamps and
the built list, all of which change now. `Fabric` is those three, PER
BUILT TOWN rather than one flat vector with ranges into it: a range
cannot be taken out of the middle without moving every range after it,
and a town going out of range does exactly that. The PLANET never
changes, so not one chunk is remeshed when a town arrives, which is the
whole reason sites are planned for every town from the start.

**Every index that names a town is the PLANNED one.** A lamp is its town
and its place in that town's own list; a crowd and a theft read the
built set every frame and skip what is not in it. A slot in the built
list would name a different thing the moment a town went out of range.
A lamp inside a town is keyed by its TILE and its place in that tile's
own list (`PER_TILE`, 4,096), because a tile's lamps come and go with
its boxes and a place in a town wide list would shift under them.

**And a reach bounds the count, which the count cannot bound itself.**
`TOWNS_REACH` is 200 km: without it the nearest eight are built however
far off they are, which is eight cities' triangles held for a view of
open ocean.

**A distance to a town is along the GROUND**, the angle between two
directions times the radius. Written as the straight line to a point on
the MEAN radius it adds the eye's own altitude to every distance, and
the town under the walker's feet read as 1,113 m away. That is the
`cars_near` mistake of the commit before it, in a new place, which is
what makes it a rule rather than a slip: a position multiplied by a
radius it already carries is not a distance to anything but the centre.

**And a road is ON THE GROUND now**, levelled into the planet's own
field and paved with tarmac that streams, which is the section on the
roads below.

Measured: 160 cities and 544 roadside villages planned and BAKED in
23.8 s (20,000 candidates), 704 settlements from 180 m across down to 23,
8 of them built into 2,326,024 triangles, 121,329 collision boxes and
1,292 lamps in 367 ms. The same eight at one size were 2,836,252
triangles, so a world of cities and villages costs a little LESS than a
world of identical towns while carrying a city two and a half times the
area of any of them. A chunk near the port is 17 ms on lavapipe against
12 before, which is what a site that cuts costs: `surface` can no longer
return a level without asking the relief what was under it.

## A city is BLOCKS of four by four lots, and a settlement has a TIER

`docs/mockups/city-blocks.html` is the record and `town/plot.rs` is it
ported, once the owner approved the page. A block was ONE building on
a `BLOCK` of 10 m with a street on every side of it, which put 27% of
a town's ground under buildings against the 40 to 55% the six real
plans on that page carry; a block is `SIDE` (four) lots of `LOT`
(10 m) a side now, 40 m across, and `PITCH` is that and the street.
Nothing else about the grid moved: `streets_of` lays its runs on the
same lines, the traffic reads the same lines, and `PITCH` is still
one number said three ways.

**What stands on a block is decided twice, and the second time is
lot by lot.** The block's zone is the demand field at its middle, as
it always was; then:

- **Downtown and whatever faces the SQUARE fills its block with LARGE
  buildings of two by two lots**, each a corner on two streets, 20 m
  square and built PARAMETRICALLY (`model::building` at the lot's own
  footprint), because Blender is not in this container and a bake is
  one size. `Library::model` reaches for a bake only for a lot of one
  `LOT`, and `Kind::baked` says which kinds have one.
- **Everywhere else the PERIMETER builds and the interior is a
  COURTYARD**, twelve lots round a two by two of back land, because a
  lot with no frontage is a lot nobody can get to. A suburb is the same
  ring at `SUBURB_FILL` of its lots.
- **A lot's DOOR faces the street it fronts.** `Lot::yaw` turns the
  lot's frame so a house on the north side of a block opens north, and
  `Frame::turned` is the one place a frame is turned. A building drawn
  with its door on its own south wall would otherwise open onto the
  courtyard on three sides of every block.
- **And the GRAIN thins the ring lot by lot** (`shape::bite` at the
  lot's own place), so a town's edge is ragged at the scale of the
  thing that is laid. `grain_octaves` is keyed to the LOT now and not
  the block: keyed to a 48.5 m block a 537 m city fell from six octaves
  to four and its outline measured 1.034 against the oval's 0.987,
  which is a picture nobody could tell from the oval.

**The market SQUARE is one piece of paving with no street through
it.** `square_of` is the mockup's own ladder (none under 120 m, one
block under 300, two under 620, three past that); its cells carry
`fronts::SQUARE` and nothing else, `streets_of` lays no run between
two of them, and `square_piece` emits the whole of it as a `Piece`
with `arm::SQUARE`: a raised slab at kerb height, a stone plinth in
the middle and a lamp in from each corner. The blocks sharing an EDGE
with it build big, which is the mockup's civic and commercial
frontage. `road::crossing` skips it, because a slip arriving in the
middle of a square is a highway arriving in the middle of a square.

**And a settlement is one of THREE, which is the owner's own list.**
`Tier::of` reads the settlement's SIZE, which is how near the sea it
stands (the next section), so the three are one law:

| tier | from | what it is |
| --- | --- | --- |
| `City` | 380 m | towers and offices downtown, houses and shops of one to three storeys on the streets, suburbs, a square of two blocks |
| `Town` | 180 m | a downtown of one and two storey SHOPS round a square of one block, houses everywhere else |
| `Village` | | houses and nothing else: one storey, no square, no big building |

The thresholds are the mockup's own size table, read against the real
plans. `Kind::Shop` is the one kind this needed: one to three storeys
under a flat roof in brick, concrete, stone or vinyl, on one lot or on
four, and never baked.

**Three things moved with the block, each measured rather than
guessed:**

- **The APRON is half a block and a street** (28.5 m, from 12), which
  is how far the far kerb of the outermost street can stand past the
  middle of the outermost block. At twelve the first street laid past a
  town's edge stood on bare relief 8 cm over the level, which the kerb
  test found as a walker standing 0.080 m up on a carriageway.
- **Every built block fronts all four sides**, which is the mockup's
  rule. A suburb block fronted one side once, because a block was ONE
  house then and a lone house in a ring of tarmac was a moat; a ring of
  a dozen houses is a block, and fronting one side left three of its
  sides with houses on them and no street.
- **The fixtures grew.** The test ball's level ground stands three
  times the size law's reach from its own sea, so every town it grows
  is at the law's floor, a third of `biggest`, and at sixty metres the
  port came out as one block with nothing on it: 250 m where the tests
  need a town with structure, 300 where they need one with an outline.

**What is MISSING, named rather than hidden.** The mockup's ARMS, a
town running out along the highways that arrive at it, are not built:
they want a `Site` that carries its road bearings and an atlas re-bake,
and the highway already merges at the edge (`road::slip`), which was
the half of that page the owner corrected. Nor are its street RANKS
(avenues wider than streets) or its ribbon of shops along an arterial.
A courtyard is bare ground, and a square is a slab with a plinth on it.

## A city is BIG because it is NEAR THE SEA, which is a distance and not a height

`town::coastal` read a town's HEIGHT over the sea as a stand in for its
distance from it, on the reasoning that the continent term is a shelf
and low ground is the coast. Measured on the atlas it was not: with
`town::CUT` capping how deep a site may cut, the flattest big sites
were inland basins, not one of 521 settlements had open water within
three kilometres, and the port's own nearest sea was 15.7 km off. The
owner's ask is big cities ALONG THE COAST, which is the thing itself.

`town/shore.rs` is the thing itself: an equirect mask of the bare
body's water, 1024 by 512, and a Dijkstra out of every wet texel over
its eight neighbours with the true ground step at every latitude, so
`Shore::distance` is metres to the nearest water anywhere on the body
for an index. Half a million analytic samples and under a second,
against a march per bearing per step at twenty thousand candidates.

**`COAST` is a share of the RADIUS, and the share was measured.** It
is where a town is halfway down to the smallest, and the bake prints
what it is set against: the body's LAND stands a median 31 km from its
sea, so three hundredths (30 km) puts half the interior nearer the
halfway point than the median and half further, which is what spreads
the sizes. At a tenth the bake came back with **199 settlements and 97
roads against 521**, because nearly every site was asked to be a city
and a city needs a plain; at three hundredths it is **832 settlements
(160 cities and 672 villages) and 365 roads over 60,651 km joining 153
of the 160**, and the port stands **6 km** from the sea.

The fixtures moved with it, because the test ball's level ground is a
tenth of its radius from its own water: every fixture town is at the
law's floor, and `biggest` is what sets their size now.

## A car BURNS FUEL, a tank is ten dollars, and a jerrycan walks back

`fuel.rs` in the core is the owner's numbers and nothing else: a
thousand dollars to start (`START_CASH`), eighty kilometres on a tank
(`RANGE`), ten dollars a fill (`TANK_PRICE`). A `Tank` is a SHARE of a
full one and never litres, because nothing here has a litre in it:
what a car has is a distance.

**A car burns the ground it MAKES**, `Driver::gone`, which is the arc
its wheels actually swept and never its speed integrated: a car wedged
against a wall with the throttle held burns nothing, and one coasting
down a mountain burns every metre. A dry tank is a dead engine: the
throttle does nothing either way, and the wheel and the brake still
work, because a car that has run out of fuel is still a car rolling.
`a_car_burns_the_ground_it_makes_and_a_dry_one_will_not_pull_away`
holds the burn to the distance within two per cent.

**A car taken off the street has whatever was in it**, `Tank::part`,
three to nine tenths off the agent's own hash, because nobody parks
full and a car stolen dry is a walk.

**G is gas**, one key for the three things a player does with it: at
the wheel, stopped (`STOPPED`, 1 m/s) within `PUMP_REACH` (14 m) of a
forecourt's middle, it fills the car; on foot within a stride and a
half of that, it buys a JERRYCAN; on foot beside a car the player
owns, with a can in hand, it pours the can in. The purse and the can
are the app's (`fuel.rs` there, `Wallet` and `Jerrycan`), because
they are the player's; the prices and the burn are the core's. The
dash reads the tank, its range, the purse and the distance to the
next pump, and a walker's line reads the purse and whether a can is in
hand.

**What is MISSING, named rather than hidden.** Money buys fuel and
nothing else yet, nothing is saved, and the traffic on the rails burns
nothing, because a car on rails is a function and a function has no
tank.

## A GAS STATION is a fact about the ROAD, and it streams with it

`road/station.rs` is where one stands and what one is built of.
`station::plan` walks a road's own line and drops a station on the
first piece that will take one once it has run `EVERY` (35 km) since
the last, the first `FIRST` (8 km) out of the town the road leaves,
alternating sides. A piece takes one when tarmac is laid at both ends,
the corridor is cut (so a slip over a town's own plateau never carries
one), no lamps stand on it (so it is in the country), it is longer
than the forecourt, and it is nearly LEVEL: half a per cent, because a
forecourt is one flat slab at the piece's own middle height and over
its 28 m the ground drifts by 7 cm at that grade, which the slab's own
2 cm over the tarmac and the tarmac's 15 cm lift keep it clear of.
Thirty five kilometres is under half a tank, and a car that left town
on a stolen car's own three tenths (24 km) reaches the first one.

**The forecourt is parametric**, which is this project's default for
new geometry: a slab 11 m deep and 28 m long on the verge the corridor
already levels (the shoulder is 3.45 m off the centreline, so the slab
ends at 14.45 inside the corridor's 16), two pump islands with two
pumps each under a canopy on four pillars, a kiosk that is a
`Kind::Shop` turned to face the pumps, and a lit sign at the road's
edge. The pumps, the islands, the pillars and the kiosk's walls are
`solid`, so a car is stopped by a pump the way it is stopped by a wall
and a walker walks up to one; the canopy and the sign are trim.
`Model::place` sets one model down in another, turned about the up,
with its triangles, its boxes and its lamps carried together: it is how
the kiosk goes on the forecourt and the forecourt on the stretch.

**It STREAMS with the stretch.** `Course` carries the road's stations
and which piece the course starts at, `ribbon::stretch` sets each
forecourt down at the middle of its own piece turned to the road, and
`roads::lay` puts the model's boxes into the `Verge` beside its lamps,
where `Fabric::underfoot` reads them with the towns' walls: one list a
body is stopped by, and nothing that has to know a pump from a wall.
`Network` keeps every forecourt's middle so the dash can say how far
the next pump is and G can ask whether one is within reach, and the
log says where the first one out of the port is so a picture can be
aimed at it.

**What is MISSING, named rather than hidden.** No station stands in a
town, where a car would want one most; the corridor is not widened for
a forecourt, which is why the slab is sized to fit the verge; and a
station on a road that is all hills has nowhere to stand, which the
test holds by building one.

## A SLIP'S RAMP dragged it under the ground, and the census miscounted its last piece

Two numbers off the first picture of the port as blocks, and each was
measured on the PRE-CHANGE atlas too before it was called a defect of
this change: neither was. Both are in the harness's own `report`, which
is the whole reason it prints what it prints.

**The slip was 0.12 m UNDER the drawn ground at piece 5 of 15, where the
town's plateau weighs 0.93**, and on the older atlas 0.20 m at piece 16
of 32 at 0.99. `road::slip` is anchored at the highway's own height,
which is `ribbon::LIFT` over its corridor, and its first point wants
`LIFT + SLIP_CLEAR`: the step is half a metre DOWN, and `grade` ramped
that over the whole slip, so a point a third of the way along was still
being pulled 0.33 m down against a lift that, most of the way into the
plateau, is 0.09. The ramp has a FLOOR now, the street's own five
centimetres over the ground the point stands on, which is the least any
tarmac here stands over what it is laid on. Both ends are exactly where
they were: the mouth's own point is the highway's and the crossing's is
the street's.

**And the grade census counted the TAIL's join piece as the highway's.**
A piece with either end in a slip is the slip's; the census read `k <
slip.0` at the head, which catches the join piece there, and `k >= len -
slip.1` at the tail, which does not, so the one piece from the highway's
last point to the slip's first at every road's far end was reported as
highway: 28 pieces at up to 26.2% on this atlas and 20 at up to 49.7% on
the last, on a body whose highways `road::smooth` holds to seven. It is
`k + 1 >= len - slip.1` now, which is the same rule at both ends.

Measured after both, on the same report: **nought of 734,808 pieces on
the highways themselves are over the seven per cent, and the port's slip
stands over its ground everywhere by its own lift, -0.05 m at the
worst**, at the crossing end where the plateau weighs 1.00.

## Two highways on ONE TRUNK are one road, and the trunk carries the higher profile

The owner's picture was two highways overlapping out of the port
without merging into one geometry, and the second was the consequence:
"I get stuck instantly as I drive down the road". Both are one fact
about the router. Every road on a body comes off ONE Dijkstra tree
(`road::route`), so the roads leaving a town run on the same chain of
waypoints until their routes split, and each was then laid on its
own: its own tarmac, its own corridor, its own mound and its own
profile, three and four deep on the trunk out of a city. Measured on
the atlas: **727 of 730 road ends stand on a trunk shared with another
road, 1,573 pairs of roads share waypoints, the deepest share is 23
waypoints (230 km), and all four of the port's own roads share their
first two**.

**The profiles DIFFER, and that is the wall.** `road::smooth` envelopes
each road over its own whole length, so two roads surveyed over the
same ground on the trunk carry two profiles there, metres apart
wherever one of them had a hill of its own to climb toward. The field
takes the nearest arc's level, and two arcs a metre apart at two
heights is ground that steps between them: a car driving the trunk met
a wall in the road.

**The merge is at LOAD, in `road/trunk.rs`, and the atlas is untouched.**
The file keeps each road's own line and profile; `trunk::merge` decides
who owns what once the centrelines are laid, and `world::lay_routes`
runs it before a single corridor is cut. The lowest indexed road owns
a trunk. Every later road standing within `SHARE` (4 m, a lane and a
half: where two carriageways overlap) of an earlier one for at least
`LEAST` (three) stations is SNAPPED onto the owner's line there and
CLOSED: no tarmac, no corridor, no mound and no lamps, because the
owner's are there, and no slip of its own at an end that is on a
trunk, which `Route::shared` says. A road forks off through the
owner's corridor and its tarmac is held `FORK_LIFT` (3 cm, a surfacing
overlay) over the trunk's where the two overlap, so the two sheets do
not fight for the depth test. `docs/civil-engineering.md` carries the
table.

**The trunk's profile is the HIGHEST of the roads on it, and that is
the envelope's own rule arriving at a junction.** The first cut pinned
a sharer DOWN to the owner's profile. A road only ever rises, so the
sharer then had to climb back to its own baked profile past the fork,
and its profile was raised precisely to hold the grade toward its own
high ground: the whole climb landed on the one piece past the fork,
and the steepest fork on the body came out at **103%**. So the owner is
raised to the sharer wherever the sharer stands higher, eased along its
own length at `STEEPEST` by the very envelope `smooth` bakes with
(`trunk::envelope`, one function with two callers now), and the sharer
is pinned to it. A raise on one road is a demand on every road it
shares a trunk with, so `merge` carries the profiles round until
nothing moves by a millimetre.

**And the raise walks the pins HIGHEST ROAD FIRST, or it never
settles.** An owner's point can itself be a pin on a road below it.
Raised in road order, a demand road C put on road B's pinned point was
reset by B's own pin to A before B's pin could carry it down to A,
C's own envelope raised it again the next pass, and the three chased
each other to the cap: **64 passes and four shared pieces left at up
to 47.5%**. Every owner has a lower index than its sharer, so walking
the pins downward carries a demand the whole way down its chain in one
pass, and `a_demand_on_a_pinned_point_is_carried_down_to_the_road_that_owns_it`
holds it with a road that forks off a road that forks off the owner.

**What a JOIN is not, is a dead end.** The harness's census counted
every place a road's tarmac stops as a mouth left bare, and a road
joining a trunk stops laying tarmac: 2,027 mouths with the worst 98 km
from any paving, which was a fork in open country and not a town
anybody had left a road short of. `Route::trunk` marks the points that
stand on another road's trunk or fork off it, and the census reports
the joins on their own.

**What is MISSING, named rather than hidden.** A road that climbs off a
trunk lifts the trunk with it through the fork, which is embankment on
the lower road through the junction and is what a real merge is built
on; it is not a ramp beside the trunk, and there is no widening, no
give way and no fan of tarmac where the two carriageways part, so the
fork's own tarmac is two sheets a lip apart and the mound is the
owner's. A CROSSING, two roads meeting at an angle for fewer than three
stations, is left exactly as it was, which is the junction the roads
section already names as missing.

**And a point is pinned ONCE.** A run's fork zone was extended while
the point was inside any owner's corridor, so it ran straight through
the next run on the same road and pinned that run's interior a second
time with the fork's lip: a point pinned once at the trunk's height and
once a lip over it asked its owner for the lip every pass, and the
profiles rose three centimetres a pass to the cap. A fork zone stops at
the neighbouring run now. And a raise is spread over the owner's
segment by the interpolation's own weights, because raised at both ends
by the whole difference a pin standing at a station lifted the next
station too and the next pin lifted it again: the fixture's ten point
fork came out **two metres over what any road on it stood at**, and
`a_road_that_leaves_a_trunk_and_comes_back_is_pinned_once_at_every_point`
holds both.

Measured on the harness body, one binary either side: **350 of the 365
roads stand on another road's trunk for 78,182 points, the longest
trunk is 226.7 km, the profiles settle in 8 passes, and the steepest
piece touching a shared point climbs at 7.0%**, which is the grade and
nothing over it; **nought of 718,864 pieces on the highways themselves
are over the seven per cent**, against four at up to 47.5% and one at
103% on the way here. The body cuts **633,198 corridor pieces against
708,185**, lays **10,377 stretches of tarmac against 11,664** and
carries **1,621 gas stations against 1,757**, which is the trunks no
longer cut, laid and furnished three and four times over. The census
reads **581 joins and 1,446 bare mouths through settlements with the
worst 124 m from any paving**, against 2,027 mouths and 97,890 m with
the joins counted among them.

## A building is built of a TRADE, and a town is not one grey

Every house and every office wore `CONCRETE`, so a suburb and a downtown
were the same wall at two heights, and the owner asked for the trades'
own list instead: a house out of wood, red brick or vinyl and an office
out of red brick, concrete, marble, glass or stone blocks.

**It is a TABLE and a hash into it.** `model::Kind::skin` is the whole
of the rule, which is this file's own "open for extension" rule: a new
trade is a row and a set, not a branch in a builder. A hangar is neither
a house nor an office and keeps its concrete. It is the SEED and never
the lot's place, because `city::stream` builds a town as the eye comes
near it and drops it again, and a skin picked off anything that streams
would change colour every time you drove back into town.

**A baked model is RE-SKINNED on the way out of the library.** The
buildings are thirteen Blender bakes with three LODs each and they are
authored in concrete; which trade a building is actually put up in is a
fact about the LOT and not about the variant, so a skin baked in would
mean sixty five bakes. `Model::reskin` repaints the mesh AND the solids
together, which is this file's own rule that a wall is one oriented box
that is drawn and collided from one set of numbers: a repaint that moved
only one of them would be a wall that looked like brick and answered
concrete to whatever asks what a body is standing on. The parametric
fallback builds its skin in directly, because it is making the walls
anyway and knows a wall from a FLOOR: a slab is poured concrete whatever
is hung off the outside.

**The sets are `tools/make_building_textures.py`, and `tools/texkit.py`
is what both generators are made of.** Material Maker does not export in
this container (the asphalt note below is the long form), so a generated
set is the source; the hashes, the tileable noise, the worley and the
normal off a height were written out twice the day there were two
generators, and they are in one file now, with `make_asphalt_texture.py`
proved byte identical across the extraction by its own `--check`.

**EVERY NUMBER IS IN METRES FIRST.** A built thing's tile is three
metres and the map is 1024, so a pixel is 2.93 mm of wall. A brick is
215 by 65 mm with a 10 mm joint, a clapboard's reveal is 200, a board is
150, an ashlar block is 500 by 300, a curtain wall's bay is 1.5 m: those
are the sizes and the cell counts are what they come to on a three metre
tile. A texture authored by eye comes out at whatever scale it came out
at, and a wall of it reads as a doll's house or as a cliff.

**The finest octave is 256 CELLS and no finer**, which is four pixels
each. Past that an octave is noise under the sampler rather than detail,
the mip chain throws all of it away, and what is left in the normal map
is speckle. The first cut ran octaves out to 1,760 cells, which is under
two pixels, and the stone came back as gravel.

**A NIGHT is what found the normals, and the number is the mean LEAN.**
Measured as the angle a normal stands off its own surface, asphalt was
86.3 degrees against concrete's 21.1 and hull plate's 13.8: a chip's
dome is a sharp worley cone, so nearly every texel of a road stood on
end. Under the sun that reads as coarse gravel and is arguable; under a
STREET LAMP a few metres up it is a field of white specular glints, and
the first picture of the port at midnight was a road covered in them.
That is the mockups' own lesson about a lamp a hand under a ceiling
lighting every grain of a normal map at a grazing angle, arriving
outdoors. The cure is `texkit::soften`: the normal is read off a LOW
PASSED height and the map keeps all of its own detail. Every set is
measured against concrete's 21 now: asphalt 27.0 (it was 86.3), brick
33.3 (55.2), stone 28.0 (44.1), vinyl 19.1, wood 16.5, marble 5.0.

**A whole tower of the PANE material vanished against the sky.**
`field::GLASS` is what a window is drawn with, a flat dark colour at a
roughness of 0.08, which is a mirror; the first render of an office
skinned in it reflected the sky so exactly that the building was not
there, leaving its own floor slabs and window frames hanging in the air.
A facade is a GRID, so `CURTAIN` is a set of its own: dark panes in a
grid of aluminium mullions, the mullions METAL in the ORM's own blue
lane so their reflection takes the frame's colour and the pane's does
not. The owner's word was glass and what they meant was a glazed facade.

**Two lifts on the brick and one on the wood, both read off a picture.**
Linear albedo is what a shader does arithmetic in and it is not what an
eye reads: brick at a mean of 0.083 came back BLACK beside concrete's
0.449, and cedar at 0.075 to 0.165 came back as charred timber with
white speckle. A fired red brick is a fifth to a third in red with a
third of that in green and stained cedar is much the same, which is sRGB
0.44 to 0.58: brick is 0.130 now and wood 0.143. And a knot is 60 mm on
a three metre tile; the first cut put five cells across the whole map,
which is a 600 mm knot, and the wall came back blotched like scorched
plywood.

## A town is INHABITED, and a townsman is on rails

`traffic.rs` in the core says who is out on a town's streets and where
they are at a moment; `traffic.rs` in the app draws the few dozen near
the eye; `figure.rs` is what a person and a car are made of.

**An NPC and a car are ON RAILS**, which is this file's own rule for
everything that moves and is not the player. A townsman's place is a
closed form function of his town, his own index and the world clock:
nothing integrates, nothing is saved, two clients agree by construction,
and a town nobody is near costs exactly NOTHING, because the function is
never asked. 160 towns carrying tens of thousands of people is a world
that costs what an empty one does until you land in it, which is the
lamps' rule ("a light near the eye and a number everywhere else") on the
other thing a city is full of.

It also makes spawning by distance seamless for free. An entity that has
just appeared is exactly where it would have been had it existed all
along, because there is no state to carry across the boundary.

**The streets are a GRAPH, derived from the paving that is DRAWN.**
`town::streets_of` lays its pieces on the lines of the block grid, so a
crossing is a node, a block's frontage is an edge, and `Streets::of`
reads both off `town.pieces` rather than off the plan that made them.
That is a wall's own rule again: traffic rides the tarmac a player can
see, so a street nobody laid can never have a car on it and the two
cannot drift. No `HashMap` in any of it, because this is replayed state:
the edges are a sorted vector and a lookup is a binary search.

**A circuit is not SEARCHED for, it is FOLLOWED.** `Streets::next` is the
classic face traversal of a planar graph: arrive at a node and leave
along the next street clockwise from the way back. The successor is a
PERMUTATION of the directed edges, so every orbit of it closes and the
orbits partition them, which `every_street_is_on_exactly_one_circuit`
measures both halves of. On a street grid an orbit is the way round one
block, or round a group of them where the grid has gaps, or the outside
of the whole town. A dead end has one way off it, which is the way back,
so a face walks up a cul de sac and turns round, and so does a car.

**A lane is the face offset to its RIGHT and FILLETED at every corner.**
Neither offset is a number of its own: a car rides the MIDDLE OF ITS LANE
(`LANE * 0.5`, 1.375 m) and a person the middle of the pavement
(`LANE + WALK * 0.5`, 3.5), so both are read off the street's own cross
section and cannot drift from the paving they are drawn over. Oncoming
traffic passes on the left and a pavement is the outside of the street,
measured: the two ways down one street are 2.69 m apart.

**And a person steps DOWN off the kerb where he crosses a street, and
nowhere else.** Along a run the pavement is the only ground there is, so
he is always up on it; at a crossing his own lane takes him over the side
street's band, and that band is carriageway exactly when the arm is
there. `Traffic::lift` asks `town::paved` the same question the mesh
asks, so a foot and the concrete under it cannot disagree. Measured on a
plain grid, a 118.8 m walk is 96.6 m on the kerb and 22.2 m crossing. The fillet is the part worth the code. An agent walking the
offset polyline straight would turn ninety degrees BETWEEN TWO FRAMES,
which reads as a car teleporting round the corner, so a corner is a
quarter circle of `CAR_TURN` (1.15 m) or `FOOT_TURN` (0.7) and a dead end
is a half circle round the end of the street at the lane's own offset.
Measured over 2 cm of travel: the place is out by 6.8e-7 m and the
heading turns by at most 1.6 degrees.

**Nobody ever leaves the street, and that is a test rather than a
picture.** `nobody_ever_leaves_the_street` samples every agent every
tenth of a metre all the way round its own loop and holds it inside the
corridor of some street the town actually paved: the worst is 3.673 m
against a half street of 4.25. That number is understood rather than
observed, which is the difference between a bound and a coincidence: a
RIGHT turn's fillet bulges by `lane + radius * (1 - 1/root 2)`, which is
3.5 + 0.205 on foot and 1.375 + 0.337 driving.

**And NOBODY steps off the paving at all**, which
`nobody_steps_off_the_paving_at_all` measures at 0.000 m. A run used to
reach only as far as a crossing's own centre line, so the square of
tarmac where two streets met was covered from the sides a street arrived
on and no further: at a four way crossing that is all four quadrants and
nothing could leave it, and at an L BEND the far quadrant was bare and
the lane turning LEFT crossed it, because keeping right round the outside
of a bend is what the far corner IS. The ground a town stands on is
levelled flat, so what that looked like was somebody cutting a corner
over a verge, and it measured 1.62 m. A crossing is a PIECE of its own
now, paved whole whatever arms reach it, which is the section on the
cities above.

**A person and a car are PARAMETRIC**, in `figure.rs`, which is
`model.rs`'s rule for the things that move: a procedural game's default
for new geometry is procedural. A townsman is three parts, a body and two
legs that swing on their own hips, and a car is one. Three and not five,
because at the size a person is drawn on a street the silhouette that
says WALKING is the gap between the legs, and an arm that swung would be
two more entities each for something nobody can see.

**A gait is measured in METRES and never in seconds.** A leg swings on
`along / STRIDE`, which is how far the figure has actually come, so a
thing that has stopped has stopped its legs too and nothing needs a clock
of its own.

**A figure wears its colour in the VERTEX.** Eight tints times four parts
is thirty two meshes built once at startup, against a material per agent
spawned and dropped every time somebody walks past. The core hands out a
material BYTE and the app owns the palette, which is this file's own
crate split: what a thing IS belongs to the core and what colour it is
belongs to the app. None of the tints is near white, and the first render
is why: at this camera's exposure a coat at 0.72 comes back as a
MANNEQUIN and a street of them reads as a shop window.

**The set of who is DRAWN has hysteresis, and the lamps' does not need
any.** `light_lamps` holds its set still by not looking again until the
eye has gone four metres, which works because a lamp does not move. A
townsman walks across the boundary on his own, so without it the thirty
second and thirty third nearest swap places every few frames and
somebody forty metres off blinks in and out. Somebody already out is
ranked at `KEEP` (three quarters) of his real distance, so a newcomer
has to be a third nearer to take his place.

**A crowd belongs to ONE body**, and flying to the next planet despawns
it rather than redrawing this planet's townsmen against that one's radius
and centre. And it is turned out on the BUILT towns only, because a
person walking a street nobody has laid the buildings of stands on a bare
levelled plateau, which is the gap a far city already has.

**AND THE ROADS BETWEEN TOWNS CARRY A FEW CARS**, which is the owner's
ask and is the same rule one level up. `road::commute` is a car out in
the country: `plan` puts one every `EVERY` (10 km) of a road, hashed off
the road's own index, and `spot` says where it is at a moment. This body
turns out about six thousand of them over 63,840 km of road and draws
the nearest `MOST_ROAD_CARS` (twelve) within 450 m, so a road nobody is
near costs exactly nothing.

**Its place is counted in centreline POINTS and not in metres**, which
is what makes it a constant time answer: the points are `road::PIECE`
apart by construction, so turning a distance into an index would mean
walking a road's two thousand of them every frame for every car. What it
costs is that the one SHORT piece at the end of each ten kilometre
waypoint span is crossed a little slower, which is one piece in a
hundred and eighteen and is under the speed's own spread.

**They ride the middle of their own LANE**, `traffic::CAR_LANE` read off
the street's cross section rather than written again, so a car in the
country and a car in a street sit the same distance off the paint and
oncoming traffic passes on the left in both. Half of them come the other
way, which is what makes a road a road rather than a queue, and they go
at 27 m/s (97 km/h) against the player's 160, so they are something to
come up behind. `a_car_on_a_road_runs_its_length_in_its_own_lane`
measures the furthest one sits off the centreline at **1.38 m**, which
is its lane and inside the tarmac.

**And a commuter cannot be STOLEN**, which is named rather than hidden:
a `Theft` names its car by the town and agent it was, and a car on a
road is neither.

**What is MISSING, named rather than hidden.** A figure is
`Model::trim`, which only draws, so a walker passes through a townsman
and two townsmen pass through each other. A CAR is a box now
(`driver::car_box`) and stops the player, but only the player: nothing
on rails is stopped by anything. A body that stops another body
wants the walker to know about boxes that MOVE, which is a larger change
than a crowd on a street. Nor does anybody give way: a car crossing a
junction does not know the other car is there, because knowing would mean
state, and state is the one thing rails do not have. And a town that is
planned but not built has nobody on it, for the same reason it has no
buildings.

## A car can be STOLEN, and the one with you in it leaves the rails

`driver.rs` in the core is the car and `drive.rs` in the app is the
theft. **E** gets in and out, and it is one key because getting into the
car you are standing beside and getting out of the one you are in are
the same verb.

**A stolen car is not an exception to the rails rule, it is the rule.**
Every other car in a town is a closed form function of the town, its own
index and the clock; the moment somebody is at the wheel that stops
being true of it, and this file already says why: **what INTEGRATES is
the player, and only the player.** A car with a driver IS the player, so
it comes off the rails and integrates, and a car nobody is in goes back
to being a number. Nothing new was needed to say that.

**A car you GET OUT OF is a car that is still there**, which is
tenebris's longest open bug written down as a rule two sections below.
There is ONE `Driver` per theft whether you are in it or not, so there
is nowhere for the pose to be lost: getting out sets a flag, the car
keeps drawing where you left it, and walking up to it and pressing E
again reads back the same pose it wrote. Two poses, one for driving and
one for parked, is what that bug IS.

**And the agent stays stolen for good.** Putting it back would teleport
it to wherever the closed form says it should have got to by now, which
is a car jumping across the street the moment its driver walks away. So
`Thefts::stolen` is the list `traffic::near` skips, and it only grows.
A second theft is allowed and leaves the first parked for ever, which is
one `Vec` rather than a rule about how many cars a player may own.

**A car is glued to a surface, so it is a BASIS and not a quaternion**,
which is this file's rule for the walker and for everything that cannot
roll. What makes it a car rather than a fast walker is ONE thing: **it
steers with its wheels.** The yaw rate is the BICYCLE model,
`speed * tan(steer * lock) / WHEELBASE`, so it is proportional to the
speed and a car standing still cannot turn at all however hard the wheel
is held, which `a_car_standing_still_cannot_turn_however_hard_the_wheel_is_held`
measures at 0.0000 degrees over three seconds of full lock. The
wheelbase is READ off the model that is drawn (`figure::car` puts its
wheels 1.32 either side of the middle, so it is 2.64 m) rather than
chosen beside it, and the lock TAPERS with speed, because a car holding
full lock at 58 km/h would be cornering at four gravities.

**It is pushed out of walls by the same function the walker is**, with
its own OUTLINE rather than the walker's circle: `walker::resolve_body`
takes the ring of points a body has and the heights to test them at, and
a walker hands it twelve round its 0.35 m radius while a car hands it
its own four corners and four side middles. One circle could not serve
both: round a 4.1 m car it is two metres across and could not fit down a
2.75 m lane, and inside it the bonnet passes through the wall.

**A kerb is driven up and a wall is not**, which is the difference
between a road and a pavement to something with wheels. The car's own
test heights start at 0.45 m, well over a 0.17 m kerb, so a stolen car
mounts the pavement; `CLIMB` (0.3 m) is what refuses anything taller,
and a wall takes `CRASH` (0.15) of the speed off. Measured: a car driven
at a wall eight metres off stops at 5.53 m, which is eight less its own
2.05 m of bonnet, and the same car over a 17 cm kerb goes 72.9 m without
dropping under 16 m/s.

**And a HILL is neither, which took two rules that were the same rule
said twice.** The owner's "my car cannot drive up slopes" was both of
them:

- **`resolve_body` pushed a body out of anything it overlapped, and a
  hillside is something a body overlaps.** The guard was
  `push.length() < 0.25`, which is the SINE of the surface's own tilt:
  fourteen and a half degrees, where `walker::STAND` is fifty. A walker
  never met the difference, because a walker is 35 cm across and ground
  rising its own 60 cm step within 35 is a slope of 1.7, far past
  `STAND` anyway. A CAR is 4.1 m long, so its ring reaches 2.05 m
  forward and one in four was already a wall to it: measured, ten
  seconds up a one in two hill threw it **164 m back DOWN the slope**.
  It reads `STAND` now, off the surface's own normal, so a body is
  pushed out of exactly what it cannot stand on and drives and walks
  over everything else. That is the owner's "just like the player
  controller", and it is ONE rule rather than two.
- **And the rise a step was allowed was `CLIMB` alone, which is a rule
  in METRES that is really a rule in FRAMES.** At sixty a second a car
  covers 0.27 m and a one in two hill rises 0.13 under it; at the
  twentieth a software rasteriser runs at it covers 0.8 and rises 0.4,
  so the same hill the same car climbed was a wall. It is `CLIMB` plus
  `STEEPEST` times the ground the car actually made, and `STEEPEST` is
  `walker::STAND` said as a GRADE
  (`a_car_climbs_exactly_what_a_walker_can_stand_on` holds the two in
  step across that boundary). Measured, ten seconds up a one in two
  hill: 136.9 m in sixtieths and 137.1 in twentieths, against 64.8
  before.

**And a hill COSTS something now, which is the owner's own "torque vs
momentum".** Those two rules got the car over a hill and left it going
136.9 m in ten seconds up a one in two exactly as it does on the flat,
which is a car that does not feel a gradient at all. The owner's word
for what was missing is the right one: gravity along the way it is
going. `GRAVITY * sin(theta)` off the ground the car ACTUALLY made
(`rise / made`, so it is measured rather than assumed) is one term in
`Driver::roll`, and everything else falls out of it. `Driver::holds` is
the grade the engine exactly balances, `ACCEL / sqrt(GRAVITY^2 -
ACCEL^2)` = **0.677**, so anything under about two in three is a hill
this car climbs and anything over it is one it does not; and a descent
speeds it up, capped at `RUNAWAY` (one and a half) of its own top speed,
because a car rolling down a mountain is faster than one on the flat and
is not infinitely faster.

Measured over every grade a road is ever built at and past it
(`a_car_drives_up_a_hill_and_is_stopped_by_a_cliff`): ten seconds of
throttle goes **263.9 m on the flat, 238.4 m at the seven per cent a
highway is built at, 56.1 m at one in two, and BACKWARD down a one in
one**, which is the car losing ground to a wall it cannot climb rather
than being thrown down it. The seven per cent is the number that
matters, because it is the steepest a road on this body is, and 90% of
the flat ground's distance is a car that notices a hill and climbs it.
`a_car_climbs_the_same_hill_at_any_frame_rate` holds the sixtieth and
the twentieth in step, which is the rule that `CLIMB` is a grade and
never a number of metres a frame.

**The camera is a CHASE**, behind and over the car, and the walker's is
first person. That is the one place this world has two camera rules, and
the reason is that the point of stealing a car is the car: a first
person view of one is a view of the inside of its own bonnet.

**And it SWINGS, rather than being welded to the roof.** Placed rigidly
off `car.fwd` the camera turns with the car exactly, so the car never
turns on screen at all and the WORLD whips round it: a corner reads as
the horizon snapping rather than as the car going round. `Theft::swung`
eases a heading of its own toward the car's and the camera sits behind
THAT, SLERPED and not lerped, because a lerp crosses the chord and so
turns fastest in the middle of a swing and arrives with a jerk. `SWING`
is 0.40 SECONDS, a time constant and not a share of a frame, so the
swing takes the same wall time at twenty frames a second as at a hundred
and twenty, which is swarm-demo's own rule for its sliding deck. The
heading is squared to the car's own up before it is eased, or a heading
carried over a curving planet leaves the tangent plane and the camera
sinks into the ground a hundred kilometres down the road. It is per
THEFT rather than one for the player, so getting back into a parked car
resumes behind it instead of whipping round from wherever the last one
was pointing, and on a scripted drive it eases over the DRIVING the
frame carried (`steps` sixtieths) and not the frame's own delta, or
every picture the flag takes is aimed a second behind the car.

**And it PULLS BACK as the car goes faster**, `STRETCH` (4 m) and `RISE`
(1.2 m) at the top speed. A camera at one distance says nothing about
how fast the car is going; one that stretches does, and it is what makes
the same corner read as fast.

**And a car burned WHITE at both ends, which is what made that chase
view unreadable.** The first picture of one came back apparently showing
the car's FRONT from a camera that is provably behind it, and the
reason was neither: a head lamp and a tail lamp were ONE mesh in ONE
material, `LinearRgba::rgb(900, 846, 720)`, so the tail lamps burned
the head lamp's warm white. An emissive is a property of the MATERIAL
and not of the vertex: Bevy's standard material multiplies its base
colour by the vertex colour and adds its emissive whole, so the red in
`PALETTE[TAIL_LAMP]` was in the table, was in the mesh's own vertex
colour, and could not reach the picture. Nothing in this world said
which way a car was pointing. It is a mesh and a material a lamp KIND
now, and the emissive is the PALETTE's own row times the kind's glow,
which also collapses a second copy of the head lamp's colour that had
already drifted (`LAMP_NITS * 0.94` and `* 0.8` against the table's
0.96 and 0.85).

**And splitting the material was not enough, because the NUMBER was
wrong by two orders of magnitude.** The second render came back white
as well: Bevy's `emissive_exposure_weight` is NOUGHT by default, so an
emissive is added to the frame AFTER the camera's exposure and is not
in candela at all. `LAMP_NITS` was 900 with a comment claiming it was
"the same order as a building's lit pane", and a lit pane is 3.0 in
`terrain.wgsl` and a street lamp 8.0: 900 is a hundred times over
white, so every lamp clipped to 255 whatever colour it carried. It is
`GLOW` now, in the SHADER's own units, 8.0 at the front and 2.2 at the
back, because a tail lamp is far dimmer than a head lamp and that is a
fact about the LAMP rather than about its colour. Measured on the same
frame: the lamps were 252, 252, 252 and are 226, 110, 107, and 0.188%
of the picture moved. A constant whose comment names the units it is
NOT in is a constant that cannot be checked by reading.

**The end of a car is told by the GAP between its lamps, and that is how
this was settled rather than by squinting.** The head lamps stand 0.52
either side of the centreline and are 0.34 m wide, so the gap between
them is 2.06 of a lamp; the tail lamps stand 0.56 out and are 0.30 wide,
which is 2.73. Measured on the render itself, the two bright blobs are
38 px wide with 104 px between them, which is **2.74**: the chase camera
was right all along and it was the colour that was lying. A picture that
looks wrong names a symptom, and the number that tells two symptoms
apart is the one worth finding.

**And the EYE has THREE places to be, not two**, which is what the first
picture of a stolen car caught and no test did. `place_eye` and `fly`
both read the WALKER to decide whether the fly camera owns the eye, and
stealing a car TAKES THE WALKER AWAY: the theft fired, the log said so,
and the picture came back from the fly camera with no car anywhere in
it. `fly::Aboard` is the question asked once now, the walker or a car
with somebody at the wheel, rather than the walker asked twice. A rule
about who owns a thing, asked in terms of only one of the things that
can own it, is a rule that is wrong the day a second one arrives.

**And `--drive N` steals one headless and holds the throttle**, which is
`--walk`'s own rule: a headless run has nobody to press E and then hold
W, and a car nobody can photograph is a car whose feel nobody can check.
It waits for the crowds to have turned a car out rather than firing on
frame nought, because a theft of nothing is a walk.

**A car's place is a PLACE, and returning a DIRECTION cost a theft
everything.** `Crowds::cars_near` handed back `at.normalize()` and the
caller compared `c.2 * planet.radius`, which puts every car on the MEAN
radius: the nearest car to a walker standing 1,105.7 m up measured
1,105.8 m away, which is his own ALTITUDE and not a distance to
anything, while town 0's own centre was 117 m off. So no car was ever
in reach and the scripted theft never fired, with a log line saying
nothing was near. A position multiplied by a radius it already carries
is a number that LOOKS like a distance and is one to the planet's
centre.

**And a reach is measured from the car's own MIDDLE**, so it has to
clear the car. `REACH` was 4 m and a car is 4.1 m long, which put the
door handle a hand's width outside it; it is 8 m, a stride or two off
the kerb. A SCRIPTED theft reaches `town::OUTLINE * 200`, the whole
town, because `--drive` has no legs: at a player's own reach a headless
run stands on the kerb waiting for a car to happen past it, which is a
flag that photographs nothing most of the time it is used.

**And a stolen car belongs to ONE body**, which is the crowd's own rule
and the same rule as getting out of it: fly to another planet and the
car stays parked where it was rather than being drawn against that
planet's radius and centre, and it is still there when you come back.

**A car STEERS TOWARD somewhere**, which is what a drive between two
towns is made of. `Driver::toward` is the bearing off the car's own
NOSE to a place, as a wheel position, and a bearing rather than a
heading difference because a heading is a tangent vector and two of
them at two points of a sphere are not in the same plane. It is full
lock past `AIM` (a quarter turn) and eases in, so a car does not saw at
the wheel about its own line. Measured on the test ball: 199.3 m closed
to 10.2 m in thirty seconds.

**And `--drive N` is N SECONDS aimed at the nearest settlement that is
not the one it is standing in.** Driving straight ahead measures the
CAR and says nothing about whether the world has anywhere to drive TO,
which is what the owner actually asked for. It steps a second of
driving per rendered FRAME, in sixtieths: a frame of this world on a
software rasteriser is most of a second, so a scripted drive stepped one
sixtieth a frame covers 480 m in half an hour of rendering and the
nearest settlement is nine kilometres off. The step stays a sixtieth,
which is what keeps the drive the same drive on any machine, and the
wheel is re-read every sub step, or the car holds one bearing for a
whole second and weaves round its own line at sixteen metres a second.

**The HUD says the speed you are GOING.** It led with the WHEEL's own
setting, which near the ground is a number the air never allows:
`Planets::advance` tapers a cruise down to `surface_speed` and already
RETURNED the speed it applied, and `fly` threw that away. The readout
leads with the measured speed, names the wheel beside it, says when the
air is holding it down, and prints anything over a kilometre a second in
km/s, because two million metres a second is a number nobody can hold
and is the same speed as 2,000 km/s.

**A car gets out of a TOWN on the town's own streets, and that is the
last thing between a stolen car and the country.** `roads::Network::
follow` gives up past `OFF_ROAD` (400 m), and a car stolen in the middle
of the port stands 700 m from the nearest tarmac: the scripted drive fell
back to aiming at a settlement nine kilometres off, which from a street
between two buildings is aiming at a wall. `traffic::Streets::route` is
the answer and it needed no new data, because the graph of a town's
paving is already there and is read off the pieces that are DRAWN. It is
a breadth first walk of it: `town::streets_of` lays a piece only where a
block or its neighbour carries a lot, so a suburb's grid has holes and a
walk that only ever steps NEARER the goal walks into one and stops; a
town is a few hundred edges, so a full sweep costs nothing.

**Three defects on the way out, and each is the same shape: aiming at
something that is not the next thing.**

- **The road following was never once DRIVEN ON.** `steer_for` computed
  the point on the tarmac and put it on the wheel of the outer input, and
  a sub stepped drive then threw that away and replaced it with
  `car.toward(goal)`. It was written, tested and dead, and the car went
  on wedging itself against the building it always had. `Auto::aim` is
  what the sub steps read now.
- **A look ahead of thirty metres on a pitch of eighteen and a half skips
  a corner.** A route is a chain of crossings joined by streets the town
  laid, so the straight line to the NEXT one is on the paving and the
  straight line to the one after it is through whatever stands on the
  corner. The car made 230 m, reached a turn, and held the throttle
  against a building with its aim 33 m away for the rest of the run.
  There is nothing to tune, which is why the constant went: a town's look
  ahead IS its pitch.
- **And falling back to the route's LAST point was a car steering at the
  crossing it was standing on.** The bearing off its own nose to a place
  it is on top of is noise. No crossing left means the car is at the
  town's exit, and what it wants then is the tarmac.

**A road's DIRECTION is a fact about the road and not about the next
point along it.** `follow` asked whether `line[near + 1]` was nearer the
goal than `line[near]`, which is a local test on a thing that winds: the
car turned round at every bend it met and covered 1,425 m of tarmac in
seven minutes while closing 40 m of nine kilometres. It is the two ENDS
now, so the answer cannot flip under a car that has not gone anywhere.

Measured on the harness planet, the scripted drive out of the port: 12 m
in 30 s wedged against a building, then 230 m and stuck at a corner, then
1,425 m in 420 s oscillating on the road, and **9,297 m in 600 s at a
steady 58 km/h**, which is the car flat out on a country road for ten
minutes with nothing to back off from.

**What WAS missing is the NETWORK, and the route closes it.** The car
followed ONE road, the one it joined, from the town's own streets: there
was no route across the roads that join at a town, so a settlement that
was not on the road it reached was a settlement it drove past rather
than to, and the distance to its goal never closed. It drives the ROUTE
the map plans now, which is A* over the road graph one level up from the
town's own breadth first walk, and the section on the route below is the
whole of it; `Network::follow` is what it falls back to when no road
joins the car to its goal.

**And a car with no ROUTE cannot get out of a town, which is measured
rather than guessed.** The scripted drive aimed straight at the next
settlement, which from a street between two buildings is straight at a
wall. It drove twelve metres out of the port and then oscillated in
place for the rest of the run: forward into the building, back off,
forward into the same building. `driven 12 m in 30 s`, and 12 m at 60,
90 and 120.

**So it FOLLOWS THE ROAD now**, which is what a road is for and what the
section above finally put on the ground. `roads::Network::follow` is the
point on the tarmac `AHEAD` (400 m) along from the piece nearest the car,
in whichever direction gets nearer the town it is driving to, and
`steer_for` hands that to `Driver::toward` instead of the town itself.
Off the road, further than `OFF_ROAD` (400 m) from any tarmac, it falls
back to aiming at the town, which is what gets it out of a street and
onto one. It is pure pursuit with one knob, and the knob is how far ahead
it looks: nearer and it saws at the wheel, further and it cuts the bends.

The search is not a walk of the road. The `Network` keeps each stretch's
own middle, so the nearest of 11,987 of them is an angle each, and only
that stretch's seventeen points are looked at; walking 190,168 points a
frame would be the frame.

Two things came out of measuring it, and both are worth keeping
whatever drives next:

- **A car is stuck when it makes no GROUND, not when the dial says so.**
  The wedged car read 4 km/h on its own `speed` while covering nought
  metres a second: a crash scales the speed down rather than stopping
  the car, so the dial says what the engine is asking for and not what
  the wheels are doing. `Auto` measures the arc the car actually turned
  through, and the recovery fires now where reading the dial never did.
- **And the car itself is sound at every scale and against anything a
  test can build.** `a_car_pulls_away_at_planet_scale` drives 136.9 m in
  ten seconds on a 2 km, a 100 km and a 1,000 km ball, identically;
  `a_car_that_has_met_a_wall_can_back_off_it_again` drives 5.53 m into a
  CORNER of two walls and reverses 12.60 m off it;
  `a_car_slowed_to_a_crawl_can_pull_away_again` starts at 1 cm a second
  and reaches 16 m/s in five seconds. So what stops the car in the port
  is not its physics, it is that a building stands between it and where
  it is going and it has nothing to follow round one.

What makes a drive between two towns a journey is the ROAD, and the
section on the roads below is why there is not one on the ground yet.

**A CAR IN A WALL COMES OUT OF IT, and for a long time it could not.**
That is the owner's "my car gets stuck in the buildings", and it is one
line: `roll` asked whether a step was allowed and, where it was not,
RETURNED without taking the position the push had already worked out. A
car nosed into a building kept whatever overlap it had arrived with for
ever, and one standing still inside a wall never moved at all, because
`roll` returns at once at nought speed. Nothing in the whole frame could
take a body out of a solid it was already in, which is this project's
own oldest collision rule (**resolve, never "may I"**) missing from the
one body that most needed it. `Driver::free` is that push, FIRST, every
frame, whatever the pedals say, and a car that is clear of everything is
not moved by it.

**And a fast car TUNNELLED, twice over.** A wall here is `model::WALL`,
0.35 m, and a car at 44.4 m/s covers 2.2 m in a frame of a software
rasteriser and 0.74 m in a sixtieth: a step longer than the wall walks
the outline's own eight points clean through it, nothing overlaps at
either end, and the car comes down on the far side. The roll is SUB
STEPPED at `Driver::STEP` (0.25 m) now, capped at sixteen steps a frame.

That was not enough on its own, and the reason is worth keeping: **a
push out of a signed distance box goes out of its NEAREST face, and a
point past the box's own mid plane is nearest the FAR one.** Measured on
that 0.35 m wall, a bumper 0.03 m past the middle was pushed 0.15 m
FORWARD, out the other side, and the car drove on at 160 km/h without
so much as slowing. No sub step short of half the wall's thickness fixes
that and nothing knows how thin the next wall is. `walker::swept` is the
answer: a step whose OUTLINE crosses a wall between where it started and
where it ended is refused rather than resolved, sampled every 0.1 m,
with both ends left out so a body sliding along a wall it is touching is
not glued to it. Measured, a wall 220 m off: **217.76 m at sixty frames
a second, 217.76 at twenty and 217.77 at twelve**, which is the wall
less the car's own 2.05 m of bonnet at every frame rate, and nothing
gets through.

**One function decides what a WALL is.** `walker::out_of` is the `STAND`
test and the push in one place, and both `resolve_body` and `swept` ask
it, so a step refused and a push applied cannot be looking at two
different worlds. `walker::Shape` is the ring and the heights a body is,
which is what kept those two signatures inside this file's own limit.

**AND A CAR IS STOPPED BY ANOTHER CAR.** `driver::car_box` is the box a
car is to another body, built from `figure`'s own three numbers so the
box and the car that is drawn cannot drift, and `drive::around` gathers
the ones within `CAR_REACH` (30 m) of the wheel: the town's traffic, the
cars out on the ROADS, and any the player has parked. They go into the
same `Built` the town's walls are in, so the collider does not know a
car from a wall and there is no second resolution path.

**What is MISSING, named rather than hidden.** A car on the rails does
not know the player is there. It is a closed form function of its town
or its road and the clock, and knowing would mean state, which is the
one thing rails do not have: so the traffic never gives way, and what
happens when the two MEET is the section on ramming below. Nothing is saved, so a car is
stolen afresh every run. And the sea does not stop it: the bounds it
drives against carry no water, so a car driven off a beach keeps going
down the sea bed.

## A car that is HIT leaves the rails, and both cars take the knock

The owner's ask: "when I ram other cars only I get affected by physics,
both cars should be affected, I should be able to ram others off the
road." A car on the rails is a closed form function of its town or its
road and the clock, and it cannot be moved, because moving it would be
state; so the moment a car is hit it comes OFF the rails for good,
exactly as a stolen one does, and from then on it is a `Driver` with
nobody at the wheel: it takes the shove, slides on its tyres, turns if
it was struck off centre, rolls to a stop wherever that puts it, is a
box to the player's car, is drawn by `show_cars` like a parked car, and
is boarded with E like one. `Theft::who` is a `Rails` now, a town's
agent or a road's commuter, because a road car can be hit where it
could never be stolen, and the highway skips a knocked one the way the
streets skip a stolen one.

**What the two exchange is the core's, `ram.rs`**, because it is a rule
two clients have to agree on: equal masses and a restitution, which is a
car to car collision to first order. The closing speed along the
contact's normal is shared out, each car's own changes by
`(1 + RESTITUTION) / 2` of it, so momentum is kept exactly and
`RESTITUTION` (0.3, the middle of the 0.2 to 0.5 crash tests put a car
at, because a car is crumple and not a billiard ball) of the closing
speed comes back as separation. The normal is the least penetrated of
the two cars' own axes, which is the FACE the hit is on, and the
contact is where the two edges driven together OVERLAP along that face:
a nose square on a tail meets it at the middle of the bumper and turns
nothing, and a nose on a flank behind the other car's middle meets it
there and turns it by that lever over a rectangle's own moment of
inertia. The first cut asked whose face the axis was and found no lever
at all, because a nose on a flank is on both cars' axes at once.
`SPIN_SHARE` (0.35) is how much of the rigid body's turn a car takes,
since tyres resist a spin the way they resist a slide; at one a rear
quarter hit at 30 km/h turned a car twice round.

**A knock is two things on a `Driver`, and the pedals own neither.**
Along the car it is `speed`, which the throttle, the brake and the drag
already own; across it is `shove`, a velocity in the tangent plane the
tyres take back at `SKID` (6 m/s^2, six tenths of a gravity, a tyre's
grip on dry tarmac), sub stepped and pushed out of walls exactly as the
roll is; and `spin` is worn off at `SPIN_DRAG` (3 rad/s^2). A car
struck at 10 m/s across slides 7.87 m and stops, and a spin of a radian
a second turns it ten degrees.

**The harness asks every car near the wheel, before the car is driven.**
`ram::ram_cars` reads the rails a tick apart for each car's velocity,
because a rail is a place and not a velocity, which is what makes the
other half of the ask true: a rails car that drives into the player
shoves the player and comes off the rails for it. The knock comes off
the speed the car ARRIVED at and not off what the wall it then meets
leaves of it, and for `CLEAR` (a quarter of a second) after a hit the
two pass through each other's boxes, because a car sent on its way at
`(1 + e) / 2` of the closing speed is faster than the one that sent it
and a wall on top of the knock took `CRASH` of what was left: a rammer
that stopped dead against the car it had just shoved. And the rails no
longer put a GHOST box where a car off them would have got to, which
was an invisible wall driving down the road behind every stolen car.

Measured, on the core's own ball: a car rammed square from behind at
16 m/s takes **10.40 m/s** and the rammer goes on at **5.60**, which
sums to the 16 it arrived with; a car struck on its rear quarter across
its path at 10 m/s is shoved at 6.5 m/s and turned at **1.69 rad/s**
toward the rammer's line.

**What is MISSING, named rather than hidden.** A knocked car rolls on
with nothing on the pedals, so it never brakes, never steers back onto
the road and never rejoins the traffic; two knocked cars do not hit each
other, only the player's car hands out knocks; and a car on the rails
still does not know the player is there until it touches, because
knowing would mean state. The scripted drive out of the port WEDGED at
80 m on this binary and on the one before it alike (81 m in 180 s at
4 km/h against 83 m at 3, the same car, the same street), so the
wedge was the blocks port's own way out and not the knock's; it was
the routing, and the section on the route below is how it was closed.

## A road is ON THE GROUND now, and its corridor is levelled like a town's

A road was DATA: a chain of directions the chart painted and nothing
underfoot, so driving between two towns was driving cross country over a
route nothing marked. This is the other half, and it is the same shape as
a town: the GROUND is cut in the planet's own field from the first frame,
and the TARMAC streams.

**A `Site` is an ARC now, and a town's disc is the case where its two
ends are the same direction.** One type and not a disc beside a capsule,
because everything that reads a site (`site_band`, `site_weight`,
`surface_blend`, `local_solid`, `Planet::around` and the slope bound they
all rest on) would otherwise be written twice and two of them are subtle
enough that one copy would be wrong. `Site::nearest` is the whole of the
new geometry: the point projected onto the arc's own great circle, clamped
to the segment, and the level ramped along it.

**How FINELY a corridor is cut was measured, not chosen.**
`examples/road_ground.rs` walks the body's own roads and asks how far the
ground strays from a straight ramp between two stations. On the atlas's
ten kilometre waypoints it is a median of 45 m and up to 830, which is a
canyon rather than a cutting. The sweep halves with the spacing: 2.7 km is
10.3 m, 1.4 km is 4.8, 683 m is 2.3, 341 m is 1.20 with a 99th of 6.2, and
171 m is 0.72, and 85 m is 0.54 with a 99th of 2.10.

**`PIECE` is 85 m, and it was 341 because the WORST was never looked
at.** A median of 1.20 m and a 99th of 6.13 read as a verge and a
cutting, which are things a road HAS; the worst at that spacing is a
**36.25 m canyon**, and a road that disappears into the country once on
a body is a road that disappears. The owner read it off a picture as a
highway sinking into the ground. At 85 m the worst is 4.56 m, which is a
cutting everywhere on the body and a canyon nowhere.

What it costs is the count, four times over: 186,788 arcs become
749,431, the bake goes from 23.8 s to 99.7 (the survey sphere traces four
times the stations), the atlas from 4.4 MB to 6.6 and reading it from
160 ms to 753. What it does NOT cost is the chunk, and that was worth
measuring rather than assuming: the same road scene settles in **69.5 s
at 30.0 ms a chunk against 123.5 s at 106.3**, the same 2,304 chunks
either side. Four times the pieces landed at 377 ms a chunk on their own,
and the arc reject below is what turned a 3.5 fold regression into a 3.5
fold improvement. `ribbon::STRETCH` went 16 to 64 with it, because a
stretch is a LENGTH of road and not a count of pieces: left at sixteen
every road would have arrived and left in 1.4 km bites, four times the
meshes and four times the entities for the same tarmac.

**And `site_weight` REJECTS an arc before it does its trigonometry**,
which is what made four times the pieces affordable. `Site::nearest`
costs an `atan2` and a `sin_cos`, and it is asked of every corridor
piece a chunk keeps for every one of that chunk's seven thousand
samples: fine while a piece was 341 m and a chunk on a road kept one,
and not fine at 85 m where it keeps three or four. Every point of an arc
is within its own CHORD of the end it starts at, so a direction further
off than that plus the band cannot be in it, and the reject is two
subtractions and a squared length. A town's chord is nought, so its
reject is exact.

**And the piece is in the ATLAS'S FINGERPRINT now.** The file keeps the
corridor's heights and DERIVES its directions at a spacing both sides
compute from `PIECE`, so a file baked at another spacing has a run of the
wrong length for every road on the body and `road::corridor` hands back
nothing: roads on the chart, a route a car can follow, and no ground
under any of it and no tarmac on it. That is a silent failure of exactly
the kind the other six fields exist to refuse, and it was not one of
them.

**The file is written COMPACT, which is most of its size.** It is
760,000 numbers and pretty printing puts each on its own line under three
levels of indentation: 13 bytes of whitespace a number, 7.3 MB of a
13.9 MB file, and nothing a reader could have read anyway. It is 6.6 MB
compact.

**A road RIDES the country, and never cuts into it.** The owner read it
off the air: terrain on top of the road, and the road not moving up and
down with the ground. A CUT is the one thing this terrain cannot draw.
The corridor is `CORRIDOR` either side of the centreline and the
rings put a cell of about a SIXTY FOURTH of its own distance under the
eye (a box at level L is 32 * 2^L metres across and its cell is
0.5 * 2^L), so once the cell is wider than the cutting the mesher has no
sample inside it at all: it draws the hill that was there before the road
and the ground closes over the tarmac. That is this file's own "nothing
thinner than a cell's DIAGONAL exists", arriving outdoors.

**So the VERGE is what carries the road through the LOD, and that is
what sets its width.** The flat part of a corridor is `2 * CORRIDOR`
wide and a lattice column is only GUARANTEED to land on it while a cell
fits inside it, so **a road survives to about 128 times `CORRIDOR`**. At
7 m that is 896 m, and the owner's picture from 900 m up is the road
DASHED, which is the relation arriving exactly where it said it would.
Measured off that frame, the gaps are IRREGULAR, 22, 28, 56 and 39 m of
tarmac against 4 to 26 m of nothing, which is what says it is the
lattice phase beating against the relief and not the 85 m piece: a
per piece defect would come out on an 85 m period and this does not.
`CORRIDOR` is 16 m, so the flat is two cells of level 5 rather than
under one, a column lands on it whatever the phase, and the road holds
to 2,048 m.

Three things about that number rather than a bigger one. It wants **no
re-bake**, because the width is runtime only: what the atlas stores is
the road's own heights and `survey` takes those off the BARE ground
before any corridor exists. It does not touch the **grade**, because the
skirt is still `SKIRT_IN + SKIRT_OUT` and the blend's gradient and the
planet's slope bound are exactly what they were; only the flat grew. And
32 m of graded ground for 11 m of tarmac is a verge either side, which
is what a highway alignment occupies, and the whole 63,840 km network is
0.016% of the body.

**And geometry has a CEILING here, which is named rather than hidden.**
Holding the road's own WIDTH rather than one column wants `5.5 + cell`,
21.5 m at level 5 and 37 m at level 6, and no width at all carries a
road past the altitude where it is under a pixel anyway: a pixel is
about `h / 870` here, so 11 m of tarmac is sub pixel from 9.6 km up.
What reaches past that is a DECAL, and never geometry and never a
material byte on a vertex, which is what the paint below already
learned.

It is NOT the depth buffer, and that is worth saying because it is the
first thing anybody reaches for. Bevy's perspective is infinite reverse
Z, so at a near plane of a tenth of a metre the step at two kilometres
is well under a millimetre and the tarmac's own 0.15 m lift is thousands
of steps clear of the ground. A logarithmic depth buffer is what a
program with a FINITE far plane needs and this one has none.

So a road only ever RISES. `road::smooth` held its grade by CLAMPING
both ways, which is real road engineering and means cutting: a station
standing higher than the grade allowed was pulled down into the hill.
It takes the MAX both ways now, so the forward pass bounds the descent
and the backward pass the ascent and neither ever pulls a station down;
the fixed point is the least profile above the ground that a road may be
built at, which starts a climb earlier and stands on an embankment. An
embankment the mesher loses leaves the road a little proud of the
ground. A cutting it loses leaves the road under it.

And the CHORD is held over the ground too, which a station's own height
says nothing about: `road::PROBES` (3) samples the ground at the quarter
points inside every piece, and a probe is a FLOOR on the two stations
either side of it. A straight chord between two points both at or above
a sample is everywhere at or above it, so that clears the ground between
two stations by construction and asks nothing of the chord itself; what
it costs is that a road stands at the highest ground within its own
piece either side, which is an embankment over a crag and is what a road
has.

**THE ORDER IS THE WHOLE OF IT, and it was wrong.** There were three
rounds of forward, backward, probes, floor, ending on the PROBES: the two
envelope passes bound the grade and the probe pass then lifted both ends
of a chord by whatever the ground between them stood above it, which
changes the slope to each end's OTHER neighbour and is bounded by nothing
at all. Measured on this body, the steepest piece came out at **4966%**,
which is eighty eight degrees, and 4.72% of 769,371 pieces were over even
the tenth this used to allow. The owner read one off a picture as a road
going straight up a hillside. It is TWO statements now and neither
iterates, because a fixed point nobody can name is a fixed point nobody
can check: the probes are a floor on the stations FIRST, and then the
envelope once and LAST, forward for the descent and backward for the
ascent, which is exact rather than approximate. Measured after:
**nought of 661,492 pieces on the whole body's highways are over the
seven per cent**, and the steepest piece anywhere is in a SLIP. Measured on
the rough test ball, the worst a road still cuts into its own ground:
**16.84 m clamping both ways, 1.86 m rising with one probe a piece, and
0.92 m with three** (`a_road_rides_over_the_ground_rather_than_cutting_into_it`).
What the probes cost is the BAKE: 322.5 s against 99.7, four
`surface_radius` marches a piece against one, and nothing at all
afterwards, because what the atlas carries is the answer.

**And the highway RUNS IN on a SLIP, which is the owner's own ask:
the highway should lower to meet the height of the city, curve down and
merge with the local roads.** `road::open` stops the corridor at every
town's levelling, because inside that the town's site answers the
ground and a corridor there would lay its tarmac at the road's level
over ground held at the town's. That is right for the FIELD and wrong
for the GEOMETRY: measured on the port, it left the highway's first
tarmac 169 m out with the town's own paving reaching 270 m, and what a
picture showed was a square ended road in a field with the city beyond
it.

**A MASK cannot close it, and that was measured rather than argued.**
The first answer was `road::paved`, the other half of `open`, with
`road::mouths` bisecting how much of the last piece carried tarmac. It
lays the road's own BAKED PROFILE over a town's flat plateau, and the
two are not the same height: `the_tarmac_lands_on_the_ground_its_corridor_levelled`
came back at **1.790 m of float** the moment the mask's own gate was
fixed. The profile is what `road::smooth` raised to clear the country;
the plateau is what the town cut. A mask can only choose WHERE to lay
tarmac and never at what height, so the whole of `paved`, `mouths`,
`clear` and `MEET` went out.

`road::slip` is GEOMETRY instead, and it reads the ground rather than
guessing at it. A cubic Hermite from the highway's own mouth to the
nearest CROSSING the town paved, sampled every `SLIP_PIECE` (3 m,
where a 15 m radius turn's chord is out by 7 cm, under the tarmac's own
lift), leaving along the road's heading and arriving along the town's
own grid axis. Three things fall out of that:

- **A CROSSING and never a run**, because a crossing is a node where
  streets already meet and it owns its whole square (`town::paved`), so
  a slip arriving at one merges into the grid; arriving at the middle
  of a run would T bone a street at whatever angle the road came in on
  and leave the corner bare.
- **The height is the GROUND's and never a lerp.** The town's site has
  levelled its plateau and the corridor has been cut outside it, so the
  field's own surface between the two IS the ramp from the road's grade
  down to the town's, and a slip that reads it lands on it. That is the
  owner's "lower to meet the height of the city" and it needs no second
  opinion about what that height is.
- **And the LIFT tapers**, `ribbon::LIFT` (0.15 m) at the highway to
  `town::LIFT` (0.05) at the street, which is the 10 cm lip this file
  once gave as the reason not to run tarmac into a town.

It is SPLICED onto the head of the route rather than drawn beside it,
because a route is what everything downstream reads: the stretches that
stream, the lamps that light them and the `roads::Network` a car
follows. A slip that was a mesh of its own would be tarmac a car could
not drive onto, which is the same defect one level up.

**And it is spliced AFTER the sites are installed, which is the whole
of the first picture of one.** A slip stands on the GROUND, and the
ground is the field with the towns' plateaus and the roads' corridors
cut into it; the first cut ran inside the loop that BUILDS that list,
so `town::surface_radius` was asked of a planet whose `sites` were
still empty and the slip was laid on the BARE relief. Near a town's
mouth the corridor is on an embankment metres over that, so the tarmac
was UNDER the ground it was laid on: the picture from 90 m showed the
slip's own curved SHADOW crossing an empty field with the road nowhere
in it, the highway and the crossing showing at the two ends where the
levelling gave out. A shadow with nothing casting it is the shape of
geometry drawn below the terrain, and the order of two statements is
what put it there.

**A slip reads the SAME GROUND THE MESHER DRAWS, and the march is cheap
because it starts where the answer is.** `Planet::surface` is the relief
with the sites applied, written down; `town::surface_radius` is a sphere
trace of the field for its own first crossing, and it is what the mesher
agrees with and what `road::survey` reads for the highway's own profile.
The slip read the ANALYTIC one for a commit, on an embankment of
`ribbon::LIFT + EMBANK` to cover the difference, and the difference is
bigger than that: measured along the port's own slip, the drawn ground
stood **1.045 m over the analytic surface at one point and 0.11 m under
it three metres away**, which is the volumetric term the analytic answer
leaves out, and 0.39 m of tarmac was buried under the hill it was laid
on.

Two things had made the march unaffordable and neither is the march. The
slip is spliced AFTER the corridors are installed, so an unfiltered trace
walks all 646,000 of the body's levelled arcs at every step of every
sample: `Planet::around` at the slip's own mouth, filtered ONCE for the
whole curve, is this file's oldest performance rule. And
`town::surface_radius` starts at the top of the RELIEF BAND, which here
is sixteen kilometres of air at a floor of half a metre a step;
`town::surface_radius_from` is the same march from a radius the caller
knows is air, and a caller holding the analytic surface knows one within
`Planet::overhang`, because the volumetric term is the only thing that
answer leaves out and it lifts a surface by half of that at the most.
Measured: **74.6 s of startup against 0.16 s for the same answer**, and
`a_march_from_the_analytic_surface_finds_the_same_ground` holds the two
to a centimetre over four hundred directions.

So a slip carries the highway's own `ribbon::LIFT + EMBANK` where the
ground is relief and a street's own `town::LIFT` where the town has
levelled it, blended by `Planet::site_weight`, which is the one function
that says where a town's plateau is. Written as a share of the SLIP that
taper was wrong twice over, because the crossing a slip ends on stands
well inside the levelling; written as a band off `Site::level_r` it was
wrong by the half of the skirt that fades INSIDE that boundary, and
buried the tarmac the same 0.39 m in the same place. It is one number
now and the ground and the lift cannot disagree about it.

**And reading the traced ground does NOT make the embankment
unnecessary**, which is what the first picture of a junction from 90 m
said and no number had. Dropped on the reasoning that there was nothing
left for one to hide, the slip came back BITTEN THROUGH in half a dozen
places by jagged terrain while the highway either side of it was solid.
The trace and the MESHER are not the same surface either: dual
contouring puts its vertex within a cell of the crossing, and a cell
under an eye 90 m up is over a metre, against `ribbon::LIFT`'s own
0.15 m of clearance. `EMBANK` is the 0.5 m the highway has always
carried for exactly that. Inside a town's levelling there is nothing to
hide, because the ground there is a plane the audit holds to two
millimetres, so the street's five centimetres is enough and the blend is
the ramp between the two.

**And the slip is anchored at the HIGHWAY's own height and lands on the
TOWN's own street, with the step between them tapered out along it.**
`road::smooth` raises a station to clear every probe inside its own
piece, so the highway's last tarmac stands over the ground at that same
direction; laid on the ground alone the slip starts with that whole
difference as a STEP over one `SLIP_PIECE` of three metres, and the
steepest piece anywhere on the body was **-0.50 m over 0.015 m, a grade
of 3355%**, which is that step over a piece of no length at all after
`world::splice_slip` had left the mouth in the line twice. The step is
one straight ramp of `d / length` now, which leaves both ends exactly
where they have to be. What it does NOT do is hold the slip inside the
highway's grade, and that is named rather than hidden: the first cut
enveloped it from the mouth and let the far end fall where it fell,
which on the rough test ball left a slip standing **2.39 m over the
crossing it was laid to reach**. No profile that lands on both ends is
inside seven per cent when the ground between them is not, and what is
between them is a town's own apron.

**And it ends on ground the town has LEVELLED.** A town's grid runs out
past its own site onto the skirt, where the blend ramps and the
volumetric term comes back, so the nearest crossing to a highway's mouth
is often the one part of the town's paving that is itself partly in the
hill. `road::crossing` prefers a crossing inside `Site::level_r` and
falls back to any crossing and then to any piece at all, which is what a
one street hamlet needs. Measured on the port: the slip ends at a
crossing **129 m out rather than 158**, and the drawn ground stands
**-0.05 m** over its own tarmac at the worst, which is the tarmac over
the ground everywhere by exactly the lift it was laid at.
`a_highway_joins_a_town_at_a_crossing_and_lands_on_its_ground` holds
both halves on six highways: nought under the ground anywhere, and never
higher over its ground than the highway it leaves.

**And the harness measured its own constant, for the third time in this
file.** `gap_to_town` walked in from `mouth_of`, which after the splice
is the CROSSING: a walk that starts on the paving reports nought bare
ground whatever the road does, exactly as `road::clear`'s own `MEET`
did before it and `cars_near`'s radius did in another file. Both went,
and `roads::slip_of` is the one measure left: `Route::slip` records how
many head points are the slip, so the number is the slip's own length
and how far its END stands from the paving it was laid to reach.
Measured on the port: **the slip is 14 pieces over 40 m, from the
highway's mouth 169 m out of town 0 to a crossing 158 m out, ending
0.00 m from the town's own paving.**

**What was tried and taken OUT: painting the road on the terrain's own
triangles.** `Density::material` was given the sample's own cell so
`Planet::material` could answer `STREET` on ground too coarse to hold a
corridor, which is continuous at every level by construction because it
IS the ground. It is also a per TRIANGLE tag, so the band is at least
one cell wide, and the arithmetic says what that looks like: a cell is
about a sixty fourth of its own distance and a metre subtends about
640 / (0.414 d) pixels, so a painted road is a constant **24 px across
at every range** against a true 3.4 px at 2.5 km, with sawtooth edges
where the triangles fall. The owner's word for it was that painting it
on the geometry vertices is a horrible idea, and the picture agrees. If
it comes back it comes back as a TEXTURE or a decal, not as a material
byte on a vertex.

**A road may FILL and a town may not**, and that is the one rule the two
do not share. A town's level is the lowest its own survey found, so a site
that filled would be a city on a pedestal with its apron over the valley,
which is what the owner read off a picture once. A road's level is the
ground at its own stations, so the ramp between two of them runs over
every hollow between: tarmac laid on a corridor that could only cut
floated 18.6 m over the ground in the worst place on this body. An
embankment is the other half of a cutting and no road is built without
both. `Site::fills` is the flag and `Planet::levelling` is where the two
answers come off one loop.

**The count is what `field::Sites` is for.** A body with eight towns can
be walked and a body with a hundred and sixty can, once `Planet::around`
filters once a CHUNK; a body whose roads are levelled cannot, because
`surface_blend` is asked for every one of a chunk's seven thousand sample
points and there are 186,788 sites. The index is the simplest thing that
works on a sphere and keeps this crate's no-`HashMap` rule: the sites
sorted by LATITUDE, which is `dir.y` because that is what `biome` already
measures latitude on, plus one number for how far the widest of them
reaches off its own. A query is a binary search and a walk of a thin band.
`the_site_index_finds_every_site_a_walk_would` holds it against a brute
force walk at a thousand directions on a body of discs and corridors of
every size, because an index that MISSES a site is ground nobody levelled
and a hole in the world.

**And `..self.clone()` was the whole cost of a road.** Struct update
syntax evaluates its base FIRST, so `Planet { sites, ..self.clone() }`
clones every site on the body and then throws the list away. With eight
towns that is a hundred bytes and nobody notices; with 186,788 corridor
pieces it is 13 MB a call, and `around` is called once a chunk and once a
sample in the bake's own survey. Measured on the port: **71 ms a chunk,
against 8 before and 8 after `Planet::bare`**. A body with a hundred and
ninety thousand levelled corridor pieces costs a chunk exactly what a
body with eight towns did.

**What is BAKED is the HEIGHTS and nothing else.** The refined
centreline's directions are a slerp off the atlas's own waypoints at a
spacing both sides compute from `PIECE`, so only the ground under them has
to be written down: 190,168 numbers rounded to the centimetre, which took
the atlas from 1.7 MB to 4.4 rather than to 17. `road::pieces` and
`road::step` are the one function each side derives from, because a bake
and a game that disagreed by one piece would be a road whose levelling and
whose tarmac are in different places.

**A corridor stops at EVERY town it passes and not only at its two ends.**
`road::waysides` grows a village wherever a road has run a day's cart, so
a road passes THROUGH settlements as well as ending at them, and a town's
disc levels its ground to the town's level while a corridor ramps to the
road's: where the two overlapped the field answered whichever it reached
first and the tarmac stood on the other. `road::open` is the one answer
both the levelling and the tarmac are cut by, so they cover the same
ground by construction.

**A point SEVERAL sites cover outright takes the NEAREST one's level, and
that is a road's own profile rather than a tie break.** A corridor is a
chain of arcs whose ends MEET, and an arc's band is a CAPSULE: its round
end reaches `CORRIDOR` past its own last station into its
neighbour's, so the ground either side of every station is covered twice.
`Planet::levelling` returned on the first site its own latitude index
reached, so a point seven metres along the next arc was given the level
AT the station rather than the ramp: every station on every road carried
a fourteen metre LANDING, a one in ten grade stepped 0.66 m at each of
them, and which of the two arcs won was the sort's business rather than
the geometry's. The tarmac is laid on the ramp, so it floated over those
landings, and that is how this was found: the dashes above are the first
geometry this world ever put at an INTERIOR point of a piece, and the
tarmac test went from 0.103 m of float to 0.796 the moment they landed.
The point seven metres along the next arc stands ON that arc's axis and
seven metres off the last one's, so the nearest is the one whose ramp it
is, and `a_corridors_ground_ramps_through_a_station_without_a_landing`
holds the profile to within a centimetre of the straight line the road
was routed at, against 0.66 m before.

**And a road's skirt is laid OVER a town's ground, never under it.** A
road FILLS where a town CUTS, and `Planet::levelling` composited every
partial site in the index's own order: where a town came after a road
it pulled the road's skirt toward the town's level, so at the edge of
the corridor's flat, where the road's own weight comes to one, the
ground stepped by the town's share of the difference, and where the
town covered the ground outright the road's skirt was dropped and the
ground stepped by all of it. The scripted car out of the port found it
as a wall it could not see: the slip's corridor stood 4.2 m over the
sloping apron a metre ahead of the bonnet, and the car made three
centimetres in thirty seconds against a step the car's own rule was
right to refuse. It is two composites now (`field::Layer`), the towns'
over the relief and the roads' over that, so a road's weight going to
one takes its own level exactly whatever stands under it; towns alone
and roads alone come out as they always did.
`a_roads_skirt_ramps_down_onto_a_towns_ground_and_never_steps` walks a
road 5 m up out of a town cut to -3 every five centimetres, in both
orders the two can be listed in: **8.000 m in one step before, 0.055 m
after**, which is the skirt's own smoothstep and nothing else.

**The tarmac is the streets' own cross section at the country's scale.**
`LANE` each way read off `town::street` rather than written again, a
dashed centreline, a solid line down each edge, and a SHOULDER falling
from the tarmac into the ground. The lift is three times a street's five
centimetres and the reason is measured: a street is laid on a town's one
level and a road on a RAMP, so where two pieces meet at a bend the tarmac
is MITRED and its outer corner sits a little along the ramp from the
station it belongs to. 4 mm almost everywhere and up to 0.103 m at the
waypoint bends. Fifteen centimetres clears that, and the shoulder is
buried deeper than the error runs so there is no crack at the outside of a
bend for the ground to show through.
`the_tarmac_lands_on_the_ground_its_corridor_levelled` measures both.

**A DASH IS THREE METRES AND A PIECE IS THREE HUNDRED AND FORTY ONE**,
so the markings cannot be a property of the piece, and the first picture
of a road is what said so: two edge lines running to the horizon with no
middle at all. The centreline was asked once a piece, so it came out as
341 m of solid paint and then 682 m of nothing, and the piece the camera
stood on had fallen in a gap. The piece is walked in `DASH + GAP` steps
now and the painted part of each step is its own quad, with the station
and the across interpolated to where it falls, which is what
`the_centreline_is_dashed_in_dashes_and_not_in_pieces` measures: 33.3% of
the centreline is paint and the longest single mark on it is 3.00 m.
The phase is measured from the start of the STRETCH rather than of the
road, so one dash in 5.5 km is short at a seam; a stretch is what a road
is built and streamed in and knows nothing of the pieces before it, and a
global phase would mean walking a whole road to lay any of it.

**It STREAMS, one stretch a frame, like a town.** 63,840 km of road is
sixty million triangles and the eye is only ever in one place: a stretch
is sixteen pieces, 5.5 km, one mesh in a frame of its own (an `f32` there
holds a third of a millimetre), and the ones within `REACH` of the eye are
laid one a frame and dropped one a frame. `roads::Network` keeps each
stretch's own middle so the near ones can be ranked without walking a
centreline, and the distance is along the GROUND, the angle times the
radius, which is this file's own rule about a position multiplied by a
radius it already carries.

**A road is LIT on the approach to a town and dark in the country**,
which is the owner's own rule. `road::lit` is where that is decided and it
is measured from a settlement's own site, so it cannot drift from where
the towns are: `LIT_NEAR` is 1.5 km, and a lamp stands every `LAMP_EVERY`
(45 m) along it, seven metres up and half a metre off the carriageway,
ALTERNATING sides, so it is ninety metres between two on the same side,
which is what a staggered pair on a two lane road is. In METRES and not
in PIECES, which is the dashes' own mistake in the same file: one lamp
every third piece is one lamp a kilometre, and the port's whole lit
approach, which is about a kilometre of open road between the town's own
band and `LIT_NEAR`, carried exactly ONE light, standing where the camera
was. The night picture had nothing on it at all.
`the_lamps_on_an_approach_are_staggered_a_stride_apart` holds both the
spacing and the stagger, and the harness says where the lighting actually
STOPS rather than restating the constant: road 0 out of the port is lit
from its first open piece to piece 18, **1,354 m of approach**. It is
that line the road camera is aimed off now, and the camera's own look
ahead and stand off are in METRES rather than in pieces for the reason
the dashes and the lamps are: written as a count of pieces they were
341 m of road at the old spacing and 85 at the new, so quartering the
piece quartered the framing and the picture came back with the camera's
nose on the tarmac. The post only DRAWS, which is this file's rule
that anything a body should pass through is trim: a lamp post is not what
stops a car.
The lights themselves are `lamps.rs`'s, unchanged but for knowing that a
lamp can be a road's as well as a town's, and a road's is indexed along
the whole ROAD because a stretch of it streams.

**And it JOINS the town now, on the slip above.** `roads::slip_of` is
the number and the harness prints it: road 0's slip is 14 pieces over
40 m, from the highway's mouth 169 m out of the port to a crossing
158 m out, **ending 0.00 m from the town's own paving**. The highway's
mouth stands where it does because the road leaves along a bearing
where the town is NARROW, so its tarmac starts at that bearing's own
`Site::level_r`, the outline plus a 12 m apron; the town's outermost
block frontage on that bearing is further in, which is what the slip
crosses. It is not the SKIRT: dropping `field::site_skirt` from
`road::open` moves the levelling 24.6 m inward and the mouth not at
all, because the first tarmac is laid at a STATION and the stations are
85 m apart. That measurement is why the skirt is still in `open`, where
it keeps the corridor out of the band the town's own site is winning:
inside it a road's tarmac floats 0.10 m off its own 0.15 m lift, and
past it 0.003.

**A BEND IS A CURVE A CAR CAN HOLD, and it was a corner with no
radius at all.** The router hands back waypoints ten kilometres apart
and a vertex of that chain turns the whole deflection in one step:
measured at the spacing the road is actually laid in, a 45 degree bend
was a **111 m** radius and a right angle **60 m**, which at 160 km/h is
twenty gravities. `road::centreline` fits a circular curve at every
vertex now (`road/align.rs`), and the radius is the real discipline's
own formula at this world's own design speed: `R = v^2 / (g (e + f))`
with `e` 8% of superelevation and `f` 0.10 of side friction is
**1,116 m**, which is AASHTO's `V^2 / (127 (e + f))` in SI.
`docs/civil-engineering.md` is the long form of that and of every other
road number here, and `CLAUDE.md`'s own rules now send a road generation
question to it first.

**The design speed is `driver::TOP` and not a number of its own**, which
is the rule about one number said once: a road is aligned for the
fastest thing that drives on it, and if the car gets faster the roads
get straighter without anybody editing two files.

**A curve is CAPPED twice and the caps are named where they bind.** The
tangent `R tan(delta / 2)` may not eat more than `CURVE_SHARE` (0.45) of
either leg, so two neighbouring curves cannot overlap; and the MID
ORDINATE `R (sec(delta / 2) - 1)`, which is how far the built road
leaves the line the router measured, is capped at `CURVE_OFFSET`
(250 m), because a curve that cut a headland off a coast road would be a
road in its own bay. Measured: 10, 30, 45 and 60 degree bends are all
built at the full 1,116 m and a right angle at **604 m**, where the
offset cap wins, which is 118 km/h and a bend a real road would sign.

**And it is DERIVED rather than stored**, which is `pieces` and `step`'s
own rule: the atlas keeps the waypoints and both the bake and the game
fit the same arcs to them. `road::CURVE` is in the atlas's fingerprint
for the reason `PIECE` is, and so is `EMBANK`, which claimed to be there
for a commit while `Atlas::fits` never read it.

**THE HIGHWAY SITS ON A MOUND, and the mound is part of the road.** The
owner's own diagram, and the defect it fixes is one this file had
already measured from the air: a corridor is 32 m of flat and the rings
put a cell of about a sixty fourth of its own distance under the eye, so
past a couple of kilometres the mesher has no sample inside the corridor
at all and draws the hill that was there before the road, while the
tarmac is laid to nine kilometres whatever the terrain does. Between
those two ranges the road is a ribbon hanging over a hill that does not
know about it, and the hill wins wherever it stands higher. Measured
before: the coarse ground stood **0.83 m over road 0's tarmac** at its
worst, and after, swept over THIRTY TWO roads rather than one, the
worst is **-0.49 m** and the median **-3.15**: the drawn ground is
under the tarmac everywhere on the body, which is the road being the
highest thing on its own mound rather than a ribbon over a hill.

**So the road CARRIES its own ground.** `ribbon::mound` draws the verge
out to `road::CORRIDOR` and the batter down the corridor's own skirt,
both sides, every point of it read off `Planet::surface` and sunk
`ribbon::BURIED` (15 cm). That is the SAME function the mesher contours
and the walker and the car collide against, so the mound that is DRAWN
and the mound that is STOOD ON are one surface by construction rather
than two that have to agree, which is this file's own rule about a wall
being one box that is drawn and collided arriving at the one thing here
that is not a box. `the_mound_is_the_ground_the_field_levels` holds
every one of its 5,208 vertices to that surface: -0.150 m to -0.150 m,
which is the sink and nothing else.

**And it is SUNK rather than laid on**, which is the pavement slab's own
trick: wherever the terrain really is drawn at this detail the terrain
wins and the mound is inside the hill, and wherever it is not the mound
IS the ground. What it cost is one more mesh a stretch, a CHILD of the
tarmac entity in the same frame with an identity transform, so it is
placed, rebased and dropped with it: one `Anchored` and one `Paved`.
The two are filtered out of ONE model by material
(`to_mesh_filtered`), because the two are mapped from different places:
the tarmac is a BUILT thing and maps in the stretch's own frame, and the
mound is TERRAIN and maps the way a chunk does, in the planet's frame
modulo the ground's own tile and carrying this latitude's climate, so
the grass on it tiles with the grass beside it.

**`road::EMBANK` is two metres now and it was half a metre**, which is
inside the LOD error it was supposed to clear. Two metres over the
corridor's eleven metre skirt is a batter of about one in five and a
half, which `docs/civil-engineering.md` puts between the 1:3 a car can
drive back up and the 1:6 a mower can take. It is over the walker's
0.60 m step, so the batter and not the shoulder is how a body gets up
onto a road now, which is what a real embankment is too.

**And `open` and `graded` are two flags where there was one.** A slip
runs over a town's own plateau, which the TOWN levelled and the road did
not, so it is open (tarmac is laid) and not graded (there is no
embankment of this road's to draw). The app's own splice already named
one flag doing two jobs as the thing to watch here.

**What is MISSING, named rather than hidden.** A road has no junctions
WITH ANOTHER ROAD: where two roads cross, two corridors overlap and the
field answers whichever it reaches first, and there is no give way, no
roundabout and no fan of tarmac where three arms converge. Measured on
this body, all **1,118 road confluences are at settlements and none is
in open country**, and the worst of them are at wayside villages nobody
has built, so what that wants is a HUB at a settlement rather than a
crossroads primitive, and it is named here rather than hidden. A car has no HEADLAMPS: its lamps are emissive
and cast no light, so a night drive out past `LIT_NEAR` is a drive in the
dark, which is what a day arriving on a world with unlit country roads
costs and is named here rather than hidden. And a road is not drawn in a
town, because the town's streets are there, so the join between the two
is a change of surface rather than a junction.

## Cities are JOINED, and the plan of a body is BAKED

`road.rs` routes the network and `atlas.rs` is the file it is kept in.
The owner's ask was a system that generates cities across a planet and
connects them with roads, statically, loaded into the procedural planet
at run time, and those are the two halves of it.

**A road network is not a line between every pair.** It is what wears in
when everyone walks toward the nearest town, so that is how it is built:
waypoints spread evenly over the whole body on the same golden angle
spiral `town::plan` picks its candidates off, ONE multi source Dijkstra
outward from every town at once, and a road wherever two towns'
territories meet. One search labels the whole planet, and the border rule
gets three things for free that a road per pair has to be told:

- **Two towns on different continents are never joined**, because no
  chain of land waypoints runs between them. 123 of this planet's 160
  towns are on the network and the other 37 are on islands, which is what
  an ocean world looks like: at 26% water it was 152 of 160.
- **A road round a bay is shorter than a road across it**, without
  anything knowing what a bay is.
- **The network is SPARSE and it is planar looking.** 155 roads for 160
  towns, against 12,720 pairs. A town ringed by others is joined to its
  ring and to nothing past it, which is a road network rather than a
  spiderweb, and no rule says how many roads a town may have.

An edge is refused into the sea (`DRY`, two metres over it) and up
anything past `STEEPEST` (SEVEN per cent, which is the owner's own
number and the grade a motorway is designed to), and it costs its
distance times `GRADE` (eight) on its own grade, so a route goes a long
way round a range rather than over it.

**Three defects, and the first two came back as nothing at all:**

- **The waypoint count was over FOUR rather than over four pi.** A
  sphere's area is 4 pi, so asking for `4 / spacing^2` points spreads
  them 1.77 times as wide as asked, which is wider than the neighbour
  reach: not one waypoint on the body had a neighbour and the search
  returned nought roads.
- **A town seeded the waypoint NEAREST it, and half of them are
  offshore.** Waypoints are ten kilometres apart and a town stands on the
  coast because that is where level land near the sea is, so the point of
  the grid nearest one is as likely to be in the water as on the land. A
  town that seeds a wet node owns no ground, meets no border and gets no
  road: 6 of 12 towns on the test planet were stranded, and 11 of 12 once
  a town seeds the nearest DRY waypoint.
- **And the bake took 551 seconds**, because `surface_radius` marches
  tens of samples down through the band and every one of them walked all
  hundred and sixty town sites. It is the rule this file already keeps
  for chunks, arriving at a new caller: a march is along ONE direction,
  so `Planet::around` filters the sites to that direction once and the
  whole bake is 29.4 s for the same 336 roads.

**What is BAKED is the PLAN and never the geometry.** A town's lots, its
streets, its buildings and its lamps are a pure function of where it
stands and its seed (`town::lay`), so the atlas keeps the placement and
the game lays the grid out again in milliseconds. The triangles would be
sixty million for one body, would be reshipped whenever a building kind
changed, and would say nothing a reader could check; the placements are
1.7 MB of JSON beside `planets.json`, and
`a_town_read_back_is_the_town_that_was_baked` holds every lot of every
town through the file.

**An atlas of another body is REFUSED rather than used.** The name, the
seed, the radius, the town size, the SEA and the OCTAVE COUNT all have to
match, because `Shape::oct` caps every term by the octaves: a plan made at
eighteen describes ground that fourteen does not have, and a road routed
over the one can cross water on the other. A refusal is not fatal, it
plans the body here and says so in the log, which is what keeps a
checkout nobody has baked runnable, the same rule a missing texture set
follows.

**And none of those six says what SHAPE the body has**, which is what the
PROBE is for. Every constant in `biome` is outside that fingerprint, so
moving one moves every coast on the planet and leaves an atlas that still
fits, with its cities standing in the new sea and nothing on screen to say
so. It happened the first time the shelf landed. The atlas carries the
body's own ground at twelve fixed directions off the golden spiral,
sampled BARE because a town levels its own site and the plan is what puts
the towns there, and a metre out at any of them is a different world. It
is the cheapest thing in the file that cannot be stale, and the refusal
prints what it read.

**The roads are on the CHART**, drawn along their own lines at half a
texel a step so the network cannot come out dotted, in `Kind::Road`, and
the CITIES are stamped over them. Painted the other way round each road
erased the town it serves, because every road ends at a town's own
centre: 37 city texels survived of 160, and the 123 missing were exactly
the towns a road reaches.
**And both are drawn WIDER than the ground, because a mark nobody can
see says nothing.** A town is eighty metres across and a texel here is
six kilometres, so a city drawn true is a seventy fifth of one texel;
drawn one texel it exists on the chart and still cannot be seen, because
a body from two and a half radii up is a disk about as many pixels across
as the chart has texels round its equator. A city is `CITY_TEXELS` (2.2)
and a road `ROAD_TEXELS` (0.9), measured against that PICTURE rather than
against the ground: three pixels of the disk for a city and one solid
pixel for a road. Both are round marks on the SPHERE and not squares of
the image, because an equirect texel narrows as the cosine of its own
latitude and a mark a fixed number of columns wide is a dot at the equator
and a hundred texel smear at eighty degrees.

**The cities and the roads are LIT, and the night side is DARK.** The
slope map's blue channel was a constant 255 nobody read; it is the body's
own LIGHTS now, written by the same `blot` that draws a city or a road and
brightest at a mark's middle, maxed rather than overwritten so a crossing
is not dimmed by the second road's falloff. `distant.wgsl` takes the sun
as a direction, measures the night on the body's own RADIAL rather than on
the bent normal (which half of a planet the sun is on is a fact about the
planet, and a normal leaned off a mountain would put a patch of midnight
on a slope at noon), and does two things with it: the albedo goes to
`NIGHT_FLOOR` (a twentieth) of itself, and the lights burn as EMISSIVE at
`LAMPS` (120) in sodium. A twentieth rather than nought, which is this
file's own ambient lesson twice over: nought is a hole in the picture
rather than a planet. What was making the dark side bright was never the
sun, it was the sky's own environment light, and a body drawn with nothing
but PBR is a lit blue ball with a terminator painted on it.

**And the dumped chart is written RGB now.** Its alpha is the water mask,
so the four channel dump opened with every scrap of land transparent and
every ocean opaque: the one picture that exists so a person can look at
the chart showed the planet inside out in every viewer.

**And that is what the section above builds on**: a `Site` is an ARC now,
so the corridor under a road is levelled the way a town's ground is, and
the tarmac over it streams the way a town's buildings do.

Measured on this planet: 160 towns and 155 roads over 36,716 km joining
123 of them, baked in 23.7 s and READ in 10 ms, against 7,200 ms to plan
the towns alone.

## The sets on the field, and the walker on it, in Bevy

**A triangle carries what it is made of, and there are eight.** `TERRAIN`,
`CONCRETE`, `PLATE`, `GLASS`, `LAMP`, `LIT` for a glowing pane, `STREET`
and `PAINT` for a road marking. A chunk's comes from the FIELD, which `dc.rs` asks half a fine
cell inside every triangle's middle (`HAND`), and the ground answers
terrain everywhere; a model's is whatever its own box or quad was given
(`model.rs`). The app puts it in the vertex colour's red, every corner of
the triangle the same, so no driver's choice of provoking vertex can change
it, which is the mockup's hatched walls not happening twice, and the town's
models wear the SAME material the ground does, so a wall and a hillside are
one shader and one set of sets. The sea's two, `SURFACE` and `BURIED`, are
the same channel on the other mesh.

**A texture coordinate is a number the CPU works out, never one the
fragment forms.** Every UV used to come from `rel = world_position -
planet_centre`, a planet scale vector held in `f32`. At the port `|rel|`
is 999,603 m, where one `f32` to the next is 6.25 cm, so the ground's own
coordinate took EIGHT distinct values over two metres of walking: a two
metre tile sampled in 32 steps. The concrete on a wall was worse in kind,
because it went through `dot(up, east_t) * base` with `up` off that same
quantised `rel`: 613 values over ten metres, treads of 1.5 to 4.5 cm, and
a scale that drifted (3.00 m read as 2.98). What that does to a picture is
not a shifted texture but a wrecked one, because a GPU picks its MIP LEVEL
off the DERIVATIVE of the coordinate: a staircase has a derivative of
nought along each tread and a spike at every riser, so the mip choice is
noise and the wall comes out streaked and jagged. It is the rule this file
keeps for meshes ("at a thousand kilometres that quantisation is six
centimetres and the warning is the whole surface") arriving at the one
place that still broke it, and it is why every earlier picture looked
right: on a five kilometre planet the step is half a millimetre.

Measured as a picture, the same frame before and after: 18.6% of pixels
moved by more than 8 of 255. So `to_mesh` hands every vertex the position the shader maps FROM, worked
out in `f64` where it is small and exact, and the fragment does no
arithmetic on it at all. A chunk's is its planet relative place reduced
MODULO the ground's own tile, which is what makes it small and keeps two
chunks agreeing, since a triplanar tiling is periodic and congruence
modulo the tile is all a seam needs; every chunk corner on this lattice
has the same residue, so the whole planet shares one offset. A built
thing's is the town frame position its model was already written in, ±90 m
and exact, so `in_frame` computes only the frame's AXES now (unit vectors,
which cost no precision) and never where the point is. The height over the
sea rides the same vertex, because the sand band is a metre and a half
wide and `length(rel) - sea` measured it in six centimetre steps.

**The sea had the same disease and it is the CELL AND FRACTION that cured
it.** `water.wgsl` took its ripples from `q = world_position - centre` in
`f32` too: over a metre of water the coordinate took four values, 8.3 cm
steps on features about 0.67 m across. The terrain's fix does not port,
because `fbm3` is NOT periodic, so there is no modulus to reduce by and an
offset per chunk would put a seam in the sea wherever two of them met.

The cure is the one this file named and had not built: a noise that takes
a lattice CELL and a FRACTION rather than one float an axis. `to_sheet`
works the chunk's cell out in `f64`, where it is exact, and hands it down
the vertex; the shader adds the vertex's own offset inside the chunk,
which is metres and exact, and DOUBLES the pair per octave, because an
integer times two is an integer and a fraction times two is a carry and a
fraction (`gnoise3_at`, `fbm3_at`). Nothing is reduced and nothing tiles,
so the sea is one continuous noise over a body two thousand kilometres
across. What it costs is tenebris's 2.1 and 2.3 octave ratios, which are
there so two octaves do not line up: a cell doubles exactly and a cell
times 2.1 does not, and a per octave TRANSLATION buys the same thing at
no precision at all.

The SWELL keeps the imprecise coordinate on purpose: its wavelengths are
tens of metres, so 6.25 cm is a thousandth of a wave and nothing an eye
can find. It is the ripples, at two thirds of a metre, that the same six
centimetres wrecked.

**The first render of it was a blank white sheet from the shore to the
horizon.** The cell rides the vertex COLOUR, and Bevy's standard material
multiplies its base colour by the vertex colour, so the sea drew at a
million times white. It is a channel this shader owns and it is masked
out before the standard material ever sees it. The A/B at eight hundred
metres is 0.003% of pixels, and that is honest rather than impressive:
at that range the ripples are already faded (`RIPPLE_NEAR` 30 m,
`RIPPLE_FAR` 160 m) and the quantisation is a MOTION artifact a still
frame understates, the pattern holding still and then jumping a twelfth
of a cell as the eye moves.

**The ground a walker stands on is the biome the chart paints.** The
climate rides the VERTEX, worked out in `f64` where the field is
(`chunk_mapping` asks `Shape::climate` per vertex, and per vertex rather
than per chunk because a chunk wide tint puts a hard line down every
chunk boundary on the planet). `temp` is the second uv lane and `wet` the
second uv set, and `terrain.wgsl` blends `Kind::colour`'s own table
between them rather than picking a kind, because a biome that snapped
would draw a line across the ground wherever the climate crossed a
threshold. Dry ground is SAND whatever its height, which is what makes a
desert a desert on foot rather than a tan patch from orbit, and cold
ground goes to snow. The tint is at `BIOME_TINT` (0.72) of the set's own
colour, so the texture still carries the grain and the shadow and the
tint carries what the place IS: the hay under the feet in a desert is
sand coloured hay. At nought, which is what this world was, every planet
is the same meadow.

**The shader is the mockup's, transcribed.** `terrain.wgsl` is an extension
on Bevy's standard material: `tri` and `triN` line for line (three planes
weighted by the normal's fourth power, a normal map read on each and turned
into the world), rock on the steep and grass on the flat by the same
smoothstep, concrete, plate and street where the triangle says so and mapped
in the nearest town's frame (the section on the cities above), glass, lamp
and lit as flat colours with an emissive, because what they are is a colour
and a glow and not a surface, and Bevy's own PBR lighting after. Sand is
the mockup's band by height, all sand to 1.3 m over the sea and all grass
past 2.8, measured off the sea's own radius, so a beach is what a walker
wades out onto. **A sample under a weight of NOUGHT is SKIPPED**, and that
is what makes a set cost nothing when it is not on the triangle: the
mapping's derivatives are formed before any branch and `textureSampleGrad`
carries them past it, so the sample is legal outside uniform control flow
and `tri` returns early. This file said the opposite for a long time
("every sample is taken whatever the material, because a texture sample
under a branch is not in uniform control flow and the compiler refuses
it"), which was true of `textureSample` and has not been true of this
shader since the gradients were lifted out; it is why twelve sets cost
what six did. The sets are twelve, basalt, dunes, grass, concrete and hull
plate on the ground and in the old town, asphalt on the road, and the six
a BUILDING is made of, as three array
textures of twelve layers each (`terrain.rs`; `FREEPORT_ASSETS` or the checkout the
binary was built from), and a missing map is a flat layer with a warning so
the harness runs anywhere. Every layer carries its MIP CHAIN, built at load
by a box filter down to one texel and laid out layer major, which is the
order wgpu reads, with eight samples of anisotropy: without the chain a
four metre tile of grass seen from seventy metres up is one texel a pixel
picked at random, which is the noise the far ground read as, and without
the anisotropy a street looked along is a stripe of one texel. The
ground's tile is two metres and not four, because a strand of the hay is a
quarter of a tile and at four a blade was a metre long, which read as a
ploughed field at a grazing angle; and the grass set's own normal is worn
at `GRASS_BUMP` (0.45) toward the surface's, because a hay normal at full
strength on ground seen at a grazing angle speckles, which is swarm-demo's
finishes at a fifth on a field.

**The walker is the mockup's, in the core.** `walker.rs` is the `Walker`
class and the marched page's three rules ported number for number: an eye
at 1.7 m, a body of 35 cm, a step of 60 cm, a head at 1.85 m, five metres
a second walking and eight and a half running, a jump at 5.3 m/s that
clears a metre; `ground` (the highest solid no more than a step over the
feet, else the first solid going down, else the first crossing from space
when the feet are not yet known), `ceiling`, `resolve` (a ring of twelve
points at three heights pushed out along the field's gradient, sideways
only, three passes), `can_stand` (fifty degrees, unless it tops out within
a step two body widths on) and `float` (the sea, above). The field it walks
is the ground the mesher contoured PLUS the boxes of every model within
reach (`World::field_near`), and those boxes are the ones the walls were
drawn from, so the picture is still the collider and a city is walked into:
in at a door, up to a wall, stopped by it. The harness is `walk.rs`: WASD, Shift, Space, the mouse, F to swap
with the fly camera from wherever it is, and a line of text saying where
the feet are and what they stand on. Five walks are the tests: two seconds
on a ball walks 8 to 10 m and running further; a jump peaks between 1.0 and
1.6 m and lands inside 1.3 s; a 0.4 m kerb is walked up and a 1.2 m wall
stops the body its own radius short; a wall walked into diagonally is slid
along, and a lintel a stride ahead holds a jump under it to 0.6 m; and the
sea holds the feet at wading depth. One lesson from writing them: on a
20 m ball a flat block three metres from the pole stood 0.22 m higher than
the curving ground, so a 0.4 kerb was a 0.63 wall and a wall's far end
stood clear of the ground; the test ball is two kilometres, and a real
block on a planet is built plumb on its own patch, which is the lot frame
and the streets in pieces again.

## A hex world was built, and it is set aside

The owner asked for hex terrain that gives way to a planet tessellation
with the geometry on the GPU, and it was built: a disc of Goldberg columns
round the eye and sp4cerat's Planet-LOD past it, both made in the VERTEX
stage with not one triangle uploaded, a walker standing on a column's flat
top rather than on the smooth field under it, water in columns inside the
disc and a sheet past it, and towns grown as raised and tagged tiles. Then
the owner looked at both worlds and preferred the cube marched one, so the
tiers came out: `hex.rs`, `lod.rs`, `columns.rs`, `stack.rs`, `grow.rs`,
`tiers.rs`, `tiers.wgsl`, `feed.rs`, `raise.rs`, `field.wgsl` and
`frame.wgsl`, 4,700 lines.

It is written down here rather than forgotten, because the measurements
are the argument for or against ever bringing it back, and the branch it
was built on still has every line:

- **Planet-LOD's split test is per EDGE**, so two triangles sharing an
  edge always agree about it and the mesh is crack free with NO neighbour
  lookup, no patch cache, no skirts and no streaming. That is the property
  that makes a far tier a shader at all, and it is the one to reach for
  the day this planet needs ground past the coarsest ring.
- **A tile is an ADDRESS and never a row.** A thousand kilometre planet at
  one metre tiles is 12,257,789,082,012 tiles, so everything about one is
  computed from `(face, i, j)`; a step off a face is carried across by
  unfolding the two triangles flat, and an icosahedron corner, where five
  meet rather than six, is the one place that is defective.
- **The vertex stage did 48 times the field work a compute dispatch would**
  (452,000 evaluations a frame against 9,409), which is a tenth of a
  millisecond on real silicon and most of four seconds on a software
  rasteriser. That is the measured argument for moving it, and it never
  had to be made.
- **A hex world has no walls in it on this relief**: at one metre tiles the
  steepest step between two neighbouring tiles is 0.24 m and the mean
  0.06, against the walker's 0.6 m stride, so the whole world is a
  staircase a walker climbs without stopping. A wall arrives at four metre
  tiles.
- **The one thing it was plainly better at** is that nothing is remeshed:
  a raised column is a write of one `f32` where a dual contoured edit
  dirties eight chunks. That is the argument to remember if editing ever
  comes back as the point of the game.
- **And the seam between the two tiers is an OVERLAP**, six tiles of it,
  because at a grazing angle one tile of overlap left a band of sky
  wherever the far tier stood higher than a column's flat top. No number
  found that; a picture did.

## A day is FOUR HOURS, and the BODY is held still while its sky turns

`day.rs` in the core is what time it is and where the sun stands.
`DAY` is 240 minutes, which is the owner's number: an hour of play is a
quarter of one and a night is two hours long.

**The body is held STILL and the sky turns, which is what a body fixed
frame IS.** Every direction this game reasons about is written on a
sphere that never moves: a town is a direction, a road is a chain of
them, a chunk's corner is an index off a lattice pinned to the centre.
Spinning the planet would mean moving all of that once a frame, and
re-meshing nothing, for a picture identical to turning the ONE vector
the light, the dome, the fog, the sea, the lamps and the impostor all
read. So the sun goes round `day::AXIS` and the ground does not, and
`AXIS` is plus y because `biome::Shape::climate` already reads latitude
straight off `dir.y`: the one axis in the crate that means something,
rather than a second one to keep in step with it.

The turn HOLDS the sun's declination and moves only its bearing, so a
body keeps its seasons and gains its hours, and the sign is the one that
takes the sun WEST over the ground: east at a direction is `AXIS cross
up` (`town::frame_at`), so a sun leaving plus x toward plus z is a sun
setting. `the_sun_crosses_the_sky_from_east_to_west` measures it at
twelve places rather than at one lucky longitude.

**An HOUR is nameable, and that is the whole reason `highest` exists.**
A sun direction says nothing about what time it is ANYWHERE, so a
picture asked for at dawn or at midnight cannot be aimed by hand, which
is this file's own solved camera rule arriving at the clock. `highest`
solves the turn to a place's own noon in closed form: the elevation
through a day is `A cos t + B sin t + C` with `A = up.x sun.x + up.z
sun.z` and `B = up.z sun.x - up.x sun.z`, so it peaks at `atan2(B, A)`.
`oclock` is twelve MINUS that turn on a twenty four hour dial (a sun a
quarter turn short of its noon is six in the morning, not six in the
evening, and written the other way up the clock runs backwards and every
hour in it is still a valid hour), and `at_oclock` is the inverse:
`--hour 0` is the port's own midnight and `--hour 6` its dawn, on any
planet, any seed and any port. That closes the `--nightwater` gap this
file named: a shore at a given sun elevation is now a flag.

**The lamps come on because the sun went down, in ONE place.**
`day::daylight` is the terminator, `DUSK_FROM` 0.14 to `DUSK_TO` -0.10
on the sine of the sun's elevation over a place's own radial, a BAND
rather than a line because a planet has air. It is the reference and
there are THREE transcriptions, each naming it: `distant.wgsl` fades a
body's albedo to its night floor and burns its cities, `water.wgsl`
decides whether a sheet mirrors the day sky or the one the dome is
painting, and `terrain.wgsl` lights a town's street lamps and its
windows. `lamps.rs` calls the core function directly, so the light a
lamp CASTS and the glow the pane is drawn with are one answer rather
than two that have to agree. Before this the constants were written out
twice in two shaders and the ground had no night at all: a lit pane at
noon reads as a shading defect and a street lamp at noon reads as waste.

**`dim_lamps` is a system of its own and that is the point.** Which
lamps exist is a question about where the EYE is (`light_lamps`, its set
held still until the eye has gone four metres); how bright they are is a
question about what TIME it is. Folded into one system it would have
been an eighth argument, which this file calls the smell that says a
struct is missing, and the answer here was that there are two jobs. It
also means a lamp dims through dusk on its own rather than waiting for
the eye to move.

**The sky that LIGHTS the world is baked again as the sun turns**, on a
thread of its own. `RE_BAKE` is half a degree, which is the sun's own
width and the finest step worth taking, since what the cubemap feeds is
an ambient averaged over a whole hemisphere; at four hours to a day the
sun covers it in twenty seconds of play and a bake is four to twenty two
milliseconds. It is a THREAD and not the frame's own work because
twenty two milliseconds is more than a frame: done in line it would be a
visible hitch every twenty seconds, which is a worse picture than the
stale ambient it replaces. A `JoinHandle` and not a channel, because
there is one answer and no queue, and `is_finished` is the poll. Nothing
had to be written to make the filtered light follow: `StaticEnvironment`
caches Bevy's own filtering on the SOURCE TEXTURE's id, so a new image
is a new texture is a new filter.

**AND THE SUN GOES OUT ON THE NIGHT SIDE.** A directional light shines
on every surface whose normal faces it, and nothing in a shadow cascade a
few hundred metres deep can put a PLANET in the way: at midnight the sun
stood under the ground and every wall facing it was lit from below, which
is sunlight shining up through the world. The owner saw it and named
where the answer is.

It is TENEBRIS's, and it is one line there too. `hex.vs.glsl` carries
`v_sun_brightness = smoothstep(term_lo, term_hi, dot(radial, sun))` and
`hex.fs.glsl` multiplies the sun's own diffuse by it, so which half of a
planet is in its own night is decided on the RADIAL and never on the
surface normal. That is `day::daylight`, which this crate already carried
and which `distant.wgsl`, `water.wgsl` and `terrain.wgsl` already
transcribe; the sun was the one thing not reading it.

`turn_sun` scales the light's own illuminance by it, at the EYE's own
radial, because Bevy's light loop is inside `apply_pbr_lighting` and
there is nowhere to scale ONE light per fragment without writing the loop
again. On the ground that is exact to a tenth of a degree, which is what
a 1.8 km horizon subtends; from the air near the terminator it is one
answer for a scene spanning several degrees of it; and from orbit the
impostor does the same rule per fragment off its own chart. Measured as a
picture, the port at eleven at night: 8.55% of pixels moved by more than
8 of 255, worst 220.

**What is MISSING, named rather than hidden.** The cubemap is baked for
where the eye was when the sun last moved, so flying from a beach to
orbit carries the beach's own sky up with it until the next re-bake; the
trigger is the sun and not the altitude. The sun is one direction for
the whole system rather than a star a body orbits, so `noon` does not
change when you fly to another planet. And nothing else knows what time
it is yet: the traffic runs at the same rate at midnight as at noon, and
a shop is not shut.

## The driver's HUD and the MAP are BUILT, from the page the owner approved

The owner's ask: a proper gauge for the fuel, a speedometer, the money
in the wallet, a frame counter in the corner, and a map to know where
you are driving with markers you can put down by hand for a route, A*
being a feature after it. A screen goes through this file's own mockup
rule (rendered, published, approved, then built), and
`docs/mockups/driving-hud.html` is the page, read by the owner six
times over: the tank's PERCENTAGE went, because the gauge is that
number and a figure beside a dial saying the same thing is the dial
not trusted; the game's own HOUR stands in the corner over the frame
counter, because a day here is four hours long and how far off dusk is
is a thing a driver wants to know; G is CONTEXTUAL, a prompt only
while the car is stopped within `PUMP_REACH` of a forecourt, which is
`fuel.rs`'s own rule for the key and never a line in a legend; H, the
hour menu, is a debug panel and not a driver's control; and then the
legend went altogether, M and E with it, because it is easy enough to
figure out, so the wallet is the top left corner at the size of a
readout, the next pump the bottom left, and the two dials ROUND at the
bottom right. `hud.rs` is that page in Bevy and `map.rs` the map
behind M.

**Nothing on it is a number the core does not already compute.** The
speed is `Driver::speed`, the gauge is `Tank`, a SHARE of a full one,
so the figure under it is `Tank::reach`, RANGE at eighty kilometres a
fill, which is what a driver actually wants; the wallet is `Wallet`
and moves on G at a pump and nowhere else; the next pump is
`Network::nearest_pump`, in the colour of the ALARM once it is further
than the tank reaches, because "amber and 6 km" and "amber and 40 km"
are different decisions and the gauge is for that decision; the clock
is `clock::reading`, the same line the status bar carries; and the
frame counter is a `std::time::Instant` and never Bevy's `Time`, which
is this file's own rule, and prints the milliseconds beside the rate,
because 16.7 ms is a number that can be held against a budget.

**A dial is an ARC and a needle, and both are UI nodes.** The arc is
a `BorderGradient` of one conic gradient on a round node's border,
starting at `ARC_FROM` (bottom left, 225 degrees round from the top)
and sweeping `ARC_SWEEP` (three quarters of a turn) with a hard stop
where the fill ends and nothing past it, over a second ring carrying
the track; a needle is a bar hung off a node of no size at the dial's
middle, turned by `UiTransform::from_rotation` through the same two
constants, so the arc and the needle read one number twice and cannot
disagree. The compass strip is twenty four ticks and eight letters
slid `width / 120` a degree under a lubber line, the first marker's
tick and its distance on it, so a route is driven without opening the
map; `hud::bearing` is the heading clockwise from north in the place's
own frame (`town::frame_at`) and `hud::toward` a marker's bearing and
its distance along the ground, and both are tested on the sphere and
never on a plane.

**The status line GOES at the wheel.** It stood under the compass
strip for a commit, and the side by side with the page found it
running through the strip and the clock: the page has no line of the
harness's own over the HUD, and what the line says is in the log. On
foot and in the air it is the line along the bottom it always was. It
is `status.rs`, because `main.rs` went over this file's own nine
hundred lines the moment the HUD was wired in.

**The map is a MODE and not a screen**, which is swarm-demo's sensors
manager rule: M covers the drive rather than leaving it, the car goes
on being driven under it, Esc or M brings the road back, and the mouse
is given back while it is open, because a map a player clicks on is a
map the cursor has to be free for. `map.rs` is the mode, `map/raster.rs`
the PICTURE (the next section) and `map/draw.rs` what is drawn over it:

- **An OVERLAY camera**, a `Camera2d` at order one that clears nothing
  and draws over the world, on `RenderLayers` 2, because 1 is the LOD
  wireframe's, with the page and the picture as sprites at the bottom,
  `MapGizmos` as a gizmo group of its own on that layer, `Text2d`
  labels, meshes for the markers and the car, sprites for the pumps and
  UI nodes aimed at it for the panels. The 3D camera says
  `IsDefaultUiCamera` outright, because two cameras on one window with
  no word on which draws the UI is a warning and a guess.
- **The projection is GNOMONIC**, the core's `map::View`, which the
  app's `Chart` only carries into `f32` pixels: ONE projection, because
  the picture is drawn in the core's and a marker set in a second one
  would be a hair off the ground it was set on everywhere but the
  middle. It is the sphere seen from over
  the map's own middle, east across and north up, exact both ways, so a
  click is turned back into a direction on the sphere without a search
  (`the_chart_goes_to_a_pixel_and_back` holds the round trip under a
  micron at a thousand kilometres). A zoom holds the ground under the
  cursor still by recentring, and ONE recentre is exact on a plane and
  not on the sphere, because the chart's frame turns with its centre:
  three notches left the ground 0.46 px off the cursor, and a second
  step closes it to under a hundredth, which
  `a_zoom_holds_the_ground_under_the_cursor` measures.
- **What is drawn OVER the picture is what only a map has.** The car's
  OWN road (`Network::nearest_road`, within `OFF_ROAD`) again and
  brighter, the route solid along its tarmac and dashed across a hop,
  the markers as the page's filled cyan discs with their numbers, the
  car as its filled amber arrow turned to its bearing, the pumps as
  the page's amber squares with the page showing through them, every
  settlement's name past its own reach on the map, and the port ringed
  in the route's colour while it is small enough for a ring to find
  it. The roads, the towns and the sea are the PICTURE's, so a road on
  the map is the road under the wheels drawn once and not twice.
- **A marker is a PLACE and never a road.** A click sets one, a press
  that moves `CLICK` (4 px) is a drag and not a click, a right click
  takes the last one back, `MOST` is twelve, and the legs from the car
  through them are summed along the GROUND (the angle times the
  radius, this file's own rule) against the tank in the route panel,
  which says "short" when the tank does not reach. The legs are the
  way over the ROADS between them now, which is the section on the
  route below; the crow's distance, which understates a drive round a
  bay, is what a leg falls back to where no road joins its two ends.
- **The scale bar is a round number that fits**, `scale_of` on a
  ladder from 100 m to 50 km inside 160 px. The first ladder started
  at 500 m, and at the two metres a pixel a town is looked at that is
  a 250 px bar in a 160 px box, which the test found before a picture
  did.

**A scripted drive MARKS its goal**, in `aim_drive`, which is what a
player does before setting off and what a headless run has no pointer
to do: the compass strip's tick and the map's first leg are then in
the picture. `--map` opens the map once the car is at the wheel, which
is M pressed by a run that has no key to press, and HALFWAY to the first
marker, which is M and then a drag: the next town is fifty kilometres
off and the map is sixty two across, so a map opened on the car had the
route running off the edge of the picture.

**What it decided against is a minimap.** A small always-on map in a
corner was drawn and taken out: at the scale a car covers ground it is
either a blur of the road under the car or too coarse to place a pump
on, and the compass strip carries the one thing a driver needs from it.

**What is MISSING, named rather than hidden.** The G prompt is tested
and not photographed, because the scripted drive
never stops at a pump; and the HUD is laid
out in shares of the window and not scaled by its height the way
swarm-demo's deck is, so on a very wide window the dials stand further
from the strip than the page draws them.

Measured, in the tests: the chart's round trip is under a micron at a
thousand kilometres over 117 directions across the window; a zoom of
three notches leaves the ground under the cursor 0.46 px off with one
recentre and under 0.01 px with two; the scale bar's first ladder put a
250 px bar in a 160 px box at two metres a pixel; and the frame counter
reads 60 frames over a second as 60 fps and 16.67 ms. The first render
found what no test could, a panel builder that carried `BorderColor`
twice, which is a panic at spawn and not a compile error.

**Measured, in the pictures, on lavapipe at `--levels 5 --drive 20`.**
The HUD at the wheel: 1,652 chunks and 171,196 triangles settled in
177.5 s at 38.4 ms a chunk on two workers, the car stolen in the port
and its goal marked 51.63 km off; the lubber line stands at pixel 639
of 1280, the strip reads SW and W either side of it with the goal's
tick and "51.6 km" at 845 to 921, and the fuel arc sweeps clockwise
from the bottom left of its ring to the top for a tank at 52%, the
same 41 km the status line prints beside it, so the dial, its figure
and the harness's own line are one number three times. The map over
it, at 48 m a pixel across 62 km: the four roads out of the port with
the car's own drawn brighter, the port ringed and labelled, the pumps
on the roads, the leg to the marked goal running off the frame, the
route panel reading one leg of 52 km against a tank that holds 41 and
saying short, and a 5.0 km scale bar of 103 px. And the DIM under the map was measured
rather than the page's number: the page composites in sRGB, where its
0.82 leaves the drive at 18% of its brightness, and Bevy composites in
linear light, where 0.82 left the street under the map at 45% in sRGB
(17% in linear, which is the alpha doing exactly what it says in the
wrong space). The map is opaque now and the lesson went to the GLASS,
which is the same mistake at the page's 0.58: the next section.

## The map is a PICTURE of the ground, and the HUD is the page's own glass

The owner read the first map off a picture and named what was wrong
with it: "I want an actual map, not a screen overlay", a terrain height
map with the water, the roads as they are and the buildings, "a
rendered version of the height map plus roads and buildings top down".
The page's own map was a diagram over a dimmed drive, lines and rings
and a grid of wet cells, and that is what the first build carried.

**`freeport_core::map` DRAWS it, because a picture of the world is a
function of the world.** One sample of the planet a pixel, on the
gnomonic `View` the harness turns a click back into a place with, over
threads the caller hands in:

- **The land is tinted by its height and shaded by its slope**, a
  relief map's own order from lowland green through olive and tan to
  grey rock and snow, lit from the north west and steepened four times,
  because relief seen from straight over it is flatter than it looks
  from the ground. The lowest land is the brightest green on the page:
  the first tint put it at the darkest, and the port's own valley floor,
  a coastal plain falling from 25 m to 4 m over seven kilometres, read
  as a hole in the picture.
- **It is CONTOURED at the scale's own interval**, the finest of 5, 10,
  20, 50, 100, 200 and 500 m that is at least two pixels of scale (a
  hundred metres at the region's 48 m a pixel, five at a street's 2),
  on the HIGHER side of the height it marks so a line is one pixel and
  never two, every fifth an index line drawn darker, which is how a
  topographic sheet lets an eye count height with no label on it.
  `a_contour_interval_follows_the_scale_and_its_lines_are_one_pixel`
  holds the ladder and the one pixel.
- **The sea is by its depth**, lighter over a shelf and the page's own
  blue past it, with its coast drawn.
- **A road is its own line**, `World::routes` on its tarmac only, at
  its true width (6.9 m, the lanes and the shoulders) or two pixels,
  whichever is wider.
- **A town is its streets and its buildings**, every piece of paving
  and every lot at its own size and place, a lot under two pixels laid
  on the pixel it stands in by its own area so a town of buildings
  smaller than a pixel still reads as a town; and a town whose plan
  would be smaller than the page's own mark for its tier (a city seven
  pixels, a town five, a village three) is drawn AS that mark, which
  is the page's region and a real plan the moment there are pixels for
  one. `a_map_draws_a_towns_plan_and_its_roads_where_they_are` holds
  the town's pixels to between 0.8 and 1.3 of what its own plan covers.
- **And a town too fine to draw is drawn as its BUILT-UP AREA**, which
  is how a real map generalises a city as it zooms out. The map opens
  at forty eight metres a pixel, where a lot is a fifth of a pixel and a
  street a sixth, so laid on by their own area the port's lots came out
  as a faint grey smudge twelve pixels across and the owner asked
  whether a road grid like a city was supposed to be there. It was, and
  it was under a pixel. `map::built_up` lays every built block's own
  cell, the block and half the street round it, in the town's colour,
  wholly while a lot is under half a pixel and not at all past a pixel
  and a half, and the plan is drawn over it. Each pixel asks where it
  stands in the TOWN's frame, four samples a pixel, rather than a
  quadrilateral being laid a cell at a time, because the cells tile the
  ground and two laid one after the other leave a seam wherever they
  share a pixel. `a_town_too_fine_to_draw_is_drawn_as_its_built_up_area`
  holds the fixture town at 12 m a pixel to 321 pixels changed against
  278 of its own cells.

**It is drawn on a THREAD and never in the frame**, `map/raster.rs`,
the sky's own rule: one answer and no queue, a `JoinHandle` and
`is_finished`. A pan or a zoom moves and scales the LAST picture under
the cursor at once, by the ratio of the scale it was drawn at to the
scale the view is at, and the new one replaces it when it lands; the
page under it is opaque, which is what a pan uncovers at the edge
until then. A screenshot with the map open waits for its picture.

**`--map-png PATH` draws one with no window**, at `--map-scale` metres a
pixel over the port or `--eye`, before any of Bevy is built, which is
the atlas bake's own rule for a thing that is arithmetic and a file.

**The HUD's GLASS was the map's dim a second time.** The side by side
with the page, element by element, found every panel a pale grey over
the sky where the page's are dark smoked glass: its 0.58 composited in
sRGB is 0.82 composited in linear light over a daylit sky, 0.80 over
mid grey and 0.72 over a dark street. And 0.82 was still grey, because
the page is dark because its WORLD is: measured off the second side by
side, the page's glass reads (32, 42, 57) over a night blue scene and
this one (103, 115, 115) over a noon sky of (215, 249, 249). The glass
is 0.94, the page's own darkness over the brightest sky this world has
and the page exactly over a dark street, and every panel on both
screens reads it. The same comparison found three things
the page has and the build did not: the speed dial's nine ticks, every
other one longer, and the fuel dial's one at half a tank; each dial's
own dark face under its arc; and the speed arc in the readout's cream
rather than the needle's amber. A tick and a needle are one builder
now (`hud::spoke`), a bar turned about the dial's middle, from the
middle out for a needle and from part way for a tick, so the two turn
by the one rule and cannot disagree about where the arc starts.

Measured, headless off `--map-png` on the harness body: a picture of
1280 by 720 is **326 to 349 ms on four cores** at 48, 8 and 2 m a
pixel, the same third of a second at every scale because it is one
sample of the planet a pixel and the towns and roads are a small part
of it. At 48 m a pixel the region reads as hills, the coastal plain,
the sea and the four roads out of the port; at 8 the port's own plan
stands in the middle of its plain; at 2 it is blocks, courtyards, the
square and the streets between them.

## A route FOLLOWS the roads, and A* finds it on the atlas's own waypoints

The owner named it as the feature after the map: markers put down by
hand, and the way between them found along the roads. `road/path.rs` in
the core is the finding and `route.rs` in the app is when to ask.

**The graph is the ATLAS's, and that is what makes it exact.** Every
road on a body is a walk of ONE Dijkstra tree over one set of waypoints
(`road::connect`), so two roads that share a stretch share the very
waypoints it runs through, bit for bit, and every road ends at its two
towns' own centres. A waypoint is a node, a step on any road is an edge,
and a junction is simply a node with more than two edges: a fork where a
trunk splits, or a town where roads meet. Nothing is matched by
distance, so there is no tolerance anywhere to get wrong; a node is
keyed on its direction to a billionth of the radius, which is a
millimetre here. The harness body is **4,456 waypoints and 4,668
steps**. The other graph there was to build is the refined lines the
roads are DRAWN on, which is 700,000 points and junctions found by
looking for points that stand near each other: a tolerance that is right
on a trunk and wrong at a crossing, which is exactly what the merge's
own `SHARE` and `LEAST` exist to tell apart.

**A* on the straight run still to go**, which is ADMISSIBLE because no
road between two places is shorter than the great circle between them,
and CONSISTENT because a step and the run after it cannot be beaten by
the run alone, so the first time the goal comes off the frontier it has
come off by the shortest way. The frontier is ordered on `total_cmp` and
then the node, so two runs pop the same node at every tie, which is the
network's own search's rule. `a_star_finds_the_shortest_way_between_every_pair_of_towns`
holds it against a search with NO heuristic over every pair of towns on
a real routed planet: 144 pairs, 74 joined and agreeing to the
millimetre, and the other 70 agreeing that there is no way at all.

**A step three roads share is run on the road that OWNS it**, the lowest
numbered, which is `trunk::merge`'s own rule for who carries a trunk's
tarmac: every other road there is snapped onto the owner's line and
CLOSED, so a route that ran a trunk on a sharer's line would run it on
points with no tarmac under them.

**A way is TRACED onto the lines the roads are drawn on**
(`path::trace`), because a waypoint is ten kilometres from the next and
a curve of 1,116 m cuts every corner by up to 250 m. Each leg runs from
the point of its road nearest where the last one ended, which makes a
fork ONE line, since a road forking off a trunk is snapped onto the
trunk's own line until it leaves; the first starts nearest the car and
the last ends nearest the marker, the places themselves and never the
waypoints they were snapped to. A step that is not on tarmac is a HOP:
onto the road from wherever the car is, across a town between the
crossings two roads end at, or through a village the highway stops short
of, which is every village a road passes (`road::open`).

**The map draws a route SOLID along the tarmac and DASHED across a
hop**, which is the approved page's own dashed line kept for exactly
what it meant there: a line that follows no road. The route panel sums
the legs ALONG the roads and says "no road" beside a leg with none,
since a marker on an island is a straight line to it, which is all a leg
ever was before. And the compass strip's figure is the DRIVE and not the
crow: the tick is where the marker IS and the figure how far it is to
drive there, because the bearing is what says which way and the
distance is what the tank is held against.

**The route is planned again as the car drives**, every `REPLAN`
(100 m), because its first leg runs FROM the car. Out of the port to the
next town it is **54.3 km along the roads against 51.6 km as the crow
flies, planned in 0.84 ms**: the search is 4,456 nodes and the trace a
walk of the few roads it takes, so re-planning every two seconds at the
car's top speed costs nothing a frame can find.

**And the scripted drive DRIVES it**, which is what closes the gap the
stolen car's own section named. `Auto::along` pursues the route along
its tarmac as far as `AHEAD` (400 m) and never further than the straight
line to the point stays within half the carriageway of every point of
the route it passes (`route::ahead_on`): a curve of 1,116 m holds that to
about 157 m and a slip's fifteen metre turns to a few, where a flat
400 m aimed the first run of this across the corner of the port and into
the building standing on it. It drives straight across a hop no longer than
`SHORT_HOP` (50 m, the step onto the road beside it and a fork inside
one corridor), and takes a longer hop, which is a town or a village, on
that town's own streets (`Streets::route`) to where the tarmac starts
again. Two things `through_town` needed for that, and both are the
highway's own geometry arriving at the car: a town REACHES as far as its
own tarmac does, which is its levelling and the skirt round it plus a
piece (`road::open`'s own reach), because a car at the end of the
highway outside a village was outside the village's OUTLINE and so in
no town at all; and a car ARRIVING from off the paving goes to the
nearest crossing first rather than the one after it, which is across
whatever stands between.

**And a car OFF its route inside a town takes the streets ONTO it.**
The route's first step is a hop from where it was planned onto the road
beside it, and a hop under `SHORT_HOP` is driven straight, which is
right in the country and wrong in a town: the first drive on the
chord bounded look ahead held its throttle against a building eighteen
metres from the port's slip for six minutes, `steering for the route
18 m off` at 3 km/h, because the straight line to the slip's first
point ran through the corner of a block. More than `ONTO` (the
carriageway and its shoulder, 3.45 m) off the route and inside a
town's reach, the car walks the town's streets to where the route's
tarmac starts.

**Off the route is measured to the TARMAC's LINE, never to a point.**
It was the distance to the route's nearest POINT, and the nearest point
of a route planned from the car is its first hop's own start, which is
where the car stood when it was planned: thirty metres down the slip
the car was twenty eight from it, called itself off the route, and
walked the streets back toward it. `route::off_tarmac` is the distance
to the nearest SEGMENT whose step is tarmac and never over a hop, and
the look ahead starts from whichever end of that segment the car is
nearer. Halfway between two points a hundred metres apart it reads
0.00125 m, which is the chord's own sagitta at a thousand kilometres.

**A scripted car BRAKES for a bend, or it orbits the point it aims
at.** Past the slip it held 160 km/h into the slip's fifteen metre
turns, ran wide onto the grass and went round there for two minutes,
because a point pursued faster than the lock can hold the arc to it is
a point the car circles. `driver::bend_speed` is the steering taper the
other way up, the fastest a bend of a given curvature can be held at:
an 8 m bend at 5.68 m/s, the slip's 15 m at 17.86, and anything past
about thirty metres, which is every highway curve, at the top speed.
`route::bend_limit` reads the tarmac ahead within braking distance and
gives the fastest the car may go and still slow for each bend in time
(`sqrt(v^2 + 2 a d)` at the script's own 8 m/s^2), and `Auto::pace`
takes the lower of that and the pursuit arc it is steering through,
never under a walking pace, so slowing for a corner is not taken for
being stuck.

**And a car that has JOINED its route keeps to the tarmac.** The
streets are the way ONTO the route from inside a town and never the
way back to it: a car a few metres wide of its line at speed and still
inside the port's reach was handed to the streets, which took it back
into town, down the slip and off its far end again, three times round
in twenty seconds. `Auto::joined` is a latch set the first time the car
is within `ONTO` of the tarmac and cleared only once it has strayed
`STRAYED` (60 m, a block and a street) off it, so a car that has run
wide comes back to its line rather than into town.

**Measured, the scripted drive out of the port: 9,380 m in 240 s, the
last three minutes of it at 160 km/h**, with the goal closing from
51.63 km to 44.97. The route was read against the car a second at a
time and plotted: down the slip, onto the highway fifty seconds in, and
on its tarmac line to the metre for all but one stretch, the V at 4.9 km
where the route turns back on itself through a hop, which the car ran
180 m past and came back from inside ten seconds.

**What is MISSING, named rather than hidden.** A route is shortest by
DISTANCE and nothing else: a highway and a village street cost the same
metre, a grade weighs nothing, and no pump is planned into a route the
tank cannot hold (the panel says "short", which is the moment to plan
one by hand). The way through a town is a hop on the map and a breadth
first walk of its streets under a scripted car, and the two are not the
same line. The map is still the page's own region at its widest, sixty
two kilometres across, so a route to anywhere past the next town runs
off the edge and is followed by dragging. And a crossing of two roads
that share no waypoint is not a junction, which is the rule the network
was built by: it has none.

## The hour is a MENU now, and it writes the one offset there is

`clock.rs` is a drawn panel the player opens with **H**: the hour, four
buttons that step the clock (an hour and ten minutes either way), four
that jump to dawn, noon, dusk and midnight, and a freeze that holds the
sun where it is. The owner's ask, and on a world whose day is four hours
long it is what lets "come back at dusk" be a thing a player asks for
rather than waits forty minutes for.

**It writes `Weather::start` and NOTHING else**, which is what keeps the
flag and the menu meaning one time: `--hour 6` and pressing DAWN both
end at `day::at_oclock` and the readout reads both back through
`day::oclock`. A second way of saying what time it is is exactly the
drift this file's own one clock rule exists to stop, and
`the_hour_the_menu_sets_is_the_hour_the_readout_says` holds the round
trip at nought, six, half past eleven, noon, six and eleven.

**A STEP is a share of a DAY and not a number of seconds**, so a body
with a longer day steps in its own hours: the buttons are `1/24` and
`1/144` of `Weather::day`.

**The freeze SLIDES the offset rather than stopping the clock.** The
app's own clock runs on whatever the sun does, so holding `now` still
means moving `start` under it by the same amount; letting go then
carries on from the hour it was held at rather than jumping to wherever
the app has got to. There is still one clock and one offset.

**H for the HOUR, because T is the TORCH.** The first cut took T,
which `lamps::hold_torch` has had since the day a walker could carry
one, so opening the clock would have flicked the torch on and off every
time: one press doing two things, which is this file's own divergent
path rule said about INPUT. The status line is the one table every key
in this harness is in, which is the only place a collision of two
features neither of which knows about the other can be seen, and it
names both now.

**And it gives the MOUSE back.** A panel a player has to press is a
panel the cursor has to be free for, so opening it releases the grab and
`grab_mouse` does not take it again while the menu is open: a left click
on a button must not also be the click that takes the mouse away from
him. It is the one place in this harness where a click is a press rather
than a look.

**This is a DEBUG panel and not a screen**, which is why it did not go
through this file's own mockup rule: it is nine buttons in a corner that
say what time it is, and the thing that would want a mockup is a HUD.

## The air is one march, and the sky is what lights the world

`atmos.rs` is tenebris's `atmosphere.fs.glsl` in `f64`, which is a GPU Gems
2 single scatter march under that: eight samples along the view ray, four
out to the sun from each, Rayleigh and Mie with their own phase functions,
and tenebris's two dusk terms (a glow along the sun and a band on the
horizon). It is in the CORE rather than in a shader alone because the sky
and the FOG have to agree and tenebris says why: the fog's colour is its
own sky sampled at the horizon every frame (`atmos::horizon`, averaged
over four bearings so a sun on one side does not glow behind the eye), so
the two match at noon, at dusk and at night rather than by a pair of
numbers somebody tuned to look alike. `atmos.wgsl` is the transcription
the dome runs, named function for function; the one deliberate difference
is that the GPU dithers the march by a hash of the pixel, because a fixed
offset bands a gradient across a screen, and the CPU takes the middle of
each step because it has no pixel and wants the same answer twice.

**The sky LIGHTS the world.** `sky::bake_env` marches the same function
into a 64 pixel cubemap at startup and the camera wears it as a
`GeneratedEnvironmentMapLight`, so a face turned away from the sun is the
colour of the air above it. The flat ambient is gone: what fills a shadow
is the sky, and what is left under it is a floor so a face with no sky over
it is the colour of the gap between two stars rather than a hole, which is
swarm-demo's own lesson.

**Ground fog POOLS.** The haze is measured at the MIDDLE of the view ray
and is `pooled` times thicker down at the sea, falling off over `pool`
metres, so a valley seen from a ridge is hazy and the ridge seen from the
valley is not, and a distant range reads as distant. `Air::round(radius,
relief)` is where tenebris's numbers are carried to another planet's size:
the shell is a multiple of the radius already, and the fog's lengths are
the GROUND's, so its density falls as the square root of the radius (how
far an eye sees is `sqrt(2 R h)`) and how deep the air pools is the relief,
because that is what the weather has to fill.

**Three whites and a black, each a different mistake.**

- **The sky drew BLACK**, because the march's nought to one answer was
  multiplied by the camera's own exposure (5.75e-4) as though it were a
  radiance. It is a SHARE, so it is scaled into candela (`atmos::NITS`,
  1,400) first and then taken through the exposure beside everything else.
- **The sky drew WHITE**, three times over. The Mie sum was collapsed to
  one channel, so the haze lost the per wavelength attenuation the Rayleigh
  sum has and washed the hue out. Tenebris's own last line is
  `1 - exp(-x)`, which is right for a shader writing an eight bit buffer
  and wrong here, because Bevy tone maps downstream and running BOTH
  compresses every channel toward one at the same rate exactly where the
  air is thickest: the horizon came out blue by a factor of 1.3 compressed
  and 2.0 uncompressed, and 1.3 is a white sky. And tenebris's lengths are
  shares of a three hundred metre planet, which is what `Air::round` is
  for. The coefficients were then SWEPT (`atmos::sizes::sky_colours`
  prints it) and taken at the setting where the zenith is blue by a factor
  of two and a half and the horizon is still the pale band it ought to be.
- **A BLACK sky, looking down from under the mean radius.** The march
  skipped every sample under `Air::ground` on the reasoning that under the
  planet's radius is inside the planet. It is not: the sea is four hundred
  metres under the mean radius on this world, so a walker on a beach is
  under it, and every sample of a ray that walker casts DOWNWARD is under
  it too. All eight were skipped, the march came back nought with an alpha
  of one, and the sky over a lit shore was pure black. It skips on
  `Air::floor` now, which is the lowest the ground goes, and `alt`'s own
  clamp already says that the air in a valley is the densest there is.
  `an_eye_under_the_mean_radius_has_a_sky_when_it_looks_down` holds it.
- **A dark stripe along the horizon**, which the owner's own eye would have
  caught and a picture did. `Air::floor` is the answer and the field's own
  doc comment is the long form: what stops a view ray going DOWN is the
  lowest the real ground reaches and never the mean radius, because
  wherever the terrain is lower than the mean the dome cut its ray short,
  marched fifteen hundred metres of air instead of two hundred kilometres,
  and drew a black band between the sky and the horizon that no terrain
  covered. Measured on the thousand kilometre planet: 2.93 at four tenths
  of a milliradian under the horizontal and 0.034 at eight, a factor of
  eighty seven across two pixels. It is also the only conditioning the test
  has, since in `f32` the difference of two squares at 10^12 is quantised
  to 131 km^2, which at twelve metres over the mean radius is half a per
  cent of the whole term and at eight kilometres over the floor is eight
  millionths.

**The dome is BEHIND everything, and how far behind is a function of where
the eye is.** It rides the eye, is drawn inside out, writes no depth and is
never culled, and its colour is a DIRECTION, so growing it costs nothing.
A fixed five hundred kilometres was enough while the planet was ten
kilometres across and is not at a thousand: from four radii up the eye is
three thousand kilometres off, so the dome stood IN FRONT of the planet and
painted it out, a pale blue disc with no ground in it at all. `dome_radius`
is `(|eye| + top) * 2` now, which is the far limb of the shell with the
margin doubled.

**The sun stands over where the WORLD starts, not along a world axis,
and never over the camera.** `sun_over` was asked about `start_eye`,
which is what `aim` RETURNS, so with `--sunward` the camera stands along
the sun and the sun is then measured over the camera: a circle. Measured,
`--sunward 2.6 --around 150` put the camera 58 degrees from the sun
rather than 150, and the picture of the body's own midnight came back
three quarters lit. It is asked about the walker's own start now, which
is what the next paragraph always claimed.


`SUN_UP` (32 degrees over the local horizon) and `SUN_BEARING` (40 round
from local north) are the numbers, and `sun_over` turns them into a world
direction at the harness's own starting point. It was a fixed world
vector whose comment claimed it stood "a little over the horizon at the
harness's start", which is a thing a world vector cannot promise: it is
true of one spot and the towns are placed by the ground. On the thousand
kilometre planet the port came out 56 degrees into its own NIGHT and the
picture of its main street was black with speckle, which reads as a
shading defect and is a clock. It is measured from the WORLD's start and
never from `--eye`, so two pictures from two places are lit alike and only
the camera moved.

**A frame cap, swarm-demo's.** `--fps`, 144 by default and nought to lift
it: vsync is the MONITOR's cap and not a cap at all, and a scene this cheap
to simulate draws at the refresh rate and holds the card at full clock for
frames nobody asked for. It is a DEADLINE rather than a fixed sleep, so the
cap does not drift, and a frame that has already overrun resyncs to now
rather than running the next few flat out.

## The sun goes out AT the horizon, and what is left is the SKY

A directional light shines on every surface whose normal faces it, and
nothing in a cascade a few hundred metres deep can put a PLANET in the
way: at midnight the sun stood under the ground and every wall facing it
was lit from below, which is sunlight shining up through the world. The
owner saw it and named where the answer is, and it is tenebris's, one
line there too (`hex.vs.glsl`): `smoothstep(term_lo, term_hi, dot(radial,
sun))` scales the sun's own diffuse, so which half of a planet is in its
own night is decided on the RADIAL and never on the surface normal.

**Then the owner named the second half of it: it is impossible for a
directional light to cast a shadow below nought degrees of the horizon.**
That is right and it is a fact about the BODY: a place's horizon is where
the sun's own centre crosses its tangent plane, and past it the planet
itself stands between the two. There is no beam to attenuate and nothing
for it to cast. The first cut faded over `DUSK_TO` to `DUSK_FROM`, and
`DUSK_TO` is -0.10, about five and a half degrees UNDER the horizon, so
the harness's sun was still burning at a fifth of its strength with the
sun set and casting shadows through the world.

**So there are TWO terms and they are different things**, which is the
whole of `day.rs`'s own distinction:

- **`day::daylight` is the DIRECT beam** and is nought at and below the
  horizon, by construction (`smoothstep(HORIZON, DUSK_FROM, ...)` with
  `HORIZON` nought, because a horizon is not a number anybody tunes). The
  band it keeps is ABOVE the horizon and is not geometry, it is AIRMASS:
  a sun at one degree of elevation is shining through forty atmospheres
  and delivers about a tenth of what it does overhead, so the beam comes
  up over the first eight degrees rather than switching on. `sky.rs`
  scales the `DirectionalLight` by this, so the beam and its shadows go
  together, and `terrain.wgsl` is its only other reader.
- **`day::twilight` is the SCATTERED light** and keeps the band either
  side, which is what dusk IS: the air above an observer is still lit
  long after the observer is not, and civil twilight runs six degrees
  past sunset. `distant.wgsl` fades a body's albedo by it, because a
  planet seen from off it carries a twilight ARC where the sun has set on
  the ground and not in the air above it; `water.wgsl` mirrors it,
  because what a sheet reflects is the sky; and `lamplight` is its other
  side, because what a street lamp is for is the light there IS. On the
  beam a town would switch on all at once the instant the sun's centre
  crossed the horizon.

`the_sun_casts_nothing_from_below_a_places_own_horizon` sweeps a whole
quarter turn either side and holds the beam at exactly nought below,
the sky alive through the band, and the beam never outlasting the sky.
`a_wall_at_midnight_takes_no_sun_through_the_ground` drives the real
system over a WHOLE DAY in tenths of an hour rather than at named hours,
which is its own lesson: six and eighteen are only sunrise and sunset on
a place whose latitude the sun is over, and reading them as the
terminator put the first cut of that assertion 952 lux past its own
claim.

**At the EYE's own radial**, because Bevy's light loop is inside
`apply_pbr_lighting` and there is nowhere to scale one light per fragment
without writing the loop again. On the ground that is exact to a tenth of
a degree, which is what a 1.8 km horizon subtends; from the air near the
terminator it is one answer for a scene that spans several degrees of it,
and from orbit the impostor does the same rule per fragment off its own
chart.

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

- **Textures are Material Maker's, with TWO named exceptions**, and both
  are generators rather than graphs for the same reason: Material Maker
  does not export in this container. `tools/texkit.py` is what the two
  share (the integer hashes, the tileable value noise and its octaves,
  the worley, the stretched grain, the low pass, the normal off a
  height, the occlusion and the write or check), so one hash and one
  noise rather than two that could quietly disagree about what a lattice
  is. Everything in it is integer arithmetic and polynomials: no `sin`,
  no `cos`, no random module and no GPU, which is what makes a
  `--check` byte for byte mean anything, and the same rule the core's
  field noise keeps so that two clients agree. A material
  is a graph in `materials/<name>.ptex` and nothing else is its source. `tools/bake_materials.sh`
  exports every graph through Material Maker's own command line (Godot
  target, which is the glTF layout Bevy reads: `<name>_albedo.png`,
  `<name>_normal.png` in OpenGL green up, `<name>_orm.png` with occlusion in
  red, roughness in green and metallic in blue, and `<name>_heightmap.png`),
  box filters them to 1024 because the export writes 2048 whatever the graph
  says (measured), and puts them in `assets/textures/terrain`. `--check`
  re-exports and holds the committed maps within half a percent of pixels,
  a tolerance rather than `cmp` because a GPU render is not bit exact across
  drivers, and `tools/pngdiff.py` is what measures it: its `--max` left
  the limit's own value in the positional list, so every call with one
  exited on the usage text and the check reported every map as drifted,
  which is a check that cannot pass reading as a check that always fails.
  `tools/get_material_maker.sh` fetches the tool; it runs headless
  under Xvfb and lavapipe with the forward_plus renderer, and only that one:
  the mobile and GL renderers crash under lavapipe while this one exports
  cleanly, which took an afternoon to find and is written here so nobody
  finds it twice. Seven sets ship: basalt, regolith, ice, dunes, hull
  plate, grass and concrete, plus asphalt from its own generator, all
  SHALLOW, because a normal map at
  full strength on a flat quad reads as gravel (swarm-demo runs its finishes
  at a fifth).
- **ASPHALT is the exception, and the reason is written down rather than
  hidden.** Material Maker DOES NOT EXPORT in the container this was
  built in any more. Measured, on the SEVEN COMMITTED GRAPHS with
  nothing changed: it starts, reports its Vulkan device, and then sits
  for five minutes having written none of its thirty two maps, under
  forward_plus, mobile and gl_compatibility alike, and `--check` comes
  back in three seconds having found nothing. The graph is not the
  problem and neither is the new set; the rig is. So rather than ship a
  `.ptex` nobody can bake and a PNG nobody can check against it,
  `tools/make_asphalt_texture.py` IS the source for this one set: numpy,
  integer hashes, no `sin`, no GPU, six seconds, and byte identical on a
  re-run, which is what makes its own `--check` mean anything. That is
  still "every asset has a committed source", and it is the shape
  swarm-demo's `make_chitin_texture.py` and `make_crystal_texture.py`
  already have. Re-authoring it as a graph is a follow up for a machine
  where the bake runs.
- **And a road was CONCRETE DARKENED, which is two things wrong.** The
  street wore the concrete set at `1 - 0.45`, so it was the right grey
  by accident and carried concrete's own 2 by 2 PANEL JOINTS in its
  height map: a road drawn as poured slabs somebody had driven over.
  Asphalt is chips of stone in bitumen and has no joint anywhere on it,
  and it is DARK, about a tenth against concrete's half. Measured on the
  two albedo maps: 0.106 linear against 0.449, a quarter of the
  brightness. The paint is the asphalt set BRIGHTENED now rather than
  the concrete set, which is the same one trick in the right place.
- **The first cut of it read as GRAVEL**, which is this file's own rule
  about every other set arriving at the new one: a chip is 15 mm on a
  3 m tile, so at walking distance it is five pixels and every bit of
  contrast on it is noise. The chips span 0.06 to 0.21 of albedo rather
  than 0.05 to 0.27, and the normal leans at 0.20 rather than 0.30,
  which is swarm-demo's finishes at a fifth.
- **Grass is hay.** The first grass graph was noise coloured green, and on
  the ground it read as dots. The owner's brief was green hay with its
  stems flattened on the ground, and the graph is that now: two fields of
  strands, each an `fbm2` stretched fifty to one along one axis, the second
  turned thirty four degrees from the first, made tileable, each warped by
  a swirl so the hay lies in whorls, `scratches` over them for blades
  standing proud, a height off the strands, straw and green colorized off
  the same height and blended by a clump noise, the normal and the
  occlusion off the height. Material Maker's `noise_anisotropic` node is
  the obvious tool for a strand and it crashes the loader under lavapipe at
  every setting tried, so the stretch is a transform on `fbm2`. It took
  five bakes, and the tile was looked at before the planet was each time.
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
cargo test -p freeport_core                       # 227, the core, about 100 s
cargo test -p freeport_app                        # 63, the harness. It was NOT in this list and
                                                  # went uncompilable for a commit with nothing to say so
python3 tools/shape.py --check                    # no file over 900 lines, no function over 100
cargo fmt --all -- --check                        # the format
cargo clippy -p freeport_core -- -D warnings      # the core's lints
cargo clippy -p freeport_app                      # the app's count never rises: it is nought
tools/bake_materials.sh --check                   # the maps match their graphs (Material Maker does not run in every container: see the asphalt note)
python3 tools/make_asphalt_texture.py --check     # the asphalt set matches its generator, byte for byte
python3 tools/make_building_textures.py --check   # and the six a building is built of
python3 tools/pngdiff.py before.png after.png     # a refactor's pictures, against the scene's own floor
cargo build --release -p freeport_app             # the harness (needs libwayland-dev libxkbcommon-dev libudev-dev libasound2-dev on Linux)
./target/release/freeport_app                     # a window: on foot on a street of the port, F flies, E steals a car, M the map, G gas, H the time menu, T the torch, Tab wires, Esc frees the mouse
./run.sh --test                                   # the core suite and the shape check, then the build and the window; run.bat is the Windows twin, --shot out.png takes a picture with no display
# Every headless run below is under xvfb-run with
# VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json, and `--octaves` is
# what a picture on a software rasteriser is bought down with, since the
# field is what a chunk costs.
./target/release/freeport_app --octaves 14 --levels 9 --frames 20 --shot town.png    # the port's main street, on foot, with its models round the walker
# The same street at an HOUR. `--hour` is a twenty four hour dial over
# wherever the WORLD starts, solved back into the clock by
# `day::at_oclock`: twelve is the sun at its own highest over the port
# and nought its midnight. A day is four hours, so the clock runs on
# while the picture is taken and moves under a minute of game time over
# thirty frames, which is a fiftieth of a degree of sun. The pair is
# what shows the lamps: out at eleven and lit at eleven at night.
./target/release/freeport_app --octaves 6 --levels 5 --frames 6 --hour 11 --shot day.png
./target/release/freeport_app --octaves 6 --levels 5 --frames 6 --hour 23 --shot night.png
./target/release/freeport_app --fly --octaves 14 --levels 9 --frames 30 --eye 0,1000012,-14 --look 0,1000012,60 --shot graze.png   # the horizon dead level, which is where the dark band was
./target/release/freeport_app --fly --octaves 14 --levels 9 --frames 30 --eye 0,1030000,0 --look 0,1000000,120000 --shot high.png  # 30 km up: the curve, the sea and the haze
./target/release/freeport_app --octaves 14 --levels 9 --walk 600 --frames 620 --shot walked.png   # ten seconds of walking, the distance said every second
# STEAL the nearest car and drive it: the chase camera, the asphalt and
# the town going past. `--drive` is E and then W held, since a headless
# run can press neither.
./target/release/freeport_app --octaves 14 --levels 9 --drive 300 --frames 320 --shot stolen.png
# The DRIVER'S HUD at the wheel, and the MAP over it. A scripted drive
# marks its goal, so the compass strip carries the tick and the map the
# first leg; `--map` opens the map once the car is at the wheel, which
# is M pressed by a run that has no key to press.
./target/release/freeport_app --levels 5 --drive 20 --frames 40 --shot hud.png
./target/release/freeport_app --levels 5 --drive 20 --map --frames 40 --shot map.png
# And DRIVE TO THE NEXT TOWN. `--drive N` is N SECONDS aimed at the
# nearest settlement that is not the one it stands in, a second a
# rendered frame. The OCTAVES are left alone, because the atlas is
# refused at any other count and a body with no villages and no roads on
# it is the one thing this picture is of: at 14 there are 160 towns
# hundreds of kilometres apart and nowhere to drive to.
./target/release/freeport_app --levels 7 --drive 800 --frames 810 --shot trip.png
# The body from orbit, lit: `--sunward N` stands N radii off along the SUN
# and looks at the centre, which is the only way to aim this that works.
# The sun stands over wherever the world starts, so where it is depends on
# where the towns came out, and a hand aimed camera finds a planet in its
# own night.
./target/release/freeport_app --fly --levels 5 --frames 30 --sunward 2.6 --shot orbit.png
# The same camera turned round the body from the sun: 150 degrees is the
# NIGHT side, where the cities and the roads are lit, and 100 is the
# terminator with a lit hemisphere and a dark one in one frame. It is
# `--sunward`'s own rule for a half nobody can aim at by hand either, and
# the octaves are left alone here, because the ATLAS is refused at any
# other count and a body with no roads on it is the one thing these two
# pictures are of.
./target/release/freeport_app --fly --levels 5 --frames 30 --sunward 2.6 --around 150 --shot night.png
./target/release/freeport_app --fly --levels 5 --frames 30 --sunward 2.6 --around 100 --shot dusk.png
# The body's cities and the roads joining them, planned once and written
# beside the other assets. It runs BEFORE any of Bevy is built, so it
# needs no window, no device and no Xvfb: it is arithmetic and a file.
# The game reads it at startup and plans the body itself, slowly and with
# no roads, only when there is no atlas that fits.
./target/release/freeport_app --bake-atlas
# The CLIMB. `--over N` stands N metres over the port and looks straight
# DOWN at it, which is the one camera a LOD ladder can be judged from:
# the town holds still in the middle and everything else is how the
# ground gets coarser round it. A kilometre a step is the owner's own
# ask, and what it found was a twenty four fold cliff at the chart.
#
# The floor of it is the slow end, because the fine rings are all in
# play: 2,050 chunks and 131 s to settle at 2 km against 282 and 25 s at
# 49. The OCTAVES are left alone, like the drive's and the road's,
# because the atlas is refused at any count but the one it was baked at.
for k in $(seq 1 20); do
  ./target/release/freeport_app --fly --over $((k * 1000)) --hour 11 \
      --frames 20 --shot climb_$k.png
done
# And the HAND OVER itself, which is past the coarsest ring (262 km at
# fourteen levels) and so only in frame from about 356 km up.
./target/release/freeport_app --fly --over 400000 --hour 11 --frames 20 --shot handover.png
# The road ARRIVING at a city, which is the other end of the same
# camera: `--approach N` stands N metres over the tarmac out in the
# country and looks back down it at the town it runs into, so what is in
# frame is the highway crossing terrain to reach a place rather than
# leaving one. How far out it stands is solved off the camera's own
# height, the same rule `--road` uses.
./target/release/freeport_app --fly --levels 7 --frames 30 --approach 40 --hour 10 --shot approach.png
# The ROAD between two towns, along itself. `--road N` stands N metres
# over the tarmac a kilometre out of the port and looks down it: a
# camera for a picture is solved and never hand aimed, and where a road
# leaves a town depends on where the network came out. The first cut of
# it looked sixty pieces ahead, which is twenty kilometres and well
# under the horizon of an eye 25 m up, and the picture came back as bare
# hills.
# The OCTAVES are left alone, like the drive's, and these two recipes
# carried `--octaves 10` for a commit: the atlas is refused at any count
# but the one it was baked at, so all three renders came back with
# `0 roads are 0 stretches of tarmac` and a town's own street in frame.
# A flag that buys a picture down on a software rasteriser cannot be
# used on the one picture that is OF the atlas.
./target/release/freeport_app --fly --levels 5 --frames 40 --road 3 --hour 10 --shot road.png
./target/release/freeport_app --fly --levels 5 --frames 40 --road 3 --hour 22 --shot roadnight.png
# How far the ground strays from a straight ramp between two road
# waypoints, which is the measurement that decided `road::PIECE`. It
# reads the atlas's own segments off a file, because it is a measurement
# and not a feature.
cargo run --release -p freeport_core --example road_ground -- segments.txt
# And how far the terrain a chunk at each CELL actually DRAWS stands over
# the tarmac, which is the measurement that says the corridor cannot be
# carried through the LOD by widening it. It contours the chunk the game
# would contour and raycasts the road's own columns down it, with the
# corridor in the field and without, and takes the corridor's own width
# as an argument so the sweep can be re-run at any of them.
cargo run --release -p freeport_core --example lod_over_road        # 16 m, the shipped width
cargo run --release -p freeport_core --example lod_over_road -- 48  # and what widening buys
# The charts themselves, written beside the assets as PNGs.
FREEPORT_DUMP_CHARTS=1 ./target/release/freeport_app --fly --octaves 14 --levels 9 --frames 3 --shot n.png
```

The mockups are `docs/mockups/marching-cubes.html` and
`docs/mockups/hex-terrain.html`, opened straight off the checkout (they
find the baked textures in `assets/textures/terrain` from there) or served
from the repository root, and published:

- marching cubes: https://claude.ai/code/artifact/342b7f52-5a1f-4000-94b3-1d3967b527d1
- hex terrain: https://claude.ai/code/artifact/87905fd8-d47b-4f2e-8cc3-8c226251a799
- road mound: https://claude.ai/artifact/FMimcpVizLa3yttJYdBPtd
- city blocks: https://claude.ai/artifact/CamvaxSusg8UXXRYb3UUfW
- driving HUD and map: https://claude.ai/artifact/2i1EWdyyK5pkHbxB1KH16P

`docs/mockups/README.md` is the index of them, each file beside its link
and what it decided, so a page can be found again from the checkout.

They are the record of a decision rather than a picture of the game: the
marched page still builds its buildings out of brushes in a field, which
is what the game did and does not now, and the hex page is the world that
was set aside. `docs/freeport.html` is the design page and links both. **A
published page is republished in the same change as the file it was made
from**, which is swarm-demo's registry rule: a mockup a version behind is
worse than none, because a reader cannot tell which.

A headless Bevy run needs a Vulkan device; on a box with no GPU that is
`mesa-vulkan-drivers` for lavapipe and Xvfb, the same rig Material Maker
bakes on here. Numbers from lavapipe are not GPU numbers and are for
correctness and A/B only: a frame of the port on it is most of a second,
and one render at a time, because two share the four cores and both their
settle times lie.

## Measure, then decide

Numbers in the commit message. What is measured so far:

- `freeport_core`: 194 tests in about 40 s, and `freeport_app` 49 in 3. A 6 m sphere on a 32^3 lattice
  at half a metre marches to 5,288 triangles, a closed shell within 3% of
  the sphere's area, and dual contours to one at one level and across four.
- The planet is 1,000,000 m of radius, two thousand kilometres across,
  with 8,000 m of relief on 18 octaves and the sea 1,100 m OVER the mean
  radius, which leaves it 57.8% water in THREE continents (17.9, 11.0 and
  10.1% of the body) and 381 islands. At the continent term's old 0.30 of
  the planet's lumps and a sea at +1,000 m it was 62.2% water in seven
  pieces of 17.9, 5.3, 4.1, 2.2, 2.1, 2.0 and 1.2% with 217 islands,
  which is one continent and six scraps; at +820 m, 65.0% in 6 with the
  biggest 15.3% and 331 islands; at -400 m, 26.3%. Earth, for scale, is
  71% water in four contiguous masses of 16.6, 8.2, 2.7 and 1.5%.
- The towns, on that planet: eight planned in 155 ms (four thousand
  candidates on a golden spiral, the port first), and 699 buildings and
  7,744 pieces of street MODELLED in 30 ms into 192,646 triangles, 6,635
  collision boxes and 2,771 lamps, of which the nearest 48 are lights. The
  recipes they replace compiled in 14 to 20 ms and then cost a box test
  against every structure on every chunk for ever after; a model costs
  nothing after startup, because the mesher never sees it.
- What a building is made of: five kinds, a wall one oriented box that is
  both drawn and collided, a street one quad five centimetres over
  levelled ground and no collider at all.
- The dual contoured world, measured when the planet was 10 km, in release
  on lavapipe: eleven levels of 0.25 m to 256 m cells, 2,245 to 2,521
  chunks wanted of the rings' 5,632, the rest ruled rock or air without a
  sample; 387,308 triangles from 70 m over the port and 807,367 on its main
  street; 7.3 to 8.5 ms a chunk on three workers, 16 to 21 s of work,
  settled in 17 s from the air and 56 to 77 s on the street, the difference
  the frame rate of a software rasteriser drawing a city.
- `town::surface_radius` sphere traced on the field's own slope bound
  rather than stepped a fixed half metre: 400 directions in 15 ms against
  1,760 on the big planet and 9 against 15 on a five kilometre one, the
  same answer to the last bit (nought of 400 differ). `town::plan` is 155
  to 190 ms against 15 s.
- Precision, which is what a bigger planet actually costs: a vertex at a
  thousand kilometres is 6.5 cm out of place the naive way and 18 microns
  through `pos::unit_offset`, and `log2(2 pi R / lumps / 2 m)` is 18
  octaves against 11 on a five kilometre world.
- The sky under the horizon, measured on this planet with an eye 12 m up:
  2.93 at four tenths of a milliradian under the horizontal and 0.034 at
  eight with the ray stopped at the MEAN radius, a factor of 87 across two
  pixels and a black stripe along the whole horizon; stopped at the lowest
  the ground reaches it stays within a tenth of the horizontal's to sixteen
  milliradians down. As a picture: 0.361% of pixels over 8 of 255 and a
  worst of 232.
- The sets: five on the ground (basalt, dunes, grass, concrete and hull
  plate), at 1024 a side, three array textures of five layers, each layer
  with an eleven level mip chain built at load.
- The biome model, on this planet: of 8,000 directions, 2,609 forest,
  1,935 ocean, 1,854 snow, 1,031 grass, 604 desert, 346 savanna, 180 ice,
  38 marsh and 5 beach; the relief spans -3,554 to 4,158 m of a possible
  8,000, against plus or minus 1,615 m for the single fractal it replaced.
  `fbm3`'s own spread, measured over 20,000 directions: mean 0.498,
  standard deviation 0.106, 99 in 100 inside 0.262 to 0.736, whatever the
  octave count past four.
- What the mesher can close, measured on the rough test ball: a slope
  bound of 18.7 comes back with nought open edges and 94.7 with 32 open
  edges, 81 pinches and 1,192 triangles facing in. `MAX_SLOPE` is 20; the
  harness planet asks for 9.95.
- The charts: four bodies at 1024 by 512, baked on their own threads in
  851 to 894 ms, one `Planet::surface` a texel. It was 954 to 991 ms
  while `spot_at` sampled the field a second time for the colour beside
  the one the slope already held, which is half a bake spent on an
  answer in hand and a loop comment that claimed it was not happening.
  Baked with the sites in the field it was 1,617 ms, because half a
  million texels walked 160 towns each. The distant spheres are 24, 48
  and 79 subdivisions picked by distance, 79 being Bevy's own icosphere
  cap (144 comes back `TooManyVertices` with 210,252 points).
- What the chart's slope map was measuring, in three wrongs: an ocean
  carried its own sea bed at a mean bend of 45 of 127 against the land's
  50, and carries 0 now over 10,811 texels of open sea; a normal leaned
  +0.41 INTO the steepest northward hill on the body and leans -0.42 out
  of it now, on 400 of 400 of the steepest texels; and the east gradient
  read 0.121 where the truth was 0.297 at the poles, then 0.203 against
  the equator's 0.103 once it was divided by a run that shrinks and not
  yet spanned over matched ground. As a picture, the whole body from 1.6
  radii up: 18.06% of pixels moved by more than 8 of 255, worst 180.
- The towns: 160 planned in 7.4 s from 20,000 candidates, 8 built into
  2,836,252 triangles, 146,659 boxes and 1,358 lamps in 687 ms. READ off
  a baked atlas instead they are 9 ms, which is the whole argument for
  baking a plan that never changes. Where they STAND, taken lowest first
  against taken on a hash: 3 to 35 m over the sea, every one of them on a
  shore, against 3 to 2,332 m with a median of 448 and 32 of the 160 on
  an island.
- The land, before the shelf and after, on the same body: 6 pieces over a
  per cent of it with the biggest 15.3% and 331 islands, against 7
  continents of 17.9, 5.3, 4.1, 2.2, 2.1, 2.0 and 1.2% with 217 islands.
  Either side of that sea the seven are not seven: at +700 m the body is
  54.6% water and its land is one percolated mass, at +1,300 m it is
  72.8% water with nothing bigger than 4.7% of the body and 415 islands.
- The GPU sampler, once it compiled at all: 74,088 samples in 13.97 ms
  against the CPU's 37.82 on lavapipe, identical mesh geometry, and a
  sign-only density deviation of 440 m, which is the interval arithmetic
  doing its job rather than a disagreement.
- The roads on the WIDER continents: 297 of them over 61,178 km joining
  153 of the 160 towns, and 613 roadside villages rather than 544, baked
  in 18.9 s. More road and more of it inland is what a body whose land is
  gathered into three masses rather than scattered over seven looks like.
- The roads before that: 254 of them over 52,525 km joining 156 of the 160 towns,
  routed over 125,664 waypoints ten kilometres apart in one multi source
  Dijkstra. It was 155 over 36,716 km joining 123 while the body was 65%
  water with no shelf under it, and 336 over 134,464 km joining 152 while
  it was a quarter water: how many towns a network reaches is a fact
  about how connected the LAND is, and the shelf is what joined it up.
  The bake is 23.7 s, and was 551 s while every one of those
  marches walked all 160 town sites instead of the nought or one
  `Planet::around` leaves at its own direction. The atlas is 1.7 MB of
  JSON, which is placements and road lines and not one triangle.
- The sea's ripple coordinate, before and after: four values a metre on
  0.67 m features, against one continuous noise over a two thousand
  kilometre body at the precision of a fraction. The A/B at 800 m is
  0.003% of pixels, because the ripples are faded past 160 m there and the
  quantisation is a motion artifact a still frame understates.
- The sea's COLOUR, on pale-blue-dot's numbers, at the port's own shore
  with an eye at the waterline looking three kilometres out: 142, 152, 162
  at a saturation of 20, which is a grey sheet with sparkle on it, against
  65, 120, 173 at a saturation of 108. 17.1% of that picture moved by more
  than 8 of 255. For scale, that project measured a photograph of open
  ocean at saturation 134 in its middle band and 87 in its foreground, and
  measured every shine knob in its own shader as worth about one per cent
  of a sea frame against absorption's thirty.
- What a planet scale `f32` did to a texture coordinate, at the port, where
  `|rel|` is 999,603 m and one float to the next is 6.25 cm: the ground's
  own coordinate took 8 distinct values over two metres, so a 2 m tile was
  sampled in 32 steps; the concrete's took 613 over ten metres, in treads
  of 1.5 to 4.5 cm, with the scale drifting 0.7%; and the sea's ripples
  take 4 values a metre on features 0.67 m across. The first two are fixed
  by mapping from a position the CPU works out in f64 (18.6% of the
  picture moved), and the sea's wants a noise that takes a cell and a
  fraction. On a five kilometre planet the same step is half a millimetre,
  which is why every earlier picture looked right.
- **A lot's own base against the ground under it**, on the fixture
  planet: 1.13 m INTO the ground with the site levelling one radius and
  the survey walking 1.05 of one, and 0.00 m with both at the town's
  own `OUTLINE` of 2.06. The worst a lot now floats is 0.12 m, which is
  a dip between two neighbouring survey samples and is the one thing
  the design already said a site can still fill.
- **Houses stranded off the road network**, flooding the paving from the
  middle of town: 0, against 1 of the fixture's 11 lots with `home_run`
  neutered. On the harness planet the port lays 1,821 pieces of street
  for 224 buildings.
- **The ground at altitude**: at 49.4 km up the surface stood 32,192 m
  outside a 16,384 m box and NOTHING streamed, which is what the owner's
  `0 chunks, 0 triangles` said. Followed on the eye's own ground it is
  inside the box at 0, 50, 1,200, 12,000, 49,400 and 400,000 m up.
- **What was on the ROAD, and what the first two pictures of one found.**
  The centreline was a third of a PIECE of solid paint and two thirds of
  nothing, so the day picture came back with two edge lines and no middle
  at all; walked in `DASH + GAP` steps it is 33.3% paint with no mark on
  it longer than 3.00 m. The lamps were one every third piece, which is
  one a kilometre, so the night picture of a lit approach came back
  black; every 45 m and staggered, the port's approach carries a line of
  them. And the road CAMERA stood 1.7 km out against a `LIT_NEAR` of 1.5,
  so every lamp was behind it: one piece clear of the town rather than
  three. A lamp standard is `CONCRETE` and not `PLATE`, because hull
  plate's panel lines are centimetres apart on a column 0.18 m across and
  the post in the foreground came out candy striped.
- **A town's PLATEAU against the disc it replaces**, on the fixture
  port: the disc is 109 m of radius and the town reaches 40 m at its
  narrowest and 84 at its widest, so the levelled ground is **28% of the
  disc's own area** and 72% of that plateau was bare flat apron. Every
  lot is still on ground the site levels outright and the worst burial
  is 0.00 m. The outline moves at most **1.571 m of edge a metre of
  arc** over every bearing of a thousand towns, which is what
  `town::WOBBLE` (2) is set from and what a town's own skirt is widened
  by: 11 m becomes 24.6, and a town's skirt then climbs at 7.20 against
  the planet's bound of 36.57.
- **A car on a HILL**, ten seconds of throttle: **263.9 m on the flat,
  238.4 m at the seven per cent a highway is built at, 56.1 m at one in
  two, and BACKWARD down a one in one**, which is the grade the engine
  cannot hold. `Driver::holds` says where that is in closed form,
  `ACCEL / sqrt(GRAVITY^2 - ACCEL^2)` = **0.677**. Before gravity was a
  term at all it went 136.9 m up every one of those identically, which
  is a car that does not feel a gradient; and before the push and the
  rise allowance were fixed, one in two threw it **164 m back down the
  slope**. Stepped a twentieth at a time rather than a sixtieth it is
  the same distance, which is the rise allowance being a grade rather
  than a flat `CLIMB`.
- **How far the ground strays from a road's own ramp**, swept on this
  body's 310 roads (`examples/road_ground.rs`): 10.9 km a piece is a
  median of 43.89 m and a worst of 830.13; 341 m is 1.20 and **36.25**;
  170 m is 0.72 and 15.85; and **85 m is 0.54, a 99th of 2.10 and a
  worst of 4.56**. The worst is the number that decides it, because one
  canyon on a body is a road that disappears into the country. What it
  costs: 749,431 corridor arcs against 186,788, a bake of 99.7 s against
  23.8, an atlas read of 753 ms against 160, and a file of 6.6 MB against
  4.4 (13.9 before it was written compact).
- **And what a CHUNK costs, on the road out of the port, the same 2,304
  chunks either side**: 123.5 s to settle at 106.3 ms a chunk before, 377
  ms a chunk with four times the pieces and nothing else, and **69.5 s at
  30.0 ms a chunk** with the arc reject in. The reject is worth more than
  the pieces cost, which is what says the old cost was `Site::nearest`'s
  own trigonometry asked of every piece for every sample rather than the
  count of pieces.
- **How far the road is DRAWN, which is a fact about the corridor's own
  width and not about the tarmac.** A ring box at level L reaches
  `32 * 2^L` m, so the cell under an eye `h` up is about `h / 64` and a
  lattice column only surely lands on a corridor's flat while
  `CORRIDOR >= cell / 2`: **a road survives to about 128 times
  `CORRIDOR`**, 896 m at 7 m and 2,048 at 16. Measured on the frame from
  900 m, which is where that relation says it breaks: the road comes out
  DASHED in runs of 22, 28, 56 and 39 m with gaps of 4 to 26, irregular
  rather than on the 85 m piece, which is what says it is the lattice
  phase against the relief. Widening the flat to two cells of that level
  costs no re-bake (the width is runtime only), no change of grade (the
  skirt is untouched) and 0.016% of the body in graded ground.
- **A corridor's LANDINGS, which the dashes are what found.** The tarmac
  test went from 0.103 m of float to 0.796 the moment anything was drawn
  at an interior point of a piece: an arc's band is a capsule whose round
  end reaches 7 m into its neighbour's, so `Planet::levelling` returned
  whichever of two covering arcs its latitude index reached first and
  every station on every road carried a 14 m landing at its own level. On
  a one in ten grade that is a 0.66 m step at each of them. Taking the
  NEAREST covering site holds the profile within a centimetre of the ramp
  the road was routed at, and the tarmac back to 0.240 m, which is the
  mitre and the surfacing.
- **What the sun was worth from UNDER the horizon**, which is the whole
  of the owner's second correction, as arithmetic rather than a picture:
  faded over `DUSK_TO` to `DUSK_FROM` the harness's 8,000 lux sun read
  **3,009 lux at the horizon itself, 2,188 with the sun a degree under it
  and 821 at three degrees under**, all of it casting shadows through the
  planet. A day here is 240 minutes, so the 5.7 degrees the old band ran
  past the horizon is 3.8 minutes of every dusk and every dawn. It is
  0.0 at all four now. The A/B at midnight moved 0.082% of the picture
  and that is honest rather than impressive: at midnight the old band had
  already run out, and what this fixes is the hour either side of a
  sunset.
- **The scripted drive out of the port**, four measurements on one road:
  12 m in 30 s wedged against a building; 230 m with a route out of town
  that skipped a corner; 1,425 m in 420 s oscillating on the road,
  turning round at every bend; and 9,297 m in 600 s at a steady 58 km/h.
  The last is the car flat out on a country road for ten minutes, and
  its distance to the goal does not close, because it is following the
  road it reached rather than routing over the network. On the ROUTE,
  four more: wedged at 548 m reading itself 28 m off a route it was on,
  because off was measured to a point; wedged at 838 m against a 4.2 m
  step where a road's skirt met a town's ground; orbiting on the grass
  at 1,215 m every thirty seconds, because it never braked for a bend;
  and looping back into town off a line it had already joined. Then
  **9,380 m in 240 s, the last three minutes at 160 km/h, the goal from
  51.63 km to 44.97**.
- **The JUNCTION where a highway meets a town**, on the port, in six
  measurements and six pictures: the slip spliced INSIDE the loop that
  builds the sites read the bare relief and drew as its own curved
  SHADOW on an empty field; spliced after them it drew for two thirds of
  its length with the drawn ground **0.34 m** over its tarmac at the
  worst; on the highway's own `EMBANK` it was **0.39 m**, because the
  swing between the analytic surface and the one the mesher draws is
  **1.045 m over at one point and 0.11 m under three metres away** and
  no fixed embankment covers that; and reading the same ground the
  mesher draws it is **27 pieces over 80 m, from the highway's mouth
  755 m out of the town to a crossing 695 m out, ending 0.00 m from the
  paving, with the ground -0.05 m over its tarmac at the worst**, which
  is tarmac over ground by exactly the lift it was laid at, everywhere.
  What the march costs is **0.16 s of startup against 74.6**, and the
  difference is `Planet::around` filtered once for the whole curve and
  `town::surface_radius_from` started at the analytic surface plus an
  overhang rather than at the top of sixteen kilometres of relief band.
- **The CLIMB, a kilometre a step over the port, straight down.** The
  chunk count and the triangles fall monotonically with no cliff in
  them: 2,654 chunks and 578,924 triangles at 1 km, 1,051 and 414,969 at
  5, 683 and 335,544 at 10, 461 and 258,132 at 20, and the settle goes
  208 s to 38 s on lavapipe. At 5 km a pixel is 5.75 m, so the road out
  of the port is 1.2 px of tarmac and draws as one, and the port is 85 px
  across for 483 m of ground: both at their own width, with no chart mark
  anywhere in the frame to disagree. Ten levels put a sharp 32 km ISLAND
  of terrain over a blur from 33 km up; fourteen fill the frame to 49 km
  and past it. The A/B at 32 km moved 53.55% of the picture.
- **What a chart mark is worth against the ground it stands for.** A city
  is 2.2 texels and a road 0.9, which at 2,048 wide is 6.7 km and 2.8 km
  against a town 483 m across and a road 6.9 m wide: 14 and 400 times
  over, and it was 73 and 800 at 1,024. Faded on the chart's own
  resolution the marks are whole from 2.6 radii, where the road network
  reads across every continent, and nought at 356 km and below, which is
  every altitude at which the streamed ground is in the same frame.
- **The impostor poking through**: sampled at its own 13 km vertices the
  sphere stood up to 3.3 km OVER the ground between them, forty times its
  own 80 m sink, and the coarse terrain was riddled with chart at 150 km
  up. Displaced to the LOWEST ground a vertex can see it cannot, and the
  silhouette drops by at most the relief, 4 km of 1,000.
- **The MOUND under a highway, swept over 32 roads**: the drawn ground
  stands **-0.49 m over the tarmac at its worst and -3.15 m at its
  median**, against **+0.83 m at its worst** on road 0 alone before it.
  The worst is the number that decides it, and it is negative now over
  thirty two times the sweep, which says the coarse ground is under the
  road everywhere on the body rather than merely mostly. The mound's own
  5,208 vertices sit -0.150 m to -0.150 m off `Planet::surface`, which is
  `ribbon::BURIED` and nothing else. And the SLIP did not move with
  `EMBANK` going half a metre to two: 15 pieces over 41 m from a mouth
  168 m out to a crossing 129 m out, ending 0.00 m from the town's own
  paving with the ground -0.05 m over its tarmac at the worst, against 16
  pieces over 42 m and the same three numbers before, which is what
  `road::SLIP_CLEAR` is for.
- **WHAT A COARSE CELL DOES TO A ROAD, off the mesher the game draws
  with** (`examples/lod_over_road`, twelve roads of twenty stations,
  the chunk contoured at each level and the road's own columns raycast
  down it). With the corridor in the field, as it ships: nought buried
  at 2, 4 and 8 m cells, then **54.2% of the centreline buried at 16 m
  (0.82 m at the worst), 39.3% at 32 m (4.27 m), 6.1% at 64 m and 0.4%
  at 128 m**. The failure is a BAND and not a far field limit: it sits
  where the cell is the size of the feature, which is the eye's own
  1 km to 2 km, and that is where the owner's picture is dashed.
  Widening the corridor moves the band out one level per doubling and
  never removes it: at 32 m the 16 m cell clears (0.9%) and 32 and 64
  fail (53.8%, 48.7%); at 48 m, 64 and 128 fail (58.3%, 22.2%). With
  the corridor OUT of the field the same roads are **0.0% buried at
  every cell out to 128 m** on `EMBANK` alone, standing a median 3.6 m
  over the natural ground, and only 0.9% at 256 m, where 11 m of
  tarmac is already under a pixel. A chunk is 13 to 16 ms with the
  corridor against 8 without, on a body carrying NINETEEN
  corridor sites; the harness planet carries 748,015 and the owner's
  own HUD reads 50 ms a chunk.
- **A FAST EYE's own rings**, at the car's `driver::TOP` of 44.4 m/s:
  standing, walking (5 m/s) and running (8.5) all keep the **0.50 m cell
  reaching 32 m** they always had, and driving drops the finest ring to
  **level 2, a 2.00 m cell reaching 128 m**, coming back the moment the
  car stops. The boxes stand **64 m ahead** of the eye there, which is the
  cap (half the finest ACTIVE box's own 128 m) binding on a lead of 66.6.
  Measured on rings that were ADAPTED first, which is the order
  `Streamer::want` asks them in: the same test run against an unadapted
  level 0 prints a 16 m lead the game never takes.
- **The CURVE at a bend**, at the spacing the road is laid in: a 45 degree
  bend was a **111 m** radius and a right angle **60 m**, which at 160
  km/h is twenty gravities. Fitted, 10, 30, 45 and 60 degree bends all
  come out at the full `CURVE` of **1,116 m** and a right angle at
  **604 m**, where `CURVE_OFFSET` wins, which is 118 km/h.
- **The body re-baked on the aligned centrelines**: **1104 settlements and
  310 roads over 63,840 km joining 153** of them, planned in **280.8 s**
  and read back in **779 ms** into **748,015 levelled corridor pieces**,
  against 1084 settlements, 310 roads, 767 ms and 749,431 before. More
  villages off the same 310 roads is what a curve does to a route: a
  fitted arc is a little shorter than the corner it replaces, so
  `road::waysides` walks a different line and drops its 25 km settlements
  in different places.
- **The CARS on the highways**: this body turns out **6,354** of them over
  its 310 roads, and draws the nearest **12 within 450 m**, beside the
  21,491 on foot and 5,643 driving in the 1,104 towns of which the nearest
  32 and 14 are entities. A road nobody is near costs nothing, which is
  the rails rule: `commute::spot` is never asked.
- **A car that TUNNELLED, at three frame rates**: a wall 220 m off stops
  it at **217.76 m at sixty frames a second, 217.76 at twenty and 217.77
  at twelve**, which is the wall less the car's own 2.05 m of bonnet at
  every one. Before the sweep a bumper 0.03 m past a 0.35 m wall's own mid
  plane was pushed **0.15 m FORWARD**, out the far side, and the car drove
  on at 160 km/h without slowing.
- **The atlas re-baked on cities ten times bigger and a cut a town may
  not exceed**: **521 settlements (130 cities and 391 villages) and 265
  roads over 55,105 km joining 122 of them, planned in 342 s** and read
  back in 1.8 s. On the CURVE and the seven per cent together it
  is **556 settlements and the same 265 roads over the same 55,105 km
  joining the same 122, planned in 341.8 s**: a route is measured on
  the router's own ten kilometre waypoints, which a fitted arc does not
  move, so the network is to the kilometre what it was and the 35 extra
  places are `road::waysides` walking a slightly shorter centreline and
  dropping its 25 km villages in different spots. Two changes to how a
  road is BUILT that leave how it is ROUTED alone is what that says, and
  it is the split `road::pieces` and `road::centreline` are for. At 170 m towns and no cap it was 1,084 settlements and
  310 roads over 63,840 km joining 153. What the cap buys is that a town
  cuts **17 m at its own edge (9 m on the mean) at up to 101%, against
  105 m (18 m) at 641%**, which is an apron a walker climbs rather than
  a wall round every city.
- **What is left over the SEVEN PER CENT a highway is allowed**, on the
  whole body: **nought of 661,492 pieces on the highways themselves**,
  and 3,866 in SLIPS, at up to 252%. Before `road::smooth`'s order was
  fixed the steepest piece anywhere was **4966%**, which is eighty eight
  degrees, and 4.72% of 769,371 pieces were over even the tenth that
  version allowed. Every piece still over it is a slip crossing a town's
  own graded apron, which is the town's skirt and not the road's grade,
  and it is named rather than hidden.
- **A town's own OUTLINE, box counted**: the bare lobed ellipse is
  **0.987** and the grained plan **1.173**, on a ladder from one
  `PITCH` to half a town over four seeds. It climbs at every step of
  the bite (0.987, 1.105, 1.173, 1.212) and at every step of the
  ROUGHNESS with the stretch held (0.50 gives 1.007, 0.60 gives 1.056,
  0.70 gives 1.114, 0.80 gives 1.173, 0.88 gives 1.223) while the fill
  stays between 16.2 and 16.6%, which is what says the number reads the
  grain rather than how much of the town it took. An `fbm3`'s own half
  is 1.007, a picture nobody could tell from the oval, which is why the
  persistence had to become a parameter. The grain's own spread is a
  TABLE by octave count, 0.184 at one octave to 0.073 at eight, every
  row narrower than `fbm3`'s at the same count (0.080 against 0.108 at
  six). Nought of 49,152 points stand outside the outline and the least
  demand inside 0.645 of the edge is 0.0010, so the levelling, the
  skirt, the slope bound and the baked atlas are all untouched.
- **And as a PICTURE**, the port from 1,200 m up looking straight down
  on the same 2,748 chunks and the same 383,516 triangles either side:
  **4.650% of the frame moved by more than 8 of 255**, all of it the
  town. Before it is a smooth ellipse of blocks with a uniform lace
  right out to its rim; after it has a notch bitten out of one flank,
  holes in the fringe and arms running out along its streets. Nothing
  in the terrain moved at all, which is what the identical chunk and
  triangle counts say: the grain is in the PLAN and never in the
  ground.
- **A town BUILT as the eye comes near it**, on the harness planet: town
  0 is 224 buildings, 1,821 pieces of street, 51,539 collision boxes and
  448 lamps, and the eight within the 200 km reach are built one a
  frame. Town 160, the 9 km village, is 31 buildings and 5,739 boxes.
- **RAMMING, on the core's own ball**: a car rammed square from behind
  at 16 m/s takes 10.40 m/s and the rammer keeps 5.60, momentum to a
  part in a hundred thousand (the two stand 4 m apart on a 2 km ball,
  so the normal is two milliradians off the struck car's tangent
  plane); a rear quarter hit across at 10 m/s shoves the struck car at
  6.5 m/s and turns it at 1.69 rad/s; a car shoved sideways at 10 m/s
  slides 7.87 m and stops, and a spin of a radian a second turns it ten
  degrees before the tyres take it.
- **Two highways on ONE TRUNK**, on the harness body: 727 of 730 road
  ends stand on a trunk shared with another road, 1,573 pairs share
  waypoints and the deepest share is 23 waypoints (230 km). Merged at
  load, **350 of 365 roads stand on another's trunk for 78,182 points,
  the longest trunk is 226.7 km, the profiles settle in 8 passes and
  the steepest shared piece climbs at 7.0%**, with nought of 718,864
  highway pieces over the grade: the first cut pinned a sharer down and
  put its whole climb back on one piece at 103%, the second raised
  owners in road order and chased itself to the cap at 64 passes with
  four pieces at 47.5%, and the third pinned a point twice and rose a
  lip a pass for ever. The body cuts 633,198 corridor pieces against
  708,185 and lays 10,377 stretches against 11,664; the census reads
  581 joins and 1,446 bare mouths at worst 124 m from paving, against
  2,027 mouths at 97,890 m with the joins counted as dead ends.
- **The port as BLOCKS, on the re-baked body**: town 0 is **1,676
  buildings, 7,621 pieces of street, 168,209 collision boxes and 3,705
  lamps**, against 224, 1,821, 51,539 and 448 as one building a block,
  and the picture from 1,200 m up is 2,541 chunks and 293,135 triangles
  settled in 467 s at 50.7 ms a chunk on two workers, with the render's
  other two cores on the suites. The body is **832 settlements read in
  2,967 ms, 365 roads cut into 708,185 corridor pieces carrying 1,757
  gas stations on 11,664 stretches**, turning out 73,702 on foot, 18,650
  driving and 6,022 cars on the roads. Laid in the core, a 537 m city is
  1,935 lots of which 72 are two by two, 24.9% over one storey and 3.5%
  over three; a 250 m town is 478 lots (16 big), 19.2% over one storey
  and none over two; a 150 m village is 142 lots, all one storey.
- **A car steering toward a place**, on the test ball: 199.3 m closed to
  10.2 m in thirty seconds, with the wheel full over past a quarter turn
  off the nose and nought dead ahead.
- **The car itself, at every scale and against anything a test can
  build**: 136.9 m in ten seconds on a 2 km, a 100 km and a 1,000 km
  ball, identically; 5.53 m driven into a CORNER of two walls and 12.60
  m reversed off it in three seconds; and 1 cm a second to 16 m/s in
  five seconds from a crawl. And the scripted drive out of the port:
  12 m, then 12 at 60 s, 90 s and 120 s, oscillating against a building
  with nothing to follow round it.
- The traffic on the harness planet: the 8 BUILT towns turn out 300
  people and 76 cars, of which the nearest 32 and 14 are ever entities.
  The other 765 settlements on the body turn out nobody at all, because
  nothing asks: an agent is a function and a function nobody calls costs
  nothing, which is the whole argument for rails.
- A car's own lamps, measured at the chase camera's own range on three
  renders of one frame: 252, 252, 252 with both lamps in one white
  material; 252, 251, 251 with a material a KIND but the emissive still
  written as 900 of something the shader adds AFTER the exposure; and
  226, 110, 107 once the glow is in `terrain.wgsl`'s own units, 8.0 at
  the front and 2.2 at the back. 0.188% of the picture moved. Which END
  of a car a camera is looking at is told by the GAP between its lamps
  over their width, 2.06 at the front and 2.73 at the back: the render
  measures 104 px between two 38 px blobs, which is 2.74, so the chase
  camera was behind the car the whole time and it was the COLOUR that
  was lying.
- What the cross section cost, measured on one binary either side of the
  one constant `PITCH`, on the same 773 town atlas: at 14 m the 8 built
  towns are 695 buildings, 4,250 pieces of street, 2,791,902 triangles,
  137,221 boxes and 1,390 lamps, turning out 522 people and 132 cars; at
  18.5 they are 400, 2,467, 1,644,942, 81,115 and 800, turning out 300
  and 76. 400/695 is 0.575 against the (14/18.5)^2 of 0.573 the area
  predicts, which is what says nothing else moved. Neither of those box
  counts carries a kerb collider, because the pair was measured before
  one existed; with them the 18.5 world is 87,691, so a pavement a body
  can stand on is 6,576 boxes and not one triangle.
- The traffic, on the core's own test town: 115 pieces of street are 93
  of run and 22 crossings over 31 edges, 11 circuits off them, and 17
  lots turn out 13 people and 3 cars at 0.98 to 9.15 m/s. Nobody strays
  further than 3.673 m from a street's middle against a half street of
  4.25, and the furthest anybody steps off the PAVING is 0.000 m, against
  1.62 before the crossing square was a piece of its own. Over 2 cm of
  travel the place is out by 6.8e-7 m and the heading turns by at most
  1.6 degrees, which is what the corner fillets are for.
- What a street's cross section costs and buys: a run piece was one quad
  of 2 triangles and is 2 of carriageway, 24 of the two pavement slabs
  and 6 of markings, and a crossing is 9 cells of the same. A car had
  1.90 m of street to itself and has 2.75; a pedestrian had a painted
  stripe and has a kerb 12 cm up.
- The CAR, driven: it pulls away at 5.5 m/s^2 and tops out at 16 m/s
  (58 km/h), the brake takes all of that off in 10.5 m, and reverse
  holds 5. At a crawl it holds a 5.64 m circle against the 5.53 m its
  own wheelbase and lock say, and three seconds of full lock standing
  still turns it 0.0000 degrees. Driven at a wall 8 m off it stops at
  5.53 m, which is eight less its own bonnet, and over a 17 cm kerb it
  goes 72.9 m without dropping under 16 m/s.
- The asphalt set, against the concrete it replaces on a road: 0.106
  linear albedo against 0.449, a quarter of the brightness, with the
  chips spanning 0.055 to 0.243 and no panel joint anywhere on it. Six
  seconds to bake on one core, byte identical on a re-run, and 728 KB,
  2.78 MB, 1.02 MB and 1.33 MB for its four maps, which sits between
  concrete's and grass's. Material Maker, for the comparison: five
  minutes on the seven committed graphs with none of its thirty two
  maps written, in this container, under all three renderers.
- What a crowd costs: a person is 3 parts and 144 triangles and a car is
  1 and 132, so the 32 people and 14 cars the eye ever holds at once are
  6,456 triangles and 124 entities, against the 400,000 triangles of the
  town they are walking in. Thirty two meshes are built at startup, eight
  tints by four parts, and not one is made or dropped afterwards.
- The walker, headless in the core, on a 2 km ball: 8 to 10 m in two
  seconds walking and over 14 running, a jump to between 1.0 and 1.6 m
  landing inside 1.3 s, a 0.4 m kerb climbed, a 1.2 m wall stopping the
  body 35 cm short, a diagonal walk sliding over 4 m along it, a jump
  under a lintel held to under 0.6 m, and the feet held 1.2 m under the sea.
- The mockup's join, measured at every fine crossing on it: 622 of 6,542
  with the fine surface over a centimetre above the coarse, 139 with the
  coarse above the fine, by up to 7.5 cm; a pad on a slope makes those 850,
  236 and 16 cm. A skirt hides the first and cannot close the second, which
  is why the seam is a polygon rule in the core.
- The cube sphere's corner to centre cell area ratio: 1.42 warped, 5.2
  plain, on a 16 by 16 grid.
- The marching cubes mockup: a 64 m planet with a sea and three towns on a
  112^3 lattice of 1.33 m cells, 95,200 triangles of ground marched in 0.9 s
  in Chromium on swiftshader, the towns planned in 52 ms and their twenty
  buildings built from the kit in 76 ms. The hex mockup on the same seed:
  163,842 tiles at 0.53 m, 153,009 walls, 1,363,296 triangles, in about
  6.6 s. Fourteen times the triangles for the same ground, which is the
  cost of a terrace.
- The towns in the mockup's field, which is what models replaced: 284
  structures in 33,466 coarse cells replaced by 0.22 m ones over 929
  chunks, dual contoured from 17.4 million samples into 885,862 triangles
  in 8.2 to 9.9 s, against 84,174 triangles and 0.9 s for the rest of the
  planet. A sculpted slab remarched its eight chunks in 335 ms.
- The hex world, before it was set aside: one metre tiles
  (12,257,789,082,012 round the planet), a disc of 48 tiles, 9,409 prisms
  of 150,544 triangles, and Planet-LOD at ratio 6 cut 4 ways picking 8,120
  leaves in 1.2 to 1.7 ms from the ground, 2,100 from 30 km up and 44 for
  the whole planet from four radii. Its steepest step between two
  neighbouring tiles was 0.24 m against the walker's 0.6 m stride, so it
  had no walls in it at all on this relief.
- Material Maker under lavapipe: seven graphs, twenty eight maps at 2048,
  exported in about six minutes on four cores, and byte identical on a
  re-export of the same graphs on the same machine, so `--check`'s half a
  percent is slack for another driver and not for this one. Handed a single
  graph the command line stalls, and two of the new graphs' first settings
  (a voronoi with a stretch, a bricks with no rounding) crashed it at load,
  as `noise_anisotropic` does at any; every graph is on settings the working
  ones already use, and the bake is always the whole set.
- Tenebris, for scale: 300 m planets, 163,842 hex tiles a body, 21 MB of
  voxels and a 2.7 s full remesh, one flat 5 km detail cutoff and an impostor
  past it. Every number in that project's LOD is a number this one replaces,
  and every RULE in it is one this one keeps.
