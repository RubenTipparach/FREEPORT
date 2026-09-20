//! Water: the sea's surface, and where there is none.
//!
//! The sea is a level, a radius, and its surface is the sphere at that
//! radius, contoured by the same `dc.rs` as the ground at the same levels,
//! so the two meet at a shore with the same cells. What clips it is the
//! MATERIAL, per triangle: a triangle of the sphere whose middle stands in
//! the ground's rock is BURIED and never drawn, and one over air is
//! SURFACE; the depth test settles the shoreline to the pixel, since the
//! ground is drawn first.
//!
//! It is a SURFACE and not a volume, which is what the owner asked for
//! now: a hole dug below the level away from the sea would hold water
//! here, and there is nothing that digs one. Voxel water, where a cut is
//! dry unless it touched the sea when it was made and a channel from the
//! shore floods the basin at its end, is what a world with digging in it
//! needs, and it comes back with the thing that digs.

use crate::field::{box_radii, Density};
use glam::DVec3;

/// A triangle of the sea's surface over air: drawn.
pub const SURFACE: u8 = 8;
/// A triangle of the sphere inside the ground: not drawn.
pub const BURIED: u8 = 9;

/// The sea: a level, as a radius from the planet's centre.
#[derive(Clone, Copy, Debug)]
pub struct Sea {
    pub radius: f64,
}

/// The sea on a planet: a level, and the ground that buries its surface.
///
/// Water is a SURFACE here and not a volume, which is the owner's ask for
/// now: the sheet is the sphere at the level, contoured by the same
/// mesher in the same chunks as the ground, and what clips it is the
/// MATERIAL per triangle. Voxel water, where a hole dug away from the sea
/// is dry and one dug from the shore floods, is what a cut in the ground
/// needs and there is nothing that cuts the ground yet: it comes back
/// with the thing that digs.
pub struct Water<'a> {
    pub sea: Sea,
    pub ground: &'a dyn Density,
}

impl Density for Water<'_> {
    /// The sphere at the level: positive under it.
    fn at(&self, p: DVec3) -> f64 {
        self.sea.radius - p.length()
    }

    /// Where the surface over `p` stands: over air it is the surface, and
    /// inside the ground's rock it is buried and never drawn. The depth
    /// test settles the shoreline to the pixel, since the ground is drawn
    /// first.
    fn material(&self, p: DVec3, _reach: f64) -> u8 {
        let Some(dir) = p.try_normalize() else {
            return BURIED;
        };
        if self.ground.at(dir * self.sea.radius) > 0.0 {
            BURIED
        } else {
            SURFACE
        }
    }

    /// No surface in a box the level does not cross, and none worth
    /// drawing in one the ground fills.
    fn solid(&self, lo: DVec3, hi: DVec3) -> Option<bool> {
        let (near, far) = box_radii(lo, hi);
        if near > self.sea.radius {
            return Some(false);
        }
        if far < self.sea.radius || self.ground.solid(lo, hi) == Some(true) {
            return Some(true);
        }
        None
    }

    fn slope(&self) -> f64 {
        1.0
    }
}

impl Water<'_> {
    /// Whether there is water at `p`: under the level and in the ground's
    /// own air, which is what holds a walker at wading depth.
    pub fn has_water(&self, p: DVec3) -> bool {
        p.length() < self.sea.radius && self.ground.at(p) < 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::audit;
    use crate::dc::contour;
    use crate::field::Planet;
    use crate::lattice::{Lattice, Rings};

    /// A ball with a real shore on it: deep water on one side and land
    /// well over the sea on the other, which is what the sheet has to be
    /// judged against. Its lumps came down from three when the relief
    /// became a few composed terms: at three, a sixty metre ball asks for
    /// ground steeper than `biome::MAX_SLOPE` allows and every term is
    /// scaled back to fit, which left the whole planet inside a metre and
    /// a half of the sea and no deep water to contour at all.
    fn shore() -> Planet {
        Planet {
            radius: 60.0,
            relief: 20.0,
            lumps: 1.0,
            octaves: 4,
            overhang: 0.0,
            ledge: 1.0,
            seed: 11,
            sites: vec![].into(),
        }
    }

    /// The radius of the ground along a direction.
    fn top(planet: &Planet, dir: DVec3) -> f64 {
        let (mut lo, mut hi) = (50.0, 70.0);
        for _ in 0..40 {
            let mid = 0.5 * (lo + hi);
            if planet.at(dir * mid) > 0.0 {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        0.5 * (lo + hi)
    }

    /// Where to put this ball's sea so there is as much of it as there is
    /// land: the MEDIAN of the ground, measured rather than assumed.
    ///
    /// It used to be the mean radius, which was right while the relief was
    /// one fractal centred on nought. It is not now: a mountain belt only
    /// ever ADDS, so the composed relief's median sits off the mean, and a
    /// sea at the mean radius came out as a planet that was nearly all
    /// land on one set of numbers and nearly all ocean on the next. A
    /// fixture that measures cannot go stale the next time the ground
    /// changes.
    fn sea_of(planet: &Planet) -> Sea {
        let mut h: Vec<f64> = (0..2000)
            .map(|i| {
                let t = i as f64 * 0.618;
                let d = DVec3::new(t.sin(), (t * 0.37).cos(), (t * 1.3).sin()).normalize();
                planet.surface(d).0
            })
            .collect();
        h.sort_by(f64::total_cmp);
        Sea {
            radius: planet.radius + h[h.len() / 2],
        }
    }

    /// A direction whose ground is at least `depth` under the sea, and one
    /// whose ground is at least that over it.
    fn low_and_high(planet: &Planet, sea: f64, depth: f64) -> (DVec3, DVec3) {
        let (mut low, mut high) = (None, None);
        for i in 0..2000 {
            let t = i as f64 * 0.618;
            let d = DVec3::new(t.sin(), (t * 0.37).cos(), (t * 1.3).sin()).normalize();
            let h = top(planet, d) - sea;
            if h < -depth && low.is_none() {
                low = Some(d);
            }
            if h > depth && high.is_none() {
                high = Some(d);
            }
        }
        (
            low.expect("some ground under the sea"),
            high.expect("some ground over it"),
        )
    }

    #[test]
    fn water_lies_in_the_air_under_the_level_and_nowhere_else() {
        let planet = shore();
        let sea = sea_of(&planet);
        let (low, high) = low_and_high(&planet, sea.radius, 1.5);
        let water = Water {
            sea,
            ground: &planet,
        };
        let bed = top(&planet, low);
        assert!(water.has_water(low * (bed + 0.5)), "water over the bed");
        assert!(!water.has_water(low * (bed - 0.5)), "rock under it");
        assert!(!water.has_water(low * 61.0), "air over the level");
        assert!(!water.has_water(high * 59.5), "rock in a hill");
        assert_eq!(water.material(low * 59.0, 0.0), SURFACE);
        assert_eq!(
            water.material(high * 61.0, 0.0),
            BURIED,
            "the sphere under a hill"
        );
        assert_eq!(water.material(DVec3::ZERO, 0.0), BURIED);
        let r = sea.radius;
        assert!(water.at(low * (r - 1.0)) > 0.0 && water.at(low * (r + 1.0)) < 0.0);
        assert_eq!(
            water.solid(DVec3::splat(r + 1.0), DVec3::splat(r + 2.0)),
            Some(false)
        );
        assert_eq!(
            water.solid(DVec3::splat(-1.0), DVec3::splat(1.0)),
            Some(true)
        );
        assert_eq!(water.slope(), 1.0);
    }

    #[test]
    fn the_sea_contours_to_a_closed_shell_and_only_the_surface_over_air_is_drawn() {
        let planet = shore();
        let sea = sea_of(&planet);
        let (low, _) = low_and_high(&planet, sea.radius, 2.0);
        let sea_radius = sea.radius;
        let water = Water {
            sea,
            ground: &planet,
        };
        let lat = Lattice::new(DVec3::splat(-80.0 + 0.125), 0.25);
        let rings = Rings::around(&lat, low * sea_radius, 4);
        let mut chunks = Vec::new();
        for id in rings.chunks() {
            let (lo, hi) = id.bounds(&lat, 0);
            // Buried chunks are skipped by the app; here every crossing
            // chunk is kept, so the shell can be audited whole.
            let (near, far) = box_radii(lo, hi);
            if near > sea_radius || far < sea_radius {
                continue;
            }
            let m = contour(&water, &lat, id, &rings);
            if m.triangles() > 0 {
                chunks.push((id.corner(&lat), m));
            }
        }
        let a = audit(&water, &chunks);
        assert_eq!(a.open, 0, "{a:?}");
        assert_eq!(a.non_manifold, 0, "{a:?}");
        assert_eq!(a.facing_in, 0, "{a:?}");
        let want = 4.0 * std::f64::consts::PI * sea_radius * sea_radius;
        assert!((a.area - want).abs() / want < 0.03, "area {}", a.area);
        let (mut surface, mut buried, mut worst) = (0, 0, 0.0f64);
        for (corner, m) in &chunks {
            for (t, mat) in m.indices.chunks(3).zip(&m.materials) {
                let mid = t
                    .iter()
                    .map(|&i| DVec3::from(m.positions[i as usize].map(f64::from)))
                    .sum::<DVec3>()
                    / 3.0
                    + *corner;
                worst = worst.max((mid.length() - sea.radius).abs());
                let on = mid.normalize() * sea.radius;
                match *mat {
                    SURFACE => {
                        surface += 1;
                        assert!(planet.at(on) <= 0.0, "a drawn triangle in rock at {on}");
                    }
                    _ => {
                        buried += 1;
                        assert!(planet.at(on) >= 0.0, "a buried triangle over air at {on}");
                    }
                }
            }
        }
        assert!(
            surface > 1000 && buried > 1000,
            "{surface} surface, {buried} buried"
        );
        assert!(worst < 0.3, "a triangle {worst} m off the level");
    }
}
