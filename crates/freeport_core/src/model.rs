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
use crate::field::{
    hash3, Block, BRICK, CONCRETE, CURTAIN, GLASS, LAMP, LIT, MARBLE, PLATE, STONE, VINYL, WOOD,
};
use crate::town::{lot_frame, Frame, Town};
use glam::DVec3;

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

    /// Repaint every triangle and every box of one material as another.
    ///
    /// What it is FOR is the baked building library: a bake out of
    /// Blender is authored in one material (`tools/bake_buildings.py`
    /// writes concrete), and which trade a building is actually built in
    /// is a fact about the LOT rather than about the variant, so thirteen
    /// variants would have to be baked five times over to carry it. One
    /// table of trades (`Kind::skin`) and one repaint on the way out of
    /// the library is the whole of it.
    ///
    /// It repaints the MESH and the SOLIDS together, which is this
    /// project's own rule that a wall is one oriented box that is drawn
    /// and collided from one set of numbers: a repaint that moved only
    /// one of them would be a wall that looked like brick and answered
    /// concrete to whatever asks what a body is standing on.
    pub fn reskin(&mut self, from: u8, to: u8) {
        for s in &mut self.solids {
            if s.material == from {
                s.material = to;
            }
        }
        for m in &mut self.mesh.materials {
            if *m == from {
                *m = to;
            }
        }
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

    /// Another model set down IN this one, at `at` and turned `yaw`
    /// radians about the up: its triangles, its boxes and its lamps
    /// carried over together, so what is drawn and what stops a body
    /// move as one thing. What a gas station is built of.
    pub fn place(&mut self, other: &Model, at: DVec3, yaw: f64) {
        let (sn, cs) = yaw.sin_cos();
        let turn = |v: DVec3| DVec3::new(cs * v.x - sn * v.y, sn * v.x + cs * v.y, v.z);
        let base = self.mesh.positions.len() as u32;
        for (p, n) in other.mesh.positions.iter().zip(&other.mesh.normals) {
            let p = turn(DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64)) + at;
            let n = turn(DVec3::new(n[0] as f64, n[1] as f64, n[2] as f64));
            self.mesh.positions.push(p.as_vec3().to_array());
            self.mesh.normals.push(n.as_vec3().to_array());
            self.mesh.levels.push(0);
        }
        self.mesh
            .indices
            .extend(other.mesh.indices.iter().map(|i| i + base));
        self.mesh.materials.extend_from_slice(&other.mesh.materials);
        self.solids.extend(other.solids.iter().map(|s| Solid {
            centre: turn(s.centre) + at,
            half: s.half,
            yaw: s.yaw + yaw,
            material: s.material,
        }));
        self.lamps.extend(other.lamps.iter().map(|&l| turn(l) + at));
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
    /// One to three storeys under a flat roof, in a trade's own skin:
    /// what a town's downtown and a city's high street are made of, on
    /// one lot or on four.
    Shop,
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
            Kind::Shop => "shop",
        }
    }

    /// Whether the Blender library carries a bake of this kind. A shop
    /// is built parametrically, because it stands on ONE lot or on FOUR
    /// and a bake is one size; the library refuses a bake it has not
    /// got only of the kinds that claim one.
    pub fn baked(self) -> bool {
        self != Kind::Shop
    }

    /// How much of its own LOT a kind's walls cover, as a share of the
    /// lot's footprint across.
    ///
    /// A lot is `town::LOT` and the next lot or the street's own inner
    /// kerb stands exactly `LOT / 2` from its middle, so this is the one
    /// number that says how far a building may be set back or jittered
    /// without putting a wall on a pavement or in the neighbour: the
    /// room a lot has is `LOT * (1 - covers) / 2` either way, and
    /// `town::plot` bounds every offset by it. A wall is one oriented
    /// box that is DRAWN and COLLIDED, so a building standing in a
    /// street is a body walking into a wall in the middle of the road,
    /// which is what the owner photographed.
    ///
    /// It is ONE for every kind today, and that is a fact about the
    /// LIBRARY rather than a knob nobody turned:
    /// `assets/config/buildings.json` bakes every one of the thirteen
    /// variants at the full lot and `Library` refuses a bake wider than
    /// this, so there is no room for a setback to be in. The day a house
    /// is baked at six metres this is where that is said, and the
    /// suburb's own setback appears with it and with nothing else
    /// changed.
    pub fn covers(self) -> f64 {
        match self {
            Kind::Block
            | Kind::House
            | Kind::Bungalow
            | Kind::Tower
            | Kind::Hangar
            | Kind::Shop => 1.0,
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
            Kind::Shop => (1, 3),
        }
    }

    /// What its walls are made of, off its own seed: the owner's own
    /// list of trades, a house out of wood, red brick or vinyl and an
    /// office out of red brick, concrete, marble, glass or stone blocks.
    /// The glass is a CURTAIN WALL and not the pane material, for the
    /// reason `field::CURTAIN` writes down: a whole tower of the pane's
    /// own flat dark mirror vanished against the sky.
    ///
    /// A TABLE a kind and a hash into it, which is this project's own
    /// "open for extension" rule: a new skin is a row and a set, not a
    /// branch in a builder. A hangar is neither a house nor an office
    /// and keeps the concrete it was built in, because what the owner
    /// asked about is the two a town is made of.
    ///
    /// It is the SEED and never the lot's place, so a building is the
    /// same building wherever the eye happens to be when the town is
    /// raised: `city::stream` builds a town as the eye comes near it and
    /// drops it again, and a skin picked off anything that streams would
    /// change colour every time you drove back into town.
    pub fn skin(self, seed: u32) -> u8 {
        let trades: &[u8] = match self {
            Kind::House | Kind::Bungalow => &[WOOD, BRICK, VINYL],
            Kind::Block | Kind::Tower => &[BRICK, CONCRETE, MARBLE, CURTAIN, STONE],
            Kind::Shop => &[BRICK, CONCRETE, STONE, VINYL],
            Kind::Hangar => &[CONCRETE],
        };
        let pick = hash3(seed as i64, 0x5C11, 0x2E, 0x51DE);
        trades[((pick * trades.len() as f64) as usize).min(trades.len() - 1)]
    }

    /// Every kind, so a harness can build one of each.
    pub fn all() -> [Kind; 6] {
        [
            Kind::Block,
            Kind::House,
            Kind::Bungalow,
            Kind::Tower,
            Kind::Hangar,
            Kind::Shop,
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
    // What this one is BUILT of. The walls and a pitched roof wear it;
    // a flat roof's slab, a floor and the pillars stay what they were,
    // because a flat roof IS poured concrete and a pillar IS steel
    // whatever the skin hung off it is.
    let skin = kind.skin(seed);
    match kind {
        Kind::Tower => round(&mut m, w.min(d) * 0.5, h, seed, skin),
        _ => shell(&mut m, w, d, h, seed, skin),
    }
    match kind {
        Kind::House => gable(&mut m, w, d, h, skin),
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

/// Four walls of `skin`, a concrete floor, a doorway in the south wall
/// and panes up the rest.
fn shell(m: &mut Model, w: f64, d: f64, h: f64, seed: u32, skin: u8) {
    let t = WALL * 0.5;
    let (hw, hd) = (w * 0.5, d * 0.5);
    // The FLOOR is poured concrete whatever the walls are, which is
    // what a floor is: a slab on the ground.
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
        skin,
    );
    for side in [-1.0, 1.0] {
        m.solid(
            DVec3::new(side * (hw - t), 0.0, h * 0.5),
            DVec3::new(t, hd - WALL, h * 0.5),
            0.0,
            skin,
        );
    }
    // The south wall is the doorway's: two piers and a lintel over them.
    let pier = (w - DOOR_W) * 0.25;
    for side in [-1.0, 1.0] {
        m.solid(
            DVec3::new(side * (DOOR_W * 0.5 + pier), -(hd - t), h * 0.5),
            DVec3::new(pier, t, h * 0.5),
            0.0,
            skin,
        );
    }
    m.solid(
        DVec3::new(0.0, -(hd - t), (DOOR_H + h) * 0.5),
        DVec3::new(DOOR_W * 0.5, t, (h - DOOR_H) * 0.5),
        0.0,
        skin,
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
fn round(m: &mut Model, r: f64, h: f64, seed: u32, skin: u8) {
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
            skin,
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
fn gable(m: &mut Model, w: f64, d: f64, h: f64, skin: u8) {
    let (hw, hd) = (w * 0.5 + 0.3, d * 0.5 + 0.3);
    let ridge = h + d * 0.35;
    for side in [-1.0, 1.0] {
        let eave = DVec3::new(0.0, side * hd, h);
        let a = eave - DVec3::X * hw;
        let b = eave + DVec3::X * hw;
        let c = DVec3::new(hw, 0.0, ridge);
        let e = DVec3::new(-hw, 0.0, ridge);
        if side < 0.0 {
            m.quad(a, b, c, e, skin);
        } else {
            m.quad(b, a, e, c, skin);
        }
    }
    for side in [-1.0, 1.0] {
        let x = side * hw;
        let a = DVec3::new(x, -hd, h);
        let b = DVec3::new(x, hd, h);
        let c = DVec3::new(x, 0.0, ridge);
        if side < 0.0 {
            m.tri(a, c, b, skin);
        } else {
            m.tri(a, b, c, skin);
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
        let w = lot.kind.covers() * lot.w;
        building(lot.kind, w, w, lot.storeys, seed ^ lot.id)
    })
}

/// Assemble a town from static models supplied by an asset library. Drawing,
/// collision boxes, and lamps pass through the very same lot transform.
pub fn fabric_with(
    town: &Town,
    radius: f64,
    model: impl FnMut(&crate::town::Lot) -> Model,
) -> Fabric {
    let every = Part {
        lots: &(0..town.lots.len()).collect::<Vec<_>>(),
        pieces: &(0..town.pieces.len()).collect::<Vec<_>>(),
        mesh: true,
        solids: true,
    };
    fabric_part(town, radius, &every, model, street)
}

/// WHICH of a town a fabric is built of, and which half of it: the lots
/// and the pieces of street by their places in the town's own lists, and
/// whether the triangles, the boxes and lamps, or both are wanted.
///
/// A town is built a TILE at a time, because a city of thousands of
/// buildings is drawn at the detail each part of it is worth from where
/// the eye is: a tile far off is drawn and never walked into, and a tile
/// a body can reach is walked into whatever it is drawn as. The two
/// halves are asked for apart, so neither pays for the other.
pub struct Part<'a> {
    pub lots: &'a [usize],
    pub pieces: &'a [usize],
    pub mesh: bool,
    pub solids: bool,
}

/// Assemble the part of a town `part` names, each lot drawn by `model`
/// and each piece of street by `piece`, through the very same lot
/// transform the whole town is.
pub fn fabric_part(
    town: &Town,
    radius: f64,
    part: &Part,
    mut model: impl FnMut(&crate::town::Lot) -> Model,
    mut piece: impl FnMut(&crate::town::Piece) -> Model,
) -> Fabric {
    let mut out = Fabric::default();
    let middle = lot_frame(radius, town, 0.0, 0.0);
    for lot in part.lots.iter().filter_map(|&k| town.lots.get(k)) {
        // Turned so the door faces the street the lot fronts.
        let frame = lot_frame(radius, town, lot.x, lot.z).turned(lot.yaw);
        weld(&mut out, &model(lot), &frame, &middle, part);
        out.buildings += 1;
    }
    for p in part.pieces.iter().filter_map(|&k| town.pieces.get(k)) {
        let frame = lot_frame(radius, town, p.x, p.z);
        weld(&mut out, &piece(p), &frame, &middle, part);
        out.pieces += 1;
    }
    out
}

/// One model welded into a town's fabric: its triangles carried from its
/// own frame into the town's, in `f64` and then cast, and its boxes and
/// its lamps carried into the world, as far as `part` asks for each.
fn weld(out: &mut Fabric, m: &Model, frame: &Frame, middle: &Frame, part: &Part) {
    if part.solids {
        out.blocks.extend(m.blocks(frame));
        out.lamps.extend(m.lights(frame));
    }
    if !part.mesh {
        return;
    }
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
}

/// A piece of STREET as a model: a run, a crossing or the square.
mod street;
pub use street::{street, street_graded};

/// A building as a solid block, for the ranges its detail is not worth.
mod massing;
pub use massing::massing;

#[cfg(test)]
mod tests;
