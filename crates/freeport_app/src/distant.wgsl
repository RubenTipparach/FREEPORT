// A whole body from far off: the equirectangular chart `core::chart` bakes,
// sampled PER PIXEL on a displaced icosphere.
//
// It is tenebris's `distant.glsl` ported. What it replaces is one colour a
// VERTEX, which on a sphere of forty six thousand triangles is a colour
// every thirty kilometres: continents came out as soft blobs with no coast
// anywhere on them. The albedo carries the biome, the water blend and the
// height shade; its alpha is the water mask that gates the sun's glint, so
// an ocean catches the light and dry ground stays matte. The normal map
// carries the slope of the altitude, which is what puts a mountain range on
// a sphere no mesh at this size could hold one on.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::view,
}

struct Distant {
    // xyz the body's centre in the render frame, w how hard the chart's
    // own slope bends the normal.
    centre: vec4<f32>,
    // The sky at the horizon in rgb, and how much of it is in the way per
    // metre in w, the same pair the ground is faded into.
    fog: vec4<f32>,
    // x the water's specular strength, y its power, z spare, w spare.
    sea: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> distant: Distant;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var chart: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var chart_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var slopes: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var slopes_sampler: sampler;

const TAU: f32 = 6.283185307179586;
const PI: f32 = 3.141592653589793;

// The inverse of `chart::pixel_dir`. **The two move together**: change one
// and every coast on the planet slides round it.
fn dir_to_uv(d: vec3<f32>) -> vec2<f32> {
    let u = 0.5 + atan2(d.z, d.x) / TAU;
    let v = 0.5 - asin(clamp(d.y, -1.0, 1.0)) / PI;
    return vec2<f32>(u, v);
}

// The derivative of an equirect coordinate, with the SEAM taken out of it.
// At the meridian where u wraps from one to nought the raw derivative is a
// whole turn wide, and a GPU picks its mip level off the derivative, so
// the seam samples the smallest mip there is and draws as a bright ragged
// line down the planet. Any step over half the image is the wrap rather
// than the ground, and it is the wrap that is subtracted.
fn unwrapped(d: vec2<f32>) -> vec2<f32> {
    var out = d;
    if abs(out.x) > 0.5 {
        out.x -= sign(out.x);
    }
    return out;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let rel = in.world_position.xyz - distant.centre.xyz;
    let d = normalize(rel);
    let uv = dir_to_uv(d);
    let dx = unwrapped(dpdx(uv));
    let dy = unwrapped(dpdy(uv));
    let albedo = textureSampleGrad(chart, chart_sampler, uv, dx, dy);
    let slope = textureSampleGrad(slopes, slopes_sampler, uv, dx, dy);

    // The chart's slope, applied in a tangent frame built on the radial.
    // The reference axis swaps near the poles, where a frame built on the
    // world's own up has nothing to cross with.
    var reference = vec3<f32>(0.0, 1.0, 0.0);
    if abs(d.y) >= 0.9 {
        reference = vec3<f32>(1.0, 0.0, 0.0);
    }
    let tangent = normalize(cross(reference, d));
    let bitangent = cross(d, tangent);
    let bend = (slope.rg * 2.0 - 1.0) * distant.centre.w;
    let n = normalize(d + tangent * bend.x + bitangent * bend.y);

    pbr_input.material.base_color = vec4<f32>(albedo.rgb, 1.0);
    // Water is smooth and everything else is not, which is the whole of
    // why the mask is worth carrying: an ocean has to catch the sun where
    // the land beside it does not.
    let water = albedo.a;
    pbr_input.material.perceptual_roughness = mix(0.92, 0.12, water);
    pbr_input.material.metallic = 0.0;
    pbr_input.material.reflectance = vec3<f32>(mix(0.02, distant.sea.x, water));
    pbr_input.N = n;
    pbr_input.world_normal = n;
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    // The same air the ground fades into, so a body seen through its own
    // atmosphere from inside it does not stand out of the haze.
    let away = length(in.world_position.xyz - view.world_position);
    let in_the_way = 1.0 - exp(-away * distant.fog.w);
    out.color = vec4<f32>(
        mix(out.color.rgb, distant.fog.rgb * view.exposure, in_the_way),
        out.color.a,
    );
    return out;
}
