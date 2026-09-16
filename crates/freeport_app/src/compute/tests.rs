use super::*;
use freeport_core::dc::{contour, contour_sampled};
use freeport_core::lattice::{air, Rings};
use freeport_core::town::{surface_radius, Site};
use std::time::Instant;

#[test]
#[ignore = "requires a compute-capable GPU; run explicitly with --ignored --nocapture"]
fn gpu_preserves_cpu_signs_and_lod_seams() {
    use bevy::render::renderer::initialize_renderer;
    use bevy::render::settings::{Backends, WgpuSettings};
    let resources = bevy::tasks::block_on(initialize_renderer(
        Backends::all(),
        None,
        &WgpuSettings::default(),
    ));
    println!("compute adapter: {}", resources.2.name);
    let sampler = Sampler::new(resources.0, resources.1);
    let mut planet = Planet {
        octaves: 18,
        overhang: 3.0,
        ledge: 12.0,
        ..default()
    };
    let dir = DVec3::new(0.5, 0.8, -0.3).normalize();
    let top = surface_radius(&planet, dir);
    planet.sites.push(Site {
        dir,
        h: top - planet.radius,
        r: 172.0,
    });
    let lat = Lattice::new(DVec3::splat(-2_000_099.875), 0.25);
    let eye = dir * top;
    let rings = Rings::around(&lat, eye, 12);
    let mut chunks = Vec::new();
    for level in [0, 2, 5, 10] {
        let mut candidates: Vec<_> = rings
            .chunks()
            .into_iter()
            .filter(|id| id.level == level)
            .map(|id| {
                let centre = id.corner(&lat) + DVec3::splat(id.size(&lat) * 0.5);
                let sig = rings.signature(id);
                let coarse = (0..26).any(|i| (sig >> (i * 2)) & 3 == 2);
                (coarse, planet.at(centre).abs(), id)
            })
            .collect();
        candidates.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.total_cmp(&b.1)));
        chunks.extend(candidates.into_iter().take(2).map(|(_, _, id)| (lat, id)));
    }
    sampler.sample(&planet, &chunks).expect("shader warmup");
    measure_batches(&sampler, &planet, &chunks);
    let started = Instant::now();
    let samples = sampler.sample(&planet, &chunks).expect("GPU sampling");
    let gpu_ms = started.elapsed().as_secs_f64() * 1000.0;
    let started = Instant::now();
    let reference: Vec<Vec<f32>> = chunks
        .iter()
        .map(|(lat, id)| {
            (0..POINTS)
                .map(|i| planet.at(point_at(lat, *id, i)) as f32)
                .collect()
        })
        .collect();
    println!(
        "{} samples: GPU {:.2} ms including input and readback; CPU {:.2} ms",
        POINTS * chunks.len(),
        gpu_ms,
        started.elapsed().as_secs_f64() * 1000.0
    );
    let mut worst = 0.0_f32;
    let mut seams = 0;
    for (((lat, id), values), cpu) in chunks.iter().zip(samples).zip(reference) {
        for (&a, b) in values.iter().zip(cpu) {
            worst = worst.max((a - b).abs());
            assert_eq!(air(a), air(b), "sign differs at {id:?}: {a} vs {b}");
        }
        let want = contour(&planet, lat, *id, &rings);
        seams += want.seams;
        let got = contour_sampled(&planet, lat, *id, &rings, values);
        assert_eq!(want.positions, got.positions, "vertices differ for {id:?}");
        assert_eq!(want.indices, got.indices, "topology differs for {id:?}");
        assert_eq!(want.normals, got.normals);
    }
    assert!(
        seams > 0,
        "the GPU regression must exercise mixed-level seam polygons"
    );
    println!("sign-only density deviation: {worst} metres; identical CPU/GPU mesh geometry");
    check_other_planets(&sampler);
}

fn measure_batches(sampler: &Sampler, planet: &Planet, chunks: &[(Lattice, ChunkId)]) {
    for limit in [2, 4, 8] {
        let mut timings = Vec::new();
        for _ in 0..5 {
            for batch in chunks.chunks(limit) {
                let started = Instant::now();
                sampler.sample(planet, batch).expect("GPU batch timing");
                timings.push(started.elapsed().as_secs_f64() * 1000.0);
            }
        }
        timings.sort_by(f64::total_cmp);
        println!(
            "batch {limit}: median {:.3} ms/dispatch, {:.3} ms/chunk",
            timings[timings.len() / 2],
            timings.iter().sum::<f64>() / (5 * chunks.len()) as f64
        );
    }
}

fn check_other_planets(sampler: &Sampler) {
    #[derive(serde::Deserialize)]
    struct Case {
        radius: f64,
        relief: f64,
        seed: u32,
    }
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("../../../../assets/config/planets.json"))
            .expect("bundled planets");
    let planets = cases
        .into_iter()
        .map(|c| Planet {
            radius: c.radius,
            relief: c.relief,
            seed: c.seed,
            octaves: 18,
            overhang: 3.0,
            ledge: 12.0,
            ..default()
        })
        .chain(std::iter::once(Planet {
            radius: 3_000_000.0,
            relief: 290_000.0,
            seed: u32::MAX,
            octaves: 24,
            overhang: 3.0,
            ledge: 12.0,
            ..default()
        }));
    for planet in planets {
        let dir = DVec3::new(-0.73, 0.3, -0.2).normalize();
        let eye = dir * surface_radius(&planet, dir);
        let lat = Lattice::new(DVec3::splat(-2.0 * planet.radius - 99.75), 0.5);
        let rings = Rings::around(&lat, eye, 16);
        let mut chunks = Vec::new();
        for level in [0, 3, 7, 12] {
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
            chunks.extend(candidates.into_iter().take(2).map(|(_, id)| (lat, id)));
        }
        let samples = sampler
            .sample(&planet, &chunks)
            .expect("sign stress sampling");
        for ((lat, id), values) in chunks.iter().zip(samples) {
            for (i, value) in values.into_iter().enumerate() {
                let reference = planet.at(point_at(lat, *id, i)) as f32;
                assert_eq!(
                    air(value),
                    air(reference),
                    "seed {}, {id:?}, sample {i}: {value} vs {reference}",
                    planet.seed
                );
            }
        }
    }
}
