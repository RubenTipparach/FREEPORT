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

/// Where along a leg a car is: the index of the point of it nearest the
/// car and how far off that point the car stands, metres.
pub fn progress(points: &[(DVec3, bool)], at: DVec3, radius: f64) -> Option<(usize, f64)> {
    let near = (0..points.len()).min_by(|a, b| {
        (points[*a].0 - at)
            .length_squared()
            .total_cmp(&(points[*b].0 - at).length_squared())
    })?;
    Some((near, points[near].0.angle_between(at) * radius))
}

/// Where a car at point `near` of a leg steers for along it: `ahead`
/// metres on, or the last point before the tarmac stops, whichever is
/// first; and whether the step past that is a HOP, which is where the
/// car has to find its own way through a town. A hop no longer than
/// `short` metres is driven straight across like tarmac.
pub fn ahead_on(
    points: &[(DVec3, bool)],
    near: usize,
    ahead: f64,
    radius: f64,
    short: f64,
) -> (usize, bool) {
    let mut gone = 0.0;
    let mut k = near;
    while k + 1 < points.len() {
        let step = points[k].0.angle_between(points[k + 1].0) * radius;
        if !points[k + 1].1 && step > short {
            return (k, true);
        }
        gone += step;
        k += 1;
        if gone >= ahead {
            break;
        }
    }
    (k, false)
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
        let (near, off) = progress(&points, at, R).expect("the leg has points");
        assert_eq!(near, 5);
        assert!(off < 1e-6);
        assert_eq!(ahead_on(&points, near, 350.0, R, 50.0), (9, false));
        // Near the hop it stops at the last tarmac and says so.
        assert_eq!(ahead_on(&points, 17, 400.0, R, 50.0), (19, true));
        // And a hop shorter than the car will drive across is tarmac.
        assert_eq!(ahead_on(&points, 17, 350.0, R, 150.0), (21, false));
        assert_eq!(ahead_on(&points, 0, 150.0, R, 150.0), (2, false));
        // And across it, the tarmac starts again at the point the hop
        // lands on.
        assert_eq!(tarmac_after(&points, 19), 20);
        assert_eq!(points[21].1, true);
    }
}
