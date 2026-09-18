//! What a place on a planet is LIKE: the terms its relief is made of, and
//! the climate that decides what grows there.
//!
//! One fractal is one landscape. A planet whose surface is a single `fbm3`
//! has lumps of one size everywhere, so it reads as the same hillside from
//! pole to pole however far you walk, and that is what the first cut of
//! this world was. Real ground is a FEW terms of very different characters
//! laid over each other: continents that decide where the sea is, belts
//! where the crust is pushed up into ranges, hills on the flanks of those,
//! and channels cut down through all of it by water that has to reach the
//! sea. Each is cheap, and the variety comes from the composition rather
//! than from any one of them.
//!
//! The same rule as `field`: `f64`, and nothing but add, multiply, floor
//! and compare, because two clients evaluating this have to agree bit for
//! bit. Everything here is a function of a DIRECTION, so it is the same
//! answer at every altitude over a point, which is what lets the equirect
//! bake (`chart`) and the mesher's own samples be the same planet.

use crate::field::{fbm3, noise3};
use glam::DVec3;

/// The terms of a planet's relief, in the shares of its own peak to trough
/// each is worth. They sum to more than one on purpose: a peak in a belt
/// stands over a high continent and a gorge cuts below both, and what the
/// band has to hold is the worst of those rather than the mean.
mod share {
    /// The continents, which is what decides where the sea is.
    pub const CONTINENT: f64 = 0.62;
    /// A mountain belt at its highest.
    pub const MOUNTAIN: f64 = 0.46;
    /// Hills, on everything.
    pub const HILLS: f64 = 0.11;
    /// How far a broad valley cuts under the ground round it.
    pub const VALLEY: f64 = 0.05;
}

/// The frequencies, in cycles per unit of direction, as multiples of the
/// planet's own `lumps`. A continent is much larger than a lump and a
/// channel network much finer, which is the whole of what makes them read
/// as different things.
mod freq {
    pub const CONTINENT: f64 = 0.38;
    pub const BELT: f64 = 0.44;
    pub const RIDGE: f64 = 1.15;
    pub const HILLS: f64 = 3.1;
    /// A channel network is cut at the size of a VALLEY rather than of a
    /// continent: at the planet's own twelve lumps this is a base
    /// wavelength of about two hundred kilometres, which its five octaves
    /// take down to thirteen, so a gorge is a feature of a landscape
    /// rather than a crack across a hemisphere.
    pub const CHANNEL: f64 = 2.4;
    pub const CLIMATE: f64 = 0.7;
}

/// A gorge's own depth and half width, metres. They are ABSOLUTE rather
/// than shares of the relief, because a gorge is the size a gorge is
/// whatever the planet's mountains are worth: written as a share of eight
/// kilometres it would be a rift nobody could walk out of.
///
/// CAPPED by the planet's own relief, which the first cut was not, and a
/// test planet said so at once: a hundred metre ball with four metres of
/// relief was handed a thirty four metre gorge, which is most of the way
/// to its centre, and the mesher pinched on the wall of it. An absolute
/// number is a number that is wrong on some body, so it is a number with
/// a share beside it.
const GORGE_DEEP: f64 = 34.0;
const GORGE_WIDE: f64 = 90.0;
/// The most of a planet's own relief a gorge may take.
const GORGE_SHARE: f64 = 0.25;

/// How near a channel's own middle the gorge is cut, as a share of the
/// channel function's top: the network is a ridge of the channel noise and
/// only its crest is a watercourse.
const CHANNEL_LIP: f64 = 0.82;

/// A mountain belt is where the belt noise stands this many standard
/// deviations over its own mean, and is at full height past the second.
/// Two numbers rather than a threshold, because a range that starts at a
/// line has a cliff down its whole length. In measured units: a belt
/// covers about a fifth of the planet and is at full height over a
/// twentieth of it, which is roughly what Earth's ranges are worth.
const BELT_FROM: f64 = FBM_MEAN + 0.85 * FBM_SD;
const BELT_TO: f64 = FBM_MEAN + 1.75 * FBM_SD;

/// The steepest `noise3` gets per unit of its argument, `field`'s own
/// number: a smoothstep climbs at one and a half at most, across a unit
/// cell, along each of three axes.
const NOISE_SLOPE: f64 = 2.598_076_211_353_316;

/// What `fbm3` actually spans, MEASURED rather than assumed
/// (`measure_the_fbm_spread` prints it): a sum of halving octaves is a
/// sum of many small numbers, so it piles up near its middle and never
/// reaches either end. Over twenty thousand directions its mean is 0.498
/// and its standard deviation 0.106, whatever the octave count past four,
/// and 99 in 100 samples are inside 0.262 to 0.736.
///
/// A term written as a share of the relief and handed a RAW fbm is
/// therefore worth about a fifth of what it says, which is exactly what
/// the single fractal world was: eight kilometres of relief that never
/// left plus or minus sixteen hundred metres. Everything here stretches
/// by the measured spread first.
const FBM_MEAN: f64 = 0.498;
const FBM_SD: f64 = 0.106;

/// How many standard deviations are stretched to the full range. At two
/// and a quarter the top and bottom hundredth of the planet flattens into
/// a plateau or a basin, which is a thing real ground has and no part of
/// a picture anybody looks at twice.
const FBM_REACH: f64 = 2.25;

/// A noise stretched to minus one through one, so a term written as a
/// share of the relief is worth that share.
fn signed(v: f64) -> f64 {
    ((v - FBM_MEAN) / (FBM_SD * FBM_REACH)).clamp(-1.0, 1.0)
}

/// The slope a stretch multiplies by, which the bound has to carry.
const STRETCH: f64 = 1.0 / (FBM_SD * FBM_REACH);

/// The steepest ground this model will make, metres of rise per metre
/// along the surface, before every term is scaled back to fit.
///
/// MEASURED against the mesher rather than chosen: the dual contoured
/// seam closes on a bound of 18.7 (the rough test ball as it was) and
/// does not on 94.7 (the same ball once the terms were stretched), which
/// came back as 32 open edges and 81 pinches. A body whose own numbers
/// ask for more than this is asking for ground no mesher can close, and
/// the honest answer is to give it less rather than to leave a hole in
/// it. The thousand kilometre planet asks for 9.95 and is untouched; it
/// is the twenty metre balls with a fifth of their radius in relief, a
/// planetoid rather than a planet, that this catches.
const MAX_SLOPE: f64 = 20.0;

/// How much steeper a RIDGED term is than the noise under it. `1 - |2n-1|`
/// doubles the slope, and the power sharpening its crest is at its own
/// steepest at the crest, where it multiplies by the exponent.
const RIDGE_SLOPE: f64 = 2.0 * RIDGE_POWER;

/// How sharp a ridge's crest is. One is a plain fold; higher is a crest
/// with flanks that fall away, which is what a range looks like.
const RIDGE_POWER: f64 = 1.6;

fn smoothstep(a: f64, b: f64, t: f64) -> f64 {
    let k = ((t - a) / (b - a)).clamp(0.0, 1.0);
    k * k * (3.0 - 2.0 * k)
}

/// A planet's relief terms, built from the planet's own numbers so there
/// is one place a body's size and roughness are written down.
#[derive(Clone, Copy, Debug)]
pub struct Shape {
    /// Peak to trough of the whole relief, metres.
    pub relief: f64,
    /// How many of the planet's own lumps fit round it.
    pub lumps: f64,
    /// Octaves the fine terms are summed over.
    pub octaves: u32,
    pub seed: u32,
    /// Mean radius, metres, which is what turns a slope per unit direction
    /// into a slope per metre.
    pub radius: f64,
}

/// How many octaves each term is summed over, and the seed each is salted
/// with. They are HERE, in one table, because `sampling.wgsl` transcribes
/// this function and a term whose octave count or salt differed between
/// the two would be ground the walker stands on and the mesher never drew.
/// The counts are small on purpose: these terms are what a planet looks
/// like from orbit, and the metre of detail under the feet is the hills
/// term, which keeps the planet's own octaves and the sampler's early out.
mod oct {
    pub const CONTINENT: u32 = 4;
    pub const BELT: u32 = 4;
    pub const RIDGE: u32 = 5;
    pub const CHANNEL: u32 = 5;
    pub const CLIMATE: u32 = 4;
}

impl Shape {
    /// A term's octaves, never finer than the PLANET's own. The planet's
    /// `octaves` is what ties its detail to its size (`log2(2 pi R / lumps
    /// / 2 m)`, eighteen on the thousand kilometre world and three on a
    /// twenty metre test ball), and a term that ignored it would put
    /// features under the lattice's own cell on any small body: the first
    /// cut of this handed a four metre test planet five octaves of channel
    /// at two and a half times its base frequency, which is a one metre
    /// slot four cells wide, and the mesher came back with 32 open edges,
    /// 81 pinches and 1,192 triangles facing in. A term is as fine as the
    /// body says and no finer.
    fn oct(&self, want: u32) -> u32 {
        want.min(self.octaves.max(1))
    }
}

mod salt {
    pub const CONTINENT: u32 = 0x00C0;
    pub const BELT: u32 = 0x8E17;
    pub const RIDGE: u32 = 5;
    pub const HILLS: u32 = 0x51ED;
    pub const CHANNEL: u32 = 5;
    pub const WARM: u32 = 0x7A31;
    pub const DAMP: u32 = 0x1D55;
}

impl Shape {
    /// A planet's own numbers, read off it, so a body's size and roughness
    /// are written down once and the chart, the mesher and the walker all
    /// ask the same shape.
    pub fn of(planet: &crate::field::Planet) -> Shape {
        Shape {
            relief: planet.relief,
            lumps: planet.lumps,
            octaves: planet.octaves,
            seed: planet.seed,
            radius: planet.radius,
        }
    }
}

/// A noise in nought to one, on its own seed offset.
fn layer(dir: DVec3, f: f64, seed: u32, salt: u32, octaves: u32) -> f64 {
    fbm3(dir * f, seed.wrapping_add(salt), octaves)
}

/// A ridged fold: one along the crest, nought in the troughs, with the
/// crest sharpened so the flanks fall away from it. The fold is taken of
/// the STRETCHED noise, or it reads near one nearly everywhere and the
/// belt comes out as a plateau rather than a range of peaks.
fn ridged(n: f64) -> f64 {
    let fold = 1.0 - signed(n).abs();
    fold.clamp(0.0, 1.0).powf(RIDGE_POWER)
}

impl Shape {
    /// The relief's own terms at a direction, in metres over the mean
    /// radius, before any town levels its site. Positive is up.
    ///
    /// Read as a sentence: a continent decides how high the ground stands
    /// and therefore whether there is sea over it; a belt decides whether
    /// a range is pushed up through it; hills ride everything; and a
    /// channel network cuts down through whatever is above the sea.
    pub fn height(&self, dir: DVec3) -> f64 {
        self.raw_height(dir) * self.gain()
    }

    /// How far every term is scaled back so this body's ground stays
    /// inside `MAX_SLOPE`. One on any planet worth the name.
    pub fn gain(&self) -> f64 {
        let raw = self.raw_slope();
        if raw <= MAX_SLOPE {
            1.0
        } else {
            MAX_SLOPE / raw
        }
    }

    fn raw_height(&self, dir: DVec3) -> f64 {
        let landform = self.landform(dir);
        landform - self.cut(dir, landform) + self.hills(dir)
    }

    /// What the LANDFORM is at a direction, metres over the mean radius:
    /// the continent and whatever range is pushed up through it, and
    /// nothing finer. It is what the channel network cuts into, because
    /// whether a river runs here is a question about the shape of the land
    /// and not about the hills on it, and it is what `sampling.wgsl`
    /// evaluates in full before it starts the detail octaves it can stop
    /// early on.
    pub fn landform(&self, dir: DVec3) -> f64 {
        let half = self.relief * 0.5;
        signed(self.continent(dir)) * half * share::CONTINENT
            + self.mountain(dir) * half * share::MOUNTAIN
    }

    /// The hills on everything, metres. The one term fine enough to be
    /// the ground under a walker's feet, so it keeps the planet's own
    /// octave count.
    pub fn hills(&self, dir: DVec3) -> f64 {
        signed(layer(
            dir,
            self.lumps * freq::HILLS,
            self.seed,
            salt::HILLS,
            self.octaves,
        )) * self.relief
            * 0.5
            * share::HILLS
    }

    /// The continent term alone, nought to one, which is what the sea is
    /// measured against and what the climate's continentality reads.
    pub fn continent(&self, dir: DVec3) -> f64 {
        layer(
            dir,
            self.lumps * freq::CONTINENT,
            self.seed,
            salt::CONTINENT,
            self.oct(oct::CONTINENT),
        )
    }

    /// How much of a mountain belt is at a direction, nought to one, and
    /// how high the range in it stands there.
    fn mountain(&self, dir: DVec3) -> f64 {
        let belt = layer(
            dir,
            self.lumps * freq::BELT,
            self.seed,
            salt::BELT,
            self.oct(oct::BELT),
        );
        let weight = smoothstep(BELT_FROM, BELT_TO, belt);
        if weight <= 0.0 {
            return 0.0;
        }
        let crest = ridged(layer(
            dir,
            self.lumps * freq::RIDGE,
            self.seed,
            salt::RIDGE,
            self.oct(oct::RIDGE),
        ));
        crest * weight
    }

    /// The channel network at a direction, nought off it and one along the
    /// middle of a watercourse.
    pub fn channel(&self, dir: DVec3) -> f64 {
        let n = layer(
            dir,
            self.lumps * freq::CHANNEL,
            self.seed,
            salt::CHANNEL,
            self.oct(oct::CHANNEL),
        );
        let fold = 1.0 - signed(n).abs();
        smoothstep(CHANNEL_LIP, 1.0, fold.clamp(0.0, 1.0))
    }

    /// How far the water cuts down at a direction, metres, given how high
    /// the ground stands there before it is cut.
    ///
    /// Two cuts in one: a BROAD valley, a share of the relief, which is
    /// what makes a range read as a range of separate peaks rather than a
    /// wall; and a GORGE inside it, in absolute metres, which is the thing
    /// a walker stands at the bottom of. Neither cuts below the sea, since
    /// a channel that reached the sea floor would be a trench with no
    /// water in it, and both fade out as the ground falls to the shore,
    /// which is what leaves a delta flat rather than a slot.
    fn cut(&self, dir: DVec3, standing: f64) -> f64 {
        let c = self.channel(dir);
        if c <= 0.0 {
            return 0.0;
        }
        // Nothing is cut under the sea, and the cut eases off over the
        // last of the land so a river mouth is a marsh and not a cliff.
        let over = standing - self.sea_at();
        if over <= 0.0 {
            return 0.0;
        }
        let deep = self.gorge();
        let reach = smoothstep(0.0, self.relief * 0.04 + deep, over);
        let broad = self.relief * 0.5 * share::VALLEY * c;
        (broad + deep * c) * reach
    }

    /// How deep a gorge cuts on this body, metres.
    fn gorge(&self) -> f64 {
        GORGE_DEEP.min(self.relief * GORGE_SHARE)
    }

    /// Where the sea stands in the same metres `height` answers in. The
    /// shape does not own the sea, so this is nought: the caller offsets
    /// its own level. It is here so `cut` reads as the rule it is.
    fn sea_at(&self) -> f64 {
        0.0
    }

    /// A bound on how fast `height` can change, metres of height per metre
    /// along the surface. Every term is counted at its own amplitude times
    /// its own frequency, the ridged ones at the extra slope folding and
    /// sharpening cost them, and the channel's cut at the steepest its own
    /// smoothstep gets. Deliberately GENEROUS: an overstated bound costs a
    /// chunk a few samples it did not need, and an understated one rules a
    /// chunk empty that the surface crosses, which is a hole in the world.
    pub fn slope(&self) -> f64 {
        self.raw_slope() * self.gain()
    }

    fn raw_slope(&self) -> f64 {
        let half = self.relief * 0.5;
        let per = |amp: f64, f: f64, octaves: u32| {
            amp * NOISE_SLOPE * STRETCH * self.lumps * f * octaves.max(1) as f64 / self.radius
        };
        let continent = per(
            half * share::CONTINENT,
            freq::CONTINENT,
            self.oct(oct::CONTINENT),
        );
        // A belt's own edge and the crest inside it both move; the product
        // is bounded by the sum of each moving at its own rate.
        let belt_edge = half * share::MOUNTAIN * 1.5 / (BELT_TO - BELT_FROM)
            * NOISE_SLOPE
            * self.lumps
            * freq::BELT
            * self.oct(oct::BELT) as f64
            / self.radius;
        // The channel's own fold is stretched too, and its lip is a
        // smoothstep across what is left of a cell.
        let crest = per(
            half * share::MOUNTAIN * RIDGE_SLOPE,
            freq::RIDGE,
            self.oct(oct::RIDGE),
        );
        let hills = per(half * share::HILLS, freq::HILLS, self.octaves);
        // The gorge is the steep one: its whole depth over the width of
        // its own lip, which is a fraction of one channel cell.
        let channel_cell = self.radius / (self.lumps * freq::CHANNEL * STRETCH).max(1e-9);
        let lip = (channel_cell * (1.0 - CHANNEL_LIP)).max(GORGE_WIDE);
        let cut = (half * share::VALLEY + self.gorge()) * 1.5 / lip;
        continent + belt_edge + crest + hills + cut
    }

    /// The radii the surface stays between, metres from the centre. The
    /// terms can only all agree at one point, so this is the sum of their
    /// worst cases and the band is wider than the ground ever is: the cost
    /// is chunks that are sampled and found empty, never a hole.
    pub fn band(&self) -> (f64, f64) {
        let half = self.relief * 0.5 * self.gain();
        let up = half * (share::CONTINENT + share::MOUNTAIN + share::HILLS);
        let down =
            half * (share::CONTINENT + share::HILLS + share::VALLEY) + self.gorge() * self.gain();
        (self.radius - down, self.radius + up)
    }
}

/// Every number `sampling.wgsl` needs to evaluate `landform` and `cut` for
/// one body, laid out as the uniform it binds.
///
/// The shader carries the SHAPE of the function and this carries every
/// constant in it, so a threshold cannot be tuned here and left stale
/// there: a planet whose mesher and whose walker disagreed about where a
/// mountain belt starts would be ground you fall through.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Gpu {
    /// continent, mountain, hills, valley, as shares of half the relief.
    pub shares: [f32; 4],
    /// The base frequencies, as multiples of the body's own lumps:
    /// continent, belt, ridge, hills.
    pub freqs: [f32; 4],
    /// The channel's frequency and lip, the gorge's depth in metres, and
    /// how far over the sea a cut reaches its full depth.
    pub channel: [f32; 4],
    /// A belt's two thresholds, a ridge's power, and the stretch.
    pub belt: [f32; 4],
    /// The fbm's measured mean and the span it is stretched by, half the
    /// relief, and the gain every term is scaled by.
    pub fbm: [f32; 4],
    /// The octaves of continent, belt, ridge and channel.
    pub octaves: [u32; 4],
    /// The seeds those four are salted with.
    pub salts: [u32; 4],
    /// The hills term's salt and octaves, and two spare.
    pub hills: [u32; 4],
}

impl Shape {
    /// This body's own numbers, for the sampler.
    pub fn gpu(&self) -> Gpu {
        let gain = self.gain();
        Gpu {
            shares: [
                share::CONTINENT as f32,
                share::MOUNTAIN as f32,
                share::HILLS as f32,
                share::VALLEY as f32,
            ],
            freqs: [
                freq::CONTINENT as f32,
                freq::BELT as f32,
                freq::RIDGE as f32,
                freq::HILLS as f32,
            ],
            channel: [
                freq::CHANNEL as f32,
                CHANNEL_LIP as f32,
                self.gorge() as f32,
                (self.relief * 0.04 + self.gorge()) as f32,
            ],
            belt: [
                BELT_FROM as f32,
                BELT_TO as f32,
                RIDGE_POWER as f32,
                STRETCH as f32,
            ],
            fbm: [
                FBM_MEAN as f32,
                (FBM_SD * FBM_REACH) as f32,
                (self.relief * 0.5) as f32,
                gain as f32,
            ],
            octaves: [
                self.oct(oct::CONTINENT),
                self.oct(oct::BELT),
                self.oct(oct::RIDGE),
                self.oct(oct::CHANNEL),
            ],
            salts: [salt::CONTINENT, salt::BELT, salt::RIDGE, salt::CHANNEL],
            hills: [salt::HILLS, self.octaves.max(1), 0, 0],
        }
    }
}

/// What the weather is at a place: how warm and how wet, nought to one
/// each. It is what decides whether ground at the same height and slope is
/// ice, hay, scrub or sand, and the shader reads it off the vertex rather
/// than working it out per fragment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Climate {
    /// Nought at the poles, one at the hottest part of the equator.
    pub temp: f64,
    /// Nought in a desert, one in a marsh.
    pub wet: f64,
}

/// How many METRES of altitude cost a whole unit of temperature.
///
/// It was a share of the RELIEF, and the first chart of the planet said
/// what that does: a body with eight kilometres of relief lost most of a
/// unit of temperature over its own mountains, so everything above the
/// plains froze and the world came out white from the poles to the
/// tropics with no desert anywhere on it.
///
/// Altitude is not a share of anything. Earth's lapse rate is about 6.5
/// degrees a kilometre, and this range spans about sixty, which puts a
/// unit at nine kilometres; at six the equator's snow line lands near
/// five thousand metres, which is Kilimanjaro's, and a temperate
/// mountain's near two and a half thousand. Six it is, and a planet with
/// five hundred metres of relief has snow by LATITUDE alone, which is
/// also right.
const LAPSE_M: f64 = 6_100.0;

/// How far the climate bands are pushed about by noise, in units of
/// `temp`. Without it every planet is a set of perfect stripes.
const WANDER: f64 = 0.22;

/// How fast temperature falls off the equator. Higher keeps the tropics
/// broad and the cold band tight: at 1.8 the ground freezes by latitude
/// alone past about 65 degrees, which is roughly Earth's own tree line
/// and leaves a fifth of the planet frozen rather than a third.
const BAND_FALL: f64 = 1.8;

/// How much drier the middle of a continent is than its shore, in units of
/// `wet`. It is what puts a desert inland and a marsh on the coast.
const CONTINENTAL: f64 = 0.45;

impl Shape {
    /// The climate at a direction, given how high the ground stands there
    /// over the sea in metres.
    ///
    /// Temperature is the latitude, warped so the bands are not stripes,
    /// less what the altitude takes off it. Moisture is its own noise,
    /// wetter near the sea and drier in the middle of a continent, and
    /// wetter again in the low ground where water collects.
    pub fn climate(&self, dir: DVec3, over_sea: f64) -> Climate {
        let lat = dir.y.clamp(-1.0, 1.0).abs();
        let banded = 1.0 - lat.powf(BAND_FALL);
        let wander = (layer(
            dir,
            self.lumps * freq::CLIMATE,
            self.seed,
            salt::WARM,
            self.oct(oct::CLIMATE),
        ) * 2.0
            - 1.0)
            * WANDER;
        let lapse = over_sea.max(0.0) / LAPSE_M;
        let temp = (banded + wander - lapse).clamp(0.0, 1.0);
        // Stretched, like every other term: a raw fbm sits inside a third
        // of its own range, so an unstretched moisture never reaches
        // either a desert or a marsh and the whole planet comes out the
        // one green in the middle.
        let damp = signed(layer(
            dir,
            self.lumps * freq::CLIMATE * 1.7,
            self.seed,
            salt::DAMP,
            self.oct(oct::CLIMATE),
        )) * 0.5
            + 0.5;
        // How far inland: the continent term over the sea's own level is
        // the cheapest measure of it there is, and it is already computed
        // for the height.
        let inland = smoothstep(0.5, 0.95, self.continent(dir));
        // Low ground collects water; a channel is a river, and wet.
        let low = 1.0 - smoothstep(0.0, self.relief * 0.25, over_sea.max(0.0));
        let wet =
            (damp - inland * CONTINENTAL + low * 0.25 + self.channel(dir) * 0.3).clamp(0.0, 1.0);
        Climate { temp, wet }
    }
}

/// What is actually on the ground at a place: the one thing a picture is
/// judged on, and what the planet's own texture is painted from.
///
/// It is derived rather than stored, so the chart, the chunk and the
/// shader cannot disagree about what a place is made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Ocean,
    Ice,
    Beach,
    Desert,
    Savanna,
    Grass,
    Forest,
    Marsh,
    Rock,
    Snow,
    City,
}

/// Over the sea by less than this is a beach, metres. The shader's own
/// sand band, so a wader comes out of the water onto sand on both.
pub const BEACH_TO: f64 = 2.8;

/// Colder than this is ice at the sea and snow on the ground.
const FREEZING: f64 = 0.18;
/// Warmer than this and drier than the second is desert.
const HOT: f64 = 0.45;
const ARID: f64 = 0.36;
/// Wetter than this on low ground is marsh.
const SWAMPY: f64 = 0.78;
/// Wetter than this is forest rather than open grass.
const WOODED: f64 = 0.52;

impl Climate {
    /// What is on the ground, given how far over the sea it stands and how
    /// steep it is (nought flat, one a wall). Steep ground is rock at any
    /// climate, because nothing holds on a cliff.
    pub fn kind(&self, over_sea: f64, slope: f64) -> Kind {
        if over_sea < 0.0 {
            return if self.temp < FREEZING {
                Kind::Ice
            } else {
                Kind::Ocean
            };
        }
        if self.temp < FREEZING {
            return Kind::Snow;
        }
        if slope > 0.55 {
            return Kind::Rock;
        }
        if over_sea < BEACH_TO {
            return Kind::Beach;
        }
        if self.temp > HOT && self.wet < ARID {
            return Kind::Desert;
        }
        if self.wet > SWAMPY && over_sea < 40.0 {
            return Kind::Marsh;
        }
        if self.wet < ARID {
            return Kind::Savanna;
        }
        if self.wet > WOODED {
            return Kind::Forest;
        }
        Kind::Grass
    }
}

impl Kind {
    /// The colour a body of this kind reads as from orbit, linear rgb.
    /// These are what the planet's own texture is painted with, and the
    /// near ground's tint is the same table, so a coast seen from space
    /// and the same coast walked on are the same colours.
    pub fn colour(self) -> [f32; 3] {
        match self {
            Kind::Ocean => [0.016, 0.075, 0.16],
            Kind::Ice => [0.72, 0.78, 0.84],
            Kind::Beach => [0.62, 0.55, 0.38],
            Kind::Desert => [0.66, 0.50, 0.28],
            Kind::Savanna => [0.44, 0.40, 0.19],
            Kind::Grass => [0.20, 0.30, 0.11],
            Kind::Forest => [0.10, 0.19, 0.08],
            Kind::Marsh => [0.17, 0.23, 0.13],
            Kind::Rock => [0.26, 0.24, 0.22],
            Kind::Snow => [0.82, 0.85, 0.88],
            Kind::City => [0.35, 0.34, 0.33],
        }
    }

    /// A name, for a log line and a test that fails readably.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Ocean => "ocean",
            Kind::Ice => "ice",
            Kind::Beach => "beach",
            Kind::Desert => "desert",
            Kind::Savanna => "savanna",
            Kind::Grass => "grass",
            Kind::Forest => "forest",
            Kind::Marsh => "marsh",
            Kind::Rock => "rock",
            Kind::Snow => "snow",
            Kind::City => "city",
        }
    }

    /// Every kind, for a test that has to cover them and a legend that has
    /// to list them.
    pub fn all() -> [Kind; 11] {
        [
            Kind::Ocean,
            Kind::Ice,
            Kind::Beach,
            Kind::Desert,
            Kind::Savanna,
            Kind::Grass,
            Kind::Forest,
            Kind::Marsh,
            Kind::Rock,
            Kind::Snow,
            Kind::City,
        ]
    }
}

/// The volumetric carve's own weight at a direction: nought where the
/// ground is soft and one where it is bare rock, so a cliff undercuts and
/// a meadow does not. It is what stops the overhang term turning every
/// grassy hillside into a pile of boulders.
pub fn carve_weight(climate: Climate, slope: f64) -> f64 {
    let bare = smoothstep(0.3, 0.62, slope);
    let dry = 1.0 - smoothstep(ARID, WOODED, climate.wet) * 0.6;
    bare * dry
}

/// A hash in nought to one off a direction, for anything that wants a
/// stable per place number and no continuity.
pub fn spot(dir: DVec3, seed: u32) -> f64 {
    noise3(dir * 512.0, seed)
}

#[cfg(test)]
mod tests;
