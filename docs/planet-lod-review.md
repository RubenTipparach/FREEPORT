# Adaptive planetary surface

The recommended direction is an adaptive spherical surface for the planet,
with volumetric meshing limited to local terrain that needs caves, overhangs or
excavation. This is an architectural recommendation; the live renderer still
uses the current dual-contoured rings.

## What the reference actually does

[Planet-LOD](https://github.com/sp4cerat/Planet-LOD) is an MIT-licensed teaching
implementation by Sven Forstmann. Its
[simple implementation](https://github.com/sp4cerat/Planet-LOD/blob/master/src.simple/Main.cpp)
tests each edge for refinement and collapses unsplit edge midpoints when choosing
children. The complex version draws reusable triangular patches. It is useful
as an algorithm reference, rather than a ready-made Bevy renderer or compute
mesher. Radial displacement represents a surface, not volumetric topology.

FREEPORT already implemented a port in `freeport_core::lod`, removed in commit
`5701ac1` with the hex renderer. The preceding revision contains the selector,
GPU displacement, and precision work. Recovering those pieces does not require
restoring the old hex world. The old renderer disabled shadows and the depth
prepass for displaced terrain; those omissions must not be copied into the
current renderer, where water reads terrain depth.

## Measurements from the archived selector

The old `lod.rs` was extracted into `target/planet_lod_reference.rs`, compiled
independently with optimizations, and tested on the development machine. All
eight regular tests passed. Its closed-surface test counts undirected edges by
the exact bits of their endpoints, requiring exactly two incident triangles
with horizon culling disabled at four different eye directions.

One selector benchmark at radius 1,000,000 m, eye height 12 m and ratio 6 produced
8,396 leaves in 1.44 ms, with the finest edge approximately 2.14 m. These are
selector leaves, before patch subdivision and terrain displacement. This is not
a comparison of final triangle counts, loading time, or frame rate against the
current renderer. No speedup for the complete game is established by this test.

## Integration requirements

1. Use canonical shared edges for subdivision and patch tessellation, with
   bounded refinement, altitude awareness and a measured screen-space error.
2. Cache displaced patch geometry. Compute should evaluate each shared vertex
   and normal once when its patch changes, with buffers reused by rendering,
   shadows and the depth prepass. Re-evaluating the entire noise field per drawn
   corner every frame would repeat the old renderer's expensive work.
3. Keep planet positions and boundary construction in f64. Pass small local
   offsets to the GPU, preserving shared vertex identity after rebasing.
4. Use the same planet field, seed and town flattening for rendering and ground
   queries. A spherical mesh must not quietly replace the walker's terrain with
   a different surface.
5. Treat the connection to each volumetric region as part of the topology:
   define one shared boundary contour and test the joined mesh. Overlapping a
   spherical surface with a voxel mesh does not establish a watertight join.
6. Retain the colored LOD view and frozen refinement camera. Audit open edges,
   non-manifold edges, winding, and shared positions before and after camera
   movement. Publish replacement geometry and changed neighbors together.

The existing rough-terrain stress test's pinched dual-contouring joins remain
relevant for local volumes. Switching the distant surface representation does
not repair those local component assignments.
