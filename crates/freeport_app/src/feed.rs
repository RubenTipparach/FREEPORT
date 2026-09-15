//! What the CPU hands the two tiers every frame, and nothing else.
//!
//! Three buffers and a uniform: Planet-LOD's picked leaves, what has been
//! built on the hex tiles, the towns' levelled sites, and the lanes that
//! carry where the eye is. Every one of them is written as an OFFSET from
//! the same ANCHOR, one unit direction differenced in f64 here, because a
//! shader on a thousand kilometre planet that forms a planet scale number
//! has already lost the metres it was asked about (`freeport_core::pos`,
//! and `tiers.wgsl`'s own `Spot`).
//!
//! Two of the three are written only when they change: the built tiles
//! when the anchor moves or an edit lands, the sites when the anchor
//! moves. The leaves are a function of where the eye is and go every
//! frame.

use crate::tiers::{Drawn, Lanes, Store, Tiers, OVERLAP};
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::render::storage::ShaderStorageBuffer;
use freeport_core::{hex, lod};

/// The leaves Planet-LOD picked, into the buffer the far tier and the sea
/// both read, as OFFSETS from the anchor: the subtraction is done in f64
/// here so the shader never has to form a unit vector at a planet's
/// scale. A far leaf's offset is large and its accuracy does not matter;
/// a near one's is small and exact. Answers how many went, which is where
/// the shader is told to stop.
fn send_leaves(
    at: &Tiers,
    drawn: &Drawn,
    buffers: &mut Assets<ShaderStorageBuffer>,
    anchor: DVec3,
    picked: &[lod::Tri],
) -> usize {
    let live = picked.len().min(at.most_leaves());
    if let Some(buffer) = buffers.get_mut(&drawn.leaves) {
        let mut data: Vec<Vec4> = Vec::with_capacity(live * 3);
        for tri in picked.iter().take(live) {
            for corner in tri {
                data.push((*corner - anchor).as_vec3().extend(0.0));
            }
        }
        data.resize(at.most_leaves() * 3, Vec4::ZERO);
        buffer.set_data(data.as_slice());
    }
    live
}

/// What has been BUILT on the tiles of the hex window, into the buffer the
/// two hex entry points read: one number a tile, in the window's own
/// order, so the shader indexes it with the `tile` it has already worked
/// out and never looks an address up.
///
/// It is filled from the STACKS rather than by walking the window, because
/// a window is nine thousand tiles and what is built is a handful:
/// `hex::Grid::steps` is `basis` inverted, so a built tile is asked where
/// it sits in the window and written there if it sits in it at all. And it
/// is only filled when the anchor MOVES or an edit lands, which is once
/// every tile the walker crosses rather than once a frame.
fn send_raised(
    at: &Tiers,
    drawn: &mut Drawn,
    buffers: &mut Assets<ShaderStorageBuffer>,
    ground: &crate::Ground,
    anchor: hex::Tile,
) {
    let Some(grid) = ground.0.tiles else {
        return;
    };
    let built = ground.0.stacks.len() as u64;
    if drawn.filled == Some((anchor, built)) {
        return;
    }
    drawn.filled = Some((anchor, built));
    let Some(buffer) = buffers.get_mut(&drawn.raised) else {
        return;
    };
    let span = at.span as i64;
    let wide = span * 2 + 1;
    // Two numbers a tile, because a built column is a height AND what it
    // is made of: a street is a tag at nought and a wall is both, and a
    // shader handed only the height would draw a town in grass.
    let mut data = vec![Vec2::ZERO; at.window()];
    let mut inside = 0usize;
    for (key, metres, material) in ground.0.stacks.each() {
        let Some(tile) = freeport_core::stack::tile(grid, key) else {
            continue;
        };
        let (u, v) = grid.steps(anchor, grid.dir(tile));
        let (u, v) = (u.round() as i64, v.round() as i64);
        if u.abs() > span || v.abs() > span {
            continue;
        }
        inside += 1;
        data[((v + span) * wide + u + span) as usize] = Vec2::new(metres as f32, material as f32);
    }
    if inside != built as usize {
        // A tile built and then walked away from falls out of the window
        // and is not drawn, which is right. One built HERE that missed it
        // would be an edit the walker stands on and the picture has not
        // got, so the count says so rather than leaving it to be found in
        // a screenshot.
        info!("raised: {built} tiles built, {inside} of them in the hex window");
    }
    buffer.set_data(data.as_slice());
}

/// The towns' levelled ground into the buffer `field.wgsl` owns: two lanes
/// a site, the first its direction taken to the ANCHOR and differenced in
/// f64 with the site's height in w, the second the two arcs of its skirt
/// off the core's own `field::site_band`, so the shader's `site_weight`
/// and the core's are one rule with one pair of constants.
///
/// Refilled when the anchor moves, which is once a tile the walker
/// crosses: a site's offset is a function of the anchor and of nothing
/// else, so a frame that did not rebase writes nothing. It is the anchor
/// and not the eye for the reason every offset here is: the difference of
/// two unit vectors near each other is small and accurate, and
/// `acos(dot(..))` in f32 on this planet cannot tell one end of a town
/// from the other.
fn send_sites(
    drawn: &mut Drawn,
    buffers: &mut Assets<ShaderStorageBuffer>,
    ground: &crate::Ground,
    anchor: DVec3,
    anchor_tile: hex::Tile,
) {
    if drawn.sited == Some(anchor_tile) {
        return;
    }
    drawn.sited = Some(anchor_tile);
    let Some(buffer) = buffers.get_mut(&drawn.sites) else {
        drawn.sited = None;
        return;
    };
    let sites = &ground.0.planet.sites;
    if sites.is_empty() {
        return;
    }
    let mut data: Vec<Vec4> = Vec::with_capacity(sites.len() * 2);
    for site in sites {
        let (inner, outer) = freeport_core::field::site_band(site);
        data.push((site.dir - anchor).as_vec3().extend(site.h as f32));
        data.push(Vec4::new(inner as f32, outer as f32, 0.0, 0.0));
    }
    buffer.set_data(data.as_slice());
}

/// Every frame: where the eye is, which tile it stands on, and which
/// leaves Planet-LOD picks from there. This is the whole of what the CPU
/// does for either tier.
pub fn feed_tiers(
    eye: Res<crate::Eye>,
    frame: Res<crate::stream::Frame>,
    at: Res<Tiers>,
    ground: Res<crate::Ground>,
    mut drawn: ResMut<Drawn>,
    mut assets: Store,
    mut said: Local<usize>,
) {
    let clock = std::time::Instant::now();
    let here = eye.0 .0;
    let centre = frame.0.local(freeport_core::pos::WorldPos(DVec3::ZERO));
    let dir = here.normalize_or(DVec3::Z);
    let grid = at.grid();
    // The anchor: the eye's own tile, as a point in its face's plane and
    // as the unit direction every vertex of every tier is measured off.
    // The two lattice steps come down DIVIDED by that point's own length,
    // so what the shader adds to a unit anchor is a small number.
    let (point, e1, e2) = grid.basis(grid.at(dir));
    let span = point.length().max(f64::MIN_POSITIVE);
    let anchor = point / span;
    let (e1, e2) = (e1 / span, e2 / span);
    // The one large subtraction, in f64: where the anchor's own ground
    // stands in the render frame. Everything else is an offset from it.
    let base = (anchor * at.radius - frame.0.at).as_vec3();
    // `select` is asked for the WHOLE planet, with no hole: the far tier
    // cuts its own in the shader, a sub triangle at a time, and the sea
    // rides the very same leaves with no hole at all. One walk a frame
    // serves all three.
    //
    // The hole is the near tier's disc less `OVERLAP` tiles, so the two
    // ground tiers OVERLAP at the rim rather than meeting there. A tile of
    // overlap was not enough and the picture said so: at a grazing angle
    // the rim was a band of SKY, because the two carry the same height
    // differently (a column's top is flat at its middle's height, a leaf's
    // is linear between its corners) and where the far tier stood higher
    // the line of sight went under it, over the ground behind and out.
    // The hole, as the SQUARED CHORD of its angle: `|dir - anchor|^2` is
    // `2 (1 - cos t)`, and a chord is where an f32 keeps its precision
    // while a cosine near one is a number it cannot tell from one.
    let reach = at.disc() - OVERLAP * grid.spacing(at.radius) / at.radius;
    let hole = 2.0 * (1.0 - reach.cos());
    let picked = lod::select(
        here,
        at.radius,
        &lod::Lod {
            ratio: at.ratio,
            detail: 0.0,
            cull: true,
        },
        0.0,
        dir,
    );
    let live = send_leaves(&at, &drawn, &mut assets.buffers, anchor, &picked);
    send_raised(&at, &mut drawn, &mut assets.buffers, &ground, grid.at(dir));
    send_sites(
        &mut drawn,
        &mut assets.buffers,
        &ground,
        anchor,
        grid.at(dir),
    );
    // What the CPU costs, said when it moves by a fifth: the whole of the
    // far tier's work is this one `select`, and the vertex stage makes
    // `sub * sub` triangles out of every leaf it picks.
    if picked.len() * 5 > *said * 6 || picked.len() * 6 < *said * 5 {
        // What was PICKED throttles the line, not what was drawn, so a
        // truncated frame does not pin the count and report itself for
        // ever. A truncation is a piece of the planet not drawn, so it
        // says so rather than leaving a hole for somebody to find in a
        // picture.
        if picked.len() > live {
            warn!(
                "Planet-LOD picked {} leaves and the buffer holds {live}: raise MOST_LEAVES",
                picked.len(),
            );
        }
        info!(
            "Planet-LOD picked {live} leaves in {:.2} ms, drawn as {} triangles",
            clock.elapsed().as_secs_f64() * 1e3,
            live * (at.sub * at.sub) as usize,
        );
        *said = picked.len();
    }
    push_lanes(
        &at,
        &drawn,
        &mut assets,
        Where {
            centre,
            anchor,
            base,
            e1,
            e2,
            hole,
            live,
        },
    );
}

/// Where the eye is this frame, in the terms the lanes carry it: the
/// planet's centre in the render frame, the anchor and the two lattice
/// steps at it, how far the hex disc reaches and how many leaves are live.
struct Where {
    centre: Vec3,
    anchor: DVec3,
    base: Vec3,
    e1: DVec3,
    e2: DVec3,
    hole: f64,
    live: usize,
}

/// That, into all four materials. Getting a material mutably is what
/// rebuilds its bind group, which is how a storage buffer written this
/// frame reaches the shader this frame.
fn push_lanes(at: &Tiers, drawn: &Drawn, assets: &mut Store, w: Where) {
    // The anchor doubles as the hex disc's own middle, which is what the
    // far tier measures its hole against, so the near tier and the hole
    // are one direction and never two.
    let step = |lanes: &mut Lanes| {
        lanes.at = w.centre.extend(w.live as f32);
        lanes.disc = w.anchor.as_vec3().extend(w.hole as f32);
        lanes.base = w.base.extend(0.0);
        lanes.lat1 = w.e1.as_vec3().extend(at.skirt as f32);
        lanes.lat2 = w.e2.as_vec3().extend(at.span as f32);
    };
    for handle in [&drawn.far, &drawn.near] {
        if let Some(m) = assets.materials.get_mut(handle) {
            m.extension.centre = w.centre.extend(0.0);
            step(&mut m.extension.lanes);
        }
    }
    for handle in [&drawn.sea, &drawn.shallows] {
        if let Some(m) = assets.seas.get_mut(handle) {
            m.extension.centre = w.centre.extend(m.extension.centre.w);
            step(&mut m.extension.lanes);
        }
    }
}
