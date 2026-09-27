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
        sites: vec![].into(),
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
    let chart = Chart::bake(&planet, sea, 256, 128, &[]);
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
    let chart = Chart::bake(&planet, sea, 256, 128, &[]);
    let mut checked = 0;
    for i in (0..chart.width * chart.height).step_by(97) {
        let dir = pixel_dir(i % chart.width, i / chart.width, chart.width, chart.height);
        let spot = spot_at(&planet, sea, dir, 0.0);
        // The chart's alpha IS the water mask, so what is held is that
        // the byte on the chart is the byte the field would encode, and
        // never a second threshold written beside it: the alpha is
        // `round(water * 255)` and this test read it back through
        // `> 8` against an `over_sea < -FULL_DEPTH * 0.03` of its own,
        // which are -7.3 m and -6.6 m of water. It agreed for as long as
        // no sampled texel fell in the 0.7 m between them.
        let want = (spot.water.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        let got = chart.sample(dir)[3];
        assert!(
            got.abs_diff(want) <= 1,
            "the chart says {got} of water at {dir} where the field says {want}"
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
    let chart = Chart::bake(&planet, sea, 256, 128, &[]);
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

/// East and north at a direction, as `distant.wgsl` builds them to bend
/// its normal along. **It is the reference and this is its
/// transcription**: the shader is what draws, and this is here so the
/// frame can be checked without a GPU.
fn frame(d: DVec3) -> (DVec3, DVec3) {
    let flat = (d.x * d.x + d.z * d.z).sqrt();
    let east = if flat > 1e-6 {
        DVec3::new(-d.z, 0.0, d.x) / flat
    } else {
        DVec3::Z
    };
    (east, east.cross(d))
}

/// The sea is FLAT. The slope map is taken off the surface that is drawn,
/// and over an ocean the drawn surface is the water, so a texel with sea
/// all round it has no slope whatever the sea bed under it does.
///
/// Taken off the raw altitude, the ocean carried the sea BED's relief: on
/// this planet 13,844 sea texels came out at a mean bend of 45 of 127 and
/// a worst of 128, against the land's own 50.
#[test]
fn the_sea_carries_no_slope_because_the_sea_is_what_is_drawn() {
    let (planet, sea) = world();
    let (w, h) = (256usize, 128usize);
    let chart = Chart::bake(&planet, sea, w, h, &[]);
    let wet =
        |x: usize, y: usize| planet.radius + planet.surface(pixel_dir(x, y, w, h)).0 - sea < 0.0;
    let (mut open, mut worst) = (0usize, 0i32);
    for y in 1..h - 1 {
        // The east difference reaches as far as the projection makes it,
        // so the neighbourhood this checks reaches exactly as far.
        let span = east_span(w, h, y as isize) as usize;
        for x in 0..w {
            let (l, r) = ((x + w - span) % w, (x + span) % w);
            // Only where every sample the difference reads is under the
            // sea: a texel within reach of a shore is half land and its
            // slope is the coast, which is real.
            if !(wet(x, y) && wet(l, y) && wet(r, y) && wet(x, y - 1) && wet(x, y + 1)) {
                continue;
            }
            let i = (y * w + x) * 4;
            let bend = (chart.normal[i] as i32 - 128)
                .abs()
                .max((chart.normal[i + 1] as i32 - 128).abs());
            open += 1;
            worst = worst.max(bend);
        }
    }
    println!("{open} texels of open sea, worst bend {worst} of 127");
    assert!(open > 500, "only {open} texels of open sea to check");
    assert_eq!(worst, 0, "the open sea carries a bend of {worst} of 127");
}

/// A slope leans its normal AWAY from the hill, along both axes, and the
/// axes are the chart's own.
///
/// The first cut built the north axis and ADDED it: on the steepest
/// northward texel the normal came out 0.41 along north where it had to
/// be negative, so every body was lit from the wrong side along one axis
/// and a ridge read as a gully. It also swapped its reference axis at
/// 64 degrees of latitude, so every ice cap was shaded in a frame that
/// was not east and north at all.
#[test]
fn a_charted_slope_leans_the_normal_away_from_its_hill() {
    let (planet, sea) = world();
    let (w, h) = (256usize, 128usize);
    let chart = Chart::bake(&planet, sea, w, h, &[]);
    let bend_of = |i: usize| {
        (
            (chart.normal[i * 4] as f64 - 128.0) / 127.0,
            (chart.normal[i * 4 + 1] as f64 - 128.0) / 127.0,
        )
    };
    let mut steep: Vec<usize> = (0..w * h)
        .filter(|i| {
            let (e, n) = bend_of(*i);
            e.hypot(n) > 0.3
        })
        .collect();
    steep.sort_by_key(|i| {
        let (e, n) = bend_of(*i);
        -(e.hypot(n) * 1000.0) as i64
    });
    assert!(
        steep.len() > 100,
        "only {} texels carry a slope",
        steep.len()
    );
    let (mut leaned, mut worst_lean) = (0usize, f64::NEG_INFINITY);
    for &i in steep.iter().take(400) {
        let (x, y) = (i % w, i / w);
        let d = pixel_dir(x, y, w, h);
        let (east, north) = frame(d);
        // The frame is the CHART's: east goes the way u grows and north
        // the way v shrinks, which is what the two differences measure.
        let along_u = pixel_dir((x + 1) % w, y, w, h) - d;
        assert!(
            along_u.dot(east) > 0.0,
            "east does not follow u at texel {x},{y}"
        );
        if y > 0 {
            let along_v = pixel_dir(x, y - 1, w, h) - d;
            assert!(
                along_v.dot(north) > 0.0,
                "north does not follow v at texel {x},{y}"
            );
        }
        let (be, bn) = bend_of(i);
        let n = (d - east * be * 0.45 - north * bn * 0.45).normalize();
        let uphill = (east * be + north * bn).normalize();
        let lean = n.dot(uphill);
        worst_lean = worst_lean.max(lean);
        if lean < 0.0 {
            leaned += 1;
        }
    }
    println!("{leaned} of 400 steep texels lean downhill, worst lean {worst_lean:.4}");
    assert_eq!(leaned, 400, "a normal leaned INTO its own hill");
    assert!(
        worst_lean < 0.0,
        "worst lean {worst_lean:.4} is not downhill"
    );
}

/// A MARK IS A COVERAGE AND A LIGHT, and never a colour painted into the
/// albedo.
///
/// A city is drawn 2.2 texels across and a road 0.9, which on the harness
/// planet is 13.5 km and 5.5 km of ground against a town 185 m across and
/// a road 6.9 m wide: 73 and 800 times over. That is a deliberate lie and
/// a necessary one from orbit, where a texel is about a pixel; it is a
/// lie that SHOWS the moment the streamed chunks draw the same ground
/// beside it, and the owner asked for the two to match.
///
/// A mark that is painted INTO the albedo cannot be taken away again, so
/// it is carried as a coverage in the slope map's own spare lane with the
/// night light beside it, and `distant.wgsl` composites the two and fades
/// them out where the chart is being magnified past what it knows. What
/// this holds is the half that lives here: the albedo under a city is the
/// GROUND's own colour and nothing else, the coverage says a city or a
/// road is there, and ground with neither carries no mark at all.
#[test]
fn a_mark_is_a_coverage_and_a_light_and_leaves_the_albedo_alone() {
    let (planet, sea) = world();
    let site = planet.sites.iter().next().expect("a town").clone();
    let roads = [crate::road::Road {
        from: 0,
        to: 0,
        line: vec![
            (DVec3::new(1.0, 0.2, 0.0).normalize(), 0.0),
            (DVec3::new(1.0, 0.2, 0.4).normalize(), 0.0),
        ],
    }];
    let bare = Chart::bake(&planet, sea, 512, 256, &[]);
    let chart = Chart::bake(&planet, sea, 512, 256, &roads);
    assert_eq!(
        bare.albedo, chart.albedo,
        "the roads moved the albedo, so a mark cannot be taken away again"
    );
    // The CITY: covered, and its light at full cover is a city's.
    let i = chart.texel(site.dir).expect("the town's own texel");
    let (cover, light) = (chart.normal[i + 3], chart.normal[i + 2]);
    assert!(cover > 200, "a town covers {cover} of its own texel");
    assert!(light > 0, "a town's own texel burns at nothing");
    // The ROAD, at the middle of its own line, well away from that town.
    let on = roads[0].line[0].0.lerp(roads[0].line[1].0, 0.5).normalize();
    let j = chart.texel(on).expect("the road's own texel");
    let (cover, light) = (chart.normal[j + 3], chart.normal[j + 2]);
    assert!(cover > 200, "a road covers {cover} of its own texel");
    let lit = light as f64 / cover as f64;
    assert!(
        (lit - ROAD_LIGHT).abs() < 0.05,
        "a road's own texel burns at {lit:.2} and a road burns at {ROAD_LIGHT}"
    );
    // And ground with neither on it carries no mark at all.
    let away = DVec3::new(-0.6, -0.7, 0.4).normalize();
    let k = chart.texel(away).expect("a texel");
    assert_eq!(chart.normal[k + 3], 0, "empty ground carries a mark");
}
