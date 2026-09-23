//! RAMMING: what two cars do to each other when one drives into the
//! other.
//!
//! The owner's ask: "when I ram other cars only I get affected by
//! physics, both cars should be affected, I should be able to ram others
//! off the road." A car on the rails is a closed form function of its
//! town or its road and the clock, and it cannot be moved, because
//! moving it would be state; so the moment a car is HIT it comes off
//! the rails for good, exactly as a stolen one does, and from then on it
//! is a `driver::Driver` with nobody at the wheel: it takes the shove,
//! slides on its tyres, turns if it was struck off centre, and rolls to
//! a stop wherever that puts it. What the two exchange is here, in the
//! core, because it is a rule two clients have to agree on.
//!
//! Equal masses and a restitution, which is a car to car collision to
//! first order: the closing speed along the contact's normal is shared
//! out, each car's own changes by `(1 + RESTITUTION) / 2` of it, so
//! momentum is kept exactly and `RESTITUTION` of the closing speed comes
//! back as separation. A car is crumple and not a billiard ball, and
//! crash tests put a car to car impact's restitution between 0.2 and
//! 0.5; 0.3 is the middle of that. The normal is the least penetrated
//! of the two cars' own axes, which is the FACE the hit is on, and the
//! contact is the corner (or the edge) driven deepest into it: a hit off
//! the struck car's middle turns it by that lever over a rectangle's own
//! moment of inertia, `(L^2 + W^2) / 12`.

use crate::figure::{CAR_LONG, CAR_WIDE};
use glam::DVec3;

/// How much of the closing speed comes back as separation. A car is
/// crumple: crash tests put it between 0.2 and 0.5.
pub const RESTITUTION: f64 = 0.3;
/// How near two outlines have to stand to be touching, metres: a
/// `Driver::STEP`, which is the furthest apart two cars can be at the
/// end of a frame that would have met inside it.
pub const TOUCH: f64 = 0.25;
/// How much of a rigid body's turn a car actually takes when struck off
/// centre. Its tyres resist a spin the way they resist a slide, so it
/// is a share and not the whole: at one a rear quarter hit at 30 km/h
/// turned a car twice round, which is a film and not a road.
pub const SPIN_SHARE: f64 = 0.35;
/// A rectangle's moment of inertia about its own up, per unit mass,
/// square metres.
const INERTIA: f64 = (CAR_LONG * CAR_LONG + CAR_WIDE * CAR_WIDE) / 12.0;
/// How near the furthest corner along a normal another has to be to
/// count as the same edge, metres, so a square hit lands on the middle
/// of a bumper and not on one of its two corners.
const EDGE: f64 = 0.05;

/// One car as a collision sees it: where its middle is in the planet's
/// frame (metres), which way it points, and its velocity in the tangent
/// plane (metres a second).
#[derive(Clone, Copy, Debug)]
pub struct Body {
    pub at: DVec3,
    pub fwd: DVec3,
    pub vel: DVec3,
}

/// What a hit does: the normal from the first car to the second, the
/// speed each car's own changes by along it (the second gains it and the
/// first loses it), and the turn the second takes, radians a second,
/// anticlockwise from above.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub normal: DVec3,
    pub dv: f64,
    pub spin: f64,
}

/// A body's frame: up (the radial), right and forward, squared to the up.
fn frame(b: &Body) -> [DVec3; 3] {
    let up = b.at.normalize_or(DVec3::Y);
    let fwd = (b.fwd - up * b.fwd.dot(up)).normalize_or(DVec3::X);
    [up, fwd.cross(up).normalize_or(DVec3::Z), fwd]
}

/// The four corners of a car's outline, in the planet's frame.
fn corners(b: &Body) -> [DVec3; 4] {
    let [_, right, fwd] = frame(b);
    let (w, l) = (right * (CAR_WIDE * 0.5), fwd * (CAR_LONG * 0.5));
    [b.at + w + l, b.at + w - l, b.at - w - l, b.at - w + l]
}

/// The gap between two outlines along one axis, metres: negative where
/// they overlap along it.
fn gap(axis: DVec3, a: &[DVec3; 4], b: &[DVec3; 4]) -> f64 {
    let span = |c: &[DVec3; 4]| {
        c.iter()
            .map(|p| p.dot(axis))
            .fold((f64::MAX, f64::MIN), |(lo, hi), x| (lo.min(x), hi.max(x)))
    };
    let ((a0, a1), (b0, b1)) = (span(a), span(b));
    (b0 - a1).max(a0 - b1)
}

/// The corners of an outline driven furthest along `n`: how far along
/// `n` they stand, and the span they cover along `t`, the face's own
/// direction. One corner, or the whole edge where two stand level.
fn edge(corners: &[DVec3; 4], n: DVec3, t: DVec3) -> (f64, f64, f64) {
    let top = corners.iter().map(|c| c.dot(n)).fold(f64::MIN, f64::max);
    let mut span = (f64::MAX, f64::MIN);
    for c in corners.iter().filter(|c| c.dot(n) > top - EDGE) {
        let x = c.dot(t);
        span = (span.0.min(x), span.1.max(x));
    }
    (top, span.0, span.1)
}

/// How far apart two cars' outlines stand, metres, by the separating
/// axis test on both cars' own axes in the plane: negative where they
/// overlap, and the axis it was measured on, which is the face a hit
/// lands on.
pub fn apart(a: &Body, b: &Body) -> (f64, DVec3) {
    let (fa, fb) = (frame(a), frame(b));
    let (ca, cb) = (corners(a), corners(b));
    let mut best = (f64::MIN, fa[2]);
    for axis in [fa[1], fa[2], fb[1], fb[2]] {
        let g = gap(axis, &ca, &cb);
        if g > best.0 {
            best = (g, axis);
        }
    }
    best
}

/// What happens when `a` and `b` are touching and closing: nothing where
/// they are apart or parting, else the hit.
pub fn impact(a: &Body, b: &Body) -> Option<Hit> {
    let (gap, axis) = apart(a, b);
    if gap > TOUCH {
        return None;
    }
    let up = frame(a)[0];
    let line = b.at - a.at;
    let normal = if line.dot(axis) < 0.0 { -axis } else { axis };
    let normal = (normal - up * normal.dot(up)).normalize_or(frame(a)[2]);
    let closing = (a.vel - b.vel).dot(normal);
    if closing <= 0.0 {
        return None;
    }
    let dv = closing * (1.0 + RESTITUTION) * 0.5;
    // The contact is where the two edges driven together OVERLAP along
    // the face: a nose square on a tail meets it at the middle of the
    // bumper, and a nose on a flank behind the other car's middle meets
    // it there, which is the lever that turns it. Asking whose face the
    // axis was found nothing, because a nose on a flank is on both.
    let t = up.cross(normal).normalize_or(frame(a)[1]);
    let (na, a0, a1) = edge(&corners(a), normal, t);
    let (nb, b0, b1) = edge(&corners(b), -normal, t);
    let along = (a0.max(b0) + a1.min(b1)) * 0.5;
    let deep = (na - nb) * 0.5;
    let arm = normal * (deep - b.at.dot(normal)) + t * (along - b.at.dot(t));
    let spin = SPIN_SHARE * arm.cross(normal * dv).dot(up) / INERTIA;
    Some(Hit { normal, dv, spin })
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: f64 = 2000.0;

    /// A car at the north pole moved `x` right and `z` forward of it,
    /// pointing along `fwd`, going at `vel`.
    fn car(x: f64, z: f64, fwd: DVec3, vel: DVec3) -> Body {
        Body {
            at: DVec3::new(x, R, z),
            fwd,
            vel,
        }
    }

    #[test]
    fn a_car_rammed_square_from_behind_takes_the_speed_and_does_not_turn() {
        // A points along z at 16 m/s; B stands a hand ahead of its nose.
        let a = car(0.0, 0.0, DVec3::Z, DVec3::Z * 16.0);
        let b = car(0.0, CAR_LONG + 0.1, DVec3::Z, DVec3::ZERO);
        let hit = impact(&a, &b).expect("a car a hand off another's tail is touching it");
        println!("{hit:?}");
        assert!(hit.normal.abs_diff_eq(DVec3::Z, 1e-9), "{:?}", hit.normal);
        let want = 16.0 * (1.0 + RESTITUTION) * 0.5;
        assert!((hit.dv - want).abs() < 1e-9, "{} against {want}", hit.dv);
        assert!(
            hit.spin.abs() < 1e-9,
            "a square hit turned it at {}",
            hit.spin
        );
        // Momentum: what B gains A loses, so the two sum to what A had.
        let (va, vb) = (16.0 - hit.dv, hit.dv);
        assert!((va + vb - 16.0).abs() < 1e-9);
        // And the separation is RESTITUTION of the closing speed.
        assert!((vb - va - RESTITUTION * 16.0).abs() < 1e-9);
    }

    #[test]
    fn a_car_struck_on_its_rear_quarter_is_turned_and_one_hit_across_is_shoved() {
        // B stands across A's path, pointing right (x), and A's nose
        // meets B's flank behind B's own middle: B is shoved along z
        // and its nose swings toward A's line.
        let a = car(0.0, 0.0, DVec3::Z, DVec3::Z * 10.0);
        let b = car(
            1.2,
            CAR_LONG * 0.5 + CAR_WIDE * 0.5 + 0.1,
            DVec3::X,
            DVec3::ZERO,
        );
        let hit = impact(&a, &b).expect("a nose against a flank is touching it");
        println!("{hit:?}");
        assert!(hit.normal.abs_diff_eq(DVec3::Z, 1e-9), "{:?}", hit.normal);
        assert!(hit.dv > 6.0);
        // A's nose is LEFT of B's middle along B's own axis (B points
        // along x and A stands at x = 0 under B's x = 1.2), so the push
        // on B's tail turns B's nose the other way, anticlockwise from
        // above, which is a positive spin.
        assert!(
            hit.spin > 0.5,
            "the quarter hit turned it at only {}",
            hit.spin
        );
        assert!(hit.spin < 8.0, "the quarter hit spun it at {}", hit.spin);
    }

    #[test]
    fn cars_apart_or_parting_do_not_hit() {
        let a = car(0.0, 0.0, DVec3::Z, DVec3::Z * 16.0);
        let far = car(0.0, CAR_LONG + TOUCH + 0.5, DVec3::Z, DVec3::ZERO);
        assert!(impact(&a, &far).is_none(), "a car a metre off was hit");
        let beside = car(CAR_WIDE + TOUCH + 0.2, 0.0, DVec3::Z, DVec3::ZERO);
        assert!(
            impact(&a, &beside).is_none(),
            "a car in the next lane was hit"
        );
        // Touching but going the same way faster: parting, not closing.
        let ahead = car(0.0, CAR_LONG + 0.1, DVec3::Z, DVec3::Z * 20.0);
        assert!(impact(&a, &ahead).is_none(), "a car pulling away was hit");
        // And the one BEHIND that is closing on a: the hit is on it.
        let behind = car(0.0, -(CAR_LONG + 0.1), DVec3::Z, DVec3::Z * 30.0);
        let hit = impact(&a, &behind).expect("a car closing from behind");
        assert!(hit.normal.abs_diff_eq(-DVec3::Z, 1e-9));
        assert!((hit.dv - 14.0 * (1.0 + RESTITUTION) * 0.5).abs() < 1e-9);
    }
}
