//! On foot: the keys and the mouse into the core's walker, the eye out.
//!
//! The walker itself is `freeport_core::walker`, the rules and the numbers
//! both, and this is only what a harness adds: which key is which, where
//! the eye is for the camera, a line of text saying where it stands, and a
//! key that swaps between walking and flying. The field the walker walks
//! is the one the world is DRAWN from, so the picture is the collider:
//! `World::underfoot` hands over the same field the mesher contoured.

use crate::{Args, Controls, Eye, Fly, Ground, Status};
use bevy::prelude::*;
use freeport_core::field::{Density, CONCRETE};
use freeport_core::pos::WorldPos;
use freeport_core::walker::{Bounds, Input, Walker};

/// The walker, present while on foot.
#[derive(Resource)]
pub struct OnFoot(pub Walker);

/// What a scripted walk has done so far: frames left, where it started,
/// and the worst a frame of it cost.
#[derive(Default)]
pub struct Scripted {
    left: u32,
    done: u32,
    from: Option<freeport_core::walker::Walker>,
    cost: f64,
}

/// One frame on foot.
pub fn walk(
    mut controls: Controls,
    ground: Res<Ground>,
    args: Res<Args>,
    walker: Option<ResMut<OnFoot>>,
    mut eye: ResMut<Eye>,
    mut status: ResMut<Status>,
    mut script: Local<Scripted>,
) {
    let look = controls.look();
    let Some(mut walker) = walker else {
        return;
    };
    let keys = &controls.keys;
    let axis =
        |neg: KeyCode, pos: KeyCode| (keys.pressed(pos) as i32 - keys.pressed(neg) as i32) as f64;
    let mut input = Input {
        forward: axis(KeyCode::KeyS, KeyCode::KeyW),
        right: axis(KeyCode::KeyA, KeyCode::KeyD),
        run: keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight),
        jump: keys.pressed(KeyCode::Space),
        turn: -look.x as f64,
        tilt: -look.y as f64,
    };
    // A scripted walk holds W down and steps a fixed sixtieth, because the
    // frame's own delta on a software rasteriser is most of a second and a
    // walker that moves five metres a frame measures nothing.
    let mut dt = controls.time.delta_secs_f64();
    if args.walk > 0 && script.left == 0 && script.from.is_none() {
        script.left = args.walk;
        script.from = Some(walker.0.clone());
    }
    if script.left > 0 {
        input = Input {
            forward: 1.0,
            ..Default::default()
        };
        dt = 1.0 / 60.0;
        script.left -= 1;
        script.done += 1;
    }
    let field = ground.0.underfoot(walker.0.eye(), 8.0);
    // The sea holds the feet only where there is water: a dry pit under
    // the level is walked into.
    let water = ground.0.water(&field);
    let wet = water.has_water(walker.0.dir * (walker.0.foot + 0.3));
    let bounds = Bounds {
        sea: if wet { water.sea.radius } else { 0.0 },
        ..ground.0.bounds
    };
    // A clock and never the frame's own delta, which is the thing this is
    // measuring against in the first place.
    let clock = std::time::Instant::now();
    walker.0.update(&field, &bounds, &input, dt);
    eye.0 = WorldPos(ground.1 + walker.0.eye());
    say_walk(
        &mut script,
        &walker.0,
        &ground,
        clock.elapsed().as_secs_f64(),
    );
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

/// What a scripted walk has done: how far it has come along the ground
/// and what a frame of the walker costs, said once a second. A headless
/// run has nobody to press W, and a walker's feel is a number a second
/// person can check rather than a thing to take on trust.
fn say_walk(script: &mut Scripted, w: &Walker, ground: &Ground, cost: f64) {
    let Some(from) = &script.from else {
        return;
    };
    script.cost = script.cost.max(cost);
    if !script.done.is_multiple_of(60) || script.done == 0 {
        return;
    }
    let gone = from.dir.angle_between(w.dir) * ground.0.planet.radius;
    info!(
        "walked {gone:.1} m in {:.0} s, {:.2} m over the mean radius, {:.2} ms a frame at worst{}",
        script.done as f64 / 60.0,
        w.foot - ground.0.planet.radius,
        script.cost * 1e3,
        if w.on_ground { "" } else { ", airborne" },
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
            fly.face(w.0.look(), w.0.dir);
            fly.at = ground.1 + w.0.eye();
            commands.remove_resource::<OnFoot>();
            status.walker = "flying".to_string();
        }
        None => {
            let local = fly.at - ground.1;
            let field = ground.0.underfoot(local, 8.0);
            let heading = fly.forward().as_dvec3();
            let mut walker = Walker::enter(&field, &ground.0.bounds, local, heading);
            let right = (fly.rotation * Vec3::X).as_dvec3();
            walker.fwd = (heading - walker.dir * heading.dot(walker.dir))
                .try_normalize()
                .filter(|_| heading.dot(walker.dir).abs() < 0.999999)
                .unwrap_or_else(|| walker.dir.cross(right).normalize());
            walker.pitch = heading
                .dot(walker.dir)
                .clamp(-1.0, 1.0)
                .asin()
                .clamp(-1.45, 1.45);
            commands.insert_resource(OnFoot(walker));
        }
    }
}
