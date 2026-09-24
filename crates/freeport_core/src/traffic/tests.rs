use super::*;
use crate::field::Planet;

/// A town with a real grid on it: the port of a small planet, which is
/// what every number below is measured on.
fn town() -> Town {
    let planet = Planet {
        radius: 1000.0,
        relief: 40.0,
        lumps: 6.0,
        octaves: 6,
        overhang: 1.0,
        ledge: 8.0,
        seed: 7,
        sites: vec![].into(),
    };
    // Four hundred metres for the biggest, because the fixture ball's
    // level ground stands three times the size law's reach from its own
    // sea and the towns it grows are all at the law's floor, and the
    // FRONT then eats their fringe: at sixty the port came out as one
    // block and turned nobody out, and at two hundred and fifty, once
    // the front came in, it was one block again.
    crate::town::plan(&planet, 996.0, 400.0, 4, 7)
        .into_iter()
        .next()
        .expect("the test planet grew no town")
}

/// A hand laid grid, so the graph's own rules are held on a shape a
/// reader can picture rather than on whatever the planet grew.
fn grid(n: i32) -> Town {
    let mut pieces = Vec::new();
    let steps = (crate::town::BLOCK / crate::town::PIECE).ceil();
    let cut = crate::town::BLOCK / steps;
    let steps = steps as i64;
    for i in 0..=n {
        for j in 0..n {
            for k in 0..steps {
                let mid = j as f64 * PITCH + (k as f64 + 0.5 - steps as f64 / 2.0) * cut;
                // Running north, at the street line i.
                pieces.push(Piece {
                    x: line(i),
                    z: mid,
                    w: STREET,
                    d: cut,
                    arms: 0,
                });
                // Running east, at the street line i, over block row j.
                pieces.push(Piece {
                    x: mid,
                    z: line(i),
                    w: cut,
                    d: STREET,
                    arms: 0,
                });
            }
        }
    }
    // And the crossing at every corner of it, with whichever of its four
    // arms the grid actually reaches it on.
    for i in 0..=n {
        for j in 0..=n {
            let mut arms = 0u8;
            if j < n {
                arms |= arm::NORTH;
            }
            if j > 0 {
                arms |= arm::SOUTH;
            }
            if i < n {
                arms |= arm::EAST;
            }
            if i > 0 {
                arms |= arm::WEST;
            }
            pieces.push(Piece {
                x: line(i),
                z: line(j),
                w: STREET,
                d: STREET,
                arms,
            });
        }
    }
    Town {
        dir: glam::DVec3::Y,
        east: glam::DVec3::X,
        north: glam::DVec3::Z,
        h: 0.0,
        radius: 60.0,
        along: DVec2::ZERO,
        lots: Vec::new(),
        pieces,
        index: 0,
        seed: 0,
        grade: None,
    }
}

/// How far `p` is from the segment `a` to `b`, extended by `HALF_STREET`
/// at each end: a street's own CORRIDOR, which is the paving plus the
/// crossing square at each of its ends.
fn off_street(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let d = b - a;
    let n = d.length();
    if n < 1e-9 {
        return (p - a).length();
    }
    let t = ((p - a).dot(d) / (n * n)).clamp(-HALF_STREET / n, 1.0 + HALF_STREET / n);
    (p - (a + d * t)).length()
}

/// Every centreline of a town's streets, as the pair of points it runs
/// between in the town's own metres.
fn centrelines(streets: &Streets) -> Vec<(DVec2, DVec2)> {
    streets
        .edges
        .iter()
        .map(|&(n, w)| (place(n), place(step(n, w))))
        .collect()
}

/// The paving a town lays IS the graph traffic rides, and a piece that
/// was never laid is a street nobody can be on.
#[test]
fn the_streets_are_the_graph_of_the_paving() {
    let town = town();
    let streets = Streets::of(&town);
    assert!(!streets.is_empty(), "the port paved nothing");
    let runs = town.pieces.iter().filter(|p| p.run()).count();
    let crossings = town.pieces.len() - runs;
    println!(
        "{} pieces of street are {runs} of run and {crossings} crossings, \
         over {} edges",
        town.pieces.len(),
        streets.len()
    );
    // A run is the frontage of one block cut into whole pieces, so an
    // edge is exactly that many of them and nothing is counted twice.
    let steps = (crate::town::BLOCK / crate::town::PIECE).ceil() as usize;
    assert_eq!(runs, streets.len() * steps);
    // And a crossing wherever an edge ends, once each.
    let mut nodes: Vec<Node> = streets
        .edges
        .iter()
        .flat_map(|&(n, w)| [n, step(n, w)])
        .collect();
    nodes.sort_unstable();
    nodes.dedup();
    assert_eq!(crossings, nodes.len());
    // Every edge's two ends are a block's pitch apart and nowhere else.
    for &(n, w) in &streets.edges {
        let d = place(step(n, w)) - place(n);
        assert!(
            (d.length() - PITCH).abs() < 1e-9,
            "an edge is {} m",
            d.length()
        );
        assert!(w < 2, "an edge is held east or north and this one is {w}");
    }
    // And a town with no paving turns nobody out at all.
    let mut bare = town.clone();
    bare.pieces.clear();
    assert!(Streets::of(&bare).is_empty());
    assert!(Traffic::of(&bare, 7).agents.is_empty());
}

/// The face walk's successor is a PERMUTATION of the directed edges, so
/// every orbit closes and the orbits partition them. That is the whole
/// reason a circuit needs no search: it is not found, it is followed.
#[test]
fn every_street_is_on_exactly_one_circuit() {
    for streets in [Streets::of(&town()), Streets::of(&grid(3))] {
        let directed = streets.directed();
        // A permutation: every directed edge is the successor of exactly
        // one directed edge.
        let mut hit: Vec<(Node, usize)> =
            directed.iter().map(|&(n, w)| streets.next(n, w)).collect();
        hit.sort_unstable();
        assert_eq!(hit, directed, "the successor is not a permutation");
        // And every orbit closes inside the edges there are.
        let mut walked = 0;
        let mut seen: Vec<(Node, usize)> = Vec::new();
        for start in &directed {
            if seen.binary_search(start).is_ok() {
                continue;
            }
            let mut at = *start;
            for _ in 0..=directed.len() {
                if let Err(k) = seen.binary_search(&at) {
                    seen.insert(k, at);
                }
                at = streets.next(at.0, at.1);
                walked += 1;
                if at == *start {
                    break;
                }
            }
            assert_eq!(at, *start, "a face walk did not come back");
        }
        assert_eq!(walked, directed.len(), "the faces do not cover the streets");
    }
}

/// **Nobody ever leaves the street.** The claim a picture cannot make and
/// a test can: every person and every car, sampled all the way round its
/// own loop, stands inside the corridor of some street the town actually
/// paved.
///
/// It is the walker's own rule arriving at the traffic ("a walk is what
/// proves a town, and a picture cannot"), and it is what catches the two
/// things that go wrong here: a lane offset wider than the street, and a
/// corner fillet that swings the path out of it.
#[test]
fn nobody_ever_leaves_the_street() {
    let town = town();
    let streets = Streets::of(&town);
    let lines = centrelines(&streets);
    let traffic = Traffic::of(&town, 7);
    assert!(!traffic.agents.is_empty(), "the port turned nobody out");
    let mut worst: f64 = 0.0;
    let mut worst_kind = Kind::Foot;
    for agent in &traffic.agents {
        let loop_len = traffic.faces[agent.face].of(agent.kind).length();
        // Every tenth of a metre round the whole loop, which is finer
        // than any fillet in it.
        let steps = (loop_len / 0.1).ceil() as usize;
        for k in 0..steps {
            let spot = traffic.faces[agent.face]
                .of(agent.kind)
                .at(k as f64 * loop_len / steps as f64);
            let near = lines
                .iter()
                .map(|&(a, b)| off_street(spot.at, a, b))
                .fold(f64::MAX, f64::min);
            if near > worst {
                worst = near;
                worst_kind = agent.kind;
            }
        }
    }
    println!(
        "the furthest anybody stands from a street's middle is {worst:.3} m ({worst_kind:?}), \
         against a half street of {HALF_STREET:.2} m"
    );
    assert!(
        worst <= HALF_STREET,
        "somebody stands {worst:.3} m off the nearest street"
    );
}

/// A circuit CLOSES and its heading does not jump. An agent that teleported
/// round a corner would pass every other test here: what says it does not
/// is that the step between two samples is the arc length between them and
/// the heading turns by a little at a time.
#[test]
fn a_circuit_closes_and_neither_place_nor_heading_jumps() {
    let traffic = Traffic::of(&town(), 7);
    let mut worst_turn: f64 = 0.0;
    let mut worst_step: f64 = 0.0;
    for lanes in &traffic.faces {
        for kind in [Kind::Foot, Kind::Car] {
            let c = lanes.of(kind);
            let len = c.length();
            assert!(len > 1.0);
            // The loop closes: one whole way round is where it started.
            let a = c.at(0.0);
            let b = c.at(len);
            assert!((a.at - b.at).length() < 1e-9, "the loop does not close");
            let step = 0.02;
            let n = (len / step) as usize;
            let mut last = c.at(0.0);
            for k in 1..=n {
                let now = c.at(k as f64 * step);
                // The step on the ground is the step along the loop: a
                // path that cut a corner would come up short here.
                let moved = (now.at - last.at).length();
                worst_step = worst_step.max((moved - step).abs());
                // The heading turns by a little, never by a lot. The
                // tightest turn is the car's, and at 0.02 m of step that
                // is 0.02 / CAR_TURN of a radian.
                let mut turn = (now.yaw - last.yaw).rem_euclid(std::f64::consts::TAU);
                if turn > std::f64::consts::PI {
                    turn -= std::f64::consts::TAU;
                }
                worst_turn = worst_turn.max(turn.abs());
                last = now;
            }
        }
    }
    println!(
        "over 2 cm of travel the place is out by at most {worst_step:.2e} m \
         and the heading turns by at most {:.1} degrees",
        worst_turn.to_degrees()
    );
    assert!(
        worst_step < 1e-4,
        "the path cuts a corner by {worst_step:.2e} m"
    );
    // 2 cm round the tightest fillet there is, with a little slack for
    // the sample that straddles the join between two runs.
    assert!(
        worst_turn < 0.05,
        "the heading jumps by {:.1} degrees",
        worst_turn.to_degrees()
    );
}

/// A car keeps RIGHT, which is what makes two cars pass each other rather
/// than through each other, and a person walks outside a car.
#[test]
fn a_car_keeps_right_and_a_person_walks_outside_it() {
    assert!(CAR_LANE > 0.0 && FOOT_LANE > CAR_LANE);
    // Both stay inside the paving with the width of the thing itself.
    assert!(CAR_LANE + 0.8 <= HALF_STREET, "a car hangs off the road");
    assert!(
        FOOT_LANE + 0.23 <= HALF_STREET,
        "a person hangs off the kerb"
    );
    // And each rides the MIDDLE of what it is on: a car its own lane, a
    // person the pavement. That is what ties a lane to the paving it is
    // drawn over rather than to a number somebody picked.
    assert!((CAR_LANE - crate::town::LANE * 0.5).abs() < 1e-12);
    assert!((FOOT_LANE - crate::town::LANE - crate::town::WALK * 0.5).abs() < 1e-12);
    // And the two ways round one street are a whole car apart.
    let town = grid(2);
    let traffic = Traffic::of(&town, 7);
    let c = traffic.faces[0].of(Kind::Car);
    let there = c.at(0.0);
    // The nearest point of the loop going the other way about this same
    // street is the far lane, and it stands two offsets off.
    let mut apart = f64::MAX;
    for face in &traffic.faces {
        let other = face.of(Kind::Car);
        let n = (other.length() / 0.05) as usize;
        for k in 0..n {
            let q = other.at(k as f64 * 0.05);
            // Only the ones going the other way: a lane beside itself is
            // nought apart and says nothing.
            if (q.yaw - there.yaw).cos() > -0.9 {
                continue;
            }
            apart = apart.min((q.at - there.at).length());
        }
    }
    println!("the two ways down one street are {apart:.2} m apart");
    assert!(
        (apart - 2.0 * CAR_LANE).abs() < 0.2,
        "oncoming traffic passes {apart:.2} m away, not {:.2}",
        2.0 * CAR_LANE
    );
}

/// A town turns out people and cars off its own buildings, and the same
/// town twice is the same town: this is replayed state, so an agent that
/// differed between two runs would be a city that differed between two
/// clients.
#[test]
fn a_town_turns_out_the_same_people_twice() {
    let town = town();
    let a = Traffic::of(&town, 7);
    let b = Traffic::of(&town, 7);
    println!(
        "{} lots turn out {} on foot and {} driving, over {} circuits",
        town.lots.len(),
        a.agents.iter().filter(|g| g.kind == Kind::Foot).count(),
        a.agents.iter().filter(|g| g.kind == Kind::Car).count(),
        a.faces.len()
    );
    assert_eq!(a.agents.len(), b.agents.len());
    for (x, y) in a.agents.iter().zip(&b.agents) {
        assert_eq!(x.kind, y.kind);
        assert_eq!(x.face, y.face);
        assert_eq!(x.id, y.id);
        assert_eq!(x.speed.to_bits(), y.speed.to_bits());
        assert_eq!(x.phase.to_bits(), y.phase.to_bits());
        // And at any moment, in the same place to the bit.
        let (p, q) = (a.at(x, 41.5), b.at(y, 41.5));
        assert_eq!(p.at.x.to_bits(), q.at.x.to_bits());
        assert_eq!(p.at.y.to_bits(), q.at.y.to_bits());
    }
    // The counts are off the LOTS, and nobody is turned out on none.
    assert_eq!(
        a.agents.iter().filter(|g| g.kind == Kind::Foot).count(),
        count(town.lots.len(), FOLK_PER_LOT)
    );
    assert!(
        a.agents.iter().any(|g| g.kind == Kind::Car),
        "nobody drives"
    );
    let mut empty = town.clone();
    empty.lots.clear();
    assert!(
        Traffic::of(&empty, 7).agents.is_empty(),
        "a town with no buildings has people in it"
    );
}

/// Everybody MOVES, at their own pace, and comes back round: the rails
/// are a loop and the clock only ever grows, so a run of any length has
/// to stay on it.
#[test]
fn everybody_moves_at_their_own_pace_and_comes_round_again() {
    let town = town();
    let traffic = Traffic::of(&town, 7);
    let mut fastest: f64 = 0.0;
    let mut slowest = f64::MAX;
    for agent in &traffic.agents {
        let a = traffic.at(agent, 0.0);
        let b = traffic.at(agent, 1.0);
        let moved = (b.at - a.at).length();
        // Over a second, as far as its own speed carries it, less
        // whatever a corner took out of the straight line.
        assert!(moved > agent.speed * 0.5, "somebody is standing still");
        assert!(
            moved <= agent.speed + 1e-9,
            "somebody outran their own speed"
        );
        fastest = fastest.max(agent.speed);
        slowest = slowest.min(agent.speed);
        // A whole loop later, back where it started.
        let len = traffic.faces[agent.face].of(agent.kind).length();
        let round = traffic.at(agent, len / agent.speed);
        assert!(
            (round.at - a.at).length() < 1e-6,
            "a loop did not come round"
        );
        // And a clock hours long is still on the rails.
        let far = traffic.at(agent, 36_000.0);
        assert!(
            far.at.is_finite(),
            "an agent left the world after ten hours"
        );
    }
    println!("the town walks and drives at {slowest:.2} to {fastest:.2} m/s");
    assert!(fastest > slowest, "everybody goes at exactly one speed");
}

/// NOBODY steps off the paving at all, which is a stronger claim than
/// the corridor and is the one the crossing square bought.
///
/// A run used to reach only as far as a crossing's own centre line, so
/// the square where two streets met was covered from the sides a street
/// arrived on and no further: at an L BEND the far quadrant was BARE,
/// and the lane turning left there crossed it, because keeping right
/// round the outside of a bend is what the far corner IS. Measured on
/// this town it was 1.62 m of levelled verge, which reads as a person
/// walking a corner over the grass.
///
/// A crossing is a PIECE of its own now (mining-mike's junction that
/// owns its whole cell), so the square is paved whole whatever arms
/// reach it, and this measures nought.
#[test]
fn nobody_steps_off_the_paving_at_all() {
    let town = town();
    let traffic = Traffic::of(&town, 7);
    let mut worst: f64 = 0.0;
    let mut which = Kind::Foot;
    for kind in [Kind::Foot, Kind::Car] {
        for lanes in &traffic.faces {
            let c = lanes.of(kind);
            let n = (c.length() / 0.05).ceil() as usize;
            for k in 0..n {
                let at = c.at(k as f64 * 0.05).at;
                let off = town
                    .pieces
                    .iter()
                    .map(|p| {
                        // Outside an axis aligned rectangle, by how far.
                        let dx = (at.x - p.x).abs() - p.w / 2.0;
                        let dz = (at.y - p.z).abs() - p.d / 2.0;
                        dx.max(0.0).hypot(dz.max(0.0))
                    })
                    .fold(f64::MAX, f64::min);
                if off > worst {
                    worst = off;
                    which = kind;
                }
            }
        }
    }
    println!("the furthest anybody steps off the paving is {worst:.3} m ({which:?})");
    assert!(
        worst <= 1e-6,
        "somebody is {worst:.3} m off the paving, which the crossings are there to stop"
    );
}

/// A person keeps to the PAVEMENT, a kerb over the road, and steps down
/// off it exactly where he crosses a street.
///
/// That is one answer rather than two: the nine cells of a crossing the
/// mesh is paved from are the nine cells `Traffic::lift` reads, so a
/// foot and the concrete under it cannot disagree. Measured on a plain
/// grid, where every crossing has all four arms and the walk between
/// two of them is entirely on the kerb.
#[test]
fn a_person_walks_the_kerb_and_steps_down_to_cross() {
    let town = grid(2);
    let traffic = Traffic::of(&town, 7);
    let lanes = traffic
        .faces
        .iter()
        .max_by(|a, b| a.foot.length().total_cmp(&b.foot.length()))
        .expect("the grid laid no circuit");
    let c = &lanes.foot;
    let len = c.length();
    let n = (len / 0.05).ceil() as usize;
    let (mut up, mut down) = (0.0f64, 0.0f64);
    for k in 0..n {
        let at = c.at(k as f64 * len / n as f64).at;
        let lift = traffic.lift(at);
        assert!(
            lift == 0.0 || lift == crate::town::KERB,
            "a foot stands {lift} m up, which is neither the road nor the kerb"
        );
        if lift > 0.0 {
            up += len / n as f64;
        } else {
            down += len / n as f64;
        }
    }
    println!("a walk of {len:.1} m is {up:.1} m on the kerb and {down:.1} m crossing a street");
    // Most of a walk is on the pavement, and some of it is not: a
    // pedestrian who never left the kerb would be one who never crossed
    // a road, which on a grid of crossroads is impossible.
    assert!(down > 1.0, "nobody ever steps off the kerb");
    assert!(up > down * 2.0, "a pedestrian spends his walk in the road");
    // On a RUN he is always up: the pavement is the only ground there.
    let node = place((0, 0));
    let mid = DVec2::new(node.x + FOOT_LANE, node.y + PITCH * 0.5);
    assert_eq!(traffic.lift(mid), crate::town::KERB);
}

/// A ROUTE OUT OF A TOWN runs on paving the town actually laid, and it
/// gets from the middle to the edge.
///
/// A car stolen in the middle of the port is 700 m from the nearest
/// country tarmac, which is further than the road follower's own reach,
/// so the scripted drive aimed at a settlement nine kilometres off and
/// spent the run oscillating against the building in front of it. The
/// streets are the way out; what this holds is that the chain is
/// continuous, that every step of it is an edge of the graph, and that
/// it reaches the far side.
#[test]
fn a_route_out_of_a_town_runs_on_the_towns_own_paving() {
    let town = town();
    let streets = Streets::of(&town);
    let far = DVec2::new(town.radius * crate::town::OUTLINE, 0.0);
    let route = streets.route(DVec2::ZERO, far);
    // Three crossings and not more: a block is four lots a side now, so
    // the fixture's own port is about four blocks across.
    assert!(
        route.len() >= 3,
        "a route across a town is {} crossings",
        route.len()
    );
    for pair in route.windows(2) {
        let step = pair[1] - pair[0];
        assert!(
            (step.length() - PITCH).abs() < 1e-9,
            "a route steps {:.2} m and a block is {PITCH:.2}",
            step.length()
        );
        // And the step is ON the graph, which is what says the car is
        // being sent down a street somebody laid.
        let n = (line_at(pair[0].x), line_at(pair[0].y));
        let way = if step.x > 0.0 {
            0
        } else if step.y > 0.0 {
            1
        } else if step.x < 0.0 {
            2
        } else {
            3
        };
        assert!(
            streets.has(n, way),
            "a route leaves {n:?} where nothing is paved"
        );
    }
    // It ACTUALLY gets out: the last crossing is further from the middle
    // than the first, and no crossing the town paved stands further the
    // way it was sent than a block past where it ended. How far that is
    // is the town's own FRONT and not its outline, because the country
    // eats a town's fringe long before the outline arrives.
    let (from, to) = (route[0].length(), route[route.len() - 1].length());
    let reached = route[route.len() - 1].x;
    let furthest = streets
        .nodes()
        .iter()
        .map(|n| place(*n).x)
        .fold(f64::MIN, f64::max);
    assert!(
        to > from && reached >= furthest - PITCH,
        "a route out of a town ends {to:.1} m from its middle having started {from:.1}, \
         and the town's paving runs {furthest:.1} m east"
    );
    println!(
        "a route out of this town is {} crossings, {from:.1} m to {to:.1} m from the middle",
        route.len()
    );
}
