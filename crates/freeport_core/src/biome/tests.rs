use super::*;
use std::collections::BTreeMap;

/// The harness's own planet, which is what every number here is measured
/// on: a thousand kilometres of radius with eight of relief.
fn world() -> Shape {
    Shape {
        relief: 8_000.0,
        lumps: 12.0,
        octaves: 18,
        seed: 7,
        radius: 1.0e6,
    }
}

/// Directions spread evenly over the sphere, the golden angle spiral
/// `town::plan` uses, so a sample is not biased to a pole or a face.
fn spiral(n: usize) -> Vec<DVec3> {
    let golden = std::f64::consts::PI * (3.0 - 5.0_f64.sqrt());
    (0..n)
        .map(|i| {
            let y = 1.0 - (i as f64 + 0.5) / n as f64 * 2.0;
            let r = (1.0 - y * y).max(0.0).sqrt();
            let a = golden * i as f64;
            DVec3::new(a.cos() * r, y, a.sin() * r)
        })
        .collect()
}

/// A planet is not one hillside. The relief has to put a real share of
/// itself under the sea, hold a real share of land, and push some of that
/// land far enough up that it is a mountain rather than a swell: the
/// single fractal this replaced could do the first two and never the
/// third, because one fractal has lumps of one size everywhere.
#[test]
fn a_planet_has_sea_land_and_mountains_on_it() {
    let s = world();
    let sea = -400.0;
    let heights: Vec<f64> = spiral(4_000).iter().map(|&d| s.height(d)).collect();
    let wet = heights.iter().filter(|&&h| h < sea).count();
    let land = heights.len() - wet;
    let high = heights.iter().filter(|&&h| h > s.relief * 0.28).count();
    let lo = heights.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = heights.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    println!(
        "of 4000 directions: {wet} sea, {land} land, {high} over {:.0} m; from {lo:.0} to {hi:.0} m",
        s.relief * 0.28
    );
    assert!(
        (400..3_600).contains(&wet),
        "{wet} of 4000 under the sea is not a planet with oceans AND continents"
    );
    assert!(
        high > 40,
        "{high} directions stand over a quarter of the relief: there are no mountains"
    );
    assert!(
        hi - lo > s.relief * 0.55,
        "the relief spans {:.0} m of a possible {:.0}",
        hi - lo,
        s.relief
    );
}

/// The bound the chunk rejection rests on. A chunk is ruled empty from a
/// few samples and this is what says how far the surface can have moved
/// between them, so an understated bound is a hole in the world: measured
/// here on pairs a metre to a kilometre apart, which is the range a chunk
/// actually asks over.
#[test]
fn the_slope_bound_holds_across_the_relief() {
    let s = world();
    let bound = s.slope();
    let mut worst: f64 = 0.0;
    for (i, &dir) in spiral(600).iter().enumerate() {
        // An east at that direction, to step along the surface by.
        let up = dir.normalize();
        let east = if up.y.abs() < 0.9 { DVec3::Y } else { DVec3::X }
            .cross(up)
            .normalize();
        let north = up.cross(east);
        for step in [1.0, 10.0, 120.0, 1_000.0] {
            let bearing = i as f64 * 0.7;
            let along = east * bearing.cos() + north * bearing.sin();
            let to = (up + along * (step / s.radius)).normalize();
            let rise = (s.height(to) - s.height(dir)).abs();
            worst = worst.max(rise / step);
        }
    }
    println!("the steepest measured is {worst:.4} against a bound of {bound:.4}");
    assert!(
        worst <= bound,
        "the ground climbs at {worst:.4} where the bound says {bound:.4}"
    );
    // A bound a hundred times the truth would cost every chunk its
    // samples, so it is held loose rather than free.
    assert!(
        bound < worst * 120.0,
        "the bound {bound:.4} is {:.0} times the measured {worst:.4}",
        bound / worst.max(1e-9)
    );
}

/// What the band promises: no direction's ground is outside it. The
/// streamer skips every chunk wholly outside the band without a sample,
/// so a height past it is ground nothing meshes.
#[test]
fn the_band_holds_every_height() {
    let s = world();
    let (floor, top) = s.band();
    for &d in spiral(4_000).iter() {
        let r = s.radius + s.height(d);
        assert!(
            (floor..=top).contains(&r),
            "a direction stands at {r:.0} m, outside the band {floor:.0} to {top:.0}"
        );
    }
}

/// A gorge is cut into LAND, and the sea floor is left alone: a channel
/// carved under the water would be a trench with nothing in it, and the
/// shore would be a slot rather than a beach.
#[test]
fn a_channel_cuts_land_and_leaves_the_sea_floor_alone() {
    let s = world();
    let mut cut_on_land = 0;
    let mut deepest_under_the_sea: f64 = 0.0;
    for &d in spiral(6_000).iter() {
        let c = s.channel(d);
        if c <= 0.0 {
            continue;
        }
        // What the ground would be with no cut at all.
        let uncut = s.height(d) + s.cut(d, s.height(d));
        let cut = s.cut(d, uncut);
        if uncut > 0.0 && cut > 1.0 {
            cut_on_land += 1;
        }
        if uncut < -s.relief * 0.1 {
            deepest_under_the_sea = deepest_under_the_sea.max(cut);
        }
    }
    println!("{cut_on_land} directions carry a cut over a metre deep on land");
    assert!(
        cut_on_land > 20,
        "{cut_on_land} channels cut into land: the network is not there"
    );
    assert!(
        deepest_under_the_sea == 0.0,
        "the sea floor is cut by {deepest_under_the_sea:.1} m"
    );
}

/// The poles are cold and the equator is warm, and the middle of a
/// continent is drier than its shore. Both are what put an ice cap at one
/// end of the planet and a desert in the middle of the other.
#[test]
fn climate_is_cold_at_the_poles_and_dry_inland() {
    let s = world();
    let pole = s.climate(DVec3::Y, 0.0);
    let equator = s.climate(DVec3::X, 0.0);
    println!(
        "pole {:.2}/{:.2}, equator {:.2}/{:.2}",
        pole.temp, pole.wet, equator.temp, equator.wet
    );
    assert!(
        pole.temp < FREEZING,
        "the pole is at {:.2}, which is not frozen",
        pole.temp
    );
    assert!(equator.temp > 0.5, "the equator is at {:.2}", equator.temp);
    // Altitude takes temperature off: the same direction high up is colder.
    let low = s.climate(DVec3::X, 0.0).temp;
    let high = s.climate(DVec3::X, s.relief * 0.4).temp;
    assert!(
        high < low - 0.1,
        "at {:.0} m the equator is {high:.2} against {low:.2} at the sea",
        s.relief * 0.4
    );
}

/// The planet has to grow most of what a planet has on it. A world that
/// is all grass is the one this replaced, and a test that only counts
/// kinds a table can return would pass on it.
#[test]
fn a_planet_grows_most_of_the_kinds() {
    let s = world();
    let sea = -400.0;
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    for &d in spiral(8_000).iter() {
        let h = s.height(d);
        let over = h - sea;
        // The slope at that direction, off a pair of samples a hundred
        // metres apart, which is what the mesher's own normal would say.
        let up = d.normalize();
        let east = if up.y.abs() < 0.9 { DVec3::Y } else { DVec3::X }
            .cross(up)
            .normalize();
        let to = (up + east * (100.0 / s.radius)).normalize();
        let slope = ((s.height(to) - h) / 100.0).abs().min(1.0);
        let kind = s.climate(d, over).kind(over, slope);
        *seen.entry(kind.name()).or_default() += 1;
    }
    println!("{seen:?}");
    // City is placed rather than grown, so it is not in this count.
    let grown = Kind::all().len() - 1;
    assert!(
        seen.len() >= grown - 2,
        "only {} of {grown} kinds grow anywhere: {seen:?}",
        seen.len()
    );
    for (name, count) in &seen {
        assert!(*count > 0, "{name} is counted and empty");
    }
}

/// A kind is a colour and a name, and no two are the same: a legend
/// nobody can read is a legend that says a planet is one colour.
#[test]
fn every_kind_has_its_own_colour_and_name() {
    let mut names = std::collections::BTreeSet::new();
    for kind in Kind::all() {
        assert!(names.insert(kind.name()), "{} is named twice", kind.name());
        let c = kind.colour();
        assert!(
            c.iter().all(|v| (0.0..=1.0).contains(v)),
            "{} is {c:?}, which is not a colour",
            kind.name()
        );
    }
    for a in Kind::all() {
        for b in Kind::all() {
            if a == b {
                continue;
            }
            let (x, y) = (a.colour(), b.colour());
            let apart: f32 = (0..3).map(|k| (x[k] - y[k]).abs()).sum();
            assert!(
                apart > 0.02,
                "{} and {} are the same colour",
                a.name(),
                b.name()
            );
        }
    }
}

/// The carve is what lets a cliff undercut, and it is off where nothing
/// would: a meadow of boulders is what a planet wide overhang looks like.
#[test]
fn only_bare_steep_ground_is_carved() {
    let meadow = Climate {
        temp: 0.6,
        wet: 0.8,
    };
    let scree = Climate {
        temp: 0.4,
        wet: 0.1,
    };
    assert_eq!(carve_weight(meadow, 0.0), 0.0, "flat ground is carved");
    assert!(
        carve_weight(meadow, 1.0) < carve_weight(scree, 1.0),
        "a wet cliff is carved as hard as a dry one"
    );
    assert!(
        carve_weight(scree, 1.0) > 0.5,
        "a dry cliff is not carved at all"
    );
}

/// A spot is a number per place, and two places apart have different ones.
#[test]
fn a_spot_is_stable_and_differs_between_places() {
    let a = spot(DVec3::X, 3);
    assert_eq!(a, spot(DVec3::X, 3), "a spot is not stable");
    assert!((0.0..=1.0).contains(&a), "a spot is {a}");
    assert!(
        (a - spot(DVec3::Y, 3)).abs() > 1e-6,
        "two places have one spot"
    );
}

/// What `fbm3` actually spans, which is what decides the gain each term
/// is stretched by: a sum of halving octaves concentrates near its middle
/// and never reaches nought or one, so a term written as a share of the
/// relief and fed a raw fbm is worth a fraction of what it says.
#[test]
#[ignore]
fn measure_the_fbm_spread() {
    for octaves in [4u32, 5, 6, 7, 8, 18] {
        let mut v: Vec<f64> = spiral(20_000)
            .iter()
            .map(|&d| crate::field::fbm3(d * 12.0, 7, octaves))
            .collect();
        v.sort_by(f64::total_cmp);
        let at = |q: f64| v[((v.len() - 1) as f64 * q) as usize];
        let mean = v.iter().sum::<f64>() / v.len() as f64;
        let sd = (v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / v.len() as f64).sqrt();
        println!(
            "octaves {octaves}: min {:.3} p01 {:.3} p10 {:.3} mean {mean:.3} sd {sd:.3} p90 {:.3} p99 {:.3} max {:.3}",
            v[0], at(0.01), at(0.10), at(0.90), at(0.99), v[v.len() - 1]
        );
    }
}
/// What each planet in this repository's tests asks for, against the
/// `MAX_SLOPE` the mesher can close. A body over the line has every term
/// scaled back; the harness planet is well under it.
#[test]
#[ignore]
fn measure_the_slope_bound_on_each_test_planet() {
    use crate::biome::Shape;
    for (name, s) in [
        (
            "harness 1000 km",
            Shape {
                relief: 8000.0,
                lumps: 12.0,
                octaves: 18,
                seed: 7,
                radius: 1.0e6,
            },
        ),
        (
            "rough test ball",
            Shape {
                relief: 4.0,
                lumps: 6.0,
                octaves: 6,
                seed: 7,
                radius: 20.0,
            },
        ),
        (
            "slab test ball",
            Shape {
                relief: 4.0,
                lumps: 3.0,
                octaves: 3,
                seed: 1,
                radius: 100.0,
            },
        ),
    ] {
        println!(
            "{name}: relief/radius {:.4}, slope bound {:.3}",
            s.relief / s.radius,
            s.slope()
        );
    }
}
