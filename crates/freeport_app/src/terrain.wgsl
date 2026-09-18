// The ground's material: the baked sets on three planes, the transcription
// of the mockup's terrain shader (`docs/mockups/common.js`, `tri`, `triN`
// and the slope blend), on Bevy's own PBR lighting. Mapping derivatives
// are evaluated before branching; textureSampleGrad then preserves the
// mip selection while skipping materials and projections of zero weight.
// The material rides the triangle FLAT in the vertex colour's red, so
// concrete meets rock on a line.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::view,
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
    // The sky at the horizon in rgb, and how much of it is in the way per
    // metre of view distance in w.
    fog: vec4<f32>,
    // The ground fog: x the metres its density falls off over, y how many
    // times the plain haze it is down at the sea, z the radius that is
    // measured from.
    haze: vec4<f32>,
    palette: vec4<f32>,
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

// How far the normal maps are worn out over, metres: full strength under
// the first and gone past the second. A bump map is detail at the size of
// its own tile, two metres on the ground, and past a few dozen metres a
// tile is under a pixel: what it adds there is not detail, it is NOISE,
// and it is the noise that is left once the mip chain has done albedo's
// share. Faded out, the far ground is shaded by its own shape alone,
// which is what the eye expects of a hillside a hundred metres off.
const BUMP_NEAR: f32 = 25.0;
const BUMP_FAR: f32 = 140.0;

// What a biome LOOKS like, `freeport_core::biome::Kind::colour`'s own
// table, and the same one the body's chart is painted from, so a coast
// seen from orbit and the same coast walked on are the same colours.
// Linear rgb.
const C_SNOW: vec3<f32> = vec3<f32>(0.82, 0.85, 0.88);
const C_DESERT: vec3<f32> = vec3<f32>(0.66, 0.50, 0.28);
const C_SAVANNA: vec3<f32> = vec3<f32>(0.44, 0.40, 0.19);
const C_GRASS: vec3<f32> = vec3<f32>(0.20, 0.30, 0.11);
const C_FOREST: vec3<f32> = vec3<f32>(0.10, 0.19, 0.08);
const C_BEACH: vec3<f32> = vec3<f32>(0.62, 0.55, 0.38);

// `biome`'s own thresholds: colder than this freezes, hotter and drier
// than the next two is desert, and wetter than the last is forest.
const FREEZING: f32 = 0.18;
const ARID: f32 = 0.36;
const WOODED: f32 = 0.52;

// How much of the biome's colour is laid over the set's own. The texture
// still carries the DETAIL, the grain and the shadow of it, and the tint
// carries what the place is: at one the hay under the feet in a desert is
// sand coloured hay, and at nought every planet is the same meadow, which
// is what this world was.
const BIOME_TINT: f32 = 0.72;

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

struct Mapping {
    p: vec3<f32>,
    dx: vec3<f32>,
    dy: vec3<f32>,
}

fn tri(t: texture_2d_array<f32>, s: sampler, layer: i32, at: Mapping, w: vec3<f32>, weight: f32) -> vec4<f32> {
    if (weight == 0.0) {
        return vec4<f32>(0.0);
    }
    var x = vec4<f32>(0.0);
    var y = vec4<f32>(0.0);
    var z = vec4<f32>(0.0);
    if (w.x > 0.0) {
        x = textureSampleGrad(t, s, at.p.yz, layer, at.dx.yz, at.dy.yz);
    }
    if (w.y > 0.0) {
        y = textureSampleGrad(t, s, at.p.xz, layer, at.dx.xz, at.dy.xz);
    }
    if (w.z > 0.0) {
        z = textureSampleGrad(t, s, at.p.xy, layer, at.dx.xy, at.dy.xy);
    }
    return x * w.x + y * w.y + z * w.z;
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
fn tri_normal(layer: i32, at: Mapping, w: vec3<f32>, n: vec3<f32>, weight: f32) -> vec3<f32> {
    if (weight == 0.0) {
        return vec3<f32>(0.0);
    }
    var tx = vec3<f32>(0.0);
    var ty = vec3<f32>(0.0);
    var tz = vec3<f32>(0.0);
    if (w.x > 0.0) {
        tx = textureSampleGrad(normal_maps, normal_sampler, at.p.yz, layer, at.dx.yz, at.dy.yz).xyz * 2.0 - 1.0;
        tx = vec3<f32>(tx.xy + n.zy, abs(tx.z) * n.x);
    }
    if (w.y > 0.0) {
        ty = textureSampleGrad(normal_maps, normal_sampler, at.p.xz, layer, at.dx.xz, at.dy.xz).xyz * 2.0 - 1.0;
        ty = vec3<f32>(ty.xy + n.xz, abs(ty.z) * n.y);
    }
    if (w.z > 0.0) {
        tz = textureSampleGrad(normal_maps, normal_sampler, at.p.xy, layer, at.dx.xy, at.dy.xy).xyz * 2.0 - 1.0;
        tz = vec3<f32>(tz.xy + n.xy, abs(tz.z) * n.z);
    }
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

// A town's AXES at a point, so a normal read on three planes can be
// turned back into the world: east and north there, squared to the local
// up, which are unit vectors and so cost no precision. WHERE the point is
// in that frame is not worked out here at all; it rides the vertex (the
// mapping position, `terrain.rs`'s `to_mesh`), because the number this
// used to form, `dot(up, east_t) * base` with `up` off a planet scale
// `rel`, stepped in centimetres at a thousand kilometres and made a
// staircase of every texture coordinate on a wall.
struct Local {
    n: vec3<f32>,
    to_world: mat3x3<f32>,
}

fn in_frame(f: i32, up: vec3<f32>, n: vec3<f32>) -> Local {
    var l: Local;
    l.n = n;
    l.to_world = mat3x3<f32>(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(0.0, 0.0, 1.0));
    if (f < 0) {
        return l;
    }
    let east_t = terrain.frames[f * 3 + 1].xyz;
    let east = normalize(east_t - up * dot(east_t, up));
    let north = cross(up, east);
    l.n = vec3<f32>(dot(n, east), dot(n, north), dot(n, up));
    l.to_world = mat3x3<f32>(east, north, up);
    return l;
}

// What is growing at a place, as a colour, blended rather than picked: a
// biome that snapped from one kind to the next would draw a line across
// the ground wherever the climate crossed a threshold, and what a biome
// has to do is fade.
fn biome_colour(temp: f32, wet: f32, over_sea: f32) -> vec3<f32> {
    let dry = 1.0 - smoothstep(ARID, WOODED, wet);
    let lush = smoothstep(WOODED, 0.82, wet);
    // Hot and dry is desert, cool and dry is savanna; wet is forest and
    // the middle is open grass.
    let hot = smoothstep(0.35, 0.6, temp);
    let parched = mix(C_SAVANNA, C_DESERT, hot);
    var c = mix(C_GRASS, C_FOREST, lush);
    c = mix(c, parched, dry);
    // Cold takes everything toward snow, and a shore toward sand.
    c = mix(c, C_SNOW, 1.0 - smoothstep(FREEZING, FREEZING + 0.16, temp));
    return mix(c, C_BEACH, 1.0 - smoothstep(SAND_TO, GRASS_FROM, over_sea));
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let n = normalize(in.world_normal);
    let rel = in.world_position.xyz - terrain.centre.xyz;
    let up = normalize(rel);
    // What this triangle is made of, and WHERE it is mapped from: the
    // vertex colour's red is the material, every corner of the triangle
    // the same so no driver's choice of provoking vertex can change it,
    // which is the mockup's hatched walls not happening twice, and its
    // other three are the mapping position `to_mesh` worked out in f64.
    // The ground's is its planet relative place modulo the tile and a
    // built thing's is its own town frame's; neither is ever a planet's
    // radius held in an f32, which is what made a two metre tile of grass
    // sample in 32 steps and the mip level come out as noise.
    var material = 0.0;
    var map = vec3<f32>(0.0);
#ifdef VERTEX_COLORS
    material = in.color.r;
    map = in.color.gba;
#endif
    let ground = is(material, M_TERRAIN);
    let slope = 1.0 - clamp(dot(n, up), 0.0, 1.0);
    let steep = smoothstep(0.34, 0.6, slope);
    let w_rock = steep * ground;
    // Sand along the shore and under the shallows, grass above it: the
    // mockup's band by height, measured off the sea's own radius. It rides
    // the vertex too (`to_mesh` again), because `length(rel)` is the
    // difference of two numbers near a million and a band a metre and a
    // half wide cannot be drawn in six centimetre steps.
    let over_sea = in.uv.x;
    // The climate rides the vertex too, worked out in f64 where the field
    // is: `temp` in the second uv lane and `wet` in the second uv set.
    let temp = in.uv.y;
    var wet = 0.5;
#ifdef VERTEX_UVS_B
    wet = in.uv_b.x;
#endif
    // Dry ground is SAND rather than grass whatever its height, which is
    // what makes a desert a desert on foot and not just a tan patch from
    // orbit. The shore's own band is the mockup's and rides over it.
    let arid = 1.0 - smoothstep(ARID, WOODED, wet);
    let shore = 1.0 - smoothstep(SAND_TO, GRASS_FROM, over_sea);
    let sand = max(shore, arid * smoothstep(0.35, 0.6, temp));
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
    let ground_position = map / terrain.params.x;
    let built_position = map / terrain.params.y;
    let pg = Mapping(ground_position, dpdx(ground_position), dpdy(ground_position));
    let pc = Mapping(built_position, dpdx(built_position), dpdy(built_position));
    let w = tri_weights(n);
    let local = in_frame(nearest_frame(up), up, n);
    let wc = tri_weights(local.n);
    let mapped = w_rock + w_sand + w_grass + w_conc + w_plate;
    var albedo = tri(albedo_maps, albedo_sampler, L_ROCK, pg, w, w_rock).rgb * w_rock
        + tri(albedo_maps, albedo_sampler, L_SAND, pg, w, w_sand).rgb * w_sand
        + tri(albedo_maps, albedo_sampler, L_GRASS, pg, w, w_grass).rgb * w_grass
        + tri(albedo_maps, albedo_sampler, L_CONCRETE, pc, wc, w_conc).rgb * w_conc
        + tri(albedo_maps, albedo_sampler, L_PLATE, pc, wc, w_plate).rgb * w_plate;
    // A street is the concrete set, darker, as paving is.
    albedo = albedo * (1.0 - street * 0.45);
    var orm = tri(orm_maps, orm_sampler, L_ROCK, pg, w, w_rock).rgb * w_rock
        + tri(orm_maps, orm_sampler, L_SAND, pg, w, w_sand).rgb * w_sand
        + tri(orm_maps, orm_sampler, L_GRASS, pg, w, w_grass).rgb * w_grass
        + tri(orm_maps, orm_sampler, L_CONCRETE, pc, wc, w_conc).rgb * w_conc
        + tri(orm_maps, orm_sampler, L_PLATE, pc, wc, w_plate).rgb * w_plate;
    let away = length(in.world_position.xyz - view.world_position);
    var nm = n;
    if (away < BUMP_FAR) {
        let built_n = local.to_world
            * (tri_normal(L_CONCRETE, pc, wc, local.n, w_conc) * w_conc
                + tri_normal(L_PLATE, pc, wc, local.n, w_plate) * w_plate);
        nm = tri_normal(L_ROCK, pg, w, n, w_rock) * w_rock
            + tri_normal(L_SAND, pg, w, n, w_sand) * w_sand
            + soften(tri_normal(L_GRASS, pg, w, n, w_grass), n, GRASS_BUMP) * w_grass
            + built_n;
        nm = normalize(nm * mapped + n * flat);
    }
    albedo = albedo * mapped
        + vec3<f32>(0.05, 0.08, 0.1) * glass
        + vec3<f32>(0.95, 0.92, 0.85) * lamp
        + vec3<f32>(0.9, 0.8, 0.6) * lit;
    orm = orm * mapped + vec3<f32>(1.0, 0.08, 0.0) * glass + vec3<f32>(1.0, 0.6, 0.0) * (lamp + lit);
    // Worn toward the surface's own normal with distance.
    nm = normalize(mix(nm, n, smoothstep(BUMP_NEAR, BUMP_FAR, away)));
    // Keep the baked texture's detail and brightness, and put the PLACE's
    // own colour on it: the biome where this body has one, and the body's
    // own hue where the planet definition asks for one.
    let luma = vec3<f32>(0.2126, 0.7152, 0.0722);
    let here = biome_colour(temp, wet, over_sea);
    let hue = mix(here, terrain.palette.rgb, terrain.palette.a);
    let strength = max(BIOME_TINT, terrain.palette.a);
    let coloured = hue * dot(albedo, luma) / max(dot(hue, luma), 0.001);
    albedo = mix(albedo, coloured, strength * ground);
    pbr_input.material.base_color = vec4<f32>(albedo, 1.0);
    pbr_input.material.perceptual_roughness = orm.g;
    pbr_input.material.metallic = orm.b;
    pbr_input.material.emissive = vec4<f32>(vec3<f32>(8.0, 7.4, 6.0) * lamp + vec3<f32>(3.0, 2.4, 1.5) * lit, 1.0);
    pbr_input.diffuse_occlusion = vec3<f32>(orm.r);
    pbr_input.N = nm;
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    // The air in front of this fragment, off the same march the dome is
    // drawn by (`freeport_core::atmos`, sampled at the horizon on the CPU
    // and handed down each frame), so a hillside fades into the sky
    // standing right above it rather than into a colour of its own.
    // The air POOLS in the low ground, so the fog is measured at the
    // MIDDLE of the view ray rather than at either end: a valley seen
    // from a ridge is hazy and the ridge seen from the valley is not.
    let eye_up = length(view.world_position - terrain.centre.xyz) - terrain.haze.z;
    // The same height over the sea the sand band uses, off the vertex.
    let here_up = over_sea;
    let mid_up = max((eye_up + here_up) * 0.5, 0.0);
    let pooled = 1.0 + (terrain.haze.y - 1.0) * exp(-mid_up / max(terrain.haze.x, 1.0));
    let in_the_way = 1.0 - exp(-away * terrain.fog.w * pooled);
    out.color = vec4<f32>(
        mix(out.color.rgb, terrain.fog.rgb * view.exposure, in_the_way),
        out.color.a,
    );
    return out;
}
