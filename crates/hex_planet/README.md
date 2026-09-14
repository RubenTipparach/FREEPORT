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

The first milestone is engine-free on purpose. Texture work stays in Material
Maker rather than being generated in code. The geometry and lighting rules
can be tested quickly before a Bevy render plugin transcribes them to WGSL. The
next milestone is a GPU-instanced hex patch, a shared depth surface beneath it,
and Goldberg pentagon handling at the global topology seams.
