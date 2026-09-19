//! Continuous flight limits and collision, in a body's local f64 frame.

use crate::field::{site_band, Density, Planet, Sphere};
use glam::DVec3;

/// Smoothly reduce a requested cruise speed through the atmosphere. The
/// selected speed is preserved; even boost obeys the near-ground limit.
pub fn approach_speed(requested: f64, minimum: f64, height: f64, atmosphere: f64) -> f64 {
    let low = minimum.min(requested);
    let t = (height / atmosphere.max(1.0)).clamp(0.0, 1.0);
    let blend = t * t * (3.0 - 2.0 * t);
    low + (requested - low) * blend
}

/// Sweep the entire segment against a density field, including paths whose
/// two endpoints are outside the same planet. Density is NOT a distance:
/// its slope bound converts each sample into a provably empty step. A work
/// limit stops at the last safe position, never skips an unchecked interval.
/// `clearance` is a negative density margin, in the field's units.
pub fn sweep(field: &dyn Density, from: DVec3, to: DVec3, clearance: f64) -> DVec3 {
    if !from.is_finite() || !to.is_finite() {
        return from;
    }
    let delta = to - from;
    let length = delta.length();
    if length == 0.0 {
        return from;
    }
    let direction = delta / length;
    let slope = field.slope().max(1.0);
    let mut distance = 0.0;
    for _ in 0..4096 {
        let at = from + direction * distance;
        let safe = (-field.at(at) - clearance) / slope;
        if !safe.is_finite() || safe <= 1e-7 {
            return at;
        }
        if distance + safe >= length {
            return to;
        }
        distance += safe * 0.9;
    }
    from + direction * distance
}

/// Sweep with the slope of terrain that can actually influence this segment.
/// Town skirts can be much steeper than wilderness. Including a remote skirt
/// in every ray step makes contact recovery thousands of times too cautious.
/// Normalized points on a segment follow the shorter arc between its endpoint
/// directions, so their chord from the first direction never exceeds `span`.
pub fn sweep_planet(planet: &Planet, from: DVec3, to: DVec3, clearance: f64) -> DVec3 {
    if !from.is_finite() || !to.is_finite() {
        return sweep(planet, from, to, clearance);
    }
    if planet.sites.is_empty() {
        return sweep_along(planet, from, to, clearance);
    }
    let first = from.normalize_or(DVec3::Y);
    let last = to.normalize_or(first);
    // The small expansion covers rounding in normalization at planet scale.
    let span = (first - last).length() + 1e-12;
    let mut earlier_site = false;
    for site in &planet.sites {
        let (inner, _) = site_band(site);
        let inside = 2.0 * (inner / (2.0 * planet.radius)).sin();
        let separation = (first - site.dir).length();
        if !earlier_site && separation + span < inside {
            return sweep(
                &Sphere {
                    radius: planet.radius + site.h,
                },
                from,
                to,
                clearance,
            );
        }
        earlier_site |= separation - span <= inside;
    }
    sweep_along(&planet.around(first, span), from, to, clearance)
}

struct Along<'a> {
    planet: &'a Planet,
    slope: f64,
}

impl Density for Along<'_> {
    fn at(&self, p: DVec3) -> f64 {
        self.planet.at(p)
    }

    fn slope(&self) -> f64 {
        self.slope
    }
}

fn sweep_along(planet: &Planet, from: DVec3, to: DVec3, clearance: f64) -> DVec3 {
    let delta = to - from;
    let length = delta.length();
    if length == 0.0 {
        return from;
    }
    let direction = delta / length;
    let closest = from + direction * (-from.dot(direction)).clamp(0.0, length);
    let near = closest.length();
    if near < planet.radius * 0.5 {
        return sweep(planet, from, to, clearance);
    }
    // Along this line |d(normalize(p))/dt| = |from x direction| / |p|^2.
    // Radial travel has no relief or skirt derivative. Keep the full bound
    // for the 3D carve, which can change even during a perfectly radial step.
    let angular = planet.radius * from.cross(direction).length() / (near * near) + 1e-10;
    let mut radial = planet.clone();
    radial.relief = 0.0;
    radial.sites.clear();
    let slope = radial.slope() + planet.slope() * angular;
    sweep(&Along { planet, slope }, from, to, clearance)
}

/// Distance along a unit ray to entering a sphere, or infinity if it misses.
/// Used to split a space-speed step exactly at an atmosphere's boundary.
pub fn entry_distance(at: DVec3, direction: DVec3, radius: f64) -> f64 {
    let b = at.dot(direction);
    let c = at.length_squared() - radius * radius;
    if c <= 0.0 {
        return 0.0;
    }
    let discriminant = b * b - c;
    if b >= 0.0 || discriminant < 0.0 {
        f64::INFINITY
    } else {
        // Rationalized root avoids cancellation when already near the shell.
        c / (-b + discriminant.sqrt())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::town::Site;

    #[test]
    fn speed_is_continuous_monotone_and_never_increases_a_slow_selection() {
        let mut previous = 0.0;
        for i in 0..=1000 {
            let speed = approach_speed(2e6, 6.0, i as f64 * 60.0, 60_000.0);
            assert!(speed >= previous && speed <= 2e6);
            previous = speed;
        }
        assert_eq!(approach_speed(2e6, 6.0, -1.0, 60_000.0), 6.0);
        assert_eq!(approach_speed(2e6, 6.0, 60_000.0, 60_000.0), 2e6);
        assert_eq!(approach_speed(0.25, 6.0, 10.0, 60_000.0), 0.25);
    }

    #[test]
    fn sweep_cannot_tunnel_even_when_both_endpoints_are_clear() {
        let planet = Sphere { radius: 1e6 };
        let start = DVec3::Y * 2e6;
        let stop = sweep(&planet, start, -start, 0.5);
        assert!((stop.y - 1_000_000.5).abs() < 1e-5);
        assert!(planet.at(stop) <= -0.5);
        assert_eq!(
            sweep(&planet, start, start + DVec3::X * 2e6, 0.5),
            start + DVec3::X * 2e6
        );
    }

    #[test]
    fn rough_ground_and_cliffs_are_swept_using_the_density_slope() {
        let planet = Planet {
            radius: 1000.0,
            relief: 120.0,
            overhang: 8.0,
            ledge: 12.0,
            octaves: 8,
            ..Default::default()
        };
        for i in 0..64 {
            let direction = DVec3::new(i as f64 * 0.13 - 4.0, 1.0, 0.31).normalize();
            let start = direction * 1300.0;
            let end = sweep(&planet, start, -start, 0.5);
            assert!(planet.at(end) <= -0.5, "penetration at {end}");
            assert!(end.dot(direction) > 900.0);
        }
    }

    #[test]
    fn entry_handles_large_offsets_tangents_misses_and_inside() {
        assert_eq!(entry_distance(DVec3::Y * 20.0, DVec3::NEG_Y, 10.0), 10.0);
        assert_eq!(entry_distance(DVec3::ZERO, DVec3::Y, 10.0), 0.0);
        assert!(entry_distance(DVec3::Y * 20.0, DVec3::Y, 10.0).is_infinite());
        assert_eq!(
            entry_distance(DVec3::new(10.0, 20.0, 0.0), DVec3::NEG_Y, 10.0),
            20.0
        );
    }

    #[test]
    fn a_flat_town_recovers_to_contact_without_a_remote_skirt_work_limit() {
        let planet = Planet {
            octaves: 18,
            sites: vec![Site {
                dir: DVec3::Y,
                h: 1200.0,
                r: 160.0,
            }],
            ..Default::default()
        };
        let landed = sweep_planet(&planet, DVec3::Y * 1_004_100.0, DVec3::ZERO, 0.51);
        assert!((landed.y - 1_001_200.51).abs() < 1e-5);
        assert!(planet.at(landed) <= -0.51);
        let takeoff = landed + DVec3::Y * 0.1;
        assert_eq!(sweep_planet(&planet, landed, takeoff, 0.5), takeoff);
    }

    /// A sweep between two CLEAR endpoints cannot step over the ground
    /// between them, and a site's skirt is the hard case because it is
    /// the steepest thing on a body.
    ///
    /// Two pans rather than one mesa: a site CUTS now, so it can no
    /// longer stand a mound in a flier's way, and a fixture whose site
    /// was above the ground is a fixture with nothing in it at all. Two
    /// towns dug into a plain with untouched ground between them put the
    /// PLAIN in the way, and a line from inside one pan to inside the
    /// other runs straight through it.
    #[test]
    fn local_slope_keeps_skirts_crossed_between_clear_endpoints() {
        let pan = |x: f64| Site {
            dir: DVec3::new(x, 1000.0, 0.0).normalize(),
            h: -20.0,
            r: 40.0,
        };
        let planet = Planet {
            radius: 1000.0,
            relief: 0.0,
            overhang: 0.0,
            sites: vec![pan(-60.0), pan(60.0)],
            ..Default::default()
        };
        let start = DVec3::new(-60.0, 985.0, 0.0);
        let end = DVec3::new(60.0, 985.0, 0.0);
        assert!(planet.at(start) < -0.5 && planet.at(end) < -0.5);
        let stopped = sweep_planet(&planet, start, end, 0.5);
        assert!(stopped.x < 0.0);
        for i in 0..=1000 {
            assert!(planet.at(start.lerp(stopped, i as f64 / 1000.0)) <= -0.5);
        }
    }

    #[test]
    fn remote_towns_cannot_slow_wilderness_collision() {
        let mut planet = Planet {
            octaves: 18,
            ..Default::default()
        };
        let start = DVec3::X * 1_010_000.0;
        let end = DVec3::X * 990_000.0;
        let expected = sweep_planet(&planet, start, end, 0.5);
        planet.sites.push(Site {
            dir: DVec3::Y,
            h: 1000.0,
            r: 160.0,
        });
        assert_eq!(sweep_planet(&planet, start, end, 0.5), expected);
        assert!(planet.at(expected) <= -0.5);
    }

    #[test]
    fn radial_takeoff_on_a_skirt_keeps_its_distance_and_carve_collision() {
        let planet = Planet {
            octaves: 18,
            sites: vec![Site {
                dir: DVec3::Y,
                h: 1200.0,
                r: 160.0,
            }],
            ..Default::default()
        };
        // ON THE SKIRT, which is the middle of the band rather than a
        // hard coded 80 m: `site_band` is what says where a skirt is.
        let (inner, outer) = crate::field::site_band(&planet.sites[0]);
        let across = (inner + outer) * 0.5 / planet.radius;
        let direction = DVec3::new(across, 1.0, 0.0).normalize();
        let ground = crate::town::surface_radius(&planet, direction);
        let landed = sweep_planet(
            &planet,
            direction * (ground + 2.0),
            direction * (ground - 2.0),
            0.51001,
        );
        let wanted = landed + direction * 0.1 + DVec3::Z * 1e-5;
        assert_eq!(sweep_planet(&planet, landed, wanted, 0.5), wanted);
        let stopped = sweep_planet(&planet, wanted, direction * (ground - 20.0), 0.5);
        assert!(planet.at(stopped) <= -0.5);
    }
}
