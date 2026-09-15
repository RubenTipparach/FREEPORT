// The two tiers' geometry, made in the VERTEX stage off nothing but
// `@builtin(vertex_index)`: the mesh both entry points are drawn with
// carries no data at all (`tiers::blank_mesh`), and every position here is
// computed from the index and the uniforms. `terrain.wgsl` is the fragment
// shader for both, so the tiers wear the same five sets the dual contoured
// ground does.
//
// Both tiers are WATERTIGHT by the same argument, and it is an argument
// about floats rather than about topology: a vertex two triangles share is
// computed from the SAME inputs in the same order on both sides, so it
// comes out on the same bits. A hex corner is `normalize(a + b + c)` of the
// three tiles round it, whichever of the three is asking, and floating
// point addition is commutative, so the three orders agree exactly. A
// point on a level of detail leaf's edge has nought weight on the third
// corner, and Planet-LOD's own rule is that two leaves sharing an edge
// share the WHOLE edge, so both cut it into the same `sub` pieces with the
// same weights.
//
// What the float costs: a position is `dir * ground + centre`, both terms
// about a planet's radius, so the render frame position carries about a
// millimetre of rounding at five kilometres. It is the same rounding on
// every vertex that shares a place, so it opens no crack, and it is under
// what a renderer can show, which is the floating origin's own argument.

#import bevy_pbr::{
    forward_io::VertexOutput,
    mesh_view_bindings::{view, globals},
}
#import freeport::field
#import freeport::water

// What the tiers add to `terrain.wgsl`'s own bindings, which are the
// hundreds and which this file never reads: the planet, where the eye
// stands and how each tier is cut. It is binding 110 and nothing else,
// exactly `tiers::Tier`'s `#[uniform(110)]` fields in their order, because
// a struct here longer than the buffer there is a pipeline wgpu refuses
// with a size in bytes and no name (880 against 80, which was this file
// carrying the hundreds' fields as well).
struct Tier {
    // The planet's centre in the render frame.
    centre: vec4<f32>,
    // Where the hex disc's middle is, as a UNIT direction, w: the cosine
    // of the angle the far tier's hole reaches. It is not the anchor
    // below: an anchor is a point in its FACE's plane and is shorter than
    // one, so it cannot be the axis of an angle.
    disc: vec4<f32>,
    // x: mean radius, y: the sea's radius, z: the relief, w: the lumps.
    shape: vec4<f32>,
    // x: octaves, y: seed, z: pieces a leaf's edge is cut into, w: the hex
    // grid's `n`, which nothing here reads and the log prints.
    counts: vec4<u32>,
    // The hex anchor: the eye's own tile as a direction, w: how many
    // leaves of the storage buffer are live.
    eye: vec4<f32>,
    // The hex lattice's first step as a vector off the anchor, w: how deep
    // a column's skirt hangs, metres.
    lat1: vec4<f32>,
    // Its second step, w: how many tiles the window reaches.
    lat2: vec4<f32>,
    // The sheet's own two lanes, `water::WaterExt`'s `wave` and `deep`, so
    // the sea tier lifts its vertices by the same swell the dual contoured
    // sheet does. Nought on a ground tier, which never asks.
    wave: vec4<f32>,
    deep: vec4<f32>,
}

// Binding 100 is NOT named here, and that is deliberate: it is
// `terrain.wgsl`'s uniform under the ground tiers and `water.wgsl`'s under
// the sea, two different structs at one number, and a vertex shader shared
// by all three cannot name either. The planet's centre this stage needs
// therefore rides the tier's own lane, which costs one vec4 written twice
// in `tiers::feed_tiers` and buys one vertex shader for three materials.
@group(#{MATERIAL_BIND_GROUP}) @binding(110) var<uniform> tier: Tier;
@group(#{MATERIAL_BIND_GROUP}) @binding(111) var<storage, read> leaves: array<vec4<f32>>;

// The only thing a tier's mesh carries is which vertex of the draw this
// is, in `position.x`. See `tiers::counted_mesh` for why it is not
// `@builtin(vertex_index)`.
struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
}

// The planet this tier draws, as `field.wgsl` wants it.
fn planet() -> field::Planet {
    return field::planet_of(tier.shape, tier.counts);
}

// A vertex that is clipped away whole: a triangle whose three corners are
// all this is never rasterised, which is how a thread with no tile and a
// leaf past the live count says nothing.
fn nowhere(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(0.0, 0.0, -1.0, 1.0);
    out.world_position = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    out.world_normal = vec3<f32>(0.0, 1.0, 0.0);
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif
    return out;
}

// A point of the ground in the render frame: the direction out, the field
// along it, and the planet's centre.
fn place(p: field::Planet, dir: vec3<f32>) -> vec3<f32> {
    return dir * field::ground(p, dir) + tier.centre.xyz;
}

// One vertex, given where it is, the normal its triangle stands on and
// which vertex of the draw it is.
fn emit(vertex: Vertex, world: vec3<f32>, normal: vec3<f32>) -> VertexOutput {
    var out: VertexOutput;
    out.world_position = vec4<f32>(world, 1.0);
    out.world_normal = normal;
    out.position = view.clip_from_world * vec4<f32>(world, 1.0);
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif
    return out;
}

// The outward normal of a triangle, which is the cross of two of its edges
// turned to face away from the planet. A sliver whose cross is nought
// takes the radial, because a normalised nought is every pixel after it
// wrong.
fn face_normal(a: vec3<f32>, b: vec3<f32>, c: vec3<f32>, up: vec3<f32>) -> vec3<f32> {
    let raw = cross(b - a, c - a);
    if (dot(raw, raw) < 1.0e-24) {
        return up;
    }
    let n = normalize(raw);
    if (dot(n, up) < 0.0) {
        return -n;
    }
    return n;
}

// ---------------------------------------------------------------- the far tier

// `freeport_core::lod` picks the leaves on the CPU and this cuts each of
// them into `sub * sub` triangles. A leaf's sub lattice is (p, q) with
// nought <= q <= p <= sub, weighting the leaf's corners (sub - p), (p - q)
// and q, so an edge of the leaf is a whole row of the lattice and two
// leaves sharing an edge cut it alike.
fn leaf_dir(a: vec3<f32>, b: vec3<f32>, c: vec3<f32>, p: i32, q: i32, sub: i32) -> vec3<f32> {
    let n = f32(sub);
    return normalize(a * (f32(sub - p) / n) + b * (f32(p - q) / n) + c * (f32(q) / n));
}

// Which three lattice points of a leaf's sub triangle `s` is. Row `r`
// holds `2r + 1` triangles, so the rows before it hold `r * r`: the row is
// the square root and the rest is the place in it, an even one pointing
// the leaf's way and an odd one the other. The root is mended either way,
// because a float's root of a square is not always the square's own root.
fn sub_triangle(s: i32, sub: i32) -> array<vec2<i32>, 3> {
    var r = i32(sqrt(f32(s)));
    if ((r + 1) * (r + 1) <= s) {
        r = r + 1;
    }
    if (r * r > s) {
        r = r - 1;
    }
    let m = s - r * r;
    let j = m / 2;
    if (m - j * 2 == 1) {
        return array<vec2<i32>, 3>(
            vec2<i32>(r, j),
            vec2<i32>(r + 1, j + 1),
            vec2<i32>(r, j + 1),
        );
    }
    return array<vec2<i32>, 3>(
        vec2<i32>(r, j),
        vec2<i32>(r + 1, j),
        vec2<i32>(r + 1, j + 1),
    );
}

@vertex
fn lod(vertex: Vertex) -> VertexOutput {
    let sub = max(i32(tier.counts.z), 1);
    let per = sub * sub;
    let id = i32(vertex.position.x);
    let leaf = id / (per * 3);
    let rest = id - leaf * per * 3;
    let s = rest / 3;
    let corner = rest - s * 3;
    if (leaf >= i32(tier.eye.w)) {
        return nowhere(vertex);
    }
    let a = leaves[u32(leaf * 3)].xyz;
    let b = leaves[u32(leaf * 3 + 1)].xyz;
    let c = leaves[u32(leaf * 3 + 2)].xyz;
    let pq = sub_triangle(s, sub);
    let p = planet();
    var dir = array<vec3<f32>, 3>();
    for (var k = 0; k < 3; k = k + 1) {
        dir[k] = leaf_dir(a, b, c, pq[k].x, pq[k].y, sub);
    }
    // The HOLE the hex disc stands in, cut here rather than by `select` on
    // the CPU: a sub triangle wholly inside the disc is not drawn, which
    // resolves the rim at the sub triangle's own size instead of a whole
    // leaf's, and leaves the CPU one `select` a frame to serve both the
    // ground and the sea.
    if (dot(dir[0], tier.disc.xyz) > tier.disc.w
        && dot(dir[1], tier.disc.xyz) > tier.disc.w
        && dot(dir[2], tier.disc.xyz) > tier.disc.w) {
        return nowhere(vertex);
    }
    // The normal is the FIELD's gradient at this vertex, not its
    // triangle's face: a leaf cut four ways is a metre of ground at the
    // feet and a kilometre at the horizon, and a face normal makes every
    // one of them a facet. The gradient is continuous, so the ground is
    // smooth at every distance and the sub triangles stop showing. It is
    // measured over the sub triangle's OWN size, which is the leaf's edge
    // over `sub`, so the normal is as coarse as the triangle carrying it
    // and no coarser.
    let step = length(a - b) * p.radius / f32(sub);
    let d = dir[corner];
    return emit(vertex, place(p, d), field::ground_normal(p, d, step));
}

// ---------------------------------------------------------------- the sea

// The sheet is ONE surface for both tiers, because a sea is flat whatever
// the ground under it is made of: it rides the same Planet-LOD leaves the
// far tier does, at the sea's radius instead of the ground's, with no hole
// cut in it, so it lies over the hex columns at a shore exactly as it lies
// over the far tier's triangles. A sub triangle whose three corners all
// stand on ground above the sea is not drawn, which is where a coastline
// comes from.
//
// It carries the water's own COLUMN in `uv.x`, which is what
// `water.wgsl` turns into the thickness Beer's law attenuates over: the
// field knows how deep the sea is here, and that is a better answer than a
// depth buffer, which measures to whatever is behind rather than to the
// floor underneath.
@vertex
fn sea(vertex: Vertex) -> VertexOutput {
    let sub = max(i32(tier.counts.z), 1);
    let per = sub * sub;
    let id = i32(vertex.position.x);
    let leaf = id / (per * 3);
    let rest = id - leaf * per * 3;
    let s = rest / 3;
    let corner = rest - s * 3;
    if (leaf >= i32(tier.eye.w)) {
        return nowhere(vertex);
    }
    let a = leaves[u32(leaf * 3)].xyz;
    let b = leaves[u32(leaf * 3 + 1)].xyz;
    let c = leaves[u32(leaf * 3 + 2)].xyz;
    let pq = sub_triangle(s, sub);
    let p = planet();
    var depth = array<f32, 3>();
    var dry = 0;
    var dir = array<vec3<f32>, 3>();
    for (var k = 0; k < 3; k = k + 1) {
        dir[k] = leaf_dir(a, b, c, pq[k].x, pq[k].y, sub);
        depth[k] = p.sea - field::ground(p, dir[k]);
        if (depth[k] <= 0.0) {
            dry = dry + 1;
        }
    }
    if (dry == 3) {
        return nowhere(vertex);
    }
    let d = dir[corner];
    let q = d * p.sea;
    // The swell is `water_lib.wgsl`'s, the same function the dual
    // contoured sheet's vertex stage calls, so the two seas are one sea.
    let lift = water::swell(q, tier.wave, tier.deep.w, globals.time);
    var out = emit(vertex, q + d * lift + tier.centre.xyz, d);
#ifdef VERTEX_UVS_A
    out.uv = vec2<f32>(max(depth[corner], 0.0), 0.0);
#endif
    return out;
}

// ---------------------------------------------------------------- the near tier

// A prism is sixteen triangles: four across the top of the hexagon and two
// down each of its six sides. Its forty eight vertices are laid out top
// first, so the index says which without a table.
const TOP_TRIS: i32 = 4;
const PRISM_TRIS: i32 = 16;
const PRISM_VERTS: i32 = 48;

// The six steps round a lattice point, in order, which is
// `freeport_core::hex::STEPS`.
fn hex_step(k: i32) -> vec2<f32> {
    var s = array<vec2<f32>, 6>(
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(-1.0, 1.0),
        vec2<f32>(-1.0, 0.0),
        vec2<f32>(0.0, -1.0),
        vec2<f32>(1.0, -1.0),
    );
    return s[k];
}

// A tile's middle, as a direction. The lattice is carried as two steps off
// the eye's own tile rather than as an address, which is
// `freeport_core::hex::Grid::basis`: over a disc of a few dozen metres on
// a planet of kilometres the icosahedron's own lattice IS this affine one
// to a few millimetres, and the twelve corners where it is not are what
// `a_window_of_steps_is_the_grids_own_tiles` measures.
fn tile_dir(uv: vec2<f32>) -> vec3<f32> {
    return normalize(tier.eye.xyz + tier.lat1.xyz * uv.x + tier.lat2.xyz * uv.y);
}

@vertex
fn hex(vertex: Vertex) -> VertexOutput {
    let span = i32(tier.lat2.w);
    let wide = span * 2 + 1;
    let id = i32(vertex.position.x);
    let tile = id / PRISM_VERTS;
    let k = id - tile * PRISM_VERTS;
    let u = tile % wide - span;
    let v = tile / wide - span;
    // The disc inside the square window, in the lattice's own norm: a
    // corner of the square is a fifth again as far as its middle edge, and
    // drawing it would put a hexagon of ground inside a disc of sky.
    if (u * u + u * v + v * v > span * span) {
        return nowhere(vertex);
    }
    let p = planet();
    let uv = vec2<f32>(f32(u), f32(v));
    let mid = tile_dir(uv);
    let top = field::ground(p, mid);
    let foot = top - tier.lat1.w;
    // The hexagon's corners: each is the middle of the three tiles round
    // it, which is the dual of the lattice and the one construction every
    // tile sharing that corner agrees on.
    var corner = array<vec3<f32>, 6>();
    var here = array<vec3<f32>, 6>();
    for (var i = 0; i < 6; i = i + 1) {
        here[i] = tile_dir(uv + hex_step(i));
    }
    for (var i = 0; i < 6; i = i + 1) {
        corner[i] = normalize(mid + here[i] + here[(i + 1) % 6]);
    }
    let tri = k / 3;
    let which = k - tri * 3;
    if (tri < TOP_TRIS) {
        // The top, as a fan from the first corner.
        var idx = array<i32, 3>(0, tri + 1, tri + 2);
        let d = corner[idx[which]];
        let world = d * top + tier.centre.xyz;
        return emit(vertex, world, mid);
    }
    // A side: the quad from one corner to the next, down to the skirt.
    let side = (tri - TOP_TRIS) / 2;
    let half = (tri - TOP_TRIS) - side * 2;
    let a = corner[side];
    let b = corner[(side + 1) % 6];
    var at = array<vec3<f32>, 3>(a * top, b * top, b * foot);
    if (half == 1) {
        at = array<vec3<f32>, 3>(a * top, b * foot, a * foot);
    }
    // A side faces the edge it stands on, across the tile's own up: the
    // cross of its two edges says the same thing and says nothing at all
    // when the two columns either side of it are the same height, which is
    // most of a plain.
    let edge = normalize(a + b);
    let across = edge - mid * dot(edge, mid);
    var n = mid;
    if (dot(across, across) > 1.0e-12) {
        n = normalize(across);
    }
    return emit(vertex, at[which] + tier.centre.xyz, n);
}
