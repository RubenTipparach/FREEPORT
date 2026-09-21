//! The TARMAC between towns, streamed the way a town is.
//!
//! A road was DATA: a chain of directions the chart painted and nothing
//! on the ground, so driving between two towns was driving cross country
//! over a route nothing marked. The LEVELLING is in the planet's own
//! field from the first frame (`road::corridor`, because a chunk is
//! meshed once and ground a road will stand on has to be cut before the
//! chunk over it is contoured); this is the other half, and it streams,
//! because 63,840 km of road is sixty million triangles and the eye is
//! only ever in one place.
//!
//! It is `city::stream`'s own shape: a near set held still until the eye
//! has moved, one stretch built a frame and one dropped a frame, and
//! every entity placed through the floating origin like a chunk.

use crate::stream::{Anchored, Frame};
use crate::terrain::{to_mesh_filtered, Vertex};
use crate::world::{Route, Verge, World};
use crate::{Eye, Ground};
use bevy::asset::RenderAssetUsages;
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::NoAutoAabb;
use bevy::math::DVec3;
use bevy::prelude::*;
use freeport_core::field::TERRAIN;
use freeport_core::model::Model;
use freeport_core::pos::WorldPos;
use freeport_core::road::ribbon;

/// How far from the eye a stretch of road is BUILT, metres. A stretch is
/// 5.5 km of tarmac, so this is the one in front, the one behind and a
/// neighbour either side of those.
pub const REACH: f64 = 9_000.0;

/// How far the eye moves before the near set is worked out again. The
/// lamps' own trick: a set held still cannot flicker, and a stretch is
/// kilometres long so a quarter of a kilometre of hysteresis costs
/// nothing.
const RECHECK: f64 = 250.0;

/// How far off a road a car may be and still be FOLLOWING it, metres. A
/// car further out than this is not on the road: it is steered AT the
/// road instead (`Network::mouth`), because the thing a car in a town
/// has to do first is reach the tarmac.
const OFF_ROAD: f64 = 400.0;

/// Where every stretch of road on the body is, worked out once.
///
/// A body carries 190,168 corridor points in 11,900 stretches, and the
/// near ones have to be found every time the eye moves a furlong. The
/// middle of each stretch is enough to rank them, so that is all this
/// keeps: walking 11,900 directions is nothing beside contouring one
/// chunk, and it needs no index of its own.
#[derive(Resource, Default)]
pub struct Network {
    /// Which road, which stretch of it, and where its middle is.
    stretches: Vec<(usize, usize, DVec3)>,
    /// Every GAS STATION on the body: the middle of its forecourt in the
    /// planet's frame, which is what a car pulls up beside.
    pumps: Vec<DVec3>,
}

impl Network {
    /// Every stretch of every road on a body, and every gas station.
    pub fn of(world: &World) -> Network {
        let radius = world.planet.radius;
        let pumps = world
            .routes
            .iter()
            .flat_map(|route| {
                route
                    .pumps
                    .iter()
                    .filter_map(|p| freeport_core::road::station::middle(route.course(), p, radius))
            })
            .collect();
        let mut stretches = Vec::new();
        for (r, route) in world.routes.iter().enumerate() {
            for k in 0..ribbon::count(route.line.len()) {
                let at = ribbon::span(k, route.line.len());
                // A stretch with nothing open in it is inside a town,
                // which has laid its own streets there.
                // PAVED and not open: a stretch whose only tarmac is
                // the run into a town is still a stretch of tarmac.
                if at.len() < 2 || !route.open[at.clone()].windows(2).any(|o| o[0] && o[1]) {
                    continue;
                }
                stretches.push((r, k, route.line[(at.start + at.end) / 2]));
            }
        }
        Network { stretches, pumps }
    }

    /// How many stretches there are, for the log.
    pub fn len(&self) -> usize {
        self.stretches.len()
    }

    /// How many gas stations there are, for the log.
    pub fn pumps(&self) -> usize {
        self.pumps.len()
    }

    /// The nearest gas station to a point in the planet's frame: how far
    /// off its forecourt's middle is, metres, and where it is.
    pub fn nearest_pump(&self, at: DVec3) -> Option<(f64, DVec3)> {
        self.pumps
            .iter()
            .map(|&p| ((p - at).length(), p))
            .filter(|(d, _)| d.is_finite())
            .min_by(|a, b| a.0.total_cmp(&b.0))
    }

    /// Where on a ROAD to steer for: `look` metres along the tarmac from
    /// the point nearest the car, in whichever direction gets nearer
    /// `goal`.
    ///
    /// This is what makes a drive between two towns a JOURNEY rather than
    /// a car driving cross country. Aimed straight at the next settlement
    /// a car leaving the port drove twelve metres and then oscillated
    /// against a building for the rest of the run, because there was
    /// nothing to follow round one; aimed at the road it follows the road,
    /// which is what a road is FOR.
    ///
    /// The nearest stretch is found off the stretch middles (11,987 of
    /// them, and a distance to each is an angle) and only that stretch's
    /// own seventeen points are looked at. Walking 190,168 points a frame
    /// would be the whole frame.
    pub fn follow(
        &self,
        world: &World,
        at: DVec3,
        goal: DVec3,
        look: f64,
        radius: f64,
    ) -> Option<DVec3> {
        let (r, near) = self.nearest(world, at)?;
        let route = world.routes.get(r)?;
        if route.line[near].angle_between(at) * radius > OFF_ROAD {
            return None;
        }
        // Which WAY along it, decided off the road's two ENDS and never
        // off the next point along. A road winds, so a step that goes
        // away from the goal is not a road that goes away from it: the
        // local test turned the car round at every bend it met, and a
        // drive out of the port covered 1,425 m of tarmac in seven
        // minutes while closing 40 m of nine kilometres. The ends are a
        // fact about the whole road, so the answer cannot flip under a
        // car that has not gone anywhere.
        let ends = (route.line[0], route.line[route.line.len() - 1]);
        let ahead = ends.1.angle_between(goal) < ends.0.angle_between(goal);
        let pieces = ((look / freeport_core::road::PIECE).ceil() as usize).max(1);
        let want = if ahead {
            (near + pieces).min(route.line.len() - 1)
        } else {
            near.saturating_sub(pieces)
        };
        Some(route.line[want] * (radius + route.run[want]))
    }

    /// The nearest tarmac to a direction, WHATEVER the distance: where a
    /// car standing in a town has to get to before `follow` can take it
    /// anywhere. Planet local, standing on the level the corridor was
    /// cut to, like everything else here.
    pub fn mouth(&self, world: &World, at: DVec3) -> Option<DVec3> {
        let (r, near) = self.nearest(world, at)?;
        let route = world.routes.get(r)?;
        Some(route.line[near] * (world.planet.radius + route.run[near]))
    }

    /// Which road and which of its OPEN centreline points is nearest a
    /// direction. The nearest stretch is found off the stretch middles
    /// and only that stretch's own seventeen points are looked at:
    /// walking 190,168 points a frame would be the frame.
    fn nearest(&self, world: &World, at: DVec3) -> Option<(usize, usize)> {
        let (r, k, _) = self
            .stretches
            .iter()
            .min_by(|a, b| a.2.angle_between(at).total_cmp(&b.2.angle_between(at)))
            .copied()?;
        let route = world.routes.get(r)?;
        let near = ribbon::span(k, route.line.len())
            .filter(|i| route.open[*i])
            .min_by(|a, b| {
                route.line[*a]
                    .angle_between(at)
                    .total_cmp(&route.line[*b].angle_between(at))
            })?;
        Some((r, near))
    }

    /// The stretches within `REACH` of a direction, nearest first: the
    /// distance is along the GROUND, the angle between two directions
    /// times the radius, which is this file's own rule. Written as the
    /// straight line to a point on the mean radius it would add the
    /// eye's own altitude to every distance, and the tarmac under the
    /// wheels would read a kilometre away.
    fn near(&self, at: DVec3, radius: f64) -> Vec<(usize, usize)> {
        let mut out: Vec<_> = self
            .stretches
            .iter()
            .filter_map(|(r, k, mid)| {
                let away = mid.angle_between(at) * radius;
                (away < REACH).then_some((away, *r, *k))
            })
            .collect();
        out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        out.into_iter().map(|(_, r, k)| (r, k)).collect()
    }
}

/// Which stretch of which road an entity is: the road's index in the
/// world's own list and the stretch's index along it.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub struct Paved(pub usize, pub usize);

/// What the streamer needs to lay one stretch of tarmac.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Kit<'w> {
    pub meshes: ResMut<'w, Assets<Mesh>>,
    pub ground: Res<'w, crate::terrain::Ground3d>,
    pub frame: Res<'w, Frame>,
    /// Where every stretch on the body is, and what is standing: the
    /// two lists this system moves things between.
    pub network: Res<'w, Network>,
    pub fabric: ResMut<'w, crate::world::Fabric>,
}

/// Build the stretches of road near the eye and drop those out of reach,
/// ONE a frame each way, which is the rule `stream.rs` keeps for the
/// ground, `city::stream` for the towns and `lamps.rs` for the lights: a
/// frame that built everything it wanted would be a frame you could see.
pub fn stream_roads(
    mut commands: Commands,
    eye: Res<Eye>,
    ground: Res<Ground>,
    laid: Query<(Entity, &Paved)>,
    mut last: Local<Option<DVec3>>,
    mut kit: Kit,
) {
    let here = eye.0 .0 - ground.1;
    if last.is_some_and(|l| (l - here).length() < RECHECK) {
        return;
    }
    let want = kit
        .network
        .near(here.normalize_or(DVec3::Y), ground.0.planet.radius);
    // DROP first, so a frame that both drops and builds does not hold
    // both at once, and one a frame so the work is spread.
    if let Some((entity, gone)) = laid.iter().find(|(_, p)| !want.contains(&(p.0, p.1))) {
        commands.entity(entity).despawn();
        // And its LAMPS with it: a stretch that is not there does not
        // light the road, and `light_lamps` reads this list.
        kit.fabric.verges.retain(|v| v.which != (gone.0, gone.1));
        return;
    }
    let Some(&(r, k)) = want
        .iter()
        .find(|(r, k)| !laid.iter().any(|(_, p)| p.0 == *r && p.1 == *k))
    else {
        // Nothing left to do: the set is settled, so hold it still until
        // the eye has moved.
        *last = Some(here);
        return;
    };
    let Some(route) = ground.0.routes.get(r) else {
        return;
    };
    if let Some(verge) = lay(&mut commands, &mut kit, &ground.0, route, Paved(r, k)) {
        kit.fabric.verges.push(verge);
    }
}

/// One stretch of tarmac: its own mesh, in its own frame, placed through
/// the floating origin like a chunk.
fn lay(
    commands: &mut Commands,
    kit: &mut Kit,
    ground: &World,
    route: &Route,
    which: Paved,
) -> Option<Verge> {
    let radius = ground.planet.radius;
    let at = ribbon::span(which.1, route.line.len());
    if at.len() < 2 {
        return None;
    }
    let course = route.stretch(at);
    let frame = ribbon::frame(course.line, course.run, radius);
    // The MOUND reads the field's own surface, which is what the mesher
    // contours and what the walker and the car collide against: the road
    // carries its own ground, so it cannot disagree with the ground it
    // stands on however coarse the chunk under it happens to be.
    let planet = &ground.planet;
    let model = ribbon::stretch(&frame, course, radius, &|dir| planet.surface(dir).0);
    if model.mesh.positions.is_empty() {
        return None;
    }
    let sea = route.sea;
    let origin = frame.world(DVec3::ZERO);
    let earth_at = crate::terrain::ground_mapping(origin, sea, Some(planet.shape()));
    // TWO meshes out of ONE model, filtered by material, because the two
    // are mapped from different places: the tarmac and its markings are
    // a BUILT thing and map in this stretch's own frame, and the mound
    // is TERRAIN and maps the way a chunk does, in the planet's frame
    // and modulo the ground's own tile, so the grass on it tiles with
    // the grass beside it and carries this latitude's own climate.
    let built = |p: Vec3| Vertex::built(p, (frame.world(p.as_dvec3()).length() - sea) as f32);
    let earth = |p: Vec3| earth_at(frame.world(p.as_dvec3()));
    let mut mesh = to_mesh_filtered(&model.mesh, built, |m| m != TERRAIN);
    let mut skirt = to_mesh_filtered(&model.mesh, earth, |m| m == TERRAIN);
    let bounds = Aabb::enclosing(mesh_points(&model.mesh));
    mesh.asset_usage = RenderAssetUsages::RENDER_WORLD;
    skirt.asset_usage = RenderAssetUsages::RENDER_WORLD;
    let anchor = WorldPos(frame.world(DVec3::ZERO));
    let basis = Mat3::from_cols(
        frame.east.as_vec3(),
        frame.north.as_vec3(),
        frame.dir.as_vec3(),
    );
    let mut entity = commands.spawn((
        Mesh3d(kit.meshes.add(mesh)),
        // The TARMAC's own material: the ground's, with a depth bias.
        MeshMaterial3d(kit.ground.tarmac.clone()),
        Transform {
            translation: kit.frame.0.local(anchor),
            rotation: Quat::from_mat3(&basis),
            scale: Vec3::ONE,
        },
        Anchored { at: anchor },
        which,
    ));
    if let Some(bounds) = bounds {
        entity.insert((bounds, NoAutoAabb));
    }
    // The mound is a CHILD of the tarmac, in the same frame with an
    // identity transform, so it is placed, rebased and despawned with
    // it: one `Anchored` and one `Paved`, which is what keeps the near
    // set a set of STRETCHES rather than of meshes.
    entity.with_children(|kids| {
        let mut kid = kids.spawn((
            Mesh3d(kit.meshes.add(skirt)),
            MeshMaterial3d(kit.ground.ground.clone()),
            Transform::IDENTITY,
        ));
        if let Some(bounds) = bounds {
            kid.insert((bounds, NoAutoAabb));
        }
    });
    Some(verge_of(&model, &frame, which))
}

/// What a stretch puts in the WORLD besides its picture: its lamps and
/// whatever stands on it, in the body's own frame.
fn verge_of(model: &Model, frame: &freeport_core::town::Frame, which: Paved) -> Verge {
    // The lamps each carrying their index along the whole road: a
    // stretch streams, so an index into one would name a different lamp
    // the moment a neighbour arrived. The stretch's own number times
    // `ribbon::LAMPS`, which is more than a stretch can hold, is what
    // makes a place in one a place along the road.
    let lamps = model
        .lamps
        .iter()
        .enumerate()
        .take(ribbon::LAMPS)
        .map(|(i, p)| {
            (
                which.1 * ribbon::LAMPS + i,
                frame.world(*p),
                ribbon::LAMP_REACH,
            )
        })
        .collect();
    // And whatever STANDS on the stretch, which is a gas station's pumps,
    // pillars and kiosk: boxes in the body's frame, the same boxes the
    // walls of a town are, so a car is stopped by a pump the way it is
    // stopped by a wall.
    let blocks = model.blocks(frame);
    let bounds = blocks
        .iter()
        .fold((DVec3::INFINITY, DVec3::NEG_INFINITY), |(lo, hi), b| {
            let (blo, bhi) = b.bounds();
            (lo.min(blo), hi.max(bhi))
        });
    Verge {
        which: (which.0, which.1),
        blocks,
        bounds,
        lamps,
    }
}

fn mesh_points(mesh: &freeport_core::dc::DcMesh) -> impl Iterator<Item = Vec3> + '_ {
    mesh.positions.iter().map(|p| Vec3::from(*p))
}

/// How far the first TARMAC of a road stands from the nearest paving of
/// the town it leaves: metres along the ground, and which town.
///
/// It is the one number that says whether a highway JOINS a city or
/// merely points at it, and it is measured rather than reasoned about
/// because the two ends are decided by different rules. A town's streets
/// are laid where its `demand` says a block carries a lot; a road's
/// tarmac starts where `road::open` says the point is outside the town's
/// own levelling AND its skirt, because inside that the town's site
/// answers the ground and a road laid there would float over it. Nothing
/// makes those meet, so the gap is whatever it is.
///
/// Measured against every piece of the town's paving rather than along
/// the road's own bearing, because a town's grid is a grid: the nearest
/// tarmac to a street is not always the street the road is pointing at.
///
/// On this body it is **39 m**, and the two ends say which is short: the
/// first tarmac stands 169 m out of the port while the town's own paving
/// reaches 270 at its furthest. The road leaves along a bearing where
/// the town is NARROW, so the tarmac starts at that bearing's own
/// `level_r` (the outline plus a 12 m apron) and the town's outermost
/// block frontage there is 27 m further in, which is a block and a half
/// of grid. It is NOT the skirt: measured, dropping `field::site_skirt`
/// from `road::open` moved the levelling 24.6 m inward and the gap not
/// at all, because the first tarmac is laid at a STATION and the
/// stations are 85 m apart.
///
/// Closing it is a JUNCTION between two paving systems, which this
/// project already names as missing: a road arrives on an arbitrary
/// bearing and a town's streets run on its own grid, so there is nothing
/// for the tarmac to meet until one of them is laid toward the other.
/// Running the tarmac on INTO the town instead would put a 10 cm lip
/// across whatever suburb street it crossed, since a road is lifted
/// 0.15 m and a street 0.05.
/// Where a road's tarmac actually STARTS, which is the junction: the
/// road, the town it runs into, the direction the first laid piece
/// begins at and the ground under it.
///
/// It is part way ALONG its own piece wherever a road meets a town's
/// paving, which is `ribbon::stretch`'s own rule and not merely the
/// first open station, so the point has to be lerped exactly the way the
/// ribbon lerps it. One implementation, because `slip_of` measures this
/// point and `aim::junction` photographs it, and a camera aimed at a
/// junction the harness measures somewhere else is a picture of the
/// wrong place.
pub fn mouth_of(world: &World) -> Option<(usize, usize, DVec3, f64)> {
    let (k, road) = world.roads.iter().enumerate().next()?;
    let route = world.routes.get(k)?;
    // The HIGHWAY's own mouth, which is where the corridor stops being
    // cut and the slip takes over: `route.slip.0` head points are the
    // slip, so this is the first point the highway itself lays.
    //
    // The route's HEAD is the crossing the slip ends at, and measuring
    // from there is the third tautology in this file's history: a walk
    // in from a point already on the town's paving reports nought bare
    // ground whatever the road does, exactly as `road::clear`'s own
    // `MEET` did. A camera aimed there frames a crossing and not the
    // junction.
    // The MIDDLE of the slip, so a camera over it frames the whole
    // junction rather than one end of it: over the highway's mouth the
    // slip runs off the bottom of the frame and the picture is of a
    // road, which is what the first three renders of this were.
    let mid = route.slip.0 / 2;
    let at = route.line.get(mid).copied()?;
    let h = route.run.get(mid).copied()?;
    Some((k, road.from, at, h))
}

/// The SLIP at a road's near end: how many pieces it is, how far it runs
/// and where its two ends stand out of the town's own middle, metres.
///
/// `mouth_of` returns its MIDDLE, which is what a camera frames; this is
/// what says whether the junction is there.
///
/// A junction is either there or it is not, and the one number that says
/// so is how far the slip's own end stands from the crossing it was laid
/// to. Everything else about it (the mouth, the length) is what says
/// which end is short when it is not.
pub fn slip_of(world: &World) -> Option<Slip> {
    let route = world.routes.first()?;
    let town = world.towns.get(world.roads.first()?.from)?;
    let radius = world.planet.radius;
    let n = route.slip.0;
    let paved = town
        .pieces
        .iter()
        .map(|p| {
            let dir = (town.dir * radius + town.east * p.x + town.north * p.z).normalize();
            dir.angle_between(town.dir) * radius
        })
        .fold(0.0f64, f64::max);
    if n < 2 {
        return Some(Slip {
            n: 0,
            ran: 0.0,
            mouth: 0.0,
            end: 0.0,
            meets: f64::NAN,
            paved,
            buried: 0.0,
            at: 0,
            weight: 0.0,
        });
    }
    let head = &route.line[..n];
    let ran: f64 = head
        .windows(2)
        .map(|w| w[0].angle_between(w[1]) * radius)
        .sum();
    let out = |d: DVec3| d.angle_between(town.dir) * radius;
    // And how far the slip's OWN END is from the paving it was laid to
    // reach, which is the junction itself.
    let end = head[0] * radius - town.dir * radius;
    let (x, z) = (end.dot(town.east), end.dot(town.north));
    let to_paving = town
        .pieces
        .iter()
        .map(|p| {
            ((p.x - x).abs() - p.w * 0.5)
                .max(0.0)
                .hypot(((p.z - z).abs() - p.d * 0.5).max(0.0))
        })
        .fold(f64::INFINITY, f64::min);
    // And how far the ground the mesher DRAWS stands over the slip's own
    // tarmac, which is the one number that says whether a junction is
    // visible: the slip reads the analytic surface and the mesher
    // contours the field, and the two part company wherever the
    // volumetric term still bites.
    // WHICH piece and how far into the town's own plateau it stands,
    // because a slip crosses the skirt where the volumetric term comes
    // back, and a number with no place on it cannot be looked for.
    let site = freeport_core::town::site_of(town);
    let (buried, at, weight) = (0..n)
        .map(|k| {
            let dir = route.line[k];
            let near = world.planet.around(dir, 1e-9);
            let here = freeport_core::town::surface_radius(&near, dir) - radius;
            (
                here - (route.run[k] + ribbon::LIFT),
                k,
                near.site_weight(&site, dir),
            )
        })
        .fold(
            (f64::NEG_INFINITY, 0, 0.0),
            |a, b| if b.0 > a.0 { b } else { a },
        );
    Some(Slip {
        n,
        ran,
        mouth: out(head[n - 1]),
        end: out(head[0]),
        meets: to_paving,
        paved,
        buried,
        at,
        weight,
    })
}

/// What `slip_of` measures about road 0's slip.
pub(crate) struct Slip {
    pub n: usize,
    pub ran: f64,
    pub mouth: f64,
    pub end: f64,
    pub meets: f64,
    pub paved: f64,
    pub buried: f64,
    pub at: usize,
    pub weight: f64,
}

/// Everything the harness has to SAY about the roads on this body, in
/// one place.
///
/// It lived in `main.rs`, which is the arguments, the `App` and the
/// schedule: a measurement's report belongs beside the measurement, and
/// four log lines of it took that file over this project's own nine
/// hundred.
pub fn report(world: &World) {
    let (steep, share, pieces, at_run, at_rise) = worst_grade(world);
    bevy::log::info!(
        "the steepest road piece on the body climbs at {:.1}% ({at_rise:.2} m over {at_run:.3} m) and {:.2}% of {pieces} pieces are over the {:.0}% a highway is allowed",
        steep * 100.0,
        share * 100.0,
        freeport_core::road::STEEPEST * 100.0,
    );
    let (mouths, slipped, worst, joins) = dead_ends(world);
    bevy::log::info!(
        "{slipped} slips carry a road's two ENDS into a town, {joins} mouths are a road joining or leaving another's TRUNK, and {mouths} mouths of tarmac are left bare where a road passes THROUGH a settlement; the worst stands {worst:.0} m from any paving"
    );
    if let Some(s) = slip_of(world) {
        bevy::log::info!(
            "road 0's SLIP is {} pieces over {:.0} m, from the highway's mouth {:.0} m out of town 0 to {:.0} m out, ending {:.2} m from the town's own paving, which reaches {:.0} m; the drawn ground stands {:.2} m over its own tarmac at the worst, at piece {} of {} where the town's plateau weighs {:.2}",
            s.n, s.ran, s.mouth, s.end, s.meets, s.paved, s.buried, s.at, s.n, s.weight
        );
    }
}

/// The steepest GRADE any road on the body is built at, and the share of
/// its pieces over the limit a highway is allowed: rise over run, so 0.07
/// is the seven per cent a motorway is designed to.
///
/// Between STATIONS of the refined centreline, which is what the tarmac
/// is actually laid on and what a car actually drives: the router only
/// ever looked at waypoints ten kilometres apart, and `road::smooth` is
/// what is supposed to hold the promise between the pieces it did not
/// look at.
pub fn worst_grade(world: &World) -> (f64, f64, usize, f64, f64) {
    let radius = world.planet.radius;
    let (mut worst, mut over, mut all) = (0.0f64, 0usize, 0usize);
    let (mut at_run, mut at_rise) = (0.0f64, 0.0f64);
    // A slip's pieces are three metres and a highway's eighty five, so
    // "a runt piece" is every slip piece by construction: the first cut
    // of this counted them and reported its own `SLIP_PIECE`, which is
    // the fourth time in this repository a harness has measured a
    // constant it set itself. What tells the two apart is WHICH of them
    // is steep, so each carries its own worst.
    let (mut in_slip, mut short, mut on_road) = (0usize, 0.0f64, 0.0f64);
    for route in &world.routes {
        for k in 0..route.line.len().saturating_sub(1) {
            let run = route.line[k].angle_between(route.line[k + 1]) * radius;
            if run <= 0.0 {
                continue;
            }
            let rise = route.run[k + 1] - route.run[k];
            let grade = rise.abs() / run;
            if grade > worst {
                worst = grade;
                at_run = run;
                at_rise = rise;
            }
            all += 1;
            // Against the limit PLUS what the file can express. The
            // atlas rounds a height to the centimetre, so a piece whose
            // two ends are each out by one reads up to `0.02 / run`
            // steeper than it was baked: `road::smooth`'s envelope
            // BINDS on most of a road at seven per cent, which puts
            // tens of thousands of pieces exactly at the limit with
            // half of them rounding over it by a hundredth of a per
            // cent. Measuring them as defects is measuring the file's
            // own precision.
            let slack = freeport_core::road::STEEPEST + 0.02 / run;
            if grade > slack {
                over += 1;
                // A piece with EITHER end in a slip is the slip's: the
                // tail's join piece runs from the highway's own last
                // point to the slip's first, and counted off its head
                // end alone it read as the highway's, 28 pieces at up
                // to 26.2% on a body whose highways are held to seven.
                if k < route.slip.0 || k + 1 >= route.line.len() - route.slip.1 {
                    in_slip += 1;
                    short = short.max(grade);
                } else {
                    on_road = on_road.max(grade);
                }
            }
        }
    }
    bevy::log::info!(
        "of {over} pieces over the limit, {in_slip} are in a SLIP and climb at up to {:.1}%, and {} are on the HIGHWAY itself at up to {:.1}%",
        short * 100.0,
        over - in_slip,
        on_road * 100.0
    );
    (worst, over as f64 / all.max(1) as f64, all, at_run, at_rise)
}

/// Every DEAD END on the body: a place where a road's tarmac stops and
/// nothing of the town's own paving carries on from it.
///
/// A road is cut by `road::open` at EVERY settlement it passes, not only
/// at the two it joins, because `road::waysides` grows a village
/// wherever a road has run a day's cart. Each of those is a gap with two
/// mouths, and `world::splice_slip` lays a slip at the route's two ENDS
/// and nowhere else: a road through a village stops short of it on one
/// side and starts again past it on the other, which is two dead ends
/// that no picture of road 0 would ever show.
///
/// A road joining or leaving another's TRUNK stops laying tarmac too,
/// because the owner's is there, and that is a JOIN and not a dead end:
/// counted among the mouths, 350 roads' trunks put the worst bare mouth
/// 98 km from any paving, which was a fork in open country and not a
/// town anybody had left a road short of.
///
/// It returns how many BARE mouths there are, how many route ends a slip
/// carries in, the worst distance from a bare mouth to the nearest
/// paving any town laid, metres, and how many mouths are joins.
pub fn dead_ends(world: &World) -> (usize, usize, f64, usize) {
    let radius = world.planet.radius;
    let (mut mouths, mut slipped, mut worst, mut joins) = (0usize, 0usize, 0.0f64, 0usize);
    for (r, route) in world.routes.iter().enumerate() {
        let ends = (route.slip.0, route.line.len() - route.slip.1);
        for k in 0..route.line.len().saturating_sub(1) {
            // A MOUTH is where the tarmac starts or stops: the gate the
            // ribbon lays a piece on is `open[k] && open[k + 1]`.
            let (a, b) = (route.open[k], route.open[k + 1]);
            if a == b {
                continue;
            }
            if route.trunk[k] || route.trunk[k + 1] {
                joins += 1;
                continue;
            }
            let at = route.line[if a { k + 1 } else { k }];
            mouths += 1;
            worst = worst.max(to_paving(world, at, radius));
        }
        // A slip does not MARK a mouth, it takes one away: its points
        // are open and so is the highway's first piece, so there is no
        // longer a transition there to count. Counting them among the
        // mouths reported nought of 2,756 carried by a slip on a body
        // whose 308 roads each carry two, which is the harness reading
        // its own fix as a failure. What a route HAS is what is counted.
        slipped += usize::from(ends.0 > 0) + usize::from(ends.1 < route.line.len());
        let _ = r;
    }
    (mouths, slipped, worst, joins)
}

/// How far a direction stands from the nearest paving ANY town laid,
/// metres: nought where it is already on a street.
fn to_paving(world: &World, at: DVec3, radius: f64) -> f64 {
    world
        .towns
        .iter()
        .map(|town| {
            let here = at * radius - town.dir * radius;
            let (x, z) = (here.dot(town.east), here.dot(town.north));
            town.pieces
                .iter()
                .map(|p| {
                    ((p.x - x).abs() - p.w * 0.5)
                        .max(0.0)
                        .hypot(((p.z - z).abs() - p.d * 0.5).max(0.0))
                })
                .fold(f64::INFINITY, f64::min)
        })
        .fold(f64::INFINITY, f64::min)
}

/// How far the ground a COARSE chunk draws stands over a road's own
/// tarmac: the worst, the median and how many roads were swept.
///
/// The terrain's cell is about a sixty fourth of its own distance from
/// the eye and a corridor is 14 m wide, so past a couple of hundred
/// metres the mesher has no sample inside the corridor at all and draws
/// the planet WITHOUT it. That is the planet carrying the towns' sites
/// and none of the roads', which is what this measures against, and
/// anything it stands over the tarmac by is road the country hides.
pub fn ground_over_tarmac(world: &World) -> Option<(f64, f64, usize)> {
    let bare = freeport_core::field::Planet {
        sites: world
            .towns
            .iter()
            .map(freeport_core::town::site_of)
            .collect(),
        ..world.planet.clone()
    };
    let radius = world.planet.radius;
    // ALONG the chord and not at the stations alone, and through
    // `town::surface_radius`, which is the function the survey itself
    // asked: `Planet::surface` is the analytic height and the two part
    // company wherever the volumetric term bites, so measuring against
    // the other one reports a disagreement as a burial.
    const STEPS: usize = 8;
    // A SWEEP of roads and not road 0 alone. The worst is the number
    // that decides whether an embankment clears the terrain's own LOD,
    // and one road is one sample of a body: the first thirty two are a
    // few thousand kilometres of it and cost a second of startup.
    const ROADS: usize = 32;
    let mut over: Vec<f64> = Vec::new();
    let roads = world.routes.len().min(ROADS);
    for route in world.routes.iter().take(ROADS) {
        for i in 0..route.line.len().saturating_sub(1) {
            if !(route.graded[i] && route.graded[i + 1]) {
                continue;
            }
            for k in 0..STEPS {
                let dir = freeport_core::road::step(route.line[i], route.line[i + 1], k, STEPS);
                let here =
                    freeport_core::town::surface_radius(&bare.around(dir, 1e-9), dir) - radius;
                let t = k as f64 / STEPS as f64;
                let road = route.run[i] * (1.0 - t) + route.run[i + 1] * t + ribbon::LIFT;
                over.push(here - road);
            }
        }
    }
    if over.is_empty() {
        return None;
    }
    let worst = over.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    over.sort_by(f64::total_cmp);
    Some((worst, over[over.len() / 2], roads))
}
