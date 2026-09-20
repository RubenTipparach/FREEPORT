//! What a town or a road LEVELS: the patch of planet its ground is cut
//! (and, on a road, filled) to, and the outline a town's own is shaped
//! by.
//!
//! It is out of `town.rs` because a road levels one too, and because
//! `town.rs` was over this project's nine hundred lines the day a site
//! stopped being a circle.

use super::{edge, frame_at, APRON, OUTLINE, WOBBLE};
use glam::{DVec2, DVec3};

/// A patch of planet LEVELLED: a town's own ground, or the corridor a
/// road is cut along.
///
/// It is an ARC and a disc is the case where the two ends are the same
/// direction, which is what a town is. A road's corridor is a chain of
/// short arcs whose ends meet, each cut to the ground its own two ends
/// stand on, so the corridor follows the country instead of being a
/// straight ramp through it: measured on this body's own roads, a
/// corridor levelled on the atlas's ten kilometre waypoints cuts a
/// median of 45 m and up to 830 m into the ground, and one cut every
/// 341 m cuts a median of 1.2 m and a 99th of 6.2, which is a cutting
/// and an embankment rather than a canyon.
///
/// One TYPE rather than a town's disc beside a road's capsule, because
/// everything that reads a site (`field::site_band`, `site_weight`,
/// `surface_blend`, `local_solid`, `Planet::around` and the slope bound
/// they all rest on) would otherwise need saying twice, and two of them
/// A town's own EDGE, as a function of the bearing out of its middle.
///
/// A town is not a circle (`demand`), so the ground it levels is not one
/// either. This is what `Site::level_r` and `demand` both read, so how
/// far a town REACHES and how far its ground is LEVELLED are one answer
/// rather than two that have to agree.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outline {
    /// The town's NOMINAL radius, metres: what `edge` is measured in.
    pub radius: f64,
    /// Which way the town grew, east and north in its own frame.
    pub along: DVec2,
    /// The town's own seed, which its lobes are drawn off.
    pub seed: u32,
}

/// are subtle enough that one copy would be wrong.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Site {
    /// One end of the arc, and a town's own middle.
    pub dir: DVec3,
    /// The level at `dir`, metres over the mean radius.
    pub h: f64,
    /// The other end, equal to `dir` on a town.
    pub to: DVec3,
    /// The level there.
    pub to_h: f64,
    /// How far either side of the arc the levelling reaches, metres.
    pub r: f64,
    /// Whether it may BUILD GROUND UP as well as cut it away.
    ///
    /// A town may not, and that is one of this game's own rules: its
    /// level is the lowest its survey found, so a site that filled would
    /// be a city standing on a pedestal with its apron hanging out over
    /// the valley, which is what the owner read off a picture.
    ///
    /// A ROAD may, and that is what a road IS. Its level is the ground
    /// at its own stations, so the ramp between two of them runs over
    /// every hollow between: measured on this body, tarmac laid on a
    /// corridor that could only cut floated 18.6 m over the ground in
    /// the worst place. An embankment is the other half of a cutting and
    /// no road is built without both.
    pub fills: bool,
    /// The town's own outline, when this site is a town's ground.
    ///
    /// Nought on a road's corridor, which IS a capsule and levels one.
    /// A TOWN is not a disc, and the first cut of this levelled one
    /// anyway: `radius * OUTLINE + APRON` right round, which along the
    /// squeezed axis is more than twice as far as the town ever reaches.
    /// The owner read it off the climb as big flat discs, and it was:
    /// two hundred metres of bare levelled plateau round a town three
    /// hundred and seventy across.
    pub outline: Option<Outline>,
}

impl Site {
    /// A round site: a town's own ground, level right across it.
    pub fn round(dir: DVec3, h: f64, r: f64) -> Site {
        Site {
            dir,
            h,
            to: dir,
            to_h: h,
            r,
            fills: false,
            outline: None,
        }
    }

    /// A TOWN's ground: level right across the town's own outline and an
    /// apron past that, and falling to the relief over a skirt.
    ///
    /// `r` is the FURTHEST that outline can reach, so every bound built
    /// off it (`field::site_band`, `Planet::around`, the slope bound)
    /// stays the upper bound it always was and only the fade moved.
    pub fn town(dir: DVec3, h: f64, radius: f64, along: DVec2, seed: u32) -> Site {
        Site {
            dir,
            h,
            to: dir,
            to_h: h,
            r: radius * OUTLINE + APRON,
            fills: false,
            outline: Some(Outline {
                radius,
                along,
                seed,
            }),
        }
    }

    /// An ARC: a corridor `r` metres either side of the great circle
    /// between two directions, its level ramped from one end to the
    /// other. A road's own piece.
    pub fn arc(from: (DVec3, f64), to: (DVec3, f64), r: f64) -> Site {
        Site {
            dir: from.0,
            h: from.1,
            to: to.0,
            to_h: to.1,
            r,
            fills: true,
            outline: None,
        }
    }

    /// Where along the arc a direction falls, nought at `dir` and one at
    /// `to`, clamped to the segment. Nought for a round site.
    ///
    /// The point is projected onto the great circle through the two ends
    /// and then measured from `dir` along it, which is the only honest
    /// "how far along" on a sphere: a chord through the middle of the
    /// planet is not a distance along the ground.
    pub fn along(&self, dir: DVec3) -> f64 {
        let pole = self.dir.cross(self.to);
        let sweep = self.dir.angle_between(self.to);
        if sweep < 1e-12 || pole.length_squared() < 1e-24 {
            return 0.0;
        }
        let pole = pole.normalize();
        let on = (dir - pole * dir.dot(pole)).normalize_or(self.dir);
        // The tangent at `dir` toward `to`, so the angle is SIGNED and a
        // point behind the start clamps to the start rather than to the
        // far end.
        let ahead = (self.to - self.dir * self.dir.dot(self.to)).normalize_or_zero();
        (on.dot(ahead).atan2(on.dot(self.dir)) / sweep).clamp(0.0, 1.0)
    }

    /// The nearest point of the arc to a direction, and the level there.
    pub fn nearest(&self, dir: DVec3) -> (DVec3, f64) {
        let t = self.along(dir);
        if t <= 0.0 {
            return (self.dir, self.h);
        }
        if t >= 1.0 {
            return (self.to, self.to_h);
        }
        let sweep = self.dir.angle_between(self.to);
        let (s, c) = (sweep * t).sin_cos();
        let ahead = (self.to - self.dir * self.dir.dot(self.to)).normalize_or_zero();
        (
            (self.dir * c + ahead * s).normalize_or(self.dir),
            self.h + (self.to_h - self.h) * t,
        )
    }

    /// How far the arc reaches off the axis, as a CHORD: half its own
    /// sweep. A round site reaches nought, so a query window built from
    /// this is a point for a town and a segment for a road.
    pub fn reach(&self) -> f64 {
        (self.dir - self.to).length() * 0.5
    }

    /// How steeply its own level ramps along it, as a slope. A town's is
    /// nought; a road's is the grade it was routed at.
    pub fn grade(&self, radius: f64) -> f64 {
        let run = self.dir.angle_between(self.to) * radius;
        if run <= 0.0 {
            return 0.0;
        }
        (self.to_h - self.h).abs() / run
    }

    /// How far this site levels the ground at a direction, metres: its
    /// own `r` on a road's corridor, and the TOWN's own edge plus an
    /// apron on a town.
    ///
    /// Only the BEARING out of the site's middle is read off `dir`, so
    /// the chord this projects on is a direction and never a distance:
    /// how far away the query point is, the caller already measured
    /// along the ground.
    pub fn level_r(&self, dir: DVec3) -> f64 {
        let Some(o) = self.outline else {
            return self.r;
        };
        let (east, north) = frame_at(self.dir);
        let b = DVec2::new(dir.dot(east), dir.dot(north));
        let len = b.length();
        // Dead on the town's own middle, where there is no bearing to
        // read an outline along: its widest is the honest answer.
        if len <= 0.0 || !len.is_finite() {
            return self.r;
        }
        (edge(b / len, o.radius, o.along, o.seed) + APRON).min(self.r)
    }

    /// The LEAST this site levels anywhere within `arc` metres of a
    /// direction: what a box spanning that much of the ground can be
    /// ruled against.
    ///
    /// A road's corridor is a capsule and levels `r` all the way round,
    /// so the arc buys it nothing. A town's own edge moves at up to
    /// `WOBBLE` a metre, and a bound that ignored that would rule a box
    /// air over ground the town never levelled, which is a hole.
    pub fn level_floor(&self, dir: DVec3, arc: f64) -> f64 {
        match self.outline {
            None => self.r,
            Some(_) => self.level_r(dir) - WOBBLE * arc.max(0.0),
        }
    }
}
