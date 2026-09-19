//! Landable bodies, their local frames, and continuous interplanetary flight.

use crate::terrain::TerrainMaterial;
use crate::water::WaterMaterial;
use crate::{Ground, World};
use bevy::math::DVec3;
use bevy::prelude::*;
use freeport_core::atmos::Air;
use freeport_core::field::{Density, Planet};
use freeport_core::flight::{approach_speed, entry_distance, sweep_planet};
use freeport_core::walker::Bounds;
use freeport_core::water::Sea;
use serde::Deserialize;
use std::sync::Arc;

/// The OTHER bodies of the system, read off `assets/config/planets.json`.
///
/// **It ships EMPTY, and the reason is a field nothing reads.** It
/// carried Ember, Pelagos and Rime, and the owner's picture of the
/// system showed four bodies that were plainly copies of one another:
/// green continents on a blue ocean at three sizes. They are copies.
/// Every body is the same `biome::Shape` with a different seed and
/// radius, and the one thing in the file that was supposed to tell them
/// apart, `colour`, reaches `DistantMaterial`'s `palette` uniform and
/// is read by NOTHING: `distant.wgsl` paints a body from its chart, and
/// a chart is baked off `Kind::colour`, which is one table for every
/// world. A rust red planet and an ice planet drew the same greens.
///
/// So the copies are gone rather than reskinned, which is the owner's
/// own word for them. The file stays as the extension point it always
/// was, and a body goes back in it the day a body can differ: what that
/// wants is a per body palette the CHART is baked through, not a
/// uniform beside it.
#[derive(Deserialize)]
struct Definition {
    name: String,
    centre: [f64; 3],
    radius: f64,
    relief: f64,
    sea_offset: f64,
    seed: u32,
    colour: [f32; 3],
}

pub(crate) struct Body {
    pub name: String,
    pub centre: DVec3,
    pub world: Arc<World>,
    pub air: Air,
    pub colour: [f32; 3],
    pub material: Handle<TerrainMaterial>,
    pub water: Handle<WaterMaterial>,
    /// The body's own distant surface, which reasons in planet local
    /// coordinates like the other two and is moved by the same rebase.
    pub distant: Handle<crate::distant::DistantMaterial>,
}

#[derive(Resource, Default)]
pub(crate) struct Planets {
    pub bodies: Vec<Body>,
    pub active: usize,
    pub target: usize,
}

impl Planets {
    pub fn load(home: Arc<World>) -> Self {
        let mut bodies = vec![Body {
            name: "Freeport".into(),
            centre: DVec3::ZERO,
            air: Air::round(home.planet.radius, home.planet.relief),
            world: home,
            colour: [0.26, 0.43, 0.19],
            material: default(),
            water: default(),
            distant: default(),
        }];
        let fallback = include_str!("../../../assets/config/planets.json").to_owned();
        let source = crate::terrain::assets_dir()
            .and_then(|p| std::fs::read_to_string(p.join("config/planets.json")).ok())
            .unwrap_or_else(|| fallback.clone());
        let definitions = serde_json::from_str::<Vec<Definition>>(&source).unwrap_or_else(|e| {
            warn!("invalid planets.json: {e}; using bundled planets");
            serde_json::from_str(&fallback).expect("bundled planet definitions are valid")
        });
        for definition in definitions {
            if let Some(body) = Body::from_definition(definition) {
                let separate = bodies.iter().all(|other| {
                    (body.centre - other.centre).length() > body.air.top + other.air.top
                });
                if separate {
                    bodies.push(body);
                } else {
                    warn!("overlapping planet {} skipped", body.name);
                }
            }
        }
        Self {
            target: usize::from(bodies.len() > 1),
            bodies,
            active: 0,
        }
    }

    pub fn nearest(&self, at: DVec3) -> usize {
        self.bodies
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.altitude(at).total_cmp(&b.altitude(at)))
            .map_or(0, |(i, _)| i)
    }

    /// Subdivide on atmosphere entry and by remaining altitude, so even a
    /// long space-speed frame cannot skip the deceleration region.
    pub fn advance(
        &self,
        from: DVec3,
        direction: DVec3,
        requested: f64,
        minimum: f64,
        clearance: f64,
        dt: f64,
    ) -> (DVec3, f64) {
        let mut at = from;
        let mut left = dt.clamp(0.0, 0.1);
        let mut speed = requested;
        for body in &self.bodies {
            at = body.recover(at, clearance);
        }
        if direction.length_squared() < 0.5 {
            return (at, 0.0);
        }
        for _ in 0..256 {
            if left <= 1e-9 {
                break;
            }
            speed = requested;
            let mut distance = f64::INFINITY;
            for body in &self.bodies {
                let local = at - body.centre;
                let shell = body.air.top;
                let entry = entry_distance(local, direction, shell);
                if entry > 1e-5 {
                    distance = distance.min(entry);
                }
                if local.length() <= shell + 1e-5 {
                    let height = (-body.world.planet.at(local) - clearance).max(0.0);
                    let thickness = (shell - (local.length() - height)).max(1.0);
                    speed = speed.min(approach_speed(requested, minimum, height, thickness));
                    distance = distance.min((height * 0.1).max(clearance));
                }
            }
            let tick = left.min(distance / speed.max(1e-9));
            let desired = at + direction * speed * tick;
            let mut end = desired;
            for body in &self.bodies {
                end = body.stop(at, end, clearance);
            }
            let blocked = (end - desired).length_squared() > 1e-10;
            at = end;
            left -= tick;
            if blocked {
                break;
            }
        }
        (at, speed)
    }
}

impl Body {
    fn from_definition(d: Definition) -> Option<Self> {
        let centre = DVec3::from_array(d.centre);
        if !centre.is_finite()
            || !d.radius.is_finite()
            || d.radius < 1000.0
            || !d.relief.is_finite()
            || !(0.0..d.radius * 0.1).contains(&d.relief)
            || !d.sea_offset.is_finite()
            || d.sea_offset.abs() > d.radius * 0.1
        {
            warn!("invalid planet {} skipped", d.name);
            return None;
        }
        let planet = Planet {
            radius: d.radius,
            relief: d.relief,
            seed: d.seed,
            octaves: crate::OCTAVES,
            overhang: 3.0,
            ledge: 12.0,
            ..default()
        };
        let (floor, top) = planet.band();
        let air = Air::round(d.radius, d.relief);
        Some(Self {
            name: d.name,
            centre,
            air,
            colour: d.colour,
            material: default(),
            water: default(),
            distant: default(),
            world: Arc::new(World {
                planet,
                towns: vec![],
                roads: vec![],
                bounds: Bounds {
                    radius: d.radius,
                    floor: floor - 2.0,
                    top: top + 40.0,
                    sea: 0.0,
                },
                sea: Sea {
                    radius: d.radius + d.sea_offset,
                },
            }),
        })
    }

    pub fn altitude(&self, at: DVec3) -> f64 {
        (at - self.centre).length() - self.world.planet.radius
    }

    fn recover(&self, at: DVec3, clearance: f64) -> DVec3 {
        let local = at - self.centre;
        let planet = &self.world.planet;
        let radius = local.length();
        let outside_radius = planet.band().1 + clearance + 1.0;
        if radius > outside_radius {
            return at;
        }
        let density = planet.at(local);
        if density < -clearance - 0.0001 {
            return at;
        }
        // Only invalid starting positions use recovery. Normal travel is swept.
        // Bracket nearby air first: a contact needs centimetres of correction,
        // not a sweep through kilometres of empty air from the relief envelope.
        let direction = local.normalize_or(DVec3::Y);
        let reach = (outside_radius - radius).max(0.0);
        let mut distance = (density + clearance + 0.02).max(0.02).min(reach);
        let mut outside = local + direction * distance;
        while distance < reach && planet.at(outside) >= -clearance - 0.01 {
            distance = (distance * 2.0).min(reach);
            outside = local + direction * distance;
        }
        self.centre + sweep_planet(planet, outside, local, clearance + 0.01)
    }

    fn stop(&self, from: DVec3, to: DVec3, clearance: f64) -> DVec3 {
        let local = from - self.centre;
        let delta = to - from;
        let length = delta.length();
        if length == 0.0 {
            return to;
        }
        let entry = entry_distance(
            local,
            delta / length,
            self.world.planet.band().1 + clearance + 1.0,
        );
        if entry >= length {
            return to;
        }
        let start = local + delta / length * entry;
        let end = sweep_planet(&self.world.planet, start, to - self.centre, clearance);
        self.centre + end
    }
}

/// Switch the local terrain frame in space, with hysteresis at the midpoint.
pub(crate) fn activate(
    mut commands: Commands,
    eye: Res<crate::Eye>,
    mut planets: ResMut<Planets>,
    mut ground: ResMut<Ground>,
    mut streamer: ResMut<crate::stream::Streamer>,
    mut weather: ResMut<crate::sky::Weather>,
    on_foot: Option<Res<crate::OnFoot>>,
) {
    if on_foot.is_some() {
        return;
    }
    let next = planets.nearest(eye.0 .0);
    let previous = &planets.bodies[planets.active];
    let body = &planets.bodies[next];
    if next == planets.active || body.altitude(eye.0 .0) >= previous.altitude(eye.0 .0) * 0.85 {
        return;
    }
    streamer.change_body(&mut commands, eye.0 .0, body);
    *ground = Ground(body.world.clone(), body.centre);
    weather.air = body.air;
    weather.sea = body.world.sea.radius;
    info!("approaching {} at {:.0}", body.name, body.centre);
    planets.active = next;
}

#[cfg(test)]
mod tests;
