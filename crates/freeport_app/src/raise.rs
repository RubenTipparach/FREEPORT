//! Building on the hex world: raising and lowering a COLUMN.
//!
//! A hex world is edited the way tenebris's is, and not the way the dual
//! contoured one is: the ground is already flat in tiles, so a tile is the
//! unit a player builds in and half a metre is what a block is worth.
//! There is no brush, no site frame and no signed distance here, because
//! there is nothing to cut a shape out of: what a column stands at IS the
//! ground.
//!
//! One store (`freeport_core::stack::Stacks`), read by the walker through
//! `columns::Columns::top` and by the shader through the window
//! `tiers::send_raised` fills, so what is built is walked and drawn the
//! frame it lands with nothing to remesh and nothing to keep in step.

use crate::edit::{aim, REACH, SNAP};
use crate::walk::OnFoot;
use crate::{Args, Eye, Ground, Status};
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::window::{CursorOptions, PrimaryWindow};
use freeport_core::hex::Tile;

/// How many rings of tiles round the one aimed at an edit covers, by
/// brush: one tile, then a ring of seven, then two rings of nineteen.
const BRUSHES: [(&str, i64); 3] = [("a tile", 0), ("seven", 1), ("nineteen", 2)];

/// How many clicks `--sculpt` spends on its one patch. Three, which is
/// 1.5 m: at eight the mesa's own face filled the whole frame from an eye
/// 1.7 m up standing 2.2 m from it, and a picture of a wall says nothing
/// about what built it. At three the top of the mesa is just under the
/// eye, so the patch reads as a patch with ground round it, and it is
/// still well over the walker's 0.6 m stride, so the walker is stopped by
/// what it built.
const CLICKS: u32 = 3;

/// What the tile builder is set to, and what it has done.
#[derive(Resource, Default)]
pub struct Raising {
    /// Whether B has armed it.
    pub on: bool,
    /// Which of `BRUSHES`.
    pub brush: usize,
    /// Every edit made, newest last, so Z takes one back: the tiles it
    /// touched and by how much.
    pub done: Vec<Vec<(Tile, f64)>>,
}

/// The tiles an edit covers: the one aimed at and `rings` of neighbours
/// round it, which is a walk of the grid and never a list of the planet.
fn round_about(ground: &Ground, at: Tile, rings: i64) -> Vec<Tile> {
    let Some(grid) = ground.0.tiles else {
        return Vec::new();
    };
    let mut out = vec![at];
    let mut edge = vec![at];
    for _ in 0..rings {
        let mut next = Vec::new();
        for t in edge.drain(..) {
            for nb in grid.round(t) {
                if out.contains(&nb) {
                    continue;
                }
                out.push(nb);
                next.push(nb);
            }
        }
        edge = next;
    }
    out
}

/// One frame of the tile builder: B arms it, the mouse raises and lowers
/// the tiles under the crosshair, `[` and `]` pick the brush, Z takes the
/// last edit back.
#[allow(clippy::too_many_arguments)]
pub fn raise(
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    cursor: Query<&CursorOptions, With<PrimaryWindow>>,
    args: Res<Args>,
    walker: Option<Res<OnFoot>>,
    eye: Res<Eye>,
    mut ground: ResMut<Ground>,
    mut tool: ResMut<Raising>,
    mut status: ResMut<Status>,
    mut frame: Local<u32>,
) {
    *frame += 1;
    scripted(&args, &walker, &eye, &mut ground, &mut tool, *frame);
    if keys.just_pressed(KeyCode::KeyB) {
        tool.on = !tool.on;
    }
    if !tool.on {
        status.build = String::new();
        return;
    }
    if keys.just_pressed(KeyCode::BracketLeft) {
        tool.brush = (tool.brush + BRUSHES.len() - 1) % BRUSHES.len();
    }
    if keys.just_pressed(KeyCode::BracketRight) {
        tool.brush = (tool.brush + 1) % BRUSHES.len();
    }
    if keys.just_pressed(KeyCode::KeyZ) {
        undo(&mut ground, &mut tool);
    }
    // The mouse builds only while the window holds it, so a click on the
    // way back into the game does not dig a hole.
    let held = cursor.single().is_ok_and(|c| !c.visible);
    let up = buttons.just_pressed(MouseButton::Left) && held;
    let down = buttons.just_pressed(MouseButton::Right) && held;
    if up || down {
        let by = if up { SNAP } else { -SNAP };
        let look = walker.as_ref().map(|w| w.0.look());
        if let Some(look) = look.or_else(|| args.look.map(|l| (l - eye.0 .0).normalize())) {
            place(&mut ground, &mut tool, eye.0 .0, look, by);
        }
    }
    let (name, _) = BRUSHES[tool.brush];
    status.build = format!(
        "tiles: {name}, {} built, [ ] brush, click raises, right click lowers, Z back, B off",
        ground.0.stacks.len()
    );
}

/// The edit `--sculpt` places, once, a few frames in: a headless run has
/// nobody to click, and a picture of a builder with nought edits in it is
/// a picture of the ground. On foot it is aimed DOWN twenty degrees,
/// because the walker starts looking along the ground and a level look
/// meets nothing within the crosshair's reach, which is how the dual
/// contoured builder's first two pictures came back empty; from the fly
/// camera `--look` has already aimed it.
///
/// The patch is aimed at ONCE and then raised `CLICKS` times, which is
/// what a player holding the crosshair on one tile does. Re-aiming between
/// the clicks is what the first cut did, and a mesa re-aimed at grows
/// TOWARD the eye: the crosshair walks down its own near face a tile a
/// click, the last click lands on the tile under the feet, the walker
/// rides up on it, and the picture is a camera standing on its own edit
/// with the ground at eye level. Measured: 39 tiles raised where the brush
/// covers 19, every one of them within two tiles of the anchor.
///
/// `--sculpt KIND` names the BRUSH here and the shape on the dual
/// contoured world, since a hex world has one shape and it is a column.
fn scripted(
    args: &Args,
    walker: &Option<Res<OnFoot>>,
    eye: &Eye,
    ground: &mut Ground,
    tool: &mut Raising,
    frame: u32,
) {
    let Some(kind) = args.sculpt.as_deref() else {
        return;
    };
    if frame != 4 {
        return;
    }
    // `--sculpt KIND` names the BRUSH here, where on the dual contoured
    // world it names the shape: the widest unless the name is one of the
    // three, so an argument the flag carries is never ignored.
    let brush = BRUSHES
        .iter()
        .position(|(name, _)| *name == kind)
        .unwrap_or(BRUSHES.len() - 1);
    // On foot the look is pitched DOWN twenty degrees; from the fly
    // camera it is already aimed, and `--eye` and `--look` are how a
    // picture of the patch is taken from anywhere but eye level, which is
    // the only place a 1.5 m mesa two metres away cannot be seen from.
    let look = match walker {
        Some(w) => (w.0.look() - w.0.dir * 0.36).normalize(),
        None => match args.look {
            Some(l) => (l - eye.0 .0).normalize_or_zero(),
            None => return,
        },
    };
    if look == DVec3::ZERO {
        return;
    }
    tool.brush = brush;
    let tiles = aimed(ground, eye.0 .0, look, BRUSHES[brush].1);
    for _ in 0..CLICKS {
        lift(ground, tool, &tiles, SNAP);
    }
    let up = eye.0 .0.normalize_or(DVec3::Y);
    let far = tiles
        .iter()
        .filter_map(|t| ground.0.tiles.map(|g| g.dir(*t)))
        .map(|d| (d - up * d.dot(up)).length() * eye.0 .0.length())
        .fold(f64::INFINITY, f64::min);
    info!(
        "sculpt: {} tiles ({}) raised {:.1} m, the nearest {:.1} m ahead of the eye",
        tiles.len(),
        BRUSHES[brush].0,
        SNAP * CLICKS as f64,
        far,
    );
}

/// The tiles the crosshair is on: the brush's rings round the tile the
/// look ray first meets the field at, which is the ground the walker is
/// standing on and so the same answer the picture has.
fn aimed(ground: &Ground, eye: DVec3, look: DVec3, rings: i64) -> Vec<Tile> {
    let Some(grid) = ground.0.tiles else {
        return Vec::new();
    };
    let hit = {
        let field = ground.0.underfoot(eye, REACH + 2.0);
        aim(&field, eye, look)
    };
    let Some((at, _)) = hit else {
        return Vec::new();
    };
    round_about(ground, grid.at(at.normalize_or(DVec3::Y)), rings)
}

/// Those tiles, raised by `by`, as one edit: Z takes the whole patch back
/// rather than a tile of it.
fn lift(ground: &mut Ground, tool: &mut Raising, tiles: &[Tile], by: f64) {
    let Some(grid) = ground.0.tiles else {
        return;
    };
    if tiles.is_empty() {
        return;
    }
    let mut world = (*ground.0).clone();
    for tile in tiles {
        world.stacks.raise(grid, *tile, by);
    }
    ground.0 = std::sync::Arc::new(world);
    tool.done.push(tiles.iter().map(|t| (*t, by)).collect());
}

/// An edit at the crosshair: every tile of the brush raised by `by`.
fn place(ground: &mut Ground, tool: &mut Raising, eye: DVec3, look: DVec3, by: f64) {
    let tiles = aimed(ground, eye, look, BRUSHES[tool.brush].1);
    lift(ground, tool, &tiles, by);
}

/// The last edit, undone: every tile it raised, lowered by the same.
fn undo(ground: &mut Ground, tool: &mut Raising) {
    let Some(grid) = ground.0.tiles else {
        return;
    };
    let Some(last) = tool.done.pop() else {
        return;
    };
    let mut world = (*ground.0).clone();
    for (tile, by) in last {
        world.stacks.raise(grid, tile, -by);
    }
    ground.0 = std::sync::Arc::new(world);
}
