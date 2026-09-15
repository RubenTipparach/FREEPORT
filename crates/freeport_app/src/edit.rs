//! Building on foot: the mockup's builder, in Bevy.
//!
//! B, then build: a brush at the point the crosshair meets the field (a
//! march along the look ray, the field's gradient for the face), snapped
//! to half a metre in its SITE's frame, added with the left button and cut
//! with the right, R and F lifting where it lands by half a metre, Z
//! taking it back, P writing everything built to a file. The shapes are
//! the mockup's: a block, a slab, a pillar, a ball, a ramp, a wall laid
//! from one click to the next, a pad of flat ground, a room, a door and a
//! window; the materials concrete, plate, glass, lamp and terrain. An edit
//! is a structure like any other, evaluated in the same field the walker
//! walks and the mesher contours, so what is built is walkable the frame
//! it lands and the chunks it touches are contoured again. Edits are built
//! on sites: the first edit on a spot fixes a frame, its patch of the
//! sphere, and every edit within `SITE_REACH` of it is placed in that
//! frame, because two edits on their own patches lean against each other.
//! A cut below the sea that touches no water is DRY, and stays so until a
//! cut that does reaches it (`water.rs`).

use crate::stream::{Chunk, Frame, Streamer};
use crate::walk::OnFoot;
use crate::{Args, Eye, Ground, Status, World};
use bevy::ecs::system::SystemParam;
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use freeport_core::field::{Block, Density, Structure, CONCRETE, GLASS, LAMP, PLATE, TERRAIN};
use freeport_core::json::Value;
use freeport_core::pos::WorldPos;
use freeport_core::recipe::{Building, Op, Shape, Src};
use freeport_core::town::{frame_at, Frame as SiteFrame};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Metres from a site's origin that build in its frame.
const SITE_REACH: f64 = 8.0;
/// What is built snaps to this, metres.
pub(crate) const SNAP: f64 = 0.5;
/// How far the crosshair reaches, metres, and its march's step.
pub(crate) const REACH: f64 = 7.0;
pub(crate) const STRIDE: f64 = 0.08;
/// A cut touching water within this of its box is wet.
const WET_REACH: f64 = 0.5;

/// One shape the builder offers.
struct Kind {
    name: &'static str,
    shape: Shape,
    size: DVec3,
    pitch: f64,
    room: bool,
    pad: bool,
    line: bool,
    /// How far a wall's foot is sunk, metres.
    sink: f64,
    cut: bool,
}

const KINDS: [Kind; 10] = [
    Kind {
        name: "block",
        shape: Shape::Box,
        size: DVec3::new(1.0, 1.0, 1.0),
        pitch: 0.0,
        room: false,
        pad: false,
        line: false,
        sink: 0.0,
        cut: false,
    },
    Kind {
        name: "slab",
        shape: Shape::Box,
        size: DVec3::new(2.0, 2.0, 0.4),
        pitch: 0.0,
        room: false,
        pad: false,
        line: false,
        sink: 0.0,
        cut: false,
    },
    Kind {
        name: "pillar",
        shape: Shape::Cyl,
        size: DVec3::new(0.5, 0.5, 2.6),
        pitch: 0.0,
        room: false,
        pad: false,
        line: false,
        sink: 0.0,
        cut: false,
    },
    Kind {
        name: "ball",
        shape: Shape::Sphere,
        size: DVec3::new(1.0, 1.0, 1.0),
        pitch: 0.0,
        room: false,
        pad: false,
        line: false,
        sink: 0.0,
        cut: false,
    },
    Kind {
        name: "ramp",
        shape: Shape::Box,
        size: DVec3::new(1.4, 1.6, 0.4),
        pitch: 30.0,
        room: false,
        pad: false,
        line: false,
        sink: 0.0,
        cut: false,
    },
    Kind {
        name: "wall",
        shape: Shape::Box,
        size: DVec3::new(0.4, 0.4, 2.6),
        pitch: 0.0,
        room: false,
        pad: false,
        line: true,
        sink: 0.3,
        cut: false,
    },
    Kind {
        name: "pad",
        shape: Shape::Box,
        size: DVec3::new(6.0, 6.0, 4.0),
        pitch: 0.0,
        room: false,
        pad: true,
        line: false,
        sink: 0.0,
        cut: false,
    },
    Kind {
        name: "room",
        shape: Shape::Box,
        size: DVec3::new(4.0, 4.0, 2.6),
        pitch: 0.0,
        room: true,
        pad: false,
        line: false,
        sink: 0.0,
        cut: true,
    },
    Kind {
        name: "door",
        shape: Shape::Box,
        size: DVec3::new(1.2, 1.2, 2.2),
        pitch: 0.0,
        room: false,
        pad: false,
        line: false,
        sink: 0.0,
        cut: true,
    },
    Kind {
        name: "window",
        shape: Shape::Box,
        size: DVec3::new(1.2, 1.2, 1.2),
        pitch: 0.0,
        room: false,
        pad: false,
        line: false,
        sink: 0.0,
        cut: true,
    },
];

const MATS: [(&str, u8); 5] = [
    ("concrete", CONCRETE),
    ("plate", PLATE),
    ("glass", GLASS),
    ("lamp", LAMP),
    ("terrain", TERRAIN),
];

/// One edit: which site it is on and the brushes it wrote there.
#[derive(Clone, Debug)]
pub struct Edit {
    pub site: usize,
    pub srcs: Vec<Src>,
    /// Where it stands in the world's structures.
    index: usize,
}

/// The builder's state.
#[derive(Resource, Default)]
pub struct Builder {
    pub on: bool,
    kind: usize,
    mat: usize,
    lift: f64,
    /// A wall's first click: its site and where it starts, in that frame.
    from: Option<(usize, DVec3)>,
    pub sites: Vec<SiteFrame>,
    pub edits: Vec<Edit>,
    /// What a click would build now.
    proposal: Option<Proposal>,
}

/// The brush a click would make, on its site, snapped.
#[derive(Clone, Debug)]
struct Proposal {
    site: usize,
    new_site: Option<SiteFrame>,
    srcs: Vec<Src>,
    post: bool,
    cut: bool,
}

/// The ghost that shows the proposal.
#[derive(Component)]
pub struct Ghost;

/// Where the crosshair meets the field: a march along the look ray, then
/// the field's gradient there for the face. The tile builder aims the same
/// way (`raise.rs`), because where a crosshair lands is one question.
pub(crate) fn aim(field: &dyn Density, eye: DVec3, look: DVec3) -> Option<(DVec3, DVec3)> {
    let mut prev = 0.3;
    let mut t = 0.4;
    while t < REACH {
        if field.at(eye + look * t) > 0.0 {
            let (mut lo, mut hi) = (prev, t);
            for _ in 0..8 {
                let m = 0.5 * (lo + hi);
                if field.at(eye + look * m) > 0.0 {
                    hi = m;
                } else {
                    lo = m;
                }
            }
            let p = eye + look * (0.5 * (lo + hi));
            let e = 0.04;
            let g = DVec3::new(
                field.at(p + DVec3::X * e) - field.at(p - DVec3::X * e),
                field.at(p + DVec3::Y * e) - field.at(p - DVec3::Y * e),
                field.at(p + DVec3::Z * e) - field.at(p - DVec3::Z * e),
            );
            return Some((p, (-g).normalize_or(p.normalize())));
        }
        prev = t;
        t += STRIDE;
    }
    None
}

fn snap(v: f64) -> f64 {
    (v / SNAP).round() * SNAP
}

/// Flat ground at a point: a fill of terrain up to its height and a
/// clearing above it, so a hill is cut and a dip is filled.
fn pad_at(at: DVec3, kind: &Kind) -> Vec<Src> {
    let fill = Src::one(
        Op::Add,
        Shape::Box,
        at - DVec3::Z * 2.0,
        kind.size,
        TERRAIN,
        0.0,
    );
    let clear = Src::one(
        Op::Cut,
        Shape::Box,
        at + DVec3::Z * 1.5,
        DVec3::new(kind.size.x, kind.size.y, 3.0),
        TERRAIN,
        0.0,
    );
    vec![fill, clear]
}

/// A wall: its first click is a post where it will start, its second the
/// wall from there to here, its foot sunk so it keeps its foot in the
/// ground where the ground curves away.
fn wall_to(p: DVec3, kind: &Kind, mat: u8, from: Option<DVec3>) -> (Vec<Src>, bool) {
    let (w, h) = (kind.size.x, kind.size.z);
    let Some(from) = from else {
        let post = Src::one(
            Op::Add,
            Shape::Box,
            DVec3::new(p.x, p.y, p.z + h / 2.0 - kind.sink),
            DVec3::new(w, w, h),
            mat,
            0.0,
        );
        return (vec![post], true);
    };
    let de = p.x - from.x;
    let dn = p.y - from.y;
    let len = de.hypot(dn).max(SNAP);
    let mut wall = Src::one(
        Op::Add,
        Shape::Box,
        DVec3::new(
            (from.x + p.x) / 2.0,
            (from.y + p.y) / 2.0,
            from.z + h / 2.0 - kind.sink,
        ),
        DVec3::new(w, len, h),
        mat,
        0.0,
    );
    wall.rot = (-de).atan2(dn).to_degrees();
    (vec![wall], false)
}

impl Builder {
    /// The site a point builds on: the nearest within reach, else a new one
    /// at the point, plumb on its own patch with its base at the ground.
    fn site_for(&self, p: DVec3, world: &World) -> (usize, Option<SiteFrame>) {
        let mut best: Option<(usize, f64)> = None;
        for (i, s) in self.sites.iter().enumerate() {
            let l = s.local(p);
            let d = l.x.hypot(l.y);
            if d < SITE_REACH && best.is_none_or(|b| d < b.1) {
                best = Some((i, d));
            }
        }
        if let Some((i, _)) = best {
            return (i, None);
        }
        let dir = p.normalize_or(DVec3::Y);
        let (east, north) = frame_at(dir);
        let base = snap(p.length()).max(world.bounds.floor);
        (
            self.sites.len(),
            Some(SiteFrame {
                dir,
                east,
                north,
                base,
            }),
        )
    }

    /// The brush a click would make.
    fn propose(&self, world: &World, eye: DVec3, look: DVec3, cut: bool) -> Option<Proposal> {
        let field = world.field_near(eye, REACH + 2.0);
        let (hit, normal) = aim(&field, eye, look)?;
        let kind = &KINDS[self.kind];
        let mat = MATS[self.mat].1;
        let (site, new_site) = match self.from {
            Some((s, _)) => (s, None),
            None => self.site_for(hit, world),
        };
        let frame = new_site.unwrap_or_else(|| self.sites[site]);
        let snapped = |p: DVec3| {
            let l = frame.local(p);
            DVec3::new(snap(l.x), snap(l.y), snap(l.z) + self.lift)
        };
        let cut = kind.cut || cut;
        let (srcs, post, cut) = if kind.pad {
            (pad_at(snapped(hit), kind), false, false)
        } else if kind.line {
            let (srcs, post) = wall_to(snapped(hit), kind, mat, self.from.map(|f| f.1));
            (srcs, post, false)
        } else {
            let half = kind.size * 0.5;
            let reach = if normal.dot(frame.dir).abs() > 0.7 {
                half.z
            } else {
                half.x.max(half.y)
            };
            let c = hit + normal * if cut { -(reach - 0.05) } else { reach + 0.02 };
            let op = if cut { Op::Cut } else { Op::Add };
            let mut src = Src::one(op, kind.shape, snapped(c), kind.size, mat, kind.pitch);
            src.room = kind.room;
            (vec![src], false, cut)
        };
        Some(Proposal {
            site,
            new_site,
            srcs,
            post,
            cut,
        })
    }

    /// What the status line says.
    fn status(&self) -> String {
        let kind = &KINDS[self.kind];
        let how = if kind.line {
            if self.from.is_some() {
                "LMB the far end"
            } else {
                "LMB the near end"
            }
        } else if kind.pad {
            "LMB flattens the ground here"
        } else if kind.room {
            "LMB cuts a room"
        } else {
            "LMB add, RMB cut"
        };
        format!(
            "BUILD {} of {}: {}, [ ] shape, , . material, R F height{}, Z undo ({} edits), P save",
            kind.name,
            MATS[self.mat].0,
            how,
            if self.lift != 0.0 {
                format!(" ({:+} m)", self.lift)
            } else {
                String::new()
            },
            self.edits.len()
        )
    }
}

/// The world with an edit written into it, and the chunks it touches
/// contoured again.
fn apply(builder: &mut Builder, ground: &mut Ground, streamer: &mut Streamer, p: Proposal) {
    let mut world = (*ground.0).clone();
    let site = match p.new_site {
        Some(f) => {
            builder.sites.push(f);
            builder.sites.len() - 1
        }
        None => p.site,
    };
    let frame = builder.sites[site];
    let building = Building::several(&p.srcs);
    let st = Structure::new(frame, building);
    // A cut below the sea is dry unless it touches water now, and a wet
    // cut floods every dry cut it reaches.
    if p.cut {
        for src in &p.srcs {
            if src.shape != Shape::Box {
                continue;
            }
            let block = block_of(&frame, src);
            let field = world.field_near(st.frame.world(src.at), 12.0);
            let water = world.water(&field);
            if water.touches(&block, WET_REACH) {
                let mut w = world.water(&field);
                w.flood(&block);
                let left = w.dry.clone();
                world.dry = left;
            } else {
                world.dry.push(block);
            }
        }
    }
    let (lo, hi) = (st.lo, st.hi);
    for lamp in st.lamps() {
        world.lamps.push(lamp);
    }
    world.structures.push(st);
    builder.edits.push(Edit {
        site,
        srcs: p.srcs,
        index: world.structures.len() - 1,
    });
    ground.0 = Arc::new(world);
    streamer.dirty(lo, hi);
}

/// A box brush as a block in the world, for the water's dry list.
fn block_of(frame: &SiteFrame, src: &Src) -> Block {
    let (c, s) = (src.rot.to_radians().cos(), src.rot.to_radians().sin());
    let east = frame.east * c + frame.north * s;
    let north = frame.dir.cross(east);
    Block {
        centre: frame.world(src.at),
        half: src.size * 0.5,
        axes: [east, north, frame.dir],
    }
}

/// Take the last edit back.
fn undo(builder: &mut Builder, ground: &mut Ground, streamer: &mut Streamer) {
    let Some(edit) = builder.edits.pop() else {
        return;
    };
    let mut world = (*ground.0).clone();
    if edit.index < world.structures.len() {
        let st = world.structures.remove(edit.index);
        let lamps = st.lamps();
        world
            .lamps
            .retain(|l| !lamps.iter().any(|m| (m.0 - l.0).length() < 1e-9));
        streamer.dirty(st.lo, st.hi);
        for e in builder.edits.iter_mut() {
            if e.index > edit.index {
                e.index -= 1;
            }
        }
    }
    ground.0 = Arc::new(world);
}

/// Everything built, as recipes: one a site, its brushes in the site's
/// frame, and where the site is.
pub fn recipes_json(builder: &Builder) -> String {
    let mut sites = Vec::new();
    for (i, site) in builder.sites.iter().enumerate() {
        let brushes: Vec<Value> = builder
            .edits
            .iter()
            .filter(|e| e.site == i)
            .flat_map(|e| e.srcs.iter().map(src_value))
            .collect();
        if brushes.is_empty() {
            continue;
        }
        let mut o = BTreeMap::new();
        o.insert("name".into(), Value::String(format!("sculpted {}", i + 1)));
        o.insert("dir".into(), nums(&[site.dir.x, site.dir.y, site.dir.z]));
        o.insert("base".into(), Value::Number(site.base));
        o.insert("footprint".into(), nums(&[1.0, 1.0]));
        o.insert("brushes".into(), Value::Array(brushes));
        sites.push(Value::Object(o));
    }
    Value::Array(sites).write()
}

fn nums(v: &[f64]) -> Value {
    Value::Array(v.iter().map(|x| Value::Number(*x)).collect())
}

fn src_value(s: &Src) -> Value {
    let mat = MATS
        .iter()
        .find(|m| m.1 == s.mat)
        .map_or("concrete", |m| m.0);
    let shape = match s.shape {
        Shape::Box => "box",
        Shape::Cyl => "cyl",
        Shape::Sphere => "sphere",
        Shape::Stairs => "stairs",
    };
    let mut o = BTreeMap::new();
    let text = |t: &str| Value::String(t.to_string());
    o.insert(
        "op".into(),
        text(if s.op == Op::Cut { "cut" } else { "add" }),
    );
    o.insert("shape".into(), text(shape));
    o.insert("mat".into(), text(mat));
    o.insert("at".into(), nums(&[s.at.x, s.at.y, s.at.z]));
    o.insert("size".into(), nums(&[s.size.x, s.size.y, s.size.z]));
    if s.rot != 0.0 {
        o.insert("rot".into(), Value::Number(s.rot));
    }
    if s.pitch != 0.0 {
        o.insert("pitch".into(), Value::Number(s.pitch));
    }
    if s.room {
        o.insert("room".into(), Value::Bool(true));
    }
    Value::Object(o)
}

/// What a frame of building reads: the keys, the buttons, whether the
/// window holds the mouse, and the scripted edit.
#[derive(SystemParam)]
pub struct Presses<'w, 's> {
    keys: Res<'w, ButtonInput<KeyCode>>,
    buttons: Res<'w, ButtonInput<MouseButton>>,
    cursor: Query<'w, 's, &'static CursorOptions, With<PrimaryWindow>>,
    args: Res<'w, Args>,
    frame: Local<'s, u32>,
    /// Whether the scripted edit has been placed.
    done: Local<'s, bool>,
}

/// What an edit changes: the builder's own state, the world, the chunks
/// drawn from it, and the status line.
#[derive(SystemParam)]
pub struct Editing<'w> {
    builder: ResMut<'w, Builder>,
    ground: ResMut<'w, Ground>,
    streamer: ResMut<'w, Streamer>,
    status: ResMut<'w, Status>,
}

/// Where the eye is and what it looks along, through the origin.
#[derive(SystemParam)]
pub struct Sight<'w> {
    walker: Option<Res<'w, OnFoot>>,
    eye: Res<'w, Eye>,
    frame: Res<'w, Frame>,
}

/// What the ghost is drawn with.
#[derive(SystemParam)]
pub struct Drawing<'w, 's> {
    commands: Commands<'w, 's>,
    ghost: Query<'w, 's, (Entity, &'static mut Transform, &'static mut Visibility), With<Ghost>>,
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
}

/// The keys that set the builder up: which shape, which material, how
/// high, and taking the last edit back or writing them all out.
fn press_keys(keys: &ButtonInput<KeyCode>, editing: &mut Editing) {
    let n = KINDS.len();
    let b = &mut editing.builder;
    if keys.just_pressed(KeyCode::BracketRight) {
        b.kind = (b.kind + 1) % n;
        b.from = None;
    }
    if keys.just_pressed(KeyCode::BracketLeft) {
        b.kind = (b.kind + n - 1) % n;
        b.from = None;
    }
    if keys.just_pressed(KeyCode::Period) {
        b.mat = (b.mat + 1) % MATS.len();
    }
    if keys.just_pressed(KeyCode::Comma) {
        b.mat = (b.mat + MATS.len() - 1) % MATS.len();
    }
    if keys.just_pressed(KeyCode::KeyR) {
        b.lift = snap(b.lift + SNAP);
    }
    if keys.just_pressed(KeyCode::KeyF) {
        b.lift = snap(b.lift - SNAP);
    }
    if keys.just_pressed(KeyCode::KeyZ) {
        undo(
            &mut editing.builder,
            &mut editing.ground,
            &mut editing.streamer,
        );
    }
    if keys.just_pressed(KeyCode::KeyP) {
        let text = recipes_json(&editing.builder);
        match std::fs::write("freeport_edits.json", &text) {
            Ok(()) => info!(
                "wrote {} edits on {} sites to freeport_edits.json",
                editing.builder.edits.len(),
                editing.builder.sites.len()
            ),
            Err(e) => warn!("could not write freeport_edits.json: {e}"),
        }
    }
}

/// A press on a proposal: a wall's first click only remembers where it
/// starts; anything else is built.
fn place(editing: &mut Editing, p: Proposal) {
    if p.post {
        let site = match p.new_site {
            Some(f) => {
                editing.builder.sites.push(f);
                editing.builder.sites.len() - 1
            }
            None => p.site,
        };
        let kind = &KINDS[editing.builder.kind];
        let at = p.srcs[0].at - DVec3::Z * (kind.size.z / 2.0 - kind.sink);
        editing.builder.from = Some((site, at));
    } else {
        editing.builder.from = None;
        apply(
            &mut editing.builder,
            &mut editing.ground,
            &mut editing.streamer,
            p,
        );
    }
}

/// One frame of the builder: the keys, the proposal, the ghost, the clicks.
pub fn build(mut presses: Presses, mut editing: Editing, sight: Sight, mut drawing: Drawing) {
    *presses.frame += 1;
    // The scripted edit: the frame after the first load has settled the
    // builder is on, the shape is the one asked for, and the left button
    // is pressed, so the remesh it starts is measured on its own and not
    // among the first load's.
    let scripted = presses
        .args
        .sculpt
        .as_deref()
        .filter(|_| editing.streamer.settled() && !*presses.done);
    if let Some(name) = scripted {
        *presses.done = true;
        editing.builder.on = true;
        if let Some(k) = KINDS.iter().position(|k| k.name == name) {
            editing.builder.kind = k;
        }
    }
    if presses.keys.just_pressed(KeyCode::KeyB) {
        editing.builder.on = !editing.builder.on;
        editing.builder.from = None;
    }
    let Some(walker) = &sight.walker else {
        editing.builder.on = false;
        editing.status.build.clear();
        return;
    };
    if !editing.builder.on {
        editing.status.build.clear();
        for (_, _, mut v) in &mut drawing.ghost {
            *v = Visibility::Hidden;
        }
        return;
    }
    press_keys(&presses.keys, &mut editing);
    let taken = presses
        .cursor
        .single()
        .map(|c| c.grab_mode == CursorGrabMode::Locked)
        .unwrap_or(false);
    let cut = presses.buttons.just_pressed(MouseButton::Right);
    // The scripted edit looks down at the street a few metres on, because
    // the walker starts looking along it and a level look meets nothing
    // within reach.
    let look = match scripted {
        Some(_) => (walker.0.fwd - walker.0.dir * 0.6).normalize(),
        None => walker.0.look(),
    };
    let proposal = editing
        .builder
        .propose(&editing.ground.0, sight.eye.0 .0, look, cut);
    editing.builder.proposal = proposal.clone();
    let pressed =
        (taken && (presses.buttons.just_pressed(MouseButton::Left) || cut)) || scripted.is_some();
    if let (true, Some(p)) = (pressed, proposal) {
        place(&mut editing, p);
    }
    editing.status.build = editing.builder.status();
    show_ghost(&editing.builder, &sight.frame, &mut drawing);
}

/// The ghost box where the proposal would land.
fn show_ghost(builder: &Builder, frame: &Frame, drawing: &mut Drawing) {
    let Some(p) = &builder.proposal else {
        for (_, _, mut v) in drawing.ghost.iter_mut() {
            *v = Visibility::Hidden;
        }
        return;
    };
    let site = p.new_site.unwrap_or_else(|| builder.sites[p.site]);
    let src = &p.srcs[0];
    let at = site.world(src.at);
    let rot = Quat::from_mat3(&Mat3::from_cols(
        site.east.as_vec3(),
        site.north.as_vec3(),
        site.dir.as_vec3(),
    )) * Quat::from_rotation_z(src.rot.to_radians() as f32)
        * Quat::from_rotation_x(src.pitch.to_radians() as f32);
    let tf = Transform {
        translation: frame.0.local(WorldPos(at)),
        rotation: rot,
        scale: src.size.as_vec3(),
    };
    if let Ok((_, mut t, mut v)) = drawing.ghost.single_mut() {
        *t = tf;
        *v = Visibility::Visible;
        return;
    }
    let mesh = drawing.meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    drawing.commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(drawing.materials.add(StandardMaterial {
            base_color: Color::srgba(0.5, 0.85, 1.0, 0.35),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            cull_mode: None,
            ..default()
        })),
        tf,
        Ghost,
        Chunk {
            corner: WorldPos(at),
        },
    ));
}
