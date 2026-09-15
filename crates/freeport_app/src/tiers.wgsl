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
    mesh_view_bindings::view,
}
#import freeport::field

// What the tiers add to `terrain.wgsl`'s own bindings, which are the
// hundreds and which this file never reads: the planet, where the eye
// stands and how each tier is cut. It is binding 110 and nothing else,
// exactly `tiers::Tier`'s `#[uniform(110)]` fields in their order, because
// a struct here longer than the buffer there is a pipeline wgpu refuses
// with a size in bytes and no name (880 against 80, which was this file
// carrying the hundreds' fields as well).
struct Tier {
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
}

// The first lanes of `terrain.wgsl`'s own uniform, named here because the
// vertex stage needs the planet's centre out of it. A shader may name the
// START of a buffer and stop, so this is two lanes of a struct that is
// fifty five, and a struct LONGER than its buffer is a pipeline wgpu
// refuses outright with a size and no name, which is the guard on this
// being spelled in two files.
struct Ground {
    params: vec4<f32>,
    centre: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> ground: Ground;
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
    return dir * field::ground(p, dir) + ground.centre.xyz;
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
    // Row `r` of the sub lattice holds `2r + 1` triangles, so the rows
    // before it hold `r * r`: the row is the square root and the rest is
    // the place in it, an even one pointing the leaf's way and an odd one
    // the other. The square root is mended either way, because a float's
    // root of a square is not always the square's own root.
    var r = i32(sqrt(f32(s)));
    if ((r + 1) * (r + 1) <= s) {
        r = r + 1;
    }
    if (r * r > s) {
        r = r - 1;
    }
    let m = s - r * r;
    let j = m / 2;
    var pq = array<vec2<i32>, 3>(
        vec2<i32>(r, j),
        vec2<i32>(r + 1, j),
        vec2<i32>(r + 1, j + 1),
    );
    if (m - j * 2 == 1) {
        pq = array<vec2<i32>, 3>(
            vec2<i32>(r, j),
            vec2<i32>(r + 1, j + 1),
            vec2<i32>(r, j + 1),
        );
    }
    let p = planet();
    var at = array<vec3<f32>, 3>();
    for (var k = 0; k < 3; k = k + 1) {
        at[k] = place(p, leaf_dir(a, b, c, pq[k].x, pq[k].y, sub));
    }
    let up = normalize(at[0] + at[1] + at[2] - 3.0 * ground.centre.xyz);
    return emit(vertex, at[corner], face_normal(at[0], at[1], at[2], up));
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
        let world = d * top + ground.centre.xyz;
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
    return emit(vertex, at[which] + ground.centre.xyz, n);
}
