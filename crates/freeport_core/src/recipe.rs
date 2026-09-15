//! A building is a list of brushes in the field: the kit, `kit.js` line for
//! line.
//!
//! A recipe (`assets/buildings/*.json`, the schema in that folder's README)
//! is a list of signed distance brushes in the building's own frame, x
//! east, y north, z up, metres, origin at the middle of the footprint on
//! the ground. Each is ADDED (a union, or a smooth one with `blend`) or
//! CUT, in list order, so a door cut after a wall goes through the wall and
//! a flight added after a room stands in it; `each` repeats a brush a
//! storey at a time and `alternate` mirrors it through the centre on odd
//! storeys; a window is an OPENING onto the room. Nothing here knows about
//! a planet: a point comes in in the building's frame and a density and a
//! material go back, which is what lets the same list be evaluated by the
//! mesher and by the walker.
//!
//! One rule the mockup did not have: a window whose opening would meet a
//! flight is DROPPED, so the flight's slab never runs across a pane and no
//! window is cut where a ramp climbs the wall.

use crate::field::{CONCRETE, GLASS, LAMP, LIT, PLATE, STREET, TERRAIN};
use crate::json::{self, Value};
use glam::DVec3;

/// Whether a brush adds rock or takes it away.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Op {
    Add,
    Cut,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shape {
    Box,
    Cyl,
    Sphere,
    Stairs,
}

/// How a brush repeats over the storeys.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Each {
    Once,
    Storey,
    Upper,
    Flight,
    Top,
}

/// A cylinder's axis, in the building's frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Axis {
    Up,
    North,
    East,
}

/// One brush as the recipe wrote it.
#[derive(Clone, Debug)]
pub struct Src {
    pub op: Op,
    pub shape: Shape,
    pub mat: u8,
    pub at: DVec3,
    pub size: DVec3,
    pub rot: f64,
    pub tilt: f64,
    pub pitch: f64,
    pub clip: Option<[f64; 2]>,
    pub blend: f64,
    pub room: bool,
    pub each: Each,
    pub alternate: bool,
    pub axis: Axis,
    pub steps: u32,
    pub south: bool,
}

/// A window as written: the face it is in, and whether it holds a pane.
#[derive(Clone, Debug)]
struct Window {
    face: char,
    at: DVec3,
    size: DVec3,
    each: Each,
    alternate: bool,
    pane: bool,
    lit: Option<bool>,
}

#[derive(Clone, Debug)]
enum Item {
    Brush(Src),
    Window(Window),
}

/// A recipe, read from its JSON.
#[derive(Clone, Debug)]
pub struct Recipe {
    pub name: String,
    pub footprint: [f64; 2],
    pub storey: f64,
    pub storeys: [u32; 2],
    pub door: f64,
    items: Vec<Item>,
}

/// A brush compiled for one storey: half sizes, the turns as sines and
/// cosines, and a box round it.
#[derive(Clone, Debug)]
pub struct Brush {
    pub op: Op,
    pub shape: Shape,
    pub mat: u8,
    pub c: DVec3,
    pub h: DVec3,
    pub rot: Option<(f64, f64)>,
    pub tilt: Option<(f64, f64)>,
    pub pitch: Option<(f64, f64)>,
    pub clip: Option<[f64; 2]>,
    pub blend: f64,
    /// The room this cut names, or -1.
    pub room: i32,
    pub axis: Axis,
    pub steps: u32,
    pub south: bool,
    pub lo: DVec3,
    pub hi: DVec3,
    /// A window's opening, which the flight rule may drop.
    pub window: bool,
}

/// A building compiled for a count of storeys: what the field samples.
#[derive(Clone, Debug, Default)]
pub struct Building {
    pub brushes: Vec<Brush>,
    pub rooms: usize,
    /// Each lamp's place and reach.
    pub lamps: Vec<(DVec3, f64)>,
    pub lo: DVec3,
    pub hi: DVec3,
    pub storeys: u32,
    pub storey: f64,
    pub door: f64,
    pub footprint: [f64; 2],
    pub warnings: Vec<String>,
    /// Windows dropped for meeting a flight.
    pub dropped: usize,
}

/// A sample of the field in the building's frame: what is there and what
/// it is made of.
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub d: f64,
    pub mat: u8,
    pub room: i32,
    /// Shaded smooth: a curved brush or a blended one.
    pub curved: bool,
}

const RAD: f64 = std::f64::consts::PI / 180.0;
/// Brushes thinner than this can hold both faces in one cell.
const THIN: f64 = 0.4;
/// How far a flight's box is grown when a window is tested against it.
const FLIGHT_CLEAR: f64 = 0.1;

/// The material a recipe names.
pub fn material(name: &str) -> Option<u8> {
    Some(match name {
        "terrain" => TERRAIN,
        "concrete" => CONCRETE,
        "plate" => PLATE,
        "glass" => GLASS,
        "lamp" => LAMP,
        "lit" => LIT,
        "street" => STREET,
        _ => return None,
    })
}

/// Signed distance to a box of half sizes `h`, centred on the origin.
pub fn sd_box(q: DVec3, h: DVec3) -> f64 {
    let d = q.abs() - h;
    d.max(DVec3::ZERO).length() + d.x.max(d.y).max(d.z).min(0.0)
}

/// A capped cylinder: `rad` the distance from the axis, `along` the
/// coordinate on it.
fn sd_cyl(rad: f64, along: f64, r: f64, h: f64) -> f64 {
    let dx = rad - r;
    let dy = along.abs() - h;
    dx.max(dy).min(0.0) + (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt()
}

/// A flight of steps rising along +y from the low end, only the step
/// under the point and its neighbours measured.
fn sd_stairs(q: DVec3, h: DVec3, steps: u32) -> f64 {
    let a = q.y + h.y;
    let up = q.z + h.z;
    let tread = 2.0 * h.y / steps as f64;
    let riser = 2.0 * h.z / steps as f64;
    let j = ((a / tread).floor() as i64).clamp(0, steps as i64 - 1);
    let mut d = f64::INFINITY;
    for jj in (j - 1).max(0)..=(j + 1).min(steps as i64 - 1) {
        let top = (jj + 1) as f64 * riser;
        let local = DVec3::new(a - (jj as f64 + 0.5) * tread, q.x, up - top / 2.0);
        d = d.min(sd_box(local, DVec3::new(tread / 2.0, h.x, top / 2.0)));
    }
    d
}

/// Smooth maximum of two densities: the union with a fillet of radius `k`.
pub fn smax(a: f64, b: f64, k: f64) -> f64 {
    let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
    a + (b - a) * h + k * h * (1.0 - h)
}

impl Brush {
    /// The signed distance of the brush at a point in the building frame.
    pub fn sd(&self, p: DVec3) -> f64 {
        let mut q = p - self.c;
        if let Some((c, s)) = self.rot {
            q = DVec3::new(c * q.x + s * q.y, -s * q.x + c * q.y, q.z);
        }
        if let Some((c, s)) = self.tilt {
            q = DVec3::new(c * q.x + s * q.z, q.y, -s * q.x + c * q.z);
        }
        if let Some((c, s)) = self.pitch {
            q = DVec3::new(q.x, c * q.y + s * q.z, -s * q.y + c * q.z);
        }
        let h = self.h;
        let mut d = match self.shape {
            Shape::Box => sd_box(q, h),
            Shape::Sphere => q.length() - h.x,
            Shape::Cyl => match self.axis {
                Axis::North => sd_cyl(q.x.hypot(q.z), q.y, h.x.min(h.z), h.y),
                Axis::East => sd_cyl(q.y.hypot(q.z), q.x, h.y.min(h.z), h.x),
                Axis::Up => sd_cyl(q.x.hypot(q.y), q.z, h.x.min(h.y), h.z),
            },
            Shape::Stairs => sd_stairs(
                DVec3::new(q.x, if self.south { -q.y } else { q.y }, q.z),
                h,
                self.steps,
            ),
        };
        if let Some([lo, hi]) = self.clip {
            d = d.max(lo - p.z).max(p.z - hi);
        }
        d
    }

    fn holds(&self, p: DVec3) -> bool {
        p.cmpge(self.lo).all() && p.cmple(self.hi).all()
    }

    fn meets(&self, lo: DVec3, hi: DVec3) -> bool {
        self.lo.cmple(hi).all() && self.hi.cmpge(lo).all()
    }
}

impl Building {
    /// The field at a point in the building's frame: `s` comes in carrying
    /// the ground's density and material and leaves carrying the building's.
    /// An add deeper than what is there wins the point and its material, a
    /// cut nearer than what is there takes it, and a cut that is a room
    /// names the room.
    pub fn sample(&self, p: DVec3, mut s: Sample) -> Sample {
        for b in &self.brushes {
            if !b.holds(p) {
                continue;
            }
            let sd = b.sd(p);
            match b.op {
                Op::Add => {
                    let v = -sd;
                    if b.blend > 0.0 {
                        let nd = smax(s.d, v, b.blend);
                        if v > s.d {
                            s.mat = b.mat;
                            s.curved = true;
                        }
                        s.d = nd;
                    } else if v > s.d {
                        s.d = v;
                        s.mat = b.mat;
                        s.curved = !matches!(b.shape, Shape::Box | Shape::Stairs);
                    }
                }
                Op::Cut => {
                    if sd < s.d {
                        s.d = sd;
                        if b.room >= 0 && sd < 0.0 {
                            s.room = b.room;
                        }
                    }
                }
            }
        }
        s
    }

    /// The building from afar: its added brushes no thinner than `cell`,
    /// nothing cut, so a house is a block and a tower a drum on a lattice
    /// that could not hold its walls.
    pub fn massing(&self, p: DVec3, mut s: Sample, cell: f64) -> Sample {
        for b in &self.brushes {
            if b.op != Op::Add || b.h.min_element() * 2.0 < cell || !b.holds(p) {
                continue;
            }
            let v = -b.sd(p);
            if v > s.d {
                s.d = v;
                s.mat = b.mat;
            }
        }
        s
    }
}

/// How far a box of half sizes `h`, turned as `Brush::sd` turns it back,
/// reaches along each axis: its eight corners through the turns. A sphere
/// of the box's diagonal was the first bound, and it dropped a ground
/// floor window for a ramp a storey up.
fn oriented_extent(
    h: DVec3,
    rot: Option<(f64, f64)>,
    tilt: Option<(f64, f64)>,
    pitch: Option<(f64, f64)>,
) -> DVec3 {
    let mut ext = DVec3::ZERO;
    for corner in 0..8 {
        let sx = if corner & 1 != 0 { 1.0 } else { -1.0 };
        let sy = if corner & 2 != 0 { 1.0 } else { -1.0 };
        let sz = if corner & 4 != 0 { 1.0 } else { -1.0 };
        let mut q = DVec3::new(h.x * sx, h.y * sy, h.z * sz);
        // The inverse of each turn in `sd`, applied in the opposite order.
        if let Some((c, s)) = pitch {
            q = DVec3::new(q.x, c * q.y - s * q.z, s * q.y + c * q.z);
        }
        if let Some((c, s)) = tilt {
            q = DVec3::new(c * q.x - s * q.z, q.y, s * q.x + c * q.z);
        }
        if let Some((c, s)) = rot {
            q = DVec3::new(c * q.x - s * q.y, s * q.x + c * q.y, q.z);
        }
        ext = ext.max(q.abs());
    }
    ext
}

fn compile_one(
    src: &Src,
    dz: f64,
    parity: bool,
    rooms: &mut usize,
    warnings: &mut Vec<String>,
    window: bool,
) -> Brush {
    let mut at = src.at + DVec3::new(0.0, 0.0, dz);
    let mut rot = src.rot;
    if src.alternate && parity {
        at.x = -at.x;
        at.y = -at.y;
        rot += 180.0;
    }
    let h = src.size * 0.5;
    let turn = |deg: f64| (deg != 0.0).then(|| ((deg * RAD).cos(), (deg * RAD).sin()));
    let rot = (rot.rem_euclid(360.0) != 0.0).then(|| ((rot * RAD).cos(), (rot * RAD).sin()));
    let tilt = turn(src.tilt);
    let pitch = turn(src.pitch);
    let ext = if rot.is_some() || tilt.is_some() || pitch.is_some() {
        oriented_extent(h, rot, tilt, pitch)
    } else {
        h
    };
    let m = src.blend + 0.05;
    let room = if src.room && src.op == Op::Cut {
        *rooms += 1;
        *rooms as i32 - 1
    } else {
        -1
    };
    if src.op == Op::Add && src.shape != Shape::Stairs && src.size.min_element() < THIN {
        let w = format!(
            "{:?} thinner than {THIN} m can hold both its faces in one cell",
            src.shape
        );
        if !warnings.contains(&w) {
            warnings.push(w);
        }
    }
    Brush {
        op: src.op,
        shape: src.shape,
        mat: if src.op == Op::Cut { TERRAIN } else { src.mat },
        c: at,
        h,
        rot,
        tilt,
        pitch,
        clip: src.clip,
        blend: src.blend,
        room,
        axis: src.axis,
        steps: src.steps,
        south: src.south,
        lo: at - ext - DVec3::splat(m),
        hi: at + ext + DVec3::splat(m),
        window,
    }
}

/// A window is an opening onto the room: the hole through the wall and
/// nothing in it, unless the recipe asks for a pane, dark or glowing.
fn expand_window(w: &Window, index: usize, seed: u32) -> Vec<(Src, bool)> {
    let side = w.face == 'e' || w.face == 'w';
    let size = if side {
        DVec3::new(w.size.y, w.size.x, w.size.z)
    } else {
        w.size
    };
    let base = Src {
        op: Op::Cut,
        shape: Shape::Box,
        mat: TERRAIN,
        at: w.at,
        size,
        rot: 0.0,
        tilt: 0.0,
        pitch: 0.0,
        clip: None,
        blend: 0.0,
        room: false,
        each: w.each,
        alternate: w.alternate,
        axis: Axis::Up,
        steps: 10,
        south: false,
    };
    let mut out = vec![(base.clone(), true)];
    if w.pane {
        let pane = if side {
            DVec3::new(THIN, w.size.x, w.size.z)
        } else {
            DVec3::new(w.size.x, THIN, w.size.z)
        };
        let lit = w
            .lit
            .unwrap_or(crate::field::hash3(index as i64, 7, 0, seed) > 0.45);
        out.push((
            Src {
                op: Op::Add,
                mat: if lit { LIT } else { GLASS },
                size: pane,
                ..base
            },
            true,
        ));
    }
    out
}

impl Recipe {
    /// Read a recipe from its JSON.
    pub fn parse(text: &str) -> Result<Recipe, String> {
        let v = json::parse(text)?;
        let name = v
            .get("name")
            .and_then(Value::str)
            .unwrap_or("recipe")
            .to_string();
        let fp = v
            .get("footprint")
            .and_then(Value::nums)
            .filter(|f| f.len() == 2)
            .ok_or("a recipe needs a footprint of two numbers")?;
        let storeys = v
            .get("storeys")
            .and_then(Value::nums)
            .filter(|s| s.len() == 2)
            .map(|s| [s[0] as u32, s[1] as u32])
            .unwrap_or([1, 1]);
        let mut items = Vec::new();
        for b in v.get("brushes").map(Value::items).unwrap_or(&[]) {
            items.push(read_item(b)?);
        }
        Ok(Recipe {
            name,
            footprint: [fp[0], fp[1]],
            storey: v.get("storey").and_then(Value::num).unwrap_or(3.0),
            storeys,
            door: v.get("door").and_then(Value::num).unwrap_or(0.0),
            items,
        })
    }

    /// Compile the recipe for a building of `storeys` storeys.
    pub fn compile(&self, storeys: u32, seed: u32) -> Building {
        let t = self.storey;
        let s = storeys.clamp(1, 64);
        let mut list: Vec<(Src, bool)> = Vec::new();
        let mut wi = 0;
        for item in &self.items {
            match item {
                Item::Brush(src) => list.push((src.clone(), false)),
                Item::Window(w) => {
                    list.extend(expand_window(w, wi, seed));
                    wi += 1;
                }
            }
        }
        let mut b = Building {
            storeys: s,
            storey: t,
            door: self.door,
            footprint: self.footprint,
            ..Default::default()
        };
        let mut rooms = 0;
        for (src, window) in &list {
            let reps: Vec<(f64, bool)> = match src.each {
                Each::Storey => (0..s).map(|i| (i as f64 * t, i % 2 == 1)).collect(),
                Each::Upper => (1..s).map(|f| (f as f64 * t, (f - 1) % 2 == 1)).collect(),
                Each::Flight => (0..s.saturating_sub(1))
                    .map(|i| (i as f64 * t, i % 2 == 1))
                    .collect(),
                Each::Top => vec![(s as f64 * t, s % 2 == 1)],
                Each::Once => vec![(0.0, false)],
            };
            for (dz, parity) in reps {
                let brush = compile_one(src, dz, parity, &mut rooms, &mut b.warnings, *window);
                if brush.op == Op::Add && brush.mat == LAMP {
                    b.lamps.push((
                        brush.c,
                        self.footprint[0].max(self.footprint[1]) * 0.8 + 1.5,
                    ));
                }
                b.brushes.push(brush);
            }
        }
        b.rooms = rooms;
        b.drop_windows_on_flights();
        let mut lo = DVec3::splat(f64::INFINITY);
        let mut hi = DVec3::splat(f64::NEG_INFINITY);
        for br in b.brushes.iter().filter(|br| br.op == Op::Add) {
            lo = lo.min(br.lo);
            hi = hi.max(br.hi);
        }
        b.lo = lo;
        b.hi = hi;
        b
    }
}

impl Src {
    /// A brush of one shape with no storeys: what an edit or a piece of
    /// street is. `at` and `size` are in the frame it will stand in,
    /// `pitch` in degrees.
    pub fn one(op: Op, shape: Shape, at: DVec3, size: DVec3, mat: u8, pitch: f64) -> Src {
        Src {
            op,
            shape,
            mat,
            at,
            size,
            rot: 0.0,
            tilt: 0.0,
            pitch,
            clip: None,
            blend: 0.0,
            room: false,
            each: Each::Once,
            alternate: false,
            axis: Axis::Up,
            steps: 10,
            south: false,
        }
    }
}

impl Building {
    /// A building of one box: a piece of street. `at` and `size` are in
    /// the frame it will stand in, `pitch` in degrees.
    pub fn slab(at: DVec3, size: DVec3, mat: u8, pitch: f64) -> Building {
        Building::several(&[Src::one(Op::Add, Shape::Box, at, size, mat, pitch)])
    }

    /// One brush or a few with no storeys, as an edit is: compiled in the
    /// frame they are given, in list order, with their box and any lamp
    /// among them. A cut's box is kept in the building's, so an edit that
    /// only cuts still has somewhere to be.
    pub fn several(srcs: &[Src]) -> Building {
        let mut rooms = 0;
        let mut b = Building {
            storey: 3.0,
            lo: DVec3::splat(f64::INFINITY),
            hi: DVec3::splat(f64::NEG_INFINITY),
            ..Default::default()
        };
        for src in srcs {
            let br = compile_one(src, 0.0, false, &mut rooms, &mut b.warnings, false);
            b.lo = b.lo.min(br.lo);
            b.hi = b.hi.max(br.hi);
            if br.op == Op::Add && br.mat == LAMP {
                b.lamps.push((br.c, 4.5));
            }
            b.brushes.push(br);
        }
        b.rooms = rooms;
        b
    }

    /// Drop every window whose opening meets a flight, grown a little, so a
    /// ramp never runs across a pane.
    fn drop_windows_on_flights(&mut self) {
        let flights: Vec<(DVec3, DVec3)> = self
            .brushes
            .iter()
            .filter(|b| b.op == Op::Add && (b.pitch.is_some() || b.shape == Shape::Stairs))
            .map(|b| {
                (
                    b.lo - DVec3::splat(FLIGHT_CLEAR),
                    b.hi + DVec3::splat(FLIGHT_CLEAR),
                )
            })
            .collect();
        let before = self.brushes.len();
        self.brushes
            .retain(|b| !(b.window && flights.iter().any(|(lo, hi)| b.meets(*lo, *hi))));
        self.dropped = before - self.brushes.len();
    }
}

fn read_item(b: &Value) -> Result<Item, String> {
    let vec3 = |key: &str| -> Result<DVec3, String> {
        b.get(key)
            .and_then(Value::nums)
            .filter(|v| v.len() == 3)
            .map(|v| DVec3::new(v[0], v[1], v[2]))
            .ok_or_else(|| format!("a brush needs '{key}' of three numbers"))
    };
    let num = |key: &str| b.get(key).and_then(Value::num).unwrap_or(0.0);
    let each = match b.get("each").and_then(Value::str) {
        Some("storey") => Each::Storey,
        Some("upper") => Each::Upper,
        Some("flight") => Each::Flight,
        Some("top") => Each::Top,
        _ => Each::Once,
    };
    let alternate = b.get("alternate").and_then(Value::bool).unwrap_or(false);
    let at = vec3("at")?;
    let size = vec3("size")?;
    if b.get("shape").and_then(Value::str) == Some("window") {
        return Ok(Item::Window(Window {
            face: b
                .get("face")
                .and_then(Value::str)
                .and_then(|f| f.chars().next())
                .unwrap_or('n'),
            at,
            size,
            each,
            alternate,
            pane: b.get("pane").is_some(),
            lit: b.get("lit").and_then(Value::bool),
        }));
    }
    let shape = match b.get("shape").and_then(Value::str) {
        Some("box") => Shape::Box,
        Some("cyl") => Shape::Cyl,
        Some("sphere") => Shape::Sphere,
        Some("stairs") => Shape::Stairs,
        other => return Err(format!("a brush of no known shape: {other:?}")),
    };
    let op = match b.get("op").and_then(Value::str) {
        Some("cut") => Op::Cut,
        _ => Op::Add,
    };
    let mat = match b.get("mat").and_then(Value::str) {
        Some(name) => material(name).ok_or_else(|| format!("unknown material {name}"))?,
        None => CONCRETE,
    };
    let clip = b
        .get("clip")
        .and_then(Value::nums)
        .filter(|c| c.len() == 2)
        .map(|c| [c[0], c[1]]);
    Ok(Item::Brush(Src {
        op,
        shape,
        mat,
        at,
        size,
        rot: num("rot"),
        tilt: num("tilt"),
        pitch: num("pitch"),
        clip,
        blend: num("blend"),
        room: b.get("room").and_then(Value::bool).unwrap_or(false),
        each,
        alternate,
        axis: match b.get("axis").and_then(Value::str) {
            Some("n") => Axis::North,
            Some("e") => Axis::East,
            _ => Axis::Up,
        },
        steps: b.get("steps").and_then(Value::num).map_or(10, |s| s as u32),
        south: b.get("dir").and_then(Value::str) == Some("s"),
    }))
}

#[cfg(test)]
mod tests;
