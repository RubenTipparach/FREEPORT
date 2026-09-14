//! Deterministic settlement anchors and hex-aligned street lots.

use glam::DVec3;

/// A procedural location on the planet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Location {
    pub id: u64,
    pub direction: DVec3,
    pub radius_cells: u16,
}

/// Produces stable, evenly distributed candidate locations for a world seed.
pub fn locations(seed: u64, count: usize) -> Vec<Location> {
    let golden = std::f64::consts::PI * (3.0 - 5.0_f64.sqrt());
    (0..count)
        .map(|i| {
            let y = 1.0 - 2.0 * (i as f64 + 0.5) / count.max(1) as f64;
            let radial = (1.0 - y * y).sqrt();
            let phase = unit_hash(seed ^ i as u64) * std::f64::consts::TAU;
            let angle = golden * i as f64 + phase * 0.08;
            Location {
                id: mix(seed ^ i as u64),
                direction: DVec3::new(radial * angle.cos(), y, radial * angle.sin()),
                radius_cells: 8 + (mix(seed.wrapping_add(i as u64)) % 25) as u16,
            }
        })
        .collect()
}

fn unit_hash(value: u64) -> f64 {
    mix(value) as f64 / u64::MAX as f64
}

fn mix(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_locations_are_stable_and_on_unit_sphere() {
        let a = locations(42, 32);
        assert_eq!(a, locations(42, 32));
        assert!(a
            .iter()
            .all(|site| (site.direction.length() - 1.0).abs() < 1.0e-12));
    }
}
