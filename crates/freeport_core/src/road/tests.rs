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
        sites: vec![].into(),
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

/// A CORRIDOR follows the country rather than ramping straight through
/// it: every piece is cut to the ground its own two ends stand on, and
/// what it cuts is a verge rather than a canyon.
#[test]
fn a_corridor_follows_the_ground_and_stops_short_of_its_towns() {
    let (planet, sea, towns) = world();
    let levelled = crate::field::Planet {
        sites: towns.iter().map(crate::town::site_of).collect(),
        ..planet.clone()
    };
    let roads = connect(&levelled, sea, &towns, SPACING);
    let road = roads.first().expect("a road between two of them");
    let run = survey(&levelled, road, sea - planet.radius + 2.0);
    let line = centreline(road, planet.radius);
    assert_eq!(run.len(), line.len(), "a height a point and no more");
    // Every piece is about `PIECE` long, which is what the atlas's own
    // ten kilometre waypoints are refined to.
    for pair in line.windows(2) {
        let run_m = pair[0].angle_between(pair[1]) * planet.radius;
        assert!(
            run_m <= PIECE + 1e-6,
            "a piece {run_m:.0} m long against a {PIECE} m limit"
        );
    }
    // What it CUTS is the gap between the levelled corridor and the bare
    // ground, and it is a verge rather than a canyon.
    let mut worst = 0.0f64;
    for (dir, h) in line.iter().zip(&run) {
        let bare = crate::town::surface_radius(&levelled.around(*dir, 1e-9), *dir) - planet.radius;
        worst = worst.max((bare - h).abs());
    }
    // A road held to SEVEN PER CENT over ground that is steeper than
    // that has to cut, and this fixture's ball carries 400 m of relief
    // on a 40 km radius, which is ground no highway crosses on the
    // level. The grade is the promise the owner asked for and it wins:
    // `a_corridor_is_never_steeper_than_a_road_is_built` is the hard
    // one, and this is how much ground the corridor has to move to keep
    // it. Its own relief is the bound, because a cut deeper than the
    // hill is a cut through the planet.
    println!("the corridor moves {worst:.1} m of ground at its worst");
    assert!(
        worst < planet.relief * 0.25,
        "the corridor moves {worst:.1} m of ground against {:.0} m of relief",
        planet.relief
    );
    // And it stops short of both towns, so a town's disc owns its ground.
    let discs: crate::field::Sites = towns.iter().map(crate::town::site_of).collect();
    let sites = corridor(road, &run, planet.radius, &discs);
    let town = crate::town::site_of(&towns[road.from]);
    assert!(
        !sites.is_empty(),
        "a road with no corridor is a road on data"
    );
    let centre = towns[road.from].dir;
    for site in &sites {
        for end in [site.dir, site.to] {
            // Outside the town's own levelling AT THAT BEARING, which
            // is what owns the ground there. Against the town's WIDEST
            // instead, a road out along the squeezed axis stopped three
            // hundred metres short of anything the town had levelled
            // and the tarmac ended in a field.
            let skip = town.level_r(end) + crate::field::site_skirt(&town);
            assert!(
                end.angle_between(centre) * planet.radius > skip,
                "a corridor piece reaches inside the town it serves"
            );
        }
    }
    // Consecutive pieces MEET, which is what lets the field's slope
    // bound assume two overlapping skirts rather than a count.
    for pair in sites.windows(2) {
        assert_eq!(pair[0].to, pair[1].dir, "a gap between two pieces");
        assert_eq!(pair[0].to_h, pair[1].h, "a step between two pieces");
    }
}

/// The corridor's own GRADE is the one the route was allowed, between
/// the pieces the router never looked at as well as between its own
/// waypoints.
#[test]
fn a_corridor_is_never_steeper_than_a_road_is_built() {
    let (planet, sea, towns) = world();
    let levelled = crate::field::Planet {
        sites: towns.iter().map(crate::town::site_of).collect(),
        ..planet.clone()
    };
    for road in connect(&levelled, sea, &towns, SPACING).iter().take(6) {
        let run = survey(&levelled, road, sea - planet.radius + 2.0);
        let line = centreline(road, planet.radius);
        for (pair, h) in line.windows(2).zip(run.windows(2)) {
            let along = pair[0].angle_between(pair[1]) * planet.radius;
            let grade = (h[1] - h[0]).abs() / along.max(1e-9);
            assert!(
                grade <= crate::road::STEEPEST + 1e-9,
                "the corridor climbs at {:.1}% against the {:.0}% a highway is built at",
                grade * 100.0,
                crate::road::STEEPEST * 100.0
            );
        }
    }
}

/// THE TARMAC IS ON THE GROUND. Every vertex of every stretch stands
/// `ribbon::LIFT` over the surface the corridor levelled under it, which
/// is what makes a road a road and not a ribbon floating over a hill:
/// the levelling and the tarmac read the same survey, so they cannot
/// drift.
#[test]
fn the_tarmac_lands_on_the_ground_its_corridor_levelled() {
    use crate::road::ribbon;
    let (planet, sea, towns) = world();
    let mut levelled = crate::field::Planet {
        sites: towns.iter().map(crate::town::site_of).collect(),
        ..planet.clone()
    };
    let roads = connect(&levelled, sea, &towns, SPACING);
    let road = roads.first().expect("a road");
    let run = survey(&levelled, road, sea - planet.radius + DRY);
    let discs: crate::field::Sites = towns.iter().map(crate::town::site_of).collect();
    let mut sites: Vec<_> = discs.iter().copied().collect();
    sites.extend(corridor(road, &run, planet.radius, &discs));
    levelled.sites = sites.into();

    let line = centreline(road, planet.radius);
    let open = open(&line, planet.radius, &discs);
    let lamps = lit(&line, planet.radius, &discs);
    let (mut over_most, mut under_most) = (0.0f64, 0.0f64);
    let mut vertices = 0;
    for k in 0..ribbon::count(line.len()) {
        let at = ribbon::span(k, line.len());
        let (l, r, o, t) = (
            &line[at.clone()],
            &run[at.clone()],
            &open[at.clone()],
            &lamps[at],
        );
        if l.len() < 2 {
            continue;
        }
        let frame = ribbon::frame(l, r, planet.radius);
        let m = ribbon::stretch(&frame, l, r, o, t, planet.radius);
        assert!(m.solids.is_empty(), "tarmac collides with nothing");
        // The road SURFACE alone: a lamp post stands seven metres up and
        // is not tarmac, which is what the material byte is for.
        let surface: Vec<_> = m
            .mesh
            .materials
            .iter()
            .enumerate()
            .filter(|(_, mat)| **mat == crate::field::STREET || **mat == crate::field::PAINT)
            .flat_map(|(t, _)| m.mesh.positions[t * 3..t * 3 + 3].to_vec())
            .collect();
        for p in &surface {
            let world = frame.world(glam::Vec3::from(*p).as_dvec3());
            let dir = world.normalize();
            // The ground the field actually holds under this vertex.
            let ground = planet.radius + levelled.surface(dir).0;
            let over = world.length() - ground;
            over_most = over_most.max(over);
            under_most = under_most.min(over);
            vertices += 1;
        }
    }
    assert!(vertices > 100, "{vertices} vertices is not a road");
    // ON the ground. `MITRE` is the error the cross section's own mitre
    // makes at a bend, measured on this road at 0.103 m: the outer
    // corner of a piece sits a little along the ramp from the station it
    // belongs to, and the ramp is at a different height there. Nothing
    // floats more than the surfacing plus that, so the tarmac is laid on
    // the ground and not over it; and the shoulder is under the ground
    // even at its worst, so there is no crack for the field to show
    // through.
    //
    // And the PROBES are a floor on the two stations either side of
    // them now rather than a lift on the chord between them, which is
    // what makes the grade exact: a station stands at the highest
    // ground within its own piece, so the tarmac rides over the rest of
    // that piece by up to what the ground falls across it. That is an
    // embankment, which is the thing a road has and the thing this
    // terrain can draw.
    const MITRE: f64 = 0.12;
    const EMBANKED: f64 = 0.35;
    assert!(
        over_most <= ribbon::LIFT + MITRE + EMBANKED,
        "the tarmac floats {over_most:.3} m over its own ground"
    );
    assert!(
        under_most < -MITRE && under_most > -0.35,
        "the shoulder's own edge reaches {under_most:.3} m, which is not buried"
    );
}

/// A straight run of lit road, for measuring what is PAINTED on it and
/// what stands beside it. Everything open, everything lit, flat ground.
fn lit_run(radius: f64, points: usize) -> (crate::model::Model, Vec<DVec3>) {
    use crate::road::ribbon;
    let line: Vec<DVec3> = (0..points)
        .map(|i| {
            let a = i as f64 * PIECE / radius;
            DVec3::new(a.sin(), 0.0, a.cos())
        })
        .collect();
    let (run, open, on) = (vec![0.0; points], vec![true; points], vec![true; points]);
    let frame = ribbon::frame(&line, &run, radius);
    let model = ribbon::stretch(&frame, &line, &run, &open, &on, radius);
    let local = line
        .iter()
        .map(|d| frame.local(*d * (radius + ribbon::LIFT)))
        .collect();
    (model, local)
}

/// How far a point stands to the side of a road's local centreline, and
/// how far along it: the road is straight here, so its own direction is
/// end to end and the across is square to that in the tangent plane.
fn off_road(local: &[DVec3], p: DVec3) -> (f64, f64) {
    let along = (local[local.len() - 1] - local[0]).normalize_or(DVec3::Y);
    let across = DVec3::new(along.y, -along.x, 0.0).normalize_or(DVec3::X);
    ((p - local[0]).dot(along), (p - local[0]).dot(across))
}

/// THE CENTRELINE IS DASHED, in three metre dashes and not in three
/// hundred and forty one metre ones.
///
/// A dash was asked once a PIECE, so it came out as a whole piece of
/// solid paint and then two whole pieces of nothing: the first picture
/// of a road showed two edge lines and no middle at all, because the
/// piece the camera stood on had fallen in a gap. What this measures is
/// that a third of the centreline is paint and that no single mark is
/// longer than one dash.
#[test]
fn the_centreline_is_dashed_in_dashes_and_not_in_pieces() {
    use crate::road::ribbon;
    const RADIUS: f64 = 1_000_000.0;
    let (m, local) = lit_run(RADIUS, ribbon::STRETCH + 1);
    let (mut painted, mut longest) = (0.0, 0.0f64);
    for (t, mat) in m.mesh.materials.iter().enumerate() {
        if *mat != crate::field::PAINT {
            continue;
        }
        let p: Vec<DVec3> = (0..3)
            .map(|i| glam::Vec3::from(m.mesh.positions[t * 3 + i]).as_dvec3())
            .collect();
        let (at, off): (Vec<f64>, Vec<f64>) = p.iter().map(|q| off_road(&local, *q)).unzip();
        // The EDGE lines stand a metre out; only the middle is dashed.
        if off.iter().any(|o| o.abs() > 0.5) {
            continue;
        }
        painted += (p[1] - p[0]).cross(p[2] - p[0]).length() * 0.5;
        longest = longest.max(
            at.iter().copied().fold(f64::MIN, f64::max)
                - at.iter().copied().fold(f64::MAX, f64::min),
        );
    }
    let length = (local[local.len() - 1] - local[0]).length();
    let share = painted / ribbon::PAINT_W / length;
    let want = ribbon::DASH / (ribbon::DASH + ribbon::GAP);
    assert!(
        (share - want).abs() < 0.02,
        "{:.1}% of the centreline is paint and {:.1}% should be",
        share * 100.0,
        want * 100.0
    );
    assert!(
        longest <= ribbon::DASH + 0.05,
        "the longest mark on the centreline is {longest:.1} m against a dash of {:.1}",
        ribbon::DASH
    );
}

/// THE LAMPS ON A LIT APPROACH STAND A STRIDE APART AND ALTERNATE SIDES,
/// rather than one a kilometre all down one side.
///
/// They were placed one every third PIECE, which is the dashes' own
/// mistake: the port's lit approach is about a kilometre of open road
/// and it carried exactly one light, standing where the camera was, so
/// the night picture of a road had nothing on it at all.
#[test]
fn the_lamps_on_an_approach_are_staggered_a_stride_apart() {
    use crate::road::ribbon;
    const RADIUS: f64 = 1_000_000.0;
    let (m, local) = lit_run(RADIUS, ribbon::STRETCH + 1);
    let mut posts: Vec<(f64, f64)> = m.lamps.iter().map(|p| off_road(&local, *p)).collect();
    posts.sort_by(|a, b| a.0.total_cmp(&b.0));
    let length = (local[local.len() - 1] - local[0]).length();
    assert!(
        posts.len() as f64 > length / ribbon::LAMP_EVERY - 2.0,
        "{} lamps over {length:.0} m is not an approach lit every {:.0} m",
        posts.len(),
        ribbon::LAMP_EVERY
    );
    for pair in posts.windows(2) {
        let gap = pair[1].0 - pair[0].0;
        assert!(
            (gap - ribbon::LAMP_EVERY).abs() < 1.0,
            "two lamps stand {gap:.1} m apart"
        );
        assert!(
            pair[0].1 * pair[1].1 < 0.0,
            "two lamps in a row stand on the same side, at {:.2} and {:.2}",
            pair[0].1,
            pair[1].1
        );
    }
}

/// A road RIDES over the ground rather than cutting into it, which is
/// what keeps the terrain off the top of it.
///
/// A CUT is a feature the terrain's own LOD cannot hold: the corridor is
/// `CORRIDOR` (7 m) either side of the centreline and the rings put a
/// cell of about a sixty fourth of its own distance under the eye, so
/// past a couple of hundred metres the mesher has no sample inside the
/// cutting, draws the hill that was there before the road, and the
/// ground closes over the tarmac. A FILL the mesher loses leaves the
/// road standing a little proud of the ground, which is an embankment.
/// So what this holds is the CUT, and the fill is only reported.
#[test]
fn a_road_rides_over_the_ground_rather_than_cutting_into_it() {
    let (planet, sea, towns) = world();
    let levelled = crate::field::Planet {
        sites: towns.iter().map(crate::town::site_of).collect(),
        ..planet.clone()
    };
    let roads = connect(&levelled, sea, &towns, SPACING);
    let dry = sea - planet.radius + 2.0;
    let (mut cut, mut fill, mut n) = (0.0f64, 0.0f64, 0usize);
    for road in roads.iter().take(6) {
        let run = survey(&levelled, road, dry);
        let line = centreline(road, planet.radius);
        if run.len() != line.len() {
            continue;
        }
        // ALONG the chord between every pair of stations, because that
        // is what the tarmac is laid on and what the corridor levels to,
        // and the ground between two stations is exactly what a station
        // sample by itself could not see.
        const STEPS: usize = 8;
        for (w, h) in line.windows(2).zip(run.windows(2)) {
            for k in 0..=STEPS {
                let dir = crate::road::step(w[0], w[1], k, STEPS);
                let bare =
                    crate::town::surface_radius(&levelled.around(dir, 1e-9), dir) - planet.radius;
                let here = h[0] + (h[1] - h[0]) * (k as f64 / STEPS as f64);
                cut = cut.max(bare - here);
                fill = fill.max(here - bare);
                n += 1;
            }
        }
    }
    println!(
        "over {n} points a road stands {fill:.2} m over its own ground at most and {cut:.2} m under it"
    );
    assert!(
        cut < 1.0,
        "a road cuts {cut:.2} m into its own ground, which a coarse chunk cannot hold and the terrain then closes over"
    );
}

/// A highway JOINS a town: its slip reaches a crossing the town paved,
/// and it lands on the ground the whole way.
///
/// What this replaces measured the run in, which was a MASK: the tarmac
/// carried on past `open` and stopped `MEET` from the nearest piece in
/// whatever direction that happened to be. It passed at 0.51 m for as
/// long as the pieces it described were never drawn, and it could not
/// have failed, because `road::clear` stops the tarmac at `MEET` and the
/// test then measured the distance to the nearest piece. The moment
/// `ribbon::stretch` stopped throwing those pieces away, the tarmac they
/// drew floated 1.790 m over the town's own plateau.
///
/// So a slip is GEOMETRY that reads the ground, and the two halves of
/// that are what this holds: it ENDS on a crossing, and no point of it
/// stands off the ground the field actually makes there.
#[test]
fn a_highway_joins_a_town_at_a_crossing_and_lands_on_its_ground() {
    let (planet, sea, towns) = world();
    let levelled = crate::field::Planet {
        sites: towns.iter().map(crate::town::site_of).collect(),
        ..planet.clone()
    };
    let roads = connect(&levelled, sea, &towns, SPACING);
    let discs: crate::field::Sites = towns.iter().map(crate::town::site_of).collect();
    let (mut worst_gap, mut worst_float) = (0.0f64, 0.0f64);
    let mut seen = 0;
    let mut worst_sunk = f64::NEG_INFINITY;
    let mut worst_over_mouth = f64::NEG_INFINITY;
    for road in roads.iter().take(6) {
        let line = centreline(road, planet.radius);
        let open = crate::road::open(&line, planet.radius, &discs);
        let Some(first) = open.iter().position(|o| *o) else {
            continue;
        };
        let town = &towns[road.from];
        let at = line[first];
        let along = (at - line[(first + 1).min(line.len() - 1)]).normalize_or(at);
        // The height the HIGHWAY itself stands at where the two meet,
        // which is what a slip is anchored to: started at the ground
        // instead it puts a step at the mouth of however far `smooth`
        // raised the profile over it, in one three metre piece.
        let run = survey(&levelled, road, sea - planet.radius + DRY);
        let slip = crate::road::slip(&levelled, town, at, run[first], along, planet.radius);
        if slip.len() < 2 {
            continue;
        }
        // It ENDS on a crossing the town actually paved.
        let (end, _) = slip[slip.len() - 1];
        let gap = town
            .pieces
            .iter()
            .map(|p| {
                let dir =
                    (town.dir * planet.radius + town.east * p.x + town.north * p.z).normalize();
                dir.angle_between(end) * planet.radius - p.w.max(p.d) * 0.5
            })
            .fold(f64::INFINITY, f64::min)
            .max(0.0);
        worst_gap = worst_gap.max(gap);
        // And it RIDES its own ground: over it everywhere, by the
        // street's own five centimetres at the crossing and never by
        // more than the HIGHWAY's own tarmac stands over the ground
        // where the two meet. Under it ANYWHERE is the defect the
        // pictures showed, which is a road drawn below the hill it was
        // laid on.
        //
        // The bound is the highway's own float rather than a constant,
        // because the slip's first point IS the highway's last one: the
        // step between the baked profile and the ground it was raised
        // off is tapered out along the slip, so nothing on it can stand
        // higher than the end that step is at.
        let mouth_over = run[first] + ribbon::LIFT
            - (crate::town::surface_radius(&levelled, at) - planet.radius);
        for (dir, h) in &slip {
            let ground = crate::town::surface_radius(&levelled, *dir) - planet.radius;
            let over = h + ribbon::LIFT - ground;
            worst_over_mouth = worst_over_mouth.max(over - mouth_over);
            worst_float = worst_float.max(over);
            worst_sunk = worst_sunk.max(-over);
        }
        seen += 1;
    }
    println!(
        "{seen} slips end {worst_gap:.2} m from a crossing and stand {worst_float:.3} m over their own ground, {worst_sunk:.3} m under it and {worst_over_mouth:.3} m over their own highway at the worst"
    );
    assert!(seen > 0, "no road laid a slip at all");
    assert!(
        worst_gap < 2.0,
        "a slip ends {worst_gap:.2} m short of the crossing it is supposed to join"
    );
    assert!(
        worst_sunk <= 0.0,
        "a slip stands {worst_sunk:.3} m INTO the ground it is laid on"
    );
    assert!(
        worst_over_mouth <= 1e-6,
        "a slip stands {worst_over_mouth:.3} m higher over its ground than the highway it leaves"
    );
}
