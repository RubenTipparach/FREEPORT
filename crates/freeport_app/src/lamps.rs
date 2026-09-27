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
pub(crate) const REACH: f64 = 60.0;
/// How many lamps a town's TILE may carry before its index runs into the
/// next tile's. A tile is one block, sixteen lots at the most, and a
/// building hangs one lamp over its door and one a storey.
const PER_TILE: usize = 4096;
const MOST: usize = 48;
/// Lumens a lamp gives at full NIGHT: enough to read under this camera's
/// own daylight exposure. `dim_lamps` is the one writer of it, so how
/// bright a lamp is is decided in one place and by the time of day.
const LUMENS: f32 = 250_000.0;

/// Whether a lamp is a town's or a road's, which is what stops a
/// village's third lamp and a road's third lamp being the same lamp.
#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub enum Of {
    Town,
    Road,
}

/// Which lamp a light is: whose it is, the PLANNED town or the road it
/// belongs to, and its place in that one's own list: along the whole
/// road for a road, and its tile's place times `PER_TILE` and its place
/// in that tile for a town.
///
/// The planned index and not a slot in the built list, because the built
/// list STREAMS: a town going out of range takes its own entry out of
/// the middle and every slot after it would name a different lamp. A
/// road's lamps are indexed along the whole ROAD for the same reason,
/// since a stretch of it streams too.
#[derive(Component, PartialEq, Eq, Hash, Clone, Copy)]
pub struct Lamp(pub Of, pub usize, pub usize);

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
    // A town's lamps are its TILES' own, and only the tiles a body can
    // reach carry theirs: `city::detail` builds them well past `REACH`.
    for town in &fabric.towns {
        for (k, stops) in town.state.iter().enumerate() {
            let Some(stops) = &stops.stops else {
                continue;
            };
            for (i, (at, reach)) in stops.lamps.iter().enumerate() {
                let d = (*at + ground.1 - eye.0 .0).length();
                if d < REACH {
                    near.push((d, Lamp(Of::Town, town.town, k * PER_TILE + i), *at, *reach));
                }
            }
        }
    }
    // And the lamps on the approaches to a town, which stand along the
    // ROAD rather than in anybody's street.
    for verge in &fabric.verges {
        for (i, at, reach) in &verge.lamps {
            let d = (*at + ground.1 - eye.0 .0).length();
            if d < REACH {
                near.push((d, Lamp(Of::Road, verge.which.0, *i), *at, *reach));
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
                // Nought here and the number is `dim_lamps`'s: this
                // system decides WHICH lamps exist and that one decides
                // how hard they burn, so a lamp spawned at noon does not
                // arrive lit for the one frame before the clock is read.
                intensity: 0.0,
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

/// How hard the lamps burn, every frame, off the same terminator the
/// ground's own shader reads (`freeport_core::day::daylight`): they come
/// on as the sun goes down and are out by day.
///
/// A system of its own rather than an eighth argument on `light_lamps`,
/// which is this project's own "a struct is missing" smell answered by
/// noticing there are TWO jobs here: which lamps exist is a question
/// about where the eye is, and how bright they are is a question about
/// what time it is. It also means a lamp dims through dusk on its own
/// rather than waiting for the eye to move the four metres `light_lamps`
/// holds its set still for.
pub fn dim_lamps(weather: Res<crate::sky::Weather>, mut lit: Query<&mut PointLight, With<Lamp>>) {
    let burn = weather.lamplight() as f32 * LUMENS;
    for mut light in &mut lit {
        light.intensity = burn;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use freeport_core::day;

    fn weather(sun: DVec3, here: DVec3) -> crate::sky::Weather {
        crate::sky::Weather {
            air: freeport_core::atmos::Air::round(1_000_000.0, 8_000.0),
            sea: 1_000_000.0,
            sun,
            noon: sun,
            start: 0.0,
            now: 0.0,
            day: day::DAY,
            here,
            frozen: false,
        }
    }

    /// The lamps are OUT at noon and hard on at midnight, which is the
    /// whole of what a day costs a city: it was a street lit at noon.
    /// Driven through the real system, because the number is the
    /// weather's and the wiring is where an app side bug would be.
    #[test]
    fn a_street_lamp_is_out_at_noon_and_lit_at_midnight() {
        let here = DVec3::new(0.2, 0.3, 0.93).normalize();
        let noon = DVec3::new(0.9, 0.1, 0.42).normalize();
        for (hour, want) in [(12.0, 0.0), (0.0, 1.0)] {
            let sun = day::sun_at(noon, day::at_oclock(noon, here, hour, day::DAY), day::DAY);
            let mut app = App::new();
            app.insert_resource(weather(sun, here))
                .add_systems(Update, dim_lamps);
            let lamp = app
                .world_mut()
                .spawn((PointLight::default(), Lamp(Of::Town, 0, 0)))
                .id();
            app.update();
            let lit = app
                .world()
                .get::<PointLight>(lamp)
                .expect("the lamp")
                .intensity;
            assert!(
                (lit - want * LUMENS).abs() < LUMENS * 1e-3,
                "at {hour} o'clock the lamp burns {lit} and should burn {}",
                want * LUMENS
            );
        }
    }
}

/// A TORCH the player carries, and a car's HEAD LAMPS: the two lights in
/// this world that are carried rather than standing somewhere.
///
/// They are lights rather than the emissives they were beside, and that
/// is the whole difference: a head lamp drawn as an emissive is a bright
/// PIXEL that lights nothing, so a night drive past `road::LIT_NEAR` was
/// a drive in the dark with two glowing patches on the bonnet. This file
/// already names that gap.
#[derive(Component)]
pub struct Torch;

/// A car's own head lamps, on the STOLEN car and never on the rails
/// traffic. `light_lamps` keeps forty eight of a body's thousands of
/// street lamps for the same reason: a light near the eye and a number
/// everywhere else, and two shadowing spots on every town car is dozens
/// of lights for cars nobody is looking at.
#[derive(Component)]
pub struct Headlamp;

/// How far a torch throws, and how wide. A metre and a half of hot spot
/// at ten metres is what a hand torch does, which is `TORCH_INNER`.
pub const TORCH_REACH: f32 = 45.0;
pub const TORCH_INNER: f32 = 0.10;
pub const TORCH_OUTER: f32 = 0.38;
pub const TORCH_COLOUR: Color = Color::srgb(1.0, 0.97, 0.90);
const TORCH_LUMENS: f32 = 90_000.0;

/// How far a head lamp throws. Further and narrower than a torch,
/// because that is what a head lamp is for: seeing the road at 160 km/h
/// rather than the ground at your feet. At 44.4 m/s this is under three
/// seconds of stopping sight, which is the honest number and is why a
/// night drive still wants the lit approaches.
pub const HEAD_REACH: f32 = 120.0;
const HEAD_LUMENS: f32 = 400_000.0;

/// The torch is a TOGGLE and the head lamps are the CLOCK, and that is
/// the difference the owner asked for: a torch is on when you want it,
/// at noon or at midnight, and head lamps come on because the sun went
/// down.
#[derive(Resource, Default)]
pub struct TorchOn(pub bool);

/// T turns the torch on and off, and it only burns ON FOOT: a torch
/// carried by a walker who is at the wheel or flying is a light with
/// nobody holding it.
pub fn hold_torch(
    keys: Res<ButtonInput<KeyCode>>,
    on_foot: Option<Res<crate::walk::OnFoot>>,
    mut on: ResMut<TorchOn>,
    mut lit: Query<&mut SpotLight, With<Torch>>,
) {
    if keys.just_pressed(KeyCode::KeyT) {
        on.0 = !on.0;
    }
    let burn = if on.0 && on_foot.is_some() {
        TORCH_LUMENS
    } else {
        0.0
    };
    for mut light in &mut lit {
        light.intensity = burn;
    }
}

/// A car's head lamps come on when the sun goes down, off the SAME
/// `day::twilight` the street lamps read (`sky::Weather::lamplight`), so
/// a town and the car driving through it light up together rather than
/// on two clocks that have to agree.
pub fn light_headlamps(
    weather: Res<crate::sky::Weather>,
    mut lit: Query<&mut SpotLight, With<Headlamp>>,
) {
    let burn = weather.lamplight() as f32 * HEAD_LUMENS;
    for mut light in &mut lit {
        light.intensity = burn;
    }
}
