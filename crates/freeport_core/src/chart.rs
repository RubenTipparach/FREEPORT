//! The whole planet as two pictures: what it looks like from orbit.
//!
//! Past the streamed chunks a body is one sphere, and what that sphere is
//! coloured by decides whether a planet reads as a world or as a ball. The
//! first cut coloured it by VERTEX, one colour a vertex off the icosphere,
//! which is a colour every thirty kilometres: continents came out as soft
//! blobs with no coast anybody could point at, and from thirty kilometres
//! up the picture was a green smear on a blue ball.
//!
//! This is tenebris's answer ported (`build_distant_textures_for_body` in
//! its renderer, `distant.glsl` in its shaders): an EQUIRECTANGULAR pair
//! baked from the same field and the same biome rules the chunks use, so
//! the distant body is a zoomed out preview of the real planet rather than
//! an average of one. The albedo carries the biome's colour with the water
//! blended in and a height shade over it, and its alpha is the water mask
//! the shader gates a sun glint on; the normal carries the slope of the
//! altitude, so a range casts its own shading at a size no mesh on that
//! sphere could hold.
//!
//! It is in the CORE because it is the field's own answer to what is at a
//! place: a chart that disagreed with the ground would be a coast in the
//! wrong place the moment you flew down to it.

use crate::biome::{Climate, Kind, BEACH_TO};
use crate::field::Planet;
use crate::road::Road;
use glam::DVec3;

/// A baked chart of a body: two equirectangular RGBA8 images.
pub struct Chart {
    pub width: usize,
    pub height: usize,
    /// rgb the surface's colour, a the water mask.
    pub albedo: Vec<u8>,
    /// rg the slope of the DRAWN surface in east and north, b flat, a
    /// unused. East is where the chart's u grows and north where its v
    /// shrinks, which is what `distant.wgsl` bends its normal along.
    pub normal: Vec<u8>,
}

/// Which of a chart's own slopes reads as a full unit, as a share of the
/// steepest texel on it. MEASURED per body rather than set: tenebris's
/// `DISTANT_NORMAL_SLOPE_SCALE` is five metres across a texel because its
/// planets are three hundred metres across and its texels are metres, and
/// any fixed number carried here is wrong on the next body. What a texel
/// can see depends on the body's relief AND on how wide the texel is, and
/// a chart normalised to a guess came out flat: on a forty kilometre test
/// planet a hundredth of the relief across a kilometre of texel encoded
/// as 7 of a possible 127, so the whole world shaded as a smooth ball.
///
/// Normalising to the body's own 99th percentile puts the range where the
/// ground actually is, and the shader's own strength is what tunes how
/// bumpy that reads.
const SLOPE_SPAN: f64 = 0.99;

/// How much darker the lowest ground is than the highest, and how much
/// brighter. A shade over the biome's own colour is what makes a range
/// read as a range rather than as a grey patch.
const SHADE_LOW: f64 = 0.72;
const SHADE_HIGH: f64 = 1.18;

/// How deep water has to be to be its full colour, metres. Shallow water
/// over a beach keeps some of the sand under it, which is the one thing
/// that makes a coastline read at this size.
const FULL_DEPTH: f64 = 220.0;

/// The colour of deep water, linear rgb, which is what the shallows blend
/// toward. It is `Kind::Ocean`'s, read off the one table.
fn deep() -> [f64; 3] {
    let c = Kind::Ocean.colour();
    [c[0] as f64, c[1] as f64, c[2] as f64]
}

/// The direction a pixel of an equirectangular image looks along.
///
/// U sweeps longitude through `atan2(z, x)` and V sweeps latitude through
/// `asin(y)`. **The shader's `dir_to_uv` is the inverse of this and the
/// two move together**, which is tenebris's own warning written on its
/// own copy: change one and the planet's coasts slide round it.
pub fn pixel_dir(x: usize, y: usize, w: usize, h: usize) -> DVec3 {
    let u = (x as f64 + 0.5) / w as f64;
    let v = (y as f64 + 0.5) / h as f64;
    let lon = (u - 0.5) * std::f64::consts::TAU;
    let lat = (0.5 - v) * std::f64::consts::PI;
    let cl = lat.cos();
    DVec3::new(cl * lon.cos(), lat.sin(), cl * lon.sin())
}

/// What is at a direction on a body: how far its ground stands over the
/// sea, what is growing there, and how much water is over it.
pub struct Spot {
    pub over_sea: f64,
    pub kind: Kind,
    pub water: f64,
}

/// Everything a chart pixel and a distant vertex both need, off the same
/// field the chunks are contoured from.
pub fn spot_at(planet: &Planet, sea: f64, dir: DVec3, slope: f64) -> Spot {
    let over_sea = planet.radius + planet.surface(dir).0 - sea;
    spot_of(planet, over_sea, dir, slope)
}

/// The same answer as `spot_at` from an altitude already in hand.
///
/// A bake samples the field ONCE a texel and reads that one number for
/// the colour and for the two central differences the slope comes off.
/// `spot_at` asking `surface` again here was half the cost of the whole
/// bake spent on an answer already held, and the loop's own comment
/// claimed it was not happening.
pub fn spot_of(planet: &Planet, over_sea: f64, dir: DVec3, slope: f64) -> Spot {
    let climate = planet.shape().climate(dir, over_sea);
    // A town levels its own ground, and that is what makes it a place
    // rather than a patch of colour: where the site is levelling right
    // across, what is there is a CITY.
    let kind = if town_weight(planet, dir) > 0.5 {
        Kind::City
    } else {
        climate.kind(over_sea, slope)
    };
    let water = if over_sea < 0.0 {
        (-over_sea / FULL_DEPTH).min(1.0)
    } else {
        0.0
    };
    Spot {
        over_sea,
        kind,
        water,
    }
}

/// How much of a town's levelling is at a direction, nought to one. It is
/// the planet's OWN site rule read back, so a city is drawn exactly where
/// the ground was flattened for it.
pub fn town_weight(planet: &Planet, dir: DVec3) -> f64 {
    let (_, keep) = planet.surface_blend(dir);
    1.0 - keep
}

/// The colour a spot reads as from orbit, linear rgb, before lighting.
pub fn spot_colour(spot: &Spot, relief: f64) -> [f64; 3] {
    let base = spot.kind.colour();
    let mut c = [base[0] as f64, base[1] as f64, base[2] as f64];
    if spot.water > 0.0 {
        let d = deep();
        for k in 0..3 {
            c[k] += (d[k] - c[k]) * spot.water;
        }
    }
    // A height shade, so a range stands out of the plain it rises from.
    let t = (spot.over_sea / relief.max(1.0)).clamp(-1.0, 1.0) * 0.5 + 0.5;
    let shade = SHADE_LOW + (SHADE_HIGH - SHADE_LOW) * t;
    for v in &mut c {
        *v = (*v * shade).clamp(0.0, 1.0);
    }
    c
}

impl Chart {
    /// Bake a body's two charts. `w` by `h` texels, the height of the
    /// image half its width, which is what an equirectangular projection
    /// of a sphere is.
    ///
    /// The altitude is sampled ONCE per texel into a buffer and read three
    /// times: for the colour, and for the two central differences the
    /// slope comes off. Sampling it again per difference would treble the
    /// cost of the whole bake for an answer already in hand.
    pub fn bake(planet: &Planet, sea: f64, w: usize, h: usize, roads: &[Road]) -> Chart {
        // The body WITHOUT its towns, and then the towns stamped on.
        //
        // A town is eighty metres across and a texel on a thousand
        // kilometre planet is six thousand, so asking `surface` about a
        // texel's own middle finds a site one time in five thousand: the
        // cities would be invisible on the chart, and the sites would
        // still cost every one of half a million texels a walk over every
        // town on the planet (84 million tests, which took the bake from
        // 954 ms to 1,617). Baked bare and stamped after, the cities are
        // ON it and the bake is back to what it was.
        let bare = Planet {
            sites: Vec::new(),
            ..planet.clone()
        };
        let mut chart = Chart::bake_bare(&bare, sea, w, h);
        // The roads FIRST and the cities over them. Every road ends at a
        // town's own centre, so painted the other way round each road
        // erased the city it serves: 37 city texels survived of 160, and
        // the 123 missing were exactly the towns a road reaches.
        chart.lay_roads(planet, sea, roads);
        chart.stamp(planet, sea);
        chart
    }

    /// Every road on the body, drawn along its own line.
    ///
    /// Stepped at HALF a texel so a line cannot skip one: a road's
    /// waypoints are ten kilometres apart and a texel is six, so drawing
    /// only the points would leave a dotted network with gaps wider than
    /// the marks.
    ///
    /// A road is drawn one texel wide, which on this body is six
    /// kilometres of road, and that is the same honest lie a city one
    /// texel across is: what the chart is for is saying THAT there is a
    /// road and where it runs, and a mark under a texel wide would say
    /// neither. The ground's own roads are the real width.
    pub fn lay_roads(&mut self, planet: &Planet, sea: f64, roads: &[Road]) {
        let relief = planet.shape().relief;
        let texel = std::f64::consts::TAU / self.width as f64;
        for road in roads {
            for pair in road.line.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                let span = (a.0 - b.0).length();
                let steps = ((span / (texel * 0.5)).ceil() as usize).max(1);
                for k in 0..=steps {
                    let t = k as f64 / steps as f64;
                    let dir = (a.0 + (b.0 - a.0) * t).normalize_or(DVec3::Y);
                    let h = a.1 + (b.1 - a.1) * t;
                    let spot = Spot {
                        over_sea: planet.radius + h - sea,
                        kind: Kind::Road,
                        water: 0.0,
                    };
                    self.paint(dir, &spot, relief);
                }
            }
        }
    }

    /// One texel painted with what is at a direction, opaque and dry.
    fn paint(&mut self, dir: DVec3, spot: &Spot, relief: f64) {
        let Some(i) = self.texel(dir) else {
            return;
        };
        let c = spot_colour(spot, relief);
        for (k, v) in c.iter().enumerate() {
            self.albedo[i + k] = to_srgb(*v);
        }
        self.albedo[i + 3] = 0;
    }

    /// The byte a direction's texel starts at.
    fn texel(&self, dir: DVec3) -> Option<usize> {
        let d = dir.normalize_or(DVec3::Y);
        let u = 0.5 + d.z.atan2(d.x) / std::f64::consts::TAU;
        let v = 0.5 - d.y.clamp(-1.0, 1.0).asin() / std::f64::consts::PI;
        let x = ((u * self.width as f64) as usize).min(self.width - 1);
        let y = ((v * self.height as f64) as usize).min(self.height - 1);
        Some((y * self.width + x) * 4)
    }

    /// Every town on the body, painted at the texel it stands in. A city
    /// is a whole texel here where it is a tenth of a millimetre of one
    /// on the ground, which is what makes it a place a player can SEE
    /// from orbit and steer at rather than a thing they have to be told
    /// about.
    pub fn stamp(&mut self, planet: &Planet, sea: f64) {
        let relief = planet.shape().relief;
        for site in &planet.sites {
            let d = site.dir.normalize_or(DVec3::Y);
            let spot = Spot {
                over_sea: planet.radius + site.h - sea,
                kind: Kind::City,
                water: 0.0,
            };
            self.paint(d, &spot, relief);
        }
    }

    fn bake_bare(planet: &Planet, sea: f64, w: usize, h: usize) -> Chart {
        let shape = planet.shape();
        let alt: Vec<f64> = (0..w * h)
            .map(|i| planet.radius + planet.surface(pixel_dir(i % w, i / w, w, h)).0 - sea)
            .collect();
        let slopes = slopes_of(&alt, w, h, planet.radius);
        let full = percentile(&slopes, SLOPE_SPAN);
        let mut albedo = vec![0u8; w * h * 4];
        let mut normal = vec![0u8; w * h * 4];
        for i in 0..w * h {
            let (east, north) = slopes[i];
            let dir = pixel_dir(i % w, i / w, w, h);
            let slope = (east * east + north * north).sqrt();
            let spot = spot_of(planet, alt[i], dir, slope.min(1.0));
            let c = spot_colour(&spot, shape.relief);
            for k in 0..3 {
                albedo[i * 4 + k] = to_srgb(c[k]);
            }
            albedo[i * 4 + 3] = to_byte(spot.water);
            normal[i * 4] = to_byte((east / full).clamp(-1.0, 1.0) * 0.5 + 0.5);
            normal[i * 4 + 1] = to_byte((north / full).clamp(-1.0, 1.0) * 0.5 + 0.5);
            normal[i * 4 + 2] = 255;
            normal[i * 4 + 3] = 255;
        }
        Chart {
            width: w,
            height: h,
            albedo,
            normal,
        }
    }

    /// The albedo at a direction, for a test and for anything that wants
    /// the chart's own answer without a GPU.
    pub fn sample(&self, dir: DVec3) -> [u8; 4] {
        let Some(i) = self.texel(dir) else {
            return [0; 4];
        };
        [
            self.albedo[i],
            self.albedo[i + 1],
            self.albedo[i + 2],
            self.albedo[i + 3],
        ]
    }
}

/// The SLOPE of the drawn surface across one texel: east from the central
/// difference in x, north in y, each divided by the ground the difference
/// was measured over.
///
/// **Divided by its own run, which is what makes it a slope.** The run
/// between two longitude texels narrows as the cosine of the latitude, so
/// a raw difference understates the east slope by exactly that: measured
/// on the test planet, the polar band encoded a mean east of 0.121 where
/// the true slope there is 0.297, two and a half times too flat, while
/// the equator's 0.146 and 0.148 agreed. Every ice cap shaded smooth. It
/// is the same factor the steepness handed to `Kind` is read through, so
/// a chart normalised on raw differences was wrong about how steep the
/// ground is as well as about how it should shade.
///
/// The run is the straight line between the two directions actually
/// differenced rather than a small angle formula, so a row clamped at the
/// pole is divided by the one row it really spans and not by two.
///
/// **Off the ground held UP AT THE SEA**, which is what `distant::sphere`
/// displaces its vertices to and therefore the only surface there is to
/// have a slope. Taken off the raw altitude instead, an ocean carried the
/// SEA BED's relief as shading: on the test planet 13,844 sea texels came
/// out with a mean bend of 45 of 127 and a worst of 128, which is the
/// land's own 50, so every ocean on every body was as bumpy as the
/// continent beside it and the map disagreed with the mesh it was drawn
/// on everywhere the ground fell under the sea.
fn slopes_of(alt: &[f64], w: usize, h: usize, radius: f64) -> Vec<(f64, f64)> {
    let drawn: Vec<f64> = alt.iter().map(|a| a.max(0.0)).collect();
    let rise_over_run = |ax, ay, bx, by| {
        let rise = wrap_alt(&drawn, w, h, ax, ay) - wrap_alt(&drawn, w, h, bx, by);
        let run = (wrap_dir(w, h, ax, ay) - wrap_dir(w, h, bx, by)).length() * radius;
        rise / run.max(1e-9)
    };
    (0..w * h)
        .map(|i| {
            let (x, y) = ((i % w) as isize, (i / w) as isize);
            let span = east_span(w, h, y);
            (
                rise_over_run(x + span, y, x - span, y),
                rise_over_run(x, y - 1, x, y + 1),
            )
        })
        .collect()
}

/// How many texels the east difference reaches over, so that it spans the
/// same GROUND the north one does.
///
/// An equirectangular chart oversamples longitude toward the poles: at
/// eighty degrees two neighbouring texels are a kilometre apart where the
/// row above and below are still six. Ground is fractal, so a gradient
/// measured over the shorter baseline is the steeper one, and the two
/// axes then disagree by scale rather than by what the ground does: the
/// polar band came out at 0.203 against the equator's 0.103, all of it in
/// east, which drew as horizontal streaks across both ice caps.
///
/// Reaching `1 / cos(latitude)` texels holds the run at about one texel
/// of ground whatever the latitude, so the two axes measure the same
/// thing. It is capped at a quarter of the image so the two samples can
/// never meet round the far side; at the pole that is a chord across a
/// cap a few kilometres wide, which is the honest answer where east has
/// stopped meaning anything at all.
fn east_span(w: usize, h: usize, y: isize) -> isize {
    let d = wrap_dir(w, h, 0, y);
    let cos_lat = (1.0 - d.y * d.y).sqrt();
    ((1.0 / cos_lat.max(1e-6)).round() as isize).clamp(1, (w / 4) as isize)
}

/// The direction `wrap_alt` reads at a texel, so a rise and the run it is
/// divided by are measured between the same two samples, clamped rows and
/// wrapped columns alike.
fn wrap_dir(w: usize, h: usize, x: isize, y: isize) -> DVec3 {
    let yy = y.clamp(0, h as isize - 1) as usize;
    let xx = x.rem_euclid(w as isize) as usize;
    pixel_dir(xx, yy, w, h)
}

/// An altitude at a texel, with longitude wrapping round and latitude
/// clamped at the poles: a central difference at the edge of the image is
/// still a difference across real ground.
fn wrap_alt(alt: &[f64], w: usize, h: usize, x: isize, y: isize) -> f64 {
    let yy = y.clamp(0, h as isize - 1) as usize;
    let xx = x.rem_euclid(w as isize) as usize;
    alt[yy * w + xx]
}

/// The magnitude at a share of the way up a chart's own slopes, as a
/// gradient in metres of rise per metre of ground. Never nought, so a body with no relief
/// at all encodes as flat rather than as a division by zero.
fn percentile(slopes: &[(f64, f64)], share: f64) -> f64 {
    let mut m: Vec<f64> = slopes.iter().map(|(a, b)| a.abs().max(b.abs())).collect();
    m.sort_by(f64::total_cmp);
    let at = ((m.len() - 1) as f64 * share) as usize;
    m[at].max(1e-6)
}

/// A LINEAR colour as an sRGB byte, which is what a colour texture holds.
///
/// `Kind::colour` is linear, because that is what a shader wants to do
/// arithmetic in, and the chart is bound as an sRGB texture, because that
/// is where the bits belong on a picture: most of a planet is dark and
/// sRGB spends its bytes there. Writing the linear value into an sRGB
/// texture decodes it a SECOND time on the way out, and the first render
/// of this said so at once: a forest at 0.10 linear came back at 0.0095
/// and the whole planet drew nearly black.
fn to_srgb(v: f64) -> u8 {
    let c = v.clamp(0.0, 1.0);
    let s = if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (s * 255.0 + 0.5) as u8
}

fn to_byte(v: f64) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// The climate at a direction on a body, for anything drawing the near
/// ground: the same pair the chart paints from, so a hillside walked on
/// and the same hillside seen from orbit are the same biome.
pub fn climate_at(planet: &Planet, sea: f64, dir: DVec3) -> Climate {
    let over_sea = planet.radius + planet.surface(dir).0 - sea;
    planet.shape().climate(dir, over_sea)
}

/// Whether a height over the sea is inside the beach band, which both the
/// chart and the ground shader read off one number.
pub fn is_beach(over_sea: f64) -> bool {
    (0.0..BEACH_TO).contains(&over_sea)
}

#[cfg(test)]
mod tests;
