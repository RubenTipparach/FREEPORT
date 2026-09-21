//! GAS STATIONS along the highways: where one stands on a road, and what
//! one is built of.
//!
//! The owner's ask: a car goes about eighty kilometres on a tank
//! (`fuel::RANGE`), so whenever a player runs low they pull up to a
//! station along the highway and fill up. A station is therefore a fact
//! about the ROAD and streams with it: it is placed on the road's own
//! centreline at a piece that is open, graded, unlit and nearly level,
//! `EVERY` metres since the last one, and its forecourt is one more
//! model welded into the stretch that carries that piece
//! (`ribbon::stretch`), on the verge the corridor already levels.
//!
//! It is PARAMETRIC, which is this project's default for new geometry:
//! a slab of forecourt, two pump islands under a canopy on four pillars,
//! a kiosk that is a `Kind::Shop` with its door to the road, and a lit
//! sign at the road's edge. The pumps, the pillars, the islands and the
//! kiosk's walls are `solid`, so a car is stopped by them and a walker
//! walks up to one; the canopy and the sign are trim.

use super::ribbon::{Course, HALF, SHOULDER};
use crate::field::{hash3, CONCRETE, LIT, PLATE, STREET};
use crate::model::{building, Kind, Model};
use glam::DVec3;

/// How far apart stations stand along one road, metres, and how far
/// out of a town the first one is. Thirty five kilometres is under half
/// a tank, so a car that filled at one reaches the next with most of a
/// tank to spare and a car that left a town on a stolen car's own part
/// tank (`fuel::Tank::part`, three tenths at the least, 24 km) reaches
/// the first one out of town.
pub const EVERY: f64 = 35_000.0;
pub const FIRST: f64 = 8_000.0;
/// The steepest piece a forecourt is laid on, rise over run. A forecourt
/// is one flat slab at the piece's own middle height, so over its
/// `LONG` the ground drifts by half of `LONG` times this either way,
/// 7 cm at half a per cent, which the slab's own `TOP` over the tarmac
/// keeps it clear of.
const LEVEL: f64 = 0.005;
/// The forecourt: how far along the road it runs and how far out from
/// the road's shoulder it reaches, metres. Eleven metres out from a
/// shoulder 3.45 m off the centreline is 14.45, inside the corridor's
/// own `CORRIDOR` of levelled flat.
pub const LONG: f64 = 28.0;
pub const DEEP: f64 = 11.0;
/// How far the forecourt's slab stands over the tarmac it joins, metres,
/// and how deep it is buried. Two centimetres is a step a car does not
/// feel and the tarmac's own lift keeps the slab over the verge beside
/// it by that much more.
const TOP: f64 = 0.02;
const BURY: f64 = 0.6;
/// The islands: where they stand across the forecourt, how far apart
/// along it, and the two pumps on each.
const ISLAND_X: f64 = 4.0;
const ISLAND_Y: f64 = 5.0;
const ISLAND_HALF: DVec3 = DVec3::new(0.6, 2.2, 0.11);
const PUMP_HALF: DVec3 = DVec3::new(0.35, 0.45, 0.8);
const PUMP_APART: f64 = 1.3;
/// The canopy over them: its pillars, its height and its slab.
const CANOPY_H: f64 = 5.0;
const PILLAR: f64 = 0.15;
const CANOPY_HALF: DVec3 = DVec3::new(4.5, 10.5, 0.25);
/// The kiosk: a one storey shop set back on the forecourt with its door
/// to the pumps.
const KIOSK_X: f64 = 9.2;
const KIOSK_W: f64 = 7.0;
const KIOSK_D: f64 = 3.6;
/// The sign at the road's edge: a post and a lit board over it.
const SIGN_Y: f64 = 12.5;
const SIGN_H: f64 = 6.4;
const SIGN_HALF: DVec3 = DVec3::new(0.15, 1.6, 0.8);

/// One station: which PIECE of its road it stands on (the piece from
/// point `piece` to `piece + 1` of the centreline) and which side, plus
/// one for the right hand side going along the line and minus one for
/// the left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Station {
    pub piece: usize,
    pub side: f64,
}

/// The distance between two directions, metres, on a body of `radius`.
fn arc(a: DVec3, b: DVec3, radius: f64) -> f64 {
    a.angle_between(b) * radius
}

/// Whether piece `k` of a road will take a station: tarmac laid at both
/// ends, the corridor cut (not a slip over a town's plateau), no lamps
/// (so it is out in the country), long enough for the forecourt, and
/// nearly level.
fn takes(course: &Course<'_>, k: usize, radius: f64) -> bool {
    let at = |v: &[bool], i: usize| v.get(i).copied().unwrap_or(false);
    let run = arc(course.line[k], course.line[k + 1], radius);
    at(course.open, k)
        && at(course.open, k + 1)
        && at(course.graded, k)
        && at(course.graded, k + 1)
        && !at(course.lit, k)
        && !at(course.lit, k + 1)
        && run >= LONG + 4.0
        && ((course.run[k + 1] - course.run[k]) / run).abs() <= LEVEL
}

/// Where the stations on a road stand: the first `FIRST` out of the
/// town it leaves and then one `EVERY` along it, each on the first piece
/// past that mark that will take one, alternating sides.
pub fn plan(course: Course<'_>, radius: f64) -> Vec<Station> {
    let mut out = Vec::new();
    if course.line.len() < 2 || course.run.len() != course.line.len() {
        return out;
    }
    let mut since = EVERY - FIRST;
    for k in 0..course.line.len() - 1 {
        since += arc(course.line[k], course.line[k + 1], radius);
        if since < EVERY || !takes(&course, k, radius) {
            continue;
        }
        let side = if out.len() % 2 == 0 { 1.0 } else { -1.0 };
        out.push(Station { piece: k, side });
        since = 0.0;
    }
    out
}

/// A station's forecourt in its OWN frame: x out from the road's
/// shoulder, y along the road, z up from the tarmac's own level. The
/// road is at negative x and the kiosk at the far end of the slab.
pub fn forecourt(seed: u32) -> Model {
    let mut m = Model::new();
    // The slab, its top a hair over the tarmac and its underside buried.
    m.solid(
        DVec3::new(DEEP * 0.5, 0.0, (TOP - BURY) * 0.5),
        DVec3::new(DEEP * 0.5, LONG * 0.5, (TOP + BURY) * 0.5),
        0.0,
        STREET,
    );
    // Two islands with two pumps each, under the canopy.
    for sy in [-1.0, 1.0] {
        let y = sy * ISLAND_Y;
        m.solid(
            DVec3::new(ISLAND_X, y, TOP + ISLAND_HALF.z * 0.5),
            DVec3::new(ISLAND_HALF.x, ISLAND_HALF.y, ISLAND_HALF.z),
            0.0,
            CONCRETE,
        );
        for sp in [-1.0, 1.0] {
            m.solid(
                DVec3::new(
                    ISLAND_X,
                    y + sp * PUMP_APART,
                    TOP + ISLAND_HALF.z + PUMP_HALF.z,
                ),
                PUMP_HALF,
                0.0,
                PLATE,
            );
        }
        m.lamp(DVec3::new(ISLAND_X, y, CANOPY_H - CANOPY_HALF.z - 0.3));
    }
    for (x, y) in [(1.5, -8.5), (1.5, 8.5), (6.5, -8.5), (6.5, 8.5)] {
        m.solid(
            DVec3::new(x, y, TOP + CANOPY_H * 0.5),
            DVec3::new(PILLAR, PILLAR, CANOPY_H * 0.5),
            0.0,
            CONCRETE,
        );
    }
    // The canopy only DRAWS: nothing a body should pass under stops it.
    m.trim(
        DVec3::new(ISLAND_X, 0.0, TOP + CANOPY_H),
        CANOPY_HALF,
        0.0,
        CONCRETE,
    );
    // The kiosk, a shop turned so its door faces the pumps.
    let kiosk = building(Kind::Shop, KIOSK_W, KIOSK_D, 1, seed);
    m.place(
        &kiosk,
        DVec3::new(KIOSK_X, 0.0, TOP),
        -std::f64::consts::FRAC_PI_2,
    );
    // And the sign at the road's edge, lit, so a station reads from a
    // kilometre off at night.
    m.trim(
        DVec3::new(0.8, SIGN_Y, TOP + SIGN_H * 0.5),
        DVec3::new(0.1, 0.1, SIGN_H * 0.5),
        0.0,
        CONCRETE,
    );
    m.trim(
        DVec3::new(0.8, SIGN_Y, TOP + SIGN_H + SIGN_HALF.z),
        SIGN_HALF,
        0.0,
        LIT,
    );
    m
}

/// How far out from the road's centreline the forecourt's inner edge
/// stands: the tarmac's half width and the shoulder.
pub fn setback() -> f64 {
    HALF + SHOULDER
}

/// A station's own seed, off its piece, so two on one road are not
/// twins.
pub fn seed_of(piece: usize) -> u32 {
    (hash3(piece as i64, 0x6A5, 0x5747, 0x9A5) * u32::MAX as f64) as u32
}

/// Where a station's forecourt has its MIDDLE, in the planet's frame:
/// what a car has to pull up beside to be filled, and what a walker
/// carries a jerrycan from.
pub fn middle(course: Course<'_>, s: &Station, radius: f64) -> Option<DVec3> {
    let (a, b) = (*course.line.get(s.piece)?, *course.line.get(s.piece + 1)?);
    let (ha, hb) = (*course.run.get(s.piece)?, *course.run.get(s.piece + 1)?);
    let dir = (a + b).normalize_or(a);
    let along = (b - a).normalize_or_zero();
    // Across the road to its RIGHT going along the line, which is what
    // `ribbon::stations` calls across: the road's own heading turned a
    // quarter turn clockwise seen from above.
    let across = along.cross(dir).normalize_or_zero();
    let out = setback() + DEEP * 0.5;
    Some(dir * (radius + 0.5 * (ha + hb)) + across * (s.side * out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::Density;
    use crate::road::PIECE;

    /// A straight, level road with lit approaches at both ends, as the
    /// four lanes a course is.
    fn road(km: f64) -> (Vec<DVec3>, Vec<f64>, Vec<bool>, Vec<bool>) {
        let radius = 1.0e6;
        let n = (km * 1000.0 / PIECE).ceil() as usize;
        let line: Vec<DVec3> = (0..=n)
            .map(|k| {
                let a = k as f64 * PIECE / radius;
                DVec3::new(a.sin(), a.cos(), 0.0)
            })
            .collect();
        let run = vec![10.0; line.len()];
        let open = vec![true; line.len()];
        let lit: Vec<bool> = (0..line.len())
            .map(|k| (k as f64 * PIECE) < 1500.0 || ((n - k) as f64 * PIECE) < 1500.0)
            .collect();
        (line, run, open, lit)
    }

    #[test]
    fn a_station_stands_every_thirty_five_kilometres_and_off_a_hill() {
        let radius = 1.0e6;
        let (line, mut run, open, lit) = road(120.0);
        // A hill from 76 to 86 km, at the seven per cent a highway is
        // built at, where the third station is due: no forecourt on it,
        // so the third stands at the foot of the far side.
        for (k, h) in run.iter_mut().enumerate() {
            let m = k as f64 * PIECE;
            if (76_000.0..86_000.0).contains(&m) {
                *h += (m - 76_000.0) * 0.07;
            }
        }
        let course = Course {
            line: &line,
            run: &run,
            open: &open,
            graded: &open,
            lit: &lit,
            pumps: &[],
            first: 0,
        };
        let stations = plan(course, radius);
        let at: Vec<f64> = stations
            .iter()
            .map(|s| s.piece as f64 * PIECE / 1000.0)
            .collect();
        println!("{} stations at {at:?} km", stations.len());
        // Three on 120 km: at 8, at 43, and the one due at 78 pushed
        // to the foot of the hill at 86, after which 121 is off the end.
        assert_eq!(stations.len(), 3, "{at:?}");
        assert!((at[0] - 8.0).abs() < 0.2, "the first is {} km out", at[0]);
        assert!((at[1] - 43.0).abs() < 0.2, "the second is {} km out", at[1]);
        assert!(
            (86.0..86.5).contains(&at[2]),
            "the third is {} km out",
            at[2]
        );
        assert_eq!(stations[0].side, 1.0);
        assert_eq!(stations[1].side, -1.0);
        for s in &stations {
            assert!(!lit[s.piece], "a station in a lit approach");
            let grade = (run[s.piece + 1] - run[s.piece]) / PIECE;
            assert!(grade.abs() <= LEVEL, "a station on a {grade:.3} grade");
            let mid = middle(course, s, radius).expect("a middle");
            let on = line[s.piece] * (radius + run[s.piece]);
            let off = (mid - on).length();
            assert!(
                (off - (setback() + DEEP * 0.5).hypot(PIECE * 0.5)).abs() < 1.0,
                "the forecourt's middle is {off:.1} m off its own station"
            );
        }
        // A hill the whole way and there is nowhere to put one.
        let steep: Vec<f64> = (0..line.len()).map(|k| k as f64 * PIECE * 0.03).collect();
        let course = Course {
            run: &steep,
            ..course
        };
        assert!(plan(course, radius).is_empty());
    }

    #[test]
    fn a_forecourt_has_pumps_a_body_meets_and_a_kiosk_with_its_door_to_them() {
        let m = forecourt(7);
        assert!(m.mesh.triangles() > 100);
        let frame = crate::town::Frame {
            dir: DVec3::Z,
            east: DVec3::X,
            north: DVec3::Y,
            base: 0.0,
        };
        let blocks = m.blocks(&frame);
        let solid = |p: DVec3| blocks.iter().any(|b| b.at(p) > 0.0);
        // A pump stops a body and the lane between the islands does not.
        assert!(solid(DVec3::new(ISLAND_X, ISLAND_Y + PUMP_APART, 1.0)));
        assert!(!solid(DVec3::new(ISLAND_X, 0.0, 1.0)));
        // Under the canopy is clear at a car's height, and a pillar is not.
        assert!(!solid(DVec3::new(ISLAND_X, -8.5, 2.5)));
        assert!(solid(DVec3::new(1.5, -8.5, 2.5)));
        // The slab's top is a hair over the tarmac, and a body standing
        // on the slab stands on it.
        assert!(solid(DVec3::new(2.0, 0.0, -0.1)) && !solid(DVec3::new(2.0, 0.0, TOP + 0.05)));
        // The kiosk's door opens toward the pumps: a walk from the
        // forecourt into the kiosk along y = 0 meets nothing.
        for step in 0..=20 {
            let x = KIOSK_X - KIOSK_D * 0.5 - 1.0 + step as f64 * 0.1;
            assert!(
                !solid(DVec3::new(x, 0.0, 1.2)),
                "the kiosk's doorway is blocked at x = {x:.2}"
            );
        }
        // And its back wall is a wall.
        assert!(solid(DVec3::new(KIOSK_X + KIOSK_D * 0.5 - 0.15, 0.0, 1.2)));
        assert!(
            !m.lamps.is_empty(),
            "a station with no light under its canopy"
        );
    }

    #[test]
    fn a_model_placed_in_another_turns_with_it() {
        let mut inner = Model::new();
        inner.solid(DVec3::new(1.0, 0.0, 0.5), DVec3::splat(0.25), 0.0, PLATE);
        inner.lamp(DVec3::new(1.0, 0.0, 2.0));
        let mut outer = Model::new();
        outer.place(
            &inner,
            DVec3::new(10.0, 10.0, 0.0),
            std::f64::consts::FRAC_PI_2,
        );
        let s = outer.solids[0];
        assert!((s.centre - DVec3::new(10.0, 11.0, 0.5)).length() < 1e-9);
        assert!((s.yaw - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        assert!((outer.lamps[0] - DVec3::new(10.0, 11.0, 2.0)).length() < 1e-9);
        assert_eq!(outer.mesh.triangles(), inner.mesh.triangles());
    }
}
