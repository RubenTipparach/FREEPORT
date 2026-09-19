//! A building or a street as a MESH built from PARAMETERS.
//!
//! The buildings used to be lists of signed distance brushes cut into the
//! ground's own field, on a lattice six times finer under each town. That
//! bought one thing, which is that the picture and the collider were the
//! same field, and it cost a second lattice, a massing rule for the far
//! chunks, and a chunk test against every structure. The owner's ask is
//! parametric models instead: ONE lattice, dual contoured, carrying
//! nothing but terrain, and what stands on it is geometry built from
//! numbers.
//!
//! The rule that keeps the old guarantee is that a wall is ONE oriented
//! box which is both drawn and collided (`Model::solid`), so the picture
//! and the collider are the same numbers and cannot drift. Anything a
//! body should pass through is `trim` or `quad` and only draws, which is
//! the mockup's own lesson about a floor line the size of a lot catching
//! a climber's head.
//!
//! Coordinates are the frame's: x east, y north, z up from the ground,
//! which is what `town::Frame` maps to and from.

use crate::dc::DcMesh;
use crate::field::{hash3, Block, CONCRETE, GLASS, LAMP, LIT, PAINT, PLATE, STREET};
use crate::town::{lot_frame, paved, Frame, Piece, Town, BANDS, KERB, LANE, LIFT, WALK};
use glam::{DVec2, DVec3};

/// How tall a storey stands, metres.
pub const STOREY: f64 = 3.2;
/// How thick a wall is, metres.
pub const WALL: f64 = 0.35;
/// The doorway: how wide and how high, metres.
pub const DOOR_W: f64 = 1.6;
pub const DOOR_H: f64 = 2.3;
/// A floor slab and a roof slab, metres thick.
const SLAB: f64 = 0.25;
/// The parapet round a flat roof: how high it stands over the slab and how
/// thick it is, metres.
const PARAPET: f64 = 0.7;
const PARAPET_T: f64 = 0.22;
/// A window: how wide and how high, how far over its own floor the sill
/// is, and how far apart the panes are along a wall, metres.
const PANE_W: f64 = 1.2;
const PANE_H: f64 = 1.5;
const SILL: f64 = 1.0;
const PANE_PITCH: f64 = 2.6;
/// How far proud of the wall a pane sits, metres: enough that no depth
/// test can put the wall in front of it, and under anything an eye reads
/// as a ledge.
const PROUD: f64 = 0.03;
/// How many panes in ten are lit from within after dark.
const LIT_SHARE: f64 = 0.38;
/// A lamp: how big its box is and how far it reaches, metres.
const LAMP_R: f64 = 0.22;
pub const LAMP_REACH: f64 = 9.0;
/// How far a pavement's slab is sunk INTO the ground, metres. Its
/// underside is then never a plane the levelled terrain can fight with:
/// dual contouring holds a plane to two millimetres and a slab sitting
/// exactly on it would fleck along its whole length.
const BURY: f64 = 0.15;
/// How wide a painted marking is, metres, and how high over the
/// carriageway it is laid. Four millimetres is nothing an eye can see
/// as a step and is a hundred times the two the paving is flat to.
const PAINT_W: f64 = 0.12;
const PAINT_UP: f64 = 0.004;
/// How far the edge line stands in from the kerb, metres.
const EDGE_IN: f64 = 0.18;
/// What share of a piece the centreline's dash takes, so the gap between
/// two dashes is the rest of it.
const DASH: f64 = 0.55;
/// How many sides a round tower is drawn and collided with.
const SIDES: usize = 12;
/// How many pieces a barrel vault's arc is cut into.
const ARCH: usize = 9;

/// An oriented box in the model's own frame: what is DRAWN and what a body
/// is stopped by, from one set of numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Solid {
    /// Its middle.
    pub centre: DVec3,
    /// Half its extent along its own axes, metres.
    pub half: DVec3,
    /// How far its axes are turned about the frame's up, radians.
    pub yaw: f64,
    /// What it is made of, which the walker reads when it stands on it.
    pub material: u8,
}

impl Solid {
    /// Its axes in the frame: east, north and up, turned by its yaw.
    pub fn axes(&self) -> [DVec3; 3] {
        let (s, c) = self.yaw.sin_cos();
        [
            DVec3::new(c, s, 0.0),
            DVec3::new(-s, c, 0.0),
            DVec3::new(0.0, 0.0, 1.0),
        ]
    }

    /// The same box in the world, through the frame it was written in.
    pub fn block(&self, frame: &Frame) -> Block {
        let a = self.axes();
        let out = |v: DVec3| frame.east * v.x + frame.north * v.y + frame.dir * v.z;
        Block {
            centre: frame.world(self.centre),
            half: self.half,
            axes: [out(a[0]), out(a[1]), out(a[2])],
            material: self.material,
        }
    }

    /// Its eight corners in the frame, in the order the faces below read
    /// them: the low z face first, then the high one, each anticlockwise
    /// seen from outside.
    fn corners(&self) -> [DVec3; 8] {
        let a = self.axes();
        let mut out = [DVec3::ZERO; 8];
        for (i, c) in out.iter_mut().enumerate() {
            let s = DVec3::new(
                if i & 1 == 0 { -1.0 } else { 1.0 },
                if i & 2 == 0 { -1.0 } else { 1.0 },
                if i & 4 == 0 { -1.0 } else { 1.0 },
            );
            *c = self.centre
                + a[0] * (s.x * self.half.x)
                + a[1] * (s.y * self.half.y)
                + a[2] * (s.z * self.half.z);
        }
        out
    }
}

/// A model: its triangles in its own frame, the boxes a body is stopped
/// by, and where its lamps hang.
#[derive(Clone, Debug, Default)]
pub struct Model {
    pub mesh: DcMesh,
    pub solids: Vec<Solid>,
    pub lamps: Vec<DVec3>,
}

impl Model {
    /// Nothing yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// A triangle, wound so `(b - a) x (c - a)` points out of the solid.
    pub fn tri(&mut self, a: DVec3, b: DVec3, c: DVec3, material: u8) {
        let n = (b - a).cross(c - a).normalize_or(DVec3::Z).as_vec3();
        let base = self.mesh.positions.len() as u32;
        for p in [a, b, c] {
            self.mesh.positions.push(p.as_vec3().to_array());
            self.mesh.normals.push(n.to_array());
            self.mesh.levels.push(0);
        }
        self.mesh
            .indices
            .extend_from_slice(&[base, base + 1, base + 2]);
        self.mesh.materials.push(material);
    }

    /// A quad, as two triangles on one plane.
    pub fn quad(&mut self, a: DVec3, b: DVec3, c: DVec3, d: DVec3, material: u8) {
        self.tri(a, b, c, material);
        self.tri(a, c, d, material);
    }

    /// A box that only DRAWS: a parapet, an eave, a sill. Nothing a body
    /// can be inside, because nothing here is asked about collision.
    pub fn trim(&mut self, centre: DVec3, half: DVec3, yaw: f64, material: u8) {
        let k = Solid {
            centre,
            half,
            yaw,
            material,
        }
        .corners();
        // Each face anticlockwise from outside: low and high on each axis.
        self.quad(k[0], k[4], k[6], k[2], material);
        self.quad(k[1], k[3], k[7], k[5], material);
        self.quad(k[0], k[1], k[5], k[4], material);
        self.quad(k[2], k[6], k[7], k[3], material);
        self.quad(k[0], k[2], k[3], k[1], material);
        self.quad(k[4], k[5], k[7], k[6], material);
    }

    /// A box that draws AND stops a body: a wall, a slab, a pillar.
    pub fn solid(&mut self, centre: DVec3, half: DVec3, yaw: f64, material: u8) {
        self.trim(centre, half, yaw, material);
        self.solids.push(Solid {
            centre,
            half,
            yaw,
            material,
        });
    }

    /// A pane, proud of a wall whose outward normal is `out`, lit from
    /// within or dark by a throw of the dice.
    pub fn pane(&mut self, centre: DVec3, out: DVec3, wide: DVec3, high: f64, dice: f64) {
        let at = centre + out * PROUD;
        let wide = if wide.cross(DVec3::Z).dot(out) < 0.0 {
            -wide
        } else {
            wide
        };
        let (u, v) = (wide * (PANE_W * 0.5), DVec3::Z * (high * 0.5));
        let material = if dice < LIT_SHARE { LIT } else { GLASS };
        self.quad(at - u - v, at + u - v, at + u + v, at - u + v, material);
    }

    /// A lamp: a small box that glows, and the light that goes with it.
    pub fn lamp(&mut self, at: DVec3) {
        self.trim(at, DVec3::splat(LAMP_R), 0.0, LAMP);
        self.lamps.push(at);
    }

    /// This model's boxes in the world, through the frame it stands in.
    pub fn blocks(&self, frame: &Frame) -> Vec<Block> {
        self.solids.iter().map(|s| s.block(frame)).collect()
    }

    /// Its lamps in the world, with their reach.
    pub fn lights(&self, frame: &Frame) -> Vec<(DVec3, f64)> {
        self.lamps
            .iter()
            .map(|&l| (frame.world(l), LAMP_REACH))
            .collect()
    }

    /// How tall the tallest thing in it stands, metres.
    pub fn high(&self) -> f64 {
        self.mesh
            .positions
            .iter()
            .fold(0.0_f64, |h, p| h.max(p[2] as f64))
    }
}

/// What a lot carries. The recipes these replace were JSON files read at
/// startup; a kind is a match arm, so a new one is a variant and an arm
/// and nothing to ship beside the binary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A rectangular tower, four to eight storeys, plate pillars at its
    /// corners and a parapet: what a city is mostly made of.
    Block,
    /// One or two storeys under a gable.
    House,
    /// One storey, wide windows, a flat roof.
    Bungalow,
    /// Round, three to six storeys, a parapet.
    Tower,
    /// A shed under a barrel vault, with a tall door: what a port is for.
    Hangar,
}

impl Kind {
    /// Its name, for a log and for a picture's caption.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Block => "block",
            Kind::House => "house",
            Kind::Bungalow => "bungalow",
            Kind::Tower => "tower",
            Kind::Hangar => "hangar",
        }
    }

    /// The fewest and the most storeys it stands in.
    pub fn storeys(self) -> (u32, u32) {
        match self {
            Kind::Block => (4, 8),
            Kind::House => (1, 2),
            Kind::Bungalow => (1, 1),
            Kind::Tower => (3, 6),
            Kind::Hangar => (1, 1),
        }
    }

    /// Every kind, so a harness can build one of each.
    pub fn all() -> [Kind; 5] {
        [
            Kind::Block,
            Kind::House,
            Kind::Bungalow,
            Kind::Tower,
            Kind::Hangar,
        ]
    }
}

/// A building of `kind` on a lot `w` by `d` metres, `storeys` tall, from
/// `seed`: its walls, its roof, its panes and its lamps.
///
/// It is a SHELL with a doorway: a walker goes in at the door and stands
/// in one room the height of the building. Floors and stairs are what is
/// missing and are named here rather than hidden, because on a field they
/// were brushes and here they are geometry nobody has written yet.
pub fn building(kind: Kind, w: f64, d: f64, storeys: u32, seed: u32) -> Model {
    let (low, high) = kind.storeys();
    let n = storeys.clamp(low, high);
    let h = n as f64 * STOREY;
    let mut m = Model::new();
    match kind {
        Kind::Tower => round(&mut m, w.min(d) * 0.5, h, seed),
        _ => shell(&mut m, w, d, h, seed),
    }
    match kind {
        Kind::House => gable(&mut m, w, d, h),
        Kind::Hangar => vault(&mut m, w, d, h),
        Kind::Tower => flat_roof(&mut m, w.min(d), w.min(d), h),
        _ => flat_roof(&mut m, w, d, h),
    }
    if kind == Kind::Block {
        pillars(&mut m, w, d, h);
    }
    // A lamp over the door, and one under the ceiling of every storey, so
    // a room is lit by what is in it rather than by the sun it cannot see.
    m.lamp(DVec3::new(0.0, -d * 0.5 - LAMP_R, DOOR_H + 0.5));
    for k in 0..n {
        m.lamp(DVec3::new(0.0, 0.0, (k + 1) as f64 * STOREY - 0.5));
    }
    m
}

/// Four walls, a floor, a doorway in the south wall and panes up the rest.
fn shell(m: &mut Model, w: f64, d: f64, h: f64, seed: u32) {
    let t = WALL * 0.5;
    let (hw, hd) = (w * 0.5, d * 0.5);
    m.solid(
        DVec3::new(0.0, 0.0, SLAB * 0.5),
        DVec3::new(hw, hd, SLAB * 0.5),
        0.0,
        CONCRETE,
    );
    m.solid(
        DVec3::new(0.0, hd - t, h * 0.5),
        DVec3::new(hw, t, h * 0.5),
        0.0,
        CONCRETE,
    );
    for side in [-1.0, 1.0] {
        m.solid(
            DVec3::new(side * (hw - t), 0.0, h * 0.5),
            DVec3::new(t, hd - WALL, h * 0.5),
            0.0,
            CONCRETE,
        );
    }
    // The south wall is the doorway's: two piers and a lintel over them.
    let pier = (w - DOOR_W) * 0.25;
    for side in [-1.0, 1.0] {
        m.solid(
            DVec3::new(side * (DOOR_W * 0.5 + pier), -(hd - t), h * 0.5),
            DVec3::new(pier, t, h * 0.5),
            0.0,
            CONCRETE,
        );
    }
    m.solid(
        DVec3::new(0.0, -(hd - t), (DOOR_H + h) * 0.5),
        DVec3::new(DOOR_W * 0.5, t, (h - DOOR_H) * 0.5),
        0.0,
        CONCRETE,
    );
    panes(m, w, d, h, seed);
}

/// Panes up all four walls, a storey at a time, and none where the door
/// is: a window cut across a doorway is the recipes' own rule about a
/// window beside a flight, arrived at from the other side.
fn panes(m: &mut Model, w: f64, d: f64, h: f64, seed: u32) {
    let (hw, hd) = (w * 0.5, d * 0.5);
    let faces = [
        (DVec3::NEG_Y, DVec3::X, hd, w),
        (DVec3::Y, DVec3::X, hd, w),
        (DVec3::NEG_X, DVec3::Y, hw, d),
        (DVec3::X, DVec3::Y, hw, d),
    ];
    let mut storey = 0;
    while (storey as f64) * STOREY < h - 0.5 {
        let z = storey as f64 * STOREY + SILL + PANE_H * 0.5;
        for (face, (out, wide, reach, run)) in faces.iter().enumerate() {
            let count = (run / PANE_PITCH).floor().max(1.0) as i32;
            for k in 0..count {
                let t = (k as f64 + 0.5) / count as f64 - 0.5;
                let along = *wide * (t * run);
                let at = along + *out * *reach;
                // The door stands in the middle of the south wall's
                // ground storey, and a pane there would be a window in a
                // doorway.
                if storey == 0 && face == 0 && along.length() < DOOR_W * 0.5 + PANE_W * 0.5 {
                    continue;
                }
                let dice = hash3(storey as i64, face as i64, k as i64, seed);
                m.pane(at + DVec3::Z * z, *out, *wide, PANE_H, dice);
            }
        }
        storey += 1;
    }
}

/// A round wall: `SIDES` boxes in a ring, which draws as a drum and
/// collides as one, with a gap for the door on the south side.
fn round(m: &mut Model, r: f64, h: f64, seed: u32) {
    let step = std::f64::consts::TAU / SIDES as f64;
    let wide = r * (step * 0.5).tan();
    m.solid(
        DVec3::new(0.0, 0.0, SLAB * 0.5),
        DVec3::new(r, r, SLAB * 0.5),
        0.0,
        CONCRETE,
    );
    for k in 0..SIDES {
        let a = k as f64 * step - std::f64::consts::FRAC_PI_2;
        let (s, c) = a.sin_cos();
        let out = DVec3::new(c, s, 0.0);
        // The one facing due south is the doorway, and it is a lintel.
        let door = k == 0;
        let lo = if door { DOOR_H } else { 0.0 };
        m.solid(
            out * (r - WALL * 0.5) + DVec3::Z * ((h + lo) * 0.5),
            DVec3::new(WALL * 0.5, wide, (h - lo) * 0.5),
            a,
            CONCRETE,
        );
        if door {
            continue;
        }
        let dice = hash3(k as i64, 0, 0, seed);
        let mut z = SILL + PANE_H * 0.5;
        while z < h - PANE_H {
            m.pane(
                out * r + DVec3::Z * z,
                out,
                DVec3::new(-s, c, 0.0),
                PANE_H,
                dice,
            );
            z += STOREY;
        }
    }
}

/// A flat roof: a slab and a parapet round it.
fn flat_roof(m: &mut Model, w: f64, d: f64, h: f64) {
    let (hw, hd) = (w * 0.5, d * 0.5);
    m.solid(
        DVec3::new(0.0, 0.0, h + SLAB * 0.5),
        DVec3::new(hw, hd, SLAB * 0.5),
        0.0,
        CONCRETE,
    );
    let z = h + SLAB + PARAPET * 0.5;
    let t = PARAPET_T * 0.5;
    for side in [-1.0, 1.0] {
        m.trim(
            DVec3::new(0.0, side * (hd - t), z),
            DVec3::new(hw, t, PARAPET * 0.5),
            0.0,
            PLATE,
        );
        m.trim(
            DVec3::new(side * (hw - t), 0.0, z),
            DVec3::new(t, hd - PARAPET_T, PARAPET * 0.5),
            0.0,
            PLATE,
        );
    }
}

/// A gable: two pitched faces to a ridge along the lot's east axis, and a
/// triangle closing each end.
fn gable(m: &mut Model, w: f64, d: f64, h: f64) {
    let (hw, hd) = (w * 0.5 + 0.3, d * 0.5 + 0.3);
    let ridge = h + d * 0.35;
    for side in [-1.0, 1.0] {
        let eave = DVec3::new(0.0, side * hd, h);
        let a = eave - DVec3::X * hw;
        let b = eave + DVec3::X * hw;
        let c = DVec3::new(hw, 0.0, ridge);
        let e = DVec3::new(-hw, 0.0, ridge);
        if side < 0.0 {
            m.quad(a, b, c, e, CONCRETE);
        } else {
            m.quad(b, a, e, c, CONCRETE);
        }
    }
    for side in [-1.0, 1.0] {
        let x = side * hw;
        let a = DVec3::new(x, -hd, h);
        let b = DVec3::new(x, hd, h);
        let c = DVec3::new(x, 0.0, ridge);
        if side < 0.0 {
            m.tri(a, c, b, CONCRETE);
        } else {
            m.tri(a, b, c, CONCRETE);
        }
    }
}

/// A barrel vault: an arc of quads over the lot's north axis, with the
/// ends left open, which is what a hangar looks like.
fn vault(m: &mut Model, w: f64, d: f64, h: f64) {
    let r = w * 0.5;
    let hd = d * 0.5;
    let at = |k: usize| {
        let a = std::f64::consts::PI * k as f64 / ARCH as f64;
        DVec3::new(-r * a.cos(), 0.0, h + r * a.sin() * 0.55)
    };
    for k in 0..ARCH {
        let (p, q) = (at(k), at(k + 1));
        m.quad(
            p - DVec3::Y * hd,
            q - DVec3::Y * hd,
            q + DVec3::Y * hd,
            p + DVec3::Y * hd,
            PLATE,
        );
    }
}

/// Plate pillars at the corners of a block, which is what stops a tower
/// reading as one poured shape.
fn pillars(m: &mut Model, w: f64, d: f64, h: f64) {
    let (hw, hd) = (w * 0.5, d * 0.5);
    let t = WALL * 0.6;
    for sx in [-1.0, 1.0] {
        for sy in [-1.0, 1.0] {
            m.solid(
                DVec3::new(sx * (hw - t), sy * (hd - t), h * 0.5),
                DVec3::new(t, t, h * 0.5),
                0.0,
                PLATE,
            );
        }
    }
}

/// A flat panel laid in a street's own plane: `lo` and `hi` are its
/// corners east and north, `up` how high it stands over the ground.
fn panel(m: &mut Model, lo: DVec2, hi: DVec2, up: f64, material: u8) {
    m.quad(
        DVec3::new(lo.x, lo.y, up),
        DVec3::new(hi.x, lo.y, up),
        DVec3::new(hi.x, hi.y, up),
        DVec3::new(lo.x, hi.y, up),
        material,
    );
}

/// One slab of raised PAVEMENT, its top `KERB` over the carriageway and
/// its underside buried.
///
/// It is a `solid` and the carriageway is not, and the difference is
/// what a body DOES with each. Five centimetres of paving is under
/// anything and a walker stands on the ground through it; twelve is
/// ankle deep, so a pavement a body could not stand on would be a
/// pavement a body stood IN. It is well under the walker's own sixty
/// centimetre step, so he steps up onto it rather than being stopped by
/// it, and `resolve` never sees it at all because its ring of points
/// starts at the step.
fn kerb(m: &mut Model, lo: DVec2, hi: DVec2) {
    let top = LIFT + KERB;
    let mid = (lo + hi) * 0.5;
    let half = (hi - lo) * 0.5;
    m.solid(
        DVec3::new(mid.x, mid.y, (top - BURY) * 0.5),
        DVec3::new(half.x, half.y, (top + BURY) * 0.5),
        0.0,
        CONCRETE,
    );
}

/// A street's MARKINGS: one dash of the centreline a piece, and a solid
/// line down each side of the carriageway.
///
/// Paint rather than geometry standing on the road: a marking is a quad
/// four millimetres over the tarmac in the `PAINT` material, which is
/// the street's own set brightened, so it costs no texture, no second
/// draw and no shader of its own.
fn markings(m: &mut Model, long: f64, northerly: bool) {
    let up = LIFT + PAINT_UP;
    let half = long * 0.5;
    let (dash, w) = (half * DASH, PAINT_W * 0.5);
    let edge = LANE - EDGE_IN - w;
    let mut stripe = |a0: f64, a1: f64, c: f64| {
        let (lo, hi) = if northerly {
            (DVec2::new(c - w, a0), DVec2::new(c + w, a1))
        } else {
            (DVec2::new(a0, c - w), DVec2::new(a1, c + w))
        };
        panel(m, lo, hi, up, PAINT);
    };
    stripe(-dash, dash, 0.0);
    stripe(-half, half, edge);
    stripe(-half, half, -edge);
}

/// A RUN of street: two lanes of carriageway, a raised pavement either
/// side of them, and the markings between.
fn run(long: f64, northerly: bool) -> Model {
    let mut m = Model::new();
    let half = long * 0.5;
    // ALONG the run and ACROSS it, turned into the frame's own east and
    // north, so one body of arithmetic lays a street whichever way it
    // lies. Both bounds stay in order, so every panel still winds up.
    let mut band = |a0: f64, c0: f64, a1: f64, c1: f64, road: bool| {
        let (lo, hi) = if northerly {
            (DVec2::new(c0, a0), DVec2::new(c1, a1))
        } else {
            (DVec2::new(a0, c0), DVec2::new(a1, c1))
        };
        if road {
            panel(&mut m, lo, hi, LIFT, STREET);
        } else {
            kerb(&mut m, lo, hi);
        }
    };
    band(-half, -LANE, half, LANE, true);
    band(-half, LANE, half, LANE + WALK, false);
    band(-half, -LANE - WALK, half, -LANE, false);
    markings(&mut m, long, northerly);
    m
}

/// A CROSSING: the square where two runs meet, as three bands each way.
///
/// The middle cell is always carriageway and the four corners are always
/// pavement; each of the four bands between is carriageway when the arm
/// it lies on is there and pavement when it is not. So a crossroads is a
/// plus of tarmac with four kerbed corners, a bend's kerb turns the
/// corner as an L, and a DEAD END closes with a pavement across it
/// rather than stopping mid cell with an open edge.
fn crossing(arms: u8) -> Model {
    let mut m = Model::new();
    for (bx, &(x0, x1)) in BANDS.iter().enumerate() {
        for (bz, &(z0, z1)) in BANDS.iter().enumerate() {
            let (lo, hi) = (DVec2::new(x0, z0), DVec2::new(x1, z1));
            if paved(arms, bx, bz) {
                panel(&mut m, lo, hi, LIFT, STREET);
            } else {
                kerb(&mut m, lo, hi);
            }
        }
    }
    m
}

/// A piece of street: a straight RUN or the CROSSING at the end of one,
/// laid `LIFT` over the levelled ground.
///
/// The carriageway collides with nothing and the pavement does. The
/// site under a town is levelled, so the ground there is a plane and
/// dual contouring holds a plane to two millimetres; five centimetres
/// of paving clears that by twenty five times and is under anything, so
/// a body walks the ground through it. A twelve centimetre kerb is not:
/// it is what makes a pavement read as one, so it is a box a body
/// stands ON, and it is well under the walker's sixty centimetre step,
/// so he steps up rather than being stopped.
pub fn street(piece: &Piece) -> Model {
    if piece.run() {
        let long = if piece.northerly() { piece.d } else { piece.w };
        run(long, piece.northerly())
    } else {
        crossing(piece.arms)
    }
}

/// Everything a town has standing on it: ONE mesh in the town's own frame,
/// the boxes a body is stopped by in the WORLD frame, and its lamps.
///
/// One mesh a town rather than one a building, because a town is eighty
/// metres across and a building is ten: in the town's frame an `f32`
/// holds a micron, which is the chunk local rule at a town's scale, and
/// eight towns are eight draws rather than eight thousand.
#[derive(Clone, Debug, Default)]
pub struct Fabric {
    pub mesh: DcMesh,
    pub blocks: Vec<Block>,
    pub lamps: Vec<(DVec3, f64)>,
    pub buildings: usize,
    pub pieces: usize,
}

/// The fabric of one town on a planet of `radius`: every lot built to its
/// own kind and every piece of street laid, each on its OWN patch of the
/// sphere (`lot_frame`), so nothing long enough for the ground to curve
/// under it is placed in one piece.
pub fn fabric(town: &Town, radius: f64, seed: u32) -> Fabric {
    fabric_with(town, radius, |lot| {
        building(
            lot.kind,
            crate::town::BLOCK,
            crate::town::BLOCK,
            lot.storeys,
            seed ^ lot.id,
        )
    })
}

/// Assemble a town from static models supplied by an asset library. Drawing,
/// collision boxes, and lamps pass through the very same lot transform.
pub fn fabric_with(
    town: &Town,
    radius: f64,
    mut model: impl FnMut(&crate::town::Lot) -> Model,
) -> Fabric {
    let mut out = Fabric::default();
    let middle = lot_frame(radius, town, 0.0, 0.0);
    for lot in &town.lots {
        let frame = lot_frame(radius, town, lot.x, lot.z);
        let m = model(lot);
        weld(&mut out, &m, &frame, &middle);
        out.buildings += 1;
    }
    for piece in &town.pieces {
        let frame = lot_frame(radius, town, piece.x, piece.z);
        let m = street(piece);
        weld(&mut out, &m, &frame, &middle);
        out.pieces += 1;
    }
    out
}

/// One model welded into a town's fabric: its triangles carried from its
/// own frame into the town's, in `f64` and then cast, and its boxes and
/// its lamps carried into the world.
fn weld(out: &mut Fabric, m: &Model, frame: &Frame, middle: &Frame) {
    let base = out.mesh.positions.len() as u32;
    // A direction from one frame to the other: out through the lot's axes
    // and back in through the town's, which is a rotation and never the
    // position's translation.
    let turn = |v: [f32; 3]| {
        let w = frame.east * v[0] as f64 + frame.north * v[1] as f64 + frame.dir * v[2] as f64;
        DVec3::new(w.dot(middle.east), w.dot(middle.north), w.dot(middle.dir))
    };
    for (p, n) in m.mesh.positions.iter().zip(&m.mesh.normals) {
        let here = middle.local(frame.world(DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64)));
        out.mesh.positions.push(here.as_vec3().to_array());
        out.mesh
            .normals
            .push(turn(*n).normalize_or(DVec3::Z).as_vec3().to_array());
        out.mesh.levels.push(0);
    }
    out.mesh
        .indices
        .extend(m.mesh.indices.iter().map(|i| i + base));
    out.mesh.materials.extend_from_slice(&m.mesh.materials);
    out.blocks.extend(m.blocks(frame));
    out.lamps.extend(m.lights(frame));
}

#[cfg(test)]
mod tests;
