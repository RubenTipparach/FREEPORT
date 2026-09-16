//! One lattice at every level: fine addressing, chunks at levels, and the
//! rings of chunks round the eye.
//!
//! A planet is contoured at whatever detail the eye is near enough to see:
//! quarter metre cells under the feet and cells of hundreds of metres on the
//! far side. Every level is one lattice: a cell at level L is `2^L` fine
//! cells a side, a lattice point at any level is a fine point, and every
//! position is computed from a FINE index through one function, `point`,
//! so a coarse corner and the fine point under it are the same bits and two
//! chunks at two levels agree on every sample they share. A chunk is `CH`
//! cells of its level a side, and the chunks round the eye are RINGS: a box
//! of chunks at each level, each box inside the next coarser's, aligned to
//! the coarser's chunks and kept a whole coarser chunk inside it, so no two
//! chunks that touch differ by more than one level. That is what lets the
//! seam between them be one polygon rule (`dc.rs`) rather than a skirt.

use glam::DVec3;

/// Cells along a chunk's side, at the chunk's own level.
pub const CH: i64 = 16;

/// Cells of margin a chunk samples round itself, at its own level: two,
/// because a coarser neighbour's cell is two of these and a seam needs that
/// cell's far corners.
pub const MARGIN: i64 = 2;

/// Chunks each way from the middle that a level's box holds, so a box is
/// `2 * HALF` chunks a side. Four is the least that works: the finer box is
/// `HALF` coarser chunks wide and its middle is within a chunk of this
/// one's, and what is left over is at least one whole coarser chunk of
/// margin on every side, which is what keeps every neighbour within one
/// level (`a_box_is_a_whole_chunk_inside_the_next` measures it).
pub const HALF: i64 = 4;

/// The lattice: where fine point (0, 0, 0) is and how big a fine cell is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lattice {
    /// The lattice's origin, in the field's frame.
    pub corner: DVec3,
    /// The fine cell, metres: the cell at level 0.
    pub fine: f64,
}

impl Lattice {
    /// A lattice of `fine` metre cells from `corner`.
    pub fn new(corner: DVec3, fine: f64) -> Self {
        Lattice { corner, fine }
    }

    /// The position of a fine lattice point. The one function every
    /// position comes through.
    pub fn point(&self, f: [i64; 3]) -> DVec3 {
        self.corner + DVec3::new(f[0] as f64, f[1] as f64, f[2] as f64) * self.fine
    }

    /// The cell at a level, metres.
    pub fn cell(&self, level: u8) -> f64 {
        self.fine * (1u64 << level) as f64
    }

    /// The fine cell a point is in.
    pub fn fine_cell(&self, p: DVec3) -> [i64; 3] {
        let f = ((p - self.corner) / self.fine).floor();
        [f.x as i64, f.y as i64, f.z as i64]
    }
}

/// A chunk: a level and its place at that level, in chunks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChunkId {
    pub level: u8,
    pub at: [i64; 3],
}

impl ChunkId {
    /// Fine cells in one of the chunk's cells.
    pub fn scale(&self) -> i64 {
        1 << self.level
    }

    /// Fine cells along the chunk's side.
    pub fn span(&self) -> i64 {
        CH * self.scale()
    }

    /// The chunk's first fine point.
    pub fn f0(&self) -> [i64; 3] {
        let s = self.span();
        [self.at[0] * s, self.at[1] * s, self.at[2] * s]
    }

    /// The chunk's corner, in the field's frame.
    pub fn corner(&self, lat: &Lattice) -> DVec3 {
        lat.point(self.f0())
    }

    /// The chunk's side, metres.
    pub fn size(&self, lat: &Lattice) -> f64 {
        lat.cell(self.level) * CH as f64
    }

    /// The chunk at `level` holding fine cell `f`.
    pub fn holding(level: u8, f: [i64; 3]) -> ChunkId {
        let s = CH << level;
        ChunkId {
            level,
            at: [f[0].div_euclid(s), f[1].div_euclid(s), f[2].div_euclid(s)],
        }
    }

    /// The chunk's box, expanded by `margin` cells of its level.
    pub fn bounds(&self, lat: &Lattice, margin: i64) -> (DVec3, DVec3) {
        let m = margin * self.scale();
        let f0 = self.f0();
        let s = self.span();
        (
            lat.point([f0[0] - m, f0[1] - m, f0[2] - m]),
            lat.point([f0[0] + s + m, f0[1] + s + m, f0[2] + s + m]),
        )
    }
}

/// Which level's chunk holds a fine cell: what a chunk contours against.
/// `None` is nowhere, which a chunk treats as its own level with nothing
/// in it.
pub trait Levels {
    /// The level of the chunk holding fine cell `f`.
    fn level_at(&self, f: [i64; 3]) -> Option<u8>;
}

/// Every chunk is at one level: the lattice with no rings, for a test.
pub struct Flat(pub u8);

impl Levels for Flat {
    fn level_at(&self, _f: [i64; 3]) -> Option<u8> {
        Some(self.0)
    }
}

/// The rings: a box of `2 * HALF` chunks a side at each level, the finest
/// round the eye and each inside the next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rings {
    /// Each level's box middle, in that level's chunks; always even, so the
    /// box is aligned to the next level's chunks.
    pub centre: Vec<[i64; 3]>,
    /// Finest active ring. At altitude, finer boxes contain only air.
    pub min_level: u8,
}

/// How far the eye may drift from a box's middle, in that level's chunks,
/// before the box follows it. Over one so a box does not flap on a chunk
/// line, and under one and a third, which is the most that keeps a finer
/// box's middle within a chunk of this one's when both are snapped.
const DRIFT: f64 = 1.25;

impl Rings {
    /// Rings of `levels` levels with every box centred on `eye`.
    pub fn around(lat: &Lattice, eye: DVec3, levels: u8) -> Rings {
        let mut rings = Rings {
            centre: vec![[0; 3]; levels as usize],
            min_level: 0,
        };
        for level in 0..levels {
            let u = rings.eye_in(lat, eye, level);
            rings.centre[level as usize] = snap(u);
        }
        rings
    }

    /// How many levels.
    pub fn levels(&self) -> u8 {
        self.centre.len() as u8
    }

    /// Adapt the finest ring to height above the surface, with hysteresis.
    /// The finest box stays filled and every neighbour still differs by at
    /// most one level. Height and the lattice are in absolute world metres.
    pub fn adapt(&mut self, lat: &Lattice, height: f64) -> bool {
        if !height.is_finite() || self.levels() == 0 {
            return false;
        }
        let old = self.min_level;
        let reach = |level| lat.cell(level) * CH as f64 * HALF as f64;
        while self.min_level + 1 < self.levels() && height > reach(self.min_level + 1) * 1.2 {
            self.min_level += 1;
        }
        while self.min_level > 0 && height < reach(self.min_level) * 0.8 {
            self.min_level -= 1;
        }
        old != self.min_level
    }

    /// The eye's place at a level, in that level's chunks.
    fn eye_in(&self, lat: &Lattice, eye: DVec3, level: u8) -> DVec3 {
        (eye - lat.corner) / (lat.cell(level) * CH as f64)
    }

    /// Move every box the eye has drifted `DRIFT` chunks from the middle of.
    /// Returns whether any moved.
    pub fn follow(&mut self, lat: &Lattice, eye: DVec3) -> bool {
        let mut moved = false;
        for level in 0..self.levels() {
            let u = self.eye_in(lat, eye, level);
            let c = self.centre[level as usize];
            let off = (u - DVec3::new(c[0] as f64, c[1] as f64, c[2] as f64)).abs();
            if off.max_element() > DRIFT {
                self.centre[level as usize] = snap(u);
                moved = true;
            }
        }
        moved
    }

    /// Whether a chunk is inside its level's box.
    pub fn holds(&self, id: ChunkId) -> bool {
        if id.level < self.min_level {
            return false;
        }
        let Some(c) = self.centre.get(id.level as usize) else {
            return false;
        };
        (0..3).all(|i| id.at[i] >= c[i] - HALF && id.at[i] < c[i] + HALF)
    }

    /// Whether a chunk is wholly inside the next finer level's box, where
    /// the finer chunks stand in for it.
    fn under_finer(&self, id: ChunkId) -> bool {
        if id.level == self.min_level {
            return false;
        }
        let c = self.centre[id.level as usize - 1];
        (0..3).all(|i| 2 * id.at[i] >= c[i] - HALF && 2 * id.at[i] + 2 <= c[i] + HALF)
    }

    /// Every chunk the rings ask for: each level's box, less the part the
    /// finer box covers.
    pub fn chunks(&self) -> Vec<ChunkId> {
        let mut out = Vec::new();
        for (level, c) in self.centre.iter().enumerate().skip(self.min_level as usize) {
            for z in c[2] - HALF..c[2] + HALF {
                for y in c[1] - HALF..c[1] + HALF {
                    for x in c[0] - HALF..c[0] + HALF {
                        let id = ChunkId {
                            level: level as u8,
                            at: [x, y, z],
                        };
                        if !self.under_finer(id) {
                            out.push(id);
                        }
                    }
                }
            }
        }
        out
    }

    /// The levels of a chunk's twenty six neighbours, packed two bits each
    /// (finer, same, coarser), which is everything its mesh depends on
    /// besides the field: a chunk whose signature changed is contoured
    /// again.
    pub fn signature(&self, id: ChunkId) -> u64 {
        let mut sig = 0u64;
        let mut bit = 0;
        for dz in -1..=1i64 {
            for dy in -1..=1i64 {
                for dx in -1..=1i64 {
                    if dx == 0 && dy == 0 && dz == 0 {
                        continue;
                    }
                    let n = ChunkId {
                        level: id.level,
                        at: [id.at[0] + dx, id.at[1] + dy, id.at[2] + dz],
                    };
                    let kind = match self.level_at(n.f0()) {
                        Some(l) if l < id.level => 0,
                        Some(l) if l > id.level => 2,
                        _ => 1,
                    };
                    sig |= kind << bit;
                    bit += 2;
                }
            }
        }
        sig
    }
}

/// The even chunk nearest a place, per axis.
fn snap(u: DVec3) -> [i64; 3] {
    let s = |v: f64| 2 * (v * 0.5).round() as i64;
    [s(u.x), s(u.y), s(u.z)]
}

impl Levels for Rings {
    fn level_at(&self, f: [i64; 3]) -> Option<u8> {
        (self.min_level..self.levels()).find(|&level| self.holds(ChunkId::holding(level, f)))
    }
}

/// Whether a sample is air. Nought is ROCK, and every pass asks this one
/// question. A box face laid exactly on a lattice plane samples nought all
/// over that plane; called air, the plane's cells on the rock side carried
/// the face and the crease where the ground met it fell exactly on a
/// lattice edge, with a crossing on the crease itself and no normal to give
/// it, and the foot of the pad grew a shelf of slivers. Called rock, the
/// face is contoured from the cell on its air side a hair outside the
/// plane, where the gradient is the face's own.
pub fn air(v: f32) -> bool {
    v < 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_coarse_corner_is_the_fine_point_under_it_to_the_bit() {
        let lat = Lattice::new(DVec3::new(-3.7, 1.1, 0.3), 0.25);
        let id = ChunkId {
            level: 3,
            at: [2, -1, 5],
        };
        assert_eq!(id.scale(), 8);
        assert_eq!(id.span(), 128);
        assert_eq!(id.f0(), [256, -128, 640]);
        assert_eq!(id.corner(&lat), lat.point([256, -128, 640]));
        assert_eq!(id.size(&lat), 32.0);
        assert_eq!(lat.cell(3), 2.0);
        // The same point through the fine index and through the level's cell
        // arithmetic differ, and only the fine index is ever used.
        let fine = lat.point([256 + 8 * 5, -128, 640]);
        assert!((fine - (id.corner(&lat) + DVec3::X * 10.0)).length() < 1e-12);
        assert_eq!(
            ChunkId::holding(3, [256 + 127, -1, 640]),
            ChunkId {
                level: 3,
                at: [2, -1, 5]
            }
        );
        assert_eq!(
            lat.fine_cell(lat.point([7, -3, 2]) + DVec3::splat(0.1)),
            [7, -3, 2]
        );
        let (lo, hi) = id.bounds(&lat, MARGIN);
        assert_eq!(lo, lat.point([256 - 16, -128 - 16, 640 - 16]));
        assert_eq!(
            hi,
            lat.point([256 + 128 + 16, -128 + 128 + 16, 640 + 128 + 16])
        );
    }

    #[test]
    fn rings_nest_and_every_chunk_is_at_one_level() {
        let lat = Lattice::new(DVec3::splat(-100.0), 0.25);
        let eye = DVec3::new(3.0, 41.0, -7.0);
        let rings = Rings::around(&lat, eye, 4);
        for c in &rings.centre {
            assert!(c.iter().all(|v| v % 2 == 0), "a box middle is even: {c:?}");
        }
        let chunks = rings.chunks();
        // Level 0 is a full box; every coarser level is a box less its middle.
        let full = (2 * HALF).pow(3) as usize;
        let ring = full - HALF.pow(3) as usize;
        assert_eq!(chunks.len(), full + 3 * ring);
        for id in &chunks {
            assert_eq!(rings.level_at(id.f0()), Some(id.level), "{id:?}");
            let last = id.f0().map(|v| v + id.span() - 1);
            assert_eq!(
                rings.level_at(last),
                Some(id.level),
                "{id:?} at its far corner"
            );
        }
        // The eye's own fine cell is at level 0.
        assert_eq!(rings.level_at(lat.fine_cell(eye)), Some(0));
    }

    #[test]
    fn altitude_drops_air_rings_without_leaving_a_hole() {
        let lat = Lattice::new(DVec3::ZERO, 0.25);
        let mut rings = Rings::around(&lat, DVec3::ZERO, 8);
        let full = rings.chunks().len();
        assert!(rings.adapt(&lat, 500.0));
        assert!(rings.min_level > 0);
        assert!(rings.chunks().len() < full);
        assert_eq!(rings.level_at([0; 3]), Some(rings.min_level));
        for id in rings.chunks() {
            assert_eq!(rings.level_at(id.f0()), Some(id.level));
        }
        assert!(!rings.adapt(&lat, 500.1));
        assert!(rings.adapt(&lat, 0.0));
        assert_eq!(rings.min_level, 0);
        assert_eq!(rings.chunks().len(), full);
    }

    #[test]
    fn a_box_is_a_whole_chunk_inside_the_next() {
        // Wherever the eye is, and however the boxes have drifted, no chunk
        // touches a chunk two levels off: a level's box stands at least one
        // coarser chunk inside the coarser box on every side.
        let lat = Lattice::new(DVec3::ZERO, 0.25);
        let mut rings = Rings::around(&lat, DVec3::ZERO, 5);
        let mut worst = i64::MAX;
        let mut eye = DVec3::ZERO;
        for step in 0..400 {
            let t = step as f64;
            eye = DVec3::new(
                t * 3.1,
                (t * 0.7).sin() * 40.0,
                -t * 1.3 + (t * 0.3).cos() * 25.0,
            );
            rings.follow(&lat, eye);
            for level in 1..rings.levels() as usize {
                let (c, f) = (rings.centre[level], rings.centre[level - 1]);
                for i in 0..3 {
                    let lo = (f[i] - HALF) / 2 - (c[i] - HALF);
                    let hi = (c[i] + HALF) - (f[i] + HALF) / 2;
                    worst = worst.min(lo).min(hi);
                }
            }
            if step % 7 != 0 {
                continue;
            }
            // Every cell touching a chunk, a chunk's own cell apart along
            // its faces, is held by a chunk within one level of it.
            for id in rings.chunks() {
                let (f0, s, span) = (id.f0(), id.scale(), id.span());
                for dz in -1..=1i64 {
                    for dy in -1..=1i64 {
                        for dx in -1..=1i64 {
                            let d = [dx, dy, dz];
                            if d == [0, 0, 0] {
                                continue;
                            }
                            let along = |i: usize| -> Vec<i64> {
                                match d[i] {
                                    -1 => vec![f0[i] - s],
                                    1 => vec![f0[i] + span],
                                    _ => (0..CH).step_by(3).map(|c| f0[i] + c * s).collect(),
                                }
                            };
                            for x in along(0) {
                                for y in along(1) {
                                    for &z in &along(2) {
                                        if let Some(l) = rings.level_at([x, y, z]) {
                                            assert!(
                                                (l as i64 - id.level as i64).abs() <= 1,
                                                "{id:?} beside level {l} at step {step}"
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(worst >= 1, "a box came within {worst} chunks of the next");
        assert!(
            !rings.follow(&lat, eye),
            "a second look at the same eye moves nothing"
        );
    }

    #[test]
    fn a_signature_changes_when_a_neighbour_changes_level() {
        let lat = Lattice::new(DVec3::ZERO, 0.25);
        let mut rings = Rings::around(&lat, DVec3::ZERO, 3);
        let id = ChunkId {
            level: 1,
            at: [HALF / 2, 0, 0],
        };
        assert!(!rings.under_finer(id));
        let before = rings.signature(id);
        // Walk the eye east until the level 0 box moves past this chunk.
        rings.follow(&lat, DVec3::new(lat.cell(0) * CH as f64 * 4.0, 0.0, 0.0));
        assert_ne!(before, rings.signature(id));
        assert!(!air(0.0) && air(-1.0) && !air(0.5));
    }
}
