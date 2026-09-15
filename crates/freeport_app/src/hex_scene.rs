//! Connects the hex experiment to Bevy and refreshes its visible terrain window.

use bevy::ecs::system::SystemParam;
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::tasks::{block_on, futures_lite::future, AsyncComputeTaskPool, Task};
use freeport_core::field::Planet;
use freeport_core::walker::Bounds;
use hex_planet::{hex::HexGrid, PLANET_RADIUS};
use std::time::Instant;

use crate::hex_config::HexConfig;
use crate::hex_mesh::{distant_mesh, patch_mesh, HexGround};
use crate::terrain::{terrain_material, TerrainMaterial};
use crate::{Args, Ground};

type PendingPatch = Task<(HexGround, DVec3, Mesh)>;

#[derive(Resource)]
pub struct HexScene {
    mesh: Handle<Mesh>,
    materials: [Handle<TerrainMaterial>; 2],
    task: Option<PendingPatch>,
    last_eye: DVec3,
}

#[derive(Component)]
pub struct HexPatch;

#[derive(SystemParam)]
pub struct HexAssets<'w> {
    meshes: ResMut<'w, Assets<Mesh>>,
    images: ResMut<'w, Assets<Image>>,
    materials: ResMut<'w, Assets<TerrainMaterial>>,
}

fn planet(config: &HexConfig) -> Planet {
    Planet {
        radius: PLANET_RADIUS,
        relief: config.relief,
        lumps: config.lumps,
        octaves: config.octaves,
        seed: config.seed,
        overhang: 0.0,
        ledge: 0.0,
    }
}

pub fn spawn_hex_world(
    mut commands: Commands,
    mut assets: HexAssets,
    args: Res<Args>,
    config: Res<HexSettings>,
) {
    let started = Instant::now();
    let config = &config.0;
    let eye = args
        .eye
        .map(|v| v.as_dvec3())
        .unwrap_or(DVec3::Y * PLANET_RADIUS);
    let up = eye.normalize_or(DVec3::Y);
    let surface = HexGround {
        grid: HexGrid::new(up, PLANET_RADIUS, config.cell_radius)
            .expect("hex settings were validated at startup"),
        focus: up * PLANET_RADIUS,
        config: config.clone(),
    };
    let ground = Ground {
        planet: planet(config),
        blocks: Vec::new(),
        bounds: Bounds {
            radius: PLANET_RADIUS,
            floor: PLANET_RADIUS - config.relief,
            top: PLANET_RADIUS + config.relief,
        },
        hex: Some(surface.clone()),
    };
    let material = terrain_material(&mut assets.images, &mut assets.materials);
    let mut near = assets
        .materials
        .get(&material)
        .expect("just added terrain material")
        .clone();
    near.extension.params.z = 1.0;
    near.extension.observer = eye.as_vec3().extend(0.0);
    near.extension.bands = Vec4::new(
        config.lod.hex_end as f32,
        config.lod.height_start as f32,
        0.0,
        0.0,
    );
    let mut far = near.clone();
    far.extension.params.z = 2.0;
    let near = assets.materials.add(near);
    let far = assets.materials.add(far);
    commands.spawn((
        Mesh3d(
            assets
                .meshes
                .add(distant_mesh(&ground.planet, config.far_subdivisions)),
        ),
        MeshMaterial3d(far.clone()),
        Transform::default(),
    ));
    let (origin, mesh) = patch_mesh(&surface, &ground.planet);
    let vertices = mesh.count_vertices();
    let mesh = assets.meshes.add(mesh);
    commands.spawn((
        Mesh3d(mesh.clone()),
        MeshMaterial3d(near.clone()),
        Transform::from_translation(origin.as_vec3()),
        HexPatch,
    ));
    crate::spawn_lighting(&mut commands);
    crate::spawn_status(&mut commands, ground.label());
    let top = surface.radius_at(&ground.planet, up);
    let mut camera_args = args.clone();
    if camera_args.eye.is_none() {
        camera_args.eye = Some((up * (top + 4.0)).as_vec3());
        camera_args
            .look
            .get_or_insert((up * (top + 2.0) + DVec3::Z * 20.0).as_vec3());
    }
    crate::spawn_camera(&mut commands, &ground, &camera_args, top);
    commands.insert_resource(ground);
    commands.insert_resource(HexScene {
        mesh,
        materials: [near, far],
        task: None,
        last_eye: eye,
    });
    info!(
        "hex planet: {} km across, {} cap/wall triangles, built in {:.0} ms",
        PLANET_RADIUS * 0.002,
        vertices / 3,
        started.elapsed().as_secs_f64() * 1000.0
    );
}

#[derive(Resource)]
pub struct HexSettings(pub HexConfig);

pub fn stream_hex_patch(
    mut scene: ResMut<HexScene>,
    mut ground: ResMut<Ground>,
    mut assets: HexAssets,
    camera: Query<&Transform, (With<Camera3d>, Without<HexPatch>)>,
    mut patch: Query<&mut Transform, (With<HexPatch>, Without<Camera3d>)>,
) {
    let Ok(camera) = camera.single() else {
        return;
    };
    let eye = camera.translation.as_dvec3();
    if !eye.is_finite() {
        return;
    }
    if let Some(task) = scene.task.as_mut() {
        if let Some((surface, origin, mesh)) = block_on(future::poll_once(task)) {
            if let Some(current) = assets.meshes.get_mut(&scene.mesh) {
                *current = mesh;
            }
            if let Ok(mut transform) = patch.single_mut() {
                transform.translation = origin.as_vec3();
            }
            ground.hex = Some(surface);
            scene.task = None;
        }
    }
    let Some(surface) = ground.hex.as_ref() else {
        return;
    };
    // Keep the smooth sphere visible outside the installed patch during a long flight.
    let covered = (eye.normalize_or(DVec3::Y) * PLANET_RADIUS).distance(surface.focus)
        < surface.config.refresh * 2.0;
    for handle in &scene.materials {
        if let Some(material) = assets.materials.get_mut(handle) {
            material.extension.observer = eye.as_vec3().extend(if covered { 0.0 } else { 1.0 });
        }
    }
    if scene.task.is_some() || eye.distance(scene.last_eye) < surface.config.refresh {
        return;
    }
    if eye.length() - PLANET_RADIUS > surface.config.lod.height_start + surface.config.relief {
        return;
    }
    let mut next = surface.clone();
    let up = eye.normalize_or(DVec3::Y);
    next.focus = up * PLANET_RADIUS;
    // The experiment has local charts; global Goldberg pentagons remain future work.
    if next.grid.cell([0, 0]).centre.distance(next.focus) > next.config.lod.hex_end {
        let Some(grid) = HexGrid::new(up, PLANET_RADIUS, next.config.cell_radius) else {
            return;
        };
        next.grid = grid;
    }
    let shape = planet(&next.config);
    scene.last_eye = eye;
    scene.task = Some(AsyncComputeTaskPool::get().spawn(async move {
        let (origin, mesh) = patch_mesh(&next, &shape);
        (next, origin, mesh)
    }));
}
