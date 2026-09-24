# freeport

An open world space game about working in a space economy: thousands of
stars, tens of thousands of planets hundreds to thousands of kilometres
across, drawn from orbit to the ground with no seam, and a ship you climb out
of. Bevy 0.18, Rust, `f64` world frame with a floating origin, and a two
thousand kilometre planet drawn from one density field: dual contoured into
triangles a chunk at a time, on one lattice at every level, in rings of
detail round the eye. There is a sea, cities up to six and a half
kilometres across whose buildings and streets are models standing on ground
the field grades for them to the country they are built in,
and a walker on a street of the port who is stopped by the same boxes the
walls were drawn from. The sky is tenebris's scattering march, run in the
core so the dome and the fog cannot disagree.

Terrain density sampling now uses batched compute shaders, with CPU meshing for
precise surface crossings and LOD seams. The rings adapt to altitude, and a city is
drawn a block at a time: three baked detail levels near the eye and solid
blocks past a kilometre, so a whole city of thousands of buildings is in one
picture. Buildings are baked with headless Blender:
editable boolean-cut windows and doors, transparent glazing, static runtime meshes,
and collision from the same dimensions. See [the authoring and streaming guide](docs/buildings-and-streaming.md).

The home planet is 1,000 km in radius, or 2,000 km across. The finest terrain
grid defaults to 0.5 m cells; dual-contour vertices follow the surface, so edge
lengths vary rather than forming an exact 50 cm mesh. Set `terrain_cell_size` in
`assets/config/render.json`, or use `--cell-size 0.5`, to adjust it.
See [flight performance](docs/flight-performance.md) for the optimizations,
repeatable moving benchmarks and optional render profiling.

`CLAUDE.md` is the rules and the reasons. `docs/freeport.html` is the design
page ([published](https://claude.ai/code/artifact/7822c376-33d9-4391-9907-a958426efc29)): what is being built, the stack, the tooling and the
two terrain mockups.

```sh
cargo test -p freeport_core                 # the engine free core: positions, the lattice and its rings, the field, the mesher, the sea, the towns and their models, the walker
./run.sh                                    # build the Bevy harness and open a window (run.bat on Windows); --test runs the suites first, --shot out.png takes a picture with no display, -- hands the rest to the app
./target/release/freeport_app               # the window by hand: on foot on a street of the port; F flies, Tab wires, Esc frees the mouse
./target/release/freeport_app --fly         # in the air over the 2,000 km planet instead
python3 tools/shape.py --check              # no file over 900 lines, no function over 100
tools/get_material_maker.sh                 # fetch Material Maker into tools/ (gitignored)
tools/bake_materials.sh                     # bake materials/*.ptex to assets/textures/terrain
blender --background --factory-startup --python tools/bake_buildings.py # regenerate editable sources and static building LODs
cargo test --release -p freeport_app gpu_preserves_cpu_signs_and_lod_seams -- --ignored --nocapture # validate the actual compute shader on a GPU
```

Left click captures the mouse; Escape releases it. On foot, use WASD, Shift to
run, and Space to jump. **F** switches between walking and flying. In flight,
WASD moves along the camera axes, **Space/Ctrl** rises or sinks in that same
frame, **Q/E** rolls, and the mouse turns freely through a full loop. Either
Shift boosts speed; the mouse wheel adjusts the base speed shown in the HUD.
**R** levels the view against the planet's local horizon. Flight speed, boost,
roll rate and wheel sensitivity are adjustable in `assets/config/flight.json`
(restart to reload; zero selects the default). Tab toggles wireframe.

The wheel now spans 0.25 m/s to 2,000 km/s for travel between Freeport, Ember,
Pelagos and Rime. **N** selects a destination; **G** faces it, then hold W to fly.
Atmosphere entry progressively reduces speed to 6 m/s near terrain (or your
selected speed if lower), including while boosting. The HUD shows actual and
selected cruise speed separately. Collision sweeps the whole movement path
against each planet's terrain, independently of whether its chunks have loaded.
Planet positions, sizes and seeds are in `assets/config/planets.json`.
See [flight and planet exploration](docs/flight-and-planets.md) for details.

**L** toggles terrain wireframe colored by LOD, with a cell-size legend.
This view isolates terrain so water and buildings cannot obscure its topology.
**K** freezes/unfreezes the rings in that mode, so you can fly around a fixed
transition and inspect its triangles. `--lod-wire` enables it at startup.
Colors follow the chunk that owns each triangle, including transition faces.
LOD changes keep the old layout visible until the complete replacement is ready,
then switch the terrain and water together.

The two mockups are the record of the decision rather than a picture of the
game: they compare the meshers on the same seed, field, sea, towns and
textures, with a first person walker on each (Walk the port, then WASD,
Shift, Space and the mouse), and the marched page still builds its
buildings out of brushes in the field, which is what the game did before
its buildings became models. Open `docs/mockups/marching-cubes.html` and
`docs/mockups/hex-terrain.html`
straight off the checkout, or published: [marching cubes](https://claude.ai/code/artifact/342b7f52-5a1f-4000-94b3-1d3967b527d1) and
[hex terrain](https://claude.ai/code/artifact/87905fd8-d47b-4f2e-8cc3-8c226251a799). Drag to orbit, wheel to zoom, "Stand on it" for a
walker's eye height.

On Linux the harness needs `libwayland-dev libxkbcommon-dev libudev-dev
libasound2-dev pkg-config` to build and `libxkbcommon-x11-0` to open a
window under X11. If the build sits on the last line for a minute, that is
the linker, not a hang: `.cargo/config.toml` says what to do about it.
