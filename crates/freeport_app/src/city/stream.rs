//! Which towns are BUILT, and following the eye with that set.
//!
//! Every town on the body is PLANNED from the first frame: it levels its
//! own ground in the planet's field and the body's chart paints it, so a
//! city exists on the map and under the feet whether or not anybody has
//! raised a building on it. What streams is the BUILDINGS, and this is
//! the same rule `stream.rs` keeps for the ground and `lamps.rs` for the
//! lights: a town near the eye is geometry and a town over the horizon
//! is a number.
//!
//! It replaces a set picked ONCE at startup, the nearest `TOWNS_BUILT`
//! to where the world happened to begin. Everything past those eight was
//! a levelled plateau with a city painted on the chart over it and
//! nothing standing on it, which is what the owner read off the map
//! beside the ground: driving to the next town arrived at an empty
//! field.

use super::detail::{self, Kit, Raise};
use crate::stream::Frame;
use crate::world::{Fabric, World};
use crate::{Eye, Ground};
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::tasks::{futures::check_ready, Task};
use freeport_core::town::Town;
use std::sync::Arc;

/// How far the eye moves before the built set is looked at again,
/// metres. Well under the gap between two settlements, which is the
/// lamps' own hysteresis rule at the other scale.
const RECHECK: f64 = 250.0;

/// The buildings' library, loaded once and shared with the workers that
/// build a town's tiles. Reading it per town would be a file read every
/// time a village came over the horizon.
#[derive(Resource)]
pub struct Library(pub Arc<crate::buildings::Library>);

/// What the town streamer is in the middle of.
#[derive(Resource, Default)]
pub struct Building {
    /// Where the eye was when the wanted set was last worked out.
    looked: Option<DVec3>,
    /// The town being raised on a worker, if one is.
    raising: Option<(usize, Task<Raise>)>,
}

/// The planned towns nearest a point, at most `count` of them and none
/// further off than `reach`, sorted so two sets can be compared.
///
/// A COUNT bounds the cost, which a reach cannot: a town is about
/// 375,000 triangles, so eight are three million, and a reach wide
/// enough to catch the next one on an empty continent would catch forty
/// in a cluster. The REACH is what stops the count dragging eight towns
/// across an ocean to keep itself full.
pub fn wanted(towns: &[Town], radius: f64, at: DVec3, count: usize, reach: f64) -> Vec<usize> {
    // Along the GROUND, which is the angle between the two directions
    // times the radius, and never the straight line to a point on the
    // mean radius: that is this file's own `cars_near` mistake, where
    // the eye's own ALTITUDE is added to every distance and a town
    // under your feet reads as a kilometre off.
    let mut near: Vec<(usize, f64)> = towns
        .iter()
        .enumerate()
        .map(|(k, t)| (k, t.dir.angle_between(at) * radius))
        .filter(|&(_, d)| d.is_finite() && d <= reach)
        .collect();
    near.sort_by(|a, b| a.1.total_cmp(&b.1));
    near.truncate(count);
    let mut out: Vec<usize> = near.into_iter().map(|(k, _)| k).collect();
    out.sort_unstable();
    out
}

/// Raise one wanted town that is not standing, or drop one standing town
/// that is not wanted. ONE at a time, and the raising on a worker,
/// because a city of a thousand blocks is not a thing a frame does: the
/// port raised on the main thread was 601 ms of one frame.
pub fn stream_towns(
    mut commands: Commands,
    here: Place,
    mut kit: Kit,
    mut fabric: ResMut<Fabric>,
    mut state: ResMut<Building>,
) {
    let world: &Arc<World> = &here.ground.0;
    if let Some((k, task)) = state.raising.as_mut() {
        let k = *k;
        let Some(raise) = check_ready(task) else {
            return;
        };
        state.raising = None;
        let raised = detail::stand(
            &mut commands,
            &mut kit,
            &here.frame,
            here.ground.1,
            k,
            raise,
        );
        let town = &world.towns[k];
        info!(
            "town {k} built {:.0} m off: {} buildings and {} pieces of street in {} tiles",
            town.dir.angle_between(here.eye.0 .0 - here.ground.1) * world.planet.radius,
            town.lots.len(),
            town.pieces.len(),
            raised.tiles.len(),
        );
        fabric.towns.push(raised);
    }
    let at = here.eye.0 .0 - here.ground.1;
    if state
        .looked
        .is_some_and(|was| (was - at).length() < RECHECK)
    {
        return;
    }
    let want = wanted(
        &world.towns,
        world.planet.radius,
        at,
        crate::TOWNS_BUILT,
        crate::TOWNS_REACH,
    );
    let have = fabric.standing();
    // Drop FIRST, so a swap never holds one town over the count while it
    // is in flight.
    if let Some(&k) = have.iter().find(|k| !want.contains(k)) {
        if let Some(slot) = fabric.towns.iter().position(|t| t.town == k) {
            commands.entity(fabric.towns[slot].entity).despawn();
            fabric.towns.swap_remove(slot);
            debug!("town {k} left the eye's reach");
        }
        return;
    }
    let Some(&k) = want.iter().find(|k| !have.contains(k)) else {
        state.looked = Some(at);
        fabric.towns_settled = true;
        return;
    };
    fabric.bypass_change_detection().towns_settled = false;
    state.raising = Some((k, detail::raise(world.clone(), kit.library.0.clone(), k)));
}

/// Where the world is: the eye, the body under it and the render frame.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Place<'w> {
    eye: Res<'w, Eye>,
    ground: Res<'w, Ground>,
    frame: Res<'w, Frame>,
}

#[cfg(test)]
mod tests;
