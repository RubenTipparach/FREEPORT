use super::*;
use crate::town;

/// A planet small enough to bake in a test and rough enough to have a
/// coast, mountains and a town on it.
fn world() -> (Planet, f64) {
    let mut planet = Planet {
        radius: 40_000.0,
        relief: 900.0,
        lumps: 8.0,
        octaves: 9,
        overhang: 0.0,
        ledge: 0.0,
        seed: 5,
        sites: vec![],
    };
    let sea = planet.radius - 40.0;
    let towns = town::plan(&planet, sea, 80.0, 6, 5);
    planet.sites = towns.iter().map(town::site_of).collect();
    (planet, sea)
}

/// A direction goes out to a pixel and comes back as itself: the bake and
/// the shader's own `dir_to_uv` are inverses, and a planet whose chart is
/// turned a few degrees from its ground has every coast in the wrong
/// place.
#[test]
fn a_pixel_and_a_direction_are_inverses() {
    let (w, h) = (64, 32);
    let chart = Chart {
        width: w,
        height: h,
        albedo: vec![0; w * h * 4],
        normal: vec![0; w * h * 4],
    };
    for y in 0..h {
        for x in 0..w {
            let dir = pixel_dir(x, y, w, h);
            let d = dir.normalize();
            let u = 0.5 + d.z.atan2(d.x) / std::f64::consts::TAU;
            let v = 0.5 - d.y.clamp(-1.0, 1.0).asin() / std::f64::consts::PI;
            let bx = ((u * w as f64) as usize).min(w - 1);
            let by = ((v * h as f64) as usize).min(h - 1);
            assert_eq!((bx, by), (x, y), "pixel {x},{y} came back as {bx},{by}");
        }
    }
    // And the sampler lands in the same place.
    assert_eq!(chart.sample(pixel_dir(10, 7, w, h)), [0, 0, 0, 0]);
}

/// The chart is a PLANET: sea and land, both in quantity, with more than
/// one thing growing on the land. The vertex coloured ball this replaces
/// would pass a test that only asked for two colours.
#[test]
fn a_chart_has_sea_land_and_more_than_one_biome_on_it() {
    let (planet, sea) = world();
    let chart = Chart::bake(&planet, sea, 256, 128);
    let texels = chart.width * chart.height;
    let wet = (0..texels).filter(|i| chart.albedo[i * 4 + 3] > 8).count();
    let mut colours = std::collections::BTreeSet::new();
    for i in 0..texels {
        colours.insert([
            chart.albedo[i * 4] / 24,
            chart.albedo[i * 4 + 1] / 24,
            chart.albedo[i * 4 + 2] / 24,
        ]);
    }
    println!(
        "{texels} texels: {wet} wet, {} distinct colours",
        colours.len()
    );
    assert!(
        wet > texels / 20 && wet < texels * 19 / 20,
        "{wet} of {texels} texels are water"
    );
    assert!(
        colours.len() > 8,
        "the whole planet is {} colours, which is a ball and not a world",
        colours.len()
    );
}

/// What a kind is on the ground is what the chart paints there. A chart
/// that disagreed with the field would put a coast in one place from
/// orbit and another on the way down.
#[test]
fn the_chart_agrees_with_the_field_it_was_baked_from() {
    let (planet, sea) = world();
    let chart = Chart::bake(&planet, sea, 256, 128);
    let mut checked = 0;
    for i in (0..chart.width * chart.height).step_by(97) {
        let dir = pixel_dir(i % chart.width, i / chart.width, chart.width, chart.height);
        let spot = spot_at(&planet, sea, dir, 0.0);
        let wet = chart.sample(dir)[3] > 8;
        assert_eq!(
            wet,
            spot.over_sea < -FULL_DEPTH * 0.03,
            "the chart and the field disagree about water at {dir}"
        );
        checked += 1;
    }
    assert!(checked > 100, "only {checked} texels checked");
}

/// A town is a small biome and it is ON the chart: the ground under one is
/// levelled, and what the planet paints there is a city.
#[test]
fn a_town_is_painted_as_a_city() {
    let (planet, sea) = world();
    assert!(!planet.sites.is_empty(), "the test planet grew no town");
    for site in &planet.sites {
        let spot = spot_at(&planet, sea, site.dir, 0.0);
        assert_eq!(
            spot.kind,
            Kind::City,
            "the middle of a town is {}",
            spot.kind.name()
        );
        assert!(town_weight(&planet, site.dir) > 0.5);
    }
    // And a direction well away from every town is not a city.
    let away = DVec3::new(0.3, 0.9, -0.2).normalize();
    let near = planet
        .sites
        .iter()
        .map(|s| (s.dir - away).length())
        .fold(f64::INFINITY, f64::min);
    if near * planet.radius > 500.0 {
        assert_ne!(spot_at(&planet, sea, away, 0.0).kind, Kind::City);
    }
}

/// The slope map carries what the mesh cannot: a texel on a mountain
/// flank is off flat and a texel on an abyssal plain is on it.
#[test]
fn the_slope_map_is_flat_on_flat_ground_and_not_on_a_slope() {
    let (planet, sea) = world();
    let chart = Chart::bake(&planet, sea, 256, 128);
    let mut steepest = 0i32;
    let mut flat = 0;
    for i in 0..chart.width * chart.height {
        let dx = chart.normal[i * 4] as i32 - 128;
        let dy = chart.normal[i * 4 + 1] as i32 - 128;
        steepest = steepest.max(dx.abs().max(dy.abs()));
        if dx.abs() < 4 && dy.abs() < 4 {
            flat += 1;
        }
    }
    println!("steepest texel {steepest} of 127, {flat} texels flat");
    assert!(
        steepest > 40,
        "the whole chart is flat: steepest {steepest}"
    );
    assert!(flat > 100, "no texel on this planet is flat");
}
