//! Towns BUILT on the planet: which of them stand (`stream`), how each is
//! cut into tiles (`tiles`) and what every tile is drawn and walked into
//! as from where the eye is (`detail`).

pub mod detail;
pub mod stream;
pub mod tiles;

use bevy::prelude::*;

/// The glazing material, made once and kept, because a tile is spawned
/// whenever one comes into range and a material a spawn is a material a
/// drive leaks.
#[derive(Resource)]
pub struct Glazing(pub Handle<StandardMaterial>);

impl Glazing {
    pub fn new(standard: &mut Assets<StandardMaterial>) -> Self {
        Glazing(standard.add(StandardMaterial {
            base_color: Color::srgba(0.55, 0.72, 0.78, 0.18),
            perceptual_roughness: 0.12,
            reflectance: 0.5,
            alpha_mode: AlphaMode::Blend,
            cull_mode: None,
            double_sided: true,
            ..default()
        }))
    }
}

/// How deep a town CUTS at its own edge, and how steep the apron that
/// ramp makes: the worst over every settlement on the body, metres and
/// rise over run.
///
/// `town::settle` gives a site the LOWEST ground its own survey found
/// and `town::LEVEL` bounds the SLOPE across it, so the cut at a site's
/// outline is up to `LEVEL` times how far that outline reaches. That
/// was eleven metres on a town of 170 m and is seventy on one of 537:
/// the depth grows with the town while `field::site_skirt` stays the
/// same eleven metres widened by the wobble, so a bigger town is a
/// STEEPER wall round itself rather than a wider one. It is what
/// `roads::worst_grade` finds every slip piece over the limit on, and
/// it is measured here rather than reasoned about, because the number
/// a town actually rolls is what decides whether that matters.
pub fn worst_cut(world: &crate::world::World) -> (f64, f64, f64) {
    let bare = world.planet.bare();
    let radius = world.planet.radius;
    let (mut deep, mut steep, mut mean) = (0.0f64, 0.0f64, 0.0f64);
    for town in &world.towns {
        let site = freeport_core::town::site_of(town);
        let skirt = freeport_core::field::site_skirt(&site);
        let mut worst = 0.0f64;
        for k in 0..12 {
            let a = std::f64::consts::TAU * k as f64 / 12.0;
            let out = (town.east * a.cos() + town.north * a.sin()).normalize();
            let edge = site.level_r(town.dir);
            let dir = (town.dir * radius + out * edge).normalize();
            worst = worst.max(bare.surface(dir).0 - site.h);
        }
        deep = deep.max(worst);
        // A smoothstep climbs at one and a half times its own average.
        steep = steep.max(worst * 1.5 / skirt);
        mean += worst;
    }
    (deep, steep, mean / world.towns.len().max(1) as f64)
}
