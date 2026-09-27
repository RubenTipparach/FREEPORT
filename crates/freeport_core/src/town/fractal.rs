//! What a town's own plan is SHAPED like, measured rather than looked
//! at: the box counting dimension of the outline the demand field cuts.
//!
//! Batty and Longley's `Fractal Cities` is where the measure comes
//! from and why it is the right one: a built up area is not a shape
//! with a length, it is a cluster whose boundary counts more boxes the
//! finer the boxes are, and the slope of that count against the box is
//! the one number that tells an oval from a city. A smooth closed
//! curve is one whatever the ladder, because halving the box doubles
//! the count; a crinkled one more than doubles it.
//!
//! It is a MEASUREMENT and not a feature, so it lives beside the tests
//! rather than in `shape.rs`: nothing the game runs ever asks it.

#![cfg(test)]

use super::shape::{
    demand, demand_at, edge, grain, grain_octaves, CAP, FADE, FRONT, GRAIN_LUMP, GRAIN_MEAN,
    GRAIN_PERSIST, GRAIN_SLICE, GRAIN_SPREAD,
};
use super::{OUTLINE, PITCH};
use crate::field::{fbm3, fbm3_rough};
use glam::DVec2;
use glam::DVec3;

/// How many cells a side the plan is sampled on. A 1,611 m city's own
/// outline over 512 cells is 13 m a cell, about a lot, so the finest
/// box on the ladder is still finer than the block grid the plan is
/// actually laid on.
const GRID: usize = 512;

/// The box sizes, in cells. The ladder spans the scales a town can
/// actually EXPRESS and no others: four cells is about one `PITCH`, and
/// sixty four is about half the town. Finer than a block the plan has
/// nothing to say, because a lot is either on its block or not; coarser
/// than half the town there are no boxes left to count. A ladder that
/// ran to the grid's own cell would be measuring how finely the demand
/// was sampled.
const LADDER: [usize; 5] = [4, 8, 16, 32, 64];

/// The rule the FRONT replaced, kept here beside the measure that
/// condemned it and nowhere else, so the before and the after are one
/// run of one binary: the grain subtracted only where it stood OVER its
/// own mean, scaled by `1 - want`, with a strength of 0.55. Half of
/// every block just inside the outline was therefore built, which is
/// what made the outline the thing an eye traced.
fn oval_era(x: f64, z: f64, radius: f64, along: DVec2, seed: u32) -> f64 {
    let want = demand_at(x, z, radius, along, seed, 0.0, 0.0);
    let bite = (grain(x, z, radius, seed) * 0.5).clamp(0.0, 1.0);
    want - 0.55 * (1.0 - want).clamp(0.0, 1.0) * bite
}

/// Which cells of the grid are TOWN under a demand, and how far out
/// each stands on the bare outline (`want`), which is what the outer
/// band is read off.
struct Plan {
    town: Vec<bool>,
    want: Vec<f64>,
    cell: f64,
}

fn mask(radius: f64, demand: &dyn Fn(f64, f64) -> f64, bare: &dyn Fn(f64, f64) -> f64) -> Plan {
    let half = radius * OUTLINE;
    let cell = 2.0 * half / GRID as f64;
    let mut town = vec![false; GRID * GRID];
    let mut want = vec![0.0; GRID * GRID];
    for j in 0..GRID {
        for i in 0..GRID {
            let x = -half + (i as f64 + 0.5) * cell;
            let z = -half + (j as f64 + 0.5) * cell;
            town[j * GRID + i] = demand(x, z) > 0.0;
            want[j * GRID + i] = bare(x, z);
        }
    }
    Plan { town, want, cell }
}

/// The four neighbours of a cell that are on the grid.
fn around(i: usize, j: usize) -> impl Iterator<Item = (usize, usize)> {
    let n = GRID as isize;
    [(-1, 0), (1, 0), (0, -1), (0, 1)]
        .into_iter()
        .map(move |(di, dj)| (i as isize + di, j as isize + dj))
        .filter(move |&(a, b)| (0..n).contains(&a) && (0..n).contains(&b))
        .map(|(a, b)| (a as usize, b as usize))
}

/// Every BOUNDARY: a town cell with at least one of its four neighbours
/// outside the town, the grid's own rim counting as outside, which
/// counts the holes inside a town as boundary too.
fn rim(m: &[bool]) -> Vec<bool> {
    let mut r = vec![false; GRID * GRID];
    for j in 0..GRID {
        for i in 0..GRID {
            let edge = i == 0 || j == 0 || i == GRID - 1 || j == GRID - 1;
            r[j * GRID + i] =
                m[j * GRID + i] && (edge || around(i, j).any(|(a, b)| !m[b * GRID + a]));
        }
    }
    r
}

/// The HULL: the boundary the COUNTRY sees, which is a town cell beside
/// empty ground that joins the edge of the frame through empty ground.
/// A hole in the middle of town is not on it, so this is the outline an
/// eye traces round a town from the air, and the one number that says
/// whether that outline is an oval or a city.
fn hull(m: &[bool]) -> Vec<bool> {
    let mut out = vec![false; GRID * GRID];
    let mut stack: Vec<(usize, usize)> = Vec::new();
    for k in 0..GRID {
        for (i, j) in [(k, 0), (k, GRID - 1), (0, k), (GRID - 1, k)] {
            if !m[j * GRID + i] && !out[j * GRID + i] {
                out[j * GRID + i] = true;
                stack.push((i, j));
            }
        }
    }
    while let Some((i, j)) = stack.pop() {
        for (a, b) in around(i, j) {
            if !m[b * GRID + a] && !out[b * GRID + a] {
                out[b * GRID + a] = true;
                stack.push((a, b));
            }
        }
    }
    let mut h = vec![false; GRID * GRID];
    for j in 0..GRID {
        for i in 0..GRID {
            let edge = i == 0 || j == 0 || i == GRID - 1 || j == GRID - 1;
            h[j * GRID + i] =
                m[j * GRID + i] && (edge || around(i, j).any(|(a, b)| out[b * GRID + a]));
        }
    }
    h
}

/// How many boxes of `s` cells hold any of it.
fn boxes(r: &[bool], s: usize) -> usize {
    let n = GRID / s;
    let mut count = 0;
    for bj in 0..n {
        for bi in 0..n {
            let any =
                (bj * s..(bj + 1) * s).any(|j| (bi * s..(bi + 1) * s).any(|i| r[j * GRID + i]));
            count += usize::from(any);
        }
    }
    count
}

/// The slope of `ln(count)` against `ln(1 / box)`, which is the
/// dimension: least squares over the whole ladder rather than the two
/// ends, because one box size that happens to straddle a feature moves
/// a two point fit and cannot move a fit of five.
fn dimension(r: &[bool]) -> f64 {
    let pts: Vec<(f64, f64)> = LADDER
        .iter()
        .map(|&s| (-(s as f64).ln(), (boxes(r, s).max(1) as f64).ln()))
        .collect();
    let n = pts.len() as f64;
    let (mx, my) = (
        pts.iter().map(|p| p.0).sum::<f64>() / n,
        pts.iter().map(|p| p.1).sum::<f64>() / n,
    );
    let num: f64 = pts.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum();
    let den: f64 = pts.iter().map(|p| (p.0 - mx).powi(2)).sum();
    num / den
}

/// The pieces a plan is in: every four connected run of town cells, as
/// its own count of cells, biggest first.
fn pieces(m: &[bool]) -> Vec<usize> {
    let mut seen = vec![false; GRID * GRID];
    let mut sizes = Vec::new();
    for start in 0..GRID * GRID {
        if !m[start] || seen[start] {
            continue;
        }
        seen[start] = true;
        let mut stack = vec![(start % GRID, start / GRID)];
        let mut size = 0;
        while let Some((i, j)) = stack.pop() {
            size += 1;
            for (a, b) in around(i, j) {
                if m[b * GRID + a] && !seen[b * GRID + a] {
                    seen[b * GRID + a] = true;
                    stack.push((a, b));
                }
            }
        }
        sizes.push(size);
    }
    sizes.sort_unstable_by(|a, b| b.cmp(a));
    sizes
}

/// What a plan is SHAPED like, in six numbers.
#[derive(Clone, Copy, Debug, Default)]
struct Shape {
    /// The box counting dimension of every boundary, holes included.
    rim: f64,
    /// The same of the HULL alone, the outline the country sees.
    hull: f64,
    /// How much of the frame is town.
    fill: f64,
    /// How much of the outer twentieth of the bare outline is built,
    /// which is what says whether the ellipse is still the outline.
    outer: f64,
    /// How many separate pieces of a block or more the plan is in.
    pieces: f64,
    /// How much of the town is NOT its biggest piece: the outliers.
    apart: f64,
}

fn shape_of(p: &Plan) -> Shape {
    let (mut edge, mut built) = (0usize, 0usize);
    for (k, w) in p.want.iter().enumerate() {
        if *w > 0.0 && *w < 0.05 {
            edge += 1;
            built += usize::from(p.town[k]);
        }
    }
    let sizes = pieces(&p.town);
    let block = ((PITCH / p.cell).powi(2)).ceil() as usize;
    let total: usize = sizes.iter().sum();
    Shape {
        rim: dimension(&rim(&p.town)),
        hull: dimension(&hull(&p.town)),
        fill: total as f64 / p.town.len() as f64,
        outer: built as f64 / edge.max(1) as f64,
        pieces: sizes.iter().filter(|s| **s >= block).count() as f64,
        apart: 1.0 - sizes.first().copied().unwrap_or(0) as f64 / total.max(1) as f64,
    }
}

/// The mean shape of a town of `radius` over four seeds, under a rule.
fn mean_shape(radius: f64, rule: &dyn Fn(f64, f64, u32) -> f64) -> Shape {
    let along = DVec2::new(1.0, 0.0);
    let mut sum = Shape::default();
    for k in 0..4u32 {
        let seed = k * 977 + 7;
        let bare = |x, z| demand_at(x, z, radius, along, seed, 0.0, 0.0);
        let s = shape_of(&mask(radius, &|x, z| rule(x, z, seed), &bare));
        sum.rim += s.rim / 4.0;
        sum.hull += s.hull / 4.0;
        sum.fill += s.fill / 4.0;
        sum.outer += s.outer / 4.0;
        sum.pieces += s.pieces / 4.0;
        sum.apart += s.apart / 4.0;
    }
    sum
}

fn print_shape(name: &str, s: &Shape) {
    println!(
        "{name:<22} {:6.3} {:6.3}  {:5.1}%  {:5.1}%  {:6.1}  {:5.1}%",
        s.rim,
        s.hull,
        s.fill * 100.0,
        s.outer * 100.0,
        s.pieces,
        s.apart * 100.0
    );
}

/// A CITY IS NOT AN OVAL, and these are the numbers that say so.
///
/// Three rules on the same four seeds and at two sizes: the bare lobed
/// outline, the grain it carried until now, and the FRONT. What decides
/// it is the OUTER column, how much of the outline's own last twentieth
/// is built: at half, the outline IS the ellipse whatever the inside
/// does, and at a percent or two the ellipse is a bound nothing is drawn
/// against. The HULL is the outline the country sees, holes left out,
/// and its dimension is the one Batty and Longley measure a real built
/// up edge by.
#[test]
fn measure_a_towns_fractal_dimension() {
    let along = DVec2::new(1.0, 0.0);
    for radius in [537.0, 1_611.0] {
        println!("a {radius} m town, four seeds:");
        println!("rule                      rim   hull    fill   outer  pieces  apart");
        let oval = mean_shape(radius, &|x, z, seed| {
            demand_at(x, z, radius, along, seed, 0.0, 0.0)
        });
        let era = mean_shape(radius, &|x, z, seed| oval_era(x, z, radius, along, seed));
        let front = mean_shape(radius, &|x, z, seed| demand(x, z, radius, along, seed));
        print_shape("the bare outline", &oval);
        print_shape("the grain it replaces", &era);
        print_shape("the front", &front);
        assert!(
            era.outer > 0.30,
            "the old grain built {:.1}% of the outline's edge, which is not the defect this names",
            era.outer * 100.0
        );
        assert!(
            front.outer < 0.05,
            "the front builds {:.1}% of the outline's edge: the ellipse still shows",
            front.outer * 100.0
        );
        assert!(
            front.hull > era.hull + 0.05,
            "the hull is {:.3} against the old {:.3}: still one smooth curve",
            front.hull,
            era.hull
        );
    }
}

/// The FRONT swept, with the fade held, so only where a town frays
/// moves: how much of its frame it fills, how ragged its hull is, and
/// how much of it stands apart from its main body.
#[test]
fn sweep_the_front() {
    let along = DVec2::new(1.0, 0.0);
    let radius = 1_611.0;
    println!("a {radius} m town at a fade of {FADE}, four seeds:");
    println!("front                     rim   hull    fill   outer  pieces  apart");
    let mut fills = Vec::new();
    for front in [0.20, 0.25, 0.30, 0.35, 0.40, 0.45] {
        let s = mean_shape(radius, &|x, z, seed| {
            demand_at(x, z, radius, along, seed, front, FADE)
        });
        let shipped = if front == FRONT { "  (shipped)" } else { "" };
        print_shape(&format!("{front:.2}{shipped}"), &s);
        fills.push(s.fill);
    }
    assert!(
        fills.windows(2).all(|w| w[1] < w[0]),
        "a front further in does not leave less town: {fills:?}"
    );
}

/// The grain's own MEAN and SPREAD at every octave count a town can
/// ask for, which is what `GRAIN_SPREAD` is set from.
///
/// It is this file's own oldest lesson (`biome::measure_the_fbm_spread`)
/// arriving at a second sum: a share handed a RAW octave sum is worth
/// about a fifth of what it says, because the sum piles up near its
/// middle. How MUCH it piles up is a function of both the persistence
/// and the count, so a grain whose count is read off the town's own
/// size cannot borrow one number, let alone `fbm3`'s.
#[test]
fn measure_the_grains_own_spread() {
    let spread = |persist: f64, octaves: u32| {
        let n = 40_000;
        let (mut sum, mut sq) = (0.0, 0.0);
        for k in 0..n {
            let t = k as f64;
            let p = DVec3::new(t * 0.7393, t * 0.3119 + GRAIN_SLICE, t * 0.5477);
            let v = fbm3_rough(p, 7, octaves, persist);
            sum += v;
            sq += v * v;
        }
        let mean = sum / n as f64;
        (mean, (sq / n as f64 - mean * mean).max(0.0).sqrt())
    };
    println!(
        "a 537 m town's grain runs to {} octaves, a 170 m one to {}, a 23 m one to {}",
        grain_octaves(537.0),
        grain_octaves(170.0),
        grain_octaves(23.0)
    );
    println!("octaves   fbm3    grain at {GRAIN_PERSIST:.2}   the table");
    for k in 1..=GRAIN_SPREAD.len() {
        let (mean, sd) = spread(GRAIN_PERSIST, k as u32);
        let plain = spread(0.5, k as u32).1;
        println!(
            "{k:7}   {plain:.3}   {sd:.3}            {:.3}",
            GRAIN_SPREAD[k - 1]
        );
        assert!(
            (sd - GRAIN_SPREAD[k - 1]).abs() < 0.005,
            "{k} octaves have a spread of {sd:.3} against the table's {:.3}",
            GRAIN_SPREAD[k - 1]
        );
        assert!(
            (mean - GRAIN_MEAN).abs() < 0.01,
            "{k} octaves have a mean of {mean:.3} against the constant's {GRAIN_MEAN:.3}"
        );
    }
    // The grain is NARROWER than `fbm3` at the same count, which is
    // not what a rougher field sounds like and is what the
    // normalisation does: an octave kept at 0.72 rather than halved
    // means more nearly equal INDEPENDENT terms in one average, so the
    // sum piles up harder. What makes the field rough is the SHARE of
    // its variance the fine octaves carry, and the number that reads
    // that is the level set's own dimension and not the spread.
    let (six, plain) = (spread(GRAIN_PERSIST, 6).1, spread(0.5, 6).1);
    println!("at six octaves the grain spreads {six:.3} against fbm3's {plain:.3}");
    assert!(six < plain, "the grain is no narrower than fbm3");
    // And `fbm3` IS this sum at a half, bit for bit, which is what
    // says there is one octave sum in this crate and not two: a
    // transcription that had drifted would be a field the GPU sampler
    // and the CPU disagreed about.
    for k in 0..2_000 {
        let t = k as f64;
        let p = DVec3::new(t * 0.113, t * 0.257 + GRAIN_SLICE, t * 0.391);
        for octaves in 1..=8 {
            assert_eq!(
                fbm3(p, 7, octaves),
                fbm3_rough(p, 7, octaves, 0.5),
                "fbm3 and the same sum at a half disagree at {octaves} octaves"
            );
        }
    }
    let _ = GRAIN_LUMP;
}

/// The PLAN itself, drawn, because a picture is the only check there
/// is on a shape and a dimension is a number about one.
#[test]
fn draw_a_town_on_its_front() {
    let town = super::lay(DVec3::Y, 0.0, 537.0, DVec2::new(1.0, 0.0), 0, 7);
    println!(
        "a 537 m town, {} lots, front {FRONT} at a fade of {FADE}, persistence {GRAIN_PERSIST} over {} octaves:",
        town.lots.len(),
        grain_octaves(537.0)
    );
    print!("{}", super::tests::drawn(&town));
}

/// A town always has a SOLID MIDDLE, however the noise rolls, and is
/// never built past its own outline.
///
/// The grain is capped at `CAP` deviations, so the front takes nothing
/// out where the demand stands over `FRONT * (FADE + CAP) / FADE`, the
/// inner 31% of every bearing: arithmetic rather than a coincidence, and
/// what says a one block hamlet cannot be shredded to nothing and a city
/// cannot grow a hole through its own downtown.
///
/// It is also what keeps the grading untouched. The built set is a
/// SUBSET of the star shaped region `edge` bounds, so `Site::level_r`
/// is still an upper bound on how far a town reaches, `field::site_skirt`
/// still widens by the same `WOBBLE`, and the planet's own slope bound
/// is the number it always was.
#[test]
fn a_towns_middle_is_never_bitten_through() {
    let along = DVec2::new(1.0, 0.0);
    let solid = FRONT * (FADE + CAP) / FADE;
    let mut worst = f64::MAX;
    let mut outside = 0;
    for seed in 0..64u32 {
        for radius in [23.0, 170.0, 537.0, 1_611.0] {
            for k in 0..256 {
                let a = k as f64 / 256.0 * std::f64::consts::TAU;
                let b = DVec2::new(a.cos(), a.sin());
                // Just inside the solid share, on this bearing's own edge.
                let at = b * edge(b, radius, along, seed) * (1.0 - solid) * 0.999;
                worst = worst.min(demand(at.x, at.y, radius, along, seed));
                // And the built set never reaches PAST the outline,
                // which is what the grading relies on.
                let far = b * edge(b, radius, along, seed) * 1.0001;
                if demand(far.x, far.y, radius, along, seed) > 0.0 {
                    outside += 1;
                }
            }
        }
    }
    println!(
        "inside {:.3} of the edge the least demand anywhere on 64 seeds is {worst:.4}, and {outside} points stand outside the outline",
        1.0 - solid
    );
    assert!(worst > 0.0, "a town is bitten through its own middle");
    assert_eq!(outside, 0, "the front put ground outside the outline");
}
