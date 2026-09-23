//! The ROUTE the markers ask for: the way over the roads from wherever
//! the eye is through every marker in turn, which the map draws, the
//! route panel and the compass strip measure, and a scripted drive
//! follows.
//!
//! The way is the core's (`road::path`): A* over the atlas's own
//! waypoints, laid onto the lines the roads are drawn on. What is here
//! is WHEN to ask, which is when a marker moves or the eye has gone far
//! enough that the first leg, which runs from it, is out of date.

use crate::map::{Markers, Whereabouts};
use crate::roads::Network;
use crate::world::World;
use bevy::ecs::system::SystemParam;
use bevy::math::DVec3;
use bevy::prelude::*;
use freeport_core::road::path;
use std::time::Instant;

/// How far the eye goes before its route is planned again, metres.
///
/// The first leg runs FROM the eye, so it goes stale as the car drives.
/// A way is a search of a few thousand waypoints and a trace a walk of
/// the few roads it takes, which is well under a millisecond, and a
/// hundred metres is two seconds at the car's own top speed: a route
/// that is never visibly behind the car.
const REPLAN: f64 = 100.0;

/// One leg of the route, from the eye or a marker to the next marker.
pub struct Leg {
    /// Every point it runs through, and whether the step INTO each is
    /// on tarmac: a step that is not is a HOP, onto the road from
    /// wherever the eye was, across a town between two roads, or off it
    /// to the marker.
    pub points: Vec<(DVec3, bool)>,
    /// How long it is over the ground, metres, hops and all.
    pub metres: f64,
    /// Whether it follows the roads at all. A marker no road reaches, on
    /// an island or across a sea, is a straight line to it, which is
    /// what the whole route was before there was a way to find.
    pub roads: bool,
}

/// The route as last planned: what it was planned for, and its legs.
#[derive(Resource, Default)]
pub struct Plan {
    /// The markers it runs through and where the eye was, so a plan is
    /// made again when either has moved and never when neither has.
    through: Vec<DVec3>,
    from: Option<DVec3>,
    pub legs: Vec<Leg>,
}

/// The markers and the route planned through them, which is what every
/// reader of a route asks for: the two together, because a plan read
/// without the markers it was made for is a frame's worth of stale
/// distance to a place a right click has just taken off the map.
#[derive(SystemParam)]
pub struct Planned<'w> {
    pub markers: Res<'w, Markers>,
    plan: Res<'w, Plan>,
}

impl Planned<'_> {
    /// Every leg, if the plan is for the markers as they stand.
    pub fn legs(&self) -> Option<&[Leg]> {
        (self.plan.through == self.markers.0).then_some(self.plan.legs.as_slice())
    }

    /// The first leg, if the plan is for the markers as they stand.
    pub fn first(&self) -> Option<&Leg> {
        self.legs()?.first()
    }
}

/// Plan the route again when a marker has moved or the eye has gone
/// `REPLAN` from where the last one was planned.
pub fn plan_route(
    markers: Res<Markers>,
    at: Whereabouts,
    network: Res<Network>,
    mut plan: ResMut<Plan>,
) {
    let (here, radius) = (at.here(), at.radius());
    let moved = plan
        .from
        .is_none_or(|f| f.angle_between(here) * radius > REPLAN);
    let changed = plan.through != markers.0;
    if !changed && !moved {
        return;
    }
    let t0 = Instant::now();
    let world = &at.ground.0;
    let mut legs = Vec::with_capacity(markers.0.len());
    let mut from = here;
    for &m in &markers.0 {
        legs.push(leg(&network, world, from, m, radius));
        from = m;
    }
    if changed && !legs.is_empty() {
        let roads: f64 = legs.iter().map(|l| l.metres).sum();
        let mut crow = 0.0;
        let mut from = here;
        for &m in &markers.0 {
            crow += from.angle_between(m) * radius;
            from = m;
        }
        info!(
            "route: {} legs, {:.1} km along the roads against {:.1} km as the crow flies, {} of them on no road, planned in {:.2} ms",
            legs.len(),
            roads / 1000.0,
            crow / 1000.0,
            legs.iter().filter(|l| !l.roads).count(),
            t0.elapsed().as_secs_f64() * 1000.0
        );
    }
    *plan = Plan {
        through: markers.0.clone(),
        from: Some(here),
        legs,
    };
}

/// One leg: the way over the roads from one place to another, traced
/// onto the lines they are drawn on, or a straight line where no road
/// joins the two.
fn leg(network: &Network, world: &World, from: DVec3, to: DVec3, radius: f64) -> Leg {
    let Some(way) = network.graph().way(from, to) else {
        return Leg {
            points: vec![(from, false), (to, false)],
            metres: from.angle_between(to) * radius,
            roads: false,
        };
    };
    let points = path::trace(
        &way,
        from,
        to,
        |r| {
            let route = world.routes.get(r)?;
            Some((route.line.as_slice(), route.open.as_slice()))
        },
        radius,
    );
    Leg {
        metres: path::length(&points, radius),
        points,
        roads: true,
    }
}

/// How far a car stands off a leg's TARMAC, metres, the segment it is
/// nearest and the point on it: measured to the LINE the road is drawn
/// on and never to its points, which stand a piece (85 m) apart on a
/// highway, and only over steps that are tarmac. The nearest POINT of a
/// leg is often the start of its first hop, which is where the car was
/// when the route was planned: a car standing there is on no road at
/// all, and taking it for one drove the hop straight at a building.
pub fn off_tarmac(points: &[(DVec3, bool)], at: DVec3, radius: f64) -> Option<(usize, f64, DVec3)> {
    (0..points.len().saturating_sub(1))
        .filter(|k| points[k + 1].1)
        .map(|k| {
            let (a, b) = (points[k].0, points[k + 1].0);
            let ab = b - a;
            let t = ((at - a).dot(ab) / ab.length_squared().max(1e-30)).clamp(0.0, 1.0);
            let foot = a + ab * t;
            (k, (at - foot).length() * radius, foot.normalize())
        })
        .min_by(|x, y| x.1.total_cmp(&y.1))
}

/// The fastest a car at `at`, on the tarmac from point `near` of a leg,
/// may go and still slow for every bend ahead of it in time, metres a
/// second: each bend is held at `driver::bend_speed` of its own
/// curvature, a bend `d` metres on allows `sqrt(v^2 + 2 decel d)` now,
/// and nothing further than `reach` metres can bind.
///
/// A bend is measured where the tarmac turns, over a step either side,
/// and never across a HOP, whose line is the way the route was planned
/// rather than a road anybody drives. A slip's three metre pieces
/// turning a fifth of a radian each are a fifteen metre bend, which is
/// the bend a car at the top speed ran wide of onto the grass.
pub fn bend_limit(
    points: &[(DVec3, bool)],
    near: usize,
    at: DVec3,
    decel: f64,
    reach: f64,
    radius: f64,
) -> f64 {
    let mut limit = freeport_core::driver::TOP;
    let Some(first) = points.get(near + 1) else {
        return limit;
    };
    let mut gone = first.0.angle_between(at) * radius;
    for k in near + 1..points.len().saturating_sub(1) {
        if gone > reach {
            break;
        }
        if points[k].1 && points[k + 1].1 {
            let (u, v) = (points[k].0 - points[k - 1].0, points[k + 1].0 - points[k].0);
            let run = (u.length() + v.length()) * 0.5 * radius;
            if run > 0.0 {
                let hold = freeport_core::driver::bend_speed(u.angle_between(v) / run);
                limit = limit.min((hold * hold + 2.0 * decel * gone).sqrt());
            }
        }
        gone += points[k].0.angle_between(points[k + 1].0) * radius;
    }
    limit
}

/// How far along a route a car looks, and what it will cut.
pub struct Look {
    /// The furthest along the route it looks, metres.
    pub ahead: f64,
    /// The longest hop it drives straight across like tarmac, metres.
    pub short: f64,
    /// How far off the route the straight line to where it is looking
    /// may stand, metres: half the carriageway, so the line it steers
    /// along is on the tarmac.
    pub lane: f64,
}

/// Where a car at `at`, nearest point `near` of a leg, steers for along
/// it, and whether the step past that is a HOP, which is where the car
/// has to find its own way through a town.
///
/// As far on as `ahead`, and never further than the straight line to it
/// stays within `lane` of every point of the route it passes. That is
/// what makes one look ahead right on a highway and on a slip alike: a
/// curve of 1,116 m holds the chord to 157 m, and a slip's fifteen metre
/// turns hold it to a few, where a flat 400 m aimed the car across the
/// corner of a town and into the building standing on it.
pub fn ahead_on(
    points: &[(DVec3, bool)],
    at: DVec3,
    near: usize,
    look: &Look,
    radius: f64,
) -> (usize, bool) {
    let mut gone = 0.0;
    let mut k = near;
    while k + 1 < points.len() {
        let step = points[k].0.angle_between(points[k + 1].0) * radius;
        if !points[k + 1].1 && step > look.short {
            return (k, true);
        }
        // The next point would take the line off the tarmac: this one,
        // and never nothing, since a car off its line still steers back.
        if k > near && !straight(points, at, near, k + 1, look.lane, radius) {
            return (k, false);
        }
        gone += step;
        k += 1;
        if gone >= look.ahead {
            break;
        }
    }
    (k, false)
}

/// Whether the straight line from `at` to point `to` of a leg stays
/// within `lane` metres of every point of it between `near` and `to`.
/// A chord of a few hundred metres on a body this size sags a few
/// centimetres, so it is taken as straight.
fn straight(
    points: &[(DVec3, bool)],
    at: DVec3,
    near: usize,
    to: usize,
    lane: f64,
    radius: f64,
) -> bool {
    let b = points[to].0;
    let ab = b - at;
    let long = ab.length_squared().max(1e-30);
    points[near + 1..to].iter().all(|(p, _)| {
        let t = ((*p - at).dot(ab) / long).clamp(0.0, 1.0);
        (*p - (at + ab * t)).length() * radius <= lane
    })
}

/// The first point at or after `from` where the tarmac starts again:
/// where a car across a hop is headed.
pub fn tarmac_after(points: &[(DVec3, bool)], from: usize) -> usize {
    (from + 1..points.len())
        .find(|k| points[*k].1)
        .map_or(points.len().saturating_sub(1), |k| k - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: f64 = 1_000_000.0;

    /// A straight leg along the equator a point every hundred metres,
    /// with a hop at `hop` and tarmac everywhere else.
    fn leg(n: usize, hop: usize) -> Vec<(DVec3, bool)> {
        (0..n)
            .map(|k| {
                let lon = k as f64 * 100.0 / R;
                (DVec3::new(lon.cos(), 0.0, lon.sin()), k != 0 && k != hop)
            })
            .collect()
    }

    /// A car on a leg steers for the point `ahead` metres on, and stops
    /// short of the tarmac running out rather than aiming across it.
    #[test]
    fn a_car_looks_ahead_along_the_tarmac_and_not_across_a_hop() {
        let points = leg(40, 20);
        let at = points[5].0;
        let (seg, off, _) = off_tarmac(&points, at, R).expect("the leg has tarmac");
        assert_eq!(seg, 4);
        assert!(off < 1e-6);
        let near = seg + 1;
        let look = |ahead: f64, short: f64| Look {
            ahead,
            short,
            lane: 2.75,
        };
        assert_eq!(
            ahead_on(&points, at, near, &look(350.0, 50.0), R),
            (9, false)
        );
        // Near the hop it stops at the last tarmac and says so.
        let at17 = points[17].0;
        assert_eq!(
            ahead_on(&points, at17, 17, &look(400.0, 50.0), R),
            (19, true)
        );
        // And a hop shorter than the car will drive across is tarmac.
        assert_eq!(
            ahead_on(&points, at17, 17, &look(350.0, 150.0), R),
            (21, false)
        );
        assert_eq!(
            ahead_on(&points, points[0].0, 0, &look(150.0, 150.0), R),
            (2, false)
        );
        // And across it, the tarmac starts again at the point the hop
        // lands on.
        assert_eq!(tarmac_after(&points, 19), 20);
        assert_eq!(points[21].1, true);
    }

    /// Off the tarmac is measured to the road's LINE, between its points,
    /// and over tarmac only: a car halfway between two points a hundred
    /// metres apart is on the road, and a car thirty metres into a hop is
    /// thirty metres from the tarmac it left and never on the hop.
    #[test]
    fn off_the_tarmac_is_measured_to_the_line_and_never_to_a_hop() {
        let points = leg(40, 20);
        // Halfway between points 5 and 6, on the line: on the tarmac, to
        // the chord's own sagitta (a hundred metres of arc stands 1.25 mm
        // off its chord at a thousand kilometres).
        let mid = (points[5].0 + points[6].0).normalize();
        let (seg, off, _) = off_tarmac(&points, mid, R).expect("the leg has tarmac");
        assert_eq!(seg, 5);
        assert!(off < 0.01, "{off}");
        // Ten metres north of it: ten metres off.
        let beside = (mid + DVec3::Y * 10.0 / R).normalize();
        let (_, off, _) = off_tarmac(&points, beside, R).expect("the leg has tarmac");
        assert!((off - 10.0).abs() < 0.01, "{off}");
        // The step into point 20 is the hop: thirty metres along it the
        // car is thirty metres past the tarmac's end at point 19.
        let lon = (19.0 * 100.0 + 30.0) / R;
        let into = DVec3::new(lon.cos(), 0.0, lon.sin());
        let (seg, off, foot) = off_tarmac(&points, into, R).expect("the leg has tarmac");
        println!("thirty metres into the hop: {off:.2} m off segment {seg}");
        assert_eq!(seg, 18);
        assert!((off - 30.0).abs() < 0.01, "{off}");
        assert!(foot.angle_between(points[19].0) * R < 1e-3);
    }

    /// A straight is taken at the top speed, a fifteen metre bend at the
    /// speed the car holds one, and the same bend a hundred metres off at
    /// whatever braking at `decel` over those hundred metres leaves.
    #[test]
    fn a_car_slows_for_a_bend_in_time_and_not_for_a_straight() {
        let top = freeport_core::driver::TOP;
        let straight = leg(40, 20);
        assert_eq!(bend_limit(&straight, 2, straight[2].0, 8.0, 500.0, R), top);
        // A hundred and twenty metres straight, then points three metres
        // apart turning a fifth of a radian each: a fifteen metre bend.
        let mut points = vec![(DVec3::Z, true)];
        let (mut at, mut heading) = (DVec3::Z, DVec3::X);
        for k in 0..60 {
            if k >= 40 {
                heading = bevy::math::DQuat::from_axis_angle(at, 0.2) * heading;
            }
            at = (at + heading * (3.0 / R)).normalize();
            heading = (heading - at * heading.dot(at)).normalize();
            points.push((at, true));
        }
        let hold = freeport_core::driver::bend_speed(1.0 / 15.0);
        // A step short of the first turning point, which is point 40.
        let on = bend_limit(&points, 39, points[39].0, 8.0, 500.0, R);
        let want = (hold * hold + 2.0 * 8.0 * 3.0).sqrt();
        println!("a step short of the bend {on:.2} m/s against {want:.2}");
        assert!(
            (on - want).abs() < 0.2,
            "{on} a step short of the bend against {want}"
        );
        // And thirty four steps short of it.
        let before = bend_limit(&points, 6, points[6].0, 8.0, 500.0, R);
        let want = (hold * hold + 2.0 * 8.0 * 102.0).sqrt();
        println!("a hundred metres before it {before:.2} m/s against {want:.2}");
        assert!((before - want).abs() < 1.0, "{before} against {want}");
        // And out of reach it does not bind at all.
        assert_eq!(bend_limit(&points, 6, points[6].0, 8.0, 50.0, R), top);
    }

    /// Round a BEND the car looks only as far as the straight line to
    /// the point stays on the tarmac, and on the straight as far as it
    /// is let: a quarter circle of 15 m, a slip's own turn, holds it to
    /// a few metres where 400 m would cut the corner.
    #[test]
    fn a_car_looks_less_far_round_a_bend_than_down_a_straight() {
        let turn = 15.0;
        let east = |x: f64, y: f64| {
            let (lon, lat) = (x / R, y / R);
            (
                DVec3::new(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin()),
                true,
            )
        };
        // Straight for 60 m, a quarter turn of 15 m in 1 m pieces, then
        // straight north for 300 m.
        let mut points: Vec<(DVec3, bool)> = (0..=60).map(|k| east(k as f64, 0.0)).collect();
        for k in 1..=24 {
            let a = k as f64 / 24.0 * std::f64::consts::FRAC_PI_2;
            points.push(east(60.0 + turn * a.sin(), turn * (1.0 - a.cos())));
        }
        for k in 1..=300 {
            points.push(east(75.0, 15.0 + k as f64));
        }
        let look = Look {
            ahead: 400.0,
            short: 50.0,
            lane: 2.75,
        };
        let (far, _) = ahead_on(&points, points[0].0, 0, &look, R);
        let (bend, _) = ahead_on(&points, points[55].0, 55, &look, R);
        let reach = |k: usize, from: usize| points[k].0.angle_between(points[from].0) * R;
        println!(
            "on the straight it looks {:.0} m, at the bend {:.0} m",
            reach(far, 0),
            reach(bend, 55)
        );
        assert!(far <= 60 + 24, "the straight's look stops at the bend");
        assert!(reach(far, 0) > 55.0, "and reaches most of the straight");
        assert!(reach(bend, 55) < 20.0, "at the bend it looks a few metres");
    }
}
