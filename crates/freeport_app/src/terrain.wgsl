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

// How many town frames the array can hold, `terrain::FRAMES`.
const FRAMES: i32 = 16;

struct Terrain {
    // x: metres a tile on the ground, y: metres a tile on concrete, z: how
    // many town frames are set, w: the sea's radius.
    params: vec4<f32>,
    // The planet's centre in the render frame: every coordinate here is
    // planet local, which is the rule for any shader that reasons about a
    // body.
    centre: vec4<f32>,
    // Three lanes a town: its direction with the radius its ground is at
    // in w, its east, its north.
    frames: array<vec4<f32>, 48>,
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
const L_SAND: i32 = 1;
const L_GRASS: i32 = 2;
const L_CONCRETE: i32 = 3;
const L_PLATE: i32 = 4;

// The sand band: all sand to this over the sea, all grass past the second,
// metres. The mockup's numbers, which is a beach a walker wades out of.
const SAND_TO: f32 = 1.3;
const GRASS_FROM: f32 = 2.8;

// The materials, as `freeport_core::field` numbers them.
const M_TERRAIN: f32 = 0.0;
const M_CONCRETE: f32 = 1.0;
const M_PLATE: f32 = 2.0;
const M_GLASS: f32 = 3.0;
const M_LAMP: f32 = 4.0;
const M_LIT: f32 = 5.0;
const M_STREET: f32 = 6.0;

// One where the material is `m`, else nought: the vertex colour carries
// the material as a whole number, flat over the triangle.
fn is(material: f32, m: f32) -> f32 {
    return select(0.0, 1.0, abs(material - m) < 0.5);
}

fn tri_weights(n: vec3<f32>) -> vec3<f32> {
    let w = pow(abs(n), vec3<f32>(4.0));
    return w / (w.x + w.y + w.z);
}

fn tri(t: texture_2d_array<f32>, s: sampler, layer: i32, p: vec3<f32>, w: vec3<f32>) -> vec4<f32> {
    return textureSample(t, s, p.yz, layer) * w.x
        + textureSample(t, s, p.xz, layer) * w.y
        + textureSample(t, s, p.xy, layer) * w.z;
}

// How much of the grass set's own normal is kept: a hay normal at full
// strength on ground seen at a grazing angle speckles, which is the
// swarm-demo lesson (its finishes run at a fifth) on a field.
const GRASS_BUMP: f32 = 0.45;

// A normal toward the surface's own, so a map can be worn lightly.
fn soften(mapped: vec3<f32>, n: vec3<f32>, k: f32) -> vec3<f32> {
    return normalize(mix(n, mapped, k));
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

// The town whose centre is nearest a direction, or minus one with no town
// set. The count is uniform, so the loop is too.
fn nearest_frame(up: vec3<f32>) -> i32 {
    let count = min(i32(terrain.params.z), FRAMES);
    var best = -1;
    var best_dot = -2.0;
    for (var i = 0; i < count; i++) {
        let d = dot(up, terrain.frames[i * 3].xyz);
        if (d > best_dot) {
            best_dot = d;
            best = i;
        }
    }
    return best;
}

// Where a point is in a town's frame, and the frame's axes at the point:
// east and north are measured on the sphere the town's ground is at, from
// the town's centre (a point projected on its own tangent plane is nought
// everywhere, the mockup's float noise), and the height is off that
// sphere, so a building plumb on its own lot has its walls on constant
// east or north and its floors on constant height, and a panel is level
// and plumb whatever the planet's axes do. The core's `Frame::local` is
// the same map from the lot's own anchor.
struct Local {
    p: vec3<f32>,
    n: vec3<f32>,
    to_world: mat3x3<f32>,
}

fn in_frame(f: i32, rel: vec3<f32>, up: vec3<f32>, n: vec3<f32>) -> Local {
    var l: Local;
    l.p = rel;
    l.n = n;
    l.to_world = mat3x3<f32>(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(0.0, 0.0, 1.0));
    if (f < 0) {
        return l;
    }
    let base = terrain.frames[f * 3].w;
    let east_t = terrain.frames[f * 3 + 1].xyz;
    let north_t = terrain.frames[f * 3 + 2].xyz;
    let east = normalize(east_t - up * dot(east_t, up));
    let north = cross(up, east);
    l.p = vec3<f32>(dot(up, east_t) * base, dot(up, north_t) * base, length(rel) - base);
    l.n = vec3<f32>(dot(n, east), dot(n, north), dot(n, up));
    l.to_world = mat3x3<f32>(east, north, up);
    return l;
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
    let ground = is(material, M_TERRAIN);
    let slope = 1.0 - clamp(dot(n, up), 0.0, 1.0);
    let steep = smoothstep(0.34, 0.6, slope);
    let w_rock = steep * ground;
    // Sand along the shore and under the shallows, grass above it: the
    // mockup's band by height, measured off the sea's own radius.
    let over_sea = length(rel) - terrain.params.w;
    let sand = 1.0 - smoothstep(SAND_TO, GRASS_FROM, over_sea);
    let level = (1.0 - steep) * ground;
    let w_sand = level * sand;
    let w_grass = level * (1.0 - sand);
    let street = is(material, M_STREET);
    let w_conc = is(material, M_CONCRETE) + street;
    let w_plate = is(material, M_PLATE);
    // The panes and the lamps are flat colours with no map: what they
    // are is a colour and a glow, not a surface.
    let glass = is(material, M_GLASS);
    let lamp = is(material, M_LAMP);
    let lit = is(material, M_LIT);
    let flat = glass + lamp + lit;
    // The ground is mapped in the planet's frame, and concrete, plate and
    // a street in the nearest town's, where they are level and plumb.
    let pg = rel / terrain.params.x;
    let w = tri_weights(n);
    let local = in_frame(nearest_frame(up), rel, up, n);
    let pc = local.p / terrain.params.y;
    let wc = tri_weights(local.n);
    let mapped = w_rock + w_sand + w_grass + w_conc + w_plate;
    var albedo = tri(albedo_maps, albedo_sampler, L_ROCK, pg, w).rgb * w_rock
        + tri(albedo_maps, albedo_sampler, L_SAND, pg, w).rgb * w_sand
        + tri(albedo_maps, albedo_sampler, L_GRASS, pg, w).rgb * w_grass
        + tri(albedo_maps, albedo_sampler, L_CONCRETE, pc, wc).rgb * w_conc
        + tri(albedo_maps, albedo_sampler, L_PLATE, pc, wc).rgb * w_plate;
    // A street is the concrete set, darker, as paving is.
    albedo = albedo * (1.0 - street * 0.45);
    var orm = tri(orm_maps, orm_sampler, L_ROCK, pg, w).rgb * w_rock
        + tri(orm_maps, orm_sampler, L_SAND, pg, w).rgb * w_sand
        + tri(orm_maps, orm_sampler, L_GRASS, pg, w).rgb * w_grass
        + tri(orm_maps, orm_sampler, L_CONCRETE, pc, wc).rgb * w_conc
        + tri(orm_maps, orm_sampler, L_PLATE, pc, wc).rgb * w_plate;
    let built_n = local.to_world
        * (tri_normal(L_CONCRETE, pc, wc, local.n) * w_conc
            + tri_normal(L_PLATE, pc, wc, local.n) * w_plate);
    var nm = tri_normal(L_ROCK, pg, w, n) * w_rock
        + tri_normal(L_SAND, pg, w, n) * w_sand
        + soften(tri_normal(L_GRASS, pg, w, n), n, GRASS_BUMP) * w_grass
        + built_n;
    albedo = albedo * mapped
        + vec3<f32>(0.05, 0.08, 0.1) * glass
        + vec3<f32>(0.95, 0.92, 0.85) * lamp
        + vec3<f32>(0.9, 0.8, 0.6) * lit;
    orm = orm * mapped + vec3<f32>(1.0, 0.08, 0.0) * glass + vec3<f32>(1.0, 0.6, 0.0) * (lamp + lit);
    nm = normalize(nm * mapped + n * flat);
    pbr_input.material.base_color = vec4<f32>(albedo, 1.0);
    pbr_input.material.perceptual_roughness = orm.g;
    pbr_input.material.metallic = orm.b;
    pbr_input.material.emissive = vec4<f32>(vec3<f32>(8.0, 7.4, 6.0) * lamp + vec3<f32>(3.0, 2.4, 1.5) * lit, 1.0);
    pbr_input.diffuse_occlusion = vec3<f32>(orm.r);
    pbr_input.N = nm;
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
