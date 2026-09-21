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

/// The steepest a road is ever BUILT at, rise over run.
///
/// Past it the router refuses an edge outright rather than pricing one,
/// so a route never takes a cliff however far the way round is.
///
/// **Seven per cent, which is the owner's own number and is what a
/// motorway is designed to**: a fully laden lorry holds its speed up
/// one and a car never has to think about it. It was a tenth, which is
/// an alpine pass.
///
/// It is the router's refusal between waypoints AND the bound
/// `road::smooth` holds between the 85 m stations the router never
/// looked at, and the second is where it actually matters: the steepest
/// piece on this body climbed at **4966%**, which is eighty eight
/// degrees, and 4.72% of its 769,371 pieces were over even the old
/// tenth. The owner read one off a picture as a road going straight up
/// a hillside.
pub const STEEPEST: f64 = 0.07;

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
        // The line as BUILT and not the one that was routed: a curve
        // fitted at a vertex moves the road off it by up to
        // `CURVE_OFFSET`, and a village grown at a waypoint the tarmac
        // now bends past is a village the road does not go through.
        let built = aligned(&road.line, big_r);
        let mut since = EVERY * 0.5;
        for pair in built.windows(2) {
            since += arc(pair[0], pair[1]) * big_r;
            if since < EVERY {
                continue;
            }
            since = 0.0;
            let dir = pair[1];
            // The ground at the candidate itself, which is what the
            // window is about, asked of the sites this ONE direction can
            // reach. It is one march per candidate and there are a few
            // hundred of them on a body, against a height carried along
            // from a waypoint the curve has left behind.
            let over = surface_radius(&planet.around(dir, 0.0), dir) - sea;
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

mod align;
pub use align::*;

pub mod commute;

mod reach;
pub use reach::*;

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

/// How many times the ground is sampled INSIDE a piece, on top of the
/// two stations at its ends.
///
/// Three, at the quarter points, because what a road has to clear is the
/// ground between its own stations and a station cannot see it: at one
/// sample a piece the worst a road still cut into its own ground was
/// 16.84 m on the rough test ball, at two it was 1.86, and at four it is
/// what `a_road_rides_over_the_ground_rather_than_cutting_into_it`
/// prints. Each one costs a `surface_radius` march at bake time and
/// nothing at all afterwards, because what the atlas carries is the
/// answer.
const PROBES: usize = 3;

/// How far either side of its centreline a road's ground is levelled,
/// metres. The carriageway is `town::LANE` each way and the rest is the
/// verge a road needs to sit in its own cutting rather than on a ledge.
///
/// **And the verge is what carries the road through the LOD**, which is
/// what actually decides the number. A ring box at level L reaches
/// `lattice::cell(L) * CH * HALF`, which is 64 cells, so the finest
/// level whose box holds ground `h` under the eye has a cell of about
/// `h / 64`; the flat part of a corridor is `2 * CORRIDOR` wide, and a
/// lattice column is only GUARANTEED to land on it while a cell fits
/// inside it. So **a road survives to about 128 times this number** and
/// past that the mesher has no sample in the cutting, draws the hill
/// that was there before the road, and the ground closes over the
/// tarmac. That is this crate's own "nothing thinner than a cell's
/// DIAGONAL exists" arriving outdoors.
///
/// At 7 m that was 896 m, and the owner's picture from 900 m up is the
/// road DASHED: 22, 28, 56 and 39 m of tarmac with 4 to 26 m gaps, which
/// is the lattice phase beating against the relief and not the 85 m
/// piece. At 16 m it is 2,048 m, and the flat is two cells of level 5
/// rather than under one, so a column lands on it whatever the phase.
///
/// **AND THAT RULE IS OPTIMISTIC BY A LEVEL**, measured by
/// `examples/lod_over_road`, which contours the chunk the game would
/// contour and raycasts the road's own columns down it: at 16 m cells
/// the road is already 54.2% buried. A column landing on the flat is not
/// enough, because the cell owning the centreline straddles the flat's
/// edge into the `field::ARC_SKIRT` (11 m) ramp, and a cell wider than
/// that ramp solves its vertex up onto the hill. Widening this number
/// MOVES the failing band out one level per doubling rather than curing
/// it; only the corridor leaving the FIELD removes it. CLAUDE.md's
/// "Measure, then decide" carries the sweep.
///
/// 32 m of graded ground for 11 m of tarmac is a verge either side and
/// what a highway alignment actually occupies; the whole network is
/// 0.016% of the body. Going further has a floor: holding the road's own
/// WIDTH rather than one column wants `5.5 + cell`, which is 21.5 m at
/// level 5 and 37 m at level 6, and no width at all carries a road past
/// the altitude where it is under a pixel anyway (9.6 km here). What
/// reaches past that is a DECAL, never geometry and never a material
/// byte on a vertex.
pub const CORRIDOR: f64 = 16.0;

/// How high a road is BUILT over the ground it was routed on, metres.
///
/// The owner's own observation, and it is how a road is actually built:
/// a carriageway stands on a formation raised out of the country, with
/// the batter falling away either side, because water has to leave it
/// and because the ground under it has to be something other than what
/// was there. It is also the cure for the last of the terrain biting the
/// tarmac, and the reason is one sentence: the ground beside an
/// embankment is LOWER than the road, so no chord between two lattice
/// columns can close over it, whatever the cell. A road in a CUTTING has
/// the opposite property and that is what was being drawn.
///
/// It goes on LAST in `smooth`, after the raise only passes have already
/// put the profile at or above the natural ground everywhere, so it is a
/// margin over a profile that was never in a cutting rather than a
/// number papering over one that was.
///
/// **Two metres**, which is a low embankment on a country road and is
/// what makes the mound the owner asked for a mound rather than a lip.
/// It was half a metre, and half a metre is inside the error the
/// TERRAIN's own LOD has: measured on road 0 before this,
/// `roads::ground_over_tarmac` read the coarse ground standing **0.83 m
/// over the tarmac at its worst**, so the country closed over the road
/// wherever a coarse chunk rounded a hill up. Two metres clears that
/// measured worst by more than it is, and the mound
/// (`ribbon::mound`) is what fills the gap it opens between the
/// carriageway and the ground beside it.
///
/// What it costs is the batter's slope: two metres over the corridor's
/// own eleven metre skirt is about one in five and a half, which
/// `docs/civil-engineering.md` puts between the 1:3 a car can drive
/// back up and the 1:6 a mower can take. It is well under the planet's
/// own slope bound, which is set by the relief rather than by this.
/// It is over the walker's 0.60 m step, so the batter and not the
/// shoulder is now how a body gets up onto a road, which is what a real
/// embankment is too.
///
/// It is in the BAKED profile, so it is in the atlas's fingerprint: a
/// file baked at another value describes a road at the wrong level with
/// its tarmac drawn over that, and nothing else in the fingerprint
/// would have said so.
pub const EMBANK: f64 = 2.0;

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
/// routed waypoints with a horizontal CURVE fitted at every vertex that
/// bends (`align::aligned`), and `pieces` divisions between each
/// neighbouring pair of what comes out.
///
/// The curve is fitted HERE rather than stored, which is `pieces` and
/// `step`'s own rule: the atlas keeps the waypoints and both the bake
/// and the game fit the same arcs to them, so the levelling under a road
/// and the tarmac over it are the same line by construction.
pub fn centreline(road: &Road, radius: f64) -> Vec<DVec3> {
    let line = aligned(&road.line, radius);
    let mut out = Vec::new();
    for pair in line.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let n = pieces(a, b, radius);
        for k in 0..n {
            out.push(step(a, b, k, n));
        }
    }
    if let Some(last) = line.last() {
        out.push(*last);
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
///
/// **The CHORD between two stations is held over the ground it spans, so
/// a road RIDES the country rather than cutting into it.**
///
/// It was the ground at the station and nothing else, and the chord
/// between two of those dips under every bulge between them: that dip is
/// a CUTTING, and the corridor duly levels one, seven metres either side
/// of the centreline. Fourteen metres of cutting is a feature no coarse
/// chunk can hold. The rings put a cell of about a sixty fourth of its
/// own distance under the eye (a box at level L is 32 * 2^L metres and
/// its cell is 0.5 * 2^L), so past a couple of hundred metres the
/// mesher has no sample inside the cutting at all, draws the hill that
/// was there before the road, and the ground closes over the tarmac.
/// The owner saw it from the air: terrain on top of the road, and the
/// road not moving up and down with the country.
///
/// So the ground is sampled `PROBES` times INSIDE every gap as well as
/// at its ends, and `smooth` lifts both ends of any chord that passes
/// under one of those samples. Lifting both ends leaves the grade
/// exactly as it was, because a chord raised at both ends has the slope
/// it had. What the corridor then levels is a FILL
/// rather than a cut (`Site::fills` was already true for a road), and a
/// fill the mesher loses leaves the road standing a little proud of the
/// ground, which is an embankment and is what a road on cheap ground
/// is. A cut it loses leaves the road under it, which is a hole.
///
/// It costs `PROBES` extra samples a piece.
pub fn survey(planet: &Planet, road: &Road, dry: f64) -> Vec<f64> {
    let line = centreline(road, planet.radius);
    let ground = |dir: DVec3| {
        let local = planet.around(dir, 1e-9);
        crate::town::surface_radius(&local, dir) - planet.radius
    };
    let run: Vec<f64> = line.iter().map(|d| ground(*d)).collect();
    // The ground BETWEEN the stations, which is what a station's own
    // height says nothing about and what the chord has to clear.
    let mut mid = Vec::with_capacity(line.len().saturating_sub(1) * PROBES);
    for w in line.windows(2) {
        for j in 1..=PROBES {
            mid.push(ground(step(w[0], w[1], j, PROBES + 1)));
        }
    }
    let gap: Vec<f64> = line
        .windows(2)
        .map(|w| arc(w[0], w[1]) * planet.radius)
        .collect();
    smooth(run, &gap, dry, &mid)
}

/// A road SMOOTHS what it crosses by RISING: every sample cleared, then
/// every step held to the grade a highway is built at.
///
/// **The ORDER is the whole of it, and it was wrong.** There were three
/// rounds of forward, backward, probes, floor, ending on the PROBES: the
/// two envelope passes bound the grade and the probe pass then lifted
/// both ends of a chord by whatever the ground between them stood above
/// it, which changes the slope to each end's OTHER neighbour and is
/// bounded by nothing at all. Measured on this body, the steepest piece
/// came out at **4966%**, which is eighty eight degrees, and 4.72% of
/// 769,371 pieces were over even the tenth this used to allow. The owner
/// read one off a picture as a road going straight up a hillside.
///
/// It is TWO statements now and neither iterates, because a fixed point
/// nobody can name is a fixed point nobody can check:
///
/// 1. **A probe is a floor on the two STATIONS either side of it**, and
///    never a lift on the chord. A straight chord between two points
///    both at or above a sample is everywhere at or above it, so this
///    clears the ground between two stations by construction and asks
///    nothing of the chord itself. What it costs is that a road stands
///    at the highest ground within its own piece either side, which is
///    an embankment over a crag and is what a road has.
/// 2. **Then the ENVELOPE, once and LAST.** Forward holds the DESCENT
///    inside the grade and backward the ASCENT, and one pass each is
///    exact rather than approximate: the result is the least profile at
///    or above every station with `|slope| <= STEEPEST`, which is the
///    standard upper envelope. It only ever RAISES, so it cannot push a
///    chord back under a probe it has already cleared, which is why it
///    is safe to put last and why the probes can go first.
///
/// RAISE only, never lower, which is the rule this already had and is
/// worth keeping in view: clamping both ways is real road engineering
/// and it means CUTTING, and a cutting is the one thing this terrain
/// cannot draw, because it is 14 m wide and the rings put a cell of
/// about a sixty fourth of its own distance under the eye. Raising
/// starts the climb earlier and stands the road on an embankment, which
/// the mesher may lose without ever closing over the tarmac.
///
/// Each step is held against its OWN piece's length and not against
/// `PIECE`, because `pieces` rounds a span UP and its pieces are
/// therefore shorter: clamped against the nominal length the last piece
/// of a span came out at one in 9.4 against a limit of one in ten, which
/// `a_corridor_is_never_steeper_than_a_road_is_built` caught.
fn smooth(mut run: Vec<f64>, gap: &[f64], dry: f64, mid: &[f64]) -> Vec<f64> {
    if run.len() < 2 {
        return run.iter().map(|h| h.max(dry) + EMBANK).collect();
    }
    for k in 0..run.len() - 1 {
        for j in 0..PROBES {
            let Some(there) = mid.get(k * PROBES + j) else {
                continue;
            };
            run[k] = run[k].max(*there);
            run[k + 1] = run[k + 1].max(*there);
        }
    }
    for h in &mut run {
        *h = h.max(dry);
    }
    for k in 1..run.len() {
        run[k] = run[k].max(run[k - 1] - STEEPEST * gap[k - 1]);
    }
    for k in (0..run.len() - 1).rev() {
        run[k] = run[k].max(run[k + 1] - STEEPEST * gap[k]);
    }
    // And the EMBANKMENT, last, so it rides on top of a profile that
    // already clears the ground everywhere.
    for h in &mut run {
        *h += EMBANK;
    }
    run
}
