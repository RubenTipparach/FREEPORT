//! Dual contouring, a chunk at a time, against the levels round it.
//!
//! One vertex per SURFACE in a cell, at the least squares point of that
//! surface's edge crossings (positions found by bisection on the field,
//! normals the field's gradient there), and one polygon per crossing edge,
//! round the cells that share it. Which crossings are one surface is what
//! the marching cubes case already says: its triangles for the cell's
//! corner signs, joined where they share an edge (`components`).
//!
//! Every level is one lattice (`lattice.rs`), so the rule at a join is the
//! octree one: a MINIMAL edge is an edge of the finest level round it, and
//! the polygon on it joins the vertices of the LEAVES round it, which are
//! this chunk's cells on one side of a join and a coarser neighbour's cell
//! on the other. A chunk skips every edge with a finer chunk's cell round
//! it, because that chunk owns it, and among chunks of one level the lowest
//! owns an edge they share, so every edge has one polygon. Nothing dives
//! under anything and nothing is sunk: the seam is polygons whose corners
//! are cells of two sizes, every mesh edge is shared by exactly two of
//! them, and the mesh is closed by construction, which `audit.rs` measures
//! rather than assumes. The mockup's skirt was the answer before this one,
//! and the picture that retired it was a line of dark slits along the join:
//! where the coarse chord stood above the fine surface a grazing line of
//! sight went under it into the unmeshed rock, and a rim sunk BELOW the
//! coarse mesh can only open that further.
//!
//! A leaf's vertex is a function of the field and the leaf alone, computed
//! the same way by every chunk that needs it from the same fine points, so
//! two chunks place a shared vertex on the same bits and the audit's weld
//! finds it once. A coarse cell the fine surface crosses on a face without
//! crossing any of its own edges has no vertex of its own; it is given one
//! at the least squares point of the fine crossings on its faces, which
//! every fine chunk beside it computes alike, so the seam closes on it.

use crate::field::Density;
use crate::lattice::{air, ChunkId, Lattice, Levels, CH, MARGIN};
use crate::march::{CORNER, EDGE_AT};
use crate::qef::{qef, Crossing};
use crate::tables::TRI_TABLE;
use glam::DVec3;
use std::collections::HashMap;
use std::sync::OnceLock;

/// One chunk's triangles, positions in metres from the chunk's corner.
#[derive(Clone, Debug, Default)]
pub struct DcMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// Per vertex, the level of the cell that owns it.
    pub levels: Vec<u8>,
    pub indices: Vec<u32>,
    /// Per triangle, what it is made of: the field's material a hand inside
    /// its middle.
    pub materials: Vec<u8>,
    /// Polygons whose corners are cells of two levels.
    pub seams: usize,
    /// Corners a polygon wanted from a coarse cell that had no vertex and
    /// no fine crossing on its faces to make one from, which cannot happen.
    pub missing: usize,
}

impl DcMesh {
    /// How many triangles.
    pub fn triangles(&self) -> usize {
        self.indices.len() / 3
    }
}

/// The four cells round an edge along each axis, as offsets from the edge's
/// lattice point across the two other axes, in cyclic order.
const AROUND: [[[i64; 3]; 4]; 3] = [
    [[0, 0, 0], [0, -1, 0], [0, -1, -1], [0, 0, -1]],
    [[0, 0, 0], [-1, 0, 0], [-1, 0, -1], [0, 0, -1]],
    [[0, 0, 0], [-1, 0, 0], [-1, -1, 0], [0, -1, 0]],
];

/// The edge number that edge has in each of those four cells.
const EDGE_OF: [[usize; 4]; 3] = [[0, 2, 6, 4], [3, 1, 5, 7], [8, 9, 10, 11]];

/// The edges on each face of a cell: x low, x high, y low, y high, z low,
/// z high.
const FACE_EDGES: [[usize; 4]; 6] = [
    [3, 7, 8, 11],
    [1, 5, 9, 10],
    [0, 4, 8, 9],
    [2, 6, 10, 11],
    [0, 1, 2, 3],
    [4, 5, 6, 7],
];

/// Bisection steps for a crossing: a cell over two hundred and fifty, a
/// millimetre under the feet.
const BISECT: usize = 8;

/// Points a side the field is looked at before a chunk is sampled: with
/// the field's slope bound, five a side rule most chunks empty for a
/// hundred and twenty five samples rather than nine thousand.
const PEEK: i64 = 5;

/// How far inside a triangle's middle its material is read, in cells: a
/// hand, so a thin skin on a thick host reads as the host and a plate
/// thicker than a cell reads as itself.
const HAND: f64 = 0.5;

/// Points along a chunk's side that are sampled: the chunk and its margin.
pub const SAMPLE_STRIDE: i64 = CH + 2 * MARGIN + 1;
const STRIDE: i64 = SAMPLE_STRIDE;

/// For a configuration, the surface each crossed edge is on (-1 if not
/// crossed): the marching cubes triangles joined where they share an edge.
pub fn components(config: usize) -> &'static [i8; 12] {
    static TABLE: OnceLock<Vec<[i8; 12]>> = OnceLock::new();
    &TABLE.get_or_init(|| (0..256).map(components_of).collect())[config]
}

fn components_of(config: usize) -> [i8; 12] {
    let mut parent: [usize; 12] = std::array::from_fn(|e| e);
    fn root(p: &mut [usize; 12], e: usize) -> usize {
        let mut r = e;
        while p[r] != r {
            r = p[r];
        }
        let mut c = e;
        while p[c] != r {
            let next = p[c];
            p[c] = r;
            c = next;
        }
        r
    }
    let mut crossed = [false; 12];
    for tri in TRI_TABLE[config].chunks(3) {
        if tri[0] < 0 {
            break;
        }
        let (a, b, c) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        crossed[a] = true;
        crossed[b] = true;
        crossed[c] = true;
        let ra = root(&mut parent, a);
        let rb = root(&mut parent, b);
        parent[rb] = ra;
        let rc = root(&mut parent, c);
        let ra = root(&mut parent, a);
        parent[rc] = ra;
    }
    let mut comp = [-1i8; 12];
    let mut names: Vec<usize> = Vec::new();
    for e in 0..12 {
        if !crossed[e] {
            continue;
        }
        let r = root(&mut parent, e);
        let k = match names.iter().position(|&n| n == r) {
            Some(k) => k,
            None => {
                names.push(r);
                names.len() - 1
            }
        };
        comp[e] = k as i8;
    }
    comp
}

/// The two axes across `axis`, in the order that keeps the frame right
/// handed.
fn perpendicular(axis: usize) -> (usize, usize) {
    match axis {
        0 => (1, 2),
        1 => (2, 0),
        _ => (0, 1),
    }
}

/// A leaf's vertices: the surface each edge is on and a mesh vertex per
/// surface.
#[derive(Clone)]
struct Verts {
    comp: [i8; 12],
    verts: Vec<u32>,
}

/// A leaf: a cell of the chunk's level, or of the level above at a seam,
/// in its own level's cells.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Leaf {
    level: u8,
    cell: [i64; 3],
}

/// What holds a cell of this chunk's level.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Finer,
    Same(ChunkId),
    Coarser,
}

struct Chunk<'a> {
    field: &'a dyn Density,
    lat: &'a Lattice,
    levels: &'a dyn Levels,
    id: ChunkId,
    /// The chunk's first point, in its level's cells.
    c0: [i64; 3],
    /// Samples at the chunk's level over the chunk and `MARGIN` round it,
    /// taken as they are asked for; a margin sample nothing asks for is
    /// never taken.
    samples: Vec<f32>,
    origin: DVec3,
    crossings: Vec<Crossing>,
    crossing_of: HashMap<([i64; 3], u8, i64), u32>,
    leaves: HashMap<Leaf, Verts>,
    mesh: DcMesh,
}

/// Contour chunk `id`: every polygon it owns, its own cells' and the seams
/// to coarser neighbours'.
pub fn contour(field: &dyn Density, lat: &Lattice, id: ChunkId, levels: &dyn Levels) -> DcMesh {
    build(field, lat, id, levels, Vec::new())
}

/// Contour with precomputed lattice densities, x fastest, including `MARGIN`
/// on every side. GPU samples must have the reference field's signs. Crossings
/// and seam vertices still use the reference field in f64. Invalid grids fall
/// back to ordinary contouring rather than leaving a hole in the terrain.
pub fn contour_sampled(
    field: &dyn Density,
    lat: &Lattice,
    id: ChunkId,
    levels: &dyn Levels,
    samples: Vec<f32>,
) -> DcMesh {
    if samples.len() != (STRIDE * STRIDE * STRIDE) as usize
        || samples.iter().any(|v| !v.is_finite())
    {
        return contour(field, lat, id, levels);
    }
    // With no crossed edge, this grid cannot produce a polygon, including
    // the coarser cells in the apron. No CPU noise evaluations are needed.
    if samples.iter().all(|&v| air(v) == air(samples[0])) {
        return DcMesh::default();
    }
    build(field, lat, id, levels, samples)
}

fn build(
    field: &dyn Density,
    lat: &Lattice,
    id: ChunkId,
    levels: &dyn Levels,
    samples: Vec<f32>,
) -> DcMesh {
    let s = id.scale();
    let f0 = id.f0();
    let mut chunk = Chunk {
        field,
        lat,
        levels,
        id,
        c0: [f0[0] / s, f0[1] / s, f0[2] / s],
        samples,
        origin: lat.point(f0),
        crossings: Vec::new(),
        crossing_of: HashMap::new(),
        leaves: HashMap::new(),
        mesh: DcMesh::default(),
    };
    if chunk.samples.is_empty() && chunk.empty() {
        return chunk.mesh;
    }
    chunk.edges();
    chunk.mesh
}

impl Chunk<'_> {
    /// A point of the chunk's level as a fine index.
    fn fine(&self, c: [i64; 3]) -> [i64; 3] {
        let s = self.id.scale();
        [c[0] * s, c[1] * s, c[2] * s]
    }

    /// The chunk's cell, metres.
    fn cell(&self) -> f64 {
        self.lat.cell(self.id.level)
    }

    /// Whether the chunk can be ruled all rock or all air from `PEEK`
    /// points a side and the field's slope bound: every point of the chunk
    /// is within half a step's diagonal of one of them, so a field that
    /// stays farther from nought than the slope can carry over that
    /// distance keeps its sign everywhere in between. A field with no bound
    /// is never ruled.
    fn empty(&self) -> bool {
        let slope = self.field.slope();
        if !slope.is_finite() {
            return false;
        }
        let step = CH as f64 / (PEEK - 1) as f64;
        let reach = slope * step * self.cell() * 3f64.sqrt() * 0.5;
        let mut sign: Option<bool> = None;
        for k in 0..PEEK {
            for j in 0..PEEK {
                for i in 0..PEEK {
                    let p = self.lat.point(self.fine(self.c0))
                        + DVec3::new(i as f64, j as f64, k as f64) * step * self.cell();
                    let v = self.field.at(p);
                    if v.abs() <= reach {
                        return false;
                    }
                    let rock = !air(v as f32);
                    if sign.is_some_and(|s| s != rock) {
                        return false;
                    }
                    sign = Some(rock);
                }
            }
        }
        true
    }

    /// The sample at a point of the chunk's level, within its margin, taken
    /// the first time it is asked for.
    fn at(&mut self, c: [i64; 3]) -> f32 {
        let i = c[0] - self.c0[0] + MARGIN;
        let j = c[1] - self.c0[1] + MARGIN;
        let k = c[2] - self.c0[2] + MARGIN;
        debug_assert!(
            (0..STRIDE).contains(&i) && (0..STRIDE).contains(&j) && (0..STRIDE).contains(&k),
            "a sample outside the chunk's margin"
        );
        if self.samples.is_empty() {
            self.samples = vec![f32::NAN; (STRIDE * STRIDE * STRIDE) as usize];
        }
        let slot = ((k * STRIDE + j) * STRIDE + i) as usize;
        if self.samples[slot].is_nan() {
            self.samples[slot] = self.field.at(self.lat.point(self.fine(c))) as f32;
        }
        self.samples[slot]
    }

    /// What holds a cell of the chunk's level.
    fn kind(&self, cell: [i64; 3]) -> Kind {
        let f = self.fine(cell);
        match self.levels.level_at(f) {
            Some(l) if l < self.id.level => Kind::Finer,
            Some(l) if l > self.id.level => Kind::Coarser,
            _ => Kind::Same(ChunkId::holding(self.id.level, f)),
        }
    }

    /// The gradient of the field at a world point, by central differences
    /// `e` apart. It climbs INTO the rock.
    fn gradient(&self, p: DVec3, e: f64) -> DVec3 {
        let d = |axis: DVec3| self.field.at(p + axis * e) - self.field.at(p - axis * e);
        DVec3::new(d(DVec3::X), d(DVec3::Y), d(DVec3::Z))
    }

    /// The crossing on the edge from fine point `a` along `axis` for `step`
    /// fine cells, made once. Its normal is the gradient a fifth of the
    /// EDGE's length apart, never the chunk's cell: a coarser cell's vertex
    /// is solved from the same crossings by the coarse chunk and by the
    /// fine one beside it, and a normal that depended on who asked put the
    /// shared vertex in two places.
    fn crossing(&mut self, a: [i64; 3], axis: usize, step: i64) -> u32 {
        let key = (a, axis as u8, step);
        if let Some(&id) = self.crossing_of.get(&key) {
            return id;
        }
        let mut b = a;
        b[axis] += step;
        let (mut lo, mut hi) = (self.lat.point(a), self.lat.point(b));
        let lo_air = air(self.field.at(lo) as f32);
        for _ in 0..BISECT {
            let mid = (lo + hi) * 0.5;
            if air(self.field.at(mid) as f32) == lo_air {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let p = (lo + hi) * 0.5;
        let n = -self
            .gradient(p, step as f64 * self.lat.fine * 0.2)
            .normalize_or(DVec3::Y);
        let id = self.crossings.len() as u32;
        self.crossings.push(Crossing { p, n });
        self.crossing_of.insert(key, id);
        id
    }

    /// The vertices of a leaf, made once: a surface per component of its
    /// configuration, each at the least squares point of its crossings; a
    /// coarser leaf with none is given one from the fine crossings on its
    /// faces.
    fn verts(&mut self, leaf: Leaf) -> Verts {
        if let Some(v) = self.leaves.get(&leaf) {
            return v.clone();
        }
        let step = 1i64 << (leaf.level - self.id.level);
        let base = [
            leaf.cell[0] * step,
            leaf.cell[1] * step,
            leaf.cell[2] * step,
        ];
        let mut config = 0usize;
        for (c, d) in CORNER.iter().enumerate() {
            let p = [
                base[0] + d[0] as i64 * step,
                base[1] + d[1] as i64 * step,
                base[2] + d[2] as i64 * step,
            ];
            if air(self.at(p)) {
                config |= 1 << c;
            }
        }
        let comp = *components(config);
        let count = comp.iter().max().map(|&m| m + 1).unwrap_or(0).max(0) as usize;
        let lo = self.lat.point(self.fine(base));
        let hi = lo + DVec3::splat(step as f64 * self.cell());
        let mut verts = Vec::with_capacity(count.max(1));
        for k in 0..count as i8 {
            let mut xs = Vec::new();
            for (e, (at, axis)) in EDGE_AT.iter().enumerate() {
                if comp[e] != k {
                    continue;
                }
                let a = [
                    base[0] + at[0] as i64 * step,
                    base[1] + at[1] as i64 * step,
                    base[2] + at[2] as i64 * step,
                ];
                let id = self.crossing(self.fine(a), *axis, step * self.id.scale());
                xs.push(self.crossings[id as usize]);
            }
            let (p, n) = qef(&xs, lo, hi);
            verts.push(self.push_vertex(p, n, leaf.level));
        }
        if count == 0 && step > 1 {
            let xs = self.face_crossings(base, step);
            if !xs.is_empty() {
                let (p, n) = qef(&xs, lo, hi);
                verts.push(self.push_vertex(p, n, leaf.level));
            }
        }
        let v = Verts { comp, verts };
        self.leaves.insert(leaf, v.clone());
        v
    }

    /// Every crossing on the edges of this chunk's level lying in the faces
    /// of the coarser cell at `base`, `step` cells a side, in one fixed
    /// order, so every chunk beside that cell gathers the same list.
    fn face_crossings(&mut self, base: [i64; 3], step: i64) -> Vec<Crossing> {
        let mut ids: Vec<u32> = Vec::new();
        for w in 0..3 {
            let (u, v) = perpendicular(w);
            for side in [0, step] {
                for along in [u, v] {
                    let across = if along == u { v } else { u };
                    for a in 0..step {
                        for b in 0..=step {
                            let mut p = base;
                            p[w] += side;
                            p[along] += a;
                            p[across] += b;
                            let mut q = p;
                            q[along] += 1;
                            if air(self.at(p)) == air(self.at(q)) {
                                continue;
                            }
                            let id = self.crossing(self.fine(p), along, self.id.scale());
                            if !ids.contains(&id) {
                                ids.push(id);
                            }
                        }
                    }
                }
            }
        }
        ids.iter().map(|&id| self.crossings[id as usize]).collect()
    }

    fn push_vertex(&mut self, p: DVec3, n: DVec3, level: u8) -> u32 {
        let local = p - self.origin;
        self.mesh
            .positions
            .push([local.x as f32, local.y as f32, local.z as f32]);
        self.mesh.normals.push([n.x as f32, n.y as f32, n.z as f32]);
        self.mesh.levels.push(level);
        (self.mesh.positions.len() - 1) as u32
    }

    /// Every edge of the chunk's level touching its cells, along each axis.
    fn edges(&mut self) {
        for k in 0..=CH {
            for j in 0..=CH {
                for i in 0..=CH {
                    for axis in 0..3 {
                        self.edge([self.c0[0] + i, self.c0[1] + j, self.c0[2] + k], axis);
                    }
                }
            }
        }
    }

    /// The polygon on one edge, if the chunk owns it and it crosses: the
    /// cells of this level round it give their vertices and a coarser
    /// neighbour's cell gives the one the seam joins to.
    fn edge(&mut self, p: [i64; 3], axis: usize) {
        let mut q = p;
        q[axis] += 1;
        let mut owner: Option<ChunkId> = None;
        let mut kinds = [Kind::Coarser; 4];
        for (slot, d) in AROUND[axis].iter().enumerate() {
            let cell = [p[0] + d[0], p[1] + d[1], p[2] + d[2]];
            let kind = self.kind(cell);
            match kind {
                Kind::Finer => return,
                Kind::Same(id) => owner = Some(owner.map_or(id, |o: ChunkId| o.min(id))),
                Kind::Coarser => {}
            }
            kinds[slot] = kind;
        }
        if owner != Some(self.id) {
            return;
        }
        if air(self.at(p)) == air(self.at(q)) {
            return;
        }
        let xp = {
            let id = self.crossing(self.fine(p), axis, self.id.scale());
            self.crossings[id as usize].p
        };
        let mut corners = [None; 4];
        let mut seam = false;
        for (slot, d) in AROUND[axis].iter().enumerate() {
            let cell = [p[0] + d[0], p[1] + d[1], p[2] + d[2]];
            corners[slot] = match kinds[slot] {
                Kind::Same(_) => self
                    .verts(Leaf {
                        level: self.id.level,
                        cell,
                    })
                    .vertex(EDGE_OF[axis][slot]),
                Kind::Coarser => {
                    seam = true;
                    let coarse = cell.map(|v| v.div_euclid(2));
                    self.seam_vertex(coarse, p, axis, xp)
                }
                Kind::Finer => None,
            };
        }
        self.emit(corners, seam);
    }

    /// The coarse cell's vertex an edge on its boundary joins to: the
    /// surface of the coarse edge the fine one lies on, else the one surface
    /// crossing the face it lies in, else the nearest of the cell's.
    fn seam_vertex(&mut self, c: [i64; 3], f: [i64; 3], axis: usize, xp: DVec3) -> Option<u32> {
        let v = self.verts(Leaf {
            level: self.id.level + 1,
            cell: c,
        });
        if v.verts.is_empty() {
            self.mesh.missing += 1;
            return None;
        }
        let s = 2i64;
        let base = [c[0] * s, c[1] * s, c[2] * s];
        let side = |u: usize| -> Option<usize> {
            if u == axis {
                None
            } else if f[u] == base[u] {
                Some(2 * u)
            } else if f[u] == base[u] + s {
                Some(2 * u + 1)
            } else {
                None
            }
        };
        let faces: Vec<usize> = (0..3).filter_map(side).collect();
        if faces.len() == 2 {
            let on_both =
                |e: &usize| FACE_EDGES[faces[0]].contains(e) && FACE_EDGES[faces[1]].contains(e);
            if let Some(e) = (0..12).find(|e| EDGE_AT[*e].1 == axis && on_both(e)) {
                if let Some(vi) = v.vertex(e) {
                    return Some(vi);
                }
            }
        }
        let mut candidates: Vec<i8> = Vec::new();
        for face in &faces {
            for e in FACE_EDGES[*face] {
                let k = v.comp[e];
                if k >= 0 && !candidates.contains(&k) {
                    candidates.push(k);
                }
            }
        }
        if candidates.len() == 1 {
            return Some(v.verts[candidates[0] as usize]);
        }
        let pool: Vec<u32> = if candidates.is_empty() {
            v.verts.clone()
        } else {
            candidates.iter().map(|&k| v.verts[k as usize]).collect()
        };
        pool.into_iter().min_by(|&a, &b| {
            let da = (self.world(a) - xp).length_squared();
            let db = (self.world(b) - xp).length_squared();
            da.total_cmp(&db)
        })
    }

    fn world(&self, vi: u32) -> DVec3 {
        let p = self.mesh.positions[vi as usize];
        self.origin + DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64)
    }

    /// One polygon: the distinct corners in cyclic order, a triangle or a
    /// quad split along the diagonal that folds less, each triangle wound to
    /// face out of the rock by the field's gradient at its middle.
    fn emit(&mut self, corners: [Option<u32>; 4], seam: bool) {
        let mut v: Vec<u32> = Vec::with_capacity(4);
        for c in corners.into_iter().flatten() {
            if v.last() != Some(&c) {
                v.push(c);
            }
        }
        while v.len() > 1 && v.first() == v.last() {
            v.pop();
        }
        if v.len() < 3 || (1..v.len()).any(|i| v[..i].contains(&v[i])) {
            return;
        }
        if seam {
            self.mesh.seams += 1;
        }
        if v.len() == 3 {
            self.triangle(v[0], v[1], v[2]);
            return;
        }
        let fold = |a: u32, b: u32, c: u32, d: u32| {
            let n1 = (self.world(b) - self.world(a)).cross(self.world(c) - self.world(a));
            let n2 = (self.world(c) - self.world(a)).cross(self.world(d) - self.world(a));
            n1.normalize_or_zero().dot(n2.normalize_or_zero())
        };
        if fold(v[0], v[1], v[2], v[3]) >= fold(v[1], v[2], v[3], v[0]) {
            self.triangle(v[0], v[1], v[2]);
            self.triangle(v[0], v[2], v[3]);
        } else {
            self.triangle(v[1], v[2], v[3]);
            self.triangle(v[1], v[3], v[0]);
        }
    }

    /// One triangle, wound to face out of the rock, made of whatever the
    /// field says a hand inside its middle: the material rides the triangle
    /// flat, so concrete meets rock on a line and never as a blend.
    fn triangle(&mut self, a: u32, b: u32, c: u32) {
        let (pa, pb, pc) = (self.world(a), self.world(b), self.world(c));
        let face = (pb - pa).cross(pc - pa);
        let mid = (pa + pb + pc) / 3.0;
        let g = self.gradient(mid, self.cell() * 0.2);
        if face.dot(g) <= 0.0 {
            self.mesh.indices.extend_from_slice(&[a, b, c]);
        } else {
            self.mesh.indices.extend_from_slice(&[a, c, b]);
        }
        let inside = mid + g.normalize_or_zero() * (self.cell() * HAND);
        self.mesh.materials.push(self.field.material(inside));
    }
}

impl Verts {
    /// The vertex of the surface edge `e` is on, if the edge is crossed.
    fn vertex(&self, e: usize) -> Option<u32> {
        let k = self.comp[e];
        (k >= 0).then(|| self.verts[k as usize])
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod lod_tests;
