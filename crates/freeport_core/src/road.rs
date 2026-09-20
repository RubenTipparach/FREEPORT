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
pub const DRY: f64 = 2.0;

/// A waypoint: a direction on the body and what the ground is at it.
struct Node {
    dir: DVec3,
    /// Metres over the sea, negative under it.
    over: f64,
    /// Whether the ground here is under the ice line. A road does not
    /// cross a glacier, and without this a route between two temperate
    /// towns takes the short way OVER the cap, which is what put a
    /// network across both poles of the first baked planet.
    frozen: bool,
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
    let shape = planet.shape();
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
            let over = surface_radius(&here, dir) - sea;
            Node {
                dir,
                over,
                frozen: shape.climate(dir, over).frozen(),
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
    if a.over < DRY || b.over < DRY || a.frozen || b.frozen {
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
        if nodes[n].over < DRY || nodes[n].frozen {
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
        if n.over < DRY || n.frozen {
            continue;
        }
        let d = (n.dir - dir).length();
        if d < best.0 {
            best = (d, i);
        }
    }
    best.1
}

/// How far apart settlements on a road stand, metres of road between
/// them. Twenty five kilometres is about a day's walk with a cart, which
/// is the distance that puts an inn and then a village on a road, and on
/// this body it leaves a long road with a handful of them rather than a
/// ribbon of houses.
const EVERY: f64 = 25_000.0;

/// Villages ALONG the roads: a settlement wherever a road has run far
/// enough since the last one and the ground there will take a town.
///
/// A road exists because two cities wanted to trade, and what grows on it
/// afterwards is everybody who wanted to be on the way: that is the
/// owner's ask and it is also the honest order, because the road has to
/// be routed before anything can stand beside it. They come AFTER the
/// cities in the list, so every road's own `from` and `to` still name the
/// towns they named.
///
/// Their size is the same coastal law every settlement uses, cut by
/// `town::WAYSIDE`: a place that grew because the road goes past it is a
/// village whatever its shore, and one that could rival the city at
/// either end would be a city nobody routed a road to.
pub fn waysides(
    planet: &Planet,
    sea: f64,
    roads: &[Road],
    towns: &[Town],
    biggest: f64,
    seed: u32,
) -> Vec<Town> {
    let big_r = planet.radius;
    let shape = planet.shape();
    let (low, high) = crate::town::window(planet);
    let mut taken: Vec<(DVec3, f64)> = towns.iter().map(|t| (t.dir, t.radius)).collect();
    let mut placed = Vec::new();
    for road in roads {
        let mut since = EVERY * 0.5;
        for pair in road.line.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            since += arc(a.0, b.0) * big_r;
            if since < EVERY {
                continue;
            }
            since = 0.0;
            let dir = b.0;
            let over = big_r + b.1 - sea;
            if !(low..=high).contains(&over) || shape.climate(dir, over).frozen() {
                continue;
            }
            let index = towns.len() + placed.len();
            let radius =
                crate::town::size_of(biggest, over, planet, index, seed) * crate::town::WAYSIDE;
            if taken
                .iter()
                .any(|(d, r)| d.dot(dir) > ((r + radius + crate::town::BETWEEN) / big_r).cos())
            {
                continue;
            }
            // Filtered to the sites this ONE direction can reach, which
            // is the rule this file already keeps for its own march: a
            // survey is forty nine of them and each walked all hundred
            // and sixty town sites, which took the bake from nineteen
            // seconds to three hundred and seventeen.
            let near = planet.around(dir, radius * 4.0 / big_r);
            let Some(p) = crate::town::settle(&near, sea, dir, over, radius) else {
                continue;
            };
            taken.push((dir, radius));
            placed.push(p);
        }
    }
    crate::town::lay_all_from(&placed, big_r, sea, seed, towns.len())
}

pub mod ribbon;

#[cfg(test)]
mod tests;

/// How long one piece of a road's levelled CORRIDOR is, metres.
///
/// The atlas plans a road over waypoints ten kilometres apart, and a
/// corridor cut as a straight ramp between two of those is a canyon:
/// measured on this body's own roads (`examples/road_ground.rs`), the
/// ground strays from that ramp by a median of 45 m and up to 830. The
/// same measurement swept the spacing, and the deviation halves as the
/// spacing does: 2.7 km is a median of 10.3 m, 1.4 km is 4.8, 683 m is
/// 2.3, 341 m is 1.20 with a 99th of 6.2, and 171 m is 0.72 with a 99th
/// of 3.1.
///
/// It was 341 m, on the reading that a metre of cut is a verge and six
/// is a cutting and both are things a road HAS. That reading looked at
/// the median and the 99th and not at the WORST, and the worst at 341 m
/// is a **36 m canyon**: the owner read it off a picture as a highway
/// sinking into the ground, and a road that disappears into the country
/// once on a body is a road that disappears. The same sweep says 170 m
/// is 0.72 median and 15.9 worst, and 85 m is 0.54 median, a 99th of
/// 2.10 and a worst of 4.56, which is a CUTTING everywhere on the body
/// and a canyon nowhere.
///
/// What it costs is the count, four times over: 187,000 arcs become
/// 747,000 and the atlas's own heights go with them. What makes that
/// affordable is `field::Sites`, the latitude index, which is why a
/// chunk pays for the sites in its own band and not for the body's, and
/// what makes it necessary is that a corridor has to exist planet wide
/// from the first frame for the same reason a town's site does: a chunk
/// is meshed once.
pub const PIECE: f64 = 85.0;

/// How far either side of its centreline a road's ground is levelled,
/// metres. The carriageway is `town::LANE` each way and the rest is the
/// verge a road needs to sit in its own cutting rather than on a ledge.
pub const CORRIDOR: f64 = 7.0;

/// How many pieces a waypoint span is cut into.
///
/// It is a FUNCTION and not a stored count because both the bake and the
/// game have to agree about it exactly: the atlas keeps the corridor's
/// HEIGHTS and derives its directions, which is 1.5 MB of file rather
/// than 17, and a bake and a game that disagreed by one piece would be a
/// road whose levelling and whose tarmac are in different places.
pub fn pieces(a: DVec3, b: DVec3, radius: f64) -> usize {
    let run = arc(a, b) * radius;
    if !run.is_finite() || run <= 0.0 {
        return 1;
    }
    ((run / PIECE).ceil() as usize).max(1)
}

/// The k-th of `n` points along the great circle from `a` to `b`, a
/// SLERP so the points are evenly spaced over the ground rather than
/// bunched at the ends the way a chord's own division is.
pub fn step(a: DVec3, b: DVec3, k: usize, n: usize) -> DVec3 {
    let sweep = arc(a, b);
    if n == 0 || sweep < 1e-12 {
        return a;
    }
    let t = (k as f64 / n as f64).clamp(0.0, 1.0);
    let (s, c) = (sweep * t).sin_cos();
    let ahead = (b - a * a.dot(b)).normalize_or_zero();
    (a * c + ahead * s).normalize_or(a)
}

/// Every point of a road's refined centreline, ends included: the
/// waypoints with `pieces` divisions between each neighbouring pair.
pub fn centreline(road: &Road, radius: f64) -> Vec<DVec3> {
    let mut out = Vec::new();
    for pair in road.line.windows(2) {
        let (a, b) = (pair[0].0, pair[1].0);
        let n = pieces(a, b, radius);
        for k in 0..n {
            out.push(step(a, b, k, n));
        }
    }
    if let Some(last) = road.line.last() {
        out.push(last.0);
    }
    out
}

/// The GROUND under a road's refined centreline, metres over the mean
/// radius: what the bake writes into the atlas.
///
/// The heights are taken off the planet the roads were ROUTED over,
/// which is the one carrying the towns' own sites, so a road arriving at
/// a town meets the level that town cut rather than the hill that was
/// there before it. They are then held to `STEEPEST` between neighbours
/// and to `dry` over the sea: the router already refused a wet or a
/// steep edge between waypoints, and this is the same promise kept
/// between the pieces it did not look at.
pub fn survey(planet: &Planet, road: &Road, dry: f64) -> Vec<f64> {
    let line = centreline(road, planet.radius);
    let run: Vec<f64> = line
        .iter()
        .map(|dir| {
            let local = planet.around(*dir, 1e-9);
            crate::town::surface_radius(&local, *dir) - planet.radius
        })
        .collect();
    let gap: Vec<f64> = line
        .windows(2)
        .map(|w| arc(w[0], w[1]) * planet.radius)
        .collect();
    smooth(run, &gap, dry)
}

/// A road SMOOTHS what it crosses: every step held to the grade the
/// route was allowed, and nothing under `dry`.
///
/// Each step is clamped against its OWN piece's length and not against
/// `PIECE`, because `pieces` rounds a span UP and its pieces are
/// therefore shorter: clamped against the nominal length the last piece
/// of a span came out at one in 9.4 against a limit of one in ten, which
/// `a_corridor_is_never_steeper_than_a_road_is_built` caught.
///
/// Forward, then backward, then the floor, three rounds, because the
/// three pull against one another and the fixed point is the envelope
/// that satisfies all of them. It almost never binds at this spacing:
/// the ground moves a median of 1.2 m over a piece against the 34 m the
/// grade allows, and what it is there for is the one sampled crag that
/// would otherwise put a wall across the corridor.
fn smooth(mut run: Vec<f64>, gap: &[f64], dry: f64) -> Vec<f64> {
    if run.len() < 2 {
        return run.iter().map(|h| h.max(dry)).collect();
    }
    for _ in 0..3 {
        for k in 1..run.len() {
            let most = STEEPEST * gap[k - 1];
            run[k] = run[k].clamp(run[k - 1] - most, run[k - 1] + most);
        }
        for k in (0..run.len() - 1).rev() {
            let most = STEEPEST * gap[k];
            run[k] = run[k].clamp(run[k + 1] - most, run[k + 1] + most);
        }
        for h in &mut run {
            *h = h.max(dry);
        }
    }
    run
}

/// The SITES a road's corridor levels: one arc a piece, each cut to the
/// ground its own two ends stand on.
///
/// `skip` is how near a town's own centre the corridor stops, and it is
/// the town's own levelling AT THAT BEARING plus its skirt
/// (`Site::level_r` and `field::site_skirt`): inside that the town's
/// site wins at full weight and answers the town's level, so a corridor
/// that reached further would lay its tarmac at the road's level over
/// ground held at the town's, and one that stopped short of it would
/// end in a field. Measured, tarmac stood 0.10 m off its own lift there;
/// past the band it is 0.003. Consecutive arcs share an end and its level by
/// construction, which is what lets the field's slope bound assume TWO
/// overlapping skirts rather than however many roads meet at a hub.
pub fn corridor(
    road: &Road,
    run: &[f64],
    radius: f64,
    towns: &crate::field::Sites,
) -> Vec<crate::town::Site> {
    let line = centreline(road, radius);
    if run.len() != line.len() {
        return Vec::new();
    }
    let open = open(&line, radius, towns);
    line.windows(2)
        .zip(run.windows(2))
        .zip(open.windows(2))
        .filter(|(_, o)| o[0] && o[1])
        .map(|((d, h), _)| crate::town::Site::arc((d[0], h[0]), (d[1], h[1]), CORRIDOR))
        .collect()
}

/// Which points of a road's centreline are OUT of every town's own
/// levelling: one per point of `centreline`.
///
/// EVERY town and not only the two a road joins. `road::waysides` grows
/// a village wherever a road has run a day's cart since the last one, so
/// a road passes THROUGH settlements as well as ending at them, and a
/// town's disc levels its ground to the town's level while a corridor
/// ramps to the road's. Where the two overlap the field answers whichever
/// it reaches first and the tarmac stands on the other: measured, 0.10 m
/// off its own lift, which is a road stepping in and out of the ground
/// at every village on it.
///
/// It is measured against the town's whole SITE BAND rather than its
/// outline, because inside the band the town's site is what the field
/// answers; and through `field::Sites`, the latitude index, because a
/// body carries 1,084 settlements and a road 600 points, and the product
/// of those two is not a loop worth writing.
pub fn open(line: &[DVec3], radius: f64, towns: &crate::field::Sites) -> Vec<bool> {
    let window = towns.window(radius);
    line.iter()
        .map(|d| {
            !towns.near(*d, window).any(|site| {
                // How far the town levels THIS WAY, which is its own
                // outline and not the disc round it: measured against
                // the widest a town reaches, a road out along the
                // SQUEEZED axis stopped three hundred metres short of
                // ground the town had never levelled, so the tarmac
                // ended in a field.
                let outer = site.level_r(*d) + crate::field::site_skirt(site);
                (*d - site.nearest(*d).0).length() * radius <= outer
            })
        })
        .collect()
}

/// How near a settlement a stretch of road has to pass to be LIT,
/// metres.
///
/// The owner's own rule: lights along the stretches near a city and
/// none out in the country, which is what a road actually is. A
/// kilometre and a half is the approach to a town rather than the town
/// itself, since a town's own site band ends a couple of hundred metres
/// out and its streets are lit from there in.
pub const LIT_NEAR: f64 = 1_500.0;

/// Which points of a road's centreline are near enough a settlement to
/// carry lamps: one per point of `centreline`.
pub fn lit(line: &[DVec3], radius: f64, towns: &crate::field::Sites) -> Vec<bool> {
    let window = towns.window(radius) + LIT_NEAR / radius;
    line.iter()
        .map(|d| {
            towns
                .near(*d, window)
                .any(|site| (*d - site.nearest(*d).0).length() * radius <= LIT_NEAR)
        })
        .collect()
}
