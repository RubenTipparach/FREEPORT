# freeport

An open world space game about working in a space economy: thousands of
stars, tens of thousands of planets hundreds to thousands of kilometres
across, drawn from orbit to the ground with no seam, and a ship you climb out
of. Bevy 0.18, Rust, `f64` world frame with a floating origin, planets as
density fields marched into triangles.

`CLAUDE.md` is the rules and the reasons. `docs/freeport.html` is the design
page: what is being built, the stack, the tooling and the two terrain mockups.

```sh
cargo test -p freeport_core                 # the engine free core: positions, the cube sphere, the field, the mesher
cargo build --release -p freeport_app       # the Bevy harness
./target/release/freeport_app               # a window: one marched planetoid
python3 tools/shape.py --check              # no file over 900 lines, no function over 100
tools/get_material_maker.sh                 # fetch Material Maker into tools/ (gitignored)
tools/bake_materials.sh                     # bake materials/*.ptex to assets/textures/terrain
```

The two mockups compare the meshers on the same seed, field and textures:
open `docs/mockups/marching-cubes.html` and `docs/mockups/hex-terrain.html`
straight off the checkout. Drag to orbit, wheel to zoom, "Stand on it" for a
walker's eye height.

On Linux the harness needs `libwayland-dev libxkbcommon-dev libudev-dev
libasound2-dev pkg-config` to build and `libxkbcommon-x11-0` to open a
window under X11. If the build sits on the last line for a minute, that is
the linker, not a hang: `.cargo/config.toml` says what to do about it.
