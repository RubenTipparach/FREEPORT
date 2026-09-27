use super::*;
use crate::field::Planet;
use crate::town;

/// The harness body's radius, metres, so a degree of arc is a real
/// seventeen and a half kilometres of road.
const R: f64 = 1_000_000.0;

/// A direction at a latitude and a longitude, degrees.
fn at(lat: f64, lon: f64) -> DVec3 {
    let (la, lo) = (lat.to_radians(), lon.to_radians());
    DVec3::new(la.cos() * lo.cos(), la.sin(), la.cos() * lo.sin())
}

/// A road from one town to another through the given points.
fn road(from: usize, to: usize, line: &[DVec3]) -> Road {
    Road {
        from,
        to,
        line: line.iter().map(|d| (*d, 0.0)).collect(),
    }
}

/// Three towns on the equator, A at nought, B at two degrees and C at
/// four, joined A to B and B to C: the only way from A to C is THROUGH
/// B, which is a node both roads end at.
fn chain() -> Vec<Road> {
    vec![
        road(0, 1, &[at(0.0, 0.0), at(0.0, 1.0), at(0.0, 2.0)]),
        road(1, 2, &[at(0.0, 2.0), at(0.0, 3.0), at(0.0, 4.0)]),
    ]
}

/// A trunk that FORKS: both roads leave town 0 on the same two
/// waypoints, and at the second road 1 turns north to town 2 while road
/// 0 carries on east to town 1. Road 0 is the lower numbered, so it
/// owns the trunk.
fn fork() -> Vec<Road> {
    vec![
        road(
            0,
            1,
            &[at(0.0, 0.0), at(0.0, 1.0), at(0.0, 2.0), at(0.0, 3.0)],
        ),
        road(
            0,
            2,
            &[
                at(0.0, 0.0),
                at(0.0, 1.0),
                at(0.0, 2.0),
                at(1.0, 2.0),
                at(2.0, 2.0),
            ],
        ),
    ]
}

/// The length of a way that really runs through its legs: each leg
/// starts where the last one ended.
fn assert_joined(way: &Way) {
    for w in way.legs.windows(2) {
        assert!(
            (w[0].to - w[1].from).length() < 1e-12,
            "a leg starts away from where the last one ended"
        );
    }
}

/// A way from one town to another through a third runs THROUGH the
/// third, on the two roads that meet there, and is as long as the two.
#[test]
fn a_way_between_two_towns_runs_through_the_town_between_them() {
    let roads = chain();
    let g = Graph::of(&roads, R);
    assert_eq!(
        g.size(),
        (5, 4),
        "three towns and two waypoints, four steps"
    );
    let way = g.way(at(0.0, 0.0), at(0.0, 4.0)).expect("A reaches C");
    assert_joined(&way);
    let roads_taken: Vec<usize> = way.legs.iter().map(|l| l.road).collect();
    assert_eq!(roads_taken, vec![0, 1]);
    let want = 4f64.to_radians() * R;
    assert!(
        (way.metres - want).abs() < 1.0,
        "{:.1} m against {want:.1}",
        way.metres
    );
    // And the way back is the same two roads the other way round.
    let back = g.way(at(0.0, 4.0), at(0.0, 0.0)).expect("C reaches A");
    let taken: Vec<usize> = back.legs.iter().map(|l| l.road).collect();
    assert_eq!(taken, vec![1, 0]);
}

/// A place snapped onto the network lands on the nearest step of the
/// nearest road, at the foot of the perpendicular, and says how far off
/// it stood.
#[test]
fn a_place_off_the_road_is_snapped_onto_the_nearest_step() {
    let g = Graph::of(&chain(), R);
    let spot = g.snap(at(0.1, 2.5)).expect("the network has steps");
    assert_eq!((spot.road, spot.seg), (1, 0));
    let off = 0.1f64.to_radians() * R;
    assert!(
        (spot.off - off).abs() < 5.0,
        "{:.1} m off against {off:.1}",
        spot.off
    );
    assert!(spot.at.y.abs() < 1e-9, "the foot is on the equator");
}

/// Along a shared TRUNK a way rides the road that OWNS it, which is the
/// one carrying the tarmac; past the fork it hands over to the road
/// that leaves.
#[test]
fn a_fork_rides_the_owners_trunk_and_hands_over_where_it_splits() {
    let g = Graph::of(&fork(), R);
    // Five towns and waypoints on the trunk are three nodes, not six.
    assert_eq!(g.size().0, 6, "the shared waypoints are one node each");
    // On the trunk alone: one leg, on road 0, whichever road the start
    // snapped to.
    let on = g.way(at(0.0, 0.5), at(0.0, 1.5)).expect("the trunk runs");
    assert_eq!(on.legs.len(), 1);
    assert_eq!(on.legs[0].road, 0, "the trunk is road 0's to carry");
    // From road 1's own branch back along the trunk to town 1: road 1
    // to the fork, then road 0 past it.
    let way = g.way(at(1.5, 2.0), at(0.0, 3.0)).expect("the fork joins");
    assert_joined(&way);
    let taken: Vec<usize> = way.legs.iter().map(|l| l.road).collect();
    assert_eq!(taken, vec![1, 0]);
    assert!(
        (way.legs[0].to - at(0.0, 2.0)).length() < 1e-12,
        "the hand over is at the fork"
    );
    let want = 2.5f64.to_radians() * R;
    assert!((way.metres - want).abs() < 50.0, "{:.1}", way.metres);
}

/// Two places on no common network have no way between them, which is
/// the answer between two continents and not a straight line.
#[test]
fn no_way_joins_two_roads_nothing_joins() {
    let roads = vec![
        road(0, 1, &[at(0.0, 0.0), at(0.0, 1.0)]),
        road(2, 3, &[at(10.0, 0.0), at(10.0, 1.0)]),
    ];
    let g = Graph::of(&roads, R);
    assert!(g.way(at(0.0, 0.5), at(10.0, 0.5)).is_none());
    assert!(g.way(at(0.0, 0.2), at(0.0, 0.8)).is_some());
}

/// Dijkstra with no heuristic at all, over the same graph and the same
/// ends: what A* must agree with to the millimetre.
fn brute(g: &Graph, from: Spot, to: Spot) -> Option<f64> {
    let (sa, sb) = g.ends(from)?;
    let (ga, gb) = g.ends(to)?;
    let mut dist = vec![f64::INFINITY; g.at.len()];
    dist[sa as usize] = arc(from.at, g.at[sa as usize]) * g.radius;
    dist[sb as usize] = dist[sb as usize].min(arc(from.at, g.at[sb as usize]) * g.radius);
    let mut done = vec![false; g.at.len()];
    loop {
        let next = (0..g.at.len())
            .filter(|i| !done[*i] && dist[*i].is_finite())
            .min_by(|a, b| dist[*a].total_cmp(&dist[*b]));
        let Some(n) = next else { break };
        done[n] = true;
        for &(m, d) in &g.out[n] {
            let nd = dist[n] + d;
            if nd < dist[m as usize] {
                dist[m as usize] = nd;
            }
        }
    }
    let via = |n: u32| dist[n as usize] + arc(g.at[n as usize], to.at) * g.radius;
    let mut best = via(ga).min(via(gb));
    if (sa.min(sb), sa.max(sb)) == (ga.min(gb), ga.max(gb)) {
        best = best.min(arc(from.at, to.at) * g.radius);
    }
    best.is_finite().then_some(best)
}

/// On a real network, routed over a real planet with a coast and
/// mountains on it, A* finds the SHORTEST way between every pair of
/// towns, the same one a search with no heuristic finds, and says there
/// is none exactly where there is none.
#[test]
fn a_star_finds_the_shortest_way_between_every_pair_of_towns() {
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
    let roads = super::super::connect(&planet, sea, &towns, 0.06);
    let g = Graph::of(&roads, planet.radius);
    let (mut pairs, mut joined, mut through) = (0, 0, 0);
    for a in &towns {
        for b in &towns {
            let (Some(sa), Some(sb)) = (g.snap(a.dir), g.snap(b.dir)) else {
                continue;
            };
            pairs += 1;
            let found = g.route(sa, sb);
            let truth = brute(&g, sa, sb);
            match (&found, truth) {
                (Some(way), Some(t)) => {
                    joined += 1;
                    assert!(
                        (way.metres - t).abs() < 1e-3,
                        "A* {:.3} m against {t:.3}",
                        way.metres
                    );
                    assert_joined(way);
                    through += usize::from(way.legs.len() > 1);
                }
                (None, None) => {}
                _ => panic!("A* and the search disagree about whether a way exists"),
            }
        }
    }
    println!(
        "{} towns, {} roads, {} nodes: {pairs} pairs, {joined} joined, {through} over more than one road",
        towns.len(),
        roads.len(),
        g.size().0
    );
    assert!(joined > towns.len(), "the network joins most of its towns");
    assert!(through > 0, "some way takes more than one road");
}

/// A road's refined line: each step of its waypoints cut into `n`
/// pieces, the way a road is drawn.
fn refined(line: &[DVec3], n: usize) -> Vec<DVec3> {
    let mut out = vec![line[0]];
    for w in line.windows(2) {
        for k in 1..=n {
            out.push(w[0].lerp(w[1], k as f64 / n as f64).normalize());
        }
    }
    out
}

/// A traced way runs the lines the roads are DRAWN on, from the place
/// itself to the place itself: on tarmac wherever both ends of a step
/// are, and a HOP where the highway stops short of the town between.
#[test]
fn a_trace_runs_the_drawn_lines_and_hops_the_town_between() {
    let roads = chain();
    let g = Graph::of(&roads, R);
    let lines: Vec<Vec<DVec3>> = roads
        .iter()
        .map(|r| refined(&r.line.iter().map(|p| p.0).collect::<Vec<_>>(), 20))
        .collect();
    // Tarmac everywhere but inside town B, which is the last three
    // points of road 0 and the first three of road 1.
    let open: Vec<Vec<bool>> = lines
        .iter()
        .enumerate()
        .map(|(r, l)| {
            (0..l.len())
                .map(|k| if r == 0 { k + 3 < l.len() } else { k >= 3 })
                .collect()
        })
        .collect();
    let (from, to) = (at(0.02, 0.3), at(-0.02, 3.7));
    let way = g.way(from, to).expect("the chain is joined");
    let trace = trace(
        &way,
        from,
        to,
        |r| Some((lines.get(r)?.as_slice(), open.get(r)?.as_slice())),
        R,
    );
    assert_eq!(trace.first().map(|p| p.0), Some(from));
    assert_eq!(trace.last().map(|p| p.0), Some(to));
    assert!(!trace[1].1, "the step onto the road is a hop");
    // Every step along the roads heads EAST, so nothing doubles back.
    // The hops on and off are left out: the nearest drawn point to a
    // place can stand a hair behind it.
    let lon = |d: DVec3| d.z.atan2(d.x);
    for w in trace[1..trace.len() - 1].windows(2) {
        assert!(lon(w[1].0) >= lon(w[0].0) - 1e-12, "the trace doubles back");
    }
    // The hops are the way on, the town and the way off: nothing else.
    let hops: Vec<usize> = (1..trace.len()).filter(|k| !trace[*k].1).collect();
    let paved = trace.len() - 1 - hops.len();
    println!(
        "{} points, {paved} steps on tarmac, hops at {hops:?}",
        trace.len()
    );
    assert_eq!(hops.first(), Some(&1), "the way on is a hop");
    assert_eq!(
        hops.last(),
        Some(&(trace.len() - 1)),
        "the way off is a hop"
    );
    let town = &hops[1..hops.len() - 1];
    assert!(
        town.windows(2).all(|w| w[1] == w[0] + 1),
        "the hops through B are one run: {town:?}"
    );
    for k in town {
        let off = arc(trace[*k].0, at(0.0, 2.0)) * R;
        assert!(off < 0.2f64.to_radians() * R, "a hop {off:.0} m from B");
    }
    // Through B, the town both roads end at.
    let near_b = trace
        .iter()
        .map(|p| arc(p.0, at(0.0, 2.0)) * R)
        .fold(f64::INFINITY, f64::min);
    assert!(near_b < 1.0, "the trace passes {near_b:.1} m from B");
    let long = length(&trace, R);
    assert!(
        (long - way.metres).abs() < 0.02 * way.metres,
        "{long:.0} m traced against {:.0} m routed",
        way.metres
    );
}

/// At a fork the line the trunk is drawn on and the line the branch
/// leaves on are ONE piece of tarmac, so a trace through a fork has no
/// hop in it at all.
#[test]
fn a_trace_through_a_fork_is_one_piece_of_tarmac() {
    let roads = fork();
    let g = Graph::of(&roads, R);
    let lines: Vec<Vec<DVec3>> = roads
        .iter()
        .map(|r| refined(&r.line.iter().map(|p| p.0).collect::<Vec<_>>(), 20))
        .collect();
    let open: Vec<Vec<bool>> = lines.iter().map(|l| vec![true; l.len()]).collect();
    let (from, to) = (at(1.5, 2.0), at(0.0, 2.8));
    let way = g.way(from, to).expect("the fork joins");
    let trace = trace(
        &way,
        from,
        to,
        |r| Some((lines.get(r)?.as_slice(), open.get(r)?.as_slice())),
        R,
    );
    let hops = (1..trace.len()).filter(|k| !trace[*k].1).count();
    assert_eq!(hops, 2, "only the way on and the way off are hops");
}
