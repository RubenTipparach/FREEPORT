//! A WAY over the roads: which roads to take from one place to another,
//! found by A* over the waypoints the network was routed on.
//!
//! The graph is the ATLAS's own, and that is what makes it exact. Every
//! road on a body is a walk of ONE Dijkstra tree over one set of
//! waypoints (`connect`), so two roads that share a stretch share the
//! very waypoints it runs through, bit for bit, and every road ends at
//! its two towns' own centres. So a waypoint is a node, a step between
//! two waypoints on any road is an edge, and a junction is simply a node
//! with more than two edges: a fork where a trunk splits, or a town
//! where roads meet. Nothing is matched by distance and there is no
//! tolerance anywhere to get wrong.
//!
//! A node is ten kilometres of road on the harness body, so a way across
//! a continent is a search of a few thousand nodes and is over before a
//! frame notices it. What a way is DRIVEN along is the refined line each
//! road is drawn on, which `trace` lays it onto.

use super::arc;
use super::trunk::project;
use super::Road;
use glam::DVec3;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// Where on the network a place is: the road, the step of its line from
/// waypoint `seg` to the next, the point on that step nearest the place,
/// and how far off the road the place stands, metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spot {
    pub road: usize,
    pub seg: usize,
    pub at: DVec3,
    pub off: f64,
}

/// One run of a way along ONE road, from one place on it to another.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Leg {
    pub road: usize,
    pub from: DVec3,
    pub to: DVec3,
}

/// A way over the roads: its legs in order, and how long it is along
/// the waypoints, metres.
#[derive(Clone, Debug, PartialEq)]
pub struct Way {
    pub legs: Vec<Leg>,
    pub metres: f64,
}

/// The road network as a graph over its own waypoints.
#[derive(Default)]
pub struct Graph {
    radius: f64,
    /// Every distinct waypoint on the body's roads.
    at: Vec<DVec3>,
    /// Each road's waypoints as nodes, in the order of its line.
    nodes: Vec<Vec<u32>>,
    /// Each node's neighbours and how far each is, metres: ONE entry
    /// however many roads share the step, because a trunk three roads
    /// run on is one piece of ground.
    out: Vec<Vec<(u32, f64)>>,
    /// Every step of every road as its lower node, its higher node and
    /// the road, sorted, so which roads run a step is a binary search.
    steps: Vec<(u32, u32, u32)>,
}

/// One node on the search's frontier: its length so far plus the
/// straight run still to go, and which node.
///
/// Ordered on `total_cmp` and then on the node, so the order is TOTAL
/// and two runs pop the same node at every tie, which is the rule the
/// network's own search keeps.
#[derive(PartialEq)]
struct Open {
    f: f64,
    at: u32,
}

impl Eq for Open {}

impl Ord for Open {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reversed, so the heap pops the SHORTEST first.
        other
            .f
            .total_cmp(&self.f)
            .then_with(|| other.at.cmp(&self.at))
    }
}

impl PartialOrd for Open {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A direction's key, to a billionth of the radius, which is a
/// millimetre on the harness body: far finer than two waypoints ever
/// stand and far coarser than the last bit, so one waypoint read off two
/// roads is one key.
fn key(d: DVec3) -> [i64; 3] {
    let q = |v: f64| (v * 1e9).round() as i64;
    [q(d.x), q(d.y), q(d.z)]
}

impl Graph {
    /// The graph of a body's roads, at its radius, metres.
    pub fn of(roads: &[Road], radius: f64) -> Graph {
        let mut keyed: Vec<([i64; 3], u32, u32)> = roads
            .iter()
            .enumerate()
            .flat_map(|(r, road)| {
                road.line
                    .iter()
                    .enumerate()
                    .map(move |(k, (d, _))| (key(*d), r as u32, k as u32))
            })
            .collect();
        keyed.sort_unstable();
        let mut at: Vec<DVec3> = Vec::new();
        let mut nodes: Vec<Vec<u32>> = roads.iter().map(|r| vec![0; r.line.len()]).collect();
        let mut last = None;
        for (k, r, i) in keyed {
            if last != Some(k) {
                at.push(roads[r as usize].line[i as usize].0);
                last = Some(k);
            }
            nodes[r as usize][i as usize] = (at.len() - 1) as u32;
        }
        let mut out = vec![Vec::new(); at.len()];
        let mut steps = Vec::new();
        for (r, ns) in nodes.iter().enumerate() {
            for w in ns.windows(2) {
                let (a, b) = (w[0], w[1]);
                if a == b {
                    continue;
                }
                steps.push((a.min(b), a.max(b), r as u32));
                let metres = arc(at[a as usize], at[b as usize]) * radius;
                for (u, v) in [(a, b), (b, a)] {
                    let list: &mut Vec<(u32, f64)> = &mut out[u as usize];
                    if !list.iter().any(|(n, _)| *n == v) {
                        list.push((v, metres));
                    }
                }
            }
        }
        steps.sort_unstable();
        Graph {
            radius,
            at,
            nodes,
            out,
            steps,
        }
    }

    /// How many waypoints and how many steps between them, for the log.
    pub fn size(&self) -> (usize, usize) {
        (
            self.at.len(),
            self.out.iter().map(Vec::len).sum::<usize>() / 2,
        )
    }

    /// The place on the network nearest a direction, whatever the
    /// distance: every step of every road is looked at, which is a few
    /// thousand chords and is asked when a marker moves, never a frame.
    pub fn snap(&self, dir: DVec3) -> Option<Spot> {
        let dir = dir.try_normalize()?;
        let mut best: Option<Spot> = None;
        for (road, ns) in self.nodes.iter().enumerate() {
            for (seg, w) in ns.windows(2).enumerate() {
                let (_, at) = project(dir, self.at[w[0] as usize], self.at[w[1] as usize]);
                let off = arc(dir, at) * self.radius;
                if best.is_none_or(|b| off < b.off) {
                    best = Some(Spot { road, seg, at, off });
                }
            }
        }
        best
    }

    /// The shortest way over the roads from one place on them to
    /// another, or nothing when no road joins the two, which is the
    /// answer between two continents.
    ///
    /// A* on the straight run still to go, which is ADMISSIBLE because
    /// no road between two places is shorter than the great circle
    /// between them, and CONSISTENT because a step and the run after it
    /// can never be beaten by the run alone: so the first time the goal
    /// comes off the frontier it has come off by the shortest way.
    pub fn route(&self, from: Spot, to: Spot) -> Option<Way> {
        let (sa, sb) = self.ends(from)?;
        let (ga, gb) = self.ends(to)?;
        let r = self.radius;
        let h = |n: u32| arc(self.at[n as usize], to.at) * r;
        let mut dist = vec![f64::INFINITY; self.at.len()];
        let mut came = vec![u32::MAX; self.at.len()];
        let mut open = BinaryHeap::new();
        for s in [sa, sb] {
            let d = arc(from.at, self.at[s as usize]) * r;
            if d < dist[s as usize] {
                dist[s as usize] = d;
                open.push(Open { f: d + h(s), at: s });
            }
        }
        // The best way to the goal found so far and the node it leaves
        // the network from; `None` is straight along the one step both
        // places stand on.
        let shared = (sa.min(sb), sa.max(sb)) == (ga.min(gb), ga.max(gb));
        let mut best = (f64::INFINITY, None);
        if shared {
            best = (arc(from.at, to.at) * r, None);
        }
        while let Some(Open { f, at }) = open.pop() {
            if f >= best.0 {
                break;
            }
            let d = dist[at as usize];
            if f > d + h(at) + 1e-6 {
                continue;
            }
            if at == ga || at == gb {
                // Its f IS the way's whole length, since the heuristic
                // there is the step's own remainder.
                best = (f, Some(at));
                continue;
            }
            for &(next, m) in &self.out[at as usize] {
                let nd = d + m;
                if nd < dist[next as usize] {
                    dist[next as usize] = nd;
                    came[next as usize] = at;
                    open.push(Open {
                        f: nd + h(next),
                        at: next,
                    });
                }
            }
        }
        if !best.0.is_finite() {
            return None;
        }
        let mut chain = Vec::new();
        let mut at = best.1;
        while let Some(n) = at {
            chain.push(n);
            at = Some(came[n as usize]).filter(|c| *c != u32::MAX);
        }
        chain.reverse();
        Some(Way {
            legs: self.legs(from, to, (sa, sb), (ga, gb), &chain),
            metres: best.0,
        })
    }

    /// The shortest way from one direction to another over the roads:
    /// each snapped onto the network and then `route`.
    pub fn way(&self, from: DVec3, to: DVec3) -> Option<Way> {
        self.route(self.snap(from)?, self.snap(to)?)
    }

    /// The two nodes of the step a spot stands on.
    fn ends(&self, spot: Spot) -> Option<(u32, u32)> {
        let ns = self.nodes.get(spot.road)?;
        Some((*ns.get(spot.seg)?, *ns.get(spot.seg + 1)?))
    }

    /// The road a step is RUN on: the lowest numbered road that has it,
    /// because the lowest numbered road on a shared trunk is the one that
    /// owns it (`trunk::merge`) and carries its tarmac, while every other
    /// road there is snapped onto that tarmac and closed.
    fn owner(&self, a: u32, b: u32) -> Option<usize> {
        let (lo, hi) = (a.min(b), a.max(b));
        let at = self.steps.partition_point(|s| (s.0, s.1) < (lo, hi));
        self.steps
            .get(at)
            .filter(|s| (s.0, s.1) == (lo, hi))
            .map(|s| s.2 as usize)
    }

    /// The legs of a way: every step it takes, from the start's own step
    /// through the chain to the goal's, each on the road that owns it,
    /// with a run on one road one leg.
    fn legs(
        &self,
        from: Spot,
        to: Spot,
        start: (u32, u32),
        goal: (u32, u32),
        chain: &[u32],
    ) -> Vec<Leg> {
        let mut steps: Vec<(DVec3, DVec3, Option<usize>)> = Vec::new();
        match (chain.first(), chain.last()) {
            (Some(&first), Some(&last)) => {
                steps.push((
                    from.at,
                    self.at[first as usize],
                    self.owner(start.0, start.1),
                ));
                for w in chain.windows(2) {
                    let (a, b) = (self.at[w[0] as usize], self.at[w[1] as usize]);
                    steps.push((a, b, self.owner(w[0], w[1])));
                }
                steps.push((self.at[last as usize], to.at, self.owner(goal.0, goal.1)));
            }
            _ => steps.push((from.at, to.at, self.owner(start.0, start.1))),
        }
        let mut legs: Vec<Leg> = Vec::new();
        for (a, b, road) in steps {
            let road = road.unwrap_or(from.road);
            match legs.last_mut() {
                Some(leg) if leg.road == road => leg.to = b,
                _ => legs.push(Leg {
                    road,
                    from: a,
                    to: b,
                }),
            }
        }
        legs
    }
}

/// The index of the point of a line nearest a direction.
fn nearest(line: &[DVec3], dir: DVec3) -> Option<usize> {
    (0..line.len()).min_by(|a, b| {
        (line[*a] - dir)
            .length_squared()
            .total_cmp(&(line[*b] - dir).length_squared())
    })
}

/// How far two legs' lines may stand apart where one hands over to the
/// next and still be ONE piece of tarmac, metres: a fork where both run
/// in one corridor (`CORRIDOR`), and not a hop across a town between
/// two roads' own crossings.
const JOIN: f64 = super::CORRIDOR;

/// A way laid onto the lines its roads are DRAWN on: every point it runs
/// through, from `from` to `to`, and whether the step INTO each is on
/// tarmac.
///
/// `line(r)` is road `r`'s refined centreline and which of its points
/// carry tarmac, which is what the harness lays and the core never
/// holds. Each leg runs from the point of its road nearest where the
/// last one ended, which makes a fork one continuous line, since a road
/// forking off a trunk is snapped onto the trunk's own line until it
/// leaves; the first starts nearest `from` and the last ends nearest
/// `to`, the place itself and never the waypoint it was snapped to.
///
/// A step that is not on tarmac is a HOP: into the first road from
/// wherever the place was, across a town between the crossings two
/// roads end at, or through a village the highway stops short of. What
/// a car does there is drive the town's own streets, which the core
/// cannot see from here and the caller can.
pub fn trace<'a>(
    way: &Way,
    from: DVec3,
    to: DVec3,
    line: impl Fn(usize) -> Option<(&'a [DVec3], &'a [bool])>,
    radius: f64,
) -> Vec<(DVec3, bool)> {
    let mut out = vec![(from, false)];
    // The last point laid and whether it is on tarmac.
    let mut last = (from, false);
    for (i, leg) in way.legs.iter().enumerate() {
        let Some((pts, open)) = line(leg.road) else {
            continue;
        };
        let start = if i == 0 { from } else { last.0 };
        let end = if i + 1 == way.legs.len() { to } else { leg.to };
        let (Some(a), Some(b)) = (nearest(pts, start), nearest(pts, end)) else {
            continue;
        };
        let run: Vec<usize> = if a <= b {
            (a..=b).collect()
        } else {
            (b..=a).rev().collect()
        };
        let mut prev: Option<usize> = None;
        for k in run {
            let paved = match prev {
                Some(p) => open.get(k) == Some(&true) && open.get(p) == Some(&true),
                // Where one leg hands over to the next: ONE piece of
                // tarmac when both stand on it and they are a corridor
                // apart at most, and a hop otherwise.
                None => {
                    last.1 && open.get(k) == Some(&true) && arc(last.0, pts[k]) * radius <= JOIN
                }
            };
            out.push((pts[k], paved));
            last = (pts[k], open.get(k) == Some(&true));
            prev = Some(k);
        }
    }
    out.push((to, false));
    out
}

/// How long a traced way is over the ground, metres, hops and all.
pub fn length(points: &[(DVec3, bool)], radius: f64) -> f64 {
    points
        .windows(2)
        .map(|w| arc(w[0].0, w[1].0) * radius)
        .sum()
}

#[cfg(test)]
mod tests;
