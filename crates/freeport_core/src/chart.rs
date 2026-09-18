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
use glam::DVec3;

/// A baked chart of a body: two equirectangular RGBA8 images.
pub struct Chart {
    pub width: usize,
    pub height: usize,
    /// rgb the surface's colour, a the water mask.
    pub albedo: Vec<u8>,
    /// rg the tangent space slope, b flat, a unused.
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
    let shape = planet.shape();
    let over_sea = planet.radius + planet.surface(dir).0 - sea;
    let climate = shape.climate(dir, over_sea);
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
    pub fn bake(planet: &Planet, sea: f64, w: usize, h: usize) -> Chart {
        let shape = planet.shape();
        let alt: Vec<f64> = (0..w * h)
            .map(|i| planet.radius + planet.surface(pixel_dir(i % w, i / w, w, h)).0 - sea)
            .collect();
        // The slope across one texel, which is all a chart can see: east
        // from the central difference in x, north in y.
        let slopes: Vec<(f64, f64)> = (0..w * h)
            .map(|i| {
                let (x, y) = ((i % w) as isize, (i / w) as isize);
                (
                    wrap_alt(&alt, w, h, x + 1, y) - wrap_alt(&alt, w, h, x - 1, y),
                    wrap_alt(&alt, w, h, x, y - 1) - wrap_alt(&alt, w, h, x, y + 1),
                )
            })
            .collect();
        let full = percentile(&slopes, SLOPE_SPAN);
        let texel = std::f64::consts::TAU * planet.radius / w as f64;
        let mut albedo = vec![0u8; w * h * 4];
        let mut normal = vec![0u8; w * h * 4];
        for i in 0..w * h {
            let (east, north) = slopes[i];
            let dir = pixel_dir(i % w, i / w, w, h);
            let slope = (east * east + north * north).sqrt() / (2.0 * texel);
            let spot = spot_at(planet, sea, dir, slope.min(1.0));
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
        let d = dir.normalize_or(DVec3::Y);
        let u = 0.5 + d.z.atan2(d.x) / std::f64::consts::TAU;
        let v = 0.5 - d.y.clamp(-1.0, 1.0).asin() / std::f64::consts::PI;
        let x = ((u * self.width as f64) as usize).min(self.width - 1);
        let y = ((v * self.height as f64) as usize).min(self.height - 1);
        let i = (y * self.width + x) * 4;
        [
            self.albedo[i],
            self.albedo[i + 1],
            self.albedo[i + 2],
            self.albedo[i + 3],
        ]
    }
}

/// An altitude at a texel, with longitude wrapping round and latitude
/// clamped at the poles: a central difference at the edge of the image is
/// still a difference across real ground.
fn wrap_alt(alt: &[f64], w: usize, h: usize, x: isize, y: isize) -> f64 {
    let yy = y.clamp(0, h as isize - 1) as usize;
    let xx = x.rem_euclid(w as isize) as usize;
    alt[yy * w + xx]
}

/// The magnitude at a share of the way up a chart's own slopes, metres
/// of altitude across two texels. Never nought, so a body with no relief
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
