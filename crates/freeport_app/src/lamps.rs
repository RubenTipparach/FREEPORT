//! The lamps near the eye, as point lights.
//!
//! Every building's lamps are known to the world (`World::lamps`); a few
//! dozen of the nearest are lights, spawned as the eye comes within reach
//! and despawned as it leaves, placed through the origin like a chunk so
//! a rebase moves them too. A city of thousands of lamps is not thousands
//! of lights.

use crate::stream::{Anchored, Frame};
use crate::{Eye, Ground};
use bevy::prelude::*;
use freeport_core::pos::WorldPos;
use std::collections::HashMap;

/// How far a lamp is lit from, metres, and how many at most.
const REACH: f64 = 60.0;
const MOST: usize = 48;
/// Lumens a lamp gives: enough to read under daylight exposure.
const LUMENS: f32 = 250_000.0;

/// Which lamp a light is.
#[derive(Component)]
pub struct Lamp(pub usize);

/// Spawn the lamps within reach of the eye and despawn those out of it.
pub fn light_lamps(
    mut commands: Commands,
    eye: Res<Eye>,
    frame: Res<Frame>,
    ground: Res<Ground>,
    lit: Query<(Entity, &Lamp)>,
    mut last: Local<Option<bevy::math::DVec3>>,
) {
    // Again when the eye has moved, and when the world has: an edit adds
    // or takes back a lamp where the walker stands.
    if !ground.is_changed() && last.is_some_and(|l| (l - eye.0 .0).length() < 4.0) {
        return;
    }
    *last = Some(eye.0 .0);
    let mut near: Vec<(f64, usize)> = ground
        .0
        .lamps
        .iter()
        .enumerate()
        .filter_map(|(i, (at, _))| {
            let d = (*at - eye.0 .0).length();
            (d < REACH).then_some((d, i))
        })
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    near.truncate(MOST);
    let want: HashMap<usize, ()> = near.iter().map(|(_, i)| (*i, ())).collect();
    let mut have: HashMap<usize, Entity> = HashMap::new();
    for (e, lamp) in &lit {
        if want.contains_key(&lamp.0) {
            have.insert(lamp.0, e);
        } else {
            commands.entity(e).despawn();
        }
    }
    for (_, i) in near {
        if have.contains_key(&i) {
            continue;
        }
        let (at, reach) = ground.0.lamps[i];
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
            Lamp(i),
        ));
    }
}
