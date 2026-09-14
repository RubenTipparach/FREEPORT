//! Marching cubes over a sampled grid: the triangles the ground turns into.
//!
//! The classic algorithm, on the classic tables (`tables.rs`): every cell of
//! the lattice whose eight corners are not all rock or all air gets the
//! triangles its configuration calls for, with each vertex on the edge where
//! the density crosses nought, placed by linear interpolation along that
//! edge. Vertices are SHARED: an edge belongs to up to four cells and the
//! vertex on it is made once and indexed by all of them, which is what makes
//! the shell watertight and the normals smooth. A normal is the field's own
//! gradient at the vertex rather than a face normal, so a sphere comes out
//! round rather than faceted, and it points OUT of the rock.
//!
//! The corner and edge numbering is Bourke's, the one the tables assume:
//!
//! ```text
//! corner 0 (0,0,0)  1 (1,0,0)  2 (1,1,0)  3 (0,1,0)
//!        4 (0,0,1)  5 (1,0,1)  6 (1,1,1)  7 (0,1,1)
//! edge 0: 0-1  1: 1-2  2: 2-3  3: 3-0     the z = 0 square
//!      4: 4-5  5: 5-6  6: 6-7  7: 7-4     the z = 1 square
//!      8: 0-4  9: 1-5 10: 2-6 11: 3-7     the uprights
//! ```
//!
//! and corner bit `c` of a configuration is set when that corner is AIR
//! (density under the iso value), which is the convention the tables were
//! built for and the one the three.js mockup keeps.

use crate::field::Grid;
use crate::tables::{EDGE_TABLE, TRI_TABLE};

/// The triangles of one chunk, positions in metres from the chunk's corner.
#[derive(Clone, Debug, Default)]
pub struct ChunkMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

impl ChunkMesh {
    /// How many triangles.
    pub fn triangles(&self) -> usize {
        self.indices.len() / 3
    }
}

const CORNER: [[i32; 3]; 8] = [
    [0, 0, 0],
    [1, 0, 0],
    [1, 1, 0],
    [0, 1, 0],
    [0, 0, 1],
    [1, 0, 1],
    [1, 1, 1],
    [0, 1, 1],
];

/// Each edge as the lattice point it starts from (within the cell) and the
/// axis it runs along, which is the one way of naming an edge that two
/// cells sharing it agree on.
const EDGE_AT: [([i32; 3], usize); 12] = [
    ([0, 0, 0], 0),
    ([1, 0, 0], 1),
    ([0, 1, 0], 0),
    ([0, 0, 0], 1),
    ([0, 0, 1], 0),
    ([1, 0, 1], 1),
    ([0, 1, 1], 0),
    ([0, 0, 1], 1),
    ([0, 0, 0], 2),
    ([1, 0, 0], 2),
    ([1, 1, 0], 2),
    ([0, 1, 0], 2),
];

const NONE: u32 = u32::MAX;

/// March `grid` at density `iso` (nought for a field that is signed).
pub fn march(grid: &Grid, iso: f32) -> ChunkMesh {
    let n = grid.n;
    let lattice = n + 1;
    let mut cache = vec![NONE; lattice * lattice * lattice * 3];
    let mut mesh = ChunkMesh::default();
    let mut on_edge = [NONE; 12];

    for k in 0..n as i32 {
        for j in 0..n as i32 {
            for i in 0..n as i32 {
                let mut config = 0usize;
                let mut v = [0.0f32; 8];
                for (c, d) in CORNER.iter().enumerate() {
                    v[c] = grid.at(i + d[0], j + d[1], k + d[2]);
                    if v[c] < iso {
                        config |= 1 << c;
                    }
                }
                let crossed = EDGE_TABLE[config];
                if crossed == 0 {
                    continue;
                }
                for (e, (at, axis)) in EDGE_AT.iter().enumerate() {
                    if crossed & (1 << e) == 0 {
                        continue;
                    }
                    let (a, b, c) = (i + at[0], j + at[1], k + at[2]);
                    let slot =
                        ((c as usize * lattice + b as usize) * lattice + a as usize) * 3 + axis;
                    if cache[slot] == NONE {
                        cache[slot] = vertex_on(grid, iso, a, b, c, *axis, &mut mesh);
                    }
                    on_edge[e] = cache[slot];
                }
                for tri in TRI_TABLE[config].chunks(3) {
                    if tri[0] < 0 {
                        break;
                    }
                    for &e in tri {
                        mesh.indices.push(on_edge[e as usize]);
                    }
                }
            }
        }
    }
    mesh
}

/// Make the vertex on the edge from lattice point `(a, b, c)` along `axis`
/// and return its index.
fn vertex_on(
    grid: &Grid,
    iso: f32,
    a: i32,
    b: i32,
    c: i32,
    axis: usize,
    mesh: &mut ChunkMesh,
) -> u32 {
    let mut step = [0i32; 3];
    step[axis] = 1;
    let (a2, b2, c2) = (a + step[0], b + step[1], c + step[2]);
    let (va, vb) = (grid.at(a, b, c), grid.at(a2, b2, c2));
    let t = if (vb - va).abs() < 1e-12 {
        0.5
    } else {
        ((iso - va) / (vb - va)).clamp(0.0, 1.0)
    };
    let cell = grid.cell as f32;
    let mut pos = [a as f32 * cell, b as f32 * cell, c as f32 * cell];
    pos[axis] += t * cell;
    let ga = grid.gradient(a, b, c);
    let gb = grid.gradient(a2, b2, c2);
    // The gradient climbs INTO the rock, and a normal points out of it.
    let mut nrm = [
        -(ga[0] + (gb[0] - ga[0]) * t),
        -(ga[1] + (gb[1] - ga[1]) * t),
        -(ga[2] + (gb[2] - ga[2]) * t),
    ];
    let len = (nrm[0] * nrm[0] + nrm[1] * nrm[1] + nrm[2] * nrm[2]).sqrt();
    if len > 1e-12 {
        nrm = [nrm[0] / len, nrm[1] / len, nrm[2] / len];
    } else {
        nrm = [0.0, 1.0, 0.0];
    }
    mesh.positions.push(pos);
    mesh.normals.push(nrm);
    (mesh.positions.len() - 1) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::{sample, Density, Sphere};
    use glam::DVec3;
    use std::f32::consts::PI;

    fn ball() -> (ChunkMesh, f32) {
        let grid = sample(&Sphere { radius: 6.0 }, DVec3::splat(-8.0), 0.5, 32);
        (march(&grid, 0.0), 8.0)
    }

    fn v3(m: &ChunkMesh, i: u32) -> [f32; 3] {
        m.positions[i as usize]
    }

    fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
    }

    fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    }

    fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    }

    #[test]
    fn a_sphere_marches_to_a_closed_shell() {
        let (mesh, _) = ball();
        assert!(mesh.triangles() > 1000);
        let mut edges: Vec<(u32, u32)> = Vec::with_capacity(mesh.indices.len());
        for t in mesh.indices.chunks(3) {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                edges.push((a.min(b), a.max(b)));
            }
        }
        edges.sort_unstable();
        let mut i = 0;
        while i < edges.len() {
            let mut j = i;
            while j < edges.len() && edges[j] == edges[i] {
                j += 1;
            }
            assert_eq!(j - i, 2, "edge {:?} is on {} triangles", edges[i], j - i);
            i = j;
        }
    }

    #[test]
    fn the_shells_area_is_the_spheres() {
        let (mesh, _) = ball();
        let area: f32 = mesh
            .indices
            .chunks(3)
            .map(|t| {
                let (a, b, c) = (v3(&mesh, t[0]), v3(&mesh, t[1]), v3(&mesh, t[2]));
                let n = cross(sub(b, a), sub(c, a));
                dot(n, n).sqrt() * 0.5
            })
            .sum();
        let want = 4.0 * PI * 36.0;
        assert!(
            (area - want).abs() / want < 0.03,
            "area {area} against {want}"
        );
    }

    #[test]
    fn normals_point_out_of_the_rock_and_the_winding_agrees() {
        let (mesh, off) = ball();
        let mut worst = 1.0f32;
        for (p, n) in mesh.positions.iter().zip(&mesh.normals) {
            let r = [p[0] - off, p[1] - off, p[2] - off];
            let len = dot(r, r).sqrt();
            let radial = [r[0] / len, r[1] / len, r[2] / len];
            worst = worst.min(dot(radial, *n));
            assert!((len - 6.0).abs() < 0.05, "a vertex at radius {len}");
        }
        assert!(worst > 0.95, "the worst normal is {worst} off radial");
        // A face normal against the vertex normals it was made from. Slivers
        // (three vertices nearly on a line) can point anywhere, so the check
        // is weighted by area and nothing of any size may point in.
        let (mut agree, mut total, mut worst, mut worst_area) = (0.0f32, 0.0f32, 1.0f32, 0.0f32);
        for t in mesh.indices.chunks(3) {
            let (a, b, c) = (v3(&mesh, t[0]), v3(&mesh, t[1]), v3(&mesh, t[2]));
            let face = cross(sub(b, a), sub(c, a));
            let area = dot(face, face).sqrt();
            if area < 1e-9 {
                continue;
            }
            let d = dot(face, mesh.normals[t[0] as usize]) / area;
            agree += d * area;
            total += area;
            if d < worst {
                worst = d;
                worst_area = area * 0.5;
            }
        }
        assert!(
            agree / total > 0.98,
            "area weighted agreement {}",
            agree / total
        );
        assert!(
            worst > -0.5 || worst_area < 1e-4,
            "a triangle of area {worst_area} winds {worst} against its normal"
        );
    }

    #[test]
    fn all_rock_or_all_air_makes_nothing() {
        struct Flat(f64);
        impl Density for Flat {
            fn at(&self, _: DVec3) -> f64 {
                self.0
            }
        }
        for d in [1.0, -1.0] {
            let grid = sample(&Flat(d), DVec3::ZERO, 1.0, 8);
            assert_eq!(march(&grid, 0.0).triangles(), 0);
        }
    }

    #[test]
    fn every_table_row_is_whole_triangles() {
        for (config, row) in TRI_TABLE.iter().enumerate() {
            let used = row.iter().take_while(|&&e| e >= 0).count();
            assert_eq!(used % 3, 0, "config {config} lists {used} edge numbers");
            assert!(row.iter().all(|&e| e < 12));
            if config == 0 || config == 255 {
                assert_eq!(used, 0);
                assert_eq!(EDGE_TABLE[config], 0);
            }
            for &e in row.iter().take(used) {
                assert!(
                    EDGE_TABLE[config] & (1 << e) != 0,
                    "config {config} uses edge {e} it does not cross"
                );
            }
        }
    }
}
