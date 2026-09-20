use super::*;

#[test]
fn local_bounds_reject_empty_boxes_inside_the_relief_band() {
    let planet = Planet {
        octaves: 18,
        overhang: 3.0,
        ledge: 12.0,
        sites: vec![crate::town::Site::round(DVec3::Y, 0.0, 172.0)].into(),
        ..Planet::default()
    };
    for dir in [DVec3::Y, DVec3::X, DVec3::new(-0.6, 0.4, -0.7).normalize()] {
        let surface = crate::town::surface_radius(&planet, dir);
        for height in [-100.0, 100.0] {
            let centre = dir * (surface + height);
            let lo = centre - DVec3::splat(2.0);
            let hi = centre + DVec3::splat(2.0);
            assert_eq!(planet.solid(lo, hi), Some(height < 0.0));
            for k in 0..5 {
                for j in 0..5 {
                    for i in 0..5 {
                        let p = lo + DVec3::new(i as f64, j as f64, k as f64);
                        assert_eq!(planet.at(p) > 0.0, height < 0.0);
                    }
                }
            }
        }
        let centre = dir * surface;
        assert_eq!(planet.solid(centre - DVec3::ONE, centre + DVec3::ONE), None);
    }
}

#[test]
fn noise_is_in_range_and_deterministic() {
    for i in 0..200 {
        let p = DVec3::new(i as f64 * 0.37, i as f64 * -0.11, 3.0 + i as f64 * 0.05);
        let a = noise3(p, 5);
        assert!((0.0..=1.0).contains(&a));
        assert_eq!(a, noise3(p, 5));
        assert!((0.0..=1.0).contains(&fbm3(p, 5, 6)));
    }
    assert_ne!(
        noise3(DVec3::new(0.5, 0.5, 0.5), 1),
        noise3(DVec3::new(0.5, 0.5, 0.5), 2)
    );
}

#[test]
fn a_float_hash_is_the_cores_to_a_hundred_millionth() {
    // A GPU has no f64, so `field.wgsl` computes `mix3` exactly and then
    // rounds it into a float's twenty four bits. That rounding is the
    // whole of the divergence between the ground a shader draws and the
    // ground the walker stands on, so it is measured rather than
    // assumed: the bound in metres is this share of the relief.
    let mut worst: f64 = 0.0;
    for i in 0..40i64 {
        for j in 0..40i64 {
            for k in 0..40i64 {
                let a = hash3(i * 7 - 91, j * 13 - 17, k * 3 + 5, 11);
                let b = hash3_f32(i * 7 - 91, j * 13 - 17, k * 3 + 5, 11) as f64;
                worst = worst.max((a - b).abs());
            }
        }
    }
    // One part in 2^24, and a float cannot do better than half of that.
    assert!(worst < 6.0e-8, "the float hash is {worst} off");
    assert!(worst > 0.0, "a float held all thirty two bits?");
}

#[test]
fn noise_is_continuous_across_a_lattice_line() {
    let a = noise3(DVec3::new(2.0 - 1e-9, 0.3, 0.7), 9);
    let b = noise3(DVec3::new(2.0 + 1e-9, 0.3, 0.7), 9);
    assert!((a - b).abs() < 1e-6);
}

#[test]
fn a_planet_is_rock_inside_and_air_outside() {
    let planet = Planet::default();
    let inside = DVec3::new(0.5 * planet.radius, 0.0, 0.0);
    let outside = DVec3::new(0.0, planet.radius + planet.relief, 0.0);
    assert!(planet.at(inside) > 0.0);
    assert!(planet.at(outside) < 0.0);
    assert!(planet.at(DVec3::ZERO) > 0.0);
    let surface = planet.at(DVec3::new(0.0, 0.0, planet.radius));
    assert!(surface.abs() <= planet.relief * 0.5 + planet.overhang);
}

#[test]
fn a_block_is_a_signed_distance_in_its_own_frame() {
    let b = Block {
        centre: DVec3::new(1.0, 2.0, 3.0),
        half: DVec3::new(2.0, 1.0, 0.5),
        axes: [DVec3::Z, DVec3::X, DVec3::Y],
        material: CONCRETE,
    };
    assert_eq!(b.at(b.centre), 0.5);
    // A metre past the up face (world y) is minus one.
    assert!((b.at(b.centre + DVec3::Y * 1.5) + 1.0).abs() < 1e-12);
    // Along the box's east (world z) the half extent is two.
    assert!((b.at(b.centre + DVec3::Z * 2.0)).abs() < 1e-12);
    assert!((b.at(b.centre + DVec3::new(0.0, 1.5, 3.0)) + 2.0f64.sqrt()).abs() < 1e-12);
    let corners = b.corners();
    assert!(corners.iter().all(|c| b.at(*c).abs() < 1e-12));
    let ground = Sphere { radius: 1.0 };
    let built = Built {
        ground: &ground,
        blocks: vec![&b],
    };
    assert_eq!(built.at(b.centre), 0.5);
    assert_eq!(built.at(DVec3::ZERO), 1.0);
    assert_eq!(built.material(b.centre), CONCRETE);
    assert_eq!(built.material(DVec3::ZERO), TERRAIN);
    assert_eq!(ground.material(DVec3::ZERO), TERRAIN);
}

#[test]
fn a_box_is_ruled_rock_or_air_only_where_the_band_allows() {
    let planet = Planet {
        radius: 100.0,
        relief: 4.0,
        lumps: 3.0,
        octaves: 3,
        overhang: 1.0,
        ledge: 5.0,
        seed: 1,
        sites: vec![].into(),
    };
    // The band's INVARIANT rather than its arithmetic: every direction's
    // ground is inside it, and it is not so wide that ruling is
    // pointless. The pair itself moved when the relief became a few
    // composed terms rather than one fractal, and a pin on the pair
    // would have read as a defect when what changed was the planet.
    let (floor, top) = planet.band();
    assert!(floor < planet.radius && top > planet.radius);
    assert!(
        top - floor < planet.relief * 2.0 + planet.overhang * 2.0,
        "the band {floor} to {top} is wider than the relief can reach"
    );
    for i in 0..2000 {
        let t = i as f64 * 0.618;
        let d = DVec3::new(t.sin(), (t * 0.37).cos(), (t * 1.3).sin()).normalize();
        let r = planet.radius + planet.surface(d).0;
        assert!(
            (floor..=top).contains(&r),
            "ground at {r} is outside the band {floor} to {top}"
        );
    }
    let deep = (
        DVec3::new(-10.0, -10.0, -10.0),
        DVec3::new(10.0, 10.0, 10.0),
    );
    assert_eq!(planet.solid(deep.0, deep.1), Some(true));
    let high = (DVec3::new(0.0, 103.0, 0.0), DVec3::new(5.0, 110.0, 5.0));
    assert_eq!(planet.solid(high.0, high.1), Some(false));
    let crust = (DVec3::new(0.0, 95.0, 0.0), DVec3::new(5.0, 105.0, 5.0));
    assert_eq!(planet.solid(crust.0, crust.1), None);
    assert_eq!(box_radii(deep.0, deep.1), (0.0, (300.0f64).sqrt()));
    let ball = Sphere { radius: 5.0 };
    assert_eq!(
        ball.solid(DVec3::splat(6.0), DVec3::splat(7.0)),
        Some(false)
    );
    let slab = Block {
        centre: DVec3::new(0.0, 104.0, 0.0),
        half: DVec3::new(1.0, 1.0, 0.2),
        axes: [DVec3::X, DVec3::Z, DVec3::Y],
        material: CONCRETE,
    };
    let built = Built {
        ground: &planet,
        blocks: vec![&slab],
    };
    assert_eq!(built.solid(high.0, high.1), None, "a slab in the box");
    // The slope bound holds: the field between two points a step apart
    // never changes faster than it says.
    let slope = planet.slope();
    assert!(slope > 1.0 && slope < 20.0, "slope {slope}");
    assert_eq!(built.slope(), slope);
    assert_eq!(ball.slope(), 1.0);
    let mut steepest: f64 = 0.0;
    for i in 0..2000 {
        let t = i as f64 * 0.37;
        let p = DVec3::new(t.sin() * 100.0, t.cos() * 100.0, (t * 0.3).sin() * 30.0);
        let step = DVec3::new(0.011, -0.007, 0.013);
        steepest = steepest.max((planet.at(p + step) - planet.at(p)).abs() / step.length());
    }
    assert!(
        steepest < slope,
        "measured {steepest} against the bound {slope}"
    );
    assert_eq!(built.solid(deep.0, deep.1), Some(true));
    let (lo, hi) = slab.bounds();
    assert!((lo - DVec3::new(-1.0, 103.8, -1.0)).length() < 1e-12);
    assert!((hi - DVec3::new(1.0, 104.2, 1.0)).length() < 1e-12);
}

#[test]
fn a_grid_carries_its_apron_and_a_gradient() {
    let grid = sample(
        &Sphere { radius: 5.0 },
        DVec3::new(-8.0, -8.0, -8.0),
        1.0,
        16,
    );
    assert_eq!(grid.at(-1, -1, -1), (5.0 - (3.0f64 * 81.0).sqrt()) as f32);
    assert_eq!(grid.at(8, 8, 8), 5.0);
    let g = grid.gradient(12, 8, 8);
    assert!(
        g[0] < 0.0 && g[1].abs() < 1e-6 && g[2].abs() < 1e-6,
        "{g:?}"
    );
    assert_eq!(grid.point(0, 0, 0), grid.corner);
}

#[test]
fn a_sites_band_is_level_inside_and_relief_outside() {
    let mut planet = Planet {
        radius: 2_000.0,
        relief: 40.0,
        lumps: 12.0,
        octaves: 8,
        overhang: 0.0,
        ledge: 0.0,
        seed: 3,
        sites: vec![].into(),
    };
    let dir = DVec3::new(0.2, 0.9, 0.3).normalize();
    // Its level is UNDER the ground there, because a site cuts: one
    // above it is a site that does nothing, which is the other half
    // of this test.
    let site = crate::town::Site::round(dir, -12.0, 80.0);
    let (inner, outer) = site_band(&site);
    assert!(inner < outer, "the band runs inward to outward");
    planet.sites = vec![site].into();
    let (east, _) = crate::town::frame_at(dir);
    // A point a hair inside the inner arc is the site's height and a
    // point a hair outside the outer one is the relief alone, which is
    // what a shader handed the pair has to reproduce.
    let at = |m: f64| {
        let a = m / planet.radius;
        (dir * a.cos() + east * a.sin()).normalize()
    };
    assert_eq!(planet.surface(at(inner - 0.5)).0, planet.sites[0].h);
    let bare = Planet {
        sites: vec![].into(),
        ..planet.clone()
    };
    let far = at(outer + 0.5);
    assert!(
        (planet.surface(far).0 - bare.surface(far).0).abs() < 1e-12,
        "the ground past the skirt is not the relief"
    );
    // And a site standing OVER the ground changes nothing anywhere,
    // because a city flattens land and never adds it.
    let high = Planet {
        sites: vec![crate::town::Site::round(dir, 30.0, 80.0)].into(),
        ..bare.clone()
    };
    for m in [
        0.0,
        inner * 0.5,
        inner - 0.5,
        (inner + outer) * 0.5,
        outer + 0.5,
    ] {
        let d = at(m);
        assert!(
            (high.surface(d).0 - bare.surface(d).0).abs() < 1e-12,
            "a site over the ground raised it {m} m out"
        );
    }
}

/// A site's own skirt is inside the bound the mesher rules chunks on.
///
/// Its level CUTS, and cuts the STEEPEST a site can: at 30 m on a body
/// whose relief spans plus or minus twenty the site stood above every
/// scrap of ground it covers, and a site that only ever takes the
/// lower of itself and the land is then a site that does nothing at
/// all. What a bound has to cover is the worst case, so the level is
/// under the lowest ground here and the whole skirt is a cut.
/// THE INDEX MISSES NOTHING, which is the one thing that would be a
/// hole in the world rather than a slow frame: a site the latitude
/// window skipped is ground nobody levelled, and the chunk over it
/// is meshed against a field the walker does not stand on.
///
/// Held against a WALK of every site, on a body carrying discs and
/// corridors of every size, at a thousand directions.
#[test]
fn the_site_index_finds_every_site_a_walk_would() {
    let radius = 1_000_000.0;
    let mut sites = Vec::new();
    for k in 0..600u32 {
        let a = k as f64 * 2.399_963_229_728_653;
        let y = 1.0 - 2.0 * (k as f64 + 0.5) / 600.0;
        let r = (1.0 - y * y).max(0.0).sqrt();
        let dir = DVec3::new(r * a.cos(), y, r * a.sin());
        if k % 3 == 0 {
            sites.push(crate::town::Site::round(
                dir,
                k as f64,
                40.0 + k as f64 * 0.2,
            ));
        } else {
            // A corridor, of a length that runs from a piece to a
            // whole waypoint span, in a bearing of its own.
            let (east, north) = crate::town::frame_at(dir);
            let far = 300.0 + (k % 37) as f64 * 400.0;
            let b = (dir + (east * a.cos() + north * a.sin()) * (far / radius)).normalize();
            sites.push(crate::town::Site::arc(
                (dir, k as f64),
                (b, k as f64 + 9.0),
                14.0,
            ));
        }
    }
    let planet = Planet {
        radius,
        sites: Sites::new(sites.clone()),
        ..Planet::default()
    };
    for k in 0..1000u32 {
        let a = k as f64 * 1.399_963_229_728_653;
        let y = 1.0 - 2.0 * (k as f64 + 0.5) / 1000.0;
        let r = (1.0 - y * y).max(0.0).sqrt();
        let dir = DVec3::new(r * a.cos(), y, r * a.sin());
        for span in [0.0, 1e-5, 1e-3] {
            let found: Vec<_> = planet.sites_near(dir, span).collect();
            for site in &sites {
                let (_, outer) = site_band(site);
                let reaches = (dir - site.nearest(dir).0).length() - span <= outer / radius;
                assert!(
                    !reaches || found.iter().any(|f| *f == site),
                    "the index missed a site the walk found at {dir} span {span}"
                );
            }
        }
    }
}

/// A CORRIDOR levels the ground all the way along itself, ramping
/// from one end to the other, and leaves the relief alone past its
/// own band. It is what a road stands on.
#[test]
fn a_corridor_levels_the_ground_along_its_whole_length() {
    let radius = 100_000.0;
    let bare = Planet {
        radius,
        relief: 400.0,
        lumps: 12.0,
        octaves: 8,
        overhang: 0.0,
        ledge: 0.0,
        seed: 3,
        sites: Vec::new().into(),
    };
    let a = DVec3::new(0.2, 0.3, 0.93).normalize();
    let (east, north) = crate::town::frame_at(a);
    let b = (a + east * (900.0 / radius)).normalize();
    let site = crate::town::Site::arc((a, -60.0), (b, -20.0), 8.0);
    let (inner, outer) = site_band(&site);
    let planet = Planet {
        sites: vec![site].into(),
        ..bare.clone()
    };
    for k in 0..40 {
        let t = k as f64 / 39.0;
        let on = (a * (1.0 - t) + b * t).normalize();
        let want = site.nearest(on).1;
        // Right on the line, the ground IS the ramp.
        assert!(
            (planet.surface(on).0 - want).abs() < 1e-6,
            "at {t:.2} along the corridor the ground is {} and the ramp is {want}",
            planet.surface(on).0
        );
        // A hand inside the band, still the ramp.
        let close = (on + north * ((inner - 0.5) / radius)).normalize();
        assert!((planet.surface(close).0 - want).abs() < 1e-6);
        // Well outside it, the bare relief and nothing taken off it.
        let away = (on + north * ((outer + 40.0) / radius)).normalize();
        assert_eq!(planet.surface(away).0, bare.surface(away).0);
    }
}

#[test]
fn the_slope_bound_holds_across_a_sites_skirt() {
    let mut planet = Planet {
        radius: 2000.0,
        relief: 40.0,
        ..Default::default()
    };
    planet.sites = vec![crate::town::Site::round(DVec3::Z, -25.0, 100.0)].into();
    let bound = planet.slope();
    // Walk the SKIRT itself, read off `site_band` rather than
    // written out: a fixture that spells the band as two numbers is
    // a fixture that samples level ground the day the band moves,
    // and level ground has no slope to bound.
    let (inner, outer) = site_band(&planet.sites[0]);
    let (from, span) = (inner - 10.0, (outer - inner) + 20.0);
    let mut worst = 0.0f64;
    for i in 0..400 {
        let dist = from + span * i as f64 / 400.0;
        let a = dist / planet.radius;
        let dir = DVec3::new(a.sin(), 0.0, a.cos());
        for k in 0..5 {
            let p = dir * (planet.radius - 20.0 + 15.0 * k as f64);
            for d in [DVec3::X, DVec3::Y, DVec3::Z] {
                let h = 0.05;
                let g = (planet.at(p + d * h) - planet.at(p - d * h)).abs() / (2.0 * h);
                worst = worst.max(g);
            }
        }
    }
    assert!(
        worst <= bound,
        "the field climbs at {worst} against a bound of {bound}"
    );
    assert!(
        bound < worst * 8.0 + 2.0,
        "a bound of {bound} is slack against {worst}"
    );
}

/// A ROAD'S CORRIDOR RAMPS THROUGH ITS OWN STATIONS, and does not carry
/// a landing at each of them.
///
/// An arc's band is a capsule, so its round end reaches `CORRIDOR`
/// metres past its last station into its neighbour's, and two arcs cover
/// the ground either side of every station outright. Taking whichever
/// the index reached first gave that ground the level AT the station
/// rather than the ramp: on a one in ten grade every station carried a
/// fourteen metre landing stepping 0.66 m, and the tarmac laid on the
/// ramp floated over it. The nearest covering site is the one whose ramp
/// this is, so the profile is the straight line the road was routed at.
#[test]
fn a_corridors_ground_ramps_through_a_station_without_a_landing() {
    use crate::town::Site;
    const RADIUS: f64 = 40_000.0;
    const RUN: f64 = 240.0;
    const RISE: f64 = 24.0;
    // Three stations on one great circle, climbing at one in ten.
    let dir = |k: f64| {
        let a = k * RUN / RADIUS;
        DVec3::new(a.sin(), 0.0, a.cos())
    };
    let planet = Planet {
        radius: RADIUS,
        relief: 0.0,
        overhang: 0.0,
        ledge: 0.0,
        sites: vec![
            Site::arc((dir(0.0), 0.0), (dir(1.0), RISE), crate::road::CORRIDOR),
            Site::arc(
                (dir(1.0), RISE),
                (dir(2.0), RISE * 2.0),
                crate::road::CORRIDOR,
            ),
        ]
        .into(),
        ..Default::default()
    };
    let mut worst = 0.0f64;
    for i in 0..=800 {
        let k = i as f64 / 400.0;
        let want = RISE * k;
        worst = worst.max((planet.surface(dir(k)).0 - want).abs());
    }
    assert!(
        worst < 0.01,
        "the corridor's ground strays {worst:.3} m from the ramp it was cut to"
    );
}

/// And across a TOWN's skirt, which is not a circle.
///
/// A road's corridor is a capsule, so its boundary is everywhere square
/// to the way out of it and a fade over a fixed width climbs at a
/// smoothstep's one and a half over that width. A town's outline is the
/// `demand` its lots are laid on, so its boundary is TILTED, and the same
/// fade past a tilted boundary climbs `hypot(1, WOBBLE)` times as fast:
/// left at a road's width the field would climb past the bound
/// everywhere the outline swings, a chunk with surface in it would be
/// ruled empty, and that is a hole in the world. `field::site_skirt`
/// widens a town's own skirt by exactly that factor, and this is what
/// says the two numbers agree.
#[test]
fn the_slope_bound_holds_across_a_towns_own_outline() {
    let mut planet = Planet {
        radius: 2000.0,
        relief: 40.0,
        ..Default::default()
    };
    let along = glam::DVec2::new(1.0, 0.0);
    let site = crate::town::Site::town(DVec3::Z, -25.0, 100.0, along, 7);
    planet.sites = vec![site].into();
    let bound = planet.slope();
    let reach = site_band(&planet.sites[0]).1 + 20.0;
    let (east, north) = crate::town::frame_at(DVec3::Z);
    let mut worst = 0.0f64;
    const BEARINGS: usize = 160;
    const RINGS: usize = 160;
    for k in 0..BEARINGS {
        let a = k as f64 / BEARINGS as f64 * std::f64::consts::TAU;
        for i in 0..RINGS {
            let out = reach * (i as f64 + 0.5) / RINGS as f64;
            let dir =
                (DVec3::Z * planet.radius + (east * a.cos() + north * a.sin()) * out).normalize();
            for step in 0..3 {
                let p = dir * (planet.radius - 20.0 + 20.0 * step as f64);
                for d in [DVec3::X, DVec3::Y, DVec3::Z] {
                    let h = 0.05;
                    let g = (planet.at(p + d * h) - planet.at(p - d * h)).abs() / (2.0 * h);
                    worst = worst.max(g);
                }
            }
        }
    }
    println!("a town's own skirt climbs at {worst:.2} against a bound of {bound:.2}");
    assert!(
        worst <= bound,
        "the field climbs at {worst} across a town's outline against a bound of {bound}"
    );
}
