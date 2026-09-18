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
    // xyz the way the sun lies, a unit direction in the world; w how
    // bright a city burns on the night side.
    sun: vec4<f32>,
}

// How dark the unlit half of a body goes, as a share of its own albedo.
//
// A body drawn with nothing but PBR has a night side the colour of the
// sky's own ambient, which on a world with air is a lit blue ball with a
// terminator painted on it: the owner asked for the dark side to be
// DARK, and what was making it bright was never the sun. Nought would be
// a hole in the picture rather than a planet, which is this repository's
// own ambient lesson twice over, so it is a twentieth: enough to read a
// silhouette against the stars and dark enough that a city on it is the
// brightest thing there.
const NIGHT_FLOOR: f32 = 0.05;

// Where the terminator falls, in the cosine between the ground's own
// radial and the sun. It is a BAND rather than a line because a planet
// has air: the sun sets over a few degrees of longitude and a hard edge
// reads as a shadow cast by something off screen.
const DUSK_FROM: f32 = 0.14;
const DUSK_TO: f32 = -0.10;

// The colour a city burns, linear, and the colour of a road's lamps.
// Sodium, warmer than anything the daylight side carries, because what
// says a light is a light rather than a bright patch of ground is that it
// is a colour the ground is not.
const LAMP: vec3<f32> = vec3<f32>(1.0, 0.72, 0.36);

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

    // The chart's slope, applied along the axes it was MEASURED along:
    // east is where u grows and north where v shrinks, which is exactly
    // what `chart::slopes_of` takes its two central differences over.
    // They are the derivative of `pixel_dir` itself, so they degenerate
    // only at a pole, where the chart's own longitude is undefined too.
    //
    // What this replaces swapped its reference axis at |y| >= 0.9 to keep
    // one cross product well conditioned. That is 64 degrees of latitude,
    // and everything poleward of it was then shaded in a frame that was
    // not east and north at all, with a hard ring where it switched.
    let flat = length(vec2<f32>(d.x, d.z));
    var east = vec3<f32>(0.0, 0.0, 1.0);
    if flat > 1e-6 {
        east = vec3<f32>(-d.z, 0.0, d.x) / flat;
    }
    let north = cross(east, d);
    // SUBTRACTED, both of them: ground tilts its normal AWAY from what it
    // climbs. The first cut added the north term, and on the steepest
    // northward texel of the test planet the normal came out 0.41 ALONG
    // north where it had to be negative, so every slope on every body was
    // lit from the wrong side along one axis and a ridge read as a gully.
    let bend = (slope.rg * 2.0 - 1.0) * distant.centre.w;
    let n = normalize(d - east * bend.x - north * bend.y);

    // The NIGHT side, measured on the body's own radial rather than on
    // the bent normal: which half of a planet the sun is on is a fact
    // about the planet, and a normal leaned off a mountain would put a
    // patch of midnight on a slope at noon.
    let night = 1.0 - smoothstep(DUSK_TO, DUSK_FROM, dot(d, distant.sun.xyz));
    pbr_input.material.base_color =
        vec4<f32>(albedo.rgb * mix(1.0, NIGHT_FLOOR, night), 1.0);
    // And the LIGHTS on it: the chart's own city and road mask, burning
    // only where the sun is not. They are EMISSIVE, so nothing about the
    // lighting takes them away, which is the whole point of a light.
    pbr_input.material.emissive =
        vec4<f32>(LAMP * (slope.b * night * distant.sun.w), 1.0);
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
