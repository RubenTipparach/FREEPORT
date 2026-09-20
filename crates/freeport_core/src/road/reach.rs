//! WHERE a road is: which of its points the corridor may be cut along,
//! which carry tarmac, which are lit, and the sites the corridor levels.
//!
//! Out of `road.rs` because that file was over this project's nine
//! hundred lines the day the tarmac ran on into a town.

use super::{centreline, Road, CORRIDOR};
use glam::DVec3;

/// The SITES a road's corridor levels: one arc a piece, each cut to the
/// ground its own two ends stand on.
///
/// `skip` is how near a town's own centre the corridor stops, and it is
/// the town's own levelling AT THAT BEARING plus its skirt
/// (`Site::level_r` and `field::site_skirt`): inside that the town's
/// site wins at full weight and answers the town's level, so a corridor
/// that reached further would lay its tarmac at the road's level over
/// ground held at the town's, and one that stopped short of it would
/// end in a field. Measured, tarmac stood 0.10 m off its own lift there;
/// past the band it is 0.003. Consecutive arcs share an end and its level by
/// construction, which is what lets the field's slope bound assume TWO
/// overlapping skirts rather than however many roads meet at a hub.
pub fn corridor(
    road: &Road,
    run: &[f64],
    radius: f64,
    towns: &crate::field::Sites,
) -> Vec<crate::town::Site> {
    let line = centreline(road, radius);
    if run.len() != line.len() {
        return Vec::new();
    }
    let open = open(&line, radius, towns);
    line.windows(2)
        .zip(run.windows(2))
        .zip(open.windows(2))
        .filter(|(_, o)| o[0] && o[1])
        .map(|((d, h), _)| crate::town::Site::arc((d[0], h[0]), (d[1], h[1]), CORRIDOR))
        .collect()
}

/// Which points of a road's centreline are OUT of every town's own
/// levelling: one per point of `centreline`.
///
/// EVERY town and not only the two a road joins. `road::waysides` grows
/// a village wherever a road has run a day's cart since the last one, so
/// a road passes THROUGH settlements as well as ending at them, and a
/// town's disc levels its ground to the town's level while a corridor
/// ramps to the road's. Where the two overlap the field answers whichever
/// it reaches first and the tarmac stands on the other: measured, 0.10 m
/// off its own lift, which is a road stepping in and out of the ground
/// at every village on it.
///
/// It is measured against the town's whole SITE BAND rather than its
/// outline, because inside the band the town's site is what the field
/// answers; and through `field::Sites`, the latitude index, because a
/// body carries 1,084 settlements and a road 600 points, and the product
/// of those two is not a loop worth writing.
pub fn open(line: &[DVec3], radius: f64, towns: &crate::field::Sites) -> Vec<bool> {
    let window = towns.window(radius);
    line.iter()
        .map(|d| {
            !towns.near(*d, window).any(|site| {
                // How far the town levels THIS WAY, which is its own
                // outline and not the disc round it: measured against
                // the widest a town reaches, a road out along the
                // SQUEEZED axis stopped three hundred metres short of
                // ground the town had never levelled, so the tarmac
                // ended in a field.
                let outer = site.level_r(*d) + crate::field::site_skirt(site);
                (*d - site.nearest(*d).0).length() * radius <= outer
            })
        })
        .collect()
}

// **What was tried and taken OUT: running the tarmac in on a MASK.**
// `paved` was `open` plus a `clear` test against the town's own pieces,
// and `mouths` bisected that test along the one piece a town's paving
// ends inside, so the highway could carry on past `open` and stop half
// a metre from the nearest street. Three things were wrong with it and
// only the third is fatal.
//
// It reported a gap that was its own constant, because `clear` STOPS
// the tarmac `MEET` from the nearest piece and the harness then
// measured the distance to the nearest piece. It stopped at whatever
// piece happened to be nearest in ANY direction, which from the ground
// is a square ended road beside a street it never joins. And it FLOATS:
// inside a town's levelling the ground is the town's flat plateau while
// the road's baked profile is `smooth`'s own raise only answer, which
// stands above it, so the moment those pieces were actually drawn
// `the_tarmac_lands_on_the_ground_its_corridor_levelled` measured
// **1.790 m** of daylight under them.
//
// A mask cannot fix that, because the two heights are genuinely
// different and neither is wrong. What joins them is GEOMETRY that
// reads the ground between, which is `slip` below.

/// How near a settlement a stretch of road has to pass to be LIT,
/// metres.
///
/// The owner's own rule: lights along the stretches near a city and
/// none out in the country, which is what a road actually is. A
/// kilometre and a half is the approach to a town rather than the town
/// itself, since a town's own site band ends a couple of hundred metres
/// out and its streets are lit from there in.
pub const LIT_NEAR: f64 = 1_500.0;

/// Which points of a road's centreline are near enough a settlement to
/// carry lamps: one per point of `centreline`.
pub fn lit(line: &[DVec3], radius: f64, towns: &crate::field::Sites) -> Vec<bool> {
    let window = towns.window(radius) + LIT_NEAR / radius;
    line.iter()
        .map(|d| {
            towns
                .near(*d, window)
                .any(|site| (*d - site.nearest(*d).0).length() * radius <= LIT_NEAR)
        })
        .collect()
}

/// How long one piece of a SLIP is, metres. Three, because a slip is a
/// CURVE a few tens of metres long and a piece has to be short enough
/// that its chord does not cut the bend: at 3 m a 15 m radius turn is
/// out by 7 cm, which is under the tarmac's own lift.
const SLIP_PIECE: f64 = 3.0;

/// How far the two tangents reach, as a share of the gap they span. A
/// little over a half is the plain Hermite that leaves both ends along
/// their own direction without looping.
const EASE: f64 = 0.55;

/// The height a slip's tarmac RIDES at over one point of the ground,
/// metres over the mean radius, in the units `ribbon::stretch` reads
/// (it adds its own `LIFT` back to everything it is handed).
///
/// **The ground is the one the MESHER draws**, a sphere trace of the
/// field for its first crossing, which is what `road::survey` reads for
/// the highway's own profile too. `Planet::surface` is the analytic
/// relief with the sites applied and it is cheaper, which is what this
/// read for a commit; what it leaves out is the field's VOLUMETRIC
/// term, and outside a town's levelling that term is the whole
/// disagreement. Measured along the port's own slip, the drawn ground
/// stood **1.045 m over the analytic surface at one point and 0.11 m
/// under it three metres away**: a swing no fixed embankment covers,
/// and the 0.39 m of tarmac buried under the hill it was laid on was
/// that swing less the 0.65 m of `ribbon::LIFT + EMBANK` that was
/// paying to hide it.
///
/// **And the march is cheap because it starts where the answer is.**
/// Two things made it 74.6 s of startup on this body. The slip is
/// spliced AFTER the corridors are installed, so an unfiltered trace
/// walks all 646,000 levelled arcs at every step of every sample:
/// `local` is `Planet::around` at the slip's own mouth, filtered ONCE
/// for the whole curve, which is this file's oldest performance rule.
/// And `town::surface_radius` starts at the top of the relief band,
/// which here is sixteen kilometres of air at a floor of half a metre a
/// step: started at the analytic surface plus `Planet::overhang`, which
/// is twice the most the volumetric term can lift a surface, it is
/// **0.3 s for the same answer**.
///
/// **The LIFT is a road's outside the town's levelling and a street's
/// inside it**, blended by `Planet::site_weight`, which is the one
/// function that says where a town's plateau is: the lift cannot then
/// disagree with the ground about it, because the two are one number.
/// No embankment, because there is nothing left for one to hide. Written
/// as a share of the SLIP it was wrong twice over, since the crossing a
/// slip ends on stands well inside the levelling; written as a band off
/// `Site::level_r` it was wrong by the half of the skirt that fades
/// INSIDE that boundary, and buried the tarmac the same 0.39 m in the
/// same place.
fn riding(local: &crate::field::Planet, site: &crate::town::Site, dir: DVec3, radius: f64) -> f64 {
    let top = radius + local.surface(dir).0 + local.overhang;
    let ground = crate::town::surface_radius_from(local, dir, top) - radius;
    let ease = 1.0 - local.site_weight(site, dir);
    let lift = crate::town::LIFT + (super::ribbon::LIFT - crate::town::LIFT) * ease;
    ground + lift - super::ribbon::LIFT
}

/// The piece of a town's own paving a slip is laid to REACH: the
/// nearest CROSSING on ground the town has levelled.
///
/// A CROSSING and never a run, because a crossing is a node where
/// streets already meet and it owns its whole square (`town::paved`),
/// so a slip arriving at one merges into the grid; arriving at the
/// middle of a run would T bone a street at whatever angle the road
/// came in on and leave the corner bare.
///
/// And on LEVELLED ground, which is where a street is laid five
/// centimetres over a plane and stays there. A town's grid runs out
/// past its own site onto the skirt, where the blend ramps and the
/// volumetric term comes back: a slip ending there arrives at the one
/// part of the town's paving that is itself partly in the hill, and its
/// last piece is buried with it. Measured on the port, that moved the
/// slip's end from 158 m out to 129.
///
/// The two fallbacks are for the towns that have neither: any crossing
/// at all, and then any piece at all, which is what a one street hamlet
/// leaves.
fn crossing<'a>(
    town: &'a crate::town::Town,
    site: &crate::town::Site,
    p0: glam::DVec2,
    radius: f64,
) -> Option<&'a crate::town::Piece> {
    let near = |p: &&crate::town::Piece| (p.x - p0.x).hypot(p.z - p0.y);
    let level = |p: &&crate::town::Piece| {
        let dir = (town.dir * radius + town.east * p.x + town.north * p.z).normalize();
        dir.angle_between(town.dir) * radius <= site.level_r(dir)
    };
    let pick = |firm: bool| {
        town.pieces
            .iter()
            .filter(|p| !p.run() && (!firm || level(p)))
            .min_by(|a, b| near(a).total_cmp(&near(b)))
    };
    pick(true).or_else(|| pick(false)).or_else(|| {
        town.pieces
            .iter()
            .min_by(|a, b| near(a).total_cmp(&near(b)))
    })
}

/// The SLIP that joins a highway to a town's own streets: a curve from
/// the last tarmac the road lays to the nearest CROSSING the town paved,
/// with the ground under it read off the planet.
///
/// This is the thing the design file named as missing and the owner read
/// off a picture: a road arrives on an arbitrary bearing and a town's
/// streets run on its own grid, so there is nothing for the tarmac to
/// meet until one of them is laid toward the other. What the highway did
/// instead was STOP, `MEET` from the nearest piece of paving in whatever
/// direction that happened to be, which from the ground is a square
/// ended road in a field with the city beyond it.
///
/// **A CROSSING and never a run**, because a crossing is a node where
/// streets already meet and it owns its whole square (`town::paved`), so
/// a slip arriving at one merges into the grid; arriving at the middle
/// of a run would T bone a street at whatever angle the road came in on
/// and leave the corner bare.
///
/// **The height is the GROUND's and never a lerp.** The town's site has
/// already levelled its plateau and the road's corridor has already been
/// cut outside it, so the field's own surface between the two IS the
/// ramp from the road's grade down to the town's, and a slip that reads
/// it lands on it. That is the owner's "lower to meet the height of the
/// city", and it needs no second opinion about what that height is.
///
/// **And the LIFT tapers**, `ribbon::LIFT` at the highway to `town::LIFT`
/// at the street, which is the 10 cm lip this project's own design gave
/// as the reason not to run tarmac into a town. `ribbon::stretch` adds
/// its own `LIFT` to every height it is handed, so what comes back here
/// is the ground plus the DIFFERENCE and the two sum to the taper.
pub fn slip(
    planet: &crate::field::Planet,
    town: &crate::town::Town,
    mouth: DVec3,
    mouth_h: f64,
    along: DVec3,
    radius: f64,
) -> Vec<(DVec3, f64)> {
    let flat = |d: DVec3| {
        let here = (d - town.dir) * radius;
        glam::DVec2::new(here.dot(town.east), here.dot(town.north))
    };
    let p0 = flat(mouth);
    let site = crate::town::site_of(town);
    let Some(target) = crossing(town, &site, p0, radius) else {
        return Vec::new();
    };
    let p1 = glam::DVec2::new(target.x, target.z);
    let gap = (p1 - p0).length();
    if !gap.is_finite() || gap <= SLIP_PIECE {
        return Vec::new();
    }
    // In along the ROAD's own heading, out along the town's own GRID,
    // so the slip leaves the highway straight and arrives square to the
    // street it is joining rather than across it.
    let here = (along - mouth * along.dot(mouth)).normalize_or(town.east);
    let t0 = glam::DVec2::new(here.dot(town.east), here.dot(town.north)).normalize_or(p1 - p0);
    let axis = p1 - p0;
    let t1 = if axis.x.abs() >= axis.y.abs() {
        glam::DVec2::new(axis.x.signum(), 0.0)
    } else {
        glam::DVec2::new(0.0, axis.y.signum())
    };
    let (m0, m1) = (t0 * (gap * EASE), t1 * (gap * EASE));
    let steps = ((gap / SLIP_PIECE).ceil() as usize).max(2);
    // The sites near the WHOLE slip, filtered ONCE. A slip is eighty
    // metres of a body two thousand kilometres across, so the same
    // handful of sites covers every point of it, and the filter is what
    // makes the march affordable: asked per point on a body carrying
    // 646,000 levelled corridor arcs it took 62.3 s of startup against
    // 1.8, because every one of them walks a latitude band of some
    // seven hundred sites and tests each with an `atan2`. That is this
    // file's own oldest rule, which is that a survey along one
    // direction filters the body once and not once a sample.
    let local = planet.around(mouth, gap * 2.0 / radius + 1e-9);
    let out: Vec<(DVec3, f64)> = (0..=steps)
        .map(|k| {
            let t = k as f64 / steps as f64;
            let (t2, t3) = (t * t, t * t * t);
            // The plain cubic Hermite, which is the curve that leaves
            // and arrives along the two tangents it is given.
            let q = p0 * (2.0 * t3 - 3.0 * t2 + 1.0)
                + m0 * (t3 - 2.0 * t2 + t)
                + p1 * (-2.0 * t3 + 3.0 * t2)
                + m1 * (t3 - t2);
            let dir = (town.dir * radius + town.east * q.x + town.north * q.y).normalize();
            (dir, riding(&local, &site, dir, radius))
        })
        .collect();
    grade(out, mouth_h, radius)
}

/// A slip carried down from the HEIGHT the highway itself stands at,
/// without moving the end it lands on.
///
/// A slip reads the ground and the highway reads its own baked profile,
/// and the two do not meet: `road::smooth` raises a station to clear
/// every probe inside its own piece and then to hold the grade, so the
/// highway's last tarmac stands `EMBANK` to a couple of metres over the
/// ground at that same direction. Laid on the ground alone the slip
/// starts with that whole difference as a STEP, over one `SLIP_PIECE`
/// of three metres. Measured on this body, the steepest piece anywhere
/// was **-0.50 m over 0.015 m, a grade of 3355%**, and a slip's own
/// mouth was where the rest of them were.
///
/// So the step is TAPERED OUT along the slip: the mouth is the
/// highway's height exactly, the far end is the town's street exactly,
/// and what is added between is one straight ramp of `d / length`. Both
/// ends are what they have to be, because a step at the mouth is a
/// kerb across the highway and a step at the crossing is a kerb across
/// the street, and neither is a thing a car drives over.
///
/// **What it does NOT do is hold the slip inside the highway's own
/// grade, and that is named rather than hidden.** The first cut
/// enveloped the profile from the mouth at `STEEPEST` and let the far
/// end fall where it fell: on the rough test ball that left a slip
/// standing **2.39 m over the crossing it was laid to reach**, which is
/// a ramp ending in the air over a street. The ground between a town's
/// own skirt and its plateau falls at whatever the town's cut makes it,
/// 12% on that fixture, and no profile that lands on both ends is
/// inside 7% when the ground between them is not. A slip is as steep as
/// the apron it is laid on, and what would fix that is the TOWN's
/// skirt rather than the road's.
fn grade(mut out: Vec<(DVec3, f64)>, mouth_h: f64, radius: f64) -> Vec<(DVec3, f64)> {
    if out.len() < 2 {
        return out;
    }
    // Against the arc each step ACTUALLY walks and never the nominal
    // one: a Hermite is longer than the chord its steps were counted
    // off, so `gap / steps` is more ground than a step covers and a
    // ramp written on it is steeper than it says. Measured on the body,
    // that was 3,331 of the 3,639 pieces left over the seven per cent.
    let arc: Vec<f64> = out
        .windows(2)
        .map(|w| w[0].0.angle_between(w[1].0) * radius)
        .collect();
    let total: f64 = arc.iter().sum();
    let step = mouth_h - out[0].1;
    if !total.is_finite() || total <= 0.0 || !step.is_finite() {
        return out;
    }
    let mut run = 0.0;
    for (k, point) in out.iter_mut().enumerate() {
        point.1 += step * (1.0 - run / total);
        run += arc.get(k).copied().unwrap_or(0.0);
    }
    out
}
