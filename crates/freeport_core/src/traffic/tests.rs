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
        sites: vec![],
    };
    crate::town::plan(&planet, 996.0, 60.0, 4, 7)
        .into_iter()
        .next()
        .expect("the test planet grew no town")
}

/// A hand laid grid, so the graph's own rules are held on a shape a
/// reader can picture rather than on whatever the planet grew.
fn grid(n: i32) -> Town {
    let mut pieces = Vec::new();
    let steps = (PITCH / crate::town::PIECE).ceil() as i64;
    for i in 0..=n {
        for j in 0..n {
            for k in 0..steps {
                let mid =
                    j as f64 * PITCH + (k as f64 + 0.5 - steps as f64 / 2.0) * crate::town::PIECE;
                // Running north, at the street line i.
                pieces.push(Piece {
                    x: line(i),
                    z: mid,
                    w: STREET,
                    d: crate::town::PIECE,
                });
                // Running east, at the street line i, over block row j.
                pieces.push(Piece {
                    x: mid,
                    z: line(i),
                    w: crate::town::PIECE,
                    d: STREET,
                });
            }
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
    println!(
        "{} pieces of street are {} edges between crossings",
        town.pieces.len(),
        streets.len()
    );
    // Four pieces to a block's frontage, so an edge is four pieces, and
    // nothing is counted twice.
    assert_eq!(streets.len(), town.pieces.len() / 4);
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

/// How far off the PAVING itself anybody gets, which is a stronger claim
/// than the corridor and the one with a real limit in it.
///
/// `town::streets_of` lays a street's pieces BETWEEN two crossings, so a
/// run stops at the crossing's own centre line and the square of tarmac
/// at a crossing is only covered from the sides a street reaches it on.
/// At a four way crossing that is all four quadrants and nothing can
/// leave the tarmac. At an L BEND, where a street arrives and another
/// leaves at a right angle, the far quadrant is bare, and the lane that
/// turns LEFT there crosses it, because keeping right round the outside
/// of a bend is what the far corner IS.
///
/// The ground a town stands on is levelled flat, so what that looks like
/// is somebody cutting the corner over a verge, and it is named here with
/// its number rather than hidden. Paving the crossing square outright is
/// the fix, and it is a change to the town's geometry that nobody has
/// asked for.
#[test]
fn the_only_ground_anybody_cuts_is_the_bare_corner_of_a_bend() {
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
    println!("the furthest anybody steps off the paving is {worst:.2} m ({which:?})");
    // A lane offset is the whole of it: nobody can be further off the
    // tarmac than the far corner of a bend stands from it.
    assert!(
        worst <= FOOT_LANE + 1e-6,
        "somebody is {worst:.2} m off the paving, past the {FOOT_LANE:.2} m a lane can be"
    );
}
