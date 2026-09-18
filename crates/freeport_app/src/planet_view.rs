//! Whole-body distant surfaces, behind the streamed volumetric terrain.
//!
//! One sphere a body, displaced by that body's own relief and painted from
//! the equirectangular chart `freeport_core::chart` bakes off the same
//! field the chunks are contoured from. `distant` owns the material, the
//! mesh and the levels; this is where a body gets one.

use crate::distant::{self, DistantLod, DistantMaterial};
use crate::planets::Planets;
use crate::stream::Anchored;
use crate::terrain::TerrainMaterial;
use crate::water::{water_material, WaterMaterial};
use bevy::prelude::*;
use freeport_core::chart::Chart;
use freeport_core::pos::WorldPos;
use std::time::Instant;

pub(crate) fn spawn(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<TerrainMaterial>,
    waters: &mut Assets<WaterMaterial>,
    images: &mut Assets<Image>,
    distants: &mut Assets<DistantMaterial>,
    planets: &mut Planets,
) {
    let template = materials
        .get(&planets.bodies[0].material)
        .expect("home terrain material exists at startup")
        .clone();
    // Every body's chart at once. A bake is one `Planet::surface` a texel
    // and the surface is the whole biome model, so four bodies in series
    // is four times a second nobody has to spend: they share nothing, so
    // they go on their own threads.
    let started = Instant::now();
    let charts: Vec<Chart> = std::thread::scope(|scope| {
        let handles: Vec<_> = planets
            .bodies
            .iter()
            .map(|body| {
                let planet = body.world.planet.clone();
                let sea = body.world.sea.radius;
                scope.spawn(move || Chart::bake(&planet, sea, distant::CHART_W, distant::CHART_H))
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("a chart bake cannot panic"))
            .collect()
    });
    info!(
        "{} charts of {} by {} baked in {:.0} ms",
        charts.len(),
        distant::CHART_W,
        distant::CHART_H,
        started.elapsed().as_secs_f64() * 1000.0
    );
    for (index, (body, chart)) in planets.bodies.iter_mut().zip(&charts).enumerate() {
        if index > 0 {
            let mut material = template.clone();
            material.extension.params.z = 0.0;
            material.extension.params.w = body.world.sea.radius as f32;
            material.extension.palette = Vec3::from_array(body.colour).extend(1.0);
            body.material = materials.add(material);
            body.water = water_material(waters, body.world.sea.radius);
        }
        let (albedo, slopes) = distant::images(chart);
        let surface = distants.add(distant::material(images.add(albedo), images.add(slopes)));
        body.distant = surface.clone();
        let built: Vec<Handle<Mesh>> = distant::levels()
            .map(|n| {
                meshes.add(distant::sphere(
                    &body.world.planet,
                    body.world.sea.radius,
                    n,
                ))
            })
            .collect();
        commands.spawn((
            Name::new(format!("{} distant surface", body.name)),
            Mesh3d(built[0].clone()),
            MeshMaterial3d(surface),
            Transform::from_translation(body.centre.as_vec3()),
            Anchored {
                at: WorldPos(body.centre),
            },
            DistantLod {
                meshes: built,
                centre: body.centre,
                radius: body.world.planet.radius,
            },
            bevy::light::NotShadowCaster,
        ));
    }
    if std::env::var("FREEPORT_DUMP_CHARTS").is_ok() {
        for (body, chart) in planets.bodies.iter().zip(&charts) {
            distant::dump(chart, &body.name.to_lowercase());
        }
    }
}

/// Every material has its body's own centre, including the home buildings
/// when another planet supplies the near terrain and atmosphere.
pub(crate) fn recentre(
    planets: Res<Planets>,
    frame: Res<crate::stream::Frame>,
    mut materials: ResMut<Assets<TerrainMaterial>>,
    mut waters: ResMut<Assets<WaterMaterial>>,
    mut distants: ResMut<Assets<DistantMaterial>>,
) {
    for body in &planets.bodies {
        let centre = frame.0.local(WorldPos(body.centre));
        let desired = centre.extend(0.0);
        let changed = materials
            .get(&body.material)
            .is_some_and(|m| m.extension.centre != desired);
        if changed {
            if let Some(material) = materials.get_mut(&body.material) {
                material.extension.centre = desired;
            }
        }
        crate::water::recentre(&mut waters, &body.water, centre);
        // The distant sphere reasons in planet local coordinates too: it
        // is the same tenebris rule, and a chart looked up from a centre
        // a rebase had moved would spin the whole planet under its own
        // coastline.
        let bend = distants
            .get(&body.distant)
            .map_or(0.0, |m| m.extension.centre.w);
        let want = centre.extend(bend);
        let moved = distants
            .get(&body.distant)
            .is_some_and(|m| m.extension.centre != want);
        if moved {
            if let Some(material) = distants.get_mut(&body.distant) {
                material.extension.centre = want;
            }
        }
    }
}
