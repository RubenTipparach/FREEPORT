//! How far a place stands from the SEA, which is what sizes a town.
//!
//! The owner's rule is that the big cities are ALONG THE COAST and the
//! interior carries market towns and villages, and the size law used to
//! read that off a town's HEIGHT over the sea on the reasoning that low
//! ground is the coastal fringe. On this body it is not: `town::CUT`
//! caps how deep a site may cut, so the flattest big sites are inland
//! basins, and measured on the atlas not one of 521 settlements had open
//! water within three kilometres and the port's own nearest sea was
//! 15.7 km off. Height is a proxy and this is the thing itself.
//!
//! It is a GRID because a search per candidate is a march per bearing
//! per step and twenty thousand candidates: the whole body's water mask
//! is half a million analytic samples instead, under a second, and the
//! distance to it is one Dijkstra over the grid with the true ground
//! step between two texels at every latitude. A lookup is then an index.

use crate::field::Planet;
use glam::DVec3;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// Texels round the equator, and half that pole to pole. On a thousand
/// kilometre body a texel is six kilometres, which is what "along the
/// coast" is measured to; on a two kilometre test ball it is twelve
/// metres, under a town's own radius, so the law spreads the same way on
/// any body.
const WIDE: usize = 1024;
const HIGH: usize = WIDE / 2;

/// Metres to the nearest water, over the whole body, as an equirect
/// grid: `dir.y` is the latitude, which is what `biome` and `day`
/// already measure it on.
#[derive(Clone, Debug)]
pub struct Shore {
    dist: Vec<f32>,
}

/// A grid cell in the heap: nearer first.
#[derive(PartialEq)]
struct Reach(f32, usize);

impl Eq for Reach {}

impl Ord for Reach {
    fn cmp(&self, other: &Self) -> Ordering {
        other.0.total_cmp(&self.0)
    }
}

impl PartialOrd for Reach {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// The latitude of a row's middle, radians, north positive.
fn latitude(j: usize) -> f64 {
    (0.5 - (j as f64 + 0.5) / HIGH as f64) * std::f64::consts::PI
}

/// The direction at the middle of a texel.
fn direction(i: usize, j: usize) -> DVec3 {
    let lon = ((i as f64 + 0.5) / WIDE as f64 - 0.5) * std::f64::consts::TAU;
    let lat = latitude(j);
    let (s, c) = lat.sin_cos();
    DVec3::new(c * lon.cos(), s, c * lon.sin())
}

impl Shore {
    /// The distance to the sea everywhere on `planet`, whose sea stands
    /// at the radius `sea`. Asked of the BARE body, because a site can
    /// only ever cut and no town makes water.
    pub fn of(planet: &Planet, sea: f64) -> Shore {
        let bare = planet.bare();
        let wet: Vec<bool> = (0..WIDE * HIGH)
            .map(|k| bare.surface(direction(k % WIDE, k / WIDE)).0 + planet.radius < sea)
            .collect();
        Shore {
            dist: distances(&wet, planet.radius),
        }
    }

    /// How far `dir` stands from the nearest water, metres along the
    /// ground; nought in the sea.
    pub fn distance(&self, dir: DVec3) -> f64 {
        let d = dir.normalize_or(DVec3::Y);
        let lon = d.z.atan2(d.x);
        let lat = d.y.clamp(-1.0, 1.0).asin();
        let i = ((lon / std::f64::consts::TAU + 0.5) * WIDE as f64).floor() as isize;
        let j = ((0.5 - lat / std::f64::consts::PI) * HIGH as f64).floor() as isize;
        let i = i.rem_euclid(WIDE as isize) as usize;
        let j = j.clamp(0, HIGH as isize - 1) as usize;
        self.dist[j * WIDE + i] as f64
    }
}

/// Dijkstra out of every wet texel over the eight neighbours, the step
/// between two texels being the ground between them: a row's east step
/// narrows as the cosine of its latitude, so a distance near the poles
/// is measured over the ground and not over the picture.
fn distances(wet: &[bool], radius: f64) -> Vec<f32> {
    let mut dist = vec![f32::INFINITY; wet.len()];
    let mut heap = BinaryHeap::new();
    for (k, &w) in wet.iter().enumerate() {
        if w {
            dist[k] = 0.0;
            heap.push(Reach(0.0, k));
        }
    }
    let north = std::f64::consts::PI * radius / HIGH as f64;
    let east = |j: usize| std::f64::consts::TAU * radius * latitude(j).cos().max(0.0) / WIDE as f64;
    while let Some(Reach(d, k)) = heap.pop() {
        if d > dist[k] {
            continue;
        }
        let (i, j) = (k % WIDE, k / WIDE);
        for dj in -1i64..=1 {
            let jj = j as i64 + dj;
            if jj < 0 || jj >= HIGH as i64 {
                continue;
            }
            let jj = jj as usize;
            // The east step of the row STEPPED INTO, and the two rows'
            // mean where a step is diagonal.
            let e = 0.5 * (east(j) + east(jj));
            for di in -1i64..=1 {
                if di == 0 && dj == 0 {
                    continue;
                }
                let ii = (i as i64 + di).rem_euclid(WIDE as i64) as usize;
                let step = ((di as f64 * e).powi(2) + (dj as f64 * north).powi(2)).sqrt();
                let n = jj * WIDE + ii;
                let via = d + step as f32;
                if via < dist[n] {
                    dist[n] = via;
                    heap.push(Reach(via, n));
                }
            }
        }
    }
    dist
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A ball with a sea on it: the water is at nought distance, the
    /// ground grows away from it, and nothing is further than the body
    /// is round.
    #[test]
    fn the_shore_is_nought_in_the_sea_and_grows_inland() {
        let planet = Planet {
            radius: 1000.0,
            relief: 40.0,
            lumps: 6.0,
            octaves: 6,
            overhang: 1.0,
            ledge: 8.0,
            seed: 7,
            sites: vec![].into(),
        };
        let sea = 996.0;
        let t0 = std::time::Instant::now();
        let shore = Shore::of(&planet, sea);
        println!(
            "the shore grid of a 1 km ball in {:.0} ms",
            t0.elapsed().as_secs_f64() * 1e3
        );
        let golden = std::f64::consts::PI * (3.0 - 5f64.sqrt());
        let (mut wet, mut dry, mut furthest) = (0usize, 0usize, 0.0f64);
        for i in 0..2000 {
            let y = 1.0 - 2.0 * (i as f64 + 0.5) / 2000.0;
            let s = (1.0 - y * y).sqrt();
            let a = golden * i as f64;
            let dir = DVec3::new(s * a.cos(), y, s * a.sin());
            let d = shore.distance(dir);
            assert!(d.is_finite() && d >= 0.0, "{d} at {dir}");
            if planet.surface(dir).0 + planet.radius < sea {
                wet += 1;
                // Its own texel is wet, or the one beside it is: the
                // sample and the texel's middle are up to half a texel
                // apart.
                assert!(d < 12.0, "{d:.1} m from the sea, in the sea");
            } else {
                dry += 1;
                furthest = furthest.max(d);
            }
        }
        println!("{wet} wet and {dry} dry of 2000, the furthest inland {furthest:.0} m");
        assert!(wet > 100 && dry > 100, "a ball with a sea AND land on it");
        assert!(furthest > 20.0, "nothing stands inland at all");
        assert!(
            furthest < std::f64::consts::PI * planet.radius,
            "further from the sea than half way round the body"
        );
    }
}
