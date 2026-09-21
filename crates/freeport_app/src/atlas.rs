//! A body's atlas: where its cities stand and which roads join them,
//! worked out ONCE offline and read back at startup.
//!
//! Planning a planet is not cheap and it is not a function of anything a
//! player does: twenty thousand candidate directions, a march down to the
//! ground at each, and then a search over a hundred thousand waypoints for
//! the roads. That is seconds of work every single launch for an answer
//! that never changes, so it is baked. `--bake-atlas` writes one file a
//! body beside the other assets and the game reads them, which is the
//! owner's own ask: static generation loaded into the procedural planet at
//! run time.
//!
//! **What is stored is the PLAN and never the geometry.** A town's lots,
//! its streets, its buildings and its lamps are a pure function of where
//! it stands, how big it is and its seed (`town::lay`), so the file keeps
//! the placement and the game lays the grid out again in a few
//! milliseconds. Storing the triangles instead would be sixty million of
//! them for one body, would have to be reshipped whenever a building kind
//! changed, and would say nothing a reader could check.
//!
//! The format is JSON, like `planets.json` beside it, because a static
//! host and a text editor both read it and because a bake nobody can
//! inspect is a bake nobody can review.

use bevy::math::{DVec2, DVec3};
use freeport_core::field::Planet;
use freeport_core::road::{self, Road};
use freeport_core::town::{self, Town};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// One body's plan, as it sits on disk.
#[derive(Serialize, Deserialize)]
pub(crate) struct Atlas {
    /// The body this is the plan of, and the seed it was planned from.
    /// Both are checked on load, because an atlas baked from another
    /// planet would put cities in the sea and roads through mountains
    /// with nothing to say it had happened.
    pub body: String,
    pub seed: u32,
    pub radius: f64,
    /// The octave count the body was planned at. It is part of what body
    /// this is: `Shape::oct` caps every term by it, so a plan made at
    /// eighteen octaves describes ground that fourteen does not have, and
    /// a road routed over the one can cross water on the other.
    pub octaves: u32,
    /// The BIGGEST town on the body, metres across. Every other town's
    /// own size is `town::size_of` of this and its rank, so a change to
    /// how sizes are spread needs no rebake of where they stand.
    pub town_radius: f64,
    /// How finely the corridor's centreline is refined, metres
    /// (`road::PIECE`).
    ///
    /// It is in the FINGERPRINT because the atlas keeps the corridor's
    /// heights and derives its directions, so a file baked at another
    /// spacing has a run of the wrong length for every road on the body
    /// and `road::corridor` hands back nothing: roads on the chart, a
    /// route a car can follow, and no ground under any of it and no
    /// tarmac on it. Refused instead, the body is planned here and the
    /// log says why, which is what every other field in this fingerprint
    /// is for. An atlas from before there was a piece parses as nought
    /// and is refused, which is the same answer.
    #[serde(default)]
    pub piece: f64,
    /// How high the baked profile stands over the ground it was routed
    /// on (`road::EMBANK`). It is IN the heights the file stores, so a
    /// file baked at another value is every road on the body at the
    /// wrong level with its tarmac drawn over that, and no other field
    /// here would have said so.
    #[serde(default)]
    pub embank: f64,
    /// The steepest grade a road is BUILT at, which `road::smooth` holds
    /// its profile to.
    ///
    /// In the fingerprint because the baked heights ARE that envelope: a
    /// file baked at a tenth describes a road climbing half again as
    /// steeply as the one the game would build, and nothing else here
    /// would have said so. It is the same silent failure `piece` and
    /// `embank` are in this list to refuse.
    pub steepest: f64,
    /// The minimum horizontal curve radius the alignment was fitted at
    /// (`road::CURVE`).
    ///
    /// It is in the fingerprint for the reason `piece` is: the refined
    /// centreline is DERIVED from the stored waypoints on both sides of
    /// the bake, and a curve fitted at another radius has a different
    /// number of points, so a file baked at one and read at another has
    /// a run of the wrong length for every road on the body and
    /// `road::corridor` hands back nothing. Roads on the chart, a route
    /// a car can follow, and no ground under any of it.
    #[serde(default)]
    pub curve: f64,
    /// The sea this plan was made against. A town qualifies on how high
    /// it stands over the sea and a road is refused into it, so a plan
    /// made at one level is a set of cities underwater at another.
    pub sea: f64,
    /// A few of the body's own heights, metres over the mean radius, at
    /// fixed directions.
    ///
    /// The name, the seed, the radius and the octaves say which BODY this
    /// is, and none of them says what SHAPE that body has: every constant
    /// in `biome` is outside the fingerprint, so moving one moved every
    /// coast on the planet and left an atlas that still fitted, with its
    /// cities standing in the new sea. This is the cheapest thing that
    /// cannot be stale: if the ground under the plan is not the ground
    /// here, the plan is not this body's.
    pub probe: Vec<f64>,
    pub towns: Vec<Placed>,
    pub roads: Vec<Line>,
}

/// How many directions of the body's ground an atlas records, and how far
/// out each may be, metres. A tolerance rather than a bit for bit match,
/// because a plan is read back through JSON and a decimal is not a
/// binary fraction; a metre on eight kilometres of relief is a ten
/// thousandth, and no shape change worth refusing an atlas over moves the
/// ground by less than that.
const PROBES: usize = 12;
const PROBE_TOL: f64 = 1.0;

/// The body's own ground at `PROBES` fixed directions, off the golden
/// angle spiral every other sampler here uses. The planet is asked BARE,
/// because a town levels its own site and the plan is what puts the towns
/// there: a probe that read a levelled site would depend on the answer it
/// is checking.
fn probe(planet: &Planet) -> Vec<f64> {
    let bare = Planet {
        sites: Vec::new().into(),
        ..planet.clone()
    };
    let golden = std::f64::consts::PI * (3.0 - 5f64.sqrt());
    (0..PROBES)
        .map(|i| {
            let y = 1.0 - 2.0 * (i as f64 + 0.5) / PROBES as f64;
            let s = (1.0 - y * y).max(0.0).sqrt();
            let a = golden * i as f64;
            let dir = DVec3::new(s * a.cos(), y, s * a.sin());
            bare.shape().height(dir)
        })
        .collect()
}

/// Where one town stands: its direction, its level, how far across it is
/// and which way it grows. Everything else about it, every lot and every
/// piece of street, is laid out again from these and its own index.
///
/// The size is STORED rather than worked out again from the rank, because
/// it is no longer a function of the rank: a town's size is how near the
/// sea it stands (`town::coastal`) and its growth direction is which way
/// the land falls there, and both are facts about the GROUND that the
/// plan already had to go and measure. Recomputing them at load would be
/// the site survey all over again, which is the thing baking exists to
/// avoid.
#[derive(Serialize, Deserialize)]
pub(crate) struct Placed {
    pub dir: [f64; 3],
    pub h: f64,
    pub r: f64,
    /// East and north, in the town's own frame, of the way it grows.
    pub along: [f64; 2],
}

/// One road: the towns it joins and the line it takes, a direction and a
/// level a point.
#[derive(Serialize, Deserialize)]
pub(crate) struct Line {
    pub from: usize,
    pub to: usize,
    /// x, y, z and the level over the mean radius.
    pub line: Vec<[f64; 4]>,
    /// The GROUND under the road's refined centreline, metres over the
    /// mean radius, one a point of `road::centreline`.
    ///
    /// Only the heights, because the DIRECTIONS are derivable: the
    /// centreline is a slerp between the waypoints at a spacing both
    /// sides compute from `road::PIECE`. Written out whole it would be
    /// 17 MB of this file; as heights alone it is 1.5. An atlas from
    /// before there was a corridor parses with none, and `road::corridor`
    /// refuses a run whose length does not match the line it derives, so
    /// an old file is a body with roads and no ground under them rather
    /// than a body with its roads in the wrong place.
    #[serde(default)]
    pub run: Vec<f64>,
}

impl Atlas {
    /// Plan a body from nothing: its towns, and the roads between them.
    pub fn plan(body: &str, planet: &Planet, sea: f64, town_radius: f64, count: usize) -> Atlas {
        let mut towns = town::plan(planet, sea, town_radius, count, planet.seed);
        // Routed against the planet WITH its sites in, so a road is laid
        // over the ground a town has already levelled rather than over the
        // hill that was there before it.
        let mut levelled = planet.clone();
        levelled.sites = towns.iter().map(town::site_of).collect();
        let roads = road::connect(&levelled, sea, &towns, road::SPACING);
        // And then the VILLAGES the roads grew, appended, so every road's
        // own `from` and `to` still name the towns they named.
        let wayside = road::waysides(&levelled, sea, &roads, &towns, town_radius, planet.seed);
        towns.extend(wayside);
        Atlas {
            body: body.to_string(),
            seed: planet.seed,
            radius: planet.radius,
            octaves: planet.octaves,
            town_radius,
            piece: road::PIECE,
            embank: road::EMBANK,
            steepest: road::STEEPEST,
            curve: road::CURVE,
            sea,
            probe: probe(planet),
            towns: towns
                .iter()
                .map(|t| Placed {
                    dir: t.dir.to_array(),
                    h: t.h,
                    r: t.radius,
                    along: t.along.to_array(),
                })
                .collect(),
            roads: roads
                .iter()
                .map(|r| Line {
                    from: r.from,
                    to: r.to,
                    line: r.line.iter().map(|(d, h)| [d.x, d.y, d.z, *h]).collect(),
                    // Surveyed against the LEVELLED planet, the one the
                    // roads were routed over, so a corridor arriving at
                    // a town meets the level that town cut rather than
                    // the hill that stood there before it.
                    // Rounded to the CENTIMETRE, which is four times
                    // finer than the half metre the finest terrain cell
                    // is and takes 2 MB off this file: serde writes an
                    // f64 in full and there are 190,168 of them.
                    run: road::survey(&levelled, r, sea - planet.radius + road::DRY)
                        .iter()
                        .map(|h| (h * 100.0).round() / 100.0)
                        .collect(),
                })
                .collect(),
        }
    }

    /// The towns this atlas holds, laid out again from their placements.
    ///
    /// The lots and the streets come back off `town::lay`, which is the
    /// same function that made them when the atlas was baked: the file
    /// carries the decision and the code carries the consequence, so a
    /// change to what a building looks like needs no rebake.
    pub fn towns(&self) -> Vec<Town> {
        self.towns
            .iter()
            .enumerate()
            .map(|(i, p)| {
                town::lay(
                    DVec3::from_array(p.dir).normalize_or(DVec3::Y),
                    p.h,
                    p.r,
                    DVec2::from_array(p.along),
                    i,
                    self.seed,
                )
            })
            .collect()
    }

    /// The roads this atlas holds.
    pub fn roads(&self) -> Vec<Road> {
        self.roads
            .iter()
            .map(|r| Road {
                from: r.from,
                to: r.to,
                line: r
                    .line
                    .iter()
                    .map(|p| (DVec3::new(p[0], p[1], p[2]).normalize_or(DVec3::Y), p[3]))
                    .collect(),
            })
            .collect()
    }

    /// The GROUND under each road's refined centreline, in the same order
    /// as `roads`. Empty for a road an old atlas carries no survey for.
    pub fn runs(&self) -> Vec<Vec<f64>> {
        self.roads.iter().map(|r| r.run.clone()).collect()
    }

    /// Whether this atlas is the plan of the body asked for. A plan from
    /// another seed or another size is not a stale plan, it is a plan of a
    /// different world.
    pub fn fits(&self, body: &str, planet: &Planet, sea: f64, town_radius: f64) -> bool {
        self.body.eq_ignore_ascii_case(body)
            && self.seed == planet.seed
            && self.octaves == planet.octaves
            && (self.radius - planet.radius).abs() < 1.0
            && (self.sea - sea).abs() < 1.0
            && (self.town_radius - town_radius).abs() < 1e-6
            && (self.piece - road::PIECE).abs() < 1e-6
            // EMBANK and CURVE are both IN the heights this file stores
            // and in the count of them, and neither was checked: the
            // doc comment on `embank` claimed it was in the fingerprint
            // for a commit while `fits` never read it, which is a
            // fingerprint field that refuses nothing.
            && (self.embank - road::EMBANK).abs() < 1e-6
            && (self.curve - road::CURVE).abs() < 1e-3
            && self.probe.len() == PROBES
            && self
                .probe
                .iter()
                .zip(probe(planet))
                .all(|(there, here)| (there - here).abs() < PROBE_TOL)
    }
}

/// Where a body's atlas lives.
pub(crate) fn path_of(body: &str) -> Option<PathBuf> {
    crate::terrain::assets_dir().map(|dir| dir.join("world").join(format!("{}.json", lower(body))))
}

fn lower(body: &str) -> String {
    body.to_lowercase()
}

/// Read a body's atlas, or nothing if there is none to read.
///
/// Nothing is an ANSWER rather than a failure: the harness plans the body
/// itself and says so, which is what keeps it runnable on a checkout
/// nobody has baked, the same rule a missing texture set follows.
pub(crate) fn load(body: &str, planet: &Planet, sea: f64, town_radius: f64) -> Option<Atlas> {
    let path = path_of(body)?;
    let text = std::fs::read_to_string(&path).ok()?;
    let atlas: Atlas = match serde_json::from_str(&text) {
        Ok(a) => a,
        Err(e) => {
            bevy::log::warn!("{} is not an atlas: {e}", path.display());
            return None;
        }
    };
    if !atlas.fits(body, planet, sea, town_radius) {
        bevy::log::warn!(
            "{} is {}'s atlas at seed {}, {} octaves, radius {:.0} and sea {:.0}, over ground {:?} m high, which is not this body: planning instead",
            path.display(),
            atlas.body,
            atlas.seed,
            atlas.octaves,
            atlas.radius,
            atlas.sea,
            atlas.probe.iter().map(|h| h.round()).collect::<Vec<_>>()
        );
        return None;
    }
    Some(atlas)
}

/// Write an atlas out, making its folder if it is not there.
pub(crate) fn write(atlas: &Atlas, path: &Path) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // COMPACT, because this file is 760,000 numbers and pretty printing
    // puts each on its own line under three levels of indentation: 13
    // bytes of whitespace a number, which is 7.3 MB of a 13.9 MB file
    // and nothing a reader could have read anyway. It is 6.6 MB.
    let text = serde_json::to_string(atlas).map_err(std::io::Error::other)?;
    std::fs::write(path, text)
}

#[cfg(test)]
mod tests;
