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

use bevy::math::DVec3;
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
        sites: Vec::new(),
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

/// Where one town stands: its direction and its level. Everything else
/// about it is laid out again from these and its own index.
#[derive(Serialize, Deserialize)]
pub(crate) struct Placed {
    pub dir: [f64; 3],
    pub h: f64,
}

/// One road: the towns it joins and the line it takes, a direction and a
/// level a point.
#[derive(Serialize, Deserialize)]
pub(crate) struct Line {
    pub from: usize,
    pub to: usize,
    /// x, y, z and the level over the mean radius.
    pub line: Vec<[f64; 4]>,
}

impl Atlas {
    /// Plan a body from nothing: its towns, and the roads between them.
    pub fn plan(body: &str, planet: &Planet, sea: f64, town_radius: f64, count: usize) -> Atlas {
        let towns = town::plan(planet, sea, town_radius, count, planet.seed);
        // Routed against the planet WITH its sites in, so a road is laid
        // over the ground a town has already levelled rather than over the
        // hill that was there before it.
        let mut levelled = planet.clone();
        levelled.sites = towns.iter().map(town::site_of).collect();
        let roads = road::connect(&levelled, sea, &towns, road::SPACING);
        Atlas {
            body: body.to_string(),
            seed: planet.seed,
            radius: planet.radius,
            octaves: planet.octaves,
            town_radius,
            sea,
            probe: probe(planet),
            towns: towns
                .iter()
                .map(|t| Placed {
                    dir: t.dir.to_array(),
                    h: t.h,
                })
                .collect(),
            roads: roads
                .iter()
                .map(|r| Line {
                    from: r.from,
                    to: r.to,
                    line: r.line.iter().map(|(d, h)| [d.x, d.y, d.z, *h]).collect(),
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
                    // Its OWN size, off its rank, which is why the file
                    // carries the biggest and not one radius a town.
                    town::size_of(self.town_radius, i, self.seed),
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
    let text = serde_json::to_string_pretty(atlas).map_err(std::io::Error::other)?;
    std::fs::write(path, text)
}

#[cfg(test)]
mod tests;
