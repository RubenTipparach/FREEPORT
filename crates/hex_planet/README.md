# Ten kilometre hex planet experiment

This is the second terrain project in the workspace. It does not replace
`freeport_core`. It explores a five kilometre radius planet with:

- camera-local, sphere-projected hex cells close to the player;
- a smooth cross-fade into a conventional displaced height-map sphere;
- deterministic settlement anchors that can own streets, lots, and landmarks;
- one single-scattering atmosphere reference for the sky and hemispherical
  ambient sampling;
- authored Material Maker terrain sets baked to albedo, normal, ORM, and height
  maps through the repository texture pipeline.

The core remains engine-free. `freeport_app --planet hex` now draws its anchored
hex grids with flat terrace caps, walls and the authored terrain sets. A moving
window preserves cell addresses within a chart, and the walker uses the same cap
planes as the mesh. The distant sphere and near patch share the depth buffer and
use complementary dithered coverage through the `LodBands` transition.

The app's normal startup picker offers both planet types. The app settings live
in `assets/config/hex_planet.yaml`; zero means the documented default. Patches
are built on a worker as the observer moves. This remains a local-chart
experiment: long journeys re-anchor the grid. GPU instancing, global Goldberg
pentagons, settlement buildings and a WGSL atmosphere are the next milestones.
