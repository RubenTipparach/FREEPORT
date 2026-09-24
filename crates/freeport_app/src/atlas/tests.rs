use super::*;
use freeport_core::road::Road;

/// A small body with towns on it, planned the way the bake plans one.
fn world() -> (Planet, f64, Vec<Town>) {
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
    let towns = town::plan(&planet, sea, 80.0, 12, planet.seed);
    planet.sites = towns.iter().map(town::site_of).collect();
    (planet, sea, towns)
}

const SEA: f64 = 40_000.0 - 40.0;

fn atlas_of(planet: &Planet, towns: &[Town], roads: Vec<Line>) -> Atlas {
    Atlas {
        body: "Test".into(),
        seed: planet.seed,
        radius: planet.radius,
        octaves: planet.octaves,
        town_radius: 80.0,
        piece: freeport_core::road::PIECE,
        embank: freeport_core::road::EMBANK,
        steepest: freeport_core::road::STEEPEST,
        curve: freeport_core::road::CURVE,
        grade: town::GRADE,
        sea: SEA,
        probe: probe(planet),
        towns: towns
            .iter()
            .map(|t| Placed {
                dir: t.dir.to_array(),
                h: t.h,
                r: t.radius,
                along: t.along.to_array(),
            })
            .collect(),
        roads,
    }
}

/// A town read back off an atlas is the town that was baked into it,
/// every lot and every piece of street.
///
/// This is the whole bet the format makes: the file keeps WHERE a town
/// stands and the code lays the grid out again, so a town that came back
/// even slightly different would be a city whose buildings moved between
/// the bake and the game while its levelled ground stayed put.
#[test]
fn a_town_read_back_is_the_town_that_was_baked() {
    let (planet, _, towns) = world();
    assert!(towns.len() > 4, "the test body grew {} towns", towns.len());
    let back = atlas_of(&planet, &towns, Vec::new()).towns(&planet, SEA);
    assert_eq!(back.len(), towns.len());
    for (a, b) in towns.iter().zip(&back) {
        assert!((a.dir - b.dir).length() < 1e-12, "a town moved");
        // And its GROUND, which is derived again rather than stored: the
        // ground a site was accepted on is the ground its town gets.
        // To rounding, because the file renormalises a direction and that
        // moves it by a bit.
        assert!(a.grade.is_some(), "a planned town is graded");
        for k in 0..64 {
            let (x, z) = ((k % 8) as f64 * 37.0 - 130.0, (k / 8) as f64 * 37.0 - 130.0);
            let gap = (a.ground(x, z) - b.ground(x, z)).abs();
            assert!(gap < 1e-6, "a town's ground moved {gap:.2e} m");
        }
        assert_eq!(a.index, b.index, "a town's index moved");
        assert_eq!(a.lots.len(), b.lots.len(), "a town's lot count moved");
        assert_eq!(a.pieces.len(), b.pieces.len(), "a town's streets moved");
        for (x, y) in a.lots.iter().zip(&b.lots) {
            assert_eq!(x.id, y.id);
            assert_eq!(x.storeys, y.storeys);
            assert_eq!(x.kind, y.kind, "a building changed kind");
            assert!((x.x - y.x).abs() < 1e-12 && (x.z - y.z).abs() < 1e-12);
        }
    }
    println!(
        "{} towns, {} lots and {} street pieces in the first, all read back",
        towns.len(),
        towns[0].lots.len(),
        towns[0].pieces.len()
    );
}

/// An atlas survives the trip through the file: written out and read back
/// it holds the same towns and the same road lines.
#[test]
fn an_atlas_survives_being_written_and_read() {
    let (planet, _, towns) = world();
    let roads = vec![Line {
        from: 0,
        to: 1,
        line: vec![[1.0, 0.0, 0.0, 12.5], [0.0, 1.0, 0.0, -3.25]],
        run: vec![12.5, 9.0, 4.25, -3.25],
    }];
    let atlas = atlas_of(&planet, &towns, roads);
    let text = serde_json::to_string_pretty(&atlas).expect("an atlas serialises");
    let back: Atlas = serde_json::from_str(&text).expect("an atlas parses");
    assert_eq!(back.towns.len(), atlas.towns.len());
    assert_eq!(back.seed, atlas.seed);
    let (there, here) = (back.roads(), atlas.roads());
    assert_eq!(there.len(), 1);
    assert_eq!(there, here, "a road changed in the file");
    // And the line came back as unit directions with their levels.
    let Road { line, .. } = &there[0];
    assert!((line[0].0 - DVec3::X).length() < 1e-12);
    assert!((line[0].1 - 12.5).abs() < 1e-12);
    println!(
        "{} bytes of atlas for {} towns",
        text.len(),
        atlas.towns.len()
    );
}

/// An atlas of ANOTHER body is refused. A plan from a different seed is
/// not a stale plan, it is the plan of a different world: its cities would
/// stand in this one's sea and its roads would run through mountains, with
/// nothing on screen to say why.
#[test]
fn an_atlas_of_another_body_is_refused() {
    let (planet, _, towns) = world();
    let atlas = atlas_of(&planet, &towns, Vec::new());
    assert!(atlas.fits("Test", &planet, SEA, 80.0));
    assert!(
        atlas.fits("test", &planet, SEA, 80.0),
        "the name is not case bound"
    );
    assert!(
        !atlas.fits("Elsewhere", &planet, SEA, 80.0),
        "another body fitted"
    );
    assert!(
        !atlas.fits("Test", &planet, SEA, 90.0),
        "another town size fitted"
    );
    assert!(
        !atlas.fits("Test", &planet, SEA + 50.0, 80.0),
        "another sea level fitted"
    );
    let mut reseeded = planet.clone();
    reseeded.seed += 1;
    assert!(
        !atlas.fits("Test", &reseeded, SEA, 80.0),
        "another seed fitted"
    );
    let mut resized = planet.clone();
    resized.radius += 1000.0;
    assert!(
        !atlas.fits("Test", &resized, SEA, 80.0),
        "another radius fitted"
    );
    let mut coarser = planet.clone();
    coarser.octaves -= 1;
    assert!(
        !atlas.fits("Test", &coarser, SEA, 80.0),
        "an atlas of ground with more detail in it than this body has fitted"
    );
    // And a body of the same NAME, seed, size and octaves whose ground is
    // a different shape. Nothing in the fingerprint above says what the
    // relief looks like, so without the probe this is the atlas that gets
    // through: cities standing where a coast used to be.
    let mut reshaped = planet.clone();
    reshaped.lumps *= 1.05;
    assert!(
        !atlas.fits("Test", &reshaped, SEA, 80.0),
        "an atlas of ground a different shape fitted"
    );
    println!(
        "the probe reads {:?} m",
        atlas.probe.iter().map(|h| h.round()).collect::<Vec<_>>()
    );
}
