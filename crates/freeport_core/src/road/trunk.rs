//! SHARED TRUNKS: where two roads run on one line, one of them owns it.
//!
//! Every road on a body comes off ONE Dijkstra tree (`road::route`), so
//! two roads leaving a town run on the same chain of waypoints until
//! their paths split, and each was then laid on its own: its own
//! tarmac, its own corridor and its own smoothed profile, three or four
//! deep on the trunk out of a city. The profiles differ, because
//! `smooth` envelopes each road over its own whole length, so the field
//! took the nearest arc's level and the ground under the trunk stepped
//! between them by metres; a car driving out of the port met a wall in
//! the road. Measured on the harness body: 727 of 730 road ends stand on
//! a trunk shared with another road, 1,573 pairs share waypoints and the
//! deepest share runs 23 waypoints, which is 230 km.
//!
//! The merge is at LOAD and never in the atlas: the file keeps each
//! road's own line and profile, and `merge` decides who owns what once
//! the centrelines are laid. The lowest indexed road owns a trunk; every
//! later road standing on it is SNAPPED to the owner's line there and
//! closed (no tarmac, no corridor, no mound, no lamps: the owner's are
//! there), and FORKS off onto its own line through the owner's corridor.
//!
//! The trunk's PROFILE is the highest of every road on it, and that is
//! the envelope's own rule arriving at a junction. A road pinned DOWN to
//! a lower owner has to climb back to its own baked profile past the
//! fork, and its own profile was raised to hold a grade toward its own
//! high ground: measured, the first cut of this put that whole climb on
//! the one piece past the fork and the steepest fork on the body came
//! out at 103%. A road only ever RISES (`smooth`), so the owner is raised
//! to the sharer where the sharer stands higher, eased along its own
//! length at the grade, and the sharer is pinned to it; a raise on one
//! road propagates to every road it shares a trunk with, so `merge`
//! carries the profiles round until nothing moves. What comes out is
//! one ground wherever two roads are within a corridor of each other,
//! held to the grade on every road, and a car meets no step anywhere.
//! A road that reaches its town on another's trunk has no slip of its
//! own there either, which the caller reads off `shared`.

use super::{CORRIDOR, STEEPEST};
use glam::DVec3;

/// How near another road's centreline a point has to stand to be ON
/// that road, metres: a lane and a half, where the two carriageways
/// overlap, since a carriageway is two lanes of `town::LANE`. Two roads
/// further apart than that have a verge between them and are two roads.
/// `docs/civil-engineering.md`, the junctions table.
pub const SHARE: f64 = 4.0;
/// How far over the trunk's own tarmac a forking road's tarmac is held
/// where the two overlap, metres: a surfacing overlay, which practice
/// lays at 25 to 50 mm. Two coplanar sheets of tarmac fight for the
/// depth test, and three centimetres is a lip a car does not feel.
pub const FORK_LIFT: f64 = 0.03;
/// The fewest points that make a shared run a TRUNK rather than a
/// crossing: two roads crossing at an angle stand within `SHARE` for a
/// point or two and are left as they are. Three stations is 170 m,
/// which is a junction's own length and not a shared alignment.
const LEAST: usize = 3;
/// The index's cell, metres of ground: wide enough that every segment
/// within `CORRIDOR` of a point has an end in a neighbouring cell, since
/// a piece is 85 m and half of one is 43.
const CELL: f64 = 64.0;
/// A raise under this is a rounding and not a change, metres.
const SETTLED: f64 = 1e-3;
/// The most passes the profiles are carried round. A raise crosses one
/// trunk a pass, and the deepest chain on the harness body settles in a
/// handful; the cap is what a loop has to have, and the log says when it
/// binds.
const PASSES: usize = 64;

/// One road's laying as the merge sees it: the fields of a route that a
/// shared trunk moves.
pub struct Laying<'a> {
    pub line: &'a mut Vec<DVec3>,
    pub run: &'a mut Vec<f64>,
    pub open: &'a mut Vec<bool>,
    pub graded: &'a mut Vec<bool>,
    pub lit: &'a mut Vec<bool>,
    /// Which points stand on another road's trunk or fork off it through
    /// the owner's corridor, so a census can tell a JOIN from a road
    /// that stops.
    pub trunk: &'a mut Vec<bool>,
    /// How many points at the head and at the tail stand on another
    /// road's trunk, so a slip is not spliced at an end that is not this
    /// road's own.
    pub shared: &'a mut (usize, usize),
}

/// What the merge did, for the log.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Merged {
    /// How many roads stand on another's trunk somewhere.
    pub roads: usize,
    /// How many of their points were snapped onto an owner's line.
    pub points: usize,
    /// The longest trunk one road shares, metres.
    pub longest: f64,
    /// The steepest piece touching a shared point, rise over run.
    pub steepest: f64,
    /// How many passes it took the profiles to settle; `PASSES` means
    /// they had not.
    pub passes: usize,
}

/// Where a point of a later road stands against the roads before it:
/// which road, which segment of its line, how far along it, and how far
/// off it in metres.
#[derive(Clone, Copy, Debug)]
struct Near {
    road: usize,
    seg: usize,
    t: f64,
    off: f64,
}

/// One point of a road that stands on another's ground: which point,
/// where on the owner, and how far over the owner's tarmac it is held.
struct Pin {
    road: usize,
    k: usize,
    at: Near,
    lip: f64,
}

/// A direction's cell in the index, packed so a sorted list of them is
/// searchable: three 21 bit lattice coordinates over the unit cube.
fn cell(dir: DVec3, radius: f64) -> u64 {
    let size = CELL / radius;
    let q = |v: f64| (((v + 1.0) / size).floor().max(0.0) as u64).min((1 << 21) - 1);
    (q(dir.x) << 42) | (q(dir.y) << 21) | q(dir.z)
}

/// The nearest point of the great circle chord from `a` to `b` to `p`,
/// as the share along the chord and the point itself on the sphere.
/// `path` snaps a place onto the network with it too.
pub(super) fn project(p: DVec3, a: DVec3, b: DVec3) -> (f64, DVec3) {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-30)).clamp(0.0, 1.0);
    (t, (a + ab * t).normalize_or(a))
}

/// Every point of every road, sorted by cell, so the roads near a point
/// are a binary search and a walk.
fn index(lines: &[Vec<DVec3>], radius: f64) -> Vec<(u64, u32, u32)> {
    let mut out: Vec<(u64, u32, u32)> = lines
        .iter()
        .enumerate()
        .flat_map(|(r, line)| {
            line.iter()
                .enumerate()
                .map(move |(k, d)| (cell(*d, radius), r as u32, k as u32))
        })
        .collect();
    out.sort_unstable();
    out
}

/// The nearest segment of any road before `road` to `p`, within
/// `CORRIDOR`, off the index; the lowest road on a tie, so the answer
/// is the same whatever order the index walks.
fn nearest(
    p: DVec3,
    road: usize,
    lines: &[Vec<DVec3>],
    index: &[(u64, u32, u32)],
    radius: f64,
) -> Option<Near> {
    let size = CELL / radius;
    let mut best: Option<Near> = None;
    for dx in -1..=1 {
        for dy in -1..=1 {
            for dz in -1..=1 {
                let at = p + DVec3::new(dx as f64, dy as f64, dz as f64) * size;
                let key = cell(at, radius);
                let from = index.partition_point(|e| e.0 < key);
                for &(k, r, i) in &index[from..] {
                    if k != key {
                        break;
                    }
                    let (r, i) = (r as usize, i as usize);
                    if r >= road {
                        continue;
                    }
                    let line = &lines[r];
                    for seg in [i.wrapping_sub(1), i] {
                        let (Some(a), Some(b)) = (line.get(seg), line.get(seg.wrapping_add(1)))
                        else {
                            continue;
                        };
                        let (t, q) = project(p, *a, *b);
                        let off = (q - p).length() * radius;
                        let better =
                            best.is_none_or(|n| off < n.off || (off == n.off && r < n.road));
                        if off < CORRIDOR && better {
                            best = Some(Near {
                                road: r,
                                seg,
                                t,
                                off,
                            });
                        }
                    }
                }
            }
        }
    }
    best
}

/// The owner's height at a projection, off its CURRENT run, so a road
/// pinned to a road that was itself pinned reads the trunk's one
/// profile.
fn height(lanes: &[Laying<'_>], n: &Near) -> f64 {
    let run = &lanes[n.road].run;
    let (a, b) = (
        run.get(n.seg).copied().unwrap_or(0.0),
        run.get(n.seg + 1).copied().unwrap_or(0.0),
    );
    a * (1.0 - n.t) + b * n.t
}

/// The ground between two consecutive points of a line, metres.
fn gap(line: &[DVec3], k: usize, radius: f64) -> f64 {
    line[k].angle_between(line[k + 1]) * radius
}

/// The grade ENVELOPE a road is baked with, applied again: forward holds
/// the DESCENT inside `STEEPEST` and backward the ASCENT, one pass each,
/// which is exact, and it only ever RAISES a point. `smooth` is its
/// caller at the bake and the merge its caller at load, so a road held
/// to the grade is held to it by one rule. It says whether anything
/// moved by more than a rounding.
pub(crate) fn envelope(run: &mut [f64], gap: &[f64]) -> bool {
    let mut moved = false;
    for k in 1..run.len() {
        let floor = run[k - 1] - STEEPEST * gap[k - 1];
        if run[k] < floor {
            moved |= floor - run[k] > SETTLED;
            run[k] = floor;
        }
    }
    for k in (0..run.len().saturating_sub(1)).rev() {
        let floor = run[k + 1] - STEEPEST * gap[k];
        if run[k] < floor {
            moved |= floor - run[k] > SETTLED;
            run[k] = floor;
        }
    }
    moved
}

/// The maximal runs of a road's points within `SHARE` of an earlier
/// road, each at least `LEAST` long, with the FORK zone either side of
/// each: past the run while still inside the owner's corridor, where the
/// ground is the owner's own flat. `(k0, k1, f0, f1)`, inclusive.
///
/// A fork zone stops at the neighbouring run's own, so every point is
/// pinned ONCE. Extended while the point was inside any owner's
/// corridor, a run's fork ran straight through the next run on the same
/// road and pinned that run's interior a second time with the fork's
/// lip: a point pinned once at the trunk's height and once a lip over
/// it asked its owner for the lip every pass, and the profiles rose
/// three centimetres a pass to the cap.
fn runs(near: &[Option<Near>]) -> Vec<(usize, usize, usize, usize)> {
    let n = near.len();
    let mut on: Vec<(usize, usize)> = Vec::new();
    let mut k0 = 0;
    while k0 < n {
        if !near[k0].is_some_and(|q| q.off < SHARE) {
            k0 += 1;
            continue;
        }
        let mut k1 = k0;
        while k1 + 1 < n && near[k1 + 1].is_some_and(|q| q.off < SHARE) {
            k1 += 1;
        }
        if k1 + 1 - k0 >= LEAST {
            on.push((k0, k1));
        }
        k0 = k1 + 1;
    }
    let mut out = Vec::with_capacity(on.len());
    let mut taken = 0;
    for (i, &(k0, k1)) in on.iter().enumerate() {
        let (mut f0, mut f1) = (k0, k1);
        while f0 > taken && near[f0 - 1].is_some() {
            f0 -= 1;
        }
        let stop = on.get(i + 1).map_or(n, |next| next.0);
        while f1 + 1 < stop && near[f1 + 1].is_some() {
            f1 += 1;
        }
        taken = f1 + 1;
        out.push((k0, k1, f0, f1));
    }
    out
}

/// Snap one run `k0..=k1` of `road` onto its owners' lines, close it,
/// flag the fork `f0..=f1` round it, and pin every point of both to the
/// owner's ground: the trunk's length.
fn snap(
    lanes: &mut [Laying<'_>],
    road: usize,
    near: &[Option<Near>],
    (k0, k1, f0, f1): (usize, usize, usize, usize),
    pins: &mut Vec<Pin>,
    radius: f64,
) -> f64 {
    let n = near.len();
    let on = |k: usize| (k0..=k1).contains(&k);
    // The owners' own line at every point of the run, read before this
    // road is touched.
    let placed: Vec<DVec3> = (k0..=k1)
        .map(|k| {
            let q = near[k].expect("a run point is inside an owner's corridor by construction");
            let owner = &lanes[q.road];
            let (a, b) = (owner.line[q.seg], owner.line[q.seg + 1]);
            (a + (b - a) * q.t).normalize_or(a)
        })
        .collect();
    let (head, tail) = (k0 == 0, k1 + 1 == n);
    let lane = &mut lanes[road];
    for (k, spot) in (k0..=k1).zip(placed) {
        lane.line[k] = spot;
    }
    for (k, q) in near.iter().enumerate().take(f1 + 1).skip(f0) {
        if on(k) && k0 < k && k < k1 {
            lane.open[k] = false;
        }
        lane.graded[k] = false;
        lane.lit[k] = false;
        lane.trunk[k] = true;
        // On the trunk's own tarmac along the run, and a hair over it at
        // the run's ends and through the fork, where this road's tarmac
        // is drawn over the owner's.
        let lip = if on(k) && k != k0 && k != k1 {
            0.0
        } else {
            FORK_LIFT
        };
        let at = q.expect("a fork zone point is inside an owner's corridor by construction");
        pins.push(Pin { road, k, at, lip });
    }
    if head {
        lane.shared.0 = k1 + 1;
        lane.open[k0] = false;
    }
    if tail {
        lane.shared.1 = n - k0;
        lane.open[k1] = false;
    }
    (k0..k1).map(|k| gap(lane.line, k, radius)).sum()
}

/// One pass of carrying the highest profile round: every road eased to
/// the grade, every owner raised under a pin that stands higher than
/// its ground there, and every pin set on its owner's ground. Whether
/// anything moved.
///
/// An owner is raised by the DIFFERENCE at the pin, spread over the two
/// ends of the segment by the interpolation's own weights (least
/// squares), so its ground at the pin comes out exactly where the pin
/// asked and a pin set there asks for nothing next pass. Raised to the
/// pin's own height instead, a segment sloping through the pin
/// flattened a little more every pass and never settled; and raised at
/// both ends by the whole difference, a pin standing at a station lifted
/// the next station too, the next pin along lifted it again, and a ten
/// point fork came out two metres over what any road on it stood at.
/// By the weights, a pin at a station raises that station alone.
///
/// The owners are raised HIGHEST ROAD FIRST and the pins set lowest
/// first, because an owner's point can itself be a pin on a road below
/// it. Raised in road order, a demand road C put on road B's pinned
/// point was reset by B's own pin to A before B's pin could carry it to
/// A, C's own envelope raised it again next pass, and the three chased
/// each other to the cap: measured on the harness body, 64 passes and
/// four shared pieces left at up to 47.5%. Every owner has a lower
/// index than its sharer, so walking down carries a demand the whole
/// way down its chain in one pass.
fn carry(lanes: &mut [Laying<'_>], pins: &[Pin], gaps: &[Vec<f64>]) -> bool {
    let mut moved = false;
    for (lane, gap) in lanes.iter_mut().zip(gaps) {
        moved |= envelope(lane.run, gap);
    }
    for p in pins.iter().rev() {
        let rise = lanes[p.road].run[p.k] - p.lip - height(lanes, &p.at);
        if rise > SETTLED {
            let (u, v) = (1.0 - p.at.t, p.at.t);
            let norm = (u * u + v * v).max(f64::EPSILON);
            let owner = &mut lanes[p.at.road];
            owner.run[p.at.seg] += rise * u / norm;
            owner.run[p.at.seg + 1] += rise * v / norm;
            moved = true;
        }
    }
    for p in pins {
        lanes[p.road].run[p.k] = height(lanes, &p.at) + p.lip;
    }
    moved
}

/// The steepest piece touching any pinned point, rise over run.
fn steepest(lanes: &[Laying<'_>], pins: &[Pin], gaps: &[Vec<f64>]) -> f64 {
    pins.iter()
        .flat_map(|p| [p.k.wrapping_sub(1), p.k].map(|k| (p.road, k)))
        .filter_map(|(r, k)| {
            let run = &lanes[r].run;
            Some((run.get(k.wrapping_add(1))? - run.get(k)?).abs() / gaps[r].get(k)?.max(1e-9))
        })
        .fold(0.0, f64::max)
}

/// Every road merged onto the roads before it, in index order, so a
/// trunk three roads share is owned by the first and the other two
/// stand on it; then the profiles carried round until the trunk holds
/// the highest of them everywhere and every road is at the grade.
pub fn merge(lanes: &mut [Laying<'_>], radius: f64) -> Merged {
    let lines: Vec<Vec<DVec3>> = lanes.iter().map(|l| l.line.clone()).collect();
    let index = index(&lines, radius);
    let mut done = Merged::default();
    let mut pins = Vec::new();
    for road in 1..lanes.len() {
        let near: Vec<Option<Near>> = lines[road]
            .iter()
            .map(|p| nearest(*p, road, &lines, &index, radius))
            .collect();
        let runs = runs(&near);
        done.roads += usize::from(!runs.is_empty());
        for run in runs {
            done.points += run.1 + 1 - run.0;
            let length = snap(lanes, road, &near, run, &mut pins, radius);
            done.longest = done.longest.max(length);
        }
    }
    let gaps: Vec<Vec<f64>> = lanes
        .iter()
        .map(|l| {
            (0..l.line.len().saturating_sub(1))
                .map(|k| gap(l.line, k, radius))
                .collect()
        })
        .collect();
    while done.passes < PASSES && carry(lanes, &pins, &gaps) {
        done.passes += 1;
    }
    done.steepest = steepest(lanes, &pins, &gaps);
    done
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::road::PIECE;

    const RADIUS: f64 = 1.0e6;

    /// A road as a line of points `PIECE` apart from the pole along a
    /// bearing, bent by `turn(k)` metres ACROSS at point k, at height
    /// `h(k)`.
    fn road(
        n: usize,
        turn: impl Fn(usize) -> f64,
        h: impl Fn(usize) -> f64,
    ) -> (Vec<DVec3>, Vec<f64>) {
        let line = (0..n)
            .map(|k| {
                let along = k as f64 * PIECE / RADIUS;
                let across = turn(k) / RADIUS;
                DVec3::new(along.sin(), along.cos() * across.cos(), across.sin()).normalize()
            })
            .collect();
        (line, (0..n).map(h).collect())
    }

    struct Lane {
        line: Vec<DVec3>,
        run: Vec<f64>,
        open: Vec<bool>,
        graded: Vec<bool>,
        lit: Vec<bool>,
        trunk: Vec<bool>,
        shared: (usize, usize),
    }

    fn lane((line, run): (Vec<DVec3>, Vec<f64>)) -> Lane {
        let n = line.len();
        Lane {
            line,
            run,
            open: vec![true; n],
            graded: vec![true; n],
            lit: vec![true; n],
            trunk: vec![false; n],
            shared: (0, 0),
        }
    }

    fn merge_all(lanes: &mut [Lane]) -> Merged {
        let mut layings: Vec<Laying<'_>> = lanes
            .iter_mut()
            .map(|l| Laying {
                line: &mut l.line,
                run: &mut l.run,
                open: &mut l.open,
                graded: &mut l.graded,
                lit: &mut l.lit,
                trunk: &mut l.trunk,
                shared: &mut l.shared,
            })
            .collect();
        merge(&mut layings, RADIUS)
    }

    /// A runs straight and flat at 10 m; B runs with it for thirty
    /// points at 12 m, then leaves at a widening angle, 4.5 m more a
    /// point, with its own profile at `own` metres from point 33 on.
    fn fork(own: f64) -> ([Lane; 2], Merged) {
        let a = lane(road(60, |_| 0.0, |_| 10.0));
        let b = lane(road(
            60,
            |k| (k as f64 - 29.0).max(0.0) * 4.5,
            move |k| if k < 33 { 12.0 } else { own },
        ));
        let mut lanes = [a, b];
        let done = merge_all(&mut lanes);
        (lanes, done)
    }

    fn grades(b: &Lane) -> Vec<f64> {
        (0..b.line.len() - 1)
            .map(|k| (b.run[k + 1] - b.run[k]) / (b.line[k].angle_between(b.line[k + 1]) * RADIUS))
            .collect()
    }

    fn at_the_grade(name: &str, lane: &Lane) {
        for (k, g) in grades(lane).iter().enumerate() {
            assert!(
                g.abs() <= STEEPEST + 1e-6,
                "{name}'s piece {k} climbs at {:.1}%",
                g * 100.0
            );
        }
    }

    #[test]
    fn a_road_on_anothers_trunk_stands_on_its_line_and_the_trunk_carries_the_higher_profile() {
        let ([a, b], done) = fork(12.0);
        println!("{done:?}");
        assert_eq!(done.roads, 1);
        // Within `SHARE`: k <= 29 exactly on A, k = 30 at 4.5 m is off.
        assert_eq!(done.points, 30, "{done:?}");
        assert_eq!(b.shared, (30, 0));
        assert!(done.passes < PASSES, "{done:?}");
        for k in 0..30 {
            assert!(
                (b.line[k] - a.line[k]).length() * RADIUS < 1e-6,
                "point {k} is off the trunk"
            );
        }
        for k in 0..=32 {
            assert!(
                !b.graded[k] && !b.lit[k] && b.trunk[k],
                "the trunk carries B's mound or lamps at {k}"
            );
        }
        assert!(!b.trunk[33] && a.trunk.iter().all(|t| !*t));
        // B stood two metres over A, so the trunk is at B's height and
        // A was RAISED to it: one profile, and B's is A's exactly.
        for k in 1..29 {
            assert!(!b.open[k], "B lays tarmac on the trunk at {k}");
            assert!(a.run[k] >= 12.0 - 1e-9, "A stays low at {k}: {}", a.run[k]);
            assert!(
                (b.run[k] - a.run[k]).abs() < 1e-9,
                "B is off A's profile at {k}: {} against {}",
                b.run[k],
                a.run[k]
            );
        }
        assert!(
            b.open[29] && b.open[30],
            "the transition piece is not drawn"
        );
        assert!((b.run[29] - a.run[29] - FORK_LIFT).abs() < 1e-9);
        // The fork: k = 30 (4.5 m) to k = 32 (13.5 m) are inside the
        // corridor, on A's ground and a lip, not graded, and never under
        // where B stood.
        for k in 30..=32 {
            assert!(!b.graded[k]);
            assert!(b.run[k] >= 12.0 - 1e-6, "fork point {k} at {}", b.run[k]);
        }
        assert!(b.graded[40] && b.open[40] && (b.run[40] - 12.0).abs() < 1e-9);
        at_the_grade("A", &a);
        at_the_grade("B", &b);
        // A is never LOWERED, and past the fork it is what it was.
        assert!(a.run.iter().all(|h| *h >= 10.0 - 1e-9));
        assert!((a.run[45] - 10.0).abs() < 1e-9 && (a.run[59] - 10.0).abs() < 1e-9);
        assert!(a.open.iter().all(|o| *o) && a.shared == (0, 0));
        assert!(
            (done.longest - 29.0 * PIECE).abs() < 1.0,
            "{}",
            done.longest
        );
    }

    #[test]
    fn a_road_that_climbs_off_a_trunk_lifts_the_trunk_with_it_and_both_hold_the_grade() {
        // B's own profile stands 30 m from point 33 on, twenty over A:
        // B climbs to it at the grade, which starts inside the fork,
        // and A is raised under it there and comes back down at the
        // grade past it, because two roads a corridor apart share one
        // ground.
        let ([a, b], done) = fork(30.0);
        println!("{done:?}");
        assert!(done.passes < PASSES, "{done:?}");
        at_the_grade("A", &a);
        at_the_grade("B", &b);
        for k in 0..60 {
            let was = if k < 33 { 12.0 } else { 30.0 };
            assert!(b.run[k] >= was - 1e-9, "B lowered at {k}: {}", b.run[k]);
            assert!(a.run[k] >= 10.0 - 1e-9, "A lowered at {k}: {}", a.run[k]);
        }
        for k in 1..29 {
            assert!((b.run[k] - a.run[k]).abs() < 1e-9, "B off A at {k}");
        }
        assert!(
            a.run[32] > 20.0,
            "A not lifted with B's climb: {}",
            a.run[32]
        );
        assert!((a.run[59] - 10.0).abs() < 1e-9 && (b.run[59] - 30.0).abs() < 1e-9);
        assert!((done.steepest - STEEPEST).abs() < 1e-6, "{done:?}");
    }

    #[test]
    fn a_road_crossing_another_is_left_alone_and_a_shared_tail_has_no_slip_of_its_own() {
        // C crosses A at a slant: one or two points near A's line, which
        // is a crossing and not a trunk.
        let a = lane(road(60, |_| 0.0, |_| 10.0));
        let c = lane(road(60, |k| (k as f64 - 30.0) * 40.0, |_| 20.0));
        let mut lanes = [a, c];
        let done = merge_all(&mut lanes);
        assert_eq!(done.points, 0, "{done:?}");
        assert!(lanes[1].open.iter().all(|o| *o) && lanes[1].shared == (0, 0));
        assert!(lanes[0].run.iter().all(|h| (*h - 10.0).abs() < 1e-9));
        // D arrives on A's trunk from the far end: its TAIL is shared.
        let a = lane(road(60, |_| 0.0, |_| 10.0));
        let d = lane(road(60, |k| ((30 - k.min(30)) as f64) * 4.5, |_| 10.0));
        let mut lanes = [a, d];
        let done = merge_all(&mut lanes);
        assert_eq!(lanes[1].shared, (0, 30), "{done:?}");
        assert!(!lanes[1].open[59] && lanes[1].open[30]);
    }

    #[test]
    fn a_demand_on_a_pinned_point_is_carried_down_to_the_road_that_owns_it() {
        // A straight at 10; B leaves A at point 30 at 12; C runs on B's
        // own line the whole way at 15. Past the fork C's nearest road
        // is B, and B's fork points are themselves pins on A, so C's
        // fifteen metres have to reach A THROUGH B or B is reset under
        // C every pass and nothing settles.
        let a = lane(road(60, |_| 0.0, |_| 10.0));
        let b = lane(road(60, |k| (k as f64 - 29.0).max(0.0) * 4.5, |_| 12.0));
        let c = lane(road(60, |k| (k as f64 - 29.0).max(0.0) * 4.5, |_| 15.0));
        let mut lanes = [a, b, c];
        let done = merge_all(&mut lanes);
        println!("{done:?}");
        assert!(done.passes < PASSES, "{done:?}");
        for (name, lane) in ["A", "B", "C"].iter().zip(&lanes) {
            at_the_grade(name, lane);
        }
        let (a, b, c) = (&lanes[0], &lanes[1], &lanes[2]);
        for k in 1..=32 {
            assert!(
                a.run[k] >= 15.0 - 2.0 * FORK_LIFT - 1e-9,
                "A under C's demand at {k}: {}",
                a.run[k]
            );
        }
        for k in 31..=58 {
            assert!((c.run[k] - b.run[k]).abs() < 1e-9, "C off B at {k}");
            assert!(c.run[k] >= 15.0 - 1e-9, "C lowered at {k}: {}", c.run[k]);
        }
        assert!((a.run[59] - 10.0).abs() < 1e-9);
    }

    #[test]
    fn a_road_that_leaves_a_trunk_and_comes_back_is_pinned_once_at_every_point() {
        // B runs on A, steps 10 m off it for ten points (inside the
        // corridor, outside `SHARE`) and comes back: two runs with a
        // fork zone between them that both would claim, and a point
        // pinned twice never settles.
        let a = lane(road(60, |_| 0.0, |_| 10.0));
        let b = lane(road(
            60,
            |k| if (20..=29).contains(&k) { 10.0 } else { 0.0 },
            |_| 12.0,
        ));
        let mut lanes = [a, b];
        let done = merge_all(&mut lanes);
        println!("{done:?}");
        assert_eq!(done.points, 50, "{done:?}");
        assert!(done.passes < PASSES, "{done:?}");
        let (a, b) = (&lanes[0], &lanes[1]);
        at_the_grade("A", a);
        at_the_grade("B", b);
        for k in (1..19).chain(31..59) {
            assert!((b.run[k] - a.run[k]).abs() < 1e-9, "B off A at {k}");
        }
        for k in 20..=29 {
            assert!(!b.graded[k] && b.trunk[k], "the gap is not a fork at {k}");
            assert!(
                b.run[k] >= 12.0 - 1e-9 && b.run[k] <= 12.0 + FORK_LIFT + 1e-9,
                "the gap stands at {} over A",
                b.run[k]
            );
        }
        assert!(a.run.iter().all(|h| (*h - 12.0).abs() <= FORK_LIFT + 1e-9));
    }

    #[test]
    fn a_trunk_three_roads_share_is_one_profile_through_all_of_them() {
        // A at 10, B on A's trunk at 12, C on B's original line at 15:
        // the trunk comes out at C's 15 on all three, and settles.
        let a = lane(road(60, |_| 0.0, |_| 10.0));
        let b = lane(road(60, |k| (k as f64 - 29.0).max(0.0) * 4.5, |_| 12.0));
        let c = lane(road(60, |k| (k as f64 - 19.0).max(0.0) * -4.5, |_| 15.0));
        let mut lanes = [a, b, c];
        let done = merge_all(&mut lanes);
        println!("{done:?}");
        assert_eq!(done.roads, 2, "{done:?}");
        assert!(done.passes < PASSES, "{done:?}");
        for (name, lane) in ["A", "B", "C"].iter().zip(&lanes) {
            at_the_grade(name, lane);
        }
        for k in 1..19 {
            assert!(
                lanes[0].run[k] >= 15.0 - 1e-9,
                "A at {k}: {}",
                lanes[0].run[k]
            );
            assert!(
                (lanes[1].run[k] - lanes[0].run[k]).abs() < 1e-9,
                "B off A at {k}"
            );
            assert!(
                (lanes[2].run[k] - lanes[0].run[k]).abs() < 1e-9,
                "C off A at {k}"
            );
        }
        assert!((lanes[0].run[59] - 10.0).abs() < 1e-9);
    }
}
