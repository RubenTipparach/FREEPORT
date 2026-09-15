// The sky dome: one sphere round the eye, drawn inside out, whose every
// fragment is a view ray marched through `freeport::atmos`. There is no
// texture in it and nothing baked: the sky is the air, so it is right at
// noon, at dusk, at night and from orbit by construction rather than by
// four things somebody matched up.
//
// The dome is BEHIND everything, so it is the sky where nothing else was
// drawn and nowhere else. What the air does in FRONT of a hillside is the
// fog term in `terrain.wgsl` and `water.wgsl`, off the same march run on
// the CPU (`atmos::horizon`), which is why the two cannot drift apart.

#import bevy_pbr::{
    forward_io::VertexOutput,
    mesh_view_bindings::view,
}
#import freeport::atmos

struct Sky {
    // The planet's centre in the render frame, w: the dome's own radius,
    // which nothing here reads and the log prints.
    centre: vec4<f32>,
    // Where the sun is, as a direction, w: unused.
    sun: vec4<f32>,
    // x: the ground's radius, y: the shell's.
    shell: vec4<f32>,
    // x: the scale height, y: rayleigh, z: mie, w: mie's g.
    coef: vec4<f32>,
    // rgb: the wavelength ratios, w: the sun's intensity.
    waves: vec4<f32>,
    // rgb: the dusk tint, w: its strength.
    tint: vec4<f32>,
    glow: vec4<f32>,
    band: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> dome: Sky;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // The dome rides the eye, so the way out to this fragment IS the view
    // ray, and the eye in the planet's own frame is what the march wants.
    let look = in.world_position.xyz - view.world_position;
    let eye = view.world_position - dome.centre.xyz;
    let air = atmos::air_of(dome.shell, dome.coef, dome.waves, dome.tint, dome.glow, dome.band);
    let got = atmos::sky(air, eye, look, dome.sun.xyz, in.position.xy);
    // The march answers in nought to one, which is a share and not a
    // radiance, so it is scaled into candela and then through the camera's
    // own exposure: that is what puts the sky at the same brightness as
    // the ground under it rather than five ten thousandths of it.
    return vec4<f32>(got.rgb * dome.sun.w * view.exposure, got.a);
}
