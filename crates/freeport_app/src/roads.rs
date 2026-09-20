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
use crate::terrain::{to_mesh, Vertex};
use crate::world::{Route, Verge, World};
use crate::{Eye, Ground};
use bevy::asset::RenderAssetUsages;
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::NoAutoAabb;
use bevy::math::DVec3;
use bevy::prelude::*;
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
}

impl Network {
    /// Every stretch of every road on a body.
    pub fn of(world: &World) -> Network {
        let mut stretches = Vec::new();
        for (r, route) in world.routes.iter().enumerate() {
            for k in 0..ribbon::count(route.line.len()) {
                let at = ribbon::span(k, route.line.len());
                // A stretch with nothing open in it is inside a town,
                // which has laid its own streets there.
                if at.len() < 2 || !route.open[at.clone()].windows(2).any(|o| o[0] && o[1]) {
                    continue;
                }
                stretches.push((r, k, route.line[(at.start + at.end) / 2]));
            }
        }
        Network { stretches }
    }

    /// How many stretches there are, for the log.
    pub fn len(&self) -> usize {
        self.stretches.len()
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
    if let Some(verge) = lay(
        &mut commands,
        &mut kit,
        route,
        ground.0.planet.radius,
        Paved(r, k),
    ) {
        kit.fabric.verges.push(verge);
    }
}

/// One stretch of tarmac: its own mesh, in its own frame, placed through
/// the floating origin like a chunk.
fn lay(
    commands: &mut Commands,
    kit: &mut Kit,
    route: &Route,
    radius: f64,
    which: Paved,
) -> Option<Verge> {
    let at = ribbon::span(which.1, route.line.len());
    if at.len() < 2 {
        return None;
    }
    let (line, run, open, lit) = (
        &route.line[at.clone()],
        &route.run[at.clone()],
        &route.open[at.clone()],
        &route.lit[at.clone()],
    );
    let frame = ribbon::frame(line, run, radius);
    let model = ribbon::stretch(&frame, line, run, open, lit, radius);
    if model.mesh.positions.is_empty() {
        return None;
    }
    let sea = route.sea;
    let place = |p: Vec3| Vertex::built(p, (frame.world(p.as_dvec3()).length() - sea) as f32);
    let mut mesh = to_mesh(&model.mesh, place);
    let bounds = Aabb::enclosing(mesh_points(&model.mesh));
    mesh.asset_usage = RenderAssetUsages::RENDER_WORLD;
    let anchor = WorldPos(frame.world(DVec3::ZERO));
    let basis = Mat3::from_cols(
        frame.east.as_vec3(),
        frame.north.as_vec3(),
        frame.dir.as_vec3(),
    );
    let mut entity = commands.spawn((
        Mesh3d(kit.meshes.add(mesh)),
        MeshMaterial3d(kit.ground.0.clone()),
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
    // The lamps in the BODY's own frame, each carrying its index along
    // the whole road: a stretch streams, so an index into one would name
    // a different lamp the moment a neighbour arrived. The stretch's own
    // number times `ribbon::LAMPS`, which is more than a stretch can
    // hold, is what makes a place in one a place along the road.
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
    Some(Verge {
        which: (which.0, which.1),
        lamps,
    })
}

fn mesh_points(mesh: &freeport_core::dc::DcMesh) -> impl Iterator<Item = Vec3> + '_ {
    mesh.positions.iter().map(|p| Vec3::from(*p))
}
