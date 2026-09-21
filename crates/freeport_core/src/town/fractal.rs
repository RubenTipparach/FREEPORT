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
    demand_bitten, grain_octaves, GRAIN, GRAIN_LUMP, GRAIN_MEAN, GRAIN_PERSIST, GRAIN_SLICE,
    GRAIN_SPREAD,
};
use super::OUTLINE;
use crate::field::{fbm3, fbm3_rough};
use glam::DVec2;
use glam::DVec3;

/// How many cells a side the plan is sampled on. Nine hundred metres of
/// a 537 m town's own outline over 512 cells is 3.5 m a cell, a fifth
/// of a `PITCH`, so the finest box on the ladder is still finer than
/// the block grid the plan is actually laid on.
const GRID: usize = 512;

/// The box sizes, in cells. The ladder spans the scales a town can
/// actually EXPRESS and no others: four cells is 17 m, about one
/// `PITCH`, and sixty four is 276 m, about half the town. Finer than a
/// block the plan has nothing to say, because a lot is either on its
/// block or not; coarser than half the town there are no boxes left to
/// count. A ladder that ran to the grid's own cell would be measuring
/// how finely the demand was sampled.
const LADDER: [usize; 5] = [4, 8, 16, 32, 64];

/// Which cells of the grid are TOWN, at a given strength of the grain.
/// Nought is the smooth oval the grain replaces, so the before and the
/// after are one run of one function rather than two binaries.
fn mask(radius: f64, along: DVec2, seed: u32, bite: f64) -> Vec<bool> {
    let half = radius * OUTLINE;
    let step = 2.0 * half / GRID as f64;
    let mut m = vec![false; GRID * GRID];
    for j in 0..GRID {
        for i in 0..GRID {
            let x = -half + (i as f64 + 0.5) * step;
            let z = -half + (j as f64 + 0.5) * step;
            m[j * GRID + i] = demand_bitten(x, z, radius, along, seed, bite) > 0.0;
        }
    }
    m
}

/// The BOUNDARY of that: a town cell with at least one of its four
/// neighbours outside the town, the grid's own rim counting as outside
/// so a plan that runs off the edge is still bounded.
fn rim(m: &[bool]) -> Vec<bool> {
    let town = |i: isize, j: isize| {
        (0..GRID as isize).contains(&i)
            && (0..GRID as isize).contains(&j)
            && m[j as usize * GRID + i as usize]
    };
    let mut r = vec![false; GRID * GRID];
    for j in 0..GRID as isize {
        for i in 0..GRID as isize {
            if !town(i, j) {
                continue;
            }
            let open = !town(i - 1, j) || !town(i + 1, j) || !town(i, j - 1) || !town(i, j + 1);
            r[j as usize * GRID + i as usize] = open;
        }
    }
    r
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

/// How much of the outline's own bounding square is town, which is what
/// says the grain THINS a plan as well as fraying it.
fn fill(m: &[bool]) -> f64 {
    m.iter().filter(|b| **b).count() as f64 / m.len() as f64
}

/// A CITY IS NOT AN OVAL, and this is the number that says so.
///
/// The sweep is over the grain's own strength on four seeds, so the
/// before (nought, the smooth lobed ellipse) and the after are one run
/// of one function. What it prints is the boundary's box counting
/// dimension and the share of the frame the plan covers.
#[test]
fn measure_a_towns_fractal_dimension() {
    let along = DVec2::new(1.0, 0.0);
    println!("grain   dimension   fill    (537 m town, four seeds)");
    let mut at: Vec<(f64, f64)> = Vec::new();
    for bite in [0.0, 0.25, GRAIN, 0.8] {
        let (mut d, mut f) = (0.0, 0.0);
        for seed in 0..4u32 {
            let m = mask(537.0, along, seed * 977 + 7, bite);
            d += dimension(&rim(&m));
            f += fill(&m);
        }
        let (d, f) = (d / 4.0, f / 4.0);
        println!("{bite:5.2}   {d:9.3}   {:5.1}%", f * 100.0);
        at.push((bite, d));
    }
    let oval = at[0].1;
    let city = at
        .iter()
        .find(|p| p.0 == GRAIN)
        .expect("the shipped grain")
        .1;
    println!("the oval is {oval:.3} and the city {city:.3}");
    // A smooth closed curve measures one, and a grid this coarse
    // measures a little over it because a stair stepped circle is a
    // stair. What says the plan is FRACTAL is the gap, which is a
    // tenth of a dimension and is far past anything sampling can give.
    assert!(
        city > oval + 0.10,
        "the oval is {oval:.3} and the city {city:.3}: one scale"
    );
    // And it is MONOTONE in the bite, which is what says the dimension
    // is reading the grain rather than the lobes underneath it.
    assert!(
        at.windows(2).all(|w| w[1].1 > w[0].1),
        "the dimension does not climb with the grain: {at:?}"
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
    let _ = (GRAIN, GRAIN_LUMP);
}

/// The PLAN itself, drawn, because a picture is the only check there
/// is on a shape and a dimension is a number about one.
#[test]
fn draw_a_grained_town() {
    let town = super::lay(DVec3::Y, 0.0, 537.0, DVec2::new(1.0, 0.0), 0, 7);
    println!(
        "a 537 m town, {} lots, grain {GRAIN} at persistence {GRAIN_PERSIST} over {} octaves:",
        town.lots.len(),
        grain_octaves(537.0)
    );
    print!("{}", super::tests::drawn(&town));
}

/// A town always has a SOLID MIDDLE, however the noise rolls.
///
/// The bite is scaled by `1 - want`, so it takes a block out only
/// where `want < GRAIN / (1 + GRAIN)`, which is 0.645 of the way out
/// to that bearing's own edge. It is arithmetic rather than a
/// coincidence, and it is what says a one block hamlet cannot be
/// shredded to nothing and a city cannot grow a hole through its own
/// downtown: a `Zone::Core` block is at `CORE_AT` of the demand, well
/// inside it.
///
/// It is also what keeps the levelling untouched. The built set is a
/// SUBSET of the star shaped region `edge` bounds, so `Site::level_r`
/// is still an upper bound on how far a town reaches, `field::site_skirt`
/// still widens by the same `WOBBLE`, and the planet's own slope bound
/// is the number it always was.
#[test]
fn a_towns_middle_is_never_bitten_through() {
    let along = DVec2::new(1.0, 0.0);
    let safe = GRAIN / (1.0 + GRAIN);
    let mut worst = f64::MAX;
    let mut outside = 0;
    for seed in 0..64u32 {
        for radius in [23.0, 170.0, 537.0] {
            for k in 0..256 {
                let a = k as f64 / 256.0 * std::f64::consts::TAU;
                let b = DVec2::new(a.cos(), a.sin());
                // Just inside the safe share, on this bearing's own edge.
                let at = b * super::shape::edge(b, radius, along, seed) * (1.0 - safe) * 0.999;
                let want = demand_bitten(at.x, at.y, radius, along, seed, GRAIN);
                worst = worst.min(want);
                // And the built set never reaches PAST the outline,
                // which is what the levelling relies on.
                let far = b * super::shape::edge(b, radius, along, seed) * 1.0001;
                if demand_bitten(far.x, far.y, radius, along, seed, GRAIN) > 0.0 {
                    outside += 1;
                }
            }
        }
    }
    println!(
        "inside {:.3} of the edge the least demand anywhere on 64 seeds is {worst:.4}, and {outside} points stand outside the outline",
        1.0 - safe
    );
    assert!(worst > 0.0, "a town is bitten through its own middle");
    assert_eq!(outside, 0, "the grain put ground outside the outline");
}
