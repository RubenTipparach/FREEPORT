// The sea's surface: tenebris's water.fs.glsl, transcribed, on Bevy's own
// transmission. The vertex stage lifts the sheet along the radial with the
// sine swell tenebris's water.vs.glsl carries; the fragment stage bends
// the radial normal by the gradient of tenebris's fbm ripples, hands Bevy
// the water's THICKNESS along the view ray off the depth prepass so its
// attenuation is the depth of water actually looked through, and lays the
// sky's reflection on by fresnel and foam on the crests, as the GLSL does.
// Every coordinate is PLANET LOCAL, from a centre the material is handed,
// never the render frame's, which moves with the origin.

#import bevy_pbr::{
    mesh_functions,
    forward_io::{Vertex, VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    mesh_view_bindings::{view, globals},
    view_transformations::{position_world_to_clip, depth_ndc_to_view_z},
    prepass_utils::prepass_depth,
}
#import freeport::water::{fbm3_at, swell}

struct Water {
    // The planet's centre in the render frame; w the sea's radius.
    centre: vec4<f32>,
    // x: time scale, y: the RIPPLE COORDINATE's scale, cells a metre, and
    // the whole of it: tenebris's own 0.9 inside its `fbm3` is folded in
    // here so there is one number, and `water::RIPPLE` is that number on
    // the CPU, which works the cell out in f64. Two would be two places
    // to change it and a sea that pixelated again on the next tune.
    // z: wave steepness, w: swell amplitude.
    wave: vec4<f32>,
    // rgb: the deep water's colour under the surface; w: swell frequency.
    deep: vec4<f32>,
    // rgb: the sky at the horizon; w: how much of it the sheet reflects there.
    horizon: vec4<f32>,
    // rgb: the sky at the zenith; w: the slope the normal is clamped to.
    zenith: vec4<f32>,
    // rgb: foam; w: its intensity.
    foam: vec4<f32>,
    // Foam bands: crest low and high on the ripple height, slope low and high.
    band: vec4<f32>,
    // The sky at the horizon in rgb, and how much of it is in the way per
    // metre of view distance in w.
    fog: vec4<f32>,
    // The ground fog, as `terrain.wgsl`'s: x the metres it falls off
    // over, y how many times the plain haze it is at the sea, z the
    // radius that is measured from.
    haze: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> water: Water;

// How far the ripples are worn out over, metres, which is `terrain.wgsl`'s
// own bump fade on the other surface.
const RIPPLE_NEAR: f32 = 30.0;
const RIPPLE_FAR: f32 = 160.0;

// The sheet over the chunks, whose vertices ARE a mesh: the swell raises
// each along its own radial, which is tenebris's `water.vs.glsl`.
@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    var wp = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0));
    let q = wp.xyz - water.centre.xyz;
    let radial = normalize(q);
    // The SWELL keeps the imprecise q on purpose: its wavelengths are tens
    // of metres, so six centimetres of quantisation is a thousandth of a
    // wave and nothing an eye can find. It is the RIPPLES, at two thirds
    // of a metre, that the same six centimetres wrecks.
    wp = vec4<f32>(wp.xyz + radial * swell(q, water.wave, water.deep.w, globals.time), 1.0);
    out.world_position = wp;
    out.position = position_world_to_clip(wp.xyz);
    out.world_normal = mesh_functions::mesh_normal_local_to_world(vertex.normal, vertex.instance_index);
    // The ripple coordinate, as an exact cell and a fraction. The cell is
    // the CHUNK's, worked out in f64 on the CPU and the same at all three
    // corners of every triangle in it; the fraction carries this vertex's
    // own offset inside the chunk, which is a few metres and exact, and
    // interpolates across the triangle because it is affine in position.
#ifdef VERTEX_COLORS
    out.color = vertex.color;
#endif
#ifdef VERTEX_UVS_A
    let inside = vertex.position * water.wave.y;
#ifdef VERTEX_UVS_B
    out.uv = vertex.uv + inside.xy;
    out.uv_b = vec2<f32>(vertex.uv_b.x + inside.z, 0.0);
#else
    out.uv = vertex.uv + inside.xy;
#endif
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif
    return out;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    // The vertex COLOUR is not a colour on this surface: it carries the
    // ripple cell, which is a whole number over a million. Bevy's standard
    // material multiplies its base colour by the vertex colour, so handing
    // it straight through drew the sea at a million times white, which is
    // what the first render of this was: a blank sheet from the shore to
    // the horizon. It is a channel this shader owns, so it is masked out
    // before the standard material ever sees it.
    var plain = in;
#ifdef VERTEX_COLORS
    plain.color = vec4<f32>(1.0, 1.0, 1.0, 1.0);
#endif
    let q = in.world_position.xyz - water.centre.xyz;
    let radial = normalize(q);
    let t = globals.time * water.wave.x;
    // The ripple coordinate off the vertex: a cell the CPU worked out in
    // f64 and a fraction, never `q * scale` formed here out of a planet's
    // own radius held in one float.
    var cell = vec3<f32>(0.0);
    var frac = q * water.wave.y;
#ifdef VERTEX_COLORS
    cell = in.color.xyz;
#ifdef VERTEX_UVS_B
    frac = vec3<f32>(in.uv, in.uv_b.x);
#endif
#endif
    let h = fbm3_at(cell, frac, t);
    let e = 0.08;
    var grad = vec3<f32>(
        fbm3_at(cell, frac + vec3<f32>(e, 0.0, 0.0), t) - h,
        fbm3_at(cell, frac + vec3<f32>(0.0, e, 0.0), t) - h,
        fbm3_at(cell, frac + vec3<f32>(0.0, 0.0, e), t) - h,
    ) * 12.5;
    grad = grad - radial * dot(grad, radial);
    let steep = length(grad);
    var bent = grad;
    if (steep > water.zenith.w) {
        bent = grad * (water.zenith.w / steep);
    }
    // The ripples are worn out with distance, as the ground's normal maps
    // are and for the same reason: a ripple is detail at its own size, and
    // past a hundred metres one is under a pixel, so what it adds is not
    // a sheet of water, it is a sheet of NOISE. Faded, the far sea is the
    // swell's own shape and the sky on it.
    let away = length(in.world_position.xyz - view.world_position);
    let near = 1.0 - smoothstep(RIPPLE_NEAR, RIPPLE_FAR, away);
    let n = normalize(radial - bent * water.wave.z * near);

    var out: FragmentOutput;
    if (!is_front) {
        // Seen from under the sheet: the deep colour, the sky through it
        // where the view comes up steep enough to leave.
        let up = clamp(dot(-radial, normalize(in.world_position.xyz - view.world_position)), 0.0, 1.0);
        let window = smoothstep(0.55, 0.75, up);
        out.color = vec4<f32>(mix(water.deep.rgb, water.horizon.rgb * 0.6, window), 1.0);
        return out;
    }

    // How much water the view ray crosses before the ground behind, off
    // the depth prepass: the sheet opts out of that pass itself, so what
    // it reads is the sea floor rather than its own depth.
#ifdef DEPTH_PREPASS
    let scene_z = depth_ndc_to_view_z(prepass_depth(in.position, 0u));
    let here_z = depth_ndc_to_view_z(in.position.z);
    let thickness = max(here_z - scene_z, 0.02);
#else
    let thickness = 2.0;
#endif

    var pbr_input = pbr_input_from_standard_material(plain, is_front);
    pbr_input.N = n;
    pbr_input.material.thickness = thickness;
    var color = apply_pbr_lighting(pbr_input);

    // The sky, by fresnel: brighter at the horizon than the zenith, and
    // only as much of the horizon as the material allows.
    let V = pbr_input.V;
    let r = reflect(-V, n);
    let up = clamp(dot(r, radial), 0.0, 1.0);
    let sky = mix(water.horizon.rgb, water.zenith.rgb, up);
    let fresnel = 0.02 + 0.98 * pow(1.0 - max(dot(V, n), 0.0), 5.0);
    let strength = fresnel * mix(water.horizon.w, 1.0, up);
    color = vec4<f32>(mix(color.rgb, sky * view.exposure, strength), color.a);

    // Foam on the crests and on the steep, as the GLSL blends them.
    let foam = max(
        smoothstep(water.band.x, water.band.y, h) * 0.55,
        smoothstep(water.band.z, water.band.w, steep) * 0.26,
    ) * water.foam.w * near;
    color = vec4<f32>(mix(color.rgb, water.foam.rgb * view.exposure, foam), color.a);

    color = main_pass_post_lighting_processing(pbr_input, color);
    // The same air the ground fades into, so the sea meets the land in
    // one haze and the horizon is one line.
    let eye_up = length(view.world_position - water.centre.xyz) - water.haze.z;
    let here_up = length(q) - water.haze.z;
    let mid_up = max((eye_up + here_up) * 0.5, 0.0);
    let pooled = 1.0 + (water.haze.y - 1.0) * exp(-mid_up / max(water.haze.x, 1.0));
    let in_the_way = 1.0 - exp(-away * water.fog.w * pooled);
    out.color = vec4<f32>(
        mix(color.rgb, water.fog.rgb * view.exposure, in_the_way),
        color.a,
    );
    return out;
}
