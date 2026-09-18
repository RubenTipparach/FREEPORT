//! Town meshes baked by Blender, batched per town and selected by distance.
//! Collision always uses the full bake, independent of the visible LOD.

use crate::stream::{Anchored, Frame};
use crate::terrain::{to_mesh_filtered, TerrainMaterial, Vertex};
use crate::world::TownMesh;
use crate::Eye;
use bevy::asset::RenderAssetUsages;
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::NoAutoAabb;
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
        let Some(bounds) = lod_bounds(&town.meshes) else {
            continue;
        };
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
                let place = |p: Vec3| {
                    Vertex::built(p, (town.frame.world(p.as_dvec3()).length() - sea) as f32)
                };
                let mut mesh =
                    to_mesh_filtered(&town.meshes[lod], place, |m| (m == GLASS) == glazing);
                // Vertex color is the terrain shader's material/mapping
                // payload, not a tint for StandardMaterial glass.
                if glazing {
                    mesh.remove_attribute(Mesh::ATTRIBUTE_COLOR);
                }
                // Bounds and collision already live outside these immutable
                // buffers. Move vertex data to the renderer without retaining
                // or cloning a CPU copy for every baked town LOD.
                mesh.asset_usage = RenderAssetUsages::RENDER_WORLD;
                meshes.add(mesh)
            });
            let mut entity = commands.spawn((
                Mesh3d(handles[0].clone()),
                transform,
                bounds,
                NoAutoAabb,
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

/// All three LODs share this local-space bound, independent of material updates.
fn lod_bounds(lods: &[freeport_core::dc::DcMesh; 3]) -> Option<Aabb> {
    Aabb::enclosing(
        lods.iter()
            .flat_map(|mesh| mesh.positions.iter())
            .map(|&p| Vec3::from(p)),
    )
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

    #[test]
    fn cached_bounds_enclose_every_lod_in_the_towns_local_frame() {
        let mut lods: [freeport_core::dc::DcMesh; 3] = std::array::from_fn(|_| default());
        lods[0].positions = vec![[1.0, 2.0, 3.0], [-4.0, -5.0, -6.0]];
        lods[1].positions = vec![[8.0, -12.0, 2.0]];
        lods[2].positions = vec![[-9.0, 4.0, 11.0]];
        let bounds = lod_bounds(&lods).unwrap();
        for mesh in &lods {
            for &position in &mesh.positions {
                let delta = (bevy::math::Vec3A::from(position) - bounds.center).abs();
                assert!(delta.cmple(bounds.half_extents).all());
            }
        }
        assert!(lod_bounds(&std::array::from_fn(|_| default())).is_none());
    }

    #[test]
    fn lod_switches_keep_cached_bounds_after_vertex_data_moves_to_the_renderer() {
        let mut meshes = Assets::<Mesh>::default();
        let handles: [Handle<Mesh>; 3] = std::array::from_fn(|lod| {
            meshes.add(
                Mesh::new(
                    bevy::mesh::PrimitiveTopology::TriangleList,
                    RenderAssetUsages::RENDER_WORLD,
                )
                .with_inserted_attribute(
                    Mesh::ATTRIBUTE_POSITION,
                    vec![
                        [0.0, 0.0, 0.0],
                        [1.0 + lod as f32, 0.0, 0.0],
                        [0.0, 1.0, 0.0],
                    ],
                ),
            )
        });
        for handle in &handles {
            // This is the same operation Bevy's render extraction uses for
            // RENDER_WORLD assets; the CPU asset becomes metadata only.
            meshes
                .get_mut_untracked(handle)
                .unwrap()
                .take_gpu_data()
                .unwrap();
        }
        let bounds = Aabb::from_min_max(Vec3::ZERO, Vec3::new(3.0, 1.0, 0.0));
        let mut app = App::new();
        app.insert_resource(meshes)
            .insert_resource(Eye(WorldPos(DVec3::X * 5000.0)))
            .init_resource::<crate::tuning::Tuning>()
            .add_systems(Update, update_lod);
        let entity = app
            .world_mut()
            .spawn((
                Mesh3d(handles[0].clone()),
                Built {
                    meshes: handles.clone(),
                    empty: [false; 3],
                    level: 0,
                    radius: 0.0,
                },
                Anchored {
                    at: WorldPos(DVec3::ZERO),
                },
                Visibility::Inherited,
                bounds,
                NoAutoAabb,
            ))
            .id();
        for (at, wanted) in [(5000.0, 2), (500.0, 1), (0.0, 0)] {
            app.world_mut().resource_mut::<Eye>().0 = WorldPos(DVec3::X * at);
            app.update();
            assert_eq!(
                app.world().get::<Mesh3d>(entity).unwrap().0,
                handles[wanted]
            );
            let actual = app.world().get::<Aabb>(entity).unwrap();
            assert_eq!(actual.center, bounds.center);
            assert_eq!(actual.half_extents, bounds.half_extents);
            assert!(app.world().get::<NoAutoAabb>(entity).is_some());
        }
    }
}
