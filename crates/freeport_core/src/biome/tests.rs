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

/// The harness planet as the app builds it, so a rule about THAT body is
/// held where the field is rather than where the window is.
fn harness() -> (crate::field::Planet, f64) {
    (
        crate::field::Planet {
            radius: 1_000_000.0,
            relief: 8_000.0,
            lumps: 12.0,
            octaves: 18,
            overhang: 3.0,
            ledge: 12.0,
            seed: 7,
            sites: vec![].into(),
        },
        // `freeport_app`'s own SEA. The two are one number in two crates
        // and this test is what keeps them one: the core cannot read the
        // app's constant, so it asserts the property the constant is set
        // for, and a sea moved there fails here.
        1_000_000.0 + 1100.0,
    )
}

/// An even spread of directions over a body: the golden angle spiral,
/// which is what every other sampler here uses.
fn spread(n: usize) -> impl Iterator<Item = DVec3> {
    let golden = std::f64::consts::PI * (3.0 - 5f64.sqrt());
    (0..n).map(move |i| {
        let y = 1.0 - 2.0 * (i as f64 + 0.5) / n as f64;
        let s = (1.0 - y * y).max(0.0).sqrt();
        let a = golden * i as f64;
        DVec3::new(s * a.cos(), y, s * a.sin())
    })
}

/// The harness planet is MOSTLY WATER, which is the owner's ask: at least
/// half of it under the sea.
///
/// A sea level is a percentile of the body's own heights and not a number
/// that means anything by itself. The relief here spans -2,920 to 4,358 m
/// with its median at +351, so the sea at -400 m left the world 26.3%
/// water: a continent with lakes in it rather than an ocean with
/// continents in it.
#[test]
fn the_harness_planet_is_mostly_water() {
    let (planet, sea) = harness();
    let shape = Shape::of(&planet);
    let n = 20_000;
    let wet = spread(n)
        .filter(|d| planet.radius + shape.height(*d) < sea)
        .count();
    let share = wet as f64 / n as f64;
    println!(
        "{:.1}% of the harness planet is under the sea",
        share * 100.0
    );
    assert!(
        share >= 0.5,
        "the body is {:.1}% water, and the ask is at least half",
        share * 100.0
    );
    // And not ALL water: a world with no land is not a world to land on.
    assert!(
        share < 0.9,
        "the body is {:.1}% water, which is a sea with nothing in it",
        share * 100.0
    );
    // What the body is MADE of, for the record: a sea level is the one
    // number that moves every one of these at once.
    let mut count = std::collections::BTreeMap::new();
    for d in spread(8_000) {
        let over = planet.radius + shape.height(d) - sea;
        *count
            .entry(shape.climate(d, over).kind(over, 0.0).name())
            .or_insert(0usize) += 1;
    }
    let mut by_size: Vec<_> = count.into_iter().collect();
    by_size.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    println!("of 8,000 directions: {by_size:?}");
}

/// No city stands on FROZEN ground. The ice caps are what the owner
/// pointed at, and the gate is the climate's own `frozen`, so a town can
/// never stand where the chart paints snow or ice.
#[test]
fn no_town_stands_on_the_ice() {
    let (mut planet, sea) = harness();
    // A smaller body, so the plan is a test and not a bake, with the same
    // relief in proportion and the same sea level as a share of it.
    planet.radius = 60_000.0;
    planet.relief = 900.0;
    planet.octaves = 9;
    let sea = planet.radius + (sea - 1_000_000.0) * 900.0 / 8_000.0;
    let shape = Shape::of(&planet);
    let towns = crate::town::plan(&planet, sea, 80.0, 16, 5);
    assert!(!towns.is_empty(), "the test body grew no town at all");
    let mut coldest: f64 = 1.0;
    for t in &towns {
        let over = planet.radius + t.h - sea;
        let climate = shape.climate(t.dir, over);
        coldest = coldest.min(climate.temp);
        assert!(
            !climate.frozen(),
            "a town stands on frozen ground at {:.2} temp, {:.0} m over the sea",
            climate.temp,
            over
        );
    }
    println!(
        "{} towns, the coldest at {coldest:.2} against a freezing line of {FREEZING}",
        towns.len()
    );
}

/// Every connected piece of LAND on a body, by the share of the whole
/// sphere's area each covers, biggest first.
///
/// An equirect grid because that is what the chart is and what a picture
/// of the body shows; the rows are weighted by the cosine of their own
/// latitude, or a speck at the pole would count for as much ground as a
/// continent at the equator. Row nought is a ring of cells round the pole
/// and every one of them is joined to the next going east, so the pole
/// needs no case of its own.
fn landmasses(planet: &crate::field::Planet, sea: f64, w: usize, h: usize) -> Land {
    let shape = Shape::of(planet);
    let mut land = vec![false; w * h];
    let row = |v: usize| std::f64::consts::PI * ((v as f64 + 0.5) / h as f64 - 0.5);
    let area: Vec<f64> = (0..h).map(|v| row(v).cos()).collect();
    for v in 0..h {
        let lat = row(v);
        for u in 0..w {
            let lon = std::f64::consts::TAU * (u as f64 + 0.5) / w as f64;
            let dir = DVec3::new(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin());
            land[v * w + u] = planet.radius + shape.height(dir) >= sea;
        }
    }
    // Union find, joined east (wrapping) and south.
    let mut up: Vec<usize> = (0..w * h).collect();
    fn root(up: &mut [usize], mut i: usize) -> usize {
        while up[i] != i {
            up[i] = up[up[i]];
            i = up[i];
        }
        i
    }
    let join = |up: &mut [usize], a: usize, b: usize| {
        let (a, b) = (root(up, a), root(up, b));
        if a != b {
            up[a] = b;
        }
    };
    for v in 0..h {
        for u in 0..w {
            let i = v * w + u;
            if !land[i] {
                continue;
            }
            let e = v * w + (u + 1) % w;
            if land[e] {
                join(&mut up, i, e);
            }
            if v + 1 < h && land[(v + 1) * w + u] {
                join(&mut up, i, (v + 1) * w + u);
            }
        }
    }
    let whole: f64 = area.iter().sum::<f64>() * w as f64;
    let mut size = std::collections::BTreeMap::new();
    for (v, a) in area.iter().enumerate() {
        for u in 0..w {
            let i = v * w + u;
            if land[i] {
                *size.entry(root(&mut up, i)).or_insert(0.0) += a / whole;
            }
        }
    }
    // And the share each TEXEL's own piece is worth, so a town can be
    // asked how big the land under it is.
    let mut at = vec![0.0f64; w * h];
    for v in 0..h {
        for u in 0..w {
            let i = v * w + u;
            if land[i] {
                at[i] = size[&root(&mut up, i)];
            }
        }
    }
    let mut pieces: Vec<f64> = size.into_values().collect();
    pieces.sort_by(|a, b| b.total_cmp(a));
    Land { pieces, at, w, h }
}

/// A body's land, as connected pieces and as a map of which piece is
/// under a direction.
struct Land {
    /// The share of the whole sphere each piece covers, biggest first.
    pieces: Vec<f64>,
    /// The share the piece under each texel is worth, nought at sea.
    at: Vec<f64>,
    w: usize,
    h: usize,
}

impl Land {
    /// How big the landmass under a direction is, as a share of the whole
    /// body. Nought if the direction is at sea.
    fn under(&self, dir: DVec3) -> f64 {
        let d = dir.normalize();
        let v =
            ((d.y.clamp(-1.0, 1.0).asin() / std::f64::consts::PI + 0.5) * self.h as f64) as usize;
        let lon = d.z.atan2(d.x).rem_euclid(std::f64::consts::TAU);
        let u = (lon / std::f64::consts::TAU * self.w as f64) as usize;
        self.at[v.min(self.h - 1) * self.w + u.min(self.w - 1)]
    }
}

/// The land on this body is a few BIG CONTINENTS with a great many
/// islands off them, which is the owner's own ask rather than anything a
/// single fractal gives on its own.
///
/// What decides a continent AT ALL is the SHELF (`biome::SHELF_AT` and
/// its neighbours), and what it replaced could not have one. A continent
/// term that is a smooth swell has a coastal gradient of about 22 m a
/// kilometre here while the hills riding it are worth 34, so the hills
/// decided land from water over most of the swell: the body came out as
/// three hundred middling blobs with no continent anywhere.
///
/// What decides how BIG one is, which is the owner's next word on it, is
/// `freq::CONTINENT`, the size of the swell itself. At 0.30 of the
/// planet's lumps the body was 62.2% water in 7 pieces of 17.9, 5.3, 4.1,
/// 2.2, 2.1, 2.0 and 1.2%, which is one continent and six scraps; at 0.18
/// with the sea 100 m higher it is 57.8% water in 3 pieces of 17.9, 11.0
/// and 10.1% with 381 islands, so the second is twice what it was and the
/// third two and a half times.
///
/// The COUNT falls when the size rises and that is arithmetic rather than
/// a setting: land is a level set of a fractal, so a swell twice as wide
/// crosses the sea half as often. Earth is the scale to read this at,
/// because its seven named continents are four contiguous masses of 16.6,
/// 8.2, 2.7 and 1.5% of the globe at 71% water; this body's three are
/// bigger than any of them and it is 58% water, so it is a MORE
/// continental world than Earth rather than a less divided one.
/// `measure_the_land_at_each_sea_level` is the sweep either side of it.
///
/// So what this holds is the SIZE, which is what was asked for, and the
/// islands, and never a count, which is the thing that cannot be set.
#[test]
fn the_land_is_a_few_continents_and_many_islands() {
    let (planet, sea) = harness();
    let land = landmasses(&planet, sea, 480, 240);
    let pieces = &land.pieces;
    let land: f64 = pieces.iter().sum();
    // A CONTINENT is a piece worth a per cent of the whole body: on this
    // radius that is 126,000 square kilometres, about the size of Greece
    // and half of Britain, so the line is drawn where an island stops
    // being somewhere you could drive across.
    let big: Vec<f64> = pieces.iter().copied().filter(|&a| a >= 0.01).collect();
    println!(
        "{:.1}% land in {} pieces: {} continents at {:?}%, {} islands",
        land * 100.0,
        pieces.len(),
        big.len(),
        big.iter()
            .map(|a| format!("{:.1}", a * 100.0))
            .collect::<Vec<_>>(),
        pieces.len() - big.len()
    );
    assert!(
        big.len() >= 3,
        "the body has {} continents, and a world needs a few",
        big.len()
    );
    // The third biggest is a real continent and not a scrap off the
    // biggest: 5% of this body is 630,000 square kilometres, which is
    // bigger than Earth's own third mass. It is the THIRD rather than the
    // biggest because one big piece and a fringe of leftovers is exactly
    // what the owner was looking at when they said the land masses were
    // too small.
    let mut order = big.clone();
    order.sort_by(|a, b| b.total_cmp(a));
    assert!(
        order[2] >= 0.05,
        "the third continent is {:.1}% of the body, which is a scrap",
        order[2] * 100.0
    );
    assert!(
        pieces.len() - big.len() >= 40,
        "only {} islands off {} continents",
        pieces.len() - big.len(),
        big.len()
    );
}

/// What the land looks like at each sea level a body could be given, for
/// picking one. Ignored, because it is a sweep rather than a rule.
#[test]
#[ignore]
fn measure_the_land_at_each_sea_level() {
    let (planet, _) = harness();
    for over in [400.0f64, 700.0, 1000.0, 1300.0, 1600.0] {
        let sea = planet.radius + over;
        let pieces = landmasses(&planet, sea, 480, 240).pieces;
        let land: f64 = pieces.iter().sum();
        let big: Vec<String> = pieces
            .iter()
            .filter(|&&a| a >= 0.01)
            .map(|a| format!("{:.1}", a * 100.0))
            .collect();
        println!(
            "sea +{over:>6.0} m: {:.1}% water, {} continents {big:?}, {} islands",
            (1.0 - land) * 100.0,
            big.len(),
            pieces.len() - big.len()
        );
    }
}

/// Towns stand ALL OVER a body: inland and on its plateaus as much as on
/// its shores, and on its ISLANDS as much as on its continents. Both are
/// the owner's own ask, and the second is why the first matters.
///
/// What this holds is the ORDER `town::in_order` takes qualifying sites
/// in. Taken lowest first, which is what `plan` did, all 160 towns came
/// out between 3 and 35 m over the sea on a body with eight kilometres of
/// relief: a ring of ports round every coast and not one city inland.
/// They span 3 m to 2,260 m now, with a quarter of them over a kilometre
/// up.
#[test]
fn towns_stand_inland_and_on_islands() {
    let (planet, sea) = harness();
    let towns = crate::town::plan(&planet, sea, 80.0, 160, planet.seed);
    assert_eq!(towns.len(), 160, "the body did not grow its towns");
    let mut hs: Vec<f64> = towns.iter().map(|t| planet.radius + t.h - sea).collect();
    hs.sort_by(f64::total_cmp);
    let at = |q: f64| hs[((hs.len() - 1) as f64 * q) as usize];
    println!(
        "160 towns over the sea: min {:.0} p25 {:.0} median {:.0} p75 {:.0} max {:.0} m",
        hs[0],
        at(0.25),
        at(0.5),
        at(0.75),
        hs[hs.len() - 1]
    );
    // The PORT is the BIGGEST town on the body and it is COASTAL, which
    // is the same fact twice: size is how near the SEA a town stands, so
    // the biggest is on the shore. Nearest by DISTANCE and not lowest,
    // because a shore can stand high and a basin low and far inland.
    let shore = crate::town::Shore::of(&planet, sea);
    let mut off: Vec<f64> = towns.iter().map(|t| shore.distance(t.dir)).collect();
    off.sort_by(f64::total_cmp);
    let port = shore.distance(towns[0].dir);
    assert!(
        towns[1..].iter().all(|t| towns[0].radius >= t.radius),
        "the port is not the biggest town on the body"
    );
    println!(
        "the port is {:.0} m across, {:.0} m over the sea and {:.0} m from it; the towns stand {:.0} to {:.0} m from the sea, median {:.0}",
        towns[0].radius,
        planet.radius + towns[0].h - sea,
        port,
        off[0],
        off[off.len() - 1],
        off[off.len() / 2]
    );
    assert!(
        port <= off[off.len() / 4],
        "the port stands {port:.0} m from the sea, past a quarter of the towns on the body"
    );
    // A quarter of them are a long way up, which is what says the height
    // ceiling is doing anything at all.
    assert!(
        at(0.75) > planet.relief * 0.05,
        "three quarters of the towns are under {:.0} m, which is a coastline",
        at(0.75)
    );
    // BIG CITIES ARE COASTAL and small ones are inland, which is the
    // owner's own observation and most of economic geography: a port
    // trades with the whole world and an inland town with its own
    // valley. The law is `town::coastal` and this is what it buys.
    let mut by_size: Vec<(f64, f64)> = towns
        .iter()
        .map(|t| (t.radius, shore.distance(t.dir)))
        .collect();
    by_size.sort_by(|a, b| b.0.total_cmp(&a.0));
    let quarter = by_size.len() / 4;
    let mean = |v: &[(f64, f64)]| v.iter().map(|p| p.1).sum::<f64>() / v.len() as f64;
    let (big, small) = (mean(&by_size[..quarter]), mean(&by_size[3 * quarter..]));
    println!(
        "the biggest quarter of the towns stand {big:.0} m from the sea and the smallest {small:.0}"
    );
    assert!(
        small > big * 3.0,
        "the biggest towns are {big:.0} m from the sea and the smallest {small:.0}: size says nothing about the coast"
    );

    // And the ISLANDS have cities on them. A continent is a piece worth a
    // per cent of the body; anything smaller is an island.
    let land = landmasses(&planet, sea, 480, 240);
    let on_islands = towns.iter().filter(|t| land.under(t.dir) < 0.01).count();
    let tiny = towns
        .iter()
        .filter(|t| {
            let a = land.under(t.dir);
            a > 0.0 && a < 0.001
        })
        .count();
    println!(
        "{on_islands} of 160 towns stand on an island rather than a continent, {tiny} of them on one under a thousandth of the body"
    );
    assert!(
        on_islands >= 10,
        "only {on_islands} of 160 towns are on islands"
    );
}
