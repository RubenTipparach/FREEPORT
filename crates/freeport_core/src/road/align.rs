//! The HORIZONTAL ALIGNMENT of a road: circular curves fitted at every
//! vertex of the routed polyline, to a minimum radius read off the speed
//! the road is built for.
//!
//! What a router hands back is a chain of waypoints ten kilometres apart
//! (`road::SPACING`), and a vertex of that chain is a corner with NO
//! radius at all: the heading steps thirty degrees between two points and
//! a car at 160 km/h is asked to turn it in one frame. Real alignment
//! never does that, and `docs/civil-engineering.md` is the long form of
//! why and of every number below.
//!
//! It is PURE GEOMETRY and it is DERIVED rather than stored, which is
//! `road::pieces` and `road::step`'s own rule: the atlas keeps the
//! waypoints and both the bake and the game fit the same curves to them,
//! so a corridor's levelling and the tarmac over it cannot land in
//! different places. `road::CURVE` is in the atlas's fingerprint for the
//! same reason `PIECE` and `EMBANK` are.

use super::PIECE;
use crate::town::frame_at;
use glam::{DVec2, DVec3};

/// The speed a road here is ALIGNED for, metres a second.
///
/// It is `driver::TOP` and not a number of its own, because a design
/// speed is a fact about the fastest thing that uses the road and this
/// world's cars do 160 km/h. `docs/civil-engineering.md` is the long
/// form: every geometric standard a road has falls out of this one
/// input, so the one thing that must not happen is for it to be written
/// down twice.
pub const DESIGN_SPEED: f64 = crate::driver::TOP;

/// The superelevation a curve is banked at and the side friction the
/// tyres are asked for, both as a share of gravity.
///
/// Eight per cent is the usual rural maximum, which is what a body with
/// no ice on its roads may use; a tenth of friction is a little inside
/// the 0.11 real practice allows at 110 km/h and well inside the 0.08 it
/// allows at 130, which is the right way to be wrong about a road built
/// for 160.
const SUPER: f64 = 0.08;
const FRICTION: f64 = 0.10;
const GRAVITY: f64 = 9.81;

/// The minimum horizontal curve radius, metres: `v^2 / (g (e + f))`,
/// which is AASHTO's `V^2 / (127 (e + f))` in SI, since 127 is
/// `3.6^2 * 9.81`.
///
/// At the design speed it is **1,116 m**. For scale, the same formula
/// at 97 km/h (60 mph) and the same banking is 366 m, which sits inside
/// the 560 ft to 1,400 ft range published for that speed; the spread
/// there is the spread in how hard the road is banked.
///
/// It is a LIMIT and not a target, which every design guide says in the
/// same place: `fit` takes the largest radius the legs and the corner
/// cutting will allow and only falls back to this.
///
/// It is in the ATLAS'S FINGERPRINT, because the centreline is derived
/// from the stored waypoints on both sides of the bake: a file baked at
/// another radius has a run of the wrong length for every road on the
/// body, which is the silent failure `Atlas::fits` exists to refuse.
pub const CURVE: f64 = DESIGN_SPEED * DESIGN_SPEED / (GRAVITY * (SUPER + FRICTION));

/// How much of the shorter leg either side of a vertex the curve's own
/// TANGENT may take. Under a half, so the two curves that share a leg
/// cannot overlap however hard each bends.
pub const CURVE_SHARE: f64 = 0.45;

/// How far a curve may cut the corner, metres: the MID ORDINATE
/// `R (sec(delta / 2) - 1)`, which is how far the road as built leaves
/// the line the router chose over ground it actually measured.
///
/// Two hundred and fifty metres against waypoints ten kilometres apart,
/// so a curve stays well inside the corridor the search walked. Without
/// it a right angle at the full radius cuts 463 m off the corner, which
/// on a coast road is a curve out into the bay the route went round.
pub const CURVE_OFFSET: f64 = 250.0;

/// How far a vertex has to bend before a curve is worth fitting,
/// radians. A degree at the minimum radius is a tangent length of ten
/// metres, which is under one `PIECE` and is a corner nothing can feel.
const LEAST_BEND: f64 = 0.017;

/// The tightest curve worth building at all, metres. Under this the
/// vertex is left as it was: a curve a car cannot take is not an
/// improvement on a corner, it is the same corner with more points in
/// it, and what actually stops a road turning this tight is the router,
/// which will not step back the way it came.
const TIGHTEST: f64 = 40.0;

/// The road's own centreline as it is BUILT: the routed waypoints with a
/// circular curve fitted at every vertex that bends.
///
/// The result always starts at the first waypoint and ends at the last,
/// so a road still runs from the town it leaves to the town it reaches.
/// Between them a vertex is replaced by the arc that is tangent to both
/// of its legs, at the largest radius that fits (`fit`).
pub fn aligned(line: &[(DVec3, f64)], radius: f64) -> Vec<DVec3> {
    let pts: Vec<DVec3> = line.iter().map(|p| p.0).collect();
    if pts.len() < 3 || !radius.is_finite() || radius <= 0.0 {
        return pts;
    }
    let mut out = Vec::with_capacity(pts.len() * 2);
    out.push(pts[0]);
    for k in 1..pts.len() - 1 {
        curve(&mut out, [pts[k - 1], pts[k], pts[k + 1]], radius);
    }
    out.push(pts[pts.len() - 1]);
    // Two points a hair apart are a piece of no length, which is a
    // degenerate corridor arc and a station the ribbon cannot take an
    // across from. A metre is far under `PIECE` and far over the
    // rounding a tangent projection costs.
    out.dedup_by(|a, b| (*a - *b).length() * radius < 1.0);
    out
}

/// The curve at ONE vertex, appended: the arc from where it leaves the
/// incoming leg to where it meets the outgoing one, or the vertex itself
/// where there is no bend worth curving.
///
/// It is worked in the TANGENT PLANE at the vertex, because a tangent
/// length is at most a kilometre and a kilometre of a thousand kilometre
/// body is a milliradian: the chart's own error over that is
/// `L^3 / (3 R^2)`, a third of a millimetre, and every point comes back
/// through `normalize`, so nothing leaves the sphere.
fn curve(out: &mut Vec<DVec3>, at: [DVec3; 3], radius: f64) {
    let (a, b, c) = (at[0], at[1], at[2]);
    let (east, north) = frame_at(b);
    // The tangent projection of a direction, in metres from the vertex.
    let flat = |d: DVec3| {
        let v = (d - b * b.dot(d)) * radius;
        DVec2::new(v.dot(east), v.dot(north))
    };
    let round = |p: DVec2| (b * radius + east * p.x + north * p.y).normalize_or(b);
    let (back, on) = (flat(a), flat(c));
    let (len_back, len_on) = (back.length(), on.length());
    if !(len_back > 1.0 && len_on > 1.0) {
        out.push(b);
        return;
    }
    let (u, w) = (back / len_back, on / len_on);
    // The DEFLECTION: how far the heading turns at this vertex, which is
    // the supplement of the angle the two legs make there.
    let bend = std::f64::consts::PI - u.dot(w).clamp(-1.0, 1.0).acos();
    let half = bend * 0.5;
    let Some(r) = fit(half, len_back.min(len_on)) else {
        out.push(b);
        return;
    };
    let tangent = r * half.tan();
    // The centre stands on the bisector, `r / cos(half)` from the
    // vertex: that is the point exactly `r` from BOTH legs, which is
    // what makes the arc tangent to each.
    let Some(bisect) = (u + w).try_normalize() else {
        out.push(b);
        return;
    };
    let centre = bisect * (r / half.cos());
    let (entry, exit) = (u * tangent - centre, w * tangent - centre);
    let from = entry.y.atan2(entry.x);
    let sweep = wrapped(exit.y.atan2(exit.x) - from);
    // One station about every `PIECE`, which is the spacing the
    // centreline is refined at anyway: finer would be points the
    // corridor cannot tell apart and coarser would be a chord standing
    // off its own arc.
    let steps = ((r * sweep.abs() / PIECE).ceil() as usize).clamp(1, 256);
    for i in 0..=steps {
        let angle = from + sweep * (i as f64 / steps as f64);
        let (s, cs) = angle.sin_cos();
        out.push(round(centre + DVec2::new(cs, s) * r));
    }
}

/// The RADIUS a vertex's curve is built at: the design minimum, or the
/// largest that fits where the legs or the corner cutting will not take
/// it. Nothing at all where the bend is too slight to be worth a curve
/// or too tight for one to help.
///
/// Three limits, and `docs/civil-engineering.md` is each one's long
/// form. `CURVE` is the design speed's own minimum and is what a curve
/// wants to be. `CURVE_SHARE` is how much of the shorter leg either side
/// the tangent may eat, so two neighbouring curves cannot overlap: at
/// under a half each, the two together are under the whole leg by
/// construction. `CURVE_OFFSET` caps the MID ORDINATE, which is how far
/// the built road leaves the line the router chose over ground it
/// actually measured, and a curve that cut a headland off a coast road
/// would be a road in the sea.
fn fit(half: f64, leg: f64) -> Option<f64> {
    if !(half.is_finite() && half > LEAST_BEND * 0.5) {
        return None;
    }
    // A vertex that turns the road right back the way it came has no
    // bisector and no curve; the router cannot make one, since every
    // step of its search strictly lowers the distance it was reached at.
    if half >= std::f64::consts::FRAC_PI_2 - 1e-6 {
        return None;
    }
    let (tan, sec) = (half.tan(), 1.0 / half.cos());
    let by_leg = leg * CURVE_SHARE / tan;
    let by_offset = CURVE_OFFSET / (sec - 1.0);
    let r = CURVE.min(by_leg).min(by_offset);
    (r.is_finite() && r > TIGHTEST).then_some(r)
}

/// An angle brought into plus or minus half a turn: which way round the
/// arc goes is the SHORTER way, always, because the two legs of a bend
/// meet at under a straight angle by construction.
fn wrapped(mut a: f64) -> f64 {
    let turn = std::f64::consts::TAU;
    while a > std::f64::consts::PI {
        a -= turn;
    }
    while a < -std::f64::consts::PI {
        a += turn;
    }
    a
}

/// The tightest curve anywhere on a line, metres, and where: what a
/// harness prints to say whether an alignment is one a car can hold.
///
/// It is measured as the circle through three neighbouring points,
/// which is the discrete curvature of the polyline that is actually
/// built rather than of the arcs that were fitted to it: an arc walked
/// in `PIECE` chords is a polygon, and this is what a car following it
/// really has to turn.
///
/// It is therefore a measure of a line AT ITS OWN SPACING, and two
/// lines are only comparable through it when they are walked at the
/// same one: the routed waypoints are ten kilometres apart, so a whole
/// corner between two of them reads as a circle kilometres across,
/// which is a fact about the sampling and not about the road.
pub fn tightest(line: &[DVec3], radius: f64) -> (f64, usize) {
    let mut best = (f64::INFINITY, 0);
    for k in 1..line.len().saturating_sub(1) {
        let r = through(line[k - 1], line[k], line[k + 1], radius);
        if r < best.0 {
            best = (r, k);
        }
    }
    best
}

/// The radius of the circle through three neighbouring points of a
/// line, metres: `a b c / (4 A)` on the triangle they make, which is
/// infinite where they are in a row.
fn through(p: DVec3, q: DVec3, r: DVec3, radius: f64) -> f64 {
    let (east, north) = frame_at(q);
    let flat = |d: DVec3| {
        let v = (d - q * q.dot(d)) * radius;
        DVec2::new(v.dot(east), v.dot(north))
    };
    let (a, c) = (flat(p), flat(r));
    let (la, lc, lb) = (a.length(), c.length(), (c - a).length());
    let area2 = (a.x * c.y - a.y * c.x).abs();
    if area2 <= f64::MIN_POSITIVE {
        return f64::INFINITY;
    }
    la * lc * lb / (2.0 * area2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::road::{centreline, Road};

    /// A road of three waypoints making one bend of `bend` radians, the
    /// legs `leg` metres long, on a body of `radius`.
    fn corner(radius: f64, leg: f64, bend: f64) -> Road {
        let (east, north) = frame_at(DVec3::Y);
        let step = |from: DVec3, way: DVec3, run: f64| {
            let a = run / radius;
            (from * a.cos() + way * a.sin()).normalize()
        };
        let b = DVec3::Y;
        let a = step(b, -east, leg);
        let (s, c) = bend.sin_cos();
        let c = step(b, east * c + north * s, leg);
        Road {
            from: 0,
            to: 1,
            line: vec![(a, 0.0), (b, 0.0), (c, 0.0)],
        }
    }

    /// How far a direction stands off a polyline, metres.
    fn off(line: &[DVec3], p: DVec3, radius: f64) -> f64 {
        line.windows(2)
            .map(|w| {
                let (a, b) = (w[0] * radius, w[1] * radius);
                let run = b - a;
                let t = (p * radius - a).dot(run) / run.length_squared().max(1e-30);
                (p * radius - (a + run * t.clamp(0.0, 1.0))).length()
            })
            .fold(f64::INFINITY, f64::min)
    }

    /// The centreline a road would be built at with NO alignment: the
    /// routed waypoints cut into `PIECE` pieces and nothing else, which
    /// is what `centreline` did before there were curves.
    ///
    /// It is the BEFORE of the measurement below, and it has to be at
    /// the same spacing as the after: `tightest` is the circle through
    /// three NEIGHBOURING points, so it reads a corner as tighter the
    /// finer the line is walked, and comparing a 10 km polyline with an
    /// 85 m one would be comparing two samplings rather than two roads.
    fn unaligned(road: &Road, radius: f64) -> Vec<DVec3> {
        let mut out = Vec::new();
        for pair in road.line.windows(2) {
            let (a, b) = (pair[0].0, pair[1].0);
            let n = crate::road::pieces(a, b, radius);
            for k in 0..n {
                out.push(crate::road::step(a, b, k, n));
            }
        }
        out.extend(road.line.last().map(|p| p.0));
        out
    }

    /// A BEND IS A CURVE A CAR CAN HOLD. The routed polyline turns a
    /// whole corner at ONE vertex, so at the spacing the road is built
    /// at that corner is `PIECE / (2 sin(delta / 2))`, tens of metres of
    /// radius; the aligned one turns it on an arc of at least the design
    /// minimum.
    #[test]
    fn a_bend_is_a_curve_a_car_can_hold() {
        let radius = 1_000_000.0;
        for degrees in [10.0, 30.0, 45.0, 60.0, 90.0] {
            let road = corner(radius, 10_000.0, degrees * std::f64::consts::PI / 180.0);
            let raw: Vec<DVec3> = road.line.iter().map(|p| p.0).collect();
            let (before, _) = tightest(&unaligned(&road, radius), radius);
            let built = centreline(&road, radius);
            let (after, _) = tightest(&built, radius);
            // What `fit` promises: the design radius, or the largest
            // the corner cutting cap will take, whichever is smaller.
            // At ninety degrees the cap wins and the curve is built at
            // 604 m, which is 118 km/h rather than 160: a bend a real
            // road would sign, and still twenty times the corner it
            // replaces.
            let half = degrees * std::f64::consts::PI / 360.0;
            let want = CURVE.min(CURVE_OFFSET / (1.0 / half.cos() - 1.0));
            println!("{degrees:>3} degrees: {before:8.0} m routed, {after:8.0} m built");
            // The polygon an arc is walked as is never TIGHTER than the
            // arc itself, so the fitted radius is a floor on what comes
            // out and the rounding either way is the chord's.
            assert!(
                after >= want * 0.99,
                "{degrees} degrees bends at {after:.0} m against a fitted {want:.0} m"
            );
            assert!(
                after > before * 2.0,
                "{degrees} degrees: {before:.0} m routed against {after:.0} m built"
            );
            assert_eq!(built.first(), Some(&raw[0]), "a road starts where it did");
            assert_eq!(built.last(), Some(&raw[2]), "and ends where it did");
        }
    }

    /// A CURVE NEVER LEAVES THE ROUTE by more than its own mid ordinate,
    /// which is what keeps a road on the ground the router measured: a
    /// corner cut too hard is a coast road out in its own bay.
    #[test]
    fn a_curve_stays_on_the_ground_the_router_walked() {
        let radius = 1_000_000.0;
        let mut worst = 0.0f64;
        for degrees in [15.0, 45.0, 90.0, 140.0] {
            let road = corner(radius, 8_000.0, degrees * std::f64::consts::PI / 180.0);
            let raw: Vec<DVec3> = road.line.iter().map(|p| p.0).collect();
            for p in aligned(&road.line, radius) {
                worst = worst.max(off(&raw, p, radius));
            }
        }
        assert!(
            worst <= CURVE_OFFSET + 1.0,
            "a curve cuts {worst:.0} m off its own route against a cap of {CURVE_OFFSET:.0}"
        );
        println!("a curve cuts at most {worst:.1} m off the routed line");
    }

    /// A line with nothing to curve comes back as itself, and a body
    /// with a garbage radius is not a reason to hand back nothing.
    #[test]
    fn a_straight_road_is_left_alone() {
        let radius = 1_000_000.0;
        let straight = corner(radius, 10_000.0, 0.0);
        assert_eq!(aligned(&straight.line, radius).len(), 3);
        let two = vec![(DVec3::Y, 0.0), (DVec3::X, 0.0)];
        assert_eq!(aligned(&two, radius).len(), 2);
        assert_eq!(aligned(&two, -1.0).len(), 2);
        assert_eq!(aligned(&[], radius).len(), 0);
        assert!(tightest(&[], radius).0.is_infinite());
    }
}
