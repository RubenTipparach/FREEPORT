//! Goldberg tiles, addressed LOCALLY: a tile is a lattice point on one face
//! of a subdivided icosahedron, and nothing ever builds the whole sphere.
//!
//! Tenebris builds its Goldberg polyhedron whole, 163,842 tiles for a three
//! hundred metre planet, and walks the list. That does not survive the scale
//! this game is at: a five kilometre planet at half metre tiles is over a
//! billion of them, which is neither a list nor an allocation. So a tile is
//! an ADDRESS computed on demand: the icosahedron subdivided `n` ways, a
//! lattice point (`i`, `j`) on a face, and every answer about it (where it
//! is, what is round it, what its hexagon's corners are) a function of that
//! address alone. Nothing is stored, so a compute shader can ask the same
//! questions of a million tiles without a buffer between them.
//!
//! The tiles are the DUAL of the subdivided icosahedron: a lattice point is
//! a tile's middle and its hexagon's corners are the middles of the six
//! triangles round it, which is how `docs/mockups/hex-terrain.html` builds
//! the same thing from the whole solid. The twelve icosahedron corners have
//! five triangles round them and are pentagons, and that is the whole of the
//! difference.
//!
//! A lattice point on a face's edge belongs to two faces and a corner to
//! five, so an address is CANONICAL: the lowest numbered face that carries
//! the point names it, and `canonical` is what every answer goes through.
//! Without it a tile on a face edge would be drawn twice and walked as two
//! places.

use glam::DVec3;

/// The twelve vertices of a regular icosahedron, normalised, and the twenty
/// faces as triples of them. The same solid `lod.rs` recurses on, so a tile
/// and a level of detail triangle stand on one base.
pub fn icosahedron() -> ([DVec3; 12], [[usize; 3]; 20]) {
    crate::lod::icosahedron()
}

/// A tile: a lattice point on a face of the subdivided icosahedron, with
/// `i` along the face's first edge and `j` along its second. Canonical,
/// which is what makes two tiles equal when they are the same place.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tile {
    pub face: u8,
    pub i: u32,
    pub j: u32,
}

/// The six steps from a lattice point to its neighbours, in order round it.
const STEPS: [(i64, i64); 6] = [(1, 0), (0, 1), (-1, 1), (-1, 0), (0, -1), (1, -1)];

/// The icosahedron subdivided `n` ways along every edge: `n * n` lattice
/// points to a face, and a tile about `2 * PI * radius / (5.5 * n)` metres
/// across.
#[derive(Clone, Copy, Debug)]
pub struct Grid {
    /// Tiles along an icosahedron edge. One is the icosahedron itself, whose
    /// dual is a dodecahedron: twelve tiles and no hexagons at all.
    pub n: u32,
}

impl Grid {
    /// A grid of `n` tiles along an icosahedron edge.
    pub fn new(n: u32) -> Grid {
        Grid { n: n.max(1) }
    }

    /// The grid whose tiles are about `metres` across on a planet of
    /// `radius`: an icosahedron edge subtends about 1.107 radians, so the
    /// tiles along it are that arc over the spacing.
    pub fn for_tile(radius: f64, metres: f64) -> Grid {
        let edge = 1.107_148_717_794_09 * radius;
        Grid::new((edge / metres.max(f64::MIN_POSITIVE)).round().max(1.0) as u32)
    }

    /// How far a tile's middle is from its neighbour's, metres.
    pub fn spacing(&self, radius: f64) -> f64 {
        1.107_148_717_794_09 * radius / self.n as f64
    }

    /// How many tiles cover the whole planet: the dual of the subdivided
    /// icosahedron has `10 n^2 + 2` faces, which is what a global list would
    /// have to hold and the reason there is no global list.
    pub fn count(&self) -> u64 {
        10 * (self.n as u64) * (self.n as u64) + 2
    }

    /// The direction of a point at fractional lattice coordinates on a face,
    /// which is a tile's middle at whole ones and a hexagon's corner at
    /// thirds.
    pub fn point(&self, face: u8, x: f64, y: f64) -> DVec3 {
        let (v, faces) = icosahedron();
        let f = faces[face as usize];
        let n = self.n as f64;
        let (a, b, c) = ((n - x - y) / n, x / n, y / n);
        (v[f[0]] * a + v[f[1]] * b + v[f[2]] * c).normalize()
    }

    /// Where a tile's middle is, as a direction from the planet's centre.
    pub fn dir(&self, tile: Tile) -> DVec3 {
        self.point(tile.face, tile.i as f64, tile.j as f64)
    }

    /// The canonical address of a lattice point: the lowest numbered face
    /// that carries it, with the coordinates written in that face's own
    /// axes. A point inside a face is already canonical; one on an edge
    /// belongs to two faces and a corner to five.
    pub fn canonical(&self, face: u8, i: i64, j: i64) -> Option<Tile> {
        let n = self.n as i64;
        let k = n - i - j;
        if i < 0 || j < 0 || k < 0 {
            return None;
        }
        let (_, faces) = icosahedron();
        let f = faces[face as usize];
        // The point's weight on each icosahedron vertex it stands on.
        let held = [(f[0], k), (f[1], i), (f[2], j)];
        for (index, g) in faces.iter().enumerate() {
            let carries = held
                .iter()
                .all(|(id, w)| *w == 0 || g.iter().any(|x| x == id));
            if !carries {
                continue;
            }
            let weight = |id: usize| -> i64 {
                held.iter()
                    .find(|(x, _)| *x == id)
                    .map(|(_, w)| *w)
                    .unwrap_or(0)
            };
            let (a, b, c) = (weight(g[0]), weight(g[1]), weight(g[2]));
            if a + b + c != n {
                continue;
            }
            return Some(Tile {
                face: index as u8,
                i: b as u32,
                j: c as u32,
            });
        }
        None
    }

    /// The tile a direction falls in: the face it points at, the lattice
    /// point nearest it in that face, canonicalised.
    pub fn at(&self, d: DVec3) -> Tile {
        let (v, faces) = icosahedron();
        let d = d.normalize_or(DVec3::Z);
        // The face a ray from the centre leaves through is the one whose
        // middle it is nearest: every face of a regular solid is the same
        // shape the same distance out, so the nearest middle is the one it
        // is inside.
        let mut face = 0usize;
        let mut best = f64::NEG_INFINITY;
        for (index, f) in faces.iter().enumerate() {
            let mid = (v[f[0]] + v[f[1]] + v[f[2]]).normalize();
            let s = mid.dot(d);
            if s > best {
                best = s;
                face = index;
            }
        }
        let f = faces[face];
        let (a, b, c) = barycentric(v[f[0]], v[f[1]], v[f[2]], d);
        let n = self.n as f64;
        let sum = a + b + c;
        let (a, b, c) = (a * n / sum, b * n / sum, c * n / sum);
        // Rounding all three and mending the worst is the triangular
        // lattice's own rounding: it keeps the three whole numbers summing
        // to `n`, which rounding two of them and subtracting does not.
        let (mut ra, mut rb, mut rc) = (a.round(), b.round(), c.round());
        let (da, db, dc) = ((ra - a).abs(), (rb - b).abs(), (rc - c).abs());
        if da > db && da > dc {
            ra = n - rb - rc;
        } else if db > dc {
            rb = n - ra - rc;
        } else {
            rc = n - ra - rb;
        }
        let _ = ra;
        let (i, j) = (rb.max(0.0) as i64, rc.max(0.0) as i64);
        let i = i.min(self.n as i64);
        let j = j.min(self.n as i64 - i);
        self.canonical(face as u8, i, j).unwrap_or(Tile {
            face: face as u8,
            i: 0,
            j: 0,
        })
    }

    /// The tiles round a tile: six of them, or five at each of the twelve
    /// icosahedron corners, in no particular order.
    ///
    /// A step that leaves the face is carried onto the face across that edge
    /// by UNFOLDING the two triangles flat, which is exact rather than near
    /// enough: an icosahedron's faces are flat equilateral triangles and the
    /// lattice on them is linear in their corners, so two faces laid flat
    /// share one lattice and a step across the edge is a step in it. The
    /// first cut extrapolated the barycentric coordinates past the edge and
    /// asked which tile the direction fell in, and at a corner that landed
    /// two of the five neighbours on one tile.
    pub fn round(&self, tile: Tile) -> Vec<Tile> {
        let n = self.n as i64;
        let (i, j) = (tile.i as i64, tile.j as i64);
        let k = n - i - j;
        // A tile ON a corner of the icosahedron is where the unfolding is
        // defective: five triangles meet there rather than six, which is the
        // whole of what makes it a pentagon.
        if [k, i, j].iter().filter(|w| **w == 0).count() == 2 {
            return self.round_corner(tile);
        }
        let (_, faces) = icosahedron();
        let f = faces[tile.face as usize];
        let mut out: Vec<Tile> = Vec::with_capacity(6);
        for (di, dj) in STEPS {
            let w = [k - di - dj, i + di, j + dj];
            let found = match w.iter().position(|x| *x < 0) {
                None => self.canonical(tile.face, w[1], w[2]),
                Some(gone) => self.across(tile.face as usize, f[gone], &w),
            };
            if let Some(t) = found {
                if t != tile && !out.contains(&t) {
                    out.push(t);
                }
            }
        }
        out
    }

    /// A lattice point one step past a face's edge, read on the face across
    /// it: the edge's own two weights a step lower, and the weight the step
    /// spent standing on the far face's third corner.
    fn across(&self, face: usize, gone: usize, w: &[i64; 3]) -> Option<Tile> {
        let (_, faces) = icosahedron();
        let f = faces[face];
        let held: Vec<(usize, i64)> = (0..3)
            .filter(|x| f[*x] != gone)
            .map(|x| (f[x], w[x] - 1))
            .collect();
        let (other, g) = faces
            .iter()
            .enumerate()
            .find(|(x, g)| *x != face && held.iter().all(|(v, _)| g.contains(v)))?;
        let weight = |v: usize| -> i64 {
            held.iter()
                .find(|(x, _)| *x == v)
                .map(|(_, p)| *p)
                .unwrap_or(1)
        };
        self.canonical(other as u8, weight(g[1]), weight(g[2]))
    }

    /// The five tiles round one of the twelve icosahedron corners: the
    /// lattice point one step along each of the five edges that meet there.
    fn round_corner(&self, tile: Tile) -> Vec<Tile> {
        let (_, faces) = icosahedron();
        let f = faces[tile.face as usize];
        let n = self.n as i64;
        let mine = if tile.i as i64 == n {
            f[1]
        } else if tile.j as i64 == n {
            f[2]
        } else {
            f[0]
        };
        let mut out: Vec<Tile> = Vec::with_capacity(5);
        for (index, g) in faces.iter().enumerate() {
            if !g.contains(&mine) {
                continue;
            }
            for v in g.iter().filter(|v| **v != mine) {
                let weight = |x: usize| -> i64 {
                    if x == mine {
                        n - 1
                    } else if x == *v {
                        1
                    } else {
                        0
                    }
                };
                if let Some(t) = self.canonical(index as u8, weight(g[1]), weight(g[2])) {
                    if t != tile && !out.contains(&t) {
                        out.push(t);
                    }
                }
            }
        }
        out
    }

    /// The corners of a tile's hexagon, in order round it: the middles of
    /// the triangles between its neighbours, which is the dual of the
    /// subdivided icosahedron and the same construction the mockup's
    /// `goldberg` uses on the whole solid.
    pub fn corners(&self, tile: Tile) -> Vec<DVec3> {
        let c = self.dir(tile);
        let mut round: Vec<(f64, DVec3)> = Vec::new();
        let (east, north) = frame(c);
        for t in self.round(tile) {
            let d = self.dir(t);
            round.push((d.dot(north).atan2(d.dot(east)), d));
        }
        round.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut out = Vec::with_capacity(round.len());
        for k in 0..round.len() {
            let a = round[k].1;
            let b = round[(k + 1) % round.len()].1;
            out.push((c + a + b).normalize());
        }
        out
    }

    /// A tile's middle and the two steps of the lattice there, which is
    /// the whole of what a shader needs to draw a window of tiles round it
    /// (`tiers.wgsl`, `tile_dir`): the tile `(u, v)` steps away is
    /// `normalize(mid + u * e1 + v * e2)`, with no face table, no
    /// canonical address and no unfolding.
    ///
    /// The three are the face's OWN plane rather than the sphere's, so on
    /// the anchor's face the window is not an approximation at all: a
    /// lattice point is linear in the face's corners and only the
    /// normalise is not, so stepping before the normalise is exactly
    /// `point`. What it cannot do is leave the face, because the next face
    /// is a different plane, and it cannot do a corner at all, where five
    /// faces meet and a window is six ways round.
    /// `a_window_of_steps_is_the_grids_own_tiles` measures all three.
    pub fn basis(&self, tile: Tile) -> (DVec3, DVec3, DVec3) {
        let (v, faces) = icosahedron();
        let f = faces[tile.face as usize];
        let n = self.n as f64;
        let (i, j) = (tile.i as f64, tile.j as f64);
        let base = (v[f[0]] * (n - i - j) + v[f[1]] * i + v[f[2]] * j) / n;
        (base, (v[f[1]] - v[f[0]]) / n, (v[f[2]] - v[f[0]]) / n)
    }

    /// How many lattice steps of `tile`'s own basis the direction `d`
    /// stands from its middle: the INVERSE of `basis`, so a window of
    /// tiles addressed by `(u, v)` in a shader can be asked where any tile
    /// of the planet sits in it, or whether it sits in it at all.
    ///
    /// `basis` says the tile `(u, v)` away is `mid + u * e1 + v * e2`
    /// normalised, so this solves `e1 u + e2 v - d t = -mid` for the three
    /// by the inverse of the matrix those columns make. It is exact
    /// wherever `basis` is, which is the anchor's own face, and carries
    /// `basis`'s own error across an edge.
    pub fn steps(&self, tile: Tile, d: DVec3) -> (f64, f64) {
        let (mid, e1, e2) = self.basis(tile);
        let m = glam::DMat3::from_cols(e1, e2, -d);
        if m.determinant().abs() < 1e-18 {
            return (0.0, 0.0);
        }
        let x = m.inverse() * -mid;
        (x.x, x.y)
    }

    /// Every tile whose middle is within `angle` radians of a direction,
    /// found by walking out from the tile under it. A disc of tiles is what
    /// the near tier draws, and it is bounded by the angle and never by the
    /// planet.
    pub fn disc(&self, centre: DVec3, angle: f64) -> Vec<Tile> {
        let centre = centre.normalize_or(DVec3::Z);
        let cos = angle.cos();
        let first = self.at(centre);
        let mut seen = vec![first];
        let mut queue = vec![first];
        while let Some(t) = queue.pop() {
            for nb in self.round(t) {
                if seen.contains(&nb) || self.dir(nb).dot(centre) < cos {
                    continue;
                }
                seen.push(nb);
                queue.push(nb);
            }
        }
        seen.sort();
        seen
    }
}

/// East and north at a direction, for putting the tiles round one in order.
fn frame(up: DVec3) -> (DVec3, DVec3) {
    let pole = if up.y.abs() < 0.9 { DVec3::Y } else { DVec3::X };
    let east = pole.cross(up).normalize();
    (east, up.cross(east).normalize())
}

/// A direction as weights on the three corners of a face, by Cramer's rule.
/// The weights are what the lattice is written in, and they are positive
/// exactly where the direction is inside the face.
fn barycentric(a: DVec3, b: DVec3, c: DVec3, d: DVec3) -> (f64, f64, f64) {
    let det = a.dot(b.cross(c));
    if det.abs() < f64::EPSILON {
        return (1.0, 0.0, 0.0);
    }
    (
        d.dot(b.cross(c)) / det,
        a.dot(d.cross(c)) / det,
        a.dot(b.cross(d)) / det,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every tile of a small grid, for the tests that want them all.
    fn all(grid: Grid) -> Vec<Tile> {
        let mut out = Vec::new();
        for face in 0..20u8 {
            for i in 0..=grid.n {
                for j in 0..=(grid.n - i) {
                    if let Some(t) = grid.canonical(face, i as i64, j as i64) {
                        if !out.contains(&t) {
                            out.push(t);
                        }
                    }
                }
            }
        }
        out
    }

    #[test]
    fn the_tiles_of_a_grid_are_ten_n_squared_and_two() {
        for n in [1u32, 2, 3, 5] {
            let grid = Grid::new(n);
            assert_eq!(
                all(grid).len() as u64,
                grid.count(),
                "a grid of {n} has the wrong count"
            );
        }
    }

    #[test]
    fn a_tile_is_the_tile_under_its_own_middle() {
        let grid = Grid::new(8);
        for t in all(grid) {
            let back = grid.at(grid.dir(t));
            assert_eq!(back, t, "{t:?} came back as {back:?}");
        }
    }

    #[test]
    fn a_point_on_an_edge_is_one_tile_from_either_face() {
        let grid = Grid::new(6);
        let (_, faces) = icosahedron();
        for (face, _) in faces.iter().enumerate() {
            for k in 0..=grid.n {
                // The three edges of the face, as lattice points on them.
                for (i, j) in [(k, 0), (0, k), (k, grid.n - k)] {
                    let mine = grid.canonical(face as u8, i as i64, j as i64).unwrap();
                    let theirs = grid.at(grid.point(face as u8, i as f64, j as f64));
                    assert_eq!(mine, theirs, "an edge point read two ways");
                }
            }
        }
    }

    #[test]
    fn twelve_tiles_are_pentagons_and_the_rest_hexagons() {
        let grid = Grid::new(5);
        let mut fives = 0;
        for t in all(grid) {
            let round = grid.round(t);
            match round.len() {
                5 => fives += 1,
                6 => {}
                other => panic!("{t:?} has {other} neighbours"),
            }
            // And a neighbour has this tile back.
            for nb in round {
                assert!(
                    grid.round(nb).contains(&t),
                    "{t:?} and {nb:?} disagree about being neighbours"
                );
            }
        }
        assert_eq!(fives, 12, "an icosahedron has twelve corners");
    }

    #[test]
    fn a_neighbour_is_about_a_tile_away_and_a_corner_half_of_that() {
        let radius = 1000.0;
        let grid = Grid::new(16);
        let spacing = grid.spacing(radius);
        for t in all(grid).into_iter().take(200) {
            let c = grid.dir(t) * radius;
            for nb in grid.round(t) {
                let d = (grid.dir(nb) * radius - c).length();
                assert!(
                    d > spacing * 0.7 && d < spacing * 1.3,
                    "a neighbour is {d} m off against a spacing of {spacing}"
                );
            }
            for corner in grid.corners(t) {
                let d = (corner * radius - c).length();
                assert!(
                    d > spacing * 0.3 && d < spacing * 0.8,
                    "a corner is {d} m off against a spacing of {spacing}"
                );
            }
        }
    }

    #[test]
    fn a_hexagon_closes_round_its_middle() {
        let grid = Grid::new(9);
        for t in all(grid).into_iter().take(120) {
            let c = grid.dir(t);
            let corners = grid.corners(t);
            assert!(corners.len() == 5 || corners.len() == 6);
            // The corners go round the middle once: the angles between one
            // and the next add up to a turn.
            let (east, north) = frame(c);
            let mut total = 0.0;
            for k in 0..corners.len() {
                let a = corners[k];
                let b = corners[(k + 1) % corners.len()];
                let pa = a.dot(north).atan2(a.dot(east));
                let pb = b.dot(north).atan2(b.dot(east));
                let mut step = pb - pa;
                while step <= -std::f64::consts::PI {
                    step += std::f64::consts::TAU;
                }
                while step > std::f64::consts::PI {
                    step -= std::f64::consts::TAU;
                }
                assert!(step > 0.0, "the corners are not in order round the tile");
                total += step;
            }
            assert!(
                (total - std::f64::consts::TAU).abs() < 1e-9,
                "the corners went round {total} rather than a turn"
            );
        }
    }

    /// The steps of a window off `basis`, and the tile each lands in.
    fn window(grid: Grid, centre: DVec3, span: i64) -> (Vec<DVec3>, Vec<Tile>) {
        let anchor = grid.at(centre);
        let (mid, e1, e2) = grid.basis(anchor);
        let (mut steps, mut tiles) = (Vec::new(), Vec::new());
        for u in -span..=span {
            for v in -span..=span {
                if u * u + u * v + v * v > span * span {
                    continue;
                }
                let step = (mid + e1 * u as f64 + e2 * v as f64).normalize();
                tiles.push(grid.at(step));
                steps.push(step);
            }
        }
        (steps, tiles)
    }

    /// How far the window's steps are from the middles of the tiles they
    /// land in, worst case metres, and how many tiles two steps had to
    /// share. A step that shares a tile is a tile drawn twice and a tile
    /// of the disc drawn not at all.
    fn window_error(grid: Grid, radius: f64, centre: DVec3, span: i64) -> (f64, usize) {
        let (steps, tiles) = window(grid, centre, span);
        let mut worst: f64 = 0.0;
        for (step, tile) in steps.iter().zip(tiles.iter()) {
            worst = worst.max((grid.dir(*tile) - *step).length() * radius);
        }
        let mut sorted = tiles.clone();
        sorted.sort();
        sorted.dedup();
        (worst, tiles.len() - sorted.len())
    }

    #[test]
    fn a_window_of_steps_is_the_grids_own_tiles() {
        // What `tiers.wgsl` draws, against what the grid says: exact on the
        // anchor's own face, off centre once the window crosses onto the
        // next one, and short of tiles at an icosahedron corner, where five
        // faces meet and a square window is six ways round. The numbers are
        // what the commit message carries.
        let radius = 5_000.0;
        let grid = Grid::for_tile(radius, 1.0);
        let spacing = grid.spacing(radius);
        let (v, faces) = icosahedron();
        let f = faces[0];
        let span = 24;
        for (what, centre) in [
            ("inside a face", (v[f[0]] + v[f[1]] + v[f[2]]).normalize()),
            ("across an edge", (v[f[0]] + v[f[1]]).normalize()),
            ("at a corner", v[f[0]]),
        ] {
            let (off, shared) = window_error(grid, radius, centre, span);
            println!("a window of {span} tiles of {spacing:.3} m, {what}: {off:.6} m off the middles, {shared} tiles shared");
            match what {
                // A lattice point is linear in its face's corners and only
                // the normalise is not, so stepping before the normalise IS
                // `point`, and what is left is the float's own rounding.
                "inside a face" => {
                    assert!(off < 1.0e-6, "{what}: {off} m");
                    assert_eq!(shared, 0, "{what}");
                }
                // The next face is a different plane, so the steps land off
                // the middles; they still land one to a tile.
                "across an edge" => {
                    assert!(
                        off > spacing * 0.05 && off < spacing * 0.6,
                        "{what}: {off} m"
                    );
                    assert_eq!(shared, 0, "{what}");
                }
                // And a corner is the one place the window is not the
                // lattice: it is drawn here and it is wrong here, and that
                // is written down rather than hidden.
                _ => assert!(shared > 0, "{what} lost nothing?"),
            }
        }
    }

    #[test]
    fn a_disc_holds_what_is_inside_it_and_nothing_else() {
        let grid = Grid::new(64);
        let centre = DVec3::new(0.3, 0.7, 0.4).normalize();
        let angle = 0.05;
        let tiles = grid.disc(centre, angle);
        for t in &tiles {
            assert!(
                grid.dir(*t).dot(centre) >= angle.cos() - 1e-12,
                "a tile outside the disc came back"
            );
        }
        // About the area it covers: a disc of `angle` over a tile's own
        // share of the sphere.
        let cap = 2.0 * std::f64::consts::PI * (1.0 - angle.cos());
        let share = 4.0 * std::f64::consts::PI / grid.count() as f64;
        let want = cap / share;
        let got = tiles.len() as f64;
        assert!(
            got > want * 0.7 && got < want * 1.3,
            "{got} tiles against about {want}"
        );
    }

    #[test]
    fn a_grid_is_chosen_by_the_tile_it_wants() {
        let radius = 5000.0;
        let grid = Grid::for_tile(radius, 0.5);
        let s = grid.spacing(radius);
        assert!((s - 0.5).abs() < 0.01, "a half metre tile came out {s} m");
        // And a planet is more tiles than anything could hold, which is the
        // reason this module addresses rather than lists.
        assert!(grid.count() > 1_000_000_000, "{} tiles", grid.count());
    }

    #[test]
    fn a_window_of_steps_is_found_again_by_steps() {
        // What the app does to put a raised tile into the shader's own
        // window: walk the window forward with `basis` and back with
        // `steps`, and they have to be the same two whole numbers.
        let grid = Grid::for_tile(1_000_000.0, 1.0);
        let anchor = grid.at(DVec3::new(0.21, 0.83, 0.52).normalize());
        let (mid, e1, e2) = grid.basis(anchor);
        let mut worst = 0.0_f64;
        for u in -24..=24 {
            for v in -24..=24 {
                let dir = (mid + e1 * u as f64 + e2 * v as f64).normalize();
                let (a, b) = grid.steps(anchor, dir);
                worst = worst.max((a - u as f64).abs()).max((b - v as f64).abs());
            }
        }
        assert!(worst < 1e-6, "a window round trip lost {worst} of a step");
    }
}
