//! Water: the sea's surface, and where there is none.
//!
//! The sea is a level, a radius, and its surface is the sphere at that
//! radius, contoured by the same `dc.rs` as the ground at the same levels,
//! so the two meet at a shore with the same cells. What clips it is the
//! MATERIAL, per triangle: a triangle of the sphere whose middle stands in
//! the ground's rock is BURIED and never drawn, and one over air is
//! SURFACE; the depth test settles the shoreline to the pixel, since the
//! ground is drawn first. That alone would put water in any hole dug below
//! the level, wherever it was dug, so water is FINITE: a cut is DRY unless
//! it touched water when it was made, the surface is buried inside a dry
//! cut, and a cut that touches water and reaches a dry one wets it, and
//! every dry cut that one reaches, so a channel dug from the shore floods
//! the basin at its end the moment it breaks through and never before.

use crate::field::{box_radii, Block, Density};
use glam::DVec3;

/// A triangle of the sea's surface over air: drawn.
pub const SURFACE: u8 = 8;
/// A triangle of the sphere inside the ground, or in a dry cut: not drawn.
pub const BURIED: u8 = 9;

/// The sea: a level, as a radius from the planet's centre.
#[derive(Clone, Copy, Debug)]
pub struct Sea {
    pub radius: f64,
}

/// The sea on a planet: its surface, the ground that buries it, and the
/// cuts it has not reached.
pub struct Water<'a> {
    pub sea: Sea,
    pub ground: &'a dyn Density,
    /// Cuts made in the dry, which water never enters.
    pub dry: Vec<Block>,
}

impl Density for Water<'_> {
    /// The sphere at the level: positive under it.
    fn at(&self, p: DVec3) -> f64 {
        self.sea.radius - p.length()
    }

    /// Where the surface over `p` stands: over air and outside every dry
    /// cut it is the surface, else buried.
    fn material(&self, p: DVec3) -> u8 {
        let Some(dir) = p.try_normalize() else {
            return BURIED;
        };
        let on = dir * self.sea.radius;
        if self.ground.at(on) > 0.0 || self.dry.iter().any(|c| c.at(on) > 0.0) {
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
    /// Whether there is water at `p`: under the level, in the ground's air,
    /// and outside every dry cut.
    pub fn has_water(&self, p: DVec3) -> bool {
        p.length() < self.sea.radius
            && self.ground.at(p) < 0.0
            && !self.dry.iter().any(|c| c.at(p) > 0.0)
    }

    /// Whether a cut, made now into this ground, touches water: any point
    /// of a grid over its box, grown by `reach`, has water. That is what
    /// decides whether the cut is dry.
    pub fn touches(&self, cut: &Block, reach: f64) -> bool {
        let (lo, hi) = cut.bounds();
        let (lo, hi) = (lo - DVec3::splat(reach), hi + DVec3::splat(reach));
        let (near, _) = box_radii(lo, hi);
        if near > self.sea.radius || self.ground.solid(lo, hi) == Some(true) {
            return false;
        }
        let n = 6;
        for k in 0..=n {
            for j in 0..=n {
                for i in 0..=n {
                    let t = DVec3::new(i as f64, j as f64, k as f64) / n as f64;
                    if self.has_water(lo + (hi - lo) * t) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Wet every dry cut a wet cut reaches, and every dry cut those reach:
    /// the dry cuts whose boxes meet `wet`'s, taken out of the dry list.
    /// Returns how many were wetted.
    pub fn flood(&mut self, wet: &Block) -> usize {
        let mut wetted = 0;
        let mut front = vec![wet.bounds()];
        while let Some((lo, hi)) = front.pop() {
            let mut i = 0;
            while i < self.dry.len() {
                let (clo, chi) = self.dry[i].bounds();
                if clo.cmple(hi).all() && chi.cmpge(lo).all() {
                    front.push(self.dry.remove(i).bounds());
                    wetted += 1;
                } else {
                    i += 1;
                }
            }
        }
        wetted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::audit;
    use crate::dc::contour;
    use crate::field::Planet;
    use crate::lattice::{Lattice, Rings};

    fn shore() -> Planet {
        Planet {
            radius: 60.0,
            relief: 8.0,
            lumps: 3.0,
            octaves: 4,
            overhang: 0.0,
            ledge: 1.0,
            seed: 11,
            sites: vec![],
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

    /// A frame on the sphere at a direction: east, north, up.
    fn frame(d: DVec3) -> [DVec3; 3] {
        let east = d.cross(DVec3::Y).normalize_or(DVec3::X);
        let north = d.cross(east);
        [east, north, d]
    }

    #[test]
    fn water_lies_in_the_air_under_the_level_and_nowhere_else() {
        let planet = shore();
        let sea = Sea { radius: 60.0 };
        let (low, high) = low_and_high(&planet, sea.radius, 1.5);
        let water = Water {
            sea,
            ground: &planet,
            dry: vec![],
        };
        let bed = top(&planet, low);
        assert!(water.has_water(low * (bed + 0.5)), "water over the bed");
        assert!(!water.has_water(low * (bed - 0.5)), "rock under it");
        assert!(!water.has_water(low * 61.0), "air over the level");
        assert!(!water.has_water(high * 59.5), "rock in a hill");
        assert_eq!(water.material(low * 59.0), SURFACE);
        assert_eq!(
            water.material(high * 61.0),
            BURIED,
            "the sphere under a hill"
        );
        assert_eq!(water.material(DVec3::ZERO), BURIED);
        assert!(water.at(low * 59.0) > 0.0 && water.at(low * 61.0) < 0.0);
        assert_eq!(
            water.solid(DVec3::splat(61.0), DVec3::splat(62.0)),
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
        let sea = Sea { radius: 60.0 };
        let (low, _) = low_and_high(&planet, sea.radius, 2.0);
        let water = Water {
            sea,
            ground: &planet,
            dry: vec![],
        };
        let lat = Lattice::new(DVec3::splat(-80.0 + 0.125), 0.25);
        let rings = Rings::around(&lat, low * 60.0, 4);
        let mut chunks = Vec::new();
        for id in rings.chunks() {
            let (lo, hi) = id.bounds(&lat, 0);
            // Buried chunks are skipped by the app; here every crossing
            // chunk is kept, so the shell can be audited whole.
            let (near, far) = box_radii(lo, hi);
            if near > sea.radius || far < sea.radius {
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
        let want = 4.0 * std::f64::consts::PI * 3600.0;
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

    #[test]
    fn a_cut_in_the_dry_stays_dry_until_a_wet_one_reaches_it() {
        let planet = shore();
        let sea = Sea { radius: 60.0 };
        let (low, high) = low_and_high(&planet, sea.radius, 2.0);
        // A pit dug on the hill, its floor a metre under the level: dry
        // ground all round it.
        let pit = Block {
            centre: high * (sea.radius + 1.0),
            half: DVec3::new(1.0, 1.0, 2.0),
            axes: frame(high),
        };
        let before = Water {
            sea,
            ground: &planet,
            dry: vec![],
        };
        assert!(
            !before.touches(&pit, 0.5),
            "a pit on a hill touches no water"
        );
        let dug = Cut {
            ground: &planet,
            cuts: vec![pit.clone()],
        };
        let mut water = Water {
            sea,
            ground: &dug,
            dry: vec![pit.clone()],
        };
        let in_pit = pit.centre - high * 1.5;
        assert!(dug.at(in_pit) < 0.0, "the pit is air");
        assert!(!water.has_water(in_pit), "the dry pit holds no water");
        assert_eq!(water.material(in_pit), BURIED, "and draws none");
        // A pool at the shore touches the sea and reaches no dry cut.
        let pool = Block {
            centre: low * (sea.radius - 0.5),
            half: DVec3::new(1.0, 1.0, 1.0),
            axes: frame(low),
        };
        assert!(water.touches(&pool, 0.5));
        assert_eq!(water.flood(&pool), 0);
        // A wet cut that reaches the pit floods it.
        let channel = Block {
            centre: pit.centre,
            half: DVec3::new(1.5, 1.5, 2.5),
            axes: frame(high),
        };
        assert_eq!(water.flood(&channel), 1);
        assert!(water.dry.is_empty());
        assert!(water.has_water(in_pit), "the wetted pit holds water");
        assert_eq!(water.material(in_pit), SURFACE);
    }

    /// The ground with cuts taken out of it, as an editor makes it.
    struct Cut<'a> {
        ground: &'a dyn Density,
        cuts: Vec<Block>,
    }

    impl Density for Cut<'_> {
        fn at(&self, p: DVec3) -> f64 {
            let mut d = self.ground.at(p);
            for c in &self.cuts {
                d = d.min(-c.at(p));
            }
            d
        }
    }
}
