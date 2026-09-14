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

fn gn_fade(t: f32) -> f32 {
    return t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
}

fn gn_hash(x: i32, y: i32, z: i32) -> u32 {
    let h = (u32(x + 1073741824) * 374761393u) ^ (u32(y + 1073741824) * 668265263u) ^ (u32(z + 1073741824) * 1274126177u);
    let g = (h ^ (h >> 13u)) * 1103515245u;
    return g ^ (g >> 16u);
}

fn gn_grad(hash: u32, x: f32, y: f32, z: f32) -> f32 {
    let h = hash & 15u;
    let u = select(y, x, h < 8u);
    var v: f32;
    if (h < 4u) {
        v = y;
    } else {
        v = select(z, x, h == 12u || h == 14u);
    }
    let a = select(u, -u, (h & 1u) != 0u);
    let b = select(v, -v, (h & 2u) != 0u);
    return a + b;
}

// Gradient noise, tenebris's gnoise3.
fn gnoise3(p: vec3<f32>) -> f32 {
    let f = floor(p);
    let xi = i32(f.x);
    let yi = i32(f.y);
    let zi = i32(f.z);
    let d = p - f;
    let u = gn_fade(d.x);
    let v = gn_fade(d.y);
    let w = gn_fade(d.z);
    let n000 = gn_grad(gn_hash(xi, yi, zi), d.x, d.y, d.z);
    let n100 = gn_grad(gn_hash(xi + 1, yi, zi), d.x - 1.0, d.y, d.z);
    let n010 = gn_grad(gn_hash(xi, yi + 1, zi), d.x, d.y - 1.0, d.z);
    let n110 = gn_grad(gn_hash(xi + 1, yi + 1, zi), d.x - 1.0, d.y - 1.0, d.z);
    let n001 = gn_grad(gn_hash(xi, yi, zi + 1), d.x, d.y, d.z - 1.0);
    let n101 = gn_grad(gn_hash(xi + 1, yi, zi + 1), d.x - 1.0, d.y, d.z - 1.0);
    let n011 = gn_grad(gn_hash(xi, yi + 1, zi + 1), d.x, d.y - 1.0, d.z - 1.0);
    let n111 = gn_grad(gn_hash(xi + 1, yi + 1, zi + 1), d.x - 1.0, d.y - 1.0, d.z - 1.0);
    return mix(
        mix(mix(n000, n100, u), mix(n010, n110, u), v),
        mix(mix(n001, n101, u), mix(n011, n111, u), v),
        w,
    );
}

// Three octaves drifting three ways, tenebris's fbm3.
fn fbm3(p: vec3<f32>, t: f32) -> f32 {
    var q = p * 0.9 + vec3(t * 0.35, t * 0.18, t * -0.42);
    var h = gnoise3(q) * 0.5;
    q = q * 2.1 + vec3(t * -0.22, t * 0.33, t * 0.17);
    h += gnoise3(q) * 0.28;
    q = q * 2.3 + vec3(t * 0.19, t * -0.27, t * 0.11);
    h += gnoise3(q) * 0.15;
    return h;
}

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    var wp = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0));
    let q = wp.xyz - water.centre.xyz;
    let radial = normalize(q);
    let t = globals.time * water.wave.x;
    let f = water.deep.w;
    let swell = (sin(t * 0.9 + q.x * 1.4 * f + q.z * 0.6 * f) * 0.18
        + sin(t * 1.3 - q.x * 0.7 * f + q.z * 1.2 * f) * 0.12
        + sin(t * 1.7 + q.x * 2.3 * f - q.z * 1.9 * f) * 0.06) * water.wave.w;
    wp = vec4<f32>(wp.xyz + radial * swell, 1.0);
    out.world_position = wp;
    out.position = position_world_to_clip(wp.xyz);
    out.world_normal = mesh_functions::mesh_normal_local_to_world(vertex.normal, vertex.instance_index);
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif
    return out;
}

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
    let n = normalize(radial - bent * water.wave.z);

    var out: FragmentOutput;
    if (!is_front) {
        // Seen from under the sheet: the deep colour, the sky through it
        // where the view comes up steep enough to leave.
        let up = clamp(dot(-radial, normalize(in.world_position.xyz - view.world_position)), 0.0, 1.0);
        let window = smoothstep(0.55, 0.75, up);
        out.color = vec4<f32>(mix(water.deep.rgb, water.horizon.rgb * 0.6, window), 1.0);
        return out;
    }

    // How much water the view ray crosses before the ground behind: the
    // prepass holds the ground's depth, this fragment its own.
#ifdef DEPTH_PREPASS
    let scene_z = depth_ndc_to_view_z(prepass_depth(in.position, 0u));
    let here_z = depth_ndc_to_view_z(in.position.z);
    let thickness = max(here_z - scene_z, 0.02);
#else
    let thickness = 2.0;
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
    ) * water.foam.w;
    color = vec4<f32>(mix(color.rgb, water.foam.rgb * view.exposure, foam), color.a);

    out.color = main_pass_post_lighting_processing(pbr_input, color);
    return out;
}
