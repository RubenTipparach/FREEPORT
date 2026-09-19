use super::*;
use crate::town;

/// A planet with a coast, mountains and towns on it, small enough to route
/// in a test. It is `chart`'s own test world, so a picture of one and a
/// network over the other are the same ground.
fn world() -> (Planet, f64, Vec<Town>) {
    let mut planet = Planet {
        radius: 40_000.0,
        relief: 400.0,
        lumps: 8.0,
        octaves: 9,
        overhang: 0.0,
        ledge: 0.0,
        seed: 5,
        sites: vec![],
    };
    let sea = planet.radius - 40.0;
    let towns = town::plan(&planet, sea, 80.0, 12, 5);
    planet.sites = towns.iter().map(town::site_of).collect();
    (planet, sea, towns)
}

/// The spacing a test routes at: coarse, because a test pays one
/// `surface_radius` a waypoint and the rules being held are about the
/// SHAPE of a network rather than about how finely it is sampled.
const COARSE: f64 = 0.06;

/// A planet with towns on it grows a road network, and every road joins
/// two real towns and ends in both of them.
#[test]
fn a_network_joins_towns_and_ends_in_them() {
    let (planet, sea, towns) = world();
    assert!(
        towns.len() > 4,
        "the test planet grew {} towns",
        towns.len()
    );
    let roads = connect(&planet, sea, &towns, COARSE);
    println!(
        "{} towns, {} roads, longest {:.0} km",
        towns.len(),
        roads.len(),
        roads
            .iter()
            .map(|r| r.length(planet.radius))
            .fold(0.0, f64::max)
            / 1000.0
    );
    assert!(!roads.is_empty(), "no town on this planet was joined");
    for r in &roads {
        assert!(r.from < towns.len() && r.to < towns.len() && r.from < r.to);
        let (a, b) = (&towns[r.from], &towns[r.to]);
        let first = r.line.first().expect("a road has a line").0;
        let last = r.line.last().expect("a road has a line").0;
        assert!(
            (first - a.dir).length() < 1e-9,
            "a road starts away from its own town"
        );
        assert!(
            (last - b.dir).length() < 1e-9,
            "a road ends away from the town it serves"
        );
    }
}

/// A road stays on LAND and off a cliff: every step of every line is over
/// the sea and no steeper than a road is built at.
///
/// The two ends are the towns' own centres and are exempt from the grade,
/// because a town levels its own ground and the step onto it is the site's
/// skirt rather than the road's.
#[test]
fn a_road_stays_out_of_the_sea_and_off_a_cliff() {
    let (planet, sea, towns) = world();
    let roads = connect(&planet, sea, &towns, COARSE);
    let (mut steps, mut worst) = (0usize, 0.0f64);
    for r in &roads {
        for (dir, h) in &r.line {
            let over = planet.radius + h - sea;
            assert!(
                over > 0.0,
                "a road runs {:.0} m under the sea at {dir}",
                -over
            );
        }
        for w in r.line.windows(2) {
            if w[0].0 == r.line[0].0 || w[1].0 == r.line[r.line.len() - 1].0 {
                continue;
            }
            let run = arc(w[0].0, w[1].0) * planet.radius;
            if run <= 0.0 {
                continue;
            }
            let grade = (w[1].1 - w[0].1).abs() / run;
            worst = worst.max(grade);
            steps += 1;
        }
    }
    println!(
        "{steps} steps of road, steepest grade 1 in {:.0}",
        1.0 / worst
    );
    assert!(steps > 20, "only {steps} steps of road to check");
    assert!(
        worst <= STEEPEST + 1e-9,
        "a road climbs at 1 in {:.1}, past the 1 in {:.0} it is built at",
        1.0 / worst,
        1.0 / STEEPEST
    );
}

/// The network is SPARSE and it is a network: far fewer roads than town
/// pairs, no town joined to itself twice, and most towns on it.
///
/// A road per pair would be a spiderweb and would say nothing about which
/// towns are actually neighbours; the border rule is what makes a road
/// mean "these two are next to each other".
#[test]
fn the_network_is_sparse_and_joined_up() {
    let (planet, sea, towns) = world();
    let roads = connect(&planet, sea, &towns, COARSE);
    let pairs = towns.len() * (towns.len() - 1) / 2;
    let mut seen = std::collections::BTreeSet::new();
    for r in &roads {
        assert!(seen.insert((r.from, r.to)), "two roads join one pair");
    }
    let joined: std::collections::BTreeSet<usize> =
        roads.iter().flat_map(|r| [r.from, r.to]).collect();
    println!(
        "{} roads of {pairs} possible pairs, {} of {} towns on the network",
        roads.len(),
        joined.len(),
        towns.len()
    );
    assert!(
        roads.len() < pairs / 2,
        "{} roads of {pairs} pairs is a spiderweb",
        roads.len()
    );
    assert!(
        joined.len() * 2 >= towns.len(),
        "only {} of {} towns are on a road",
        joined.len(),
        towns.len()
    );
}

/// The same planet routes to the same network twice. A road decides what
/// the ground under it is levelled to, so it is world state, and world
/// state two runs disagreed about is a world two clients would disagree
/// about.
#[test]
fn routing_the_same_planet_twice_gives_the_same_network() {
    let (planet, sea, towns) = world();
    let once = connect(&planet, sea, &towns, COARSE);
    let again = connect(&planet, sea, &towns, COARSE);
    assert_eq!(once, again, "two routings of one planet differ");
}

/// A body with no town, or one, has no road, and nothing panics asking.
#[test]
fn a_body_with_nothing_to_join_has_no_roads() {
    let (planet, sea, towns) = world();
    assert!(connect(&planet, sea, &[], COARSE).is_empty());
    assert!(connect(&planet, sea, &towns[..1], COARSE).is_empty());
    assert!(
        connect(&planet, sea, &towns, 0.0).is_empty(),
        "a spacing of nought asked for an infinite grid"
    );
}

/// The roads grow VILLAGES along them. A road exists because two cities
/// wanted to trade and what appears on it afterwards is everybody who
/// wanted to be on the way, which is the owner's own ask.
#[test]
fn villages_stand_along_the_roads() {
    let (planet, sea, towns) = world();
    let roads = connect(&planet, sea, &towns, COARSE);
    assert!(!roads.is_empty(), "no roads to grow anything on");
    let biggest = towns.iter().map(|t| t.radius).fold(0.0, f64::max);
    let grown = waysides(&planet, sea, &roads, &towns, biggest, 5);
    println!(
        "{} towns and {} roads grew {} villages of {:?} m",
        towns.len(),
        roads.len(),
        grown.len(),
        grown.iter().map(|t| t.radius.round()).collect::<Vec<_>>()
    );
    assert!(!grown.is_empty(), "no village on any road");
    for v in &grown {
        // ON a road: within a step of some point of some road's own line.
        let near = roads
            .iter()
            .flat_map(|r| &r.line)
            .map(|(d, _)| arc(*d, v.dir) * planet.radius)
            .fold(f64::MAX, f64::min);
        assert!(
            near < EVERY,
            "a village stands {near:.0} m off the nearest road"
        );
        // And it is a VILLAGE: what the same ground would have carried
        // as a CITY, cut by `WAYSIDE`, because a place that grew on the
        // way to somewhere is not a rival to the somewhere.
        //
        // NOT simply smaller than the smallest city, which is what this
        // held and what a shore village can legitimately beat: size is
        // how near the SEA a place stands, so a village on a beach is
        // 0.42 of a coastal size where a market town up a valley is the
        // 0.32 floor, and the village is the bigger. It passed for as
        // long as no road happened to reach a beach. What the cut
        // actually promises is this.
        let over = planet.radius + v.h - sea;
        let city = crate::town::size_of(biggest, over, &planet, v.index, 5);
        assert!(
            v.radius <= city * crate::town::WAYSIDE + 1e-9,
            "a village is {:.1} m across against {:.1} for a city on its own ground",
            v.radius,
            city * crate::town::WAYSIDE
        );
        assert!(
            v.radius < biggest,
            "a village is {:.0} m across against the biggest city's {biggest:.0}",
            v.radius
        );
    }
    // And they carry on from the cities' own indices, so two settlements
    // never share a seed and never come out the same town.
    for (i, v) in grown.iter().enumerate() {
        assert_eq!(v.index, towns.len() + i);
    }
}
