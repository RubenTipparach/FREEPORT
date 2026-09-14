//! On foot: the keys and the mouse into the core's walker, the camera out.
//!
//! The walker itself is `freeport_core::walker`, the rules and the numbers
//! both, and this is only what a harness adds: which key is which, a
//! camera at the eye looking where the walker looks, a line of text saying
//! where it stands, and a key that swaps between walking and flying. The
//! field the walker walks is the same `Built` the mesher contoured, so the
//! picture is the collider.

use crate::{Controls, Fly, Ground};
use bevy::prelude::*;
use freeport_core::field::{Density, CONCRETE};
use freeport_core::walker::{Input, Walker};

/// The walker, present while on foot.
#[derive(Resource)]
pub struct OnFoot(pub Walker);

/// The line of text that says where the walker stands.
#[derive(Component)]
pub struct Stat;

/// The camera set from the walker: at the eye, looking along the look,
/// with the local up as up.
pub fn place_camera(w: &Walker, tf: &mut Transform) {
    *tf = Transform::from_translation(w.eye().as_vec3())
        .looking_to(w.look().as_vec3(), w.dir.as_vec3());
}

/// One frame on foot.
pub fn walk(
    mut controls: Controls,
    ground: Res<Ground>,
    walker: Option<ResMut<OnFoot>>,
    mut cam: Query<&mut Transform, With<Camera3d>>,
    mut stat: Query<&mut Text, With<Stat>>,
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
    let field = ground.field();
    walker.0.update(
        &field,
        &ground.bounds,
        &input,
        controls.time.delta_secs_f64(),
    );
    if let Ok(mut tf) = cam.single_mut() {
        place_camera(&walker.0, &mut tf);
    }
    if let Ok(mut text) = stat.single_mut() {
        let w = &walker.0;
        let under = field.material(w.dir * (w.foot - 0.05));
        text.0 = format!(
            "{:.1} m over the mean radius, {:.1} m/s{}, on {}   |   F fly, Tab wire, Esc mouse",
            w.foot - ground.planet.radius,
            w.vel[0].hypot(w.vel[1]),
            if w.on_ground { "" } else { ", airborne" },
            if under == CONCRETE {
                "concrete"
            } else {
                "the ground"
            }
        );
    }
}

/// F swaps between walking and flying: on foot from wherever the camera
/// is, facing where it faces; in the air from the eye, looking where the
/// walker looked.
pub fn toggle_walk(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    ground: Res<Ground>,
    walker: Option<Res<OnFoot>>,
    mut cam: Query<(&Transform, &mut Fly), With<Camera3d>>,
    mut stat: Query<&mut Text, With<Stat>>,
) {
    if !keys.just_pressed(KeyCode::KeyF) {
        return;
    }
    let Ok((tf, mut fly)) = cam.single_mut() else {
        return;
    };
    match walker {
        Some(w) => {
            let look = w.0.look().as_vec3();
            fly.yaw = (-look.x).atan2(-look.z);
            fly.pitch = look.y.clamp(-1.0, 1.0).asin();
            commands.remove_resource::<OnFoot>();
            if let Ok(mut text) = stat.single_mut() {
                text.0 = "flying   |   F walk, Tab wire, Esc mouse".to_string();
            }
        }
        None => {
            let field = ground.field();
            let dir = tf.translation.as_dvec3();
            let heading = tf.forward().as_vec3().as_dvec3();
            commands.insert_resource(OnFoot(Walker::enter(&field, &ground.bounds, dir, heading)));
        }
    }
}
