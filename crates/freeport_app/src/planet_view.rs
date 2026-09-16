//! Whole-body distant silhouettes, behind the streamed volumetric surface.

use crate::planets::Planets;
use crate::stream::Anchored;
use crate::terrain::TerrainMaterial;
use crate::water::{water_material, WaterMaterial};
use bevy::math::DVec3;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use freeport_core::pos::WorldPos;

pub(crate) fn spawn(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<TerrainMaterial>,
    waters: &mut Assets<WaterMaterial>,
    standard: &mut Assets<StandardMaterial>,
    planets: &mut Planets,
) {
    let template = materials
        .get(&planets.bodies[0].material)
        .expect("home terrain material exists at startup")
        .clone();
    let distant = standard.add(StandardMaterial {
        perceptual_roughness: 1.0,
        ..default()
    });
    for (index, body) in planets.bodies.iter_mut().enumerate() {
        if index > 0 {
            let mut material = template.clone();
            material.extension.params.z = 0.0;
            material.extension.params.w = body.world.sea.radius as f32;
            material.extension.palette = Vec3::from_array(body.colour).extend(1.0);
            body.material = materials.add(material);
            body.water = water_material(waters, body.world.sea.radius);
        }
        // This is an interior backing surface, never a second surface over
        // the near terrain. It fills the distant disk while chunks stream.
        let radius = body.world.planet.band().0 - 2.0;
        let mut mesh = Sphere::new(radius as f32)
            .mesh()
            .ico(48)
            .expect("48 edge subdivisions fit an icosphere");
        if let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        {
            let colours: Vec<[f32; 4]> = positions
                .iter()
                .map(|p| {
                    let dir = DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64).normalize();
                    let height = body.world.planet.surface(dir).0;
                    let wet = body.world.planet.radius + height < body.world.sea.radius;
                    let colour = if wet {
                        [0.025, 0.12, 0.22]
                    } else {
                        body.colour
                    };
                    let shade = (0.8 + height / body.world.planet.relief.max(1.0) * 0.4) as f32;
                    [colour[0] * shade, colour[1] * shade, colour[2] * shade, 1.0]
                })
                .collect();
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colours);
        }
        commands.spawn((
            Name::new(format!("{} distant surface", body.name)),
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(distant.clone()),
            Transform::from_translation(body.centre.as_vec3()),
            Anchored {
                at: WorldPos(body.centre),
            },
            bevy::light::NotShadowCaster,
        ));
    }
}

/// Every material has its body's own centre, including the home buildings
/// when another planet supplies the near terrain and atmosphere.
pub(crate) fn recentre(
    planets: Res<Planets>,
    frame: Res<crate::stream::Frame>,
    mut materials: ResMut<Assets<TerrainMaterial>>,
    mut waters: ResMut<Assets<WaterMaterial>>,
) {
    for body in &planets.bodies {
        let centre = frame.0.local(WorldPos(body.centre));
        if let Some(material) = materials.get_mut(&body.material) {
            material.extension.centre = centre.extend(0.0);
        }
        crate::water::recentre(&mut waters, &body.water, centre);
    }
}
