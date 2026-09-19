//! The lamps near the eye, as point lights.
//!
//! Every building's lamps are known to the world (`World::lamps`); a few
//! dozen of the nearest are lights, spawned as the eye comes within reach
//! and despawned as it leaves, placed through the origin like a chunk so
//! a rebase moves them too. A city of thousands of lamps is not thousands
//! of lights.

use crate::stream::{Anchored, Frame};
use crate::world::Fabric;
use crate::{Eye, Ground};
use bevy::math::DVec3;
use bevy::prelude::*;
use freeport_core::pos::WorldPos;
use std::collections::HashMap;

/// How far a lamp is lit from, metres, and how many at most.
const REACH: f64 = 60.0;
const MOST: usize = 48;
/// Lumens a lamp gives: enough to read under daylight exposure.
const LUMENS: f32 = 250_000.0;

/// Which lamp a light is: the PLANNED town it belongs to and its place
/// in that town's own list.
///
/// The planned index and not a slot in the built list, because the built
/// list STREAMS: a town going out of range takes its own entry out of
/// the middle and every slot after it would name a different lamp.
#[derive(Component, PartialEq, Eq, Hash, Clone, Copy)]
pub struct Lamp(pub usize, pub usize);

/// Spawn the lamps within reach of the eye and despawn those out of it.
pub fn light_lamps(
    mut commands: Commands,
    eye: Res<Eye>,
    frame: Res<Frame>,
    ground: Res<Ground>,
    fabric: Res<Fabric>,
    lit: Query<(Entity, &Lamp)>,
    mut last: Local<Option<bevy::math::DVec3>>,
) {
    // Again when the eye has moved, and when what is BUILT has: a town
    // coming into range brings its own lamps with it.
    if !fabric.is_changed() && last.is_some_and(|l| (l - eye.0 .0).length() < 4.0) {
        return;
    }
    *last = Some(eye.0 .0);
    let mut near: Vec<(f64, Lamp, DVec3, f64)> = Vec::new();
    for town in &fabric.towns {
        for (i, (at, reach)) in town.lamps.iter().enumerate() {
            let d = (*at + ground.1 - eye.0 .0).length();
            if d < REACH {
                near.push((d, Lamp(town.town, i), *at, *reach));
            }
        }
    }
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    near.truncate(MOST);
    let want: HashMap<Lamp, ()> = near.iter().map(|(_, l, _, _)| (*l, ())).collect();
    let mut have: HashMap<Lamp, Entity> = HashMap::new();
    for (e, lamp) in &lit {
        if want.contains_key(lamp) {
            have.insert(*lamp, e);
        } else {
            commands.entity(e).despawn();
        }
    }
    for (_, which, at, reach) in near {
        if have.contains_key(&which) {
            continue;
        }
        let at = ground.1 + at;
        commands.spawn((
            PointLight {
                intensity: LUMENS,
                range: reach as f32,
                radius: 0.25,
                color: Color::srgb(1.0, 0.93, 0.8),
                shadows_enabled: false,
                ..default()
            },
            Transform::from_translation(frame.0.local(WorldPos(at))),
            Anchored { at: WorldPos(at) },
            which,
        ));
    }
}
