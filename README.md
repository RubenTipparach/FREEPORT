# freeport

An open world space game about working in a space economy: thousands of
stars, tens of thousands of planets hundreds to thousands of kilometres
across, drawn from orbit to the ground with no seam, and a ship you climb out
of. Bevy 0.18, Rust, `f64` world frame with a floating origin, planets as
density fields dual contoured into triangles a chunk at a time in rings of
detail round the eye, a finite sea, cities built from recipes of brushes in
the same field, and a builder on foot.

`CLAUDE.md` is the rules and the reasons. `docs/freeport.html` is the design
page ([published](https://claude.ai/code/artifact/7822c376-33d9-4391-9907-a958426efc29)): what is being built, the stack, the tooling and the
two terrain mockups.

```sh
cargo test -p freeport_core                 # the engine free core: positions, the lattice and its rings, the field, the mesher, the sea, the towns and their recipes, the walker
./run.sh                                    # build the Bevy harness and open a window (run.bat on Windows); --test runs the suites first, --shot out.png takes a picture with no display, -- hands the rest to the app
./target/release/freeport_app               # the window by hand: a 10 km planet, on foot on a street of the port; F flies, B builds, Tab wires, Esc frees the mouse
python3 tools/shape.py --check              # no file over 900 lines, no function over 100
tools/get_material_maker.sh                 # fetch Material Maker into tools/ (gitignored)
tools/bake_materials.sh                     # bake materials/*.ptex to assets/textures/terrain
```

The two mockups compare the meshers on the same seed, field, sea, towns and
textures, with a first person walker on each (Walk the port, then WASD,
Shift, Space and the mouse): open `docs/mockups/marching-cubes.html` and
`docs/mockups/hex-terrain.html`
straight off the checkout, or published: [marching cubes](https://claude.ai/code/artifact/342b7f52-5a1f-4000-94b3-1d3967b527d1) and
[hex terrain](https://claude.ai/code/artifact/87905fd8-d47b-4f2e-8cc3-8c226251a799). Drag to orbit, wheel to zoom, "Stand on it" for a
walker's eye height.

On Linux the harness needs `libwayland-dev libxkbcommon-dev libudev-dev
libasound2-dev pkg-config` to build and `libxkbcommon-x11-0` to open a
window under X11. If the build sits on the last line for a minute, that is
the linker, not a hang: `.cargo/config.toml` says what to do about it.
