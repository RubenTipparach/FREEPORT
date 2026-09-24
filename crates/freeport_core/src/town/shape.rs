//! How BIG a town is along a bearing, how much TOWN there is at a point
//! of its grid, and which of the three zones that point is in.
//!
//! One number and three readings of it: `edge` says where a town stops,
//! `demand` says how much town there is on the way there, and `Zone`
//! cuts that at its own thresholds. A town's outline, its density and
//! its skyline cannot disagree, because there is nothing for them to
//! disagree about.

use super::LOT;
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

/// The FRONT: how much of a town's own demand the grain decides, as
/// the demand at which HALF the ground is built.
///
/// **A CITY IS NOT AN OVAL, and it was still one.** The grain used to
/// be subtracted only where the noise stood OVER its own mean, scaled
/// by `1 - want`, so a block just inside the outline was built whenever
/// the noise stood under its mean: half of them, right up to the lobed
/// ellipse `edge` draws. Measured on a 1,611 m city, the outer twentieth
/// of the outline was **60% built**, so what an eye traced round a town
/// from the air was the ellipse and the grain only speckled the inside
/// of it. The owner read it off the map as an oval, which it was.
///
/// **So a town is a GRADIENT PERCOLATION**, which is what physics calls
/// a front between a filled region and an empty one (Sapoval, Rosso and
/// Gouyet's diffusion front) and what Makse, Havlin and Stanley used in
/// 1995 to model real urban growth: ground is built where a CORRELATED
/// noise stands under a threshold that falls steadily from the middle of
/// town to its edge. Where the threshold crosses the noise's own middle
/// half the ground is built, and that is where a city frays into
/// fingers, bays and outlying pieces; its hull is a fractal because it
/// is a level set of a fractal field, and the ellipse is never what
/// stops it, because by the time the outline arrives the threshold has
/// fallen `FADE` deviations under the noise and almost nothing is left.
///
/// The threshold is linear in the demand, `want * FADE / FRONT - FADE`
/// deviations, and the demand is the outline's own scaled by how far
/// under it the grain stands (`demand_at`), so a town's outline, its
/// density and its skyline are still three readings of ONE number.
pub(crate) const FRONT: f64 = 0.35;
/// How many of the grain's own deviations the threshold stands UNDER
/// its mean at a town's outline, which is how little is built there:
/// at 2.3 it is one cell in a hundred, so the lobed ellipse is a bound
/// nothing is drawn against rather than a line anybody can see.
pub(crate) const FADE: f64 = 2.6;
/// The most the grain may read, in deviations, which is what makes a
/// SOLID MIDDLE arithmetic rather than likely: nothing is taken out
/// where the demand stands over `FRONT * (FADE + CAP) / FADE`, which is
/// the inner quarter of every bearing, so a hamlet cannot be shredded to
/// nothing and a city cannot grow a hole through its own downtown.
///
/// **And the grain only ever takes demand AWAY**, which is what keeps
/// all of this affordable. `Site::level_r` is `edge(bearing) + APRON`
/// and the ground a town grades follows it; a front that could ADD
/// demand would put blocks outside that ground, which is the buried
/// suburb this file measured at 1.13 m once already. Taking away leaves
/// the built set a SUBSET of the star shaped region the outline bounds,
/// so the levelling, the grade, `field::site_skirt`, `WOBBLE` and the
/// planet's own slope bound are exactly what they were, not one chunk
/// is contoured differently, and the atlas needs no re-bake: what it
/// stores is where a town STANDS and `lay` derives the grid again.
pub(crate) const CAP: f64 = 3.0;
/// How far inside the front a point stands, in deviations, before it
/// reads the outline's whole demand: one deviation, so downtown's zones
/// are the smooth ones the outline gives and only the band along the
/// front, where the noise decides built from open, shades toward
/// suburb.
pub(crate) const EASE: f64 = 1.0;
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

/// The FINEST octave is one LOT and no finer, which is why the count
/// is read off the town's own size rather than written down: past the
/// lot grid an octave is noise the plan cannot express, and a hamlet
/// of four lots has ONE scale and should have one lump. It is this
/// file's own "every term is capped by the planet's octave count" rule
/// arriving at a city.
///
/// The LOT and not the block, because a block is four lots a side now
/// and the plan still decides lot by lot on a block's rim
/// (`plot::fill` thins the ring by this same bite): keyed to the block
/// a 537 m town fell from six octaves to four and its outline measured
/// 1.034 against the oval's 0.987, which is a picture nobody could
/// tell from the oval.
pub(crate) fn grain_octaves(radius: f64) -> u32 {
    let span = (radius * GRAIN_LUMP / LOT).max(1.0);
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
///
/// And the FRONT moved it again, for the same reason at a larger
/// scale: it takes the whole fringe, which is one storey houses, so at
/// 0.55 a city read 41.8% tall. Re-swept on the front with `CORE_AT`
/// moved with it: 0.59 gives 35.5%, 0.64 gives 29.6 and 0.70 gives 21.0,
/// and 0.67 gives **26.2% on a 537 m city and 25.4% on a 1,611 m one**.
/// `a_city_has_towers_a_town_has_shops_and_a_village_has_houses` holds
/// the band at both sizes.
pub(crate) const CORE_AT: f64 = 0.85;
pub(crate) const TOWN_AT: f64 = 0.67;
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
    demand_at(x, z, radius, along, seed, FRONT, FADE)
}

/// The same demand with the front handed in, so a sweep can ask what a
/// town would be at another one, and at a `front` of NOUGHT, which is
/// the bare lobed oval with no grain at all. One implementation and a
/// parameter, which is this file's rule about a second caller that
/// wants a variation.
///
/// How far a point stands INSIDE the front is its MARGIN, in the
/// grain's own deviations, and the demand is the outline's own `want`
/// scaled by that margin up to `EASE` and no further. Where a town is
/// built and where it is not is the margin's sign alone; what the
/// scaling does is keep the noise off a town's ZONES wherever it stands
/// comfortably inside its front. The first cut subtracted the grain
/// everywhere, and a block facing the square in the middle of a city
/// read as SUBURB where the noise stood high and carried a six storey
/// office on it: downtown is downtown, and only the band along the front
/// shades toward suburb, which is what the edge of a real town does.
pub(crate) fn demand_at(
    x: f64,
    z: f64,
    radius: f64,
    along: DVec2,
    seed: u32,
    front: f64,
    fade: f64,
) -> f64 {
    let p = DVec2::new(x, z);
    let d = p.length();
    // Dead on the middle, and never a NaN out of a zero length divide.
    if d <= 0.0 || !d.is_finite() {
        return 1.0;
    }
    let want = 1.0 - d / edge(p / d, radius, along, seed);
    if front <= 0.0 || fade <= 0.0 {
        return want;
    }
    // Built where the grain stands under `want * fade / front - fade`
    // deviations: the margin is how far under, capped so the middle is
    // solid by arithmetic. The `min` keeps the demand under what the
    // outline gives on both sides of it, so nothing is ever built past
    // the outline whatever the margin says.
    let margin = want * fade / front - fade - grain(x, z, radius, seed).min(CAP);
    want.min(want * (margin / EASE).min(1.0))
}

/// Where the grain stands at a point of a town's grid, in its own
/// DEVIATIONS: nought at its mean, and a town is built where this is
/// under the threshold `demand_at` draws.
///
/// The `fbm3` is asked on a SLICE of its own lattice (a fixed y), so a
/// town's plan is a two dimensional field and two towns differ because
/// their seeds do. The lump is a share of the town's own radius, so a
/// village and a city are ragged at the same share of themselves rather
/// than the city being ragged and the village being a blob. It is
/// stretched by the grain's own MEASURED spread (`GRAIN_SPREAD`), so a
/// deviation means a deviation at every octave count a town can have.
pub(crate) fn grain(x: f64, z: f64, radius: f64, seed: u32) -> f64 {
    let lump = (radius * GRAIN_LUMP).max(f64::MIN_POSITIVE);
    let p = DVec3::new(x / lump, GRAIN_SLICE, z / lump);
    let octaves = grain_octaves(radius);
    let sum = fbm3_rough(p, seed ^ GRAIN_SEED, octaves, GRAIN_PERSIST);
    (sum - GRAIN_MEAN) / GRAIN_SPREAD[octaves as usize - 1]
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
