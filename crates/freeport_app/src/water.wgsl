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
#import freeport::water::{fbm3, swell}

struct Water {
    // The planet's centre in the render frame; w the sea's radius.
    centre: vec4<f32>,
    // x: time scale, y: ripple scale, z: wave steepness, w: swell amplitude.
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
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> water: Water;

// How far the ripples are worn out over, metres, which is `terrain.wgsl`'s
// own bump fade on the other surface.
const RIPPLE_NEAR: f32 = 30.0;
const RIPPLE_FAR: f32 = 160.0;

// The sheet over the dual contoured chunks, whose vertices ARE a mesh.
// Guarded, because the hex world's sheet has no mesh to read: its vertex
// stage is `tiers.wgsl`'s `sea` entry point and its counting mesh carries
// a position and nothing else, so `Vertex` here would have no `normal` to
// name. A module is compiled whole, entry points it will never run
// included, so an entry point that cannot type check under a caller's
// shader defs has to be absent under them.
#ifdef VERTEX_NORMALS
@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    var wp = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0));
    let q = wp.xyz - water.centre.xyz;
    let radial = normalize(q);
    wp = vec4<f32>(wp.xyz + radial * swell(q, water.wave, water.deep.w, globals.time), 1.0);
    out.world_position = wp;
    out.position = position_world_to_clip(wp.xyz);
    out.world_normal = mesh_functions::mesh_normal_local_to_world(vertex.normal, vertex.instance_index);
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif
    return out;
}
#endif

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    let q = in.world_position.xyz - water.centre.xyz;
    let radial = normalize(q);
    let t = globals.time * water.wave.x;
    let p = q * water.wave.y;
    let h = fbm3(p, t);
    let e = 0.08;
    var grad = vec3<f32>(
        fbm3(p + vec3<f32>(e, 0.0, 0.0), t) - h,
        fbm3(p + vec3<f32>(0.0, e, 0.0), t) - h,
        fbm3(p + vec3<f32>(0.0, 0.0, e), t) - h,
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

    // How much water the view ray crosses before the ground behind.
#ifdef WATER_COLUMN
    // The hex world hands the water's own COLUMN under this vertex down in
    // `uv.x` (`tiers.wgsl`, the `sea` entry point), because the field
    // knows exactly how deep the sea is there. It is the better answer:
    // the depth buffer measures to whatever is behind, which at a shore is
    // the beach BESIDE the water rather than the floor under it, and the
    // tiers are not in the depth prepass at all. The path through the
    // water is that column over how steeply the view leaves the surface,
    // and the view is clamped off the grazing angle where that diverges.
    let to_eye = normalize(view.world_position - in.world_position.xyz);
    let thickness = max(in.uv.x / max(dot(to_eye, radial), 0.2), 0.02);
#else
#ifdef DEPTH_PREPASS
    let scene_z = depth_ndc_to_view_z(prepass_depth(in.position, 0u));
    let here_z = depth_ndc_to_view_z(in.position.z);
    let thickness = max(here_z - scene_z, 0.02);
#else
    let thickness = 2.0;
#endif
#endif

    var pbr_input = pbr_input_from_standard_material(in, is_front);
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

    out.color = main_pass_post_lighting_processing(pbr_input, color);
    return out;
}
