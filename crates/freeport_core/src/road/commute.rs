//! Who is out on a ROAD between two towns, and where they are at a
//! moment.
//!
//! It is `traffic.rs`'s own rule one level up: **a car out in the
//! country is ON RAILS**. Its place is a closed form function of the road
//! it is on, its own index and the world clock, so nothing integrates,
//! nothing is saved, two clients agree by construction, and a road nobody
//! is near costs exactly nothing, because the function is never asked.
//! What INTEGRATES is the player, and only the player.
//!
//! What makes this different from a town's traffic is the SHAPE of the
//! thing it runs on. A town is a graph of streets and an agent goes round
//! a closed circuit of it; a road is one line with two ends, so a car on
//! one runs its whole length and comes back round, which is the same
//! closed form with a simpler track.

use super::ribbon::{Course, HALF};
use glam::DVec3;

/// How much road there is to each car out on it, metres.
///
/// The owner asked for a FEW cars and not a lot, so it is a car every
/// ten kilometres: this body's 63,840 km of road carry about six
/// thousand of them, which is a car every three or four minutes of
/// driving at the speed they go and none at all anywhere nobody is
/// looking. For scale, a town of two hundred buildings turns out
/// thirty eight.
pub const EVERY: f64 = 10_000.0;

/// How fast country traffic goes, metres a second, and how much of that
/// a car's own hash is worth either way.
///
/// 27 m/s is 97 km/h, which is a rural two lane road's own design speed
/// (`docs/civil-engineering.md`) and is well under what the road here is
/// ALIGNED for, so the traffic is something a player at 160 km/h comes
/// up behind rather than something that keeps pace.
const SPEED: f64 = 27.0;
const SPREAD: f64 = 0.22;

/// How far from the centreline a car rides, metres: the middle of its
/// own lane, on the RIGHT of the way it is going.
///
/// Read off `traffic::CAR_LANE` rather than written again, so a car in
/// the country and a car in a town sit the same distance off the paint
/// and oncoming traffic passes on the left in both.
pub const LANE: f64 = crate::traffic::CAR_LANE;

/// One car out on a road: how far along it stood at time nought, how
/// fast it goes, which way, and a number of its own.
///
/// Its place along the road is counted in centreline POINTS rather than
/// in metres, which is what makes `spot` a constant time answer: the
/// points are `road::PIECE` apart by construction, so walking a road's
/// two thousand of them to turn a distance into an index would be the
/// frame. What it costs is that a car crossing the one SHORT piece at
/// the end of each ten kilometre waypoint span covers it a little
/// slower, which is one piece in a hundred and eighteen and is under
/// what the speed's own spread already is.
#[derive(Clone, Copy, Debug)]
pub struct Commuter {
    /// Where it stood at time nought, in centreline points.
    pub at: f64,
    /// How fast it goes, points a second, signed: negative is the car
    /// driving back the way the road was routed.
    pub rate: f64,
    /// A number of its own, for whatever a model wants to differ on.
    pub id: u32,
}

/// Everybody out on one road: a car every `EVERY` metres of it, each
/// hashed off the road's own index so a body is the same body twice.
///
/// `points` is how many centreline points the road has and `metres` how
/// long it is. A road with nothing on it (one too short to carry a car,
/// or one with no centreline) turns out nobody, which is the answer
/// rather than a failure.
pub fn plan(road: usize, points: usize, metres: f64, seed: u32) -> Vec<Commuter> {
    if points < 2 || !metres.is_finite() || metres <= 0.0 {
        return Vec::new();
    }
    let how_many = (metres / EVERY).round() as usize;
    let span = (points - 1) as f64;
    let piece = metres / span;
    (0..how_many)
        .map(|k| {
            let h = |i: i64| crate::noise::hash3(k as i64, road as i64, i, seed);
            let speed = SPEED * (1.0 + (h(2) * 2.0 - 1.0) * SPREAD);
            // Half of them drive the other way, which is what makes a
            // road a road rather than a queue.
            let back = h(3) < 0.5;
            Commuter {
                at: h(1) * span,
                rate: speed / piece * if back { -1.0 } else { 1.0 },
                id: (h(4) * u32::MAX as f64) as u32,
            }
        })
        .collect()
}

/// Where a car on a road is at `time` seconds: the direction its wheels
/// stand at, the radius of the tarmac under them, and which way it
/// points.
///
/// The ONE place the clock is read, and the whole of the motion. It is
/// the same rule `Traffic::at` keeps for a town, so a car out in the
/// country and a car in a street are both a function of one number.
pub fn spot(course: Course<'_>, radius: f64, car: &Commuter, time: f64) -> Option<(DVec3, DVec3)> {
    let n = course.line.len();
    if n < 2 || course.run.len() != n {
        return None;
    }
    let span = (n - 1) as f64;
    let along = (car.at + car.rate * time).rem_euclid(span);
    let k = (along.floor() as usize).min(n - 2);
    let t = along - k as f64;
    let (a, b) = (course.line[k], course.line[k + 1]);
    let up = a.lerp(b, t).normalize_or(a);
    let h = course.run[k] * (1.0 - t) + course.run[k + 1] * t;
    // The way it is GOING, which is the road's own direction for a car
    // running out and its reverse for one coming back.
    let ahead = (b - a) - up * (b - a).dot(up);
    let fwd = ahead.normalize_or(DVec3::X) * car.rate.signum();
    // And the middle of its own LANE, on the right of that.
    let right = fwd.cross(up).normalize_or(DVec3::Z);
    let at = up * (radius + h + super::ribbon::LIFT) + right * LANE;
    Some((at, fwd))
}

/// How far off the centreline the tarmac reaches, metres: what a caller
/// checks a place against to know a car is ON the road it was put on.
pub const EDGE: f64 = HALF;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::road::PIECE;

    /// A straight road of `points` stations on a body of `radius`, level.
    fn road(radius: f64, points: usize) -> (Vec<DVec3>, Vec<f64>, Vec<bool>) {
        let line: Vec<DVec3> = (0..points)
            .map(|i| {
                let a = i as f64 * PIECE / radius;
                DVec3::new(a.sin(), 0.0, a.cos())
            })
            .collect();
        (line, vec![0.0; points], vec![true; points])
    }

    /// A CAR ON A ROAD IS A FUNCTION OF THE CLOCK, it keeps to its own
    /// lane, and half of them come the other way.
    #[test]
    fn a_car_on_a_road_runs_its_length_in_its_own_lane() {
        let radius = 1_000_000.0;
        let points = 400;
        let (line, run, open) = road(radius, points);
        let course = Course {
            line: &line,
            run: &run,
            open: &open,
            graded: &open,
            lit: &open,
            pumps: &[],
            first: 0,
        };
        let metres = (points - 1) as f64 * PIECE;
        let cars = plan(3, points, metres, 7);
        assert!(!cars.is_empty(), "a {metres:.0} m road turned out nobody");
        println!(
            "a {:.0} km road carries {} cars at {:.0} to {:.0} km/h",
            metres / 1000.0,
            cars.len(),
            cars.iter()
                .map(|c| c.rate.abs() * PIECE * 3.6)
                .fold(f64::INFINITY, f64::min),
            cars.iter()
                .map(|c| c.rate.abs() * PIECE * 3.6)
                .fold(0.0, f64::max),
        );
        assert!(
            cars.iter().any(|c| c.rate > 0.0) && cars.iter().any(|c| c.rate < 0.0),
            "every car on the road is going the same way"
        );
        let mut worst_off = 0.0f64;
        for car in &cars {
            for step in 0..200 {
                let time = step as f64 * 0.7;
                let (at, fwd) = spot(course, radius, car, time).expect("a spot");
                // How far off the CENTRELINE it is: its own lane and no
                // further, which is what keeps a road's traffic on the
                // tarmac the ribbon laid.
                let dir = at.normalize();
                // To the LINE and not to the nearest station: the
                // stations are `PIECE` apart, so the nearest of them is
                // up to half a piece away from a car sitting exactly on
                // the paint between two.
                let off = line
                    .windows(2)
                    .map(|w| {
                        let (a, b) = (w[0] * radius, w[1] * radius);
                        let run = b - a;
                        let t = (dir * radius - a).dot(run) / run.length_squared().max(1e-30);
                        (dir * radius - (a + run * t.clamp(0.0, 1.0))).length()
                    })
                    .fold(f64::INFINITY, f64::min);
                worst_off = worst_off.max(off);
                assert!(
                    (fwd.length() - 1.0).abs() < 1e-9,
                    "a heading is a unit vector"
                );
                assert!(fwd.dot(at.normalize()).abs() < 1e-6, "and it is tangent");
            }
        }
        println!("the furthest a car sits off the centreline is {worst_off:.2} m");
        assert!(
            (LANE - 0.2..=LANE + 0.2).contains(&worst_off),
            "a car rides {worst_off:.2} m off the centreline, not {LANE:.2}"
        );
        assert!(worst_off < EDGE, "and it is on the tarmac");
    }

    /// A road too short to carry anybody carries nobody, and a garbage
    /// one is not a panic.
    #[test]
    fn a_short_road_turns_out_nobody() {
        assert!(plan(0, 1, 100.0, 7).is_empty());
        assert!(plan(0, 400, -1.0, 7).is_empty());
        assert!(plan(0, 400, 100.0, 7).is_empty());
        let (line, run, open) = road(1_000_000.0, 3);
        let course = Course {
            line: &line,
            run: &run,
            open: &open,
            graded: &open,
            lit: &open,
            pumps: &[],
            first: 0,
        };
        let car = Commuter {
            at: 0.0,
            rate: 1.0,
            id: 0,
        };
        assert!(
            spot(course, 1_000_000.0, &car, 1e9).is_some(),
            "a clock that only grows is fine"
        );
        let short = Course {
            line: &line[..1],
            run: &run[..1],
            open: &open[..1],
            graded: &open[..1],
            lit: &open[..1],
            pumps: &[],
            first: 0,
        };
        assert!(spot(short, 1_000_000.0, &car, 0.0).is_none());
    }
}
