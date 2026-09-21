//! How far the terrain a chunk at each CELL actually DRAWS stands over a
//! road's own tarmac, off the mesher the game draws with.
//!
//! This is the number behind the owner's picture of a road coming out
//! DASHED over a rise: the tarmac is its own mesh and is always drawn
//! whole, so a gap in it is terrain standing in front of it. Two things
//! can put terrain there and they are different failures with different
//! cures, so the sweep prints both:
//!
//! - **the corridor the cell could not hold.** A road's ground is
//!   levelled `road::CORRIDOR` either side of its centreline, and a
//!   lattice column is only guaranteed to land on that flat while a cell
//!   fits inside it. Past that the chunk has no sample in the corridor
//!   and draws the hill that was there before the road. That is what the
//!   `as built` row measures, because it contours the planet the game
//!   contours, corridor sites and all.
//! - **the embankment that was not tall enough.** `road::EMBANK` is a
//!   CONSTANT two metres, and what it has to clear is the drawn surface,
//!   whose error against the analytic ground grows with the cell. The
//!   `no corridor` row is that alone: the same road over a planet whose
//!   field never heard of it, which is also the world a road made of its
//!   own geometry would stand on.
//!
//! `roads::ground_over_tarmac` in the app measures the second against
//! the ANALYTIC bare surface, which is the exact ground and not what any
//! chunk draws; this measures what a chunk draws, which is the thing an
//! eye actually meets.
use freeport_core::dc::contour;
use freeport_core::field::{Built, Planet};
use freeport_core::lattice::{ChunkId, Flat, Lattice};
use freeport_core::road::{self, Road};
use freeport_core::town::{surface_radius, Site};
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

/// The OUTERMOST radius a ray from space down `dir` meets, which is the
/// surface an eye outside the body sees.
fn drawn(tris: &[[DVec3; 3]], dir: DVec3, from: f64) -> Option<f64> {
    let d = -dir;
    let o = dir * from;
    let mut best: Option<f64> = None;
    for t in tris {
        let (e1, e2) = (t[1] - t[0], t[2] - t[0]);
        let p = d.cross(e2);
        let det = e1.dot(p);
        if det.abs() < 1e-12 {
            continue;
        }
        let inv = 1.0 / det;
        let s = o - t[0];
        let u = s.dot(p) * inv;
        if !(-1e-9..=1.000_000_001).contains(&u) {
            continue;
        }
        let q = s.cross(e1);
        let v = d.dot(q) * inv;
        if v < -1e-9 || u + v > 1.000_000_001 {
            continue;
        }
        let k = e2.dot(q) * inv;
        if k < 0.0 {
            continue;
        }
        let r = from - k;
        if best.is_none_or(|b| r > b) {
            best = Some(r);
        }
    }
    best
}

/// Every triangle of the chunk `level` holds at `dir`, in world metres.
fn chunk_tris(planet: &Planet, lat: &Lattice, dir: DVec3, at: f64, level: u8) -> Vec<[DVec3; 3]> {
    let id = ChunkId::holding(level, lat.fine_cell(dir * at));
    let mesh = contour(&Built::bare(planet), lat, id, &Flat(level));
    let base = id.corner(lat);
    let world: Vec<DVec3> = mesh
        .positions
        .iter()
        .map(|p| base + DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64))
        .collect();
    mesh.indices
        .chunks_exact(3)
        .map(|t| {
            [
                world[t[0] as usize],
                world[t[1] as usize],
                world[t[2] as usize],
            ]
        })
        .collect()
}

/// A road of `span` metres from `a` on a bearing, its ground surveyed and
/// its corridor levelled: the centreline, the profile and the sites.
fn lay(
    planet: &Planet,
    a: DVec3,
    bearing: DVec3,
    span: f64,
    corridor: f64,
) -> (Vec<DVec3>, Vec<f64>, Vec<Site>) {
    let b = (a + bearing * (span / planet.radius)).normalize();
    let road = Road {
        from: 0,
        to: 0,
        line: vec![(a, 0.0), (b, 0.0)],
    };
    let line = road::centreline(&road, planet.radius);
    let run = road::survey(planet, &road, 0.0);
    let sites = line
        .windows(2)
        .zip(run.windows(2))
        .map(|(d, h)| Site {
            dir: d[0],
            h: h[0],
            to: d[1],
            to_h: h[1],
            r: corridor,
            fills: true,
            outline: None,
        })
        .collect();
    (line, run, sites)
}

fn main() {
    let corridor: f64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(road::CORRIDOR);
    let planet = harness();
    let lat = Lattice::new(DVec3::splat(-2_000_099.75), 0.5);
    let dry = planet.radius + 1100.0 + road::DRY - planet.radius;
    // A few roads rather than one: the worst is what decides whether an
    // embankment clears the terrain's own LOD, and one road is one
    // sample of a body.
    let mut roads = Vec::new();
    let mut k = 0u32;
    while roads.len() < 12 && k < 4_000 {
        let t = k as f64 * 2.399_963_229_728_653;
        let y = 1.0 - 2.0 * (k as f64 + 0.5) / 4_000.0;
        let r = (1.0 - y * y).max(0.0).sqrt();
        let a = DVec3::new(r * t.cos(), y, r * t.sin()).normalize();
        k += 1;
        if surface_radius(&planet, a) - planet.radius <= dry + 50.0 {
            continue;
        }
        let east = DVec3::Y.cross(a).normalize_or(DVec3::X);
        let (line, run, sites) = lay(&planet, a, east, 1_600.0, corridor);
        if run.iter().all(|h| *h > dry) {
            roads.push((line, run, sites));
        }
    }
    println!(
        "{} roads of {} stations each, on the harness planet, corridor {corridor:.0} m",
        roads.len(),
        roads.first().map_or(0, |r| r.0.len())
    );
    println!();
    println!("      cell   the eye's                as built            no corridor in the field      a chunk");
    println!("            own range      worst   median   buried      worst   median   buried     with   without");
    for level in 2u8..=10 {
        let cell = lat.cell(level);
        let mut rows = [Vec::new(), Vec::new()];
        let mut spent = [0.0f64; 2];
        let mut built = [0usize; 2];
        for (line, run, sites) in &roads {
            for (which, row) in rows.iter_mut().enumerate() {
                let mut here = planet.clone();
                here.sites = if which == 0 {
                    sites.clone().into()
                } else {
                    vec![].into()
                };
                let mut cached: Vec<(ChunkId, Vec<[DVec3; 3]>)> = Vec::new();
                for (d, h) in line.iter().zip(run.iter()) {
                    let at = planet.radius + h;
                    let id = ChunkId::holding(level, lat.fine_cell(*d * at));
                    if !cached.iter().any(|(c, _)| *c == id) {
                        let started = Instant::now();
                        let tris = chunk_tris(&here, &lat, *d, at, level);
                        spent[which] += started.elapsed().as_secs_f64() * 1000.0;
                        built[which] += 1;
                        cached.push((id, tris));
                    }
                    let tris = &cached.iter().find(|(c, _)| *c == id).expect("just put").1;
                    if let Some(r) = drawn(tris, *d, planet.radius + planet.relief) {
                        row.push(r - (at + road::ribbon::LIFT));
                    }
                }
            }
        }
        let say = |v: &mut Vec<f64>| -> (f64, f64, f64) {
            if v.is_empty() {
                return (f64::NAN, f64::NAN, f64::NAN);
            }
            let buried = v.iter().filter(|x| **x > 0.0).count() as f64 / v.len() as f64;
            let worst = v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            v.sort_by(f64::total_cmp);
            (worst, v[v.len() / 2], buried * 100.0)
        };
        let (aw, am, ab) = say(&mut rows[0]);
        let (bw, bm, bb) = say(&mut rows[1]);
        let ms = |i: usize| {
            if built[i] == 0 {
                f64::NAN
            } else {
                spent[i] / built[i] as f64
            }
        };
        println!(
            "  {cell:8.1} m  {:8.0} m   {aw:7.2}  {am:7.2}  {ab:5.1}%   {bw:8.2} {bm:8.2}  {bb:5.1}%   {:6.1}   {:6.1} ms",
            cell * 64.0,
            ms(0),
            ms(1)
        );
    }
    println!();
    println!("A positive number is terrain standing OVER the tarmac, which is road");
    println!("the country hides; `buried` is the share of the centreline it hides,");
    println!(
        "which is what a DASHED road is. the corridor here is {:.0} m and road::EMBANK is {:.0} m.",
        corridor,
        road::EMBANK
    );
}
