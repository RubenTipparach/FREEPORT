//! Repeatable CPU terrain-contouring benchmark, independent of GPU rendering.

use freeport_core::dc::contour;
use freeport_core::field::{Built, Density, Planet};
use freeport_core::lattice::{Lattice, Rings};
use freeport_core::town::surface_radius;
use glam::DVec3;
use std::hint::black_box;
use std::time::Instant;

fn main() {
    let planet = Planet {
        octaves: 18,
        overhang: 3.0,
        ledge: 12.0,
        ..Planet::default()
    };
    let dir = DVec3::new(0.5, 0.8, -0.3).normalize();
    let eye = dir * surface_radius(&planet, dir);
    let lat = Lattice::new(DVec3::splat(-2_000_099.75), 0.5);
    let rings = Rings::around(&lat, eye, 11);
    let mut chunks = Vec::new();
    for level in [0, 3, 6] {
        let mut candidates: Vec<_> = rings
            .chunks()
            .into_iter()
            .filter(|id| id.level == level)
            .map(|id| {
                let centre = id.corner(&lat) + DVec3::splat(id.size(&lat) * 0.5);
                (planet.at(centre).abs(), id)
            })
            .collect();
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
        chunks.extend(candidates.into_iter().take(2).map(|(_, id)| id));
    }
    let field = Built::bare(&planet);
    for run in 0..5 {
        let started = Instant::now();
        let mut triangles = 0;
        let mut checksum = 0u64;
        for &id in &chunks {
            let mesh = black_box(contour(&field, &lat, id, &rings));
            triangles += mesh.triangles();
            for value in mesh
                .positions
                .iter()
                .flatten()
                .map(|v| v.to_bits())
                .chain(mesh.indices)
            {
                checksum = (checksum ^ value as u64).wrapping_mul(0x100000001b3);
            }
        }
        println!(
            "run {run}: {:.2} ms, {triangles} triangles, checksum {checksum:016x}",
            started.elapsed().as_secs_f64() * 1000.0
        );
    }
}
