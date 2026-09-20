//! Every SITE on a body, and the index that finds the few which matter
//! at a direction without walking the rest.
//!
//! It is out of `field.rs` because that file was over this project's nine
//! hundred lines the day a road was painted on the ground it crosses.

use super::site_band;
use glam::DVec3;

/// Every site on a body, kept so that the few which matter at a
/// direction can be found without walking the rest.
///
/// A body with eight towns on it could be walked; a body with a hundred
/// and sixty could, once `Planet::around` filtered once a CHUNK rather
/// than once a sample. A body whose ROADS are levelled cannot: this one
/// carries 1,084 settlements and 310 roads over 63,840 km, and a
/// corridor cut every few hundred metres is hundreds of thousands of
/// sites. `surface_blend` is asked for every one of a chunk's seven
/// thousand sample points, so walking them all is a hundred million
/// tests for a chunk in the middle of an ocean.
///
/// The index is the simplest one that works on a sphere and keeps this
/// crate's no-`HashMap` rule: the sites SORTED BY LATITUDE, which is
/// `dir.y` because that is what `biome` already measures latitude on,
/// and one number for how far the widest of them reaches off its own.
/// A query is a binary search and a walk of a thin band. It is not a
/// quadtree because it does not need to be: a band of the sphere a few
/// hundred metres deep holds a handful of sites out of any number.
#[derive(Clone, Debug, Default)]
pub struct Sites {
    /// Sorted by `mid`, so a query is a range.
    by_lat: Vec<crate::town::Site>,
    /// How far the widest site reaches off its own middle latitude,
    /// as a share of the y axis (half its arc) and in metres (its outer
    /// band). `window` is what turns the pair into one number.
    reach_y: f64,
    reach_m: f64,
    /// The worst level and the worst DROP along an arc, which is what
    /// the field's slope bound reads. Precomputed, because the bound is
    /// asked once a box and the sites are not walked for it.
    level: f64,
    drop: f64,
    /// The shortest arc any site spans, radians, so the steepest grade
    /// is bounded by `drop / (sweep * radius)`.
    sweep: f64,
}

impl Sites {
    /// The index over a list of sites.
    pub fn new(sites: Vec<crate::town::Site>) -> Sites {
        let mut out = Sites {
            by_lat: sites,
            sweep: f64::INFINITY,
            ..Sites::default()
        };
        for site in &out.by_lat {
            out.reach_y = out.reach_y.max((site.dir.y - site.to.y).abs() * 0.5);
            out.reach_m = out.reach_m.max(site_band(site).1);
            out.level = out.level.max(site.h.abs()).max(site.to_h.abs());
            out.drop = out.drop.max((site.to_h - site.h).abs());
            let sweep = site.dir.angle_between(site.to);
            if sweep > 0.0 {
                out.sweep = out.sweep.min(sweep);
            }
        }
        out.by_lat.sort_by(|a, b| {
            Sites::mid(a)
                .partial_cmp(&Sites::mid(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        out
    }

    /// A site's own middle latitude, which is what it is sorted on.
    fn mid(site: &crate::town::Site) -> f64 {
        (site.dir.y + site.to.y) * 0.5
    }

    pub fn is_empty(&self) -> bool {
        self.by_lat.is_empty()
    }

    pub fn len(&self) -> usize {
        self.by_lat.len()
    }

    /// Every site, in no useful order.
    pub fn iter(&self) -> std::slice::Iter<'_, crate::town::Site> {
        self.by_lat.iter()
    }

    /// No sites at all.
    pub fn clear(&mut self) {
        *self = Sites::default();
    }

    /// One more site. It re-sorts, so it is for building a body and not
    /// for a loop.
    pub fn push(&mut self, site: crate::town::Site) {
        let mut all = std::mem::take(&mut self.by_lat);
        all.push(site);
        *self = Sites::new(all);
    }

    /// How far off a query's own latitude a site can still reach, on a
    /// body of this radius.
    pub fn window(&self, radius: f64) -> f64 {
        self.reach_y
            + if radius > 0.0 {
                self.reach_m / radius
            } else {
                0.0
            }
    }

    /// The highest level any site holds, metres, and the steepest its
    /// own level ramps ALONG it. A town's ramp is nought; a road's is
    /// the grade it was routed at, and the field's slope bound has to
    /// carry it or a chunk on a hill road is ruled empty and left as a
    /// hole.
    pub fn level(&self) -> f64 {
        self.level
    }

    pub fn grade(&self, radius: f64) -> f64 {
        if self.sweep.is_finite() && self.sweep > 0.0 && radius > 0.0 {
            self.drop / (self.sweep * radius)
        } else {
            0.0
        }
    }

    /// The sites whose own latitude band is within `window` of a
    /// direction's: a binary search and a walk. Everything else on the
    /// body is skipped without being looked at.
    pub fn near(&self, dir: DVec3, window: f64) -> impl Iterator<Item = &crate::town::Site> {
        let (lo, hi) = (dir.y - window, dir.y + window);
        let start = self.by_lat.partition_point(|s| Sites::mid(s) < lo);
        self.by_lat[start..]
            .iter()
            .take_while(move |s| Sites::mid(s) <= hi)
    }
}

impl<'a> IntoIterator for &'a Sites {
    type Item = &'a crate::town::Site;
    type IntoIter = std::slice::Iter<'a, crate::town::Site>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl std::ops::Index<usize> for Sites {
    type Output = crate::town::Site;
    /// One site, in the index's own order, which is by latitude and not
    /// the order they were handed in. It is here for the tests that hold
    /// a body with exactly one site on it.
    fn index(&self, k: usize) -> &crate::town::Site {
        &self.by_lat[k]
    }
}

impl FromIterator<crate::town::Site> for Sites {
    fn from_iter<T: IntoIterator<Item = crate::town::Site>>(sites: T) -> Sites {
        Sites::new(sites.into_iter().collect())
    }
}

impl From<Vec<crate::town::Site>> for Sites {
    fn from(sites: Vec<crate::town::Site>) -> Sites {
        Sites::new(sites)
    }
}
