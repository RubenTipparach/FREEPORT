//! Dual contouring across the two levels of the lattice, a chunk at a time.
//!
//! One vertex per SURFACE in a cell, at the least squares point of that
//! surface's edge crossings (positions found by bisection on the field,
//! normals the field's gradient there), and one polygon per crossing edge,
//! round the cells that share it. Which crossings are one surface is what
//! the marching cubes case already says: its triangles for the cell's
//! corner signs, joined where they share an edge (`components`).
//!
//! The two levels are one lattice (`lattice.rs`), so the rule is the octree
//! one: a MINIMAL edge is a fine edge wherever a subdivided cell is round
//! it and a coarse edge everywhere else, and the polygon on it joins the
//! vertices of the LEAVES round it, which are fine cells on one side of a
//! join and the coarse cell on the other. Nothing dives under anything and
//! nothing is sunk: the seam is polygons whose corners are cells of two
//! sizes, every mesh edge is shared by exactly two of them, and the mesh is
//! closed by construction, which `audit.rs` measures rather than assumes.
//! The mockup's skirt was the answer before this one, and the picture that
//! retired it was a line of dark slits along the join: where the coarse
//! chord stood above the fine surface a grazing line of sight went under
//! it into the unmeshed rock, and a rim sunk BELOW the coarse mesh can only
//! open that further.
//!
//! An edge on a chunk border has one owner, the lowest chunk among the
//! subdivided cells round it for a fine edge and among the cells round it
//! for a coarse edge, so every edge has one polygon. A leaf's vertex is a
//! function of the field and the leaf alone, computed the same way by every
//! chunk that needs it, so two chunks place a shared vertex on the same
//! bits and the audit's weld finds it once.

use crate::field::Density;
use crate::lattice::{air, Lattice, CH};
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
    /// Per vertex, the level of the cell that owns it: 0 coarse, 1 fine.
    pub levels: Vec<u8>,
    pub indices: Vec<u32>,
    /// Per triangle, what it is made of: the field's material a hand inside
    /// its middle, `field::TERRAIN` or `field::CONCRETE`.
    pub materials: Vec<u8>,
    /// Polygons whose corners are cells of both levels.
    pub seams: usize,
    /// Corners a polygon wanted from a coarse cell with no vertex at all,
    /// which the mask's growth is meant to make impossible.
    pub missing: usize,
}

impl DcMesh {
    /// How many triangles.
    pub fn triangles(&self) -> usize {
        self.indices.len() / 3
    }
}

/// The field sampled at every coarse lattice point, once for the lattice.
pub struct Coarse {
    n: usize,
    values: Vec<f32>,
}

impl Coarse {
    /// Sample `field` at the coarse points of `lat`, through the fine point
    /// under each so the bits are the ones a fine chunk sees.
    pub fn sample(field: &dyn Density, lat: &Lattice) -> Coarse {
        let n = lat.n;
        let s = lat.sub as i64;
        let mut values = Vec::with_capacity((n + 1).pow(3));
        for k in 0..=n as i64 {
            for j in 0..=n as i64 {
                for i in 0..=n as i64 {
                    values.push(field.at(lat.point([i * s, j * s, k * s])) as f32);
                }
            }
        }
        Coarse { n, values }
    }

    /// The sample at coarse point `c`, each coordinate in 0..=n.
    pub fn at(&self, c: [i64; 3]) -> f32 {
        let m = self.n + 1;
        self.values[(c[2] as usize * m + c[1] as usize) * m + c[0] as usize]
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

/// Bisection steps for a crossing: a cell over four thousand.
const BISECT: usize = 12;

/// How far inside a triangle's middle its material is read, in fine cells:
/// a hand, so a thin skin on a thick host reads as the host and a plate
/// thicker than a cell reads as itself.
const HAND: f64 = 0.5;

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

/// A leaf's vertices: the surface each edge is on and a mesh vertex per
/// surface.
#[derive(Clone)]
struct Verts {
    comp: [i8; 12],
    verts: Vec<u32>,
}

/// A leaf of the lattice: a coarse cell (level 0) or a fine cell (level 1).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Leaf {
    level: u8,
    cell: [i64; 3],
}

struct Chunk<'a> {
    field: &'a dyn Density,
    lat: &'a Lattice,
    coarse: &'a Coarse,
    me: usize,
    /// The chunk's first fine point and its extent in fine cells.
    f0: [i64; 3],
    span: i64,
    /// Fine samples over the chunk and a cell of margin, or none if no cell
    /// of the chunk is subdivided.
    fine: Vec<f32>,
    origin: DVec3,
    crossings: Vec<Crossing>,
    crossing_of: HashMap<([i64; 3], u8, i64), u32>,
    leaves: HashMap<Leaf, Verts>,
    mesh: DcMesh,
}

/// Contour chunk `b` of `lat`: every polygon the chunk owns, at both levels.
pub fn contour(field: &dyn Density, lat: &Lattice, coarse: &Coarse, b: [usize; 3]) -> DcMesh {
    let span = (CH * lat.sub) as i64;
    let f0 = [b[0] as i64 * span, b[1] as i64 * span, b[2] as i64 * span];
    let mut chunk = Chunk {
        field,
        lat,
        coarse,
        me: lat.chunk_index(b),
        f0,
        span,
        fine: Vec::new(),
        origin: lat.point(f0),
        crossings: Vec::new(),
        crossing_of: HashMap::new(),
        leaves: HashMap::new(),
        mesh: DcMesh::default(),
    };
    if lat.chunk_has_fine(b) {
        chunk.sample_fine();
    }
    chunk.coarse_edges();
    if !chunk.fine.is_empty() {
        chunk.fine_edges();
    }
    chunk.mesh
}

impl Chunk<'_> {
    fn fine_stride(&self) -> i64 {
        self.span + 3
    }

    fn sample_fine(&mut self) {
        let s = self.fine_stride();
        let mut values = Vec::with_capacity((s * s * s) as usize);
        for k in -1..=self.span + 1 {
            for j in -1..=self.span + 1 {
                for i in -1..=self.span + 1 {
                    let f = [self.f0[0] + i, self.f0[1] + j, self.f0[2] + k];
                    values.push(self.field.at(self.lat.point(f)) as f32);
                }
            }
        }
        self.fine = values;
    }

    /// A fine sample, at a fine point within the chunk's margin.
    fn fine_at(&self, f: [i64; 3]) -> f32 {
        let s = self.fine_stride();
        let (i, j, k) = (
            f[0] - self.f0[0] + 1,
            f[1] - self.f0[1] + 1,
            f[2] - self.f0[2] + 1,
        );
        self.fine[((k * s + j) * s + i) as usize]
    }

    /// A sample at a lattice point of a leaf's level: fine points from the
    /// chunk's own samples, coarse points from the lattice's.
    fn sample(&self, level: u8, f: [i64; 3]) -> f32 {
        if level == 1 {
            self.fine_at(f)
        } else {
            let s = self.lat.sub as i64;
            self.coarse.at([f[0] / s, f[1] / s, f[2] / s])
        }
    }

    /// The gradient of the field at a world point, by central differences a
    /// fifth of a fine cell apart. It climbs INTO the rock.
    fn gradient(&self, p: DVec3) -> DVec3 {
        let e = self.lat.fine * 0.2;
        let d = |axis: DVec3| self.field.at(p + axis * e) - self.field.at(p - axis * e);
        DVec3::new(d(DVec3::X), d(DVec3::Y), d(DVec3::Z))
    }

    /// The crossing on the edge from fine point `a` along `axis` for `step`
    /// fine cells (one for a fine edge, `sub` for a coarse one), made once.
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
        let n = -self.gradient(p).normalize_or(DVec3::Y);
        let id = self.crossings.len() as u32;
        self.crossings.push(Crossing { p, n });
        self.crossing_of.insert(key, id);
        id
    }

    /// The vertices of a leaf, made once: a surface per component of its
    /// configuration, each at the least squares point of its crossings.
    fn verts(&mut self, leaf: Leaf) -> Verts {
        if let Some(v) = self.leaves.get(&leaf) {
            return v.clone();
        }
        let step = if leaf.level == 1 {
            1
        } else {
            self.lat.sub as i64
        };
        let base = [
            leaf.cell[0] * step,
            leaf.cell[1] * step,
            leaf.cell[2] * step,
        ];
        let mut config = 0usize;
        for (c, d) in CORNER.iter().enumerate() {
            let f = [
                base[0] + d[0] as i64 * step,
                base[1] + d[1] as i64 * step,
                base[2] + d[2] as i64 * step,
            ];
            if air(self.sample(leaf.level, f)) {
                config |= 1 << c;
            }
        }
        let comp = *components(config);
        let count = comp.iter().max().map(|&m| m + 1).unwrap_or(0).max(0) as usize;
        let lo = self.lat.point(base);
        let hi = lo + DVec3::splat(step as f64 * self.lat.fine);
        let mut verts = Vec::with_capacity(count);
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
                let id = self.crossing(a, *axis, step);
                xs.push(self.crossings[id as usize]);
            }
            let (p, n) = qef(&xs, lo, hi);
            verts.push(self.push_vertex(p, n, leaf.level));
        }
        let v = Verts { comp, verts };
        self.leaves.insert(leaf, v.clone());
        v
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

    /// The polygons on the chunk's coarse edges: every crossing coarse edge
    /// with no subdivided cell round it that this chunk owns.
    fn coarse_edges(&mut self) {
        let s = self.lat.sub as i64;
        let n = self.lat.n as i64;
        let c0 = [self.f0[0] / s, self.f0[1] / s, self.f0[2] / s];
        let ch = CH as i64;
        for k in c0[2]..=(c0[2] + ch).min(n) {
            for j in c0[1]..=(c0[1] + ch).min(n) {
                for i in c0[0]..=(c0[0] + ch).min(n) {
                    for axis in 0..3 {
                        self.coarse_edge([i, j, k], axis);
                    }
                }
            }
        }
    }

    fn coarse_edge(&mut self, p: [i64; 3], axis: usize) {
        let mut q = p;
        q[axis] += 1;
        if q[axis] > self.lat.n as i64 {
            return;
        }
        if air(self.coarse.at(p)) == air(self.coarse.at(q)) {
            return;
        }
        let mut owner = usize::MAX;
        let mut cells = [None; 4];
        for (slot, d) in AROUND[axis].iter().enumerate() {
            let c = [p[0] + d[0], p[1] + d[1], p[2] + d[2]];
            if self.lat.key(c).is_none() {
                continue;
            }
            if self.lat.masked(c) {
                return;
            }
            owner = owner.min(self.lat.chunk_of(c));
            cells[slot] = Some(c);
        }
        if owner != self.me {
            return;
        }
        let mut corners = [None; 4];
        for (slot, c) in cells.iter().enumerate() {
            if let Some(c) = c {
                let v = self.verts(Leaf { level: 0, cell: *c });
                corners[slot] = v.vertex(EDGE_OF[axis][slot]);
            }
        }
        self.emit(corners, false);
    }

    /// The polygons on the chunk's fine edges: every crossing fine edge with
    /// a subdivided cell round it that this chunk owns, with a coarse cell's
    /// vertex standing in for the unsubdivided side of a join.
    fn fine_edges(&mut self) {
        for k in self.f0[2]..=self.f0[2] + self.span {
            for j in self.f0[1]..=self.f0[1] + self.span {
                for i in self.f0[0]..=self.f0[0] + self.span {
                    for axis in 0..3 {
                        self.fine_edge([i, j, k], axis);
                    }
                }
            }
        }
    }

    fn fine_edge(&mut self, f: [i64; 3], axis: usize) {
        let mut g = f;
        g[axis] += 1;
        let mut owner = usize::MAX;
        let mut any_fine = false;
        for d in AROUND[axis].iter() {
            let fc = [f[0] + d[0], f[1] + d[1], f[2] + d[2]];
            let c = self.lat.coarse_of(fc);
            if self.lat.masked(c) {
                any_fine = true;
                owner = owner.min(self.lat.chunk_of(c));
            }
        }
        if !any_fine || owner != self.me {
            return;
        }
        if air(self.fine_at(f)) == air(self.fine_at(g)) {
            return;
        }
        let xp = {
            let id = self.crossing(f, axis, 1);
            self.crossings[id as usize].p
        };
        let mut corners = [None; 4];
        let mut seam = false;
        for (slot, d) in AROUND[axis].iter().enumerate() {
            let fc = [f[0] + d[0], f[1] + d[1], f[2] + d[2]];
            let c = self.lat.coarse_of(fc);
            if self.lat.masked(c) {
                let v = self.verts(Leaf { level: 1, cell: fc });
                corners[slot] = v.vertex(EDGE_OF[axis][slot]);
            } else if self.lat.key(c).is_some() {
                seam = true;
                corners[slot] = self.seam_vertex(c, f, axis, xp);
            }
        }
        self.emit(corners, seam);
    }

    /// The coarse cell's vertex a fine edge on its boundary joins to: the
    /// surface of the coarse edge the fine one lies on, else the one surface
    /// crossing the face it lies in, else the nearest of the cell's.
    fn seam_vertex(&mut self, c: [i64; 3], f: [i64; 3], axis: usize, xp: DVec3) -> Option<u32> {
        let v = self.verts(Leaf { level: 0, cell: c });
        if v.verts.is_empty() {
            self.mesh.missing += 1;
            return None;
        }
        let s = self.lat.sub as i64;
        let base = [c[0] * s, c[1] * s, c[2] * s];
        // On which face of the cell, along each axis across the edge: low,
        // high, or neither.
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
            // On a coarse edge: the one along `axis` on both those faces.
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
        let g = self.gradient(mid);
        if face.dot(g) <= 0.0 {
            self.mesh.indices.extend_from_slice(&[a, b, c]);
        } else {
            self.mesh.indices.extend_from_slice(&[a, c, b]);
        }
        let inside = mid + g.normalize_or_zero() * (self.lat.fine * HAND);
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
mod tests {
    use super::*;
    use crate::audit::audit;
    use crate::field::{Block, Built, Planet, Sphere, CONCRETE, TERRAIN};
    use crate::tables::EDGE_TABLE;

    /// Every chunk of the lattice with any triangles in it.
    fn contour_all(field: &dyn Density, lat: &Lattice) -> Vec<(DVec3, DcMesh)> {
        let coarse = Coarse::sample(field, lat);
        let cn = lat.chunks();
        let mut out = Vec::new();
        for bz in 0..cn {
            for by in 0..cn {
                for bx in 0..cn {
                    let m = contour(field, lat, &coarse, [bx, by, bz]);
                    if m.triangles() > 0 {
                        let span = (CH * lat.sub) as i64;
                        let corner =
                            lat.point([bx as i64 * span, by as i64 * span, bz as i64 * span]);
                        out.push((corner, m));
                    }
                }
            }
        }
        out
    }

    #[test]
    fn every_component_row_partitions_the_crossed_edges() {
        for config in 0..256 {
            let comp = components(config);
            for e in 0..12 {
                assert_eq!(
                    comp[e] >= 0,
                    EDGE_TABLE[config] & (1 << e) != 0,
                    "config {config} edge {e}"
                );
            }
            let count = comp.iter().max().copied().unwrap_or(-1) + 1;
            for k in 0..count {
                assert!(comp.contains(&k), "config {config} skips surface {k}");
            }
        }
        // Two opposite corners of air: two surfaces, six edges each.
        let two = components(0b0100_0001);
        assert_eq!(two.iter().max(), Some(&1));
        assert_eq!(two.iter().filter(|&&k| k == 0).count(), 3);
    }

    #[test]
    fn a_sphere_contours_to_a_closed_shell_at_one_level() {
        let ball = Sphere { radius: 6.0 };
        let lat = Lattice::new(DVec3::splat(-8.0), 0.5, 4, 32);
        let chunks = contour_all(&ball, &lat);
        let a = audit(&ball, &chunks);
        assert_eq!(a.open, 0, "{a:?}");
        assert_eq!(a.facing_in, 0, "{a:?}");
        assert_eq!(a.seams, 0);
        let want = 4.0 * std::f64::consts::PI * 36.0;
        assert!(
            (a.area - want).abs() / want < 0.03,
            "area {} against {want}",
            a.area
        );
        assert!(a.triangles > 1000);
    }

    #[test]
    fn a_sphere_contours_to_a_closed_shell_across_two_levels() {
        let ball = Sphere { radius: 6.0 };
        let mut lat = Lattice::new(DVec3::splat(-8.0), 0.5, 4, 32);
        // A blob of fine cells over the top of the ball, crossing chunk
        // borders, so the join has faces at every orientation.
        lat.subdivide_near(DVec3::new(0.3, 6.0, 0.2), 2.6);
        assert_eq!(lat.grow(&ball), 0);
        let chunks = contour_all(&ball, &lat);
        let a = audit(&ball, &chunks);
        assert_eq!(a.open, 0, "{a:?}");
        assert_eq!(a.facing_in, 0, "{a:?}");
        assert!(a.seams > 100, "{a:?}");
        assert_eq!(a.missing, 0);
        let both = chunks
            .iter()
            .map(|(_, m)| m.levels.iter().filter(|&&l| l == 1).count())
            .sum::<usize>();
        assert!(both > 100, "fine vertices {both}");
        let want = 4.0 * std::f64::consts::PI * 36.0;
        assert!(
            (a.area - want).abs() / want < 0.03,
            "area {} against {want}",
            a.area
        );
    }

    /// A small planet with a slab and a wall built on its top, the lattice
    /// subdivided under them, as the app draws it.
    fn built_planet() -> (Planet, Vec<Block>, Lattice) {
        let planet = Planet {
            radius: 20.0,
            relief: 2.0,
            lumps: 3.0,
            octaves: 4,
            overhang: 0.6,
            ledge: 3.0,
            seed: 7,
        };
        let top = planet.at(DVec3::new(0.0, 20.0, 0.0)) + 20.0;
        // Half extents along the block's own axes: east, north, up.
        let slab = Block {
            centre: DVec3::new(0.0, top - 0.1, 0.0),
            half: DVec3::new(3.0, 3.0, 0.25),
            axes: [DVec3::X, DVec3::Z, DVec3::Y],
        };
        let wall = Block {
            centre: DVec3::new(2.0, top + 1.0, 0.0),
            half: DVec3::new(0.2, 2.5, 1.1),
            axes: [DVec3::X, DVec3::Z, DVec3::Y],
        };
        let mut lat = Lattice::new(DVec3::splat(-24.0), 1.0, 4, 48);
        lat.subdivide_near(DVec3::new(0.0, top, 0.0), 5.0);
        (planet, vec![slab, wall], lat)
    }

    #[test]
    fn a_planet_with_a_slab_and_a_wall_is_closed_and_the_slab_is_flat() {
        let (planet, blocks, mut lat) = built_planet();
        let slab = blocks[0].clone();
        let built = Built {
            ground: &planet,
            blocks,
        };
        let grown = lat.grow(&built);
        let chunks = contour_all(&built, &lat);
        let a = audit(&built, &chunks);
        assert_eq!(a.open, 0, "{a:?} after growing {grown}");
        assert_eq!(a.missing, 0, "{a:?}");
        assert!(a.facing_in <= 2, "{a:?}");
        assert!(a.seams > 100, "{a:?}");
        // Every fine vertex over the slab's top, well inside its edges and
        // clear of the wall, facing up and standing in what the ground alone
        // calls air, lies on the top's plane: a box face is a plane at any
        // lattice, which is what dual contouring is for.
        let top = slab.centre.y + slab.half.z;
        let (mut on_top, mut worst) = (0, 0.0f64);
        for (corner, m) in &chunks {
            for ((p, n), level) in m.positions.iter().zip(&m.normals).zip(&m.levels) {
                let w = *corner + DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64);
                let over = w.x.abs() < slab.half.x - 0.4 && w.z.abs() < slab.half.y - 0.4;
                let up = n[1] > 0.99;
                if *level == 1
                    && over
                    && up
                    && (w.y - top).abs() < 0.3
                    && (w.x - 2.0).abs() > 0.6
                    && planet.at(w) < -0.05
                {
                    on_top += 1;
                    worst = worst.max((w.y - top).abs());
                }
            }
        }
        assert!(on_top > 50, "vertices over the slab {on_top}");
        assert!(worst < 0.002, "a slab vertex {worst} m off the plane");
        // And the triangles over the slab are concrete, the ground's terrain.
        let (mut concrete, mut terrain) = (0, 0);
        for (corner, m) in &chunks {
            for (t, mat) in m.indices.chunks(3).zip(&m.materials) {
                let mid = t
                    .iter()
                    .map(|&i| DVec3::from(m.positions[i as usize].map(f64::from)))
                    .sum::<DVec3>()
                    / 3.0
                    + *corner;
                let over = mid.x.abs() < 2.0
                    && mid.z.abs() < 2.0
                    && (mid.y - top).abs() < 0.1
                    && (mid.x - 2.0).abs() > 0.6;
                if over {
                    assert_eq!(*mat, CONCRETE, "a slab triangle at {mid}");
                    concrete += 1;
                } else if mid.y < top - 3.0 {
                    assert_eq!(*mat, TERRAIN, "a ground triangle at {mid}");
                    terrain += 1;
                }
            }
        }
        assert!(
            concrete > 50 && terrain > 1000,
            "{concrete} concrete, {terrain} terrain"
        );
    }

    /// A box face laid exactly on a lattice plane puts its crease on a
    /// lattice edge, and the two cells either side of that edge both solve
    /// to the same point on the crease: two vertices in one place, which is
    /// a pinch. The rule that keeps a build off the lattice is half a fine
    /// cell of offset between the build grid and the lattice's corner, and
    /// this holds it: the same pad on a lattice it coincides with pinches,
    /// and on one offset by half a fine cell it is clean.
    #[test]
    fn a_face_on_a_lattice_plane_pinches_and_half_a_cell_of_offset_does_not() {
        let ground = Sphere { radius: 5.0 };
        let pad = Block {
            centre: DVec3::new(0.0, 5.0, 0.0),
            half: DVec3::new(1.0, 1.0, 0.3),
            axes: [DVec3::X, DVec3::Z, DVec3::Y],
        };
        let built = Built {
            ground: &ground,
            blocks: vec![pad],
        };
        let mut pinched = Vec::new();
        for offset in [0.0, 0.125] {
            let mut lat = Lattice::new(DVec3::splat(-8.0 + offset), 0.5, 2, 32);
            lat.subdivide_near(DVec3::new(0.0, 5.0, 0.0), 2.5);
            lat.grow(&built);
            let a = audit(&built, &contour_all(&built, &lat));
            assert_eq!(a.open, 0, "{a:?}");
            pinched.push(a.non_manifold);
        }
        assert!(
            pinched[0] > 0,
            "the pad on the lattice's own planes: {pinched:?}"
        );
        assert_eq!(pinched[1], 0, "the pad half a cell off them: {pinched:?}");
    }

    #[test]
    fn a_chunk_holds_its_vertices_to_the_sphere() {
        let ball = Sphere { radius: 6.0 };
        let mut lat = Lattice::new(DVec3::splat(-8.0), 0.5, 4, 32);
        lat.subdivide_near(DVec3::new(0.0, 6.0, 0.0), 2.0);
        for (corner, m) in contour_all(&ball, &lat) {
            for (p, n) in m.positions.iter().zip(&m.normals) {
                let w = corner + DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64);
                let r = w.length();
                assert!((r - 6.0).abs() < 0.03, "a vertex at radius {r}");
                let radial = w / r;
                let nn = DVec3::new(n[0] as f64, n[1] as f64, n[2] as f64);
                assert!(
                    radial.dot(nn) > 0.97,
                    "a normal {} off radial",
                    radial.dot(nn)
                );
            }
        }
    }
}
