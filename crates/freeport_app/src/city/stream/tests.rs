use super::*;
use bevy::math::DVec2;
use freeport_core::town::lay;

fn towns_along(radius: f64, gaps: &[f64]) -> Vec<Town> {
    let mut at = 0.0;
    gaps.iter()
        .enumerate()
        .map(|(k, g)| {
            at += g;
            let a = at / radius;
            let dir = DVec3::new(a.sin(), a.cos(), 0.0);
            lay(dir, 0.0, 40.0, DVec2::new(1.0, 0.0), k, 7)
        })
        .collect()
}

/// The built set FOLLOWS the eye, and it is a count with a reach on it.
#[test]
fn the_built_set_is_the_nearest_towns_and_follows_the_eye() {
    let radius = 1_000_000.0;
    let towns = towns_along(radius, &[20_000.0; 8]);
    let at = |k: usize| towns[k].dir * radius;
    assert_eq!(
        wanted(&towns, radius, at(0), 3, 100_000.0),
        vec![0, 1, 2],
        "the three nearest the first town"
    );
    assert_eq!(
        wanted(&towns, radius, at(7), 3, 100_000.0),
        vec![5, 6, 7],
        "the three nearest the last town"
    );
    let alone = wanted(&towns, radius, DVec3::NEG_Y * radius, 3, 100_000.0);
    assert!(
        alone.is_empty(),
        "{alone:?} standing within reach of nowhere"
    );
}

/// One town a frame CONVERGES, and never holds more than the count
/// while it does.
#[test]
fn one_town_a_frame_converges_without_ever_holding_too_many() {
    let radius = 1_000_000.0;
    let towns = towns_along(radius, &[20_000.0; 8]);
    let count = 3;
    let mut have: Vec<usize> = vec![];
    for k in 0..towns.len() {
        let want = wanted(&towns, radius, towns[k].dir * radius, count, 100_000.0);
        for _ in 0..16 {
            have.sort_unstable();
            if have == want {
                break;
            }
            // The streamer's own rule, stepped by hand: drop one that is
            // not wanted, else build one that is.
            if let Some(slot) = have.iter().position(|h| !want.contains(h)) {
                have.remove(slot);
            } else if let Some(&add) = want.iter().find(|w| !have.contains(w)) {
                have.push(add);
            }
            assert!(have.len() <= count, "{} towns standing at once", have.len());
        }
        have.sort_unstable();
        assert_eq!(have, want, "the set did not converge at town {k}");
    }
}

/// What the port costs tile by tile: how many tiles, how many triangles
/// its blocks are against its bakes, and how long a tile takes to build
/// at each grade. A measurement and not a check, so it prints.
#[test]
#[ignore]
fn measure_the_port_by_tile() {
    use crate::city::tiles::{draw, stops, tiles_of, MASS};
    let planet = crate::world::home_planet(crate::OCTAVES);
    let atlas = crate::atlas::load(crate::world::HOME, &planet, crate::SEA, crate::TOWN_RADIUS)
        .expect("the harness body's atlas");
    let towns = atlas.towns(&planet, crate::SEA);
    let library = crate::buildings::Library::load();
    let town = &towns[0];
    let t0 = std::time::Instant::now();
    let tiles = tiles_of(town, crate::RADIUS);
    println!(
        "the port: {} lots and {} pieces in {} tiles, cut in {:.1} ms",
        town.lots.len(),
        town.pieces.len(),
        tiles.len(),
        t0.elapsed().as_secs_f64() * 1000.0
    );
    for grade in 0..=MASS {
        let t0 = std::time::Instant::now();
        let tris: usize = tiles
            .iter()
            .map(|t| {
                draw(
                    &library,
                    town,
                    t,
                    grade,
                    crate::RADIUS,
                    crate::SEA,
                    crate::SEED,
                )
                .triangles
            })
            .sum();
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        println!(
            "grade {grade}: {tris} triangles, built in {ms:.0} ms, {:.2} ms a tile",
            ms / tiles.len() as f64
        );
    }
    let t0 = std::time::Instant::now();
    let boxes: usize = tiles
        .iter()
        .map(|t| {
            stops(&library, town, t, crate::RADIUS, crate::SEED)
                .blocks
                .len()
        })
        .sum();
    println!(
        "boxes: {boxes} in {:.0} ms",
        t0.elapsed().as_secs_f64() * 1000.0
    );
}
