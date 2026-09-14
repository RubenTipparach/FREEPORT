// The ground's material: the baked sets on three planes, the transcription
// of the mockup's terrain shader (`docs/mockups/common.js`, `tri`, `triN`
// and the slope blend), on Bevy's own PBR lighting. Every sample is taken
// whatever the material and blended by weight, because a texture sample
// under a branch is not in uniform control flow and the compiler refuses
// it; the material rides the triangle FLAT in the vertex colour's red, so
// concrete meets rock on a line.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
}

struct Terrain {
    // x: metres a tile on the ground, y: metres a tile on concrete.
    params: vec4<f32>,
    // The planet's centre in the render frame: every coordinate here is
    // planet local, which is the rule for any shader that reasons about a
    // body.
    centre: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> terrain: Terrain;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var albedo_maps: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var albedo_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var normal_maps: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var normal_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(105) var orm_maps: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(106) var orm_sampler: sampler;

// The layers, in the order `terrain::SETS` stacks them.
const L_ROCK: i32 = 0;
const L_GRASS: i32 = 1;
const L_CONCRETE: i32 = 2;

fn tri_weights(n: vec3<f32>) -> vec3<f32> {
    let w = pow(abs(n), vec3<f32>(4.0));
    return w / (w.x + w.y + w.z);
}

fn tri(t: texture_2d_array<f32>, s: sampler, layer: i32, p: vec3<f32>, w: vec3<f32>) -> vec4<f32> {
    return textureSample(t, s, p.yz, layer) * w.x
        + textureSample(t, s, p.xz, layer) * w.y
        + textureSample(t, s, p.xy, layer) * w.z;
}

// A normal map read on three planes and turned into the world, the
// mockup's `triN` line for line.
fn tri_normal(layer: i32, p: vec3<f32>, w: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    var tx = textureSample(normal_maps, normal_sampler, p.yz, layer).xyz * 2.0 - 1.0;
    var ty = textureSample(normal_maps, normal_sampler, p.xz, layer).xyz * 2.0 - 1.0;
    var tz = textureSample(normal_maps, normal_sampler, p.xy, layer).xyz * 2.0 - 1.0;
    tx = vec3<f32>(tx.xy + n.zy, abs(tx.z) * n.x);
    ty = vec3<f32>(ty.xy + n.xz, abs(ty.z) * n.y);
    tz = vec3<f32>(tz.xy + n.xy, abs(tz.z) * n.z);
    return normalize(tx.zyx * w.x + ty.xzy * w.y + tz.xyz * w.z);
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let n = normalize(in.world_normal);
    let rel = in.world_position.xyz - terrain.centre.xyz;
    let up = normalize(rel);
    var material = 0.0;
#ifdef VERTEX_COLORS
    material = in.color.r;
#endif
    let built = select(0.0, 1.0, material > 0.5);
    let slope = 1.0 - clamp(dot(n, up), 0.0, 1.0);
    let steep = smoothstep(0.34, 0.6, slope);
    let w_rock = steep * (1.0 - built);
    let w_grass = (1.0 - steep) * (1.0 - built);
    let w_conc = built;
    let pg = rel / terrain.params.x;
    let pc = rel / terrain.params.y;
    let w = tri_weights(n);
    let albedo = tri(albedo_maps, albedo_sampler, L_ROCK, pg, w).rgb * w_rock
        + tri(albedo_maps, albedo_sampler, L_GRASS, pg, w).rgb * w_grass
        + tri(albedo_maps, albedo_sampler, L_CONCRETE, pc, w).rgb * w_conc;
    let orm = tri(orm_maps, orm_sampler, L_ROCK, pg, w).rgb * w_rock
        + tri(orm_maps, orm_sampler, L_GRASS, pg, w).rgb * w_grass
        + tri(orm_maps, orm_sampler, L_CONCRETE, pc, w).rgb * w_conc;
    let nm = normalize(tri_normal(L_ROCK, pg, w, n) * w_rock
        + tri_normal(L_GRASS, pg, w, n) * w_grass
        + tri_normal(L_CONCRETE, pc, w, n) * w_conc);
    pbr_input.material.base_color = vec4<f32>(albedo, 1.0);
    pbr_input.material.perceptual_roughness = orm.g;
    pbr_input.material.metallic = orm.b;
    pbr_input.diffuse_occlusion = vec3<f32>(orm.r);
    pbr_input.N = nm;
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
