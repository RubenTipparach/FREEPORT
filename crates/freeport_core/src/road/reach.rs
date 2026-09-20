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

/// Which points of a road's centreline carry TARMAC: one per point of
/// `centreline`, and the other half of `open`.
///
/// `open` is where the road's own corridor may be CUT, which stops at
/// every town's levelling, because inside that the town's site answers
/// the ground and a corridor there would lay its tarmac at the road's
/// level over ground held at the town's. That is right for the FIELD and
/// wrong for the geometry: it left the highway ending 39 m short of the
/// port's own paving, in a field, which is the owner's "the main city I
/// spawn at doesn't appear to have a highway leaving out of it".
///
/// Inside a town's levelling the ground is the town's own LEVEL and it is
/// flat, and `survey` took its heights off the planet with the towns'
/// sites already in it, so the road's own profile there IS that level
/// and tarmac laid on it lands exactly. So the tarmac runs on in, and
/// what stops it is the town's OWN paving: a point is paved while it is
/// further than half a street from every piece the town laid. No
/// threshold to tune and no bearing to get right, and where the two meet
/// is where the streets actually are.
pub fn paved(line: &[DVec3], radius: f64, towns: &[crate::town::Town], open: &[bool]) -> Vec<bool> {
    line.iter()
        .enumerate()
        .map(|(i, d)| open.get(i).copied().unwrap_or(false) || clear(*d, radius, towns))
        .collect()
}

/// How close a road's tarmac may come to a town's own paving, metres.
///
/// Half a metre and not half a street: what this is for is keeping the
/// two surfaces from OVERLAPPING, since a road is lifted 0.15 m and a
/// street 0.05 and one laid over the other is a lip and a depth fight.
/// At half a street it was a 4 m ribbon of bare ground at the junction,
/// which is the gap the owner could see from the air.
const MEET: f64 = 0.5;

/// Whether a direction is somewhere a ROAD may lay tarmac inside a town:
/// clear of every piece that town paved, and FURTHER OUT than the
/// nearest of them.
///
/// Both halves, and the second is not obvious until the first is tried
/// on its own: a town's middle is often a plaza, so "clear of the
/// paving" is TRUE at the very centre of one and the highway was laid
/// straight through the town to its middle. A road approaches from
/// outside, so what it may pave is the ground outside the built up part
/// and never a gap inside it.
fn clear(d: DVec3, radius: f64, towns: &[crate::town::Town]) -> bool {
    let half = MEET;
    towns.iter().all(|t| {
        let out = t.dir.angle_between(d) * radius;
        // Only the town a point is actually inside can stop it.
        if out > t.radius * crate::town::OUTLINE + crate::town::STREET {
            return true;
        }
        let at = d * radius - t.dir * radius;
        let (x, z) = (at.dot(t.east), at.dot(t.north));
        let mut nearest = (f64::MAX, 0.0);
        for p in &t.pieces {
            let away = (p.x - x).hypot(p.z - z) - p.w.max(p.d) * 0.5;
            if away < nearest.0 {
                nearest = (away, p.x.hypot(p.z));
            }
        }
        nearest.0 > half && out > nearest.1
    })
}

/// How much of each PIECE carries tarmac, as a pair of parameters along
/// it: one per piece of `centreline`, so one shorter than the line.
///
/// A mask per station is not enough on its own, and the measurement is
/// why: the stations are `PIECE` (85 m) apart and the town's own paving
/// ends wherever it ends, so the first station clear of it lands up to a
/// piece further out. Measured on the port, the highway's first tarmac
/// stood 169 m out and the town's paving reached 130 m on that bearing:
/// a **39 m gap of bare levelled ground**, which is the whole of what
/// was between the highway and the city.
///
/// So a piece is laid from where it LEAVES the town's paving rather than
/// from its own station, found by bisecting `clear` along it. The rest
/// of the ribbon needs nothing: every part of it is already built from a
/// point and an across, so a piece that starts part way along is the
/// same arithmetic with a lerped end.
pub fn mouths(
    line: &[DVec3],
    radius: f64,
    towns: &[crate::town::Town],
    paved: &[bool],
) -> Vec<(f64, f64)> {
    const HALVINGS: usize = 12;
    (0..line.len().saturating_sub(1))
        .map(|k| {
            let (a, b) = (line[k], line[k + 1]);
            // Where along this piece the town's paving ends, bisected.
            // `lo` is the end that is ON the paving and `hi` the one off
            // it, so the answer is always between them.
            let edge = |from_a: bool| {
                let (mut lo, mut hi) = if from_a { (0.0, 1.0) } else { (1.0, 0.0) };
                for _ in 0..HALVINGS {
                    let mid = (lo + hi) * 0.5;
                    let at = (a + (b - a) * mid).normalize_or(a);
                    if clear(at, radius, towns) {
                        hi = mid;
                    } else {
                        lo = mid;
                    }
                }
                hi
            };
            match (paved[k], paved[k + 1]) {
                (true, true) => (0.0, 1.0),
                (true, false) => (0.0, edge(false)),
                (false, true) => (edge(true), 1.0),
                (false, false) => (0.0, 0.0),
            }
        })
        .collect()
}

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
