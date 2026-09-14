//! The two level lattice: a coarse grid of cells, some of them subdivided.
//!
//! The ground is contoured on a coarse lattice everywhere and on a finer one
//! wherever something built stands, and the two are ONE lattice: every
//! coarse lattice point is a fine one, a coarse cell is `sub` fine cells a
//! side, and a mask says which coarse cells are subdivided. Every position is
//! computed from a FINE index through one function, `point`, so a coarse
//! corner and the fine point under it are the same bits and two chunks that
//! meet at a cell agree on every sample in it. The mask is grown by `grow`
//! until no unsubdivided cell has a surface on a join face that its own
//! eight samples miss, which is what lets the seam between the levels be
//! closed by construction (`dc.rs`).

use crate::field::Density;
use glam::DVec3;

/// Coarse cells along a chunk's side. A chunk is the unit meshed and drawn.
pub const CH: usize = 4;

/// The lattice and its mask.
#[derive(Clone, Debug)]
pub struct Lattice {
    /// The lattice's origin: fine point (0, 0, 0), in the field's frame.
    pub corner: DVec3,
    /// The fine cell, metres.
    pub fine: f64,
    /// Fine cells along a coarse cell's side.
    pub sub: usize,
    /// Coarse cells along the lattice's side.
    pub n: usize,
    mask: Vec<bool>,
}

impl Lattice {
    /// A lattice of `n` coarse cells of `cell` metres, each `sub` fine cells
    /// across, from `corner`.
    pub fn new(corner: DVec3, cell: f64, sub: usize, n: usize) -> Self {
        Lattice {
            corner,
            fine: cell / sub as f64,
            sub,
            n,
            mask: vec![false; n * n * n],
        }
    }

    /// The coarse cell, metres. For reports: a position is never computed
    /// from it, only from `fine` through `point`.
    pub fn cell(&self) -> f64 {
        self.fine * self.sub as f64
    }

    /// The position of a fine lattice point.
    pub fn point(&self, f: [i64; 3]) -> DVec3 {
        self.corner + DVec3::new(f[0] as f64, f[1] as f64, f[2] as f64) * self.fine
    }

    /// The coarse cell a fine cell or point is in.
    pub fn coarse_of(&self, f: [i64; 3]) -> [i64; 3] {
        let s = self.sub as i64;
        [f[0].div_euclid(s), f[1].div_euclid(s), f[2].div_euclid(s)]
    }

    /// The index of a coarse cell, or none outside the lattice.
    pub fn key(&self, c: [i64; 3]) -> Option<usize> {
        let n = self.n as i64;
        if c.iter().any(|&v| v < 0 || v >= n) {
            return None;
        }
        Some(((c[2] * n + c[1]) * n + c[0]) as usize)
    }

    /// Whether a coarse cell is subdivided. Outside the lattice is never.
    pub fn masked(&self, c: [i64; 3]) -> bool {
        self.key(c).map(|k| self.mask[k]).unwrap_or(false)
    }

    /// Subdivide a coarse cell. Returns whether that changed anything.
    pub fn subdivide(&mut self, c: [i64; 3]) -> bool {
        match self.key(c) {
            Some(k) if !self.mask[k] => {
                self.mask[k] = true;
                true
            }
            _ => false,
        }
    }

    /// Subdivide every coarse cell whose box comes within `radius` of `p`.
    /// Returns how many were newly subdivided.
    pub fn subdivide_near(&mut self, p: DVec3, radius: f64) -> usize {
        let cell = self.cell();
        let lo = ((p - DVec3::splat(radius) - self.corner) / cell).floor();
        let hi = ((p + DVec3::splat(radius) - self.corner) / cell).floor();
        let mut added = 0;
        for k in lo.z as i64..=hi.z as i64 {
            for j in lo.y as i64..=hi.y as i64 {
                for i in lo.x as i64..=hi.x as i64 {
                    let c = [i, j, k];
                    let near = self.point([
                        i * self.sub as i64,
                        j * self.sub as i64,
                        k * self.sub as i64,
                    ]);
                    let far = near + DVec3::splat(cell);
                    let closest = p.clamp(near, far);
                    if (closest - p).length() <= radius && self.subdivide(c) {
                        added += 1;
                    }
                }
            }
        }
        added
    }

    /// How many subdivided cells there are.
    pub fn subdivided(&self) -> usize {
        self.mask.iter().filter(|&&m| m).count()
    }

    /// Chunks along the lattice's side.
    pub fn chunks(&self) -> usize {
        self.n.div_ceil(CH)
    }

    /// The chunk a coarse cell is in, as an index that orders chunks: the
    /// lowest chunk among the cells round an edge is the edge's one owner.
    pub fn chunk_of(&self, c: [i64; 3]) -> usize {
        let cn = self.chunks() as i64;
        let b = |v: i64| v.div_euclid(CH as i64);
        ((b(c[2]) * cn + b(c[1])) * cn + b(c[0])) as usize
    }

    /// The chunk index of chunk `(bx, by, bz)`.
    pub fn chunk_index(&self, b: [usize; 3]) -> usize {
        let cn = self.chunks();
        (b[2] * cn + b[1]) * cn + b[0]
    }

    /// Whether any coarse cell of chunk `b` is subdivided.
    pub fn chunk_has_fine(&self, b: [usize; 3]) -> bool {
        let ch = CH as i64;
        let (x0, y0, z0) = (b[0] as i64 * ch, b[1] as i64 * ch, b[2] as i64 * ch);
        (z0..z0 + ch).any(|k| (y0..y0 + ch).any(|j| (x0..x0 + ch).any(|i| self.masked([i, j, k]))))
    }

    /// Grow the mask until every join face is one the coarse samples see:
    /// an unsubdivided cell whose face toward a subdivided one has all four
    /// corners of one sign while a fine point on that face has the other is
    /// subdivided too, and again until nothing changes. Without it a fine
    /// surface can cross a face the coarse cell has no vertex for, and the
    /// seam polygons there would have no corner to end on. Returns how many
    /// cells it added.
    pub fn grow(&mut self, field: &dyn Density) -> usize {
        let mut added = 0;
        loop {
            let mut round = Vec::new();
            for k in 0..self.n as i64 {
                for j in 0..self.n as i64 {
                    for i in 0..self.n as i64 {
                        let c = [i, j, k];
                        if !self.masked(c) {
                            continue;
                        }
                        for axis in 0..3 {
                            for dir in [-1i64, 1] {
                                let mut nb = c;
                                nb[axis] += dir;
                                if self.key(nb).is_none() || self.masked(nb) {
                                    continue;
                                }
                                if self.face_missed(field, c, axis, dir) {
                                    round.push(nb);
                                }
                            }
                        }
                    }
                }
            }
            let before = added;
            for nb in round {
                if self.subdivide(nb) {
                    added += 1;
                }
            }
            if added == before {
                return added;
            }
        }
    }

    /// Whether the face of coarse cell `c` on side `dir` of `axis` has fine
    /// points of both signs while its four coarse corners have one.
    fn face_missed(&self, field: &dyn Density, c: [i64; 3], axis: usize, dir: i64) -> bool {
        let s = self.sub as i64;
        let mut base = [c[0] * s, c[1] * s, c[2] * s];
        if dir > 0 {
            base[axis] += s;
        }
        let (u, v) = perpendicular(axis);
        let sign = |a: i64, b: i64| {
            let mut f = base;
            f[u] += a;
            f[v] += b;
            air(field.at(self.point(f)) as f32)
        };
        let corner = sign(0, 0);
        if sign(s, 0) != corner || sign(0, s) != corner || sign(s, s) != corner {
            return false;
        }
        (0..=s).any(|a| (0..=s).any(|b| sign(a, b) != corner))
    }
}

/// Whether a sample is air. Nought is ROCK, and every pass (the mask's
/// growth, the coarse and the fine contour) asks this one question. A box
/// face laid exactly on a lattice plane samples nought all over that plane;
/// called air, the plane's cells on the rock side carried the face and the
/// crease where the ground met it fell exactly on a lattice edge, with a
/// crossing on the crease itself and no normal to give it, and the foot of
/// the pad grew a shelf of slivers. Called rock, the face is contoured
/// from the cell on its air side a hair outside the plane, where the
/// gradient is the face's own.
pub fn air(v: f32) -> bool {
    v < 0.0
}

/// The two axes across `axis`, in the order that keeps the frame right
/// handed.
pub fn perpendicular(axis: usize) -> (usize, usize) {
    match axis {
        0 => (1, 2),
        1 => (2, 0),
        _ => (0, 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::Sphere;

    #[test]
    fn a_coarse_corner_is_the_fine_point_under_it_to_the_bit() {
        let lat = Lattice::new(DVec3::new(-3.7, 1.1, 0.3), 1.3, 6, 8);
        for i in 0..=8i64 {
            let fine = lat.point([i * 6, 6, 12]);
            let by_cell = lat.corner + DVec3::new(i as f64, 1.0, 2.0) * lat.cell();
            assert!((fine - by_cell).length() < 1e-12);
            assert_eq!(lat.coarse_of([i * 6 + 5, 6, 12]), [i, 1, 2]);
        }
        assert_eq!(lat.coarse_of([-1, 0, 0]), [-1, 0, 0]);
        assert!(lat.key([-1, 0, 0]).is_none() && lat.key([8, 0, 0]).is_none());
    }

    #[test]
    fn subdividing_near_a_point_takes_the_cells_its_ball_touches() {
        let mut lat = Lattice::new(DVec3::splat(-4.0), 1.0, 4, 8);
        let added = lat.subdivide_near(DVec3::new(0.5, 0.5, 0.5), 0.6);
        // The cell round the point, and the six across its faces at 0.5.
        assert_eq!(added, 7);
        assert!(lat.masked([4, 4, 4]) && lat.masked([5, 4, 4]) && !lat.masked([5, 5, 4]));
        assert_eq!(lat.subdivided(), 7);
        assert_eq!(lat.subdivide_near(DVec3::new(0.5, 0.5, 0.5), 0.6), 0);
        assert_eq!(lat.chunk_of([4, 4, 4]), lat.chunk_index([1, 1, 1]));
        assert!(lat.chunk_has_fine([1, 1, 1]) && !lat.chunk_has_fine([0, 0, 0]));
    }

    #[test]
    fn growing_takes_a_cell_whose_samples_miss_a_face_the_fine_sees() {
        // A small ball centred on the middle of the face between two cells:
        // the four coarse corners of that face are air, the fine point at
        // its middle is rock, and no other face of either cell is touched.
        let mut lat = Lattice::new(DVec3::new(-2.0, -1.5, -1.5), 1.0, 4, 4);
        let ball = Sphere { radius: 0.3 };
        assert!(lat.subdivide([2, 1, 1]));
        assert_eq!(lat.grow(&ball), 1);
        assert!(
            lat.masked([1, 1, 1]),
            "the cell across the face the ball straddles"
        );
        assert_eq!(lat.grow(&ball), 0);
        assert!(!air(0.0) && air(-1.0) && !air(0.5));
    }
}
