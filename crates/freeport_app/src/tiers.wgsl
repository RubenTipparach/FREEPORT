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
#import freeport::frame
#import freeport::water

// What the tiers add to `terrain.wgsl`'s own bindings, which are the
// hundreds and which this file never reads: the planet, where the eye
// stands and how each tier is cut. It is binding 110 and nothing else,
// exactly `tiers::Tier`'s `#[uniform(110)]` fields in their order, because
// a struct here longer than the buffer there is a pipeline wgpu refuses
// with a size in bytes and no name (880 against 80, which was this file
// carrying the hundreds' fields as well).
struct Tier {
    // The planet's centre in the render frame, w: how many leaves of the
    // storage buffer are live.
    centre: vec4<f32>,
    // The ANCHOR: one unit direction, the eye's own tile's, that every
    // vertex in this draw is an offset from (`freeport::frame`). In w,
    // the far tier's hole, as the SQUARED CHORD of the angle it reaches
    // rather than its cosine, because a cosine near one is a number an
    // f32 cannot tell from one.
    disc: vec4<f32>,
    // Where the anchor's own ground stands in the RENDER frame, worked
    // out in f64 on the CPU: `anchor * radius + centre`, which is the one
    // large subtraction, done once and where it can be done exactly.
    // Every vertex is this plus a small accurate offset.
    base: vec4<f32>,
    // x: mean radius, y: the sea's radius, z: the relief, w: the lumps.
    shape: vec4<f32>,
    // x: octaves, y: seed, z: pieces a leaf's edge is cut into, w: the hex
    // grid's `n`, which nothing here reads and the log prints.
    counts: vec4<u32>,
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
// What every tile of the hex window carries: how far it has been RAISED in
// x, metres, and what it is MADE OF in y, one of `freeport_core::field`'s
// numbers. The same `tile` number the entry points already work out from
// the vertex index is the index of this, so a built tile is never looked
// up by address. `feed::send_raised` fills it.
@group(#{MATERIAL_BIND_GROUP}) @binding(112) var<storage, read> raised: array<vec2<f32>>;

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

// Where one vertex is: an OFFSET from the anchor, and the direction it
// works out to. Nothing here ever holds a planet sized number.
struct Spot {
    // `dir - anchor`, small and accurate.
    off: vec3<f32>,
    // The direction itself, which is only ever used as a direction.
    dir: vec3<f32>,
}

// The spot a small step off the anchor reaches.
fn spot(step: vec3<f32>) -> Spot {
    var s: Spot;
    s.off = frame::unit_offset(tier.disc.xyz, step);
    s.dir = tier.disc.xyz + s.off;
    return s;
}

// A point of the ground in the render frame, at `lift` metres over the
// mean radius on top of the relief. The radius multiplies the OFFSET and
// never the direction, which is the whole of the trick: `base` already
// carries `anchor * radius + centre`, worked out in f64.
fn place(p: field::Planet, s: Spot, up: f32) -> vec3<f32> {
    return tier.base.xyz + s.off * p.radius + s.dir * up;
}

// One vertex, given where it is, the normal its triangle stands on and
// which vertex of the draw it is.
fn emit(
    vertex: Vertex,
    world: vec3<f32>,
    normal: vec3<f32>,
    over_sea: f32,
    material: f32,
) -> VertexOutput {
    var out: VertexOutput;
    out.world_position = vec4<f32>(world, 1.0);
    out.world_normal = normal;
    out.position = view.clip_from_world * vec4<f32>(world, 1.0);
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif
    // How far this vertex stands over the sea, carried down rather than
    // worked out again from the fragment's own position. `terrain.wgsl`
    // picks sand or grass off a band 1.5 m wide, and the fragment's
    // position is INTERPOLATED across the triangle, which on a far leaf
    // is a CHORD: at four radii up the whole planet is 44 leaves, a sub
    // triangle is 75 km across, and a 75 km chord on a 1,000 km sphere
    // sags 703 m below it. The sea is 400 m down, so the middle of every
    // sub triangle read as below the sea and the continents came out
    // sand while the sea itself, which is clipped on the true field at
    // the corners, stayed where it belonged. A height interpolated
    // between three corners has no sphere in it to sag.
    // And what it is MADE OF, in the other lane of the same varying. A
    // tier's mesh is a vertex COUNT and nothing else, so there is no
    // vertex colour to put it in the way a dual contoured chunk does; and
    // every vertex of one of these triangles carries the same number, so
    // the interpolation across the triangle is a constant and `terrain`'s
    // own half unit test never sees a value between two materials.
#ifdef VERTEX_UVS_A
    out.uv = vec2<f32>(material, over_sea);
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
// The corners in the buffer are OFFSETS from the anchor, so a weighted
// sum of them with weights that add to one is the offset of the point:
// `wa*(N+ra) + wb*(N+rb) + wc*(N+rc) = N + (wa*ra + wb*rb + wc*rc)`. The
// large part cancels on the CPU, where it can.
fn leaf_spot(a: vec3<f32>, b: vec3<f32>, c: vec3<f32>, p: i32, q: i32, sub: i32) -> Spot {
    let n = f32(sub);
    return spot(a * (f32(sub - p) / n) + b * (f32(p - q) / n) + c * (f32(q) / n));
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
    if (leaf >= i32(tier.centre.w)) {
        return nowhere(vertex);
    }
    let a = leaves[u32(leaf * 3)].xyz;
    let b = leaves[u32(leaf * 3 + 1)].xyz;
    let c = leaves[u32(leaf * 3 + 2)].xyz;
    let pq = sub_triangle(s, sub);
    let p = planet();
    var at = array<Spot, 3>();
    for (var k = 0; k < 3; k = k + 1) {
        at[k] = leaf_spot(a, b, c, pq[k].x, pq[k].y, sub);
    }
    // The HOLE the hex disc stands in, cut here rather than by `select` on
    // the CPU: a sub triangle wholly inside the disc is not drawn, which
    // resolves the rim at the sub triangle's own size instead of a whole
    // leaf's, and leaves the CPU one `select` a frame to serve both the
    // ground and the sea.
    // The hole, measured as the squared chord off the anchor: a cosine
    // near one is a number an f32 cannot tell from one, and a chord is
    // the same test written where the precision is.
    if (dot(at[0].off, at[0].off) < tier.disc.w
        && dot(at[1].off, at[1].off) < tier.disc.w
        && dot(at[2].off, at[2].off) < tier.disc.w) {
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
    let here = at[corner];
    let up = field::surface(p, here.dir, here.off);
    return emit(
        vertex,
        place(p, here, up),
        field::ground_normal(p, here.dir, here.off, step),
        up - (p.sea - p.radius),
        // The far tier draws the relief and nothing built: a town past
        // the hex disc is its levelled plateau, since a column raised on
        // a tile is only ever drawn where the tiles are.
        0.0,
    );
}

// ---------------------------------------------------------------- the sea

// The sheet PAST the disc: it rides the same Planet-LOD leaves the far
// ground tier does, at the sea's radius instead of the ground's, and with
// the same hole cut in it, because inside the disc the sea is columns.
// A sub triangle whose three corners all stand on ground above the sea is
// not drawn, which is where a coastline comes from.
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
    if (leaf >= i32(tier.centre.w)) {
        return nowhere(vertex);
    }
    let a = leaves[u32(leaf * 3)].xyz;
    let b = leaves[u32(leaf * 3 + 1)].xyz;
    let c = leaves[u32(leaf * 3 + 2)].xyz;
    let pq = sub_triangle(s, sub);
    let p = planet();
    var depth = array<f32, 3>();
    var dry = 0;
    var at = array<Spot, 3>();
    // The sea's own height over the mean radius, which is what a vertex
    // of the sheet is lifted by instead of the relief.
    let sea_up = p.sea - p.radius;
    for (var k = 0; k < 3; k = k + 1) {
        at[k] = leaf_spot(a, b, c, pq[k].x, pq[k].y, sub);
        depth[k] = sea_up - field::surface(p, at[k].dir, at[k].off);
        if (depth[k] <= 0.0) {
            dry = dry + 1;
        }
    }
    if (dry == 3) {
        return nowhere(vertex);
    }
    // The hole the columns stand in, the ground tier's own test on the
    // same lane: a sub triangle wholly inside the disc is the hex sea's.
    if (dot(at[0].off, at[0].off) < tier.disc.w
        && dot(at[1].off, at[1].off) < tier.disc.w
        && dot(at[2].off, at[2].off) < tier.disc.w) {
        return nowhere(vertex);
    }
    let here = at[corner];
    // The swell is `water_lib.wgsl`'s, the same function the dual
    // contoured sheet's vertex stage calls, so the two seas are one sea.
    // It is asked in the planet's own frame, which for a point on the
    // sheet is its direction times the sea's radius.
    //
    // And it is FADED OUT toward the hole, because the sea inside the disc
    // is columns and a column is flat: the swell is up to a metre of
    // geometry, so a sheet carrying it met the flat tops at the rim with a
    // step in it, and the step caught the sun as a white band right round
    // the disc. Nought at the hole's own rim and full at twice its angle,
    // which is well inside where a leaf is wider than a wave anyway.
    let far = dot(here.off, here.off);
    let swell = smoothstep(tier.disc.w, tier.disc.w * 4.0, far);
    let lift = water::swell(here.dir * p.sea, tier.wave, tier.deep.w, globals.time) * swell;
    var out = emit(
        vertex,
        tier.base.xyz + here.off * p.radius + here.dir * (sea_up + lift),
        here.dir,
        0.0,
        // The sea wears `water.wgsl` and not `terrain.wgsl`, so its own
        // uv.x is the water's COLUMN and is written over this below.
        0.0,
    );
#ifdef VERTEX_UVS_A
    // The sheet's own lane: the water's COLUMN under this vertex, which
    // `water.wgsl` reads as `WATER_COLUMN`.
    out.uv.x = max(depth[corner], 0.0);
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

// A tile's middle, as a spot off the anchor. The lattice is carried as
// two steps rather than as an address, which is
// `freeport_core::hex::Grid::basis`: over a disc of a few dozen metres on
// a planet of kilometres the icosahedron's own lattice IS this affine one
// to a few millimetres, and the twelve corners where it is not are what
// `a_window_of_steps_is_the_grids_own_tiles` measures. The two steps come
// down already divided by the anchor point's own length, so what is added
// here is a small number and the anchor is the unit direction every other
// tier measures from.
fn tile_spot(uv: vec2<f32>) -> Spot {
    return spot(tier.lat1.xyz * uv.x + tier.lat2.xyz * uv.y);
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
    let middle = tile_spot(uv);
    // The relief at the tile's middle plus what has been built there,
    // which is `columns::Columns::top` asked on the GPU: the walker's
    // ground and the picture are one number.
    let built = raised[u32(tile)];
    let top = field::surface(p, middle.dir, middle.off) + built.x;
    let foot = top - tier.lat1.w;
    // The hexagon's corners: each is the middle of the three tiles round
    // it, which is the dual of the lattice and the one construction every
    // tile sharing that corner agrees on. They are kept as OFFSETS, since
    // an offset is what a position is built out of here: the average of
    // three offsets is the offset of their average, and the anchor part
    // of it never has to be formed.
    var corner = array<vec3<f32>, 6>();
    var round_it = array<vec3<f32>, 6>();
    for (var i = 0; i < 6; i = i + 1) {
        round_it[i] = tile_spot(uv + hex_step(i)).off;
    }
    for (var i = 0; i < 6; i = i + 1) {
        corner[i] = (middle.off + round_it[i] + round_it[(i + 1) % 6]) / 3.0;
    }
    let tri = k / 3;
    let which = k - tri * 3;
    if (tri < TOP_TRIS) {
        // The top, as a fan from the first corner.
        var idx = array<i32, 3>(0, tri + 1, tri + 2);
        let off = corner[idx[which]];
        let world = tier.base.xyz + off * p.radius + middle.dir * top;
        return emit(vertex, world, middle.dir, top - (p.sea - p.radius), built.y);
    }
    // A side: the quad from one corner to the next, down to the skirt.
    let side = (tri - TOP_TRIS) / 2;
    let half = (tri - TOP_TRIS) - side * 2;
    let a = corner[side];
    let b = corner[(side + 1) % 6];
    // A corner's position is the base plus its offset times the radius,
    // and its height is the column's top or its foot along the tile's own
    // direction.
    let pa = tier.base.xyz + a * p.radius;
    let pb = tier.base.xyz + b * p.radius;
    let up = middle.dir;
    var at = array<vec3<f32>, 3>(pa + up * top, pb + up * top, pb + up * foot);
    if (half == 1) {
        at = array<vec3<f32>, 3>(pa + up * top, pb + up * foot, pa + up * foot);
    }
    // A side faces the edge it stands on, across the tile's own up: the
    // cross of its two edges says the same thing and says nothing at all
    // when the two columns either side of it are the same height, which is
    // most of a plain.
    let edge = normalize(tier.disc.xyz + (a + b) * 0.5);
    let across = edge - up * dot(edge, up);
    var n = up;
    if (dot(across, across) > 1.0e-12) {
        n = normalize(across);
    }
    return emit(vertex, at[which], n, top - (p.sea - p.radius), built.y);
}

// ------------------------------------------------------- the sea, in tiles

// The sea INSIDE the disc, as columns: the same prism the `hex` entry
// point makes, on the same tiles, but topped at the sea's level rather
// than at the ground's and drawn only where the ground is under it. That
// is the owner's ask and tenebris's water: a shore is a wall of water down
// to the beach rather than a sheet fading into it, and a puddle in a
// hollow is the tiles of that hollow and no others.
//
// The skirt is the ground tier's own. A water column's side is only ever
// SEEN where the tile beside it has no water in it, and there the skirt
// hangs into the beach, which is what the ground draws over.
@vertex
fn hexsea(vertex: Vertex) -> VertexOutput {
    let span = i32(tier.lat2.w);
    let wide = span * 2 + 1;
    let id = i32(vertex.position.x);
    let tile = id / PRISM_VERTS;
    let k = id - tile * PRISM_VERTS;
    let u = tile % wide - span;
    let v = tile / wide - span;
    if (u * u + u * v + v * v > span * span) {
        return nowhere(vertex);
    }
    let p = planet();
    let uv = vec2<f32>(f32(u), f32(v));
    let middle = tile_spot(uv);
    // The sea's height over the mean radius, and how deep the water on
    // this tile is. A tile whose ground stands over the sea has none.
    let sea_up = p.sea - p.radius;
    let column = sea_up - (field::surface(p, middle.dir, middle.off) + raised[u32(tile)].x);
    if (column <= 0.0) {
        return nowhere(vertex);
    }
    // FLAT at the sea's own level, with no swell in the geometry at all.
    // The sheet's swell is metres of wavelength and a tile is a metre, so
    // asking it at the tile's MIDDLE aliases it: every column came out at
    // its own height and the sea read as a field of cracked slabs rather
    // than as water. A column of water is flat and the ripples are the
    // shader's, which is what tenebris's water is; the swell stays on the
    // sheet past the disc, where a leaf is wider than a wave.
    let top = sea_up;
    let foot = top - tier.lat1.w;
    var corner = array<vec3<f32>, 6>();
    var round_it = array<vec3<f32>, 6>();
    for (var i = 0; i < 6; i = i + 1) {
        round_it[i] = tile_spot(uv + hex_step(i)).off;
    }
    for (var i = 0; i < 6; i = i + 1) {
        corner[i] = (middle.off + round_it[i] + round_it[(i + 1) % 6]) / 3.0;
    }
    let tri = k / 3;
    let which = k - tri * 3;
    var out: VertexOutput;
    if (tri < TOP_TRIS) {
        var idx = array<i32, 3>(0, tri + 1, tri + 2);
        let off = corner[idx[which]];
        out = emit(
            vertex,
            tier.base.xyz + off * p.radius + middle.dir * top,
            middle.dir,
            0.0,
            0.0,
        );
    } else {
        let side = (tri - TOP_TRIS) / 2;
        let half = (tri - TOP_TRIS) - side * 2;
        let a = corner[side];
        let b = corner[(side + 1) % 6];
        let pa = tier.base.xyz + a * p.radius;
        let pb = tier.base.xyz + b * p.radius;
        let up = middle.dir;
        var at = array<vec3<f32>, 3>(pa + up * top, pb + up * top, pb + up * foot);
        if (half == 1) {
            at = array<vec3<f32>, 3>(pa + up * top, pb + up * foot, pa + up * foot);
        }
        let edge = normalize(tier.disc.xyz + (a + b) * 0.5);
        let across = edge - up * dot(edge, up);
        var n = up;
        if (dot(across, across) > 1.0e-12) {
            n = normalize(across);
        }
        out = emit(vertex, at[which], n, 0.0, 0.0);
    }
#ifdef VERTEX_UVS_A
    // The water's own COLUMN, which on a tile is one number for the whole
    // prism rather than a corner's: a column of water is as deep as its
    // tile and not as deep as the slope under its corner.
    out.uv.x = column;
#endif
    return out;
}
