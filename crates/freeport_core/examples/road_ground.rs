//! How far the real ground strays from a straight ramp between two road
//! waypoints.
//!
//! This is the ONE measurement that decides how a road's corridor can be
//! levelled: the atlas keeps a road as a chain of waypoints ten
//! kilometres apart, and levelling a corridor along that chain means
//! cutting the ground to a straight ramp between them. If the ground
//! between two waypoints is within a few metres of that ramp, the
//! atlas's own line is the corridor; if it is within hundreds, the line
//! has to be refined first, and how finely is what this prints.
//!
//! It reads pairs of `x y z h x y z h` off a file rather than the atlas,
//! because the atlas's reader is the app's and this is a measurement.
use freeport_core::field::Planet;
use freeport_core::town::surface_radius;
use glam::DVec3;

fn main() {
    let planet = Planet {
        radius: 1_000_000.0,
        relief: 8_000.0,
        lumps: 12.0,
        octaves: 18,
        overhang: 3.0,
        ledge: 12.0,
        seed: 7,
        sites: vec![].into(),
    };
    let path = std::env::args().nth(1).expect("a file of segments");
    let text = std::fs::read_to_string(&path).expect("the segments");
    // Subdivide each waypoint span into `n` and measure the ground
    // against the piecewise ramp through the division points: that is
    // what a corridor levelled at that spacing would have to cut.
    for n in [1usize, 4, 8, 16, 32, 64, 128] {
        let mut offs: Vec<f64> = Vec::new();
        let mut span_m = 0.0;
        for row in text.lines() {
            let v: Vec<f64> = row
                .split_whitespace()
                .filter_map(|s| s.parse().ok())
                .collect();
            if v.len() < 8 {
                continue;
            }
            let a = DVec3::new(v[0], v[1], v[2]).normalize();
            let b = DVec3::new(v[4], v[5], v[6]).normalize();
            span_m += a.angle_between(b) * planet.radius / text.lines().count() as f64;
            let at = |t: f64| (a * (1.0 - t) + b * t).normalize();
            let ground = |t: f64| surface_radius(&planet, at(t)) - planet.radius;
            for k in 0..n {
                let (t0, t1) = (k as f64 / n as f64, (k + 1) as f64 / n as f64);
                let (h0, h1) = (ground(t0), ground(t1));
                for j in 1..4 {
                    let f = j as f64 / 4.0;
                    let t = t0 + (t1 - t0) * f;
                    offs.push((ground(t) - (h0 + (h1 - h0) * f)).abs());
                }
            }
        }
        offs.sort_by(|x, y| x.partial_cmp(y).unwrap());
        let pick = |q: f64| offs[((offs.len() as f64 - 1.0) * q) as usize];
        println!(
            "at {:>6.0} m a piece: median {:6.2} m, 90th {:6.2} m, 99th {:6.2} m, worst {:7.2} m",
            span_m / n as f64,
            pick(0.5),
            pick(0.9),
            pick(0.99),
            offs[offs.len() - 1]
        );
    }
}
