//! Whether a set of chunk meshes is one closed surface: the proof, measured.
//!
//! The chunks are welded by position (a vertex two chunks both hold is one
//! vertex, to half a millimetre), every edge is counted, and every triangle
//! is tested against the field's gradient at its middle. An edge in one
//! triangle is a hole, an edge in more than two is a pinch, and a triangle
//! wound to face into the rock is culled and is a hole with the dark through
//! it. A mesh that is closed by construction is closed here or the
//! construction has a defect, which is the whole reason the count exists.

use crate::dc::DcMesh;
use crate::field::Density;
use glam::DVec3;
use std::collections::HashMap;

/// What the audit found.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Audit {
    pub triangles: usize,
    pub vertices: usize,
    /// Edges in exactly one triangle: holes.
    pub open: usize,
    /// Edges in more than two triangles: pinches.
    pub non_manifold: usize,
    /// Triangles wound into the rock, and their area, square metres: a
    /// sliver can point anywhere and nothing of any size may point in.
    pub facing_in: usize,
    pub facing_area: f64,
    /// The surface's area, square metres.
    pub area: f64,
    /// Seam polygons and missing corners, summed over the chunks.
    pub seams: usize,
    pub missing: usize,
    /// Where the first few open and pinched edges are: their middles, so a
    /// defect can be looked at rather than counted.
    pub open_at: Vec<DVec3>,
    pub pinch_at: Vec<DVec3>,
    pub facing_at: Vec<DVec3>,
}

/// Positions are welded to this, metres.
const WELD: f64 = 0.0005;

/// The one vertex at `w`, made if none is within `WELD` of it. The bins are
/// `WELD` wide and the neighbouring ones are searched too, because two
/// chunks place a shared vertex a float's rounding apart and a rounding
/// that straddles a bin edge is two vertices otherwise.
fn weld(ids: &mut HashMap<[i64; 3], u32>, points: &mut Vec<DVec3>, w: DVec3) -> u32 {
    let key = [
        (w.x / WELD).round() as i64,
        (w.y / WELD).round() as i64,
        (w.z / WELD).round() as i64,
    ];
    for dz in -1..=1 {
        for dy in -1..=1 {
            for dx in -1..=1 {
                if let Some(&id) = ids.get(&[key[0] + dx, key[1] + dy, key[2] + dz]) {
                    if (points[id as usize] - w).length() <= WELD {
                        return id;
                    }
                }
            }
        }
    }
    points.push(w);
    let id = (points.len() - 1) as u32;
    ids.insert(key, id);
    id
}

/// Audit `chunks`, each a mesh and the world position of its corner, against
/// `field`.
pub fn audit(field: &dyn Density, chunks: &[(DVec3, DcMesh)]) -> Audit {
    let mut ids: HashMap<[i64; 3], u32> = HashMap::new();
    let mut points: Vec<DVec3> = Vec::new();
    let mut edges: HashMap<(u32, u32), u32> = HashMap::new();
    let mut out = Audit::default();
    let eps = 0.02;
    for (corner, mesh) in chunks {
        out.seams += mesh.seams;
        out.missing += mesh.missing;
        let mut local: Vec<u32> = Vec::with_capacity(mesh.positions.len());
        for p in &mesh.positions {
            let w = *corner + DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64);
            local.push(weld(&mut ids, &mut points, w));
        }
        for t in mesh.indices.chunks(3) {
            let (a, b, c) = (
                local[t[0] as usize],
                local[t[1] as usize],
                local[t[2] as usize],
            );
            out.triangles += 1;
            for (u, v) in [(a, b), (b, c), (c, a)] {
                *edges.entry((u.min(v), u.max(v))).or_insert(0) += 1;
            }
            let (pa, pb, pc) = (points[a as usize], points[b as usize], points[c as usize]);
            let face = (pb - pa).cross(pc - pa);
            let area = face.length();
            out.area += area * 0.5;
            if area < 1e-12 {
                continue;
            }
            let m = (pa + pb + pc) / 3.0;
            let d = |axis: DVec3| field.at(m + axis * eps) - field.at(m - axis * eps);
            let g = DVec3::new(d(DVec3::X), d(DVec3::Y), d(DVec3::Z));
            if face.dot(g) > 0.2 * area * g.length() {
                out.facing_in += 1;
                out.facing_area += area * 0.5;
                if out.facing_at.len() < 64 {
                    out.facing_at.push(m);
                }
            }
        }
    }
    out.vertices = points.len();
    for (&(u, v), &count) in &edges {
        match count {
            1 => {
                out.open += 1;
                if out.open_at.len() < 64 {
                    out.open_at
                        .push((points[u as usize] + points[v as usize]) * 0.5);
                }
            }
            2 => {}
            _ => {
                out.non_manifold += 1;
                if out.pinch_at.len() < 64 {
                    out.pinch_at
                        .push((points[u as usize] + points[v as usize]) * 0.5);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::Sphere;

    /// A tetrahedron round the origin, closed and wound outward, as two
    /// chunks that share every vertex on their border.
    fn tetra(flip: bool) -> Vec<(DVec3, DcMesh)> {
        let p = [
            [1.0, 1.0, 1.0],
            [1.0, -1.0, -1.0],
            [-1.0, 1.0, -1.0],
            [-1.0, -1.0, 1.0],
        ];
        let faces: [[u32; 3]; 4] = [[0, 1, 2], [0, 3, 1], [0, 2, 3], [1, 3, 2]];
        let mut chunks = Vec::new();
        for (which, corner) in [DVec3::ZERO, DVec3::new(0.5, 0.0, 0.0)]
            .into_iter()
            .enumerate()
        {
            let mut m = DcMesh::default();
            for q in p {
                let l = DVec3::from(q) - corner;
                m.positions.push([l.x as f32, l.y as f32, l.z as f32]);
                m.normals.push([0.0, 1.0, 0.0]);
                m.levels.push(0);
            }
            for (i, f) in faces.iter().enumerate() {
                if i % 2 != which {
                    continue;
                }
                let f = if flip && i == 0 {
                    [f[0], f[2], f[1]]
                } else {
                    *f
                };
                m.indices.extend_from_slice(&f);
            }
            chunks.push((corner, m));
        }
        chunks
    }

    #[test]
    fn a_closed_tetrahedron_over_two_chunks_welds_shut() {
        let rock = Sphere { radius: 0.2 };
        let a = audit(&rock, &tetra(false));
        assert_eq!(a.triangles, 4);
        assert_eq!(a.vertices, 4);
        assert_eq!(a.open, 0);
        assert_eq!(a.non_manifold, 0);
        assert_eq!(a.facing_in, 0);
        assert!(
            (a.area - 4.0 * 3.0f64.sqrt() * 2.0).abs() < 1e-9,
            "area {}",
            a.area
        );
    }

    #[test]
    fn a_face_wound_inward_is_counted_and_a_face_dropped_opens_three_edges() {
        let rock = Sphere { radius: 0.2 };
        let flipped = audit(&rock, &tetra(true));
        assert_eq!(flipped.facing_in, 1);
        assert!((flipped.facing_area - 2.0 * 3.0f64.sqrt()).abs() < 1e-9);
        let mut chunks = tetra(false);
        chunks[1].1.indices.truncate(3);
        let a = audit(&rock, &chunks);
        assert_eq!(a.triangles, 3);
        assert_eq!(a.open, 3);
    }
}
