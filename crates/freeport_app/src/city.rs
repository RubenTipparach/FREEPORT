//! The towns as they are DRAWN: one mesh a town, spawned once.
//!
//! A town's buildings and streets are parametric models
//! (`freeport_core::model`), welded into ONE mesh in the town's own frame
//! at startup and never touched again: a town is eighty metres across, so
//! an `f32` in its frame holds a micron, and eight towns are eight draws
//! rather than eight thousand. The entity is placed from the town's own
//! world position through the floating origin, like a chunk, so
//! `rebase_origin` moves it with everything else.
//!
//! They wear the ground's own material, so concrete, plate, glass, a lamp
//! and a street are the same seven the terrain shader already draws, in
//! the same town frames it already maps them in.

use crate::stream::{Anchored, Frame};
use crate::terrain::{to_mesh, TerrainMaterial};
use crate::world::TownMesh;
use bevy::prelude::*;
use freeport_core::pos::WorldPos;

/// A mark on a town's models.
#[derive(Component)]
pub struct Built;

/// Spawn every town's mesh where it stands.
pub fn spawn_towns(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: &Handle<TerrainMaterial>,
    frame: &Frame,
    towns: Vec<TownMesh>,
) {
    for town in towns {
        if town.mesh.indices.is_empty() {
            continue;
        }
        let at = WorldPos(town.frame.world(bevy::math::DVec3::ZERO));
        // The mesh is written east, north and up in the town's frame, so
        // the entity carries that frame's own rotation and the origin
        // carries where it is.
        let basis = Mat3::from_cols(
            town.frame.east.as_vec3(),
            town.frame.north.as_vec3(),
            town.frame.dir.as_vec3(),
        );
        commands.spawn((
            Mesh3d(meshes.add(to_mesh(&town.mesh))),
            MeshMaterial3d(material.clone()),
            Transform {
                translation: frame.0.local(at),
                rotation: Quat::from_mat3(&basis),
                scale: Vec3::ONE,
            },
            Anchored { at },
            Built,
        ));
    }
}
