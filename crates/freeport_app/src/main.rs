//! freeport_app: the Bevy harness. It draws what `freeport_core` says.
//!
//! This is the first frame of the prototype and no more: one chunk of a
//! planetoid marched by the core, a light, a camera. The floating origin,
//! the chunk streamer, the ship and the walker arrive in the stages
//! `CLAUDE.md` lays out, each behind its own mockup.

use bevy::asset::RenderAssetUsages;
use bevy::math::DVec3;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use freeport_core::field::{sample, Planet};
use freeport_core::march::march;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_systems(Startup, spawn_planetoid)
        .run();
}

/// The planetoid the first frame shows, small enough for one chunk: its
/// radius, and the lattice it is marched on. A metre a cell over 96 cells
/// holds a 40 m ball with room for its relief.
const RADIUS: f64 = 40.0;
const CELLS: usize = 96;
const CELL: f64 = 1.0;

fn spawn_planetoid(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let planet = Planet {
        radius: RADIUS,
        relief: 6.0,
        lumps: 3.0,
        octaves: 5,
        overhang: 1.5,
        ledge: 4.0,
        seed: 7,
    };
    let half = CELLS as f64 * CELL * 0.5;
    let grid = sample(&planet, DVec3::splat(-half), CELL, CELLS);
    let chunk = march(&grid, 0.0);
    info!(
        "planetoid: {} triangles, {} vertices",
        chunk.triangles(),
        chunk.positions.len()
    );
    let mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, chunk.positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, chunk.normals)
    .with_inserted_indices(Indices::U32(chunk.indices));
    commands.spawn((
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.46, 0.42, 0.37),
            perceptual_roughness: 0.92,
            ..default()
        })),
        Transform::from_translation(Vec3::splat(-half as f32)),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 25_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.9, 0.5, 0.0)),
    ));
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 45.0, 130.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}
