//! Roads: which of a planet's towns are joined, and what line the joining
//! takes over the real ground.
//!
//! A road network is not a set of straight lines between city pairs. It is
//! what you get when everyone walks toward the nearest town and the tracks
//! wear in, so that is how it is built here: waypoints spread evenly over
//! the whole body, one MULTI SOURCE search outward from every town at
//! once, and a road wherever two towns' territories meet. That gives a
//! sparse, planar looking network for one search rather than a road per
//! pair, and it gets the two things a planetary network has to get right
//! for free: two towns on different continents are simply never joined,
//! because no chain of land waypoints runs between them, and a road round
//! a bay is shorter than a road across it without anything having to know
//! what a bay is.
//!
//! It is in the CORE because a road is world state: where a road runs
//! decides where a town's traffic goes and what the ground under it is
//! levelled to, and two clients that routed differently would disagree
//! about the world. `road::connect` is deterministic given a planet and
//! its towns, so the bake and the game get the same network.

use crate::field::Planet;
use crate::town::{surface_radius, Town};
use glam::DVec3;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// A road: the two towns it joins and the line it takes between them.
///
/// The line runs FROM the first town TO the second, a direction and a
/// level over the mean radius per point, with both towns' own centres as
/// its ends so a road always meets the street grid it serves.
#[derive(Clone, Debug, PartialEq)]
pub struct Road {
    pub from: usize,
    pub to: usize,
    pub line: Vec<(DVec3, f64)>,
}

impl Road {
    /// How long the road is over the ground, metres, measured along its
    /// own line rather than as the crow flies.
    pub fn length(&self, radius: f64) -> f64 {
        self.line
            .windows(2)
            .map(|w| arc(w[0].0, w[1].0) * radius)
            .sum()
    }
}

/// How far apart the waypoints a road is routed over stand, as a share of
/// the body's radius, and what the bake asks for.
///
/// A hundredth is ten kilometres on the harness planet, which is the
/// scale a highway is PLANNED at: it is the shape of the route that comes
/// off this grid and the metre by metre line that comes off the ground
/// under it later. Finer costs the bake one `surface_radius` a node and
/// buys a route that bends round features the levelling will flatten
/// anyway. It is an ARGUMENT rather than a constant read inside, so a
/// test can route a whole planet coarsely and the one implementation is
/// what it exercises.
pub const SPACING: f64 = 0.01;

/// How many waypoints a road may be routed over at most, whatever the
/// spacing asks for. A body's node count goes as the square of its radius
/// over the spacing, so the cap is what stops a big body from asking for
/// a search nobody will wait for; the bake says when it bites.
const MOST_NODES: usize = 200_000;

/// How much a slope costs a road, against the flat distance.
///
/// A road over ground climbing at its whole grade costs `GRADE` times
/// what the same distance costs on the level, so a route round a range is
/// taken whenever it is under that much longer. Eight is a road that will
/// go a long way round rather than over a mountain, which is what a road
/// does; at one it ignores the terrain and draws a great circle.
const GRADE: f64 = 8.0;

/// The steepest a road may climb, rise over run. Past it there is no edge
/// at all rather than an expensive one, so a route never takes a cliff
/// however far the way round is. A tenth is a one in ten hill, which is
/// about the steepest a road is built at.
const STEEPEST: f64 = 0.1;

/// How far above the sea a waypoint has to stand to carry a road, metres.
/// A road does not run along the tide line.
const DRY: f64 = 2.0;

/// A waypoint: a direction on the body and what the ground is at it.
struct Node {
    dir: DVec3,
    /// Metres over the sea, negative under it.
    over: f64,
}

/// The great circle angle between two directions, radians. Through the
/// half angle rather than an `acos` of a dot, because at the small angles
/// this asks about almost all the time, `acos` of a number near one loses
/// most of its digits and a chord does not.
fn arc(a: DVec3, b: DVec3) -> f64 {
    2.0 * ((a - b).length() * 0.5).clamp(0.0, 1.0).asin()
}

/// Route every road on a body: one network joining the towns that CAN be
/// joined over land.
///
/// The result is sorted, so a bake and a run of the game on the same
/// planet and the same towns produce the same network in the same order.
pub fn connect(planet: &Planet, sea: f64, towns: &[Town], spacing: f64) -> Vec<Road> {
    if towns.len() < 2 || spacing <= 0.0 || !spacing.is_finite() {
        return Vec::new();
    }
    let nodes = waypoints(planet, sea, spacing);
    let near = neighbours(&nodes, spacing);
    let seeds: Vec<usize> = towns.iter().map(|t| seed_node(&nodes, t.dir)).collect();
    let field = spread(&nodes, &near, &seeds, planet.radius);
    let pairs = borders(&nodes, &near, &field, planet.radius, towns.len());
    let mut roads: Vec<Road> = pairs
        .into_iter()
        .map(|(a, b, u, v)| Road {
            from: a,
            to: b,
            line: line_of(&nodes, &field, towns, sea, planet.radius, (a, b), (u, v)),
        })
        .filter(|r| r.line.len() > 1)
        .collect();
    roads.sort_by_key(|r| (r.from, r.to));
    roads
}

/// The waypoints a body is routed over: a golden angle spiral, which is
/// the same even spread `town::plan` picks its candidates off, with the
/// ground sampled once at each.
///
/// Once at each is the whole cost of the bake: an edge reads two heights
/// that are already in hand, so the search itself never touches the field.
///
/// Each march is against the planet's sites filtered to THAT direction
/// (`Planet::around`), which is the same rule a chunk is sampled under and
/// for the same reason. A march down through eight kilometres of band is
/// tens of samples and every one of them was walking all hundred and
/// sixty towns: the bake took 551 s, and 129 s of that was this loop
/// asking about towns nowhere near it.
fn waypoints(planet: &Planet, sea: f64, spacing: f64) -> Vec<Node> {
    // The unit sphere's area over the area one waypoint stands in, so the
    // spacing asked for is the spacing that comes out. Over FOUR rather
    // than over four pi, the spread was 1.77 times as wide as asked and
    // wider than the neighbour reach: not one waypoint on the body had a
    // neighbour, and the search came back with nought roads.
    let want = (4.0 * std::f64::consts::PI / (spacing * spacing)) as usize;
    let count = want.clamp(2, MOST_NODES);
    let golden = std::f64::consts::PI * (3.0 - 5f64.sqrt());
    (0..count)
        .map(|i| {
            let y = 1.0 - 2.0 * (i as f64 + 0.5) / count as f64;
            let s = (1.0 - y * y).max(0.0).sqrt();
            let a = golden * i as f64;
            let dir = DVec3::new(s * a.cos(), y, s * a.sin());
            // A march is along ONE direction, so the sites that can reach
            // any of its samples are the sites that reach that direction.
            let here = planet.around(dir, 0.0);
            Node {
                dir,
                over: surface_radius(&here, dir) - sea,
            }
        })
        .collect()
}

/// A cell of the grid the neighbour search buckets into, and the key it
/// is looked up by. A cell of the CUBE round the sphere rather than of
/// latitude and longitude: a longitude cell narrows to nothing at a pole
/// and a cube cell does not, so there is no pole to special case and no
/// meridian to wrap.
fn cell_of(dir: DVec3, cell: f64) -> [i32; 3] {
    [
        (dir.x / cell).floor() as i32,
        (dir.y / cell).floor() as i32,
        (dir.z / cell).floor() as i32,
    ]
}

fn key_of(c: [i32; 3]) -> i64 {
    // 21 bits an axis, which holds every cell of a grid finer than any
    // body asks for and keeps the key one integer to sort on.
    let p = |v: i32| (v as i64 + (1 << 20)) & ((1 << 21) - 1);
    (p(c[0]) << 42) | (p(c[1]) << 21) | p(c[2])
}

/// Every waypoint's near neighbours, as indices.
///
/// A road only ever steps to a waypoint about one spacing away, so the
/// search is over a grid of that size and its twenty seven surrounding
/// cells, which is every candidate and nothing else. Sorted keys and a
/// binary search rather than a hash map, because the core keeps none: a
/// map's iteration order is a thing two runs can disagree about, and a
/// network is world state.
fn neighbours(nodes: &[Node], spacing: f64) -> Vec<Vec<u32>> {
    let reach = spacing * 1.75;
    let cell = reach;
    let mut keyed: Vec<(i64, u32)> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (key_of(cell_of(n.dir, cell)), i as u32))
        .collect();
    keyed.sort_unstable();
    let mut out = vec![Vec::new(); nodes.len()];
    for (i, node) in nodes.iter().enumerate() {
        let home = cell_of(node.dir, cell);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let k = key_of([home[0] + dx, home[1] + dy, home[2] + dz]);
                    let mut at = keyed.partition_point(|(kk, _)| *kk < k);
                    while at < keyed.len() && keyed[at].0 == k {
                        let j = keyed[at].1 as usize;
                        at += 1;
                        if j == i {
                            continue;
                        }
                        if (node.dir - nodes[j].dir).length() <= reach {
                            out[i].push(j as u32);
                        }
                    }
                }
            }
        }
    }
    out
}

/// What an edge costs, or nothing where a road cannot go: into the sea,
/// or up a grade no road is built at.
fn cost(a: &Node, b: &Node, radius: f64) -> Option<f64> {
    if a.over < DRY || b.over < DRY {
        return None;
    }
    let run = arc(a.dir, b.dir) * radius;
    if run <= 0.0 {
        return None;
    }
    let rise = (b.over - a.over).abs();
    if rise / run > STEEPEST {
        return None;
    }
    Some(run * (1.0 + GRADE * rise / run))
}

/// One entry of the search's queue: how far, and where.
///
/// Ordered on `total_cmp` and then on the index, so the order is TOTAL
/// and two runs pop the same node at every tie. A network is world state,
/// and an order two clients could disagree about is a world they could
/// disagree about.
#[derive(PartialEq)]
struct Step {
    dist: f64,
    at: u32,
}

impl Eq for Step {}

impl Ord for Step {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.dist
            .total_cmp(&other.dist)
            .then_with(|| self.at.cmp(&other.at))
    }
}

impl PartialOrd for Step {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Which town owns a waypoint and how far it is from it.
#[derive(Clone, Copy)]
struct Reach {
    town: u32,
    dist: f64,
    from: u32,
}

/// The town nearest every waypoint, by ROAD rather than by line of sight,
/// and the step it was reached by.
///
/// One Dijkstra with every town pushed at the start, which labels the
/// whole body in a single search: a town's territory is the ground whose
/// cheapest road leads to it. Doing it a town at a time would be one
/// search per town for the same answer.
fn spread(nodes: &[Node], near: &[Vec<u32>], seeds: &[usize], radius: f64) -> Vec<Reach> {
    let mut reach = vec![
        Reach {
            town: u32::MAX,
            dist: f64::INFINITY,
            from: u32::MAX,
        };
        nodes.len()
    ];
    let mut queue: BinaryHeap<Reverse<Step>> = BinaryHeap::new();
    for (t, &n) in seeds.iter().enumerate() {
        if nodes[n].over < DRY {
            continue;
        }
        reach[n] = Reach {
            town: t as u32,
            dist: 0.0,
            from: n as u32,
        };
        queue.push(Reverse(Step {
            dist: 0.0,
            at: n as u32,
        }));
    }
    while let Some(Reverse(Step { dist: d, at })) = queue.pop() {
        let at = at as usize;
        if d > reach[at].dist {
            continue;
        }
        for &n in &near[at] {
            let n = n as usize;
            let Some(w) = cost(&nodes[at], &nodes[n], radius) else {
                continue;
            };
            let next = d + w;
            if next < reach[n].dist {
                reach[n] = Reach {
                    town: reach[at].town,
                    dist: next,
                    from: at as u32,
                };
                queue.push(Reverse(Step {
                    dist: next,
                    at: n as u32,
                }));
            }
        }
    }
    reach
}

/// The cheapest crossing between every pair of towns whose territories
/// touch, as the pair and the two waypoints either side of the border.
///
/// Two towns with no border between them get no road, which is what makes
/// the network sparse without a rule about how many roads a town may
/// have: a town ringed by others is joined to its ring and to nothing
/// beyond it.
fn borders(
    nodes: &[Node],
    near: &[Vec<u32>],
    field: &[Reach],
    radius: f64,
    towns: usize,
) -> Vec<(usize, usize, usize, usize)> {
    // The best crossing per ordered pair, in a dense triangle rather than
    // a map: a few hundred towns is a few tens of thousands of cells.
    let mut best = vec![(f64::INFINITY, 0usize, 0usize); towns * towns];
    for at in 0..nodes.len() {
        let here = field[at];
        if here.town == u32::MAX {
            continue;
        }
        for &n in &near[at] {
            let n = n as usize;
            let there = field[n];
            if there.town == u32::MAX || there.town == here.town {
                continue;
            }
            let Some(w) = cost(&nodes[at], &nodes[n], radius) else {
                continue;
            };
            let total = here.dist + w + there.dist;
            let (a, b) = (here.town as usize, there.town as usize);
            let (lo, hi, u, v) = if a < b { (a, b, at, n) } else { (b, a, n, at) };
            let slot = &mut best[lo * towns + hi];
            if total < slot.0 {
                *slot = (total, u, v);
            }
        }
    }
    let mut out = Vec::new();
    for a in 0..towns {
        for b in (a + 1)..towns {
            let (total, u, v) = best[a * towns + b];
            if total.is_finite() {
                out.push((a, b, u, v));
            }
        }
    }
    out
}

/// The line one road takes: back from the border to each town through the
/// steps the search itself took, with the towns' own centres as its ends.
///
/// The search wrote where every waypoint was reached FROM, so the route is
/// already in hand and nothing is searched twice.
fn line_of(
    nodes: &[Node],
    field: &[Reach],
    towns: &[Town],
    sea: f64,
    radius: f64,
    pair: (usize, usize),
    ends: (usize, usize),
) -> Vec<(DVec3, f64)> {
    let mut head = walk_back(field, ends.0);
    let tail = walk_back(field, ends.1);
    head.reverse();
    head.extend(tail);
    let mut line: Vec<(DVec3, f64)> = head
        .into_iter()
        .map(|i| (nodes[i].dir, nodes[i].over + sea - radius))
        .collect();
    // A road ends in the town it serves rather than at the waypoint
    // nearest it, or every road on the body stops a few kilometres short
    // of the streets it is there to join.
    let (a, b) = (&towns[pair.0], &towns[pair.1]);
    line.insert(0, (a.dir, a.h));
    line.push((b.dir, b.h));
    line.dedup_by(|x, y| (x.0 - y.0).length() < 1e-9);
    line
}

/// The chain of waypoints from one back to its own town.
fn walk_back(field: &[Reach], mut at: usize) -> Vec<usize> {
    let mut out = vec![at];
    // Bounded by the node count: a `from` chain cannot revisit a node,
    // since every step strictly lowers the distance it was reached at.
    for _ in 0..field.len() {
        let prev = field[at].from as usize;
        if prev == at || prev >= field.len() {
            break;
        }
        at = prev;
        out.push(at);
    }
    out
}

/// The waypoint a town's roads start at: the nearest one a road can
/// actually stand on, which is the nearest DRY one.
///
/// The plain nearest was the first cut and it stranded half the towns on
/// the test planet: waypoints are kilometres apart, a town stands on the
/// coast because that is where level land near the sea is, and the point
/// of the grid nearest it is as likely to be offshore as on. A town that
/// seeds a node in the sea seeds nothing, owns no ground, and never meets
/// a border, so it gets no road at all.
fn seed_node(nodes: &[Node], dir: DVec3) -> usize {
    let mut best = (f64::INFINITY, 0);
    for (i, n) in nodes.iter().enumerate() {
        if n.over < DRY {
            continue;
        }
        let d = (n.dir - dir).length();
        if d < best.0 {
            best = (d, i);
        }
    }
    best.1
}

#[cfg(test)]
mod tests;
