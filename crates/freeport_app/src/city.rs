//! Town meshes baked by Blender, batched per town and selected by distance.
//! Collision always uses the full bake, independent of the visible LOD.

use crate::stream::{Anchored, Frame};
use crate::terrain::{to_mesh_filtered, TerrainMaterial};
use crate::world::TownMesh;
use crate::Eye;
use bevy::math::DVec3;
use bevy::prelude::*;
use freeport_core::field::GLASS;
use freeport_core::pos::WorldPos;

#[derive(Component)]
pub struct Built {
    meshes: [Handle<Mesh>; 3],
    empty: [bool; 3],
    level: usize,
    radius: f64,
}

pub fn spawn_towns(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: &Handle<TerrainMaterial>,
    frame: &Frame,
    sea: f64,
    towns: Vec<TownMesh>,
    standard: &mut Assets<StandardMaterial>,
) {
    let glass = standard.add(StandardMaterial {
        base_color: Color::srgba(0.55, 0.72, 0.78, 0.18),
        perceptual_roughness: 0.12,
        reflectance: 0.5,
        alpha_mode: AlphaMode::Blend,
        cull_mode: None,
        double_sided: true,
        ..default()
    });
    for town in towns {
        let at = WorldPos(town.frame.world(DVec3::ZERO));
        let basis = Mat3::from_cols(
            town.frame.east.as_vec3(),
            town.frame.north.as_vec3(),
            town.frame.dir.as_vec3(),
        );
        let transform = Transform {
            translation: frame.0.local(at),
            rotation: Quat::from_mat3(&basis),
            scale: Vec3::ONE,
        };
        let radius = town.meshes[0]
            .positions
            .iter()
            .map(|&p| Vec3::from(p).length() as f64)
            .fold(0.0, f64::max);
        for glazing in [false, true] {
            let empty = std::array::from_fn(|lod| {
                !town.meshes[lod]
                    .materials
                    .iter()
                    .any(|&m| (m == GLASS) == glazing)
            });
            if empty.iter().all(|&e| e) {
                continue;
            }
            let handles = std::array::from_fn(|lod| {
                let place = |p: Vec3| (p, (town.frame.world(p.as_dvec3()).length() - sea) as f32);
                let mut mesh =
                    to_mesh_filtered(&town.meshes[lod], place, |m| (m == GLASS) == glazing);
                // Vertex color is the terrain shader's material/mapping
                // payload, not a tint for StandardMaterial glass.
                if glazing {
                    mesh.remove_attribute(Mesh::ATTRIBUTE_COLOR);
                }
                meshes.add(mesh)
            });
            let mut entity = commands.spawn((
                Mesh3d(handles[0].clone()),
                transform,
                Anchored { at },
                Built {
                    meshes: handles,
                    empty,
                    level: 0,
                    radius,
                },
            ));
            if glazing {
                entity.insert((MeshMaterial3d(glass.clone()), bevy::light::NotShadowCaster));
            } else {
                entity.insert(MeshMaterial3d(material.clone()));
            }
        }
    }
}

fn select_lod(current: usize, distance: f64, tuning: &crate::tuning::Tuning) -> usize {
    let distances = [tuning.building_lod_near, tuning.building_lod_far];
    let mut level = current;
    while level < 2 && distance > distances[level] * (1.0 + tuning.lod_hysteresis) {
        level += 1;
    }
    while level > 0 && distance < distances[level - 1] * (1.0 - tuning.lod_hysteresis) {
        level -= 1;
    }
    level
}

pub fn update_lod(
    eye: Res<Eye>,
    tuning: Res<crate::tuning::Tuning>,
    mut towns: Query<(&Anchored, &mut Built, &mut Mesh3d, &mut Visibility)>,
) {
    for (anchor, mut built, mut mesh, mut visibility) in &mut towns {
        let distance = ((anchor.at.0 - eye.0 .0).length() - built.radius).max(0.0);
        let lod = select_lod(built.level, distance, &tuning);
        if lod == built.level {
            continue;
        }
        mesh.0 = built.meshes[lod].clone();
        *visibility = if built.empty[lod] {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        built.level = lod;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lod_has_hysteresis_and_handles_teleports() {
        assert_eq!(select_lod(0, 260.0, &default()), 0);
        assert_eq!(select_lod(1, 240.0, &default()), 1);
        assert_eq!(select_lod(0, 5000.0, &default()), 2);
        assert_eq!(select_lod(2, 0.0, &default()), 0);
    }
}
