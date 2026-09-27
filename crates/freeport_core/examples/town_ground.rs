//! How far a CITY has to cut into the country it stands on, measured over
//! the harness body's own candidate sites.
//!
//! A town levelled its ground to ONE height across its whole outline, and
//! a site was accepted only where the natural ground across that outline
//! fell no more than `town::CUT`. That is what kept a city small: the
//! bigger the outline, the rarer fifteen metres of fall across it is.
//! This measures that rule against the GRADED ground a town stands on now
//! (`town::Grade`), at the size the game planned before and at three
//! times it, and how far the graded ground stands OVER the bare ground
//! between the samples it was surveyed from, which is what `town::DIP` is
//! set from.
//!
//! Every height is the ANALYTIC bare surface, which leaves out only the
//! volumetric term, a metre and a half at the most on this body.
//!
//! ```sh
//! cargo run --release -p freeport_core --example town_ground
//! ```

use freeport_core::field::Planet;
use freeport_core::town::{frame_at, Grade, DIP, OUTLINE};
use glam::DVec3;
use std::time::Instant;

/// The harness planet, which is what `freeport_app` runs.
fn harness() -> Planet {
    Planet {
        radius: 1_000_000.0,
        relief: 8_000.0,
        lumps: 12.0,
        octaves: 18,
        overhang: 3.0,
        ledge: 12.0,
        seed: 7,
        sites: vec![].into(),
    }
}

/// The harness sea, metres over the mean radius.
const SEA: f64 = 1_100.0;
/// The deepest a town may cut, metres: `town::CUT`.
const CUT: f64 = 15.0;
/// How many candidate sites are surveyed.
const SITES: usize = 240;

/// Land candidates in the habitable window, off the golden spiral the
/// game's own planner walks, an even sample of what qualified.
fn candidates(p: &Planet, count: usize) -> Vec<DVec3> {
    let golden = std::f64::consts::PI * (3.0 - 5f64.sqrt());
    let shape = p.shape();
    let total = 20_000;
    let mut out = Vec::new();
    for i in 0..total {
        let y = 1.0 - 2.0 * (i as f64 + 0.5) / total as f64;
        let s = (1.0 - y * y).sqrt();
        let a = golden * i as f64;
        let dir = DVec3::new(s * a.cos(), y, s * a.sin());
        let h = p.surface(dir).0 - SEA;
        if !(3.0..=2_400.0).contains(&h) || shape.climate(dir, h).frozen() {
            continue;
        }
        out.push(dir);
    }
    let every = (out.len() / count).max(1);
    out.into_iter().step_by(every).take(count).collect()
}

/// One site at one size: how far the bare ground falls across the town,
/// which is what one flat level has to cut, how deep its graded ground
/// cuts, and how far that ground stands over the bare ground at worst,
/// off its own survey's samples, every seven metres.
struct Cost {
    fall: f64,
    cut: f64,
    float: f64,
}

fn cost(p: &Planet, dir: DVec3, reach: f64) -> Cost {
    let graded = Grade::survey(p, dir, reach, 25.0, f64::NEG_INFINITY, &|_| reach);
    let g = &graded.grade;
    let (east, north) = frame_at(dir);
    let (mut lo, mut hi, mut float) = (f64::MAX, f64::MIN, 0.0f64);
    let k = (reach / 7.0) as i64;
    for j in -k..=k {
        for i in -k..=k {
            let (x, z) = ((i as f64 + 0.37) * 7.0, (j as f64 + 0.61) * 7.0);
            if x.hypot(z) > reach {
                continue;
            }
            let d = (dir + east * (x / p.radius) + north * (z / p.radius)).normalize();
            let bare = p.surface(d).0;
            lo = lo.min(bare);
            hi = hi.max(bare);
            float = float.max(g.at(x, z) - bare);
        }
    }
    Cost {
        fall: hi - lo,
        cut: graded.cut,
        float,
    }
}

fn main() {
    let p = harness();
    let sites = candidates(&p, SITES);
    println!(
        "{} land candidates, graded at {DIP} m under their nodes",
        sites.len()
    );
    for radius in [537.0, 1_611.0] {
        let reach = radius * OUTLINE;
        let t = Instant::now();
        let costs: Vec<Cost> = std::thread::scope(|scope| {
            let chunks: Vec<_> = sites
                .chunks(sites.len().div_ceil(4))
                .map(|chunk| {
                    let p = &p;
                    scope
                        .spawn(move || chunk.iter().map(|&d| cost(p, d, reach)).collect::<Vec<_>>())
                })
                .collect();
            chunks
                .into_iter()
                .flat_map(|h| h.join().expect("a survey thread"))
                .collect()
        });
        let n = costs.len() as f64;
        let mut falls: Vec<f64> = costs.iter().map(|c| c.fall).collect();
        falls.sort_by(f64::total_cmp);
        let flat = costs.iter().filter(|c| c.fall <= CUT).count();
        let graded: Vec<&Cost> = costs.iter().filter(|c| c.cut <= CUT).collect();
        let float = graded.iter().map(|c| c.float).fold(0.0f64, f64::max);
        println!(
            "radius {radius} m, outline {reach:.0} m, {} sites in {:.1} s: one flat level fits {flat} \
             (median fall {:.0} m); graded ground fits {} ({:.1}%), and stands at most {float:.2} m \
             over the country anywhere on them",
            costs.len(),
            t.elapsed().as_secs_f64(),
            falls[falls.len() / 2],
            graded.len(),
            100.0 * graded.len() as f64 / n,
        );
    }
}
