//! Who is out on a town's streets, and where they are at a moment.
//!
//! **An NPC and a car are ON RAILS**, which is this project's own rule for
//! everything that moves and is not the player (the section on bodies in
//! `CLAUDE.md`): a townsman's place is a closed form function of the town,
//! his own index and the world clock. Nothing integrates, nothing is
//! saved, two clients agree by construction, and a town nobody is near
//! costs exactly nothing, because the function is never asked. What
//! INTEGRATES is the player, and only the player.
//!
//! That is also the lamps' rule a second time (`lamps.rs`: a light near
//! the eye and a number everywhere else). 160 towns carry tens of
//! thousands of people; the ECS holds the few dozen within reach and the
//! rest are a function nobody evaluates.
//!
//! Coordinates here are the TOWN's, x east and z north in metres, which
//! is what `town::lot_frame` maps onto the sphere.

use crate::noise::hash3;
use crate::town::{arm, band, paved, Piece, Town, BLOCK, KERB, LANE, PITCH, STREET, WALK};
use glam::DVec2;

/// Half the street, metres: how far its edge stands from its centreline,
/// and what every lane below is measured against.
pub const HALF_STREET: f64 = STREET / 2.0;
/// How far from the centreline a car and a person keep, metres, on the
/// RIGHT of the way they are going, so oncoming traffic passes on the
/// left and a pavement is the outside of the street.
///
/// Neither is a number of its own: a car rides the MIDDLE OF ITS LANE
/// and a person the middle of the pavement, so both are read off the
/// street's own cross section and cannot drift from the paving they are
/// drawn over. A lane is 2.75 m and a car 1.6 across, so two passing
/// cars have better than a metre between them; the pavement is 1.5 m
/// and a person 0.45.
const CAR_LANE: f64 = LANE * 0.5;
const FOOT_LANE: f64 = LANE + WALK * 0.5;
/// How tightly each turns a corner, metres. A fillet rather than a
/// vertex: an agent walking the offset polyline straight would turn
/// ninety degrees between two frames, which reads as a car teleporting
/// round the corner, and an arc is what a TANGENT that does not jump
/// costs. A right turn's fillet bulges by `radius * (1 - 1 / root 2)`
/// past the lane, so a car reaches 1.71 m and a person 3.71 of the
/// 4.25 the street's own half width is: both stay on the paving.
const CAR_TURN: f64 = 1.15;
const FOOT_TURN: f64 = 0.7;
/// How fast each goes, metres a second. A town car is slow, and the pace
/// on foot is a little under the walker's own five, because a pedestrian
/// with somewhere to be still ambles beside one who is running.
const CAR_SPEED: f64 = 7.5;
const FOOT_SPEED: f64 = 1.35;
/// How much of that speed an agent's own hash is worth, either way, so a
/// street is not a parade in lockstep.
const SPREAD: f64 = 0.28;
/// A stride, metres: how far one leg carries a walker, which is what a
/// gait's phase is measured in.
pub const STRIDE: f64 = 0.78;

/// How many people and how many cars a town turns out per LOT it carries.
/// A lot is a building, so this is households rather than an area.
const FOLK_PER_LOT: f64 = 0.75;
const CARS_PER_LOT: f64 = 0.19;
/// Nobody at all where a town has no lots, and never a crowd of one.
const FEWEST: usize = 2;

/// What an agent is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// On the pavement, on foot.
    Foot,
    /// On the roadway, driving.
    Car,
}

impl Kind {
    /// How far off the centreline it keeps, metres.
    fn lane(self) -> f64 {
        match self {
            Kind::Foot => FOOT_LANE,
            Kind::Car => CAR_LANE,
        }
    }

    /// How tightly it turns a corner, metres.
    fn turn(self) -> f64 {
        match self {
            Kind::Foot => FOOT_TURN,
            Kind::Car => CAR_TURN,
        }
    }

    /// How fast it goes, metres a second.
    fn speed(self) -> f64 {
        match self {
            Kind::Foot => FOOT_SPEED,
            Kind::Car => CAR_SPEED,
        }
    }
}

/// Where an agent is and which way it faces, in the town's own metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spot {
    /// East and north, metres from the town's middle.
    pub at: DVec2,
    /// Which way it is going, radians anticlockwise from east.
    pub yaw: f64,
    /// How far it has come, metres. A gait is measured on this and never
    /// on a clock: a leg swings on `along / STRIDE` and a wheel turns on
    /// the same number, so a thing that has stopped has stopped moving
    /// its legs too, for free.
    pub along: f64,
}

/// One node of the street graph: the crossing of street line `i` east and
/// street line `j` north.
type Node = (i32, i32);

/// Where a street line stands, metres from the town's middle. The pieces
/// `town::streets_of` lays are centred on exactly these lines, and the
/// run between two of them is one block's frontage.
fn line(i: i32) -> f64 {
    i as f64 * PITCH - BLOCK / 2.0 - STREET / 2.0
}

/// Which street line a coordinate is on.
fn line_at(v: f64) -> i32 {
    ((v + BLOCK / 2.0 + STREET / 2.0) / PITCH).round() as i32
}

/// Which block row or column a coordinate falls in.
fn block_at(v: f64) -> i32 {
    (v / PITCH).round() as i32
}

/// The four ways off a crossing, anticlockwise from east, which is the
/// order the face walk turns through.
const WAYS: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

/// The node one step along `way` from `n`.
fn step(n: Node, way: usize) -> Node {
    let (dx, dz) = WAYS[way];
    (n.0 + dx, n.1 + dz)
}

/// Where a node stands in the town's own metres.
fn place(n: Node) -> DVec2 {
    DVec2::new(line(n.0), line(n.1))
}

/// The right of a heading, in a frame with x east and z north.
fn right(v: DVec2) -> DVec2 {
    DVec2::new(v.y, -v.x)
}

/// A town's streets as a GRAPH, derived from the pieces that are actually
/// DRAWN rather than from the plan that made them.
///
/// That is a wall's own rule again ("one oriented box which is drawn and
/// collided"): traffic rides the paving a player can see, so a street
/// that was never laid can never have a car on it, and the two cannot
/// drift apart because there is one set of numbers.
///
/// No `HashMap` in it, because this is replayed state: the edges are a
/// SORTED vector and a lookup is a binary search.
#[derive(Clone, Debug, Default)]
pub struct Streets {
    /// Every edge once, low end first: `(node, way)` with `way` always
    /// east (0) or north (1).
    edges: Vec<(Node, usize)>,
}

impl Streets {
    /// The graph of a town's own paving.
    pub fn of(town: &Town) -> Streets {
        // The RUNS only: a crossing is a node rather than an edge, and
        // it is square, so `edge_of` could not tell which way it lay.
        let mut edges: Vec<(Node, usize)> = town
            .pieces
            .iter()
            .filter(|p| p.run())
            .map(Streets::edge_of)
            .collect();
        edges.sort_unstable();
        edges.dedup();
        Streets { edges }
    }

    /// How many streets there are, which is what bounds every walk in the
    /// graph: a face cannot be longer than all the directed edges.
    pub fn len(&self) -> usize {
        self.edges.len()
    }

    /// Nothing paved, so nobody is out.
    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }

    /// Which edge a RUN of street lies on. A run wider east than north
    /// is part of a street running NORTH (`STREET` across it and a
    /// piece's length along it), and the other way about for one
    /// running east.
    fn edge_of(p: &Piece) -> (Node, usize) {
        if p.northerly() {
            ((line_at(p.x), block_at(p.z)), 1)
        } else {
            ((block_at(p.x), line_at(p.z)), 0)
        }
    }

    /// Which ARMS a node has, as `town::arm`'s own bits, which is what
    /// `town::streets_of` writes onto a crossing piece. The graph and
    /// the plan agree by construction, because the graph is read off
    /// the pieces the plan laid.
    pub fn arms(&self, n: Node) -> u8 {
        let bit = [arm::EAST, arm::NORTH, arm::WEST, arm::SOUTH];
        (0..4).filter(|&w| self.has(n, w)).map(|w| bit[w]).sum()
    }

    /// Is there a street from `n` along `way`?
    fn has(&self, n: Node, way: usize) -> bool {
        let key = match way {
            0 | 1 => (n, way),
            2 => ((n.0 - 1, n.1), 0),
            _ => ((n.0, n.1 - 1), 1),
        };
        self.edges.binary_search(&key).is_ok()
    }

    /// Every directed edge, sorted.
    fn directed(&self) -> Vec<(Node, usize)> {
        let mut out = Vec::with_capacity(self.edges.len() * 2);
        for &(n, w) in &self.edges {
            out.push((n, w));
            out.push((step(n, w), (w + 2) % 4));
        }
        out.sort_unstable();
        out
    }

    /// The directed edge a FACE walk takes next: arrive at `step(n, way)`
    /// and leave along the next street CLOCKWISE from the way back.
    ///
    /// This is the classic face traversal of a planar graph, and it is
    /// what makes a circuit a circuit with nobody searching for one: the
    /// successor is a PERMUTATION of the directed edges, so every orbit
    /// of it closes and the orbits partition them
    /// (`every_street_is_on_exactly_one_circuit` measures both). On a
    /// street grid an orbit is the way round one block, or round a group
    /// of them where the grid has gaps, or the outside of the whole town.
    /// A dead end has one way off it, which is the way back, so a face
    /// walks up a cul de sac and turns round, which is what a car does.
    fn next(&self, n: Node, way: usize) -> (Node, usize) {
        let to = step(n, way);
        let back = (way + 2) % 4;
        for turn in 1..=4 {
            let w = (back + 4 - turn) % 4;
            if self.has(to, w) {
                return (to, w);
            }
        }
        (to, back)
    }
}

/// A run of a circuit: a straight, or a turn about a centre. Keeping the
/// ARC rather than the vertex is what gives an agent a heading that does
/// not jump.
#[derive(Clone, Copy, Debug)]
enum Run {
    Line {
        from: DVec2,
        to: DVec2,
    },
    Turn {
        centre: DVec2,
        from: f64,
        sweep: f64,
        radius: f64,
    },
}

impl Run {
    fn length(&self) -> f64 {
        match self {
            Run::Line { from, to } => (*to - *from).length(),
            Run::Turn { sweep, radius, .. } => sweep.abs() * radius,
        }
    }

    /// The place and the heading `s` metres into this run.
    fn at(&self, s: f64) -> (DVec2, f64) {
        match self {
            Run::Line { from, to } => {
                let d = *to - *from;
                let n = d.length().max(f64::MIN_POSITIVE);
                (*from + d * (s / n), d.y.atan2(d.x))
            }
            Run::Turn {
                centre,
                from,
                sweep,
                radius,
            } => {
                let a = from + sweep.signum() * (s / radius);
                let (sin, cos) = a.sin_cos();
                (
                    *centre + DVec2::new(cos, sin) * *radius,
                    a + sweep.signum() * std::f64::consts::FRAC_PI_2,
                )
            }
        }
    }
}

/// A closed loop of street an agent goes round for ever, offset into its
/// own lane and filleted at every corner.
#[derive(Clone, Debug)]
pub struct Circuit {
    runs: Vec<Run>,
    /// Where each run starts, metres round the loop, with the whole
    /// length last, so a binary search answers `at`.
    marks: Vec<f64>,
}

impl Circuit {
    /// How far round it is, metres.
    pub fn length(&self) -> f64 {
        self.marks.last().copied().unwrap_or(0.0)
    }

    /// Where a thing `s` metres round the loop is. `s` is taken modulo
    /// the length, so a clock that only ever grows is fine.
    pub fn at(&self, s: f64) -> Spot {
        let len = self.length();
        if len <= 0.0 || self.runs.is_empty() {
            return Spot {
                at: DVec2::ZERO,
                yaw: 0.0,
                along: s,
            };
        }
        let t = s.rem_euclid(len);
        // The last mark at or under t. The subtraction cannot underflow,
        // because marks[0] is nought and t is not negative.
        let i = (self.marks.partition_point(|&m| m <= t) - 1).min(self.runs.len() - 1);
        let (at, yaw) = self.runs[i].at(t - self.marks[i]);
        Spot { at, yaw, along: s }
    }
}

/// Where a lane turns at `node` from heading `d` to heading `e`: where
/// the straight before it ends, the turn itself, and where the straight
/// after it starts. All three cases a face walk can hand it are here.
fn corner(node: DVec2, d: DVec2, e: DVec2, off: f64, radius: f64) -> (DVec2, Option<Run>, DVec2) {
    let cross = d.x * e.y - d.y * e.x;
    if cross.abs() > 1e-9 {
        // A quarter turn. The two offset lines meet a lane's own offset
        // beyond the crossing, and the fillet takes `radius` off each
        // side of that, which for a right angle is the tangent length.
        let c = node + right(d) * off + d * (off * cross.signum());
        let entry = c - d * radius;
        let centre = entry + DVec2::new(-d.y, d.x) * (radius * cross.signum());
        let from = entry - centre;
        return (
            entry,
            Some(Run::Turn {
                centre,
                from: from.y.atan2(from.x),
                sweep: cross.signum() * std::f64::consts::FRAC_PI_2,
                radius,
            }),
            c + e * radius,
        );
    }
    if d.dot(e) > 0.0 {
        // Straight on through a crossing: one line and no corner at all.
        let p = node + right(d) * off;
        return (p, None, p);
    }
    // A DEAD END, which is what a face walk does at a cul de sac: the
    // lane swings round the end of the street at its own offset, which is
    // the half circle joining the two sides of it. Filleting this as a
    // corner would leave the two lanes an offset apart with nothing
    // between them, and an agent would cross the gap in one frame.
    let start = right(d);
    (
        node + start * off,
        Some(Run::Turn {
            centre: node,
            from: start.y.atan2(start.x),
            sweep: std::f64::consts::PI,
            radius: off,
        }),
        node + right(e) * off,
    )
}

/// The polyline through `nodes`, offset into `kind`'s lane on its right
/// and filleted at each corner, as runs.
fn lane(nodes: &[Node], kind: Kind) -> Circuit {
    let n = nodes.len();
    let off = kind.lane();
    let radius = kind.turn();
    let dir = |k: usize| (place(nodes[(k + 1) % n]) - place(nodes[k])).normalize_or(DVec2::X);
    // Corner k stands at nodes[k], between leg k - 1 and leg k.
    let corners: Vec<(DVec2, Option<Run>, DVec2)> = (0..n)
        .map(|k| corner(place(nodes[k]), dir((k + n - 1) % n), dir(k), off, radius))
        .collect();
    let mut runs = Vec::with_capacity(n * 2);
    for k in 0..n {
        let from = corners[k].2;
        let to = corners[(k + 1) % n].0;
        // A leg the two fillets have eaten whole leaves no straight, and
        // a straight the wrong way round would be one walked backwards.
        if (to - from).dot(dir(k)) > 1e-9 {
            runs.push(Run::Line { from, to });
        }
        if let Some(turn) = corners[(k + 1) % n].1 {
            runs.push(turn);
        }
    }
    let mut marks = Vec::with_capacity(runs.len() + 1);
    let mut s = 0.0;
    for r in &runs {
        marks.push(s);
        s += r.length();
    }
    marks.push(s);
    Circuit { runs, marks }
}

/// The two lanes of one face: the pavement and the roadway. One face is
/// walked once and both are cut from it, because the loop belongs to the
/// street and the lane to the traveller.
#[derive(Clone, Debug)]
pub struct Lanes {
    pub foot: Circuit,
    pub car: Circuit,
}

impl Lanes {
    /// The lane a kind rides.
    pub fn of(&self, kind: Kind) -> &Circuit {
        match kind {
            Kind::Foot => &self.foot,
            Kind::Car => &self.car,
        }
    }
}

/// One of a town's people or cars: which loop it is on, how fast, and
/// where round it it stood at time nought. That is the WHOLE of what is
/// stored, because everything else is a function of it and the clock.
#[derive(Clone, Debug)]
pub struct Agent {
    pub kind: Kind,
    /// Which of the town's faces it goes round.
    pub face: usize,
    /// Metres a second, its own.
    pub speed: f64,
    /// How far round the loop it stood at time nought, metres.
    pub phase: f64,
    /// A number of its own, for whatever a model wants to differ on.
    pub id: u32,
}

/// A town's traffic: its streets, its circuits and everybody on them.
#[derive(Clone, Debug, Default)]
pub struct Traffic {
    pub faces: Vec<Lanes>,
    pub agents: Vec<Agent>,
    /// The graph the circuits were cut from, kept because a PAVEMENT is
    /// higher than a carriageway and which of the two a point stands on
    /// is a question about the crossing it is in.
    streets: Streets,
}

impl Traffic {
    /// Everybody a town turns out. The count is off its LOTS, because a
    /// lot is a building and a building is a household.
    pub fn of(town: &Town, seed: u32) -> Traffic {
        let streets = Streets::of(town);
        let faces = faces(&streets);
        if faces.is_empty() {
            return Traffic::default();
        }
        let folk = count(town.lots.len(), FOLK_PER_LOT);
        let cars = count(town.lots.len(), CARS_PER_LOT);
        let mut agents = Vec::with_capacity(folk + cars);
        for (how_many, kind) in [(folk, Kind::Foot), (cars, Kind::Car)] {
            for _ in 0..how_many {
                agents.push(one(&faces, kind, agents.len(), town.index, seed));
            }
        }
        Traffic {
            faces,
            agents,
            streets,
        }
    }

    /// How high the ground a point stands on is over the carriageway,
    /// metres: `KERB` on a pavement and nought on the road.
    ///
    /// A person keeps to the middle of the pavement, so along a RUN he
    /// is always up on the kerb. What he crosses is a crossing's own
    /// arms: walking straight through a crossroads takes him over the
    /// side street, and that band of the square is carriageway exactly
    /// when the arm is there. So he steps DOWN off the kerb where he
    /// crosses a street and nowhere else, off the same three bands
    /// `model::crossing` paves, rather than off a second rule about
    /// where a kerb is.
    pub fn lift(&self, at: DVec2) -> f64 {
        let node = (line_at(at.x), line_at(at.y));
        let d = at - place(node);
        let h = HALF_STREET;
        if d.x.abs() > h || d.y.abs() > h {
            return KERB;
        }
        if paved(self.streets.arms(node), band(d.x), band(d.y)) {
            0.0
        } else {
            KERB
        }
    }

    /// Where an agent is at `time` seconds. The ONE place the clock is
    /// read, and the whole of the motion.
    pub fn at(&self, agent: &Agent, time: f64) -> Spot {
        self.faces[agent.face]
            .of(agent.kind)
            .at(agent.phase + agent.speed * time)
    }
}

/// How many of a kind a town of `lots` buildings turns out.
fn count(lots: usize, per: f64) -> usize {
    let n = (lots as f64 * per).round() as usize;
    if n == 0 {
        0
    } else {
        n.max(FEWEST)
    }
}

/// Every face of the street graph, each with its two lanes. The orbits of
/// `Streets::next` partition the directed edges, so marking a whole orbit
/// when it is first reached cuts each face exactly once however many of
/// its edges the outer loop passes.
fn faces(streets: &Streets) -> Vec<Lanes> {
    let mut seen: Vec<(Node, usize)> = Vec::new();
    let mut out = Vec::new();
    for start in streets.directed() {
        if seen.binary_search(&start).is_ok() {
            continue;
        }
        let cap = streets.len() * 2 + 1;
        let mut nodes = Vec::new();
        let mut at = start;
        for _ in 0..cap {
            match seen.binary_search(&at) {
                Ok(_) => break,
                Err(k) => seen.insert(k, at),
            }
            at = streets.next(at.0, at.1);
            nodes.push(at.0);
            if at == start {
                break;
            }
        }
        if nodes.len() < 2 || at != start {
            continue;
        }
        let lanes = Lanes {
            foot: lane(&nodes, Kind::Foot),
            car: lane(&nodes, Kind::Car),
        };
        if lanes.foot.length() > 1.0 && lanes.car.length() > 1.0 {
            out.push(lanes);
        }
    }
    out
}

/// One agent, hashed off its own index so a town is the same town twice.
fn one(faces: &[Lanes], kind: Kind, slot: usize, town: usize, seed: u32) -> Agent {
    let h = |k: i64| hash3(slot as i64, town as i64, k, seed);
    let face = ((h(1) * faces.len() as f64) as usize).min(faces.len() - 1);
    Agent {
        kind,
        face,
        speed: kind.speed() * (1.0 + (h(2) * 2.0 - 1.0) * SPREAD),
        phase: h(3) * faces[face].of(kind).length(),
        id: (h(4) * u32::MAX as f64) as u32,
    }
}

#[cfg(test)]
mod tests;
