//! On foot: the keys and the mouse into the core's walker, the eye out.
//!
//! The walker itself is `freeport_core::walker`, the rules and the numbers
//! both, and this is only what a harness adds: which key is which, where
//! the eye is for the camera, a line of text saying where it stands, and a
//! key that swaps between walking and flying. The field the walker walks
//! is the same one the mesher contoured, so the picture is the collider.

use crate::{Controls, Eye, Fly, Ground, Status};
use bevy::prelude::*;
use freeport_core::field::{Density, CONCRETE};
use freeport_core::pos::WorldPos;
use freeport_core::walker::{Bounds, Input, Walker};

/// The walker, present while on foot.
#[derive(Resource)]
pub struct OnFoot(pub Walker);

/// One frame on foot.
pub fn walk(
    mut controls: Controls,
    ground: Res<Ground>,
    walker: Option<ResMut<OnFoot>>,
    mut eye: ResMut<Eye>,
    mut status: ResMut<Status>,
) {
    let Some(mut walker) = walker else {
        return;
    };
    let look = controls.look();
    let keys = &controls.keys;
    let axis =
        |neg: KeyCode, pos: KeyCode| (keys.pressed(pos) as i32 - keys.pressed(neg) as i32) as f64;
    let input = Input {
        forward: axis(KeyCode::KeyS, KeyCode::KeyW),
        right: axis(KeyCode::KeyA, KeyCode::KeyD),
        run: keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight),
        jump: keys.pressed(KeyCode::Space),
        turn: -look.x as f64,
        tilt: -look.y as f64,
    };
    let field = ground.0.field_near(walker.0.eye(), 8.0);
    // The sea holds the feet only where there is water: a dry pit under
    // the level is walked into.
    let water = ground.0.water(&field);
    let wet = water.has_water(walker.0.dir * (walker.0.foot + 0.3));
    let bounds = Bounds {
        sea: if wet { water.sea.radius } else { 0.0 },
        ..ground.0.bounds
    };
    walker
        .0
        .update(&field, &bounds, &input, controls.time.delta_secs_f64());
    eye.0 = WorldPos(walker.0.eye());
    let w = &walker.0;
    let under = field.material(w.dir * (w.foot - 0.05));
    status.walker = format!(
        "{:.1} m over the mean radius, {:.1} m/s{}, {}",
        w.foot - ground.0.planet.radius,
        w.vel[0].hypot(w.vel[1]),
        if w.on_ground { "" } else { ", airborne" },
        if wet && w.foot < water.sea.radius - 0.3 {
            "in the sea"
        } else if under == CONCRETE {
            "on concrete"
        } else {
            "on the ground"
        }
    );
}

/// F swaps between walking and flying: on foot from wherever the camera
/// is, facing where it faces; in the air from the eye, looking where the
/// walker looked.
pub fn toggle_walk(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    ground: Res<Ground>,
    walker: Option<Res<OnFoot>>,
    mut cam: Query<&mut Fly, With<Camera3d>>,
    mut status: ResMut<Status>,
) {
    if !keys.just_pressed(KeyCode::KeyF) {
        return;
    }
    let Ok(mut fly) = cam.single_mut() else {
        return;
    };
    match walker {
        Some(w) => {
            let look = w.0.look().as_vec3();
            fly.yaw = (-look.x).atan2(-look.z);
            fly.pitch = look.y.clamp(-1.0, 1.0).asin();
            fly.at = w.0.eye();
            commands.remove_resource::<OnFoot>();
            status.walker = "flying".to_string();
        }
        None => {
            let field = ground.0.field_near(fly.at, 8.0);
            let heading = fly.forward().as_dvec3();
            commands.insert_resource(OnFoot(Walker::enter(
                &field,
                &ground.0.bounds,
                fly.at,
                heading,
            )));
        }
    }
}
