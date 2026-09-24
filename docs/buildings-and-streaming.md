# Building authoring and terrain streaming

Building dimensions and variants live in `assets/config/buildings.json`.
Regenerate the library from the repository root with Blender 4.4:

```sh
blender --background --factory-startup --python tools/bake_buildings.py
```

Use `-- --only house_1` to rebuild one existing variant. A complete bake also
writes the manifest. `--factory-startup` avoids loading personal Blender add-ons.
The scripts do not need any third-party Python packages.

Each variant produces three types of asset under `assets/models/buildings`:

* `.blend`: editable wall solids, separate door/window cutters, Exact Boolean
  modifiers and bevels. The JSON recipe is the parametric source for regeneration;
  cutters and modifier stacks can also be edited directly in Blender. Regenerating
  overwrites the generated source files, so save hand edits as a separate source.
* `_lod0.glb`, `_lod1.glb`, `_lod2.glb`: static evaluated geometry for interchange.
  glTF uses its standard Y-up convention.
* `.json`: the same evaluated meshes, material IDs, collision boxes and lamps in
  metres, X east, Y north, Z up. This bundle feeds the existing town batching and
  floating-origin renderer without requiring Blender or per-object scene entities.

Windows are holes through the wall thickness. Frames and thin transparent glass
occupy the openings; there is no opaque wall behind them. Doors are cut through to
the floor. Collision partitions each wall using the same opening rectangles and
adds the physical glass panes. The full collision model remains active at every
visual LOD. Roof details are visual shells above a collidable ceiling slab; the
recipes currently have a single tall interior, without intermediate floors or stairs.

The bake checks wall manifoldness and positive signed volume, ray-tests every
window opening and door, and verifies that wall remains below each window.
LOD0 includes bevels and trim; LOD1 keeps the openings, frames and glass but drops
bevels and small details; LOD2 keeps the building silhouette and doorway and fills
subpixel windows. Baked meshes are reused and batched by city block (a tile of
`PITCH`, 48.5 m). Past the LOD2 distance a block is drawn as SOLID BLOCKS, one box
per building in its own skin (`model::massing`) and one quad per piece of street,
which is about 1% of LOD0's triangles. Every tile has this massing from the moment
its town is raised. Nearer tiles are rebuilt on a worker at their grade and replace
the massing when ready. Collision boxes and lamps exist only for tiles within
180 m (dropped past 260 m), again built on a worker. Glass has its own
transparent material and draw, rather than the opaque terrain shader's window color.
Missing or invalid libraries produce a warning and use the procedural reference
models, whose box and pane winding is now corrected.

## Terrain

The surface remains dual contouring on the existing shared lattice. The GPU
compute shader evaluates bounded batches of density grids (two by default, at
most eight), including the
coarse-neighbour apron. Input preparation subtracts planetary radii and blends town
sites in f64. High/low noise coordinates retain detail at large planetary radii.
Samples close enough to zero to risk a float sign difference are checked against
the CPU field. Surface root finding, QEF vertices, collision, and LOD seam topology
still use that reference field. Octave evaluation can stop when the remaining
amplitude cannot change a sample's sign. This is compute-assisted meshing, not a fully
GPU-resident mesh pipeline.

A dedicated worker owns the reusable compute buffers and receives readback.
It uses nonblocking device polling: a blocking device wait also holds wgpu locks
needed by rendering on the shared device. Meshing workers also convert to
Bevy meshes, so the frame's upload budget covers installing completed assets and
spawning their entities. Obsolete queued work is cancelled, and the bounded number
of pending jobs limits both in-flight memory and wasted work. A readback failure
switches subsequent jobs to CPU sampling. Software adapters and unsupported
compute limits select CPU automatically; `--cpu-terrain` explicitly selects it.
Conservative local field bounds skip whole air/rock chunks before dispatch.
Fully levelled town chunks (a town's ground is graded to the country,
`town::Grade`, and inside the town it is that grade outright) have no noise
to evaluate and bypass the compute dispatch, so their CPU work can overlap GPU batches for the surrounding terrain.

Layout culling, seam signatures and distance ordering run on a separate planner
thread. The frame thread consumes an already ordered queue. Terrain rings drop
unnecessary fine levels as height increases, with hysteresis,
while retaining the adjacent-level seam invariant. Replacement meshes are uploaded
hidden within the frame budget. The old layout remains visible until every new
chunk and changed neighbor seam is ready, then terrain and water swap together.
Hidden old entities are destroyed over subsequent frames within a count budget.
The target layout stays fixed while building, and catches up with the current eye
after publishing, so motion cannot continually cancel the transition. Initial
loading still appears progressively. Staging temporarily retains both layouts in
memory; no timer removes needed coverage.

Press **L** for wireframe colored by terrain LOD, or start with `--lod-wire`.
The view isolates terrain, hiding water and buildings for topology inspection.
The legend gives each level's cell size. **K** freezes the ring positions and
altitude adaptation while allowing the camera to move; pending work finishes for
that layout. Colors identify the owning chunk, including its seam triangles.
Look for edges meeting at the color boundaries, not just triangles overlapping.
Tab returns to the ordinary global wireframe toggle. Terrain keeps its shared
field normals across transitions; architectural materials retain hard creases.

The finest grid uses 0.5 m cells, configurable as `terrain_cell_size` or
`--cell-size`. Ten levels retain the former 32.8 km outer streaming box. Vertices
follow the contoured surface; cell size is not an exact triangle-edge length.

`assets/config/render.json` controls the mesh installation time/count budgets,
queue lookahead, worker count, compute batch size, building LOD distances/hysteresis
and the number of building jobs in flight. Distances are metres from each block's own
bounds, in the town's frame. Zero selects the default.
The default worker count uses half the available hardware threads and caps terrain workers
at eight. Building LOD thresholds are 80, 250 and 1,200 metres (LOD0 to LOD1, LOD1 to
LOD2, LOD2 to solid blocks), with 15% hysteresis, and three building jobs run at once.
`occlusion_culling` (on by default) lets the GPU cull, for the camera only, what the
previous frame's depth proves hidden. `shadow_proxies` (on by default) makes a tile drawn
past its nearest bake cast its shadow from its solid block, which only the sun sees.
Both are booleans rather than zero-sentinel numbers, so an A/B is one line of this file.

## Validation and measurements

```sh
cargo test -p freeport_core
cargo test --release -p freeport_app
cargo test --release -p freeport_app gpu_preserves_cpu_signs_and_lod_seams -- --ignored --nocapture
cargo clippy -p freeport_core -- -D warnings
cargo clippy -p freeport_app -- -D warnings
python tools/shape.py --check
```

The explicit GPU test compiles and dispatches the actual WGSL shader, compares
sample signs against the CPU field at multiple LODs and a town apron, and checks
that the generated vertex positions, normals and indices agree exactly. It reports
sampling times including input preparation and readback. Coverage includes every
configured planet and a 3,000 km radius, 290 km relief, 24-octave stress world.
The default app tests
also load and validate every committed building variant.

The streaming tests deliver replacement meshes out of order and check that
old terrain remains visible until adjacent seams are ready, including empty
chunks and changes spanning several levels. The rough-planet LOD audit checks
for open edges and missing seam corners. It still reports some non-manifold
edges where multiple fine crossings attach to the same coarse edge (11 and 1
in its two stress views). Closure alone is not a manifoldness guarantee; this
coarse component grouping remains a mesher limitation, separate from the fixed
streaming gaps and rendering-normal discontinuities.

`--shot target/gpu.png --frames 120` writes both a screenshot and
`target/gpu.metrics.json`, including terrain settle time, pending work, triangle
count and frame-time percentiles measured with `Instant`. Repeat with
`--cpu-terrain` for a comparable CPU run. A screenshot that reaches its frame
timeout before the terrain settles has a null settle time and pending work; it
must not be reported as a completed loading measurement. Sampling speed, total
loading time and steady frame rate are separate measurements.

Before the flight optimization pass, measured on the local RTX 3070, release
build, 11 terrain levels, six meshing workers, stationary default port view,
sequential isolated runs:

| Measurement | CPU sampling | Compute sampling |
| --- | ---: | ---: |
| Initial terrain settles | 11.45 s | 7.65 s |
| Mean work per chunk | 12.63 ms | 6.44 ms |
| Frame p95 during loading | 14.32 ms | 14.06 ms |
| Loaded chunks | 4,590 | 4,590 |
| Terrain triangles | 810,615 | 810,615 |

This is about 33% less loading time in that scene; it is not a claim of a 33%
increase in steady-state frame rate. A 600-frame automated walk also settled
with no pending jobs and 1,856 loaded chunks in the four-level test area.
See `flight-performance.md` for the current moving-flight measurements.
