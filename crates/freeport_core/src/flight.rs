//! Continuous flight limits and collision, in a body's local f64 frame.

use crate::field::Density;
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
    use crate::field::{Planet, Sphere};

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
}
