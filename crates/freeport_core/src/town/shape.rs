//! How BIG a town is along a bearing, how much TOWN there is at a point
//! of its grid, and which of the three zones that point is in.
//!
//! One number and three readings of it: `edge` says where a town stops,
//! `demand` says how much town there is on the way there, and `Zone`
//! cuts that at its own thresholds. A town's outline, its density and
//! its skyline cannot disagree, because there is nothing for them to
//! disagree about.

use super::PITCH;
use crate::field::{fbm3_rough, noise3};
use glam::{DVec2, DVec3};

/// A town's own OUTLINE, which is not a circle.
///
/// `LOBE` is how many lobes across the town the noise deciding its edge
/// has, and `REACH` how far that noise can push the edge either way as a
/// share of the nominal radius. A town is what grew where growing was
/// easy, so it reaches down one valley and stops short against whatever
/// was in the way on the other side: a disc reads as a stamp, and the
/// same disc at every one of a hundred and sixty sites reads as a stamp
/// used a hundred and sixty times.
const LOBE: f64 = 1.6;
pub(crate) const REACH: f64 = 0.42;
/// How far a town is stretched along its own shore, and squeezed across
/// it by the same factor so the ground it covers is unchanged.
const STRETCH: f64 = 1.45;
/// The furthest a town's own outline can reach, as a multiple of its
/// nominal radius: the lobes and the stretch together.
pub const OUTLINE: f64 = (1.0 + REACH) * STRETCH;

/// The GRAIN: how much of a block's own demand a fractal may take away
/// at a town's rim, how wide that fractal's coarsest lump is as a share
/// of the nominal radius, and how many octaves it runs to.
///
/// **A CITY IS NOT AN OVAL, which is the owner's own word for what the
/// lobes left.** `edge` is two octaves of noise on the BEARING, so a
/// town is a stretched ellipse with a couple of gentle dents in it: ONE
/// scale, ONE closed curve, and from a kilometre up a hundred and sixty
/// of them read as one smooth blob each. A real city is ragged at every
/// scale it has, which is what Batty and Longley measured and called a
/// fractal city: a built up area is a cluster with holes in it, arms
/// down its roads and outlying pieces off its fringe, and its boundary
/// is crinkled from the district down to the block.
///
/// **A level set of a fractal IS a fractal**, which this file already
/// says about continents ("land is a level set of a fractal, so a swell
/// twice as wide crosses the sea half as often"). So the grain is an
/// octave sum on the block's own PLACE rather than on its bearing, and
/// what it does is take demand away: the zones are the demand's own
/// thresholds, so ONE fractal makes the outline ragged, the density
/// patchy and the skyline broken all at once, which is this file's own
/// rule that a town's edge, its density and its heights are three
/// readings of one number. Measured, the boundary's own box counting
/// dimension goes from **0.987 to 1.173** (`town::fractal`), and it
/// climbs with the bite at every step of the sweep, which is what says
/// the number is reading the grain and not the lobes under it.
///
/// **It only ever takes demand AWAY, and that is the whole of what
/// keeps it affordable.** `Site::level_r` is `edge(bearing) + APRON`
/// and the ground a town levels follows it; a grain that could ADD
/// demand would put blocks outside that plateau, which is the buried
/// suburb this file measured at 1.13 m once already. Subtracting leaves
/// the built set a SUBSET of the star shaped region the outline bounds,
/// so `Site::level_r`, `field::site_skirt`, `WOBBLE` and the planet's
/// own slope bound are exactly what they were, not one chunk is
/// contoured differently, and the atlas needs no re-bake: what it
/// stores is where a town STANDS and `lay` derives the grid again.
///
/// **And it bites HARDER the further out a block is**, by `1 - want`,
/// which is nought at the middle and one at the rim. A city's core is
/// solid and its fringe is shredded, which is what the density of a
/// real built up area does as you walk out of one; a uniform bite would
/// eat the towers at the same rate and leave a town that is merely
/// thinner everywhere. It also means a block is never removed inside
/// `GRAIN / (1 + GRAIN)` of demand, which is 0.645 of the way out to
/// the edge: a town always has a solid middle however the noise rolls,
/// and a one block hamlet cannot be shredded to nothing.
pub(crate) const GRAIN: f64 = 0.55;
/// The coarsest lump, as a share of the nominal radius: about one
/// town across, so the biggest thing the grain does is take a bite out
/// of one side of a city.
pub(crate) const GRAIN_LUMP: f64 = 0.9;

/// How much of its weight each octave of the grain keeps, and it is
/// the ONE constant here that decides whether a plan is a city or an
/// oval with dents in it.
///
/// `fbm3`'s own half is a Hurst exponent of ONE, and a level set of a
/// field with `H` = 1 has dimension `2 - H` = 1, which is a smooth
/// curve however many octaves are piled on it: a town grained by an
/// `fbm3` measures **1.007** against the bare ellipse's 0.987, a
/// picture nobody could tell from the oval. SWEPT with the stretch
/// HELD, so only the roughness moves: 0.50 gives 1.007, 0.60 gives
/// 1.056, 0.70 gives 1.114, 0.80 gives **1.173** and 0.88 gives 1.223,
/// while the share of the frame the plan covers stays between 16.2 and
/// 16.6% throughout, which is what says the dimension is reading the
/// roughness and not how much was bitten.
///
/// 0.80 rather than 0.88 because the band Batty and Longley measure
/// real built up edges in starts about here, and because past it the
/// fine octaves are nibbling single blocks rather than shaping
/// anything: the drawn plan at 0.85 differs from the one at 0.72 by
/// nineteen lots in fourteen hundred and by nothing a picture shows.
pub(crate) const GRAIN_PERSIST: f64 = 0.80;

/// The FINEST octave is one block and no finer, which is why the count
/// is read off the town's own size rather than written down: past the
/// block grid an octave is noise the plan cannot express, and a hamlet
/// of four blocks has ONE scale and should have one lump. It is this
/// file's own "every term is capped by the planet's octave count" rule
/// arriving at a city.
pub(crate) fn grain_octaves(radius: f64) -> u32 {
    let span = (radius * GRAIN_LUMP / PITCH).max(1.0);
    (1.0 + span.log2()).round().clamp(1.0, 8.0) as u32
}

/// The grain's own MEASURED mean and spread, which is what lets a
/// threshold on it mean a share. A sum of many small numbers piles up
/// near its middle, so a share handed the raw sum is worth about a
/// fifth of what it says, which is this file's own oldest measurement
/// (`biome::measure_the_fbm_spread`) arriving at a second sum.
///
/// It is a TABLE and not a number, because the octave count is read
/// off the town's own size: one octave is a single value noise and
/// spreads 0.184 against six octaves' 0.080, over twice as wide, so a
/// hamlet stretched by a city's spread would be bitten everywhere it
/// could be bitten at all.
///
/// And every row is NARROWER than `fbm3`'s at the same count (0.080
/// against 0.108 at six), which is not what a rougher field sounds
/// like and is what the normalisation does: an octave kept at 0.80
/// rather than halved means more nearly equal INDEPENDENT terms in one
/// average, so the sum piles up harder. What makes a field rough is
/// the SHARE of its variance the fine octaves carry, and the number
/// that reads that is its level set's dimension and never its spread.
/// `town::fractal::measure_the_grains_own_spread` re-measures every
/// row, and the count is the same 8 `grain_octaves` clamps to.
pub(crate) const GRAIN_MEAN: f64 = 0.500;
pub(crate) const GRAIN_SPREAD: [f64; 8] = [0.184, 0.132, 0.109, 0.095, 0.087, 0.080, 0.076, 0.073];
/// Which SLICE of the noise lattice a town's plan is cut from, and what
/// its seed is turned by first. Neither is a tuning knob: they are the
/// two numbers that keep a town's grain off the lattice planes, where
/// value noise is its own interpolant and a plan would come out with
/// straight edges through it.
pub(crate) const GRAIN_SLICE: f64 = 7.5;
const GRAIN_SEED: u32 = 0x6A09;

/// How fast that outline can MOVE as you walk round it, metres of edge
/// per metre of arc.
///
/// It is the one thing a disc does not have and it is the price of an
/// outline that is not one: a fade over a fixed width past a boundary
/// that is not radial is steeper than the same fade past one that is,
/// by `hypot(1, WOBBLE)`. `field::site_skirt` widens a town's skirt by
/// exactly that, so the planet's own slope bound is untouched and no
/// chunk is ruled on ground the levelling reaches into.
///
/// MEASURED over every bearing of a thousand towns of every size,
/// stretch and seed rather than reasoned about
/// (`the_outline_never_moves_faster_than_the_bound`): the worst is
/// 1.571, and this is 2, which is the headroom a bound on noise wants.
/// Reasoned from the lobes' own gradient instead it is 4.65, which is a
/// fifty metre apron round every town for a swing no town ever takes.
pub(crate) const WOBBLE: f64 = 2.0;

/// Where a town stops being one thing and starts being another, in the
/// same demand the outline is cut from: over `CORE_AT` is downtown and
/// over `TOWN_AT` is the town proper, and everything out to nought is
/// SUBURB.
///
/// They are set from the MIX rather than chosen, which is the owner's
/// own ask: about a quarter of a town's buildings tall and three
/// quarters small houses. The shares are not the thresholds, because a
/// suburb leaves `SUBURB_FILL` of its blocks empty and downtown leaves
/// 0.15: at 0.66 and 0.38 the town proper and its core covered 38.4% of
/// the demand disc and carried 49.1% of the BUILDINGS, which is half a
/// city of offices. SWEPT rather than solved, because the demand is
/// stretched and lobed and its area shares are not `(1 - t)^2`.
///
/// It was 0.54 while a town was a smooth oval thinned by a flat coin
/// toss a block, and the GRAIN moved it: the bite takes ground off the
/// fringe, which is where the one storey houses are, so the same
/// threshold reads a higher share of tall buildings. RE-SWEPT rather
/// than left: 0.50 gives 29.9%, 0.52 gives 27.3 and 28.2, 0.54 gives
/// 25.5 and 26.4, and 0.55 gives **25.5% on a 170 m town and 25.5% on
/// a 537 m one**, which is the owner's quarter at both ends of the
/// size law and to a tenth of a per cent of itself, which is what says
/// the mix is a fact about the demand field and not about the radius.
/// `a_town_is_a_quarter_towers_and_three_quarters_houses` holds it.
pub(crate) const CORE_AT: f64 = 0.77;
pub(crate) const TOWN_AT: f64 = 0.55;
/// How much TOWN there is at a point of a town's own grid, metres east
/// and north of its middle: one at the very middle, nought at the edge,
/// and negative outside it.
///
/// It is the one number a town's shape is made of, and everything else is
/// read off it: where the town STOPS, which of the three zones a block is
/// in, and how tall what stands there is. One number with three
/// consequences rather than three rules that have to agree.
///
/// The LOBES are why an outline is not a circle. Two octaves of the
/// core's own value noise on the block's own place, a couple of lobes
/// across the town, pushing the edge in and out by `REACH` of the
/// nominal radius: a town is what grew where growing was easy, so it runs
/// a long way down one side and stops short on another. Circles at a
/// hundred and sixty sites read as one stamp used a hundred and sixty
/// times, which is exactly what the owner was looking at.
pub(crate) fn demand(x: f64, z: f64, radius: f64, along: DVec2, seed: u32) -> f64 {
    demand_bitten(x, z, radius, along, seed, GRAIN)
}

/// The same demand with the grain's own strength handed in, so a sweep
/// can ask what a town would be at another one and at NOUGHT, which is
/// the smooth oval the grain replaces. One implementation and a
/// parameter, which is this file's rule about a second caller that
/// wants a variation.
pub(crate) fn demand_bitten(
    x: f64,
    z: f64,
    radius: f64,
    along: DVec2,
    seed: u32,
    bite: f64,
) -> f64 {
    let p = DVec2::new(x, z);
    let d = p.length();
    // Dead on the middle, and never a NaN out of a zero length divide.
    if d <= 0.0 || !d.is_finite() {
        return 1.0;
    }
    let want = 1.0 - d / edge(p / d, radius, along, seed);
    want - bite * (1.0 - want).clamp(0.0, 1.0) * grain(x, z, radius, seed)
}

/// How hard the grain is biting at a point, nought to `GRAIN`.
///
/// It is the same expression `demand_bitten` subtracts, handed out so
/// that what FRAYS a town's edge and what THINS the ground inside it
/// are one number rather than two that have to agree. A town used to
/// carry two independent reasons for a block to be empty, the demand
/// and a flat coin toss a block, and only one of them had any
/// structure: from the air that reads as a fractal outline round a
/// field of static. `town::plot` reads this.
pub(crate) fn bite(x: f64, z: f64, radius: f64, along: DVec2, seed: u32) -> f64 {
    let p = DVec2::new(x, z);
    let d = p.length();
    if d <= 0.0 || !d.is_finite() {
        return 0.0;
    }
    let want = 1.0 - d / edge(p / d, radius, along, seed);
    GRAIN * (1.0 - want).clamp(0.0, 1.0) * grain(x, z, radius, seed)
}

/// How much of a block's demand the grain takes, nought to one.
///
/// The `fbm3` is asked on a SLICE of its own lattice (a fixed y), so a
/// town's plan is a two dimensional field and two towns differ because
/// their seeds do. The lump is a share of the town's own radius, so a
/// village and a city are ragged at the same share of themselves rather
/// than the city being ragged and the village being a blob.
pub(crate) fn grain(x: f64, z: f64, radius: f64, seed: u32) -> f64 {
    let lump = (radius * GRAIN_LUMP).max(f64::MIN_POSITIVE);
    let p = DVec3::new(x / lump, GRAIN_SLICE, z / lump);
    let sum = fbm3_rough(p, seed ^ GRAIN_SEED, grain_octaves(radius), GRAIN_PERSIST);
    let spread = GRAIN_SPREAD[grain_octaves(radius) as usize - 1];
    let n = (sum - GRAIN_MEAN) / spread;
    // Nought at the mean and the full bite two deviations over it,
    // which is the one town in fifty that has a hole right through.
    (n * 0.5).clamp(0.0, 1.0)
}

/// How far a town reaches along a BEARING out of its own middle, metres.
///
/// The stretch and the lobes, in one function, because this is the one
/// place a town's edge is decided: `demand` reads it to say where the
/// town stops and `Site::level_r` reads it to say how far the ground is
/// levelled, and a town whose plateau and whose lots came off two
/// answers is the disc the owner was looking at.
///
/// The lobes are read at the NOMINAL edge rather than at the query
/// point, which is what makes this a function of the bearing alone: a
/// ray out of the middle of a town then crosses the outline exactly
/// once, so there is an edge to level up to rather than a level set
/// somebody has to root find.
pub(crate) fn edge(b: DVec2, radius: f64, along: DVec2, seed: u32) -> f64 {
    // Stretched ALONG the shore and squeezed across it, at the same area:
    // a coastal town runs up and down its own beach, because the water
    // stops it one way and the hill behind it stops it the other. A town
    // with no slope under it gets no stretch and stays round.
    let (u, v) = if along.length_squared() < 0.5 {
        (b.x, b.y)
    } else {
        (b.x * along.x + b.y * along.y, b.y * along.x - b.x * along.y)
    };
    let s = (u / STRETCH).hypot(v * STRETCH);
    let nominal = radius / s.max(f64::MIN_POSITIVE);
    let at = b * nominal;
    let p = DVec3::new(at.x / (radius * LOBE), 3.5, at.y / (radius * LOBE));
    let lobe = (noise3(p, seed) - 0.5) + (noise3(p * 2.7, seed ^ 0x5B2D) - 0.5) * 0.5;
    nominal * (1.0 + lobe * REACH)
}

/// What a block of a town's grid IS. The zones are the demand's own
/// thresholds, so a town's outline, its density and its skyline are three
/// readings of one field and cannot disagree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Zone {
    /// Not town at all: country, and no block here.
    Away,
    /// Houses with space between them, one storey, set well back.
    Suburb,
    /// The town proper: streets of two and three storey buildings.
    Town,
    /// Downtown: the towers.
    Core,
}

impl Zone {
    pub(crate) fn of(want: f64) -> Zone {
        if want > CORE_AT {
            Zone::Core
        } else if want > TOWN_AT {
            Zone::Town
        } else if want > 0.0 {
            Zone::Suburb
        } else {
            Zone::Away
        }
    }
}
