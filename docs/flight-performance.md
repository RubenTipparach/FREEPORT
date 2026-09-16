# Flight performance

Freeport's home planet has a 1,000 km radius and a 2,000 km diameter. Terrain
coordinates and collision remain f64. The default finest cell is now 0.5 m,
with ten levels covering the same 32.8 km streaming box as eleven 0.25 m levels.
Cell size describes the sampling lattice: contour vertices move to the surface,
and diagonals and slopes give different edge lengths. L shows the actual LOD
wireframe and its cell-size legend; K freezes the layout for inspection.

## What was making motion expensive

A synchronization issue was a GPU wait inside the sampling worker.
In wgpu 27, blocking device polling holds shared fence and resource locks while
waiting. Queue submission and surface presentation need their write locks.
Consequently, a separate sampling thread could still stall rendering. Sampling
now polls only completed work and waits between polls without holding device
locks. On Windows, a short thread sleep uses Rust's high-resolution timer;
channel timeouts measured roughly 16 ms here even when requesting 250 us.
Readback has a 15-second deadline and switches all remaining jobs to CPU on
failure, including batches already grouped for another planet.

The rest of the changes reduce work and keep it away from the frame thread:

| Area | Change | Why it helps |
| --- | --- | --- |
| Planning | Dedicated worker computes culling, seam signatures and ordering | Moving a ring no longer scans and sorts its entire layout during the frame |
| Streaming | Ordered work queue, incremental counters and budgeted destruction | Completion checks and old-mesh release avoid repeated scans and large deletion bursts |
| Sampling | Conservative local density bounds reject air/rock boxes | Entire sampling and meshing jobs disappear, including empty space inside the global relief band |
| SIMD | Runtime-detected AVX2 hashes eight noise corners together | Integer hashes and scalar f64 interpolation preserve every reference noise bit; other CPUs use scalar code |
| Meshing | Fixed-size cell and polygon scratch arrays; skip bare-field material sampling | Removes repeated small allocations and redundant noise evaluations |
| Compute | Stop octaves when the remaining amplitude proves the sign | Avoids evaluating fine detail that cannot contribute a crossing |
| Transfers | Immutable terrain and baked building meshes use render-only asset storage | Bevy transfers their buffers instead of cloning and retaining CPU vertex/index arrays during extraction |
| Culling bounds | Cache static mesh bounds, with one conservative building bound covering all LODs | Fog/material updates cannot force repeated scans of the baked vertex arrays; frustum culling stays enabled |
| Shading | Explicit texture gradients permit skipping zero-weight materials and fully faded normal maps | A distant single-material pixel needs at most six texture reads instead of 45 |
| Sky lighting | Cache completed filtering of the static sky cubemap | The same radiance and irradiance maps no longer require convolution every frame |
| Collision | Conservative bounds for the swept region and direction; nearby recovery bracket | Remote town skirts no longer force thousands of collision samples or large recovery jumps |

GPU sampling still supplies signs to CPU contouring. f64 root finding, QEF
vertices, normals and LOD seam topology stay on the CPU. This preserves the
reference geometry and planet-scale precision. A fully GPU-resident mesher would
also need GPU seam ownership, allocation and indirect drawing; merely moving
the current mesh output through another readback would retain transfer costs.

Meshes for a new layout remain hidden until all neighboring seams are ready.
The old layout switches visibility atomically, then its hidden entities are
retired over later frames. CPU collision remains independent of visible meshes.

Sky lighting reuses Bevy's own filtering output after its GPU submission finishes.
Replacing the source or output texture triggers regeneration. The static-camera
marker is deliberately unsuitable for cubemaps animated in place on the GPU.

## Reproduce a moving benchmark

Run a release build with no concurrent compilation or other game instances:

```sh
cargo build --release -p freeport_app
target/release/freeport_app --benchmark-flight target/flight-ground.json --bench-height 0 --bench-frames 1200
target/release/freeport_app --benchmark-flight target/flight-cruise.json --bench-height 500 --bench-frames 1200
```

The benchmark waits for initial terrain and at least 120 warmup frames, then
follows a tangent flight path using the real atmospheric speed and collision
rules. Input cannot alter the route. Simulation steps are fixed at 60 Hz; frame
timings use wall-clock intervals and include rendering backpressure. Benchmark
windows update continuously even without focus and use AutoNoVsync; normal
game presentation is unchanged. The default requested speed is 2,000,000 m/s,
subject to the atmosphere. The two routes cover about 120 m and 3.92 km.

JSON includes frame percentiles, budget overruns, flight CPU time, main-update
CPU time, terrain planning/upload statistics, pending work, GPU, resolution,
grid and batch settings. Moving runs do not wait for all terrain to catch up at
the end; their reported backlog must be considered alongside frame time.

Use `--cell-size 0.25 --levels 11` for the previous grid or `--cpu-terrain` to
compare CPU sampling. Add `--profile-render` for per-pass GPU/CPU diagnostics
and mesh allocator statistics. Profiling is opt-in and has overhead; compare
ordinary benchmark runs with other ordinary runs. GPU diagnostics arrive
asynchronously, may omit frames, and overlapping spans must not be summed as
frame time.

`assets/config/render.json` controls worker count, compute batch size, queue
lookahead, mesh installation budgets and cell size. Two chunks per compute
dispatch is the default, with capacity for eight. More workers or a larger batch
can increase throughput while competing with rendering; neither is automatically
an improvement in frame latency.
Automatic worker count now uses half the available hardware threads, capped at
eight. On the test machine, four workers reduced long-route p99 frame time from
12.13 ms to 6.16 ms compared with six; this still left isolated outliers.

The regression suite checks AVX2/scalar bit equality, conservative culling at
mixed-LOD aprons, collision across town skirts, coherent asynchronous layout
publication, failed-GPU fallback and CPU/GPU mesh equality. GPU cases include all
configured planets and a 3,000 km radius / 290 km relief / 24-octave stress world.
Existing coarse-edge non-manifold pinches remain documented in the streaming
guide; these optimizations do not claim to resolve them.

## Kernel checks

On the i7-9700F / RTX 3070 test machine, the release noise benchmark evaluated
1,048,576 points in 26.02 ms with scalar hashing and 17.37 ms with runtime AVX2
dispatch, with identical checksums. The actual GPU parity test sampled 74,088
points in 8.90 ms including input preparation and readback, compared with
21.94 ms on the CPU. Two-chunk GPU batches had a median call latency of 2.06 ms;
eight-chunk batches took 7.34 ms. These are call timings, not GPU timestamp
measurements or whole-frame speedups.

The shader returns conservative signs and may stop before computing a full
density magnitude. CPU f64 crossings, normals and collision still use the full
field. The GPU regression requires exact mesh positions, indices and normals,
not just similar screenshots.

## Moving-flight results

Measured September 16, 2026, on Windows with an i7-9700F (eight hardware
threads), RTX 3070, Vulkan, 1280 x 720, release builds. Runs were sequential,
with no concurrent compilation; normal desktop applications remained open.
The baseline is `f27ef45` plus the same benchmark route and uncapped presentation.
It uses the previous 0.25 m / eleven-level grid and six workers. The final build
uses 0.5 m / ten levels, four workers and two-chunk GPU sampling batches.

| Route / build | Frames | Median ms | p99 ms | Worst ms | Frames over 16.67 ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| Ground, before | 1,200 | 10.10 | 21.35 | 63.48 | 67 |
| Ground, after | 1,200 | 5.27 | 7.30 | 72.91 | 5 |
| 500 m cruise, before | 1,200 | 4.96 | 12.84 | 81.46 | 9 |
| 500 m cruise, after | 1,200 | 3.20 | 5.19 | 52.09 | 1 |
| Longer cruise, after | 6,000 | 3.52 | 8.28 | 59.12 | 13 |

The short routes end at the same positions before and after. The longer route
covers 6.77 km; atmospheric slowing and terrain contact reduce its later speed.
Streaming plus rebasing p99 fell from 7.94 to 0.65 ms on the ground and from
1.32 to 0.21 ms during the short cruise. This is improved frame consistency,
not a zero-drop guarantee: the final ground run still has four frames above
33.33 ms, and its worst frame is slower than the baseline's worst frame.

The uncapped simulation advances one fixed 60 Hz step per rendered frame, so a
faster run gives streaming less wall time per metre. Ground, short cruise and
long cruise finish with 643, 1,325 and 911 replacement chunks still required,
respectively (26, 36 and 33 jobs pending). Coherent old layouts remain visible
while replacements finish. These timings do not establish zero streaming lag.
Initial settling was 6.93 s on the ground and 4.79 s for the short cruise,
compared with 6.81 s and 4.42 s before. The tuning prioritizes frame latency.

Before the final sky-cache change, the optimized build retained the 0.25 m grid
and measured 5.39 ms cruise p99, with no frames over 16.67 ms in 1,200 frames.
The gains therefore are not solely a consequence of increasing cell size.
Earlier runs overlapping an unrelated release build were discarded; their much
higher frame times are not a valid baseline.

An opt-in diagnostic run confirmed that sky filtering dispatches disappear after
warmup. The fixed street render matches the baseline within 4/255 per channel
outside the HUD, with zero scene pixels over the 8/255 comparison threshold.
Whole-image changes are confined to debug counters. Both renders contain
810,615 terrain triangles at the retained 0.25 m grid.

Validation: 84 core tests, 40 app tests and the explicit GPU mesh-parity test
pass. Workspace clippy with warnings denied, rustfmt, code-shape and dash checks
pass. The material graphs were unchanged, so no material rebake was required.

Raw local results are `target/flight-clean-before-{ground,cruise}.json`,
`target/flight-cache-{ground,cruise,long-cruise}.json`, and the separate diagnostic
run `target/flight-cache-profile.json`.
