//! A town's own GROUND: the country it stands on, graded to what a
//! street can climb, and cut and never filled.
//!
//! A town levelled its ground to ONE height right across its outline,
//! and a site was accepted only where the natural ground fell no more
//! than `CUT` across the whole of it. That is what kept a city small,
//! and it was measured rather than reasoned about
//! (`examples/town_ground.rs`): over 240 land candidates on the harness
//! body, the median fall across a 537 m town's outline is 99 m and not
//! one candidate in 240 falls under fifteen, so the 130 cities the atlas
//! found were the rare plains out of twenty thousand directions. At three
//! times the radius the median fall is 282 m and a flat pad is simply
//! not a thing this planet has.
//!
//! So a town's ground FOLLOWS the country. It is surveyed on a grid of
//! `STEP`, each node the lowest bare ground over its own cell; lowered
//! until no two neighbouring nodes differ by more than `GRADE` over the
//! run between them, which only ever lowers and so only ever cuts; sunk
//! `DIP` more; and drawn between nodes as a quadratic B-spline, which is
//! smooth, never steeper along either of the town's own axes than the
//! steepest node step, and inside the range of the nodes round it. On
//! the same 240 candidates at three times the radius, 33% of sites cut
//! no deeper than `CUT` anywhere and 58% cut no deeper over four fifths
//! of the town.
//!
//! A B-SPLINE and not a bilinear grid, because a street is laid on this
//! in straight pieces a few metres long and a bilinear grid has a crease
//! along every node line: at eight per cent either side of a ridge, a
//! 2.8 m piece spanning one is buried 11 cm in the middle. The spline's
//! curvature is bounded by `2 GRADE / STEP`, which puts the same piece
//! within three millimetres of the ground everywhere.

use super::frame_at;
use crate::field::Planet;
use glam::{DVec2, DVec3};

/// The steepest a town's ground may be along either axis of its own grid,
/// as a slope. `docs/civil-engineering.md` has the table: eight per cent
/// is an urban collector on rolling ground, and every street here runs
/// along one of those two axes. Across a diagonal the spline can reach
/// the square root of two of it, which is what the planet's slope bound
/// carries (`Sites::grade`).
pub const GRADE: f64 = 0.08;
/// The survey grid, metres. A block and a street is 48.5, so a node is
/// about a block: the ground under one block is what one node says.
pub const STEP: f64 = 50.0;
/// How far under its own nodes a town's ground is laid, metres.
///
/// A node is the lowest of the bare ground sampled at half a step over
/// its cell, and the spline between nodes is an average, so a dip
/// narrower than the samples or a valley tighter than the spline can
/// stand under the grade: a street or a lot floating over it. Measured at
/// five metres over twelve accepted big towns, the worst is 0.38 to
/// 0.71 m, on under 0.6% of the ground. This is that worst, so the ground
/// is cut a little deeper everywhere rather than left hanging anywhere.
pub const DIP: f64 = 0.75;
/// How far a point reads nodes off itself, in fine samples of half a
/// step either way: the spline's three nodes are a step and a half.
const READ: usize = 3;

/// A town's graded ground: a grid of nodes in the town's own frame, east
/// and north of its middle.
#[derive(Clone, Debug, PartialEq)]
pub struct Grade {
    /// The town's middle, and east and north there: the frame the grid is
    /// laid in, which is the one `town::lot_frame` places everything in.
    pub dir: DVec3,
    pub east: DVec3,
    pub north: DVec3,
    /// The planet's radius, which is what turns a direction into metres
    /// on the grid.
    radius: f64,
    /// Nodes either side of the middle.
    half: usize,
    /// Node heights, metres over the mean radius, row major from the
    /// south west: `(2 half + 1)` squared of them.
    nodes: Vec<f64>,
    /// The lowest and the highest of the nodes the town can read, which
    /// the spline never leaves.
    low: f64,
    high: f64,
}

/// What a survey found: the grade, the deepest it cuts into the natural
/// ground anywhere the town levels outright, and the lowest it stands
/// there.
pub struct Graded {
    pub grade: Grade,
    pub cut: f64,
    pub low: f64,
}

impl Grade {
    /// Level ground at `h` everywhere: a town laid with no planet under
    /// it, which is what a test fixture is.
    pub fn flat(dir: DVec3, h: f64, radius: f64) -> Grade {
        let (east, north) = frame_at(dir);
        Grade {
            dir,
            east,
            north,
            radius,
            half: 0,
            nodes: vec![h],
            low: h,
            high: h,
        }
    }

    /// Survey the ground a town at `dir` levels and grade it.
    ///
    /// `edge` is how far the town levels outright along the bearing of a
    /// direction, `far` the furthest it does along any, and `skirt` how
    /// far past that its blend back to the relief reaches. The ground is
    /// the BARE planet's, because a town's ground is what was there
    /// before it and a planet carrying other sites would grade this one
    /// to theirs.
    ///
    /// `floor` is the lowest a node may be, metres over the mean radius:
    /// the sea's own level and the few metres a town stands over it. A
    /// coast is the ground a city most wants and the sea floor off it is
    /// the lowest ground there is, so a node held to it drags the grade
    /// of a whole waterfront under the water. Held to the floor instead,
    /// it changes nothing offshore, because the ground is the LOWER of
    /// the grade and the country and the sea floor is lower still.
    pub fn survey(
        bare: &Planet,
        dir: DVec3,
        far: f64,
        skirt: f64,
        floor: f64,
        edge: &dyn Fn(DVec3) -> f64,
    ) -> Graded {
        let (east, north) = frame_at(dir);
        let radius = bare.radius;
        let half = ((far + skirt) / STEP).ceil() as usize + READ + 1;
        let n = 2 * half + 1;
        let at =
            |x: f64, z: f64| (dir + east * (x / radius) + north * (z / radius)).normalize_or(dir);
        // The bare ground at half a step: fine index f is at
        // `(f - n) STEP / 2`, so node k is fine `2k + 1`.
        let m = 2 * n + 1;
        let place = |f: usize| (f as f64 - n as f64) * STEP * 0.5;
        let keep = held(n, m, &|fi, fj| {
            let (x, z) = (place(fi), place(fj));
            x.hypot(z) <= edge(at(x, z)) + skirt
        });
        let mut fine = vec![f64::NAN; m * m];
        for fj in 0..m {
            for fi in 0..m {
                // Only where a held node's cell reaches.
                let near = |f: usize| [f.saturating_sub(1) / 2, f / 2];
                let wanted = near(fj)
                    .iter()
                    .any(|&j| near(fi).iter().any(|&i| i < n && j < n && keep[j * n + i]));
                if wanted {
                    fine[fj * m + fi] = bare.surface(at(place(fi), place(fj))).0;
                }
            }
        }
        let mut nodes = vec![f64::INFINITY; n * n];
        for j in 0..n {
            for i in 0..n {
                if !keep[j * n + i] {
                    continue;
                }
                let (fi, fj) = (2 * i + 1, 2 * j + 1);
                let mut lo = f64::INFINITY;
                for b in fj - 1..=fj + 1 {
                    for a in fi - 1..=fi + 1 {
                        let v = fine[b * m + a];
                        if v.is_finite() {
                            lo = lo.min(v);
                        }
                    }
                }
                nodes[j * n + i] = lo.max(floor);
            }
        }
        envelope(&mut nodes, n);
        let (mut low, mut high) = (f64::INFINITY, f64::NEG_INFINITY);
        for v in nodes.iter_mut() {
            if v.is_finite() {
                *v -= DIP;
                low = low.min(*v);
                high = high.max(*v);
            }
        }
        // A node past what anything reads is never read, but it is given
        // a number, so nothing downstream can meet an infinity.
        for v in nodes.iter_mut() {
            if !v.is_finite() {
                *v = low;
            }
        }
        let grade = Grade {
            dir,
            east,
            north,
            radius,
            half,
            nodes,
            low,
            high,
        };
        let (cut, least) = grade.cut(&fine, m, n, edge);
        Graded {
            grade,
            cut,
            low: least,
        }
    }

    /// The deepest this grade cuts into the half step samples it was
    /// surveyed from, anywhere the town levels outright, and the lowest
    /// it stands there.
    fn cut(&self, fine: &[f64], m: usize, n: usize, edge: &dyn Fn(DVec3) -> f64) -> (f64, f64) {
        let (mut worst, mut least) = (0.0f64, f64::INFINITY);
        for fj in 0..m {
            for fi in 0..m {
                let bare = fine[fj * m + fi];
                if !bare.is_finite() {
                    continue;
                }
                let x = (fi as f64 - n as f64) * STEP * 0.5;
                let z = (fj as f64 - n as f64) * STEP * 0.5;
                if x.hypot(z) > edge(self.dir_of(x, z)) {
                    continue;
                }
                let g = self.at(x, z);
                worst = worst.max(bare - g);
                least = least.min(g);
            }
        }
        (worst, least.min(self.at(0.0, 0.0)))
    }

    /// The direction a point of the grid is at, metres east and north of
    /// the middle: `town::lot_frame`'s own mapping.
    pub fn dir_of(&self, x: f64, z: f64) -> DVec3 {
        (self.dir + self.east * (x / self.radius) + self.north * (z / self.radius))
            .normalize_or(self.dir)
    }

    /// The ground at a point of the grid, metres over the mean radius.
    pub fn at(&self, x: f64, z: f64) -> f64 {
        self.eval(x, z).0
    }

    /// How steeply the ground climbs at a point of the grid, east and
    /// north.
    pub fn slope(&self, x: f64, z: f64) -> DVec2 {
        self.eval(x, z).1
    }

    /// The ground at a direction: the point of the grid it is at, which
    /// is `dir_of` inverted.
    pub fn at_dir(&self, dir: DVec3) -> f64 {
        let t = dir.dot(self.dir);
        if t <= 0.0 {
            return self.low;
        }
        let x = self.radius * dir.dot(self.east) / t;
        let z = self.radius * dir.dot(self.north) / t;
        self.at(x, z)
    }

    /// The highest the ground stands within `span` metres of a
    /// direction, metres over the mean radius: the highest node the
    /// spline can read anywhere in that square, which it never passes.
    ///
    /// What a box over a town is ruled AIR against. Against the town's
    /// own highest point instead, a box over the low side of a city a
    /// couple of hundred metres from end to end was never ruled and was
    /// sampled whole.
    pub fn high_near(&self, dir: DVec3, span: f64) -> f64 {
        let n = 2 * self.half + 1;
        if n == 1 {
            return self.nodes[0];
        }
        let t = dir.dot(self.dir);
        if t <= 0.0 {
            return self.high;
        }
        let x = self.radius * dir.dot(self.east) / t;
        let z = self.radius * dir.dot(self.north) / t;
        let reach = span / STEP + 2.0;
        let index = |v: f64| v / STEP + self.half as f64;
        let (i0, i1) = (index(x) - reach, index(x) + reach);
        let (j0, j1) = (index(z) - reach, index(z) + reach);
        if i1 < 0.0 || j1 < 0.0 || i0 > (n - 1) as f64 || j0 > (n - 1) as f64 {
            return self.high;
        }
        let clamp = |v: f64| v.clamp(0.0, (n - 1) as f64) as usize;
        let mut top = f64::NEG_INFINITY;
        for j in clamp(j0.floor())..=clamp(j1.ceil()) {
            for i in clamp(i0.floor())..=clamp(i1.ceil()) {
                top = top.max(self.nodes[j * n + i]);
            }
        }
        top
    }

    /// The lowest and the highest the ground stands anywhere the town
    /// reads it, metres over the mean radius.
    pub fn low(&self) -> f64 {
        self.low
    }

    pub fn high(&self) -> f64 {
        self.high
    }

    /// The spline and its gradient at a point of the grid.
    fn eval(&self, x: f64, z: f64) -> (f64, DVec2) {
        let n = 2 * self.half + 1;
        if n == 1 {
            return (self.nodes[0], DVec2::ZERO);
        }
        let (u, v) = (x / STEP + self.half as f64, z / STEP + self.half as f64);
        let (i, j) = (u.round(), v.round());
        let (wu, du) = basis(u - i);
        let (wv, dv) = basis(v - j);
        let node = |a: f64, b: f64| {
            let a = (a as i64).clamp(0, n as i64 - 1) as usize;
            let b = (b as i64).clamp(0, n as i64 - 1) as usize;
            self.nodes[b * n + a]
        };
        let (mut h, mut gx, mut gz) = (0.0, 0.0, 0.0);
        for (kb, (w_b, d_b)) in wv.iter().zip(dv).enumerate() {
            for (ka, (w_a, d_a)) in wu.iter().zip(du).enumerate() {
                let g = node(i + ka as f64 - 1.0, j + kb as f64 - 1.0);
                h += w_a * w_b * g;
                gx += d_a * w_b * g;
                gz += w_a * d_b * g;
            }
        }
        (h, DVec2::new(gx, gz) / STEP)
    }
}

/// Which of `n` by `n` nodes are HELD to the grade: those within reach of
/// a point the town reaches at all, levelled or blended, which is what
/// the spline reads there. `reached` answers for a fine sample of the
/// `m` by `m` half step grid. Nothing else is surveyed, so a gorge past
/// a town's squeezed side cannot drag its ground down.
fn held(n: usize, m: usize, reached: &dyn Fn(usize, usize) -> bool) -> Vec<bool> {
    let mut at = vec![false; m * m];
    for fj in 0..m {
        for fi in 0..m {
            at[fj * m + fi] = reached(fi, fj);
        }
    }
    // A fine sample further than the spline reads, so a point reached
    // between two samples is still covered.
    let r = READ + 1;
    let mut keep = vec![false; n * n];
    for j in 0..n {
        for i in 0..n {
            let (fi, fj) = (2 * i + 1, 2 * j + 1);
            keep[j * n + i] = (fj.saturating_sub(r)..=(fj + r).min(m - 1))
                .any(|b| (fi.saturating_sub(r)..=(fi + r).min(m - 1)).any(|a| at[b * m + a]));
        }
    }
    keep
}

/// The weights of the uniform quadratic B-spline on the three nodes about
/// the nearest one, `t` from that node in steps, and their derivatives.
fn basis(t: f64) -> ([f64; 3], [f64; 3]) {
    (
        [
            0.5 * (0.5 - t).powi(2),
            0.75 - t * t,
            0.5 * (0.5 + t).powi(2),
        ],
        [-(0.5 - t), -2.0 * t, 0.5 + t],
    )
}

/// Lower a grid of nodes until no two neighbours differ by more than
/// `GRADE` over the run between them: the lower envelope of a cone at
/// every node, which is a chamfer distance transform with the heights as
/// its seeds. A raster pass each way is exact for the eight neighbour
/// chamfer and it is repeated until nothing moves, because it costs
/// nothing to be sure. A node at infinity is not held and holds nothing.
fn envelope(nodes: &mut [f64], n: usize) {
    let rise = [GRADE * STEP, GRADE * STEP * std::f64::consts::SQRT_2];
    let n = n as i64;
    // The half masks: the neighbours a raster pass has already visited.
    let back = [(-1, -1), (0, -1), (1, -1), (-1, 0)];
    loop {
        let mut moved = false;
        for (order, sign) in [(false, 1), (true, -1)] {
            for k in 0..n * n {
                let k = if order { n * n - 1 - k } else { k };
                let (i, j) = (k % n, k / n);
                let here = nodes[k as usize];
                if !here.is_finite() {
                    continue;
                }
                let mut v = here;
                for (di, dj) in back {
                    let (a, b) = (i + di * sign, j + dj * sign);
                    if !(0..n).contains(&a) || !(0..n).contains(&b) {
                        continue;
                    }
                    v = v.min(nodes[(b * n + a) as usize] + rise[(di != 0 && dj != 0) as usize]);
                }
                if v < here - 1e-9 {
                    nodes[k as usize] = v;
                    moved = true;
                }
            }
        }
        if !moved {
            return;
        }
    }
}

#[cfg(test)]
mod tests;
