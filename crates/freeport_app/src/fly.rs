//! Free flight in the camera's own frame, with a double precision world position.

use crate::{Controls, Eye, OnFoot, Status};
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::math::DVec3;
use bevy::prelude::*;
use freeport_core::pos::WorldPos;
use serde::Deserialize;

#[derive(Resource, Clone, Deserialize)]
#[serde(default)]
pub(crate) struct FlightSettings {
    pub speed: f64,
    boost: f64,
    roll_degrees_per_second: f32,
    min_speed: f64,
    max_speed: f64,
    wheel_factor: f64,
    pub(crate) surface_speed: f64,
    pub(crate) ground_clearance: f64,
}

impl Default for FlightSettings {
    fn default() -> Self {
        Self {
            speed: 6.0,
            boost: 8.0,
            roll_degrees_per_second: 90.0,
            min_speed: 0.25,
            max_speed: 2_000_000.0,
            wheel_factor: 1.6,
            surface_speed: 6.0,
            ground_clearance: 0.5,
        }
    }
}

impl FlightSettings {
    pub fn load() -> Self {
        crate::terrain::assets_dir()
            .and_then(|root| std::fs::read(root.join("config/flight.json")).ok())
            .and_then(|bytes| match serde_json::from_slice::<Self>(&bytes) {
                Ok(settings) => Some(settings),
                Err(error) => {
                    warn!("could not read flight settings: {error}");
                    None
                }
            })
            .unwrap_or_default()
            .validated()
    }

    fn validated(mut self) -> Self {
        let defaults = Self::default();
        for (value, fallback) in [
            (&mut self.speed, defaults.speed),
            (&mut self.boost, defaults.boost),
            (&mut self.min_speed, defaults.min_speed),
            (&mut self.max_speed, defaults.max_speed),
            (&mut self.surface_speed, defaults.surface_speed),
            (&mut self.ground_clearance, defaults.ground_clearance),
        ] {
            if !value.is_finite() || *value <= 0.0 {
                *value = fallback;
            }
        }
        if self.min_speed > self.max_speed {
            self.min_speed = defaults.min_speed;
            self.max_speed = defaults.max_speed;
        }
        self.speed = self.speed.clamp(self.min_speed, self.max_speed);
        if !self.roll_degrees_per_second.is_finite() || self.roll_degrees_per_second <= 0.0 {
            self.roll_degrees_per_second = defaults.roll_degrees_per_second;
        }
        if !self.wheel_factor.is_finite() || self.wheel_factor <= 1.0 {
            self.wheel_factor = defaults.wheel_factor;
        }
        self
    }
}

#[derive(Component)]
pub(crate) struct Fly {
    pub at: DVec3,
    pub rotation: Quat,
    pub speed: f64,
}

impl Fly {
    pub fn new(at: DVec3, forward: DVec3, up: DVec3, speed: f64) -> Self {
        let mut fly = Self {
            at,
            rotation: Quat::IDENTITY,
            speed,
        };
        fly.face(forward, up);
        fly
    }

    /// Also used when leaving the walker, so the view keeps its local horizon.
    pub fn face(&mut self, forward: DVec3, up: DVec3) {
        let forward = forward.as_vec3().normalize_or(Vec3::NEG_Z);
        let side = forward.cross(up.as_vec3().normalize_or(Vec3::Y));
        let right = if side.length_squared() > 1e-8 {
            side.normalize()
        } else {
            // Looking along up has no horizon. Keep the previous right when
            // possible; normalizing a nearly zero cross product skews the view.
            let previous = self.rotation * Vec3::X;
            (previous - forward * previous.dot(forward))
                .try_normalize()
                .unwrap_or_else(|| forward.any_orthonormal_vector())
        };
        let up = right.cross(forward).normalize();
        self.rotation = Quat::from_mat3(&Mat3::from_cols(right, up, -forward)).normalize();
    }

    pub fn forward(&self) -> Vec3 {
        self.rotation * Vec3::NEG_Z
    }

    fn turn(&mut self, look: Vec2, roll: f32) {
        if look.is_finite() && roll.is_finite() {
            // Post-multiply: yaw, pitch and bank are all in the camera's frame.
            self.rotation = (self.rotation
                * Quat::from_rotation_y(-look.x)
                * Quat::from_rotation_x(-look.y)
                * Quat::from_rotation_z(-roll))
            .normalize();
        }
    }

    fn travel(&mut self, local: Vec3, distance: f64) {
        let step = (self.rotation * local.normalize_or_zero()).as_dvec3() * distance;
        if step.is_finite() {
            self.at += step;
        }
    }

    fn change_speed(&mut self, lines: f32, settings: &FlightSettings) {
        if lines.is_finite() {
            self.speed = (self.speed * settings.wheel_factor.powf(lines as f64))
                .clamp(settings.min_speed, settings.max_speed);
        }
    }

    fn level(&mut self, centre: DVec3) {
        self.face(
            self.forward().as_dvec3(),
            (self.at - centre).normalize_or(DVec3::Y),
        );
    }

    fn navigate(
        &mut self,
        keys: &ButtonInput<KeyCode>,
        planets: &mut crate::planets::Planets,
        centre: DVec3,
    ) {
        if planets.bodies.is_empty() {
            return;
        }
        if keys.just_pressed(KeyCode::KeyN) {
            planets.target = (planets.target + 1) % planets.bodies.len();
        }
        if keys.just_pressed(KeyCode::KeyG) {
            let target = planets.bodies[planets.target].centre;
            self.face(target - self.at, (self.at - centre).normalize_or(DVec3::Y));
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct FlightScene<'w> {
    settings: Res<'w, FlightSettings>,
    planets: Option<ResMut<'w, crate::planets::Planets>>,
}

/// A speed a player can read: metres a second on foot and at a landing,
/// kilometres a second between planets. Two million metres a second is a
/// number nobody can hold, and it is the same speed as 2,000 km/s.
fn pace(v: f64) -> String {
    if v.abs() >= 1000.0 {
        format!("{:.1} km/s", v / 1000.0)
    } else {
        format!("{v:.1} m/s")
    }
}

fn wheel_lines(wheel: &MouseWheel) -> f32 {
    match wheel.unit {
        MouseScrollUnit::Line => wheel.y,
        MouseScrollUnit::Pixel => wheel.y / MouseScrollUnit::SCROLL_UNIT_CONVERSION_FACTOR,
    }
}

/// Who has the EYE, if it is not the fly camera: the walker on his own
/// feet, or a stolen car with somebody at the wheel.
///
/// One thing rather than two arguments, because the question `fly` asks
/// is a single one and asking it as two was what put a stolen car's
/// camera back in the air: the first cut read the walker alone, and
/// taking a car takes the walker away.
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct Aboard<'w> {
    on_foot: Option<Res<'w, OnFoot>>,
    thefts: Res<'w, crate::drive::Thefts>,
}

impl Aboard<'_> {
    /// Whether anything but the fly camera is driving the eye.
    pub(crate) fn any(&self) -> bool {
        self.on_foot.is_some() || self.thefts.driving().is_some()
    }
}

/// Consume input even on foot, so changing modes never replays old mouse events.
pub(crate) fn fly(
    mut controls: Controls,
    mut wheel: MessageReader<MouseWheel>,
    aboard: Aboard,
    mut scene: FlightScene,
    mut cam: Query<&mut Fly, With<Camera3d>>,
    mut eye: ResMut<Eye>,
    mut status: ResMut<Status>,
) {
    let look = controls.look();
    let lines: f32 = wheel.read().map(wheel_lines).sum();
    // What the air will actually ALLOW this frame, which is what the
    // readout leads with.
    let mut allowed = f64::INFINITY;
    // On foot OR at the wheel: either way the fly camera is not what
    // owns the eye, and it writes `Eye` every frame, so reading only the
    // walker here is what put the camera back in the air the moment a
    // car was stolen and the walker went away.
    if aboard.any() {
        return;
    }
    let Ok(mut fly) = cam.single_mut() else {
        return;
    };
    let keys = &controls.keys;
    let settings = &scene.settings;
    let mut actual = 0.0;
    let boost = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    if controls.focused() {
        let dt = controls.time.delta_secs_f64().clamp(0.0, 0.1);
        let axis = |neg: KeyCode, pos: KeyCode| {
            (keys.pressed(pos) as i32 - keys.pressed(neg) as i32) as f32
        };
        let roll = axis(KeyCode::KeyQ, KeyCode::KeyE)
            * settings.roll_degrees_per_second.to_radians()
            * dt as f32;
        fly.turn(look, roll);
        let centre = scene
            .planets
            .as_ref()
            .and_then(|p| p.bodies.get(p.nearest(fly.at)))
            .map_or(DVec3::ZERO, |body| body.centre);
        if keys.just_pressed(KeyCode::KeyR) {
            fly.level(centre);
        }
        if let Some(planets) = scene.planets.as_mut() {
            fly.navigate(keys, planets, centre);
        }
        fly.change_speed(lines, settings);
        let vertical = keys.pressed(KeyCode::Space) as i32
            - (keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight)) as i32;
        let local = Vec3::new(
            axis(KeyCode::KeyA, KeyCode::KeyD),
            vertical as f32,
            axis(KeyCode::KeyW, KeyCode::KeyS),
        );
        let speed = fly.speed * if boost { settings.boost } else { 1.0 };
        if let Some(planets) = &scene.planets {
            let direction = (fly.rotation * local.normalize_or_zero())
                .as_dvec3()
                .normalize_or_zero();
            let (at, cap) = planets.advance(
                fly.at,
                direction,
                speed,
                settings.surface_speed,
                settings.ground_clearance,
                dt,
            );
            actual = if dt > 0.0 {
                (at - fly.at).length() / dt
            } else {
                0.0
            };
            allowed = cap;
            fly.at = at;
        } else {
            fly.travel(local, speed * dt);
            actual = speed * f64::from(local != Vec3::ZERO);
        }
    }
    eye.0 = WorldPos(fly.at);
    status.walker = readout(&fly, &scene, actual, allowed, boost);
}

/// What the HUD says about a frame of flight.
///
/// **The speed it leads with is the speed you are GOING.** It used to
/// lead with the WHEEL's own setting, which is a number the air almost
/// never lets you have: near the ground `Planets::advance` tapers a
/// cruise down to `surface_speed`, so the readout said two million
/// metres a second while the ship crawled, and nothing said which of
/// the two was a lie. `advance` already returned the speed it applied
/// and `fly` was throwing it away.
fn readout(fly: &Fly, scene: &FlightScene, actual: f64, allowed: f64, boost: bool) -> String {
    let wheel = fly.speed * if boost { scene.settings.boost } else { 1.0 };
    let held = allowed.min(wheel);
    let limited = held < wheel * 0.999 && held.is_finite();
    let mut out = format!(
        "fly {} | wheel {}{}{} | Q/E roll, Space/Ctrl rise/sink, R level",
        pace(actual),
        pace(fly.speed),
        if boost { " + boost" } else { "" },
        if limited {
            format!(", air holds it to {}", pace(held))
        } else {
            String::new()
        },
    );
    if let Some(planets) = &scene.planets {
        if let (Some(body), Some(target)) = (
            planets.bodies.get(planets.nearest(fly.at)),
            planets.bodies.get(planets.target),
        ) {
            out.push_str(&format!(
                "\n{}: {:.1} km altitude | target {}: {:.1} km | N next planet, G face target",
                body.name,
                body.altitude(fly.at) / 1000.0,
                target.name,
                target.altitude(fly.at).max(0.0) / 1000.0
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests;
