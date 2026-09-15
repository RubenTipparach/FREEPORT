//! The air: tenebris's single scatter ray march (`atmosphere.fs.glsl`,
//! itself a GPU Gems 2 march), ported to f64 so the CPU and the GPU can
//! be handed the same question.
//!
//! It is in the core rather than in a shader alone because the sky and the
//! distance fog have to AGREE. Tenebris says it plainly: the fog's colour
//! is its own sky sampled at the horizon every frame, by the same march,
//! so the two match by construction at noon, at dusk and at night rather
//! than by a pair of numbers somebody tuned to look alike. `sky` is what
//! `atmos.wgsl` transcribes for the dome, and `horizon` is what the ground
//! and the sea fade into: one march, two callers.
//!
//! Every position here is PLANET LOCAL metres, which is the rule for
//! anything that reasons about a body.

use glam::DVec3;

/// How bright a lit sky is, candela a square metre. The march answers in
/// nought to one, because `1 - exp(-scattered)` is a SHARE and not a
/// radiance, and everything else in the scene is a physical quantity
/// times the camera's exposure: a directional light of 8,000 lux lands at
/// about four and a half after it. A sky handed over at nought to one and
/// multiplied by the same exposure lands at five ten thousandths, which
/// is black, and that is exactly what the first cut drew. So the share is
/// scaled into the same units as the light it stands beside.
///
/// It is under a real daylight sky's eight thousand, and the reason is
/// the tone mapper: it desaturates a highlight by scaling every channel
/// toward the peak, so a sky arriving at four lands WHITE and takes its
/// own blue with it. This is swarm-demo's flame lesson on a second
/// surface, and the number is the one that leaves the zenith a blue at
/// about a half and the horizon a pale band well over one.
pub const NITS: f64 = 1_400.0;

/// What an atmosphere is made of, and how thick. The numbers are
/// tenebris's `atmosphere.yaml` for its own planet, which is the Earth
/// blue baseline; a body sets what it wants to differ.
#[derive(Clone, Copy, Debug)]
pub struct Air {
    /// Where the march measures its density from: the planet's own
    /// radius, metres.
    pub ground: f64,
    /// Where a view ray is STOPPED going down: the lowest the real
    /// ground reaches, metres. It is not `ground`, and the difference is
    /// what a picture found. A dome that cuts its ray at the MEAN radius
    /// draws a short, dark path wherever the terrain is lower than the
    /// mean, and since the terrain is not drawn there either that band
    /// stands between the sky and the horizon as a dark stripe: measured
    /// on the thousand kilometre planet, the march reads 2.93 at four
    /// tenths of a milliradian under the horizontal and 0.034 at eight,
    /// a factor of eighty seven across two pixels. Stopped at the lowest
    /// ground instead, no ray is cut short of geometry that is actually
    /// drawn, and the band is the long bright path the fog is already
    /// the colour of. It is also the only conditioning the test has: in
    /// `f32` the eye's own square is 10^12 and the difference of two of
    /// those is quantised to 131 km^2, which at twelve metres over the
    /// mean radius is half a per cent of the whole term and at eight
    /// kilometres over the floor is eight millionths.
    pub floor: f64,
    /// Where it stops going up: the outer shell, metres.
    pub top: f64,
    /// How fast the density falls off, in NORMALISED altitude (nought at
    /// the ground, one at the shell), so the shell's thickness is the
    /// only length in the march.
    pub scale_height: f64,
    /// The sky's main scattering, one being Earth's, and the haze that
    /// makes the sun glow forward.
    pub rayleigh: f64,
    pub mie: f64,
    /// How forward that haze throws, minus one to one.
    pub mie_g: f64,
    /// Overall brightness of the sky.
    pub sun: f64,
    /// The `1 / lambda^4` ratios, which ARE the sky's hue: blue heavy is
    /// an Earth sky, red heavy a dusty one.
    pub waves: DVec3,
    /// How much of the dusk tint is blended in at its peak.
    pub sunset: f64,
    /// The sky's own multiplier near the horizon at dusk, the glow along
    /// the sun's direction, and the band on the horizon itself.
    pub tint: DVec3,
    pub glow: DVec3,
    pub band: DVec3,
    /// Haze on the ground, per metre of view distance, and the altitude
    /// it has faded out by, metres: climb above the weather and the vista
    /// clears.
    ///
    /// `Default`'s three lengths are a THREE HUNDRED METRE planet's, which
    /// is tenebris's own, and `Air::round` is the one place they are
    /// carried to another size. Anything that builds an `Air` by hand and
    /// does not go through `round` is writing metres for its own planet.
    pub fog: f64,
    pub fog_height: f64,
    /// The ground fog: how many metres its density falls off over, and
    /// how many times the plain haze it is down at the sea. Air POOLS in
    /// the low ground, so a valley is hazier than the ridge above it and
    /// a mountain stands out of its own weather, which is the whole of
    /// what makes a distant range read as distant.
    pub pool: f64,
    pub pooled: f64,
}

impl Default for Air {
    fn default() -> Self {
        Air {
            ground: 1.0,
            floor: 1.0,
            // Tenebris's own shell is a quarter of its planet's radius,
            // which on a five kilometre world is a twelve hundred metre
            // doughnut: from orbit it was a thick opaque white ring
            // standing off the disc. Six per cent is three hundred metres
            // here, which reads as the bright RIM a planet wears, and the
            // sweep above is where it came from.
            top: 1.06,
            scale_height: 0.6,
            // Tenebris's own are 0.024 and 0.012 against a sun of twelve,
            // and on this planet they saturate: the zenith came out
            // 0.52,0.69,0.87 and the horizon 0.91,0.97,0.99, which is a
            // white sky over a correct ground. Swept (`sizes::sky_colours`,
            // which prints the sweep) and taken at the setting where the
            // zenith is blue by a factor of two and a half and the horizon
            // is still the pale band it ought to be: 0.20,0.31,0.50 and
            // 0.55,0.72,0.88.
            rayleigh: 0.012,
            mie: 0.006,
            mie_g: 0.85,
            sun: 7.0,
            waves: DVec3::new(5.602, 9.473, 19.644),
            sunset: 0.85,
            tint: DVec3::new(1.3, 1.0, 0.6),
            glow: DVec3::new(1.0, 0.65, 0.25),
            band: DVec3::new(0.8, 0.4, 0.15),
            // Tenebris's own haze is 0.006 a metre on its three hundred
            // metre planet, and with the pooling on top of it the vista
            // drowned. These are that, dialled to where a range reads as
            // far off and a foreground does not, on a planet of THAT size;
            // `round` is what carries them to another one, and the one
            // place that arithmetic is written.
            fog: 0.12,
            fog_height: 480.0,
            pool: 80.0,
            pooled: 2.5,
        }
    }
}

impl Air {
    /// The default air round a planet of `radius` with `relief` metres of
    /// ground between its lowest and its highest: the one place tenebris's
    /// three hundred metre numbers are carried to another size, so the
    /// same air reads the same on a planetoid and on a planet. The relief
    /// is not decoration, it decides two things a radius cannot: how deep
    /// the fog pools, and where a view ray is stopped (`Air::floor`).
    pub fn round(radius: f64, relief: f64) -> Air {
        let d = Air::default();
        // Tenebris's numbers are for a three hundred metre planet, and
        // every LENGTH in them is a share of that: the shell is a
        // multiple of the radius already, and the fog's height and its
        // density are not, so they are made into one here. A density left
        // at tenebris's on a planet sixteen times the size is a vista
        // sixteen times foggier, which is a white screen and was.
        let top = radius * d.top;
        Air {
            ground: radius,
            floor: radius - relief,
            top,
            // The fog's lengths are the GROUND's, not the shell's. How
            // far an eye sees is the horizon, `sqrt(2 R h)`, so it grows
            // as the square root of the radius and the density falls the
            // same way; how DEEP the air pools is the relief, because
            // that is what the weather has to fill. Tied to the shell
            // instead, a shell sixty kilometres thick on a thousand
            // kilometre planet left a vista with no haze in it at all.
            fog: d.fog / (radius / 300.0).sqrt() / 300.0,
            fog_height: relief * 3.0,
            pool: relief * 0.5,
            ..d
        }
    }

    /// How thick the shell is, metres: the one length the march measures
    /// its densities against.
    pub fn thickness(&self) -> f64 {
        (self.top - self.ground).max(f64::MIN_POSITIVE)
    }
}

/// Where a ray meets a sphere about the origin: the two roots, or a pair
/// of minus ones when it misses. `atmosphere.fs.glsl`'s
/// `raySphereIntersect`, and the whole of what bounds the march.
pub fn ray_sphere(from: DVec3, dir: DVec3, radius: f64) -> (f64, f64) {
    let b = from.dot(dir);
    let c = from.dot(from) - radius * radius;
    let disc = b * b - c;
    if disc < 0.0 {
        return (-1.0, -1.0);
    }
    let s = disc.sqrt();
    (-b - s, -b + s)
}

/// The air's density at a normalised altitude, which is the exponential
/// falloff and nothing else.
fn density(air: &Air, alt: f64) -> f64 {
    (-alt / air.scale_height).exp()
}

/// How much air a ray crosses on its way out to the sun, in four samples:
/// `computeOpticalDepth`.
fn optical_depth(air: &Air, from: DVec3, dir: DVec3, len: f64) -> f64 {
    let thickness = air.thickness();
    let step = len * 0.25;
    let n = step / thickness;
    let mut depth = 0.0;
    for i in 0..4 {
        let at = from + dir * (step * (i as f64 + 0.5));
        let alt = ((at.length() - air.ground) / thickness).clamp(0.0, 1.0);
        depth += density(air, alt) * n;
    }
    depth
}

/// Rayleigh's phase: how much of the blue goes which way.
fn rayleigh_phase(cos: f64) -> f64 {
    0.75 * (1.0 + cos * cos)
}

/// Mie's, which is the sun's own glow through the haze.
fn mie_phase(cos: f64, g: f64) -> f64 {
    let g2 = g * g;
    (1.0 - g2) / (12.566_370_6 * ((1.0 + g2) - (2.0 * g * cos)).max(1e-4).powf(1.5))
}

/// What a view ray gathers between `from` and `to` along it: the eight
/// sample march, the Rayleigh and the Mie sums separately, each sample
/// attenuated by the air between it and the eye AND between it and the
/// sun, and a sample the planet shadows contributing nothing but its own
/// extinction. `atmosphere.fs.glsl`'s loop, number for number.
fn gather(air: &Air, eye: DVec3, dir: DVec3, from: f64, to: f64, sun: DVec3) -> (DVec3, DVec3) {
    let thickness = air.thickness();
    let step = (to - from) * 0.125;
    let n = step / thickness;
    let (mut rayleigh, mut mie) = (DVec3::ZERO, DVec3::ZERO);
    let (mut depth_r, mut depth_m) = (0.0, 0.0);
    for i in 0..8 {
        // Half a step in, where the GPU dithers by a hash of the pixel to
        // break the banding a fixed offset leaves. The CPU has no pixel
        // and wants the same answer twice, so it takes the middle.
        let at = eye + dir * (from + step * (i as f64 + 0.5));
        let h = at.length();
        // Below the FLOOR is inside the rock and has no air in it; below
        // the mean radius is a valley, and the air there is the densest
        // there is, which `alt`'s own clamp already says. Skipping on the
        // mean radius instead was a sky that went BLACK the moment an eye
        // under the mean radius looked down: every sample of a downward
        // ray is under it, every one was skipped, and the march came back
        // nought with an alpha of one. The sea is four hundred metres
        // under the mean radius on this planet, so that is most of the
        // ground a player ever stands on.
        if h < air.floor {
            continue;
        }
        let alt = ((h - air.ground) / thickness).clamp(0.0, 1.0);
        let here = density(air, alt);
        let seg = here * n;
        depth_r += seg;
        depth_m += seg;
        if ray_sphere(at, sun, air.ground).0 > 0.0 {
            continue;
        }
        let out = ray_sphere(at, sun, air.top).1.max(0.0);
        let to_sun = optical_depth(air, at, sun, out);
        let att = (-(air.waves * air.rayleigh * (depth_r + to_sun)
            + DVec3::splat(air.mie * (depth_m + to_sun))))
        .exp();
        let contrib = att * here * n;
        rayleigh += contrib;
        mie += contrib;
    }
    (
        rayleigh * air.waves * air.rayleigh,
        mie * DVec3::splat(air.mie),
    )
}

/// How far into dusk the sun is: warmth builds as it nears the horizon,
/// peaks just under it and is gone once it is well down.
fn dusk(height: f64) -> f64 {
    smoothstep(0.18, 0.0, height) * smoothstep(-0.32, -0.1, height)
}

fn smoothstep(a: f64, b: f64, t: f64) -> f64 {
    let k = ((t - a) / (b - a)).clamp(0.0, 1.0);
    k * k * (3.0 - 2.0 * k)
}

/// The sky along a view ray, and how much of it there is: the colour, and
/// an alpha that is one where the ray meets the ground (the air in front
/// of a hillside is opaque) and the scatter's own brightness where it
/// leaves for space, so a faint sky stays clear. What stops the ray going
/// down is `Air::floor` and never `Air::ground`, and that field says why.
pub fn sky(air: &Air, eye: DVec3, dir: DVec3, sun: DVec3) -> (DVec3, f64) {
    let dir = dir.normalize_or(DVec3::Y);
    let sun = sun.normalize_or(DVec3::Y);
    let shell = ray_sphere(eye, dir, air.top);
    if shell.1 < 0.0 {
        return (DVec3::ZERO, 0.0);
    }
    let from = shell.0.max(0.0);
    let ground = ray_sphere(eye, dir, air.floor).0;
    let hits = ground > 0.0;
    let to = if hits { shell.1.min(ground) } else { shell.1 };
    if from >= to {
        return (DVec3::ZERO, 0.0);
    }
    let (rayleigh, mie) = gather(air, eye, dir, from, to, sun);
    let cos = dir.dot(sun);
    let up = eye.normalize_or(DVec3::Y);
    let at_dusk = dusk(sun.dot(up));
    let mut scattered =
        (rayleigh * rayleigh_phase(cos) + mie * mie_phase(cos, air.mie_g)) * air.sun;
    scattered *= DVec3::ONE.lerp(air.tint, at_dusk * air.sunset);
    // The two dusk terms: a glow along the sun and a band on the horizon.
    // The weights are the shape of them and the colours are the body's.
    let glow = air.glow * (cos.max(0.0).powi(3) * at_dusk) * air.sun * 0.22;
    let band = air.band * ((1.0 - dir.dot(up).abs()).powi(3) * at_dusk) * air.sun * 0.11;
    // Tenebris's own line here is `1 - exp(-(...))`, and it is right for
    // the target it draws to: that shader writes an eight bit buffer, so
    // it has to compress its own highlights. This renderer tone maps
    // downstream, and running BOTH is what made the sky white. The
    // compression saturates every channel toward one at the same rate, so
    // it destroys the hue exactly where the air is thickest, which is the
    // horizon and the limb: measured, the horizon came out blue by a
    // factor of 1.3 compressed and 2.0 uncompressed, and 1.3 is a white
    // sky. So the radiance goes out as it is and Bevy's tone mapper does
    // the compressing, once.
    let colour = scattered + glow + band;
    // How much of the view is air rather than what is behind it, which IS
    // the compressed quantity: an optical depth is what an alpha means.
    let alpha = if hits {
        1.0
    } else {
        (1.0 - (-colour.length() * 1.5).exp()).clamp(0.0, 0.95)
    };
    (colour, alpha)
}

/// What the ground and the sea fade into with distance: this air's own sky
/// at the horizon, away from the sun, which is the colour tenebris samples
/// every frame so the fog cannot drift from the dome. Averaged over four
/// bearings, because a horizon with the sun on one side of it is not one
/// colour and a fog that took only the sun's side would glow behind the
/// eye.
pub fn horizon(air: &Air, eye: DVec3, sun: DVec3) -> DVec3 {
    let up = eye.normalize_or(DVec3::Y);
    let east = up.cross(if up.y.abs() < 0.9 { DVec3::Y } else { DVec3::X });
    let east = east.normalize_or(DVec3::X);
    let north = up.cross(east);
    let mut total = DVec3::ZERO;
    for (a, b) in [(1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0)] {
        let dir = east * a + north * b;
        total += sky(air, eye, dir, sun).0;
    }
    total / 4.0
}

/// The fog's density where the eye is, per metre: the air's own, thinned
/// to nothing by `fog_height` over the ground, because climbing above the
/// weather clears the vista. It is a scale on the DENSITY rather than on
/// the result so a shader handed this one number gets exactly `haze`,
/// with no altitude in it and nothing to keep in step.
pub fn fog_density(air: &Air, eye: DVec3) -> f64 {
    let alt = (eye.length() - air.ground).max(0.0);
    air.fog * (1.0 - smoothstep(0.0, air.fog_height.max(f64::MIN_POSITIVE), alt))
}

/// How much thicker the ground fog is at an altitude over the sea: one
/// high up, `pooled` down at the water, falling off over `pool` metres.
/// The shader is handed the same two numbers and computes this at the
/// MIDDLE of the view ray, which is the one place a single sample of a
/// falling density is right.
pub fn pooling(air: &Air, over_sea: f64) -> f64 {
    1.0 + (air.pooled - 1.0) * (-over_sea.max(0.0) / air.pool.max(f64::MIN_POSITIVE)).exp()
}

/// How much of the fog is in the way over `metres`, nought to one:
/// `1 - exp(-d * density)`, tenebris's composite pass, with the pooling
/// at the eye's own altitude.
pub fn haze(air: &Air, eye: DVec3, metres: f64) -> f64 {
    let over = eye.length() - air.ground;
    let density = fog_density(air, eye) * pooling(air, over);
    1.0 - (-metres.max(0.0) * density).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The air round a planetoid, small enough that a test can walk it,
    /// with a hundredth of its radius of relief in it.
    fn air() -> Air {
        Air::round(1000.0, 10.0)
    }

    #[test]
    fn a_ray_meets_a_sphere_where_the_algebra_says() {
        let (a, b) = ray_sphere(DVec3::new(0.0, 0.0, -10.0), DVec3::Z, 2.0);
        assert!((a - 8.0).abs() < 1e-9, "{a}");
        assert!((b - 12.0).abs() < 1e-9, "{b}");
        let (miss, _) = ray_sphere(DVec3::new(0.0, 5.0, -10.0), DVec3::Z, 2.0);
        assert_eq!(miss, -1.0);
    }

    #[test]
    fn the_sky_is_blue_overhead_with_the_sun_up() {
        let air = air();
        let eye = DVec3::new(0.0, air.ground + 2.0, 0.0);
        let (up, _) = sky(&air, eye, DVec3::Y, DVec3::Y);
        assert!(up.z > up.x, "the zenith came out {up:?}");
        assert!(up.z > up.y, "the zenith came out {up:?}");
        assert!(up.length() > 0.02, "the zenith is unlit: {up:?}");
    }

    #[test]
    fn the_sun_side_of_dusk_is_warmer_than_the_zenith() {
        let air = air();
        let eye = DVec3::new(0.0, air.ground + 2.0, 0.0);
        // A sun a little under the horizon, and a look straight at it.
        let sun = DVec3::new(1.0, -0.12, 0.0).normalize();
        let low = DVec3::new(1.0, 0.02, 0.0).normalize();
        let (warm, _) = sky(&air, eye, low, sun);
        let (over, _) = sky(&air, eye, DVec3::Y, sun);
        assert!(
            warm.x / warm.z.max(1e-9) > over.x / over.z.max(1e-9),
            "dusk {warm:?} against the zenith {over:?}"
        );
    }

    #[test]
    fn the_night_side_is_darker_than_the_day() {
        let air = air();
        let eye = DVec3::new(0.0, air.ground + 2.0, 0.0);
        let (day, _) = sky(&air, eye, DVec3::Y, DVec3::Y);
        let (night, _) = sky(&air, eye, DVec3::Y, DVec3::NEG_Y);
        assert!(
            night.length() < day.length() * 0.1,
            "night {night:?} against day {day:?}"
        );
    }

    #[test]
    fn a_ray_that_never_meets_the_shell_is_nothing() {
        let air = air();
        let eye = DVec3::new(0.0, air.top * 4.0, 0.0);
        let (colour, alpha) = sky(&air, eye, DVec3::Y, DVec3::Y);
        assert_eq!(colour, DVec3::ZERO);
        assert_eq!(alpha, 0.0);
    }

    #[test]
    fn a_ground_ray_is_opaque_and_a_sky_ray_is_not() {
        let air = air();
        let eye = DVec3::new(0.0, air.ground + 50.0, 0.0);
        let (_, down) = sky(&air, eye, DVec3::NEG_Y, DVec3::Y);
        let (_, up) = sky(&air, eye, DVec3::Y, DVec3::Y);
        assert_eq!(down, 1.0, "the air over a hillside is not opaque");
        assert!(up < 0.96, "a sky ray came out solid: {up}");
    }

    #[test]
    fn the_fog_is_the_skys_own_colour_and_thins_with_height() {
        let air = air();
        let low = DVec3::new(0.0, air.ground + 2.0, 0.0);
        let fog = horizon(&air, low, DVec3::Y);
        assert!(fog.length() > 0.0, "the fog has no colour");
        // It is a sky colour, so at noon it is on the blue side.
        assert!(fog.z > fog.x, "the fog came out {fog:?}");
        // And it thins as the eye climbs out of it.
        let near = haze(&air, low, 100.0);
        let far = haze(&air, low, 400.0);
        let aloft = haze(&air, DVec3::new(0.0, air.ground + 1e9, 0.0), 400.0);
        assert!(far > near, "{far} against {near}");
        assert!(near > 0.0 && far < 1.0, "{near} to {far}");
        assert!(aloft < 1e-6, "the fog followed the eye up: {aloft}");
    }

    #[test]
    fn an_eye_under_the_mean_radius_has_a_sky_when_it_looks_down() {
        // The sea is under the mean radius and a walker stands beside it,
        // so a look down a beach is a look from under the mean radius at
        // ground under it too. Every sample of that ray is below the mean
        // radius, and a march that skipped them came back nought with an
        // alpha of one: a black sky over a lit shore.
        let air = Air::round(1_000_000.0, 8_000.0);
        let sun = DVec3::new(0.42, 0.62, -0.66).normalize();
        // Forty metres over a sea four hundred metres under the mean
        // radius, looking down at forty five degrees.
        let eye = DVec3::new(0.0, air.ground - 360.0, 0.0);
        let dir = DVec3::new(1.0, -1.0, 0.0).normalize();
        let (down, alpha) = sky(&air, eye, dir, sun);
        assert!(alpha > 0.99, "a ray into the ground is not opaque: {alpha}");
        let level = sky(&air, eye, DVec3::X, sun).0.length();
        assert!(
            down.length() > level * 0.05,
            "looking down from under the mean radius came out {down:?} against {level:.3} level"
        );
    }

    #[test]
    fn the_sky_just_under_the_horizon_is_the_long_path_and_not_a_dark_stripe() {
        // The thousand kilometre planet and an eye twelve metres up,
        // which is where the picture showed a dark band between the sky
        // and the fogged ground. A ray a milliradian under the
        // horizontal meets the real terrain tens of kilometres off, if
        // at all, so it carries the same long bright path the horizontal
        // does; cut at the MEAN radius it carries fifteen hundred metres
        // of it and comes out black.
        let air = Air::round(1_000_000.0, 8_000.0);
        let eye = DVec3::new(0.0, air.ground + 12.0, 0.0);
        let sun = DVec3::new(0.42, 0.62, -0.66).normalize();
        let level = sky(&air, eye, DVec3::X, sun).0.length();
        for milli in [-1.0, -2.0, -4.0, -8.0, -16.0] {
            let dir = (DVec3::X + DVec3::Y * (milli * 1e-3)).normalize();
            let got = sky(&air, eye, dir, sun).0.length();
            assert!(
                got > level * 0.9,
                "{milli} mrad under the horizontal came out {got:.4} against {level:.4}"
            );
        }
        // And this is the band, so nobody takes the floor back out.
        let mean = Air {
            floor: air.ground,
            ..air
        };
        let dir = (DVec3::X - DVec3::Y * 8e-3).normalize();
        let dark = sky(&mean, eye, dir, sun).0.length();
        assert!(dark < level * 0.1, "the mean radius came out {dark:.4}");
    }
}

#[cfg(test)]
mod sizes {
    use super::*;

    /// The sky's colour at the zenith and at the horizon over a sweep of
    /// the two coefficients that decide it, on the harness's own planet.
    /// This is what the shipped pair was picked off, and what a planet
    /// wanting a different air is swept with.
    #[test]
    #[ignore]
    fn sky_colours() {
        let air = Air::round(5_000.0, 50.0);
        let eye = DVec3::new(0.0, air.ground + 12.0, 0.0);
        let sun = DVec3::new(0.42, 0.62, -0.66).normalize();
        // A ray from orbit that grazes the middle of the shell, which is
        // the LIMB: the bright rim a planet wears seen from outside, and
        // the longest path any ray takes through the air.
        let limb = |air: &Air, at: f64| {
            let eye = DVec3::new(0.0, at, 0.0);
            let b = air.ground + (air.top - air.ground) * 0.5;
            let s = b / at;
            let dir = DVec3::new(s, -(1.0 - s * s).sqrt(), 0.0);
            sky(air, eye, dir, sun)
        };
        for m in [0.5, 0.2, 0.05] {
            for (r, i) in [(0.012, 7.0), (0.008, 6.0), (0.005, 5.0)] {
                let air = Air {
                    rayleigh: r,
                    mie: r * m,
                    sun: i,
                    ..air
                };
                let (z, _) = sky(&air, eye, DVec3::Y, sun);
                let (h, _) = sky(&air, eye, DVec3::new(0.0, 0.02, 1.0).normalize(), sun);
                let (l, _) = limb(&air, air.ground * 4.0);
                println!(
                    "mie x{m} rayleigh {r} sun {i}: zenith {:.2},{:.2},{:.2} (blue {:.1}x)  horizon {:.2},{:.2},{:.2} (blue {:.1}x)  limb {:.2},{:.2},{:.2}",
                    z.x, z.y, z.z, z.z / z.x.max(1e-9),
                    h.x, h.y, h.z, h.z / h.x.max(1e-9),
                    l.x, l.y, l.z
                );
            }
        }
    }
}
