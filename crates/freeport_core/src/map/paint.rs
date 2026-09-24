//! The map's PAINT: the heights under every pixel, the ground coloured
//! from them, and the strokes, fills and discs the roads and the towns
//! are laid over it with, in the approved page's own palette.

use super::{Scene, View};
use crate::town::Tier;
use glam::{DVec2, DVec3};

/// A picture, `width` by `height`, eight bit sRGB with alpha, rows from
/// the top.
#[derive(Clone, Debug)]
pub struct Picture {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

/// A colour, eight bit sRGB.
pub type Colour = [u8; 3];

/// The page's own colours: a road `#8a8676`, the sea `#1b3550` at its
/// middle depth, the towns `#c9c4b0`, `#a9a593` and `#8a8676` by size.
pub const ROAD: Colour = [138, 134, 118];
pub const STREET: Colour = [74, 72, 65];
pub const SQUARE: Colour = [107, 104, 88];
const SHALLOW: Colour = [39, 80, 122];
const SEA: Colour = [27, 53, 80];
const DEEP: Colour = [16, 35, 58];
const COAST: Colour = [74, 122, 160];

/// How deep the sea is at its darkest, metres: past a shelf's own depth
/// the colour says only that it is open water.
const DEEP_AT: f64 = 800.0;
/// The land's colour by height over the sea, metres: a relief map's own
/// order, lowland green through olive and tan to grey rock and snow, and
/// kept DARK because the page is: the lowest land is the brightest green
/// in it, so a valley floor at the sea's own level reads as ground and
/// never as a hole in the picture.
const TINT: [(f64, Colour); 7] = [
    (0.0, [52, 72, 46]),
    (150.0, [56, 74, 46]),
    (500.0, [70, 78, 50]),
    (1_000.0, [86, 82, 58]),
    (1_800.0, [96, 86, 68]),
    (3_000.0, [112, 106, 100]),
    (4_500.0, [172, 174, 180]),
];
/// How much a slope is steepened before it is shaded: relief seen from
/// straight over it is flatter than it looks from the ground, and a map
/// that did not exaggerate it would show a mountain range as a smudge.
const STEEPEN: f64 = 4.0;
/// The contour intervals a map picks from, metres, and how far apart two
/// lines of the one it picks may come on level ground, pixels a metre
/// of scale: a line every `CONTOUR_EVERY` pixels of scale, so a region
/// is drawn at a hundred metres and a town at five.
const CONTOURS: [f64; 7] = [5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0];
const CONTOUR_EVERY: f64 = 2.0;
/// Every fifth line is an INDEX contour, drawn darker, which is how a
/// topographic sheet lets an eye count height without a label on every
/// line.
const INDEX: i64 = 5;
/// How much a contour and an index contour darken the ground under them.
const CONTOUR_SHADE: f64 = 0.84;
const INDEX_SHADE: f64 = 0.70;

/// A town's buildings, by the town's own tier.
pub fn building(tier: Tier) -> Colour {
    match tier {
        Tier::City => [201, 196, 176],
        Tier::Town => [169, 165, 147],
        Tier::Village => [150, 146, 130],
    }
}

/// How big a town is drawn when its plan is smaller than this, pixels of
/// radius: the page's own marks, a city seven, a town five and a village
/// three, so a place a map is zoomed out past is still a place on it.
pub fn mark(tier: Tier) -> f64 {
    match tier {
        Tier::City => 7.0,
        Tier::Town => 5.0,
        Tier::Village => 3.0,
    }
}

/// The contour interval a map at a scale draws, metres: the finest of
/// `CONTOURS` at least `CONTOUR_EVERY` pixels of scale.
pub fn contour(scale: f64) -> f64 {
    CONTOURS
        .iter()
        .copied()
        .find(|c| *c >= scale * CONTOUR_EVERY)
        .unwrap_or(CONTOURS[CONTOURS.len() - 1])
}

/// A place in the picture, pixels from the middle with north up, as a
/// point on the raster: columns from the left, rows from the top, and a
/// pixel's middle at a half.
pub fn at(px: DVec2, width: usize, height: usize) -> DVec2 {
    DVec2::new(width as f64 * 0.5 + px.x, height as f64 * 0.5 - px.y)
}

/// The ground's height over the SEA under every pixel's middle, metres,
/// rows from the top. It is what the whole picture costs, one sample of
/// the planet a pixel, so the rows are shared over `threads`.
pub fn heights(
    scene: &Scene,
    view: &View,
    width: usize,
    height: usize,
    threads: usize,
) -> Vec<f64> {
    let mut out = vec![0.0; width * height];
    if width == 0 || height == 0 {
        return out;
    }
    let rows = height.div_ceil(threads.max(1));
    std::thread::scope(|s| {
        for (k, chunk) in out.chunks_mut(rows * width).enumerate() {
            s.spawn(move || {
                for (i, h) in chunk.iter_mut().enumerate() {
                    let (col, row) = (i % width, k * rows + i / width);
                    let px = DVec2::new(
                        col as f64 + 0.5 - width as f64 * 0.5,
                        height as f64 * 0.5 - (row as f64 + 0.5),
                    );
                    let dir = view.to_dir(px);
                    *h = scene.planet.surface(dir).0 + scene.planet.radius - scene.sea;
                }
            });
        }
    });
    out
}

/// The ground coloured: the sea by its depth with its coast drawn, and
/// the land tinted by its height, shaded by its slope under a light from
/// the north west, which is the way a relief map has always been lit,
/// and CONTOURED at the scale's own interval.
pub fn ground(heights: &[f64], width: usize, height: usize, scale: f64) -> Picture {
    let mut rgba = vec![0; width * height * 4];
    let h = |x: usize, y: usize| heights[y * width + x];
    let light = DVec3::new(-1.0, 1.0, 1.0).normalize();
    let every = contour(scale);
    let band = |v: f64| (v / every).floor() as i64;
    for y in 0..height {
        for x in 0..width {
            let here = h(x, y);
            let (l, r) = (h(x.saturating_sub(1), y), h((x + 1).min(width - 1), y));
            let (u, d) = (h(x, y.saturating_sub(1)), h(x, (y + 1).min(height - 1)));
            let colour = if here < 0.0 {
                if l >= 0.0 || r >= 0.0 || u >= 0.0 || d >= 0.0 {
                    COAST
                } else {
                    water(-here)
                }
            } else {
                let dx = (r.max(0.0) - l.max(0.0)) / (2.0 * scale) * STEEPEN;
                let dy = (u.max(0.0) - d.max(0.0)) / (2.0 * scale) * STEEPEN;
                let n = DVec3::new(-dx, -dy, 1.0).normalize();
                let shade = 0.45 + 0.95 * n.dot(light).max(0.0);
                // A line where the band changes toward a neighbour, on
                // the HIGHER side only, so it is one pixel wide and never
                // two.
                let line = [l, r, u, d]
                    .iter()
                    .filter(|v| **v >= 0.0 && band(**v) < band(here))
                    .map(|_| band(here))
                    .next();
                let lined = match line {
                    Some(b) if b % INDEX == 0 => INDEX_SHADE,
                    Some(_) => CONTOUR_SHADE,
                    None => 1.0,
                };
                scaled(tint(here), shade * lined)
            };
            let at = (y * width + x) * 4;
            rgba[at..at + 3].copy_from_slice(&colour);
            rgba[at + 3] = 255;
        }
    }
    Picture {
        width,
        height,
        rgba,
    }
}

/// The sea's colour at a depth: shallow water over a shelf is lighter
/// and open water the page's own blue, darkening past it.
fn water(depth: f64) -> Colour {
    let t = (depth / DEEP_AT).clamp(0.0, 1.0);
    if t < 0.5 {
        mix(SHALLOW, SEA, t * 2.0)
    } else {
        mix(SEA, DEEP, t * 2.0 - 1.0)
    }
}

/// The land's tint at a height over the sea.
fn tint(h: f64) -> Colour {
    let k = TINT
        .iter()
        .position(|(at, _)| *at > h)
        .unwrap_or(TINT.len());
    if k == 0 {
        return TINT[0].1;
    }
    if k == TINT.len() {
        return TINT[k - 1].1;
    }
    let (a, b) = (TINT[k - 1], TINT[k]);
    mix(a.1, b.1, (h - a.0) / (b.0 - a.0))
}

fn mix(a: Colour, b: Colour, t: f64) -> Colour {
    let t = t.clamp(0.0, 1.0);
    let c = |i: usize| (a[i] as f64 + (b[i] as f64 - a[i] as f64) * t).round() as u8;
    [c(0), c(1), c(2)]
}

fn scaled(c: Colour, k: f64) -> Colour {
    let s = |v: u8| (v as f64 * k).round().clamp(0.0, 255.0) as u8;
    [s(c[0]), s(c[1]), s(c[2])]
}

impl Picture {
    /// The colour at a pixel, for a test to read.
    pub fn pixel(&self, x: usize, y: usize) -> [u8; 4] {
        let at = (y * self.width + x) * 4;
        [
            self.rgba[at],
            self.rgba[at + 1],
            self.rgba[at + 2],
            self.rgba[at + 3],
        ]
    }

    /// Lay a colour over a pixel by a share of it.
    pub(super) fn blend(&mut self, x: i64, y: i64, colour: Colour, alpha: f64) {
        if x < 0 || y < 0 || x as usize >= self.width || y as usize >= self.height || alpha <= 0.0 {
            return;
        }
        let at = (y as usize * self.width + x as usize) * 4;
        let a = alpha.min(1.0);
        for (out, c) in self.rgba[at..at + 3].iter_mut().zip(colour) {
            let was = *out as f64;
            *out = (was + (c as f64 - was) * a).round() as u8;
        }
    }

    /// The pixels a box covers, clipped to the picture.
    fn span(&self, lo: DVec2, hi: DVec2) -> Option<(i64, i64, i64, i64)> {
        let x0 = lo.x.floor().max(0.0) as i64;
        let y0 = lo.y.floor().max(0.0) as i64;
        let x1 = (hi.x.ceil() as i64).min(self.width as i64 - 1);
        let y1 = (hi.y.ceil() as i64).min(self.height as i64 - 1);
        (x0 <= x1 && y0 <= y1 && hi.x >= 0.0 && hi.y >= 0.0).then_some((x0, y0, x1, y1))
    }

    /// A line from `a` to `b`, `width` pixels across, its edge smoothed
    /// over a pixel.
    pub fn stroke(&mut self, a: DVec2, b: DVec2, width: f64, colour: Colour) {
        let half = width * 0.5;
        let pad = DVec2::splat(half + 1.0);
        let Some((x0, y0, x1, y1)) = self.span(a.min(b) - pad, a.max(b) + pad) else {
            return;
        };
        let ab = b - a;
        let long = ab.length_squared().max(1e-12);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let p = DVec2::new(x as f64 + 0.5, y as f64 + 0.5);
                let t = ((p - a).dot(ab) / long).clamp(0.0, 1.0);
                let d = (p - (a + ab * t)).length();
                self.blend(x, y, colour, (half + 0.5 - d).clamp(0.0, 1.0));
            }
        }
    }

    /// A convex quadrilateral filled, four samples a pixel; one under two
    /// pixels across is laid on the pixel it stands in by its own area,
    /// so a town of buildings smaller than a pixel still reads as a town.
    pub fn fill(&mut self, q: [DVec2; 4], colour: Colour) {
        let lo = q[0].min(q[1]).min(q[2]).min(q[3]);
        let hi = q[0].max(q[1]).max(q[2]).max(q[3]);
        if (hi - lo).max_element() < 2.0 {
            let area = 0.5
                * (0..4)
                    .map(|i| q[i].perp_dot(q[(i + 1) % 4]))
                    .sum::<f64>()
                    .abs();
            let mid = (q[0] + q[1] + q[2] + q[3]) * 0.25;
            self.blend(mid.x.floor() as i64, mid.y.floor() as i64, colour, area);
            return;
        }
        let Some((x0, y0, x1, y1)) = self.span(lo, hi) else {
            return;
        };
        let inside = |p: DVec2| {
            let s: Vec<f64> = (0..4)
                .map(|i| (q[(i + 1) % 4] - q[i]).perp_dot(p - q[i]))
                .collect();
            s.iter().all(|v| *v >= 0.0) || s.iter().all(|v| *v <= 0.0)
        };
        for y in y0..=y1 {
            for x in x0..=x1 {
                let hits = [0.25, 0.75]
                    .iter()
                    .flat_map(|dy| [0.25, 0.75].map(|dx| DVec2::new(x as f64 + dx, y as f64 + dy)))
                    .filter(|p| inside(*p))
                    .count();
                self.blend(x, y, colour, hits as f64 / 4.0);
            }
        }
    }

    /// A disc of `r` pixels, its edge smoothed over a pixel.
    pub fn disc(&mut self, c: DVec2, r: f64, colour: Colour) {
        let pad = DVec2::splat(r + 1.0);
        let Some((x0, y0, x1, y1)) = self.span(c - pad, c + pad) else {
            return;
        };
        for y in y0..=y1 {
            for x in x0..=x1 {
                let d = (DVec2::new(x as f64 + 0.5, y as f64 + 0.5) - c).length();
                self.blend(x, y, colour, (r + 0.5 - d).clamp(0.0, 1.0));
            }
        }
    }
}
