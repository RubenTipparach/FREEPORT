//! Hex caps, terrace walls and the distant sphere in the authored terrain material.

use bevy::asset::RenderAssetUsages;
use bevy::math::DVec3;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use freeport_core::field::{Density, Planet};
use freeport_core::sphere::face_uv_to_dir;
use hex_planet::hex::{HexCell, HexGrid};

use crate::hex_config::HexConfig;

/// The visible chart and its matching collision surface.
#[derive(Clone)]
pub struct HexGround {
    pub grid: HexGrid,
    pub focus: DVec3,
    pub config: HexConfig,
}

impl HexGround {
    pub fn radius_at(&self, planet: &Planet, dir: DVec3) -> f64 {
        let smooth = smooth_radius(planet, dir);
        if (dir * planet.radius).distance(self.focus) > self.config.lod.hex_end {
            return smooth;
        }
        let Some(address) = self.grid.locate(dir) else {
            return smooth;
        };
        let cell = self.grid.cell(address);
        cell.cap_radius(dir, cap_height(planet, &cell, self.config.step))
    }
}

pub fn smooth_radius(planet: &Planet, dir: DVec3) -> f64 {
    planet.radius + planet.at(dir * planet.radius)
}

fn cap_height(planet: &Planet, cell: &HexCell, step: f64) -> f64 {
    (smooth_radius(planet, cell.centre.normalize()) / step).round() * step
}

#[derive(Default)]
struct Triangles {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
}

impl Triangles {
    fn push(&mut self, mut points: [DVec3; 3], outward: DVec3, origin: DVec3) {
        let mut normal = (points[1] - points[0]).cross(points[2] - points[0]);
        if normal.length_squared() < 1e-16 {
            return;
        }
        if normal.dot(outward) < 0.0 {
            points.swap(1, 2);
            normal = -normal;
        }
        let normal = normal.normalize().as_vec3().to_array();
        for point in points {
            self.positions.push((point - origin).as_vec3().to_array());
            self.normals.push(normal);
        }
    }

    fn mesh(self) -> Mesh {
        let colours = vec![[0.0, 0.0, 0.0, 1.0]; self.positions.len()];
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colours)
    }
}

pub fn patch_mesh(ground: &HexGround, planet: &Planet) -> (DVec3, Mesh) {
    let around = ground.grid.locate(ground.focus).unwrap_or([0, 0]);
    let origin = ground.focus;
    let mut triangles = Triangles::default();
    for cell in ground.grid.patch(around, ground.config.rings()) {
        if cell.centre.distance(ground.focus)
            > ground.config.lod.height_start + ground.config.refresh * 2.0
        {
            continue;
        }
        append_cell(&mut triangles, ground, planet, &cell, origin);
    }
    (origin, triangles.mesh())
}

fn append_cell(
    out: &mut Triangles,
    ground: &HexGround,
    planet: &Planet,
    cell: &HexCell,
    origin: DVec3,
) {
    let up = cell.centre.normalize();
    let height = cap_height(planet, cell, ground.config.step);
    let middle = up * height;
    let corners = cell.corners.map(|p| {
        let dir = p.normalize();
        dir * cell.cap_radius(dir, height)
    });
    // Neighbour across each edge, in the order visible_patch emits its corners.
    let neighbours = [[1, 0], [0, 1], [-1, 1], [-1, 0], [0, -1], [1, -1]];
    for side in 0..6 {
        let next = (side + 1) % 6;
        out.push([middle, corners[side], corners[next]], up, origin);
        let delta = neighbours[side];
        let neighbour = ground
            .grid
            .cell([cell.axial[0] + delta[0], cell.axial[1] + delta[1]]);
        let neighbour_height = cap_height(planet, &neighbour, ground.config.step);
        let edge = [side, next].map(|i| {
            let dir = cell.corners[i].normalize();
            dir * neighbour.cap_radius(dir, neighbour_height)
        });
        // Both ends of equal-height caps meet below their shared radial edge.
        // Draw a wall only from the higher cell, avoiding coincident faces.
        if corners[side].length() + corners[next].length()
            <= edge[0].length() + edge[1].length() + 1e-7
        {
            continue;
        }
        let outward = (corners[side] + corners[next]) * 0.5 - middle;
        out.push([corners[side], edge[0], edge[1]], outward, origin);
        out.push([corners[side], edge[1], corners[next]], outward, origin);
    }
}

pub fn distant_mesh(planet: &Planet, subdivisions: u32) -> Mesh {
    let mut triangles = Triangles::default();
    for face in 0..6 {
        for y in 0..subdivisions {
            for x in 0..subdivisions {
                let points = [(x, y), (x + 1, y), (x + 1, y + 1), (x, y + 1)].map(|(u, v)| {
                    let dir = face_uv_to_dir(
                        face,
                        2.0 * u as f64 / subdivisions as f64 - 1.0,
                        2.0 * v as f64 / subdivisions as f64 - 1.0,
                    );
                    dir * smooth_radius(planet, dir)
                });
                triangles.push([points[0], points[1], points[2]], points[0], DVec3::ZERO);
                triangles.push([points[0], points[2], points[3]], points[0], DVec3::ZERO);
            }
        }
    }
    triangles.mesh()
}

#[cfg(test)]
mod tests {
    use super::*;
    use freeport_core::walker::{ground, Bounds};

    #[test]
    fn hex_collision_lands_on_the_rendered_cap() {
        let config = HexConfig::default();
        let surface = HexGround {
            grid: HexGrid::new(DVec3::Y, 5000.0, config.cell_radius).unwrap(),
            focus: DVec3::Y * 5000.0,
            config,
        };
        let world = crate::Ground {
            planet: Planet {
                radius: 5000.0,
                relief: 160.0,
                overhang: 0.0,
                ..default()
            },
            blocks: Vec::new(),
            bounds: Bounds {
                radius: 5000.0,
                floor: 4900.0,
                top: 5100.0,
            },
            hex: Some(surface.clone()),
        };
        for cell in surface.grid.patch([0, 0], 2) {
            let dir = cell.centre.normalize();
            let cap = cell.cap_radius(dir, cap_height(&world.planet, &cell, surface.config.step));
            let feet = ground(&world.field(), &world.bounds, dir, None);
            assert!((feet - cap).abs() < 0.005);
            assert!(world.at(dir * (cap - 0.01)) > 0.0);
            assert!(world.at(dir * (cap + 0.01)) < 0.0);
        }
    }

    #[test]
    fn meshes_have_finite_vertices_and_outward_caps() {
        let planet = Planet {
            radius: 5000.0,
            relief: 0.0,
            overhang: 0.0,
            ..default()
        };
        let config = HexConfig::default();
        let ground = HexGround {
            grid: HexGrid::new(DVec3::Y, 5000.0, config.cell_radius).unwrap(),
            focus: DVec3::Y * 5000.0,
            config,
        };
        let mut triangles = Triangles::default();
        append_cell(
            &mut triangles,
            &ground,
            &planet,
            &ground.grid.cell([0, 0]),
            ground.focus,
        );
        assert!(triangles.positions.len() >= 18);
        for p in triangles.positions {
            assert!(Vec3::from(p).is_finite());
        }
        assert!(triangles.normals.iter().all(|n| Vec3::from(*n).is_finite()));
        let cap_vertices = triangles
            .normals
            .iter()
            .filter(|n| Vec3::from(**n).y > 0.99)
            .count();
        assert_eq!(cap_vertices, 18);
    }
}
