//! Planetary level of detail: an icosahedron subdivided per EDGE, so two
//! triangles that share one agree without asking each other.
//!
//! This is sp4cerat's Planet-LOD (`src.simple/Main.cpp`, MIT, credited here as
//! that project asks) ported to `f64` and to this crate's conventions, and it
//! is the reference implementation the compute shader transcribes.
//!
//! The rule is the whole of it. A triangle looks at its three edges; an edge
//! is SPLIT while the eye is nearer to its midpoint than `ratio` times the
//! triangle's size. The decision uses nothing but the edge's own midpoint and
//! the triangle's size, and both triangles either side of an edge see the
//! same midpoint at the same size, so they always decide alike: no
//! T junction can open, with no neighbour lookup, no stitching and no skirt.
//! That is also what makes it parallel: every triangle decides alone, which
//! is why the whole recursion can run in a compute shader with nothing
//! shared between threads.
//!
//! An edge that is not split has its midpoint COLLAPSED onto a corner, which
//! drops one of the four children, so a triangle with one unsplit edge comes
//! out as three triangles and one with three as itself. The far side of the
//! planet is culled by the horizon: a triangle whose every edge midpoint is
//! seen from behind the horizon is not drawn and not recursed into.
//!
//! What is NOT the original: the centre of detail there is the eye's
//! direction, a unit vector, so altitude does not enter and flying up draws
//! as many triangles as standing. Here it is the eye's own position in units
//! of the planet's radius, which is the same thing at the surface and thins
//! the mesh as the eye climbs.

use glam::DVec3;

/// How the recursion is steered.
#[derive(Clone, Copy, Debug)]
pub struct Lod {
    /// An edge splits while the eye is nearer than this times the triangle's
    /// size. Planet-LOD's `lod.ratio`, and its default is one.
    pub ratio: f64,
    /// The size a triangle stops at whatever the eye does, as a share of an
    /// icosahedron edge. Planet-LOD's `detail`.
    pub detail: f64,
    /// Whether to drop what the horizon hides.
    pub cull: bool,
}

impl Default for Lod {
    fn default() -> Self {
        Lod {
            ratio: 1.0,
            detail: 0.01,
            cull: true,
        }
    }
}

impl Lod {
    /// The detail that puts a triangle about `metres` across at the eye on a
    /// planet of `radius`: an icosahedron edge is a little over the radius,
    /// and the size halves a level.
    pub fn for_cell(radius: f64, metres: f64) -> f64 {
        (metres / radius).max(f64::MIN_POSITIVE)
    }
}

/// A triangle to draw: three directions on the unit sphere, wound so that
/// the cross product of its first two edges points out of the planet.
pub type Tri = [DVec3; 3];

/// How far behind the horizon a triangle may be before it is dropped,
/// radians. Planet-LOD's 0.2, which keeps a margin for the relief standing
/// up over the horizon.
const HORIZON: f64 = 0.2;

/// The twelve vertices and twenty faces of a regular icosahedron, the
/// vertices normalised. The face list is Planet-LOD's own, with the two
/// faces it repeats replaced by the ones it drops, so this is a closed
/// solid: every edge is shared by exactly two faces.
pub fn icosahedron() -> ([DVec3; 12], [[usize; 3]; 20]) {
    let t = (1.0 + 5.0_f64.sqrt()) / 2.0;
    let raw = [
        [-1.0, t, 0.0],
        [1.0, t, 0.0],
        [-1.0, -t, 0.0],
        [1.0, -t, 0.0],
        [0.0, -1.0, t],
        [0.0, 1.0, t],
        [0.0, -1.0, -t],
        [0.0, 1.0, -t],
        [t, 0.0, -1.0],
        [t, 0.0, 1.0],
        [-t, 0.0, -1.0],
        [-t, 0.0, 1.0],
    ];
    let mut v = [DVec3::ZERO; 12];
    for (i, r) in raw.iter().enumerate() {
        v[i] = DVec3::new(r[0], r[1], r[2]).normalize();
    }
    let faces = [
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];
    (v, faces)
}

/// Whether a triangle is worth walking into at all: at least one of its edge
/// midpoints stands within the horizon of the eye. `eye` is in units of the
/// planet's radius.
fn in_sight(mids: &[DVec3; 3], eye: DVec3) -> bool {
    let limit = std::f64::consts::FRAC_PI_2 - HORIZON;
    mids.iter().any(|m| {
        let to = *m - eye;
        let len = to.length();
        if len <= 0.0 {
            return true;
        }
        let cos = m.dot(to / len).clamp(-1.0, 1.0);
        cos.acos() >= limit
    })
}

/// One triangle of the recursion: emit it, or walk into the children the
/// edge tests leave.
fn walk(tri: Tri, size: f64, eye: DVec3, lod: &Lod, out: &mut Vec<Tri>, hole: f64, centre: DVec3) {
    let mids = [
        (tri[0] + tri[1]).normalize(),
        (tri[1] + tri[2]).normalize(),
        (tri[2] + tri[0]).normalize(),
    ];
    // The near tier covers the ground inside the hole, so nothing is drawn
    // there: a triangle every corner of which is well inside it is dropped,
    // and one that straddles the rim is kept whole and drawn under the
    // columns that overlap it.
    if hole > 0.0 && tri.iter().all(|p| p.dot(centre) > hole) {
        return;
    }
    if lod.cull && !in_sight(&mids, eye) {
        return;
    }
    // Planet-LOD's own test, per edge: split while the eye is nearer to the
    // midpoint than `ratio` times the size.
    let reach = size * lod.ratio;
    let split = [
        (mids[0] - eye).length() <= reach,
        (mids[1] - eye).length() <= reach,
        (mids[2] - eye).length() <= reach,
    ];
    if (!split[0] && !split[1] && !split[2]) || size < lod.detail {
        out.push(tri);
        return;
    }
    // The four children, with an unsplit edge's midpoint collapsed onto a
    // corner and the child it belonged to dropped.
    let mut p = [tri[0], tri[1], tri[2], mids[0], mids[1], mids[2]];
    let mut valid = [true; 4];
    if !split[0] {
        p[3] = tri[0];
        valid[0] = false;
    }
    if !split[1] {
        p[4] = tri[1];
        valid[2] = false;
    }
    if !split[2] {
        p[5] = tri[2];
        valid[3] = false;
    }
    const IDX: [[usize; 3]; 4] = [[0, 3, 5], [5, 3, 4], [3, 1, 4], [5, 4, 2]];
    for (k, on) in valid.iter().enumerate() {
        if !on {
            continue;
        }
        let child = [p[IDX[k][0]], p[IDX[k][1]], p[IDX[k][2]]];
        walk(child, size * 0.5, eye, lod, out, hole, centre);
    }
}

/// The triangles to draw for an eye at `eye` (planet frame, metres) on a
/// planet of `radius`, as directions on the unit sphere.
///
/// `hole` is the cosine of the angle a near tier covers round `centre`;
/// nought draws the whole planet. A triangle wholly inside the hole is
/// dropped and one across its rim is kept, so whatever draws the near
/// ground overlaps the rim rather than meeting it.
pub fn select(eye: DVec3, radius: f64, lod: &Lod, hole: f64, centre: DVec3) -> Vec<Tri> {
    let (v, faces) = icosahedron();
    let at = eye / radius;
    let mut out = Vec::new();
    for f in faces {
        walk(
            [v[f[0]], v[f[1]], v[f[2]]],
            1.0,
            at,
            lod,
            &mut out,
            hole,
            centre,
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A vertex's name: its bits, so two triangles that computed the same
    /// point name it alike and one a rounding away does not.
    fn key(p: DVec3) -> (u64, u64, u64) {
        (p.x.to_bits(), p.y.to_bits(), p.z.to_bits())
    }

    fn edges(tris: &[Tri]) -> HashMap<((u64, u64, u64), (u64, u64, u64)), usize> {
        let mut counts = HashMap::new();
        for t in tris {
            for k in 0..3 {
                let (a, b) = (key(t[k]), key(t[(k + 1) % 3]));
                let e = if a <= b { (a, b) } else { (b, a) };
                *counts.entry(e).or_insert(0) += 1;
            }
        }
        counts
    }

    #[test]
    fn the_icosahedron_is_closed_and_every_vertex_is_on_the_sphere() {
        let (v, faces) = icosahedron();
        for p in v {
            assert!((p.length() - 1.0).abs() < 1e-12, "vertex off the sphere");
        }
        let tris: Vec<Tri> = faces.iter().map(|f| [v[f[0]], v[f[1]], v[f[2]]]).collect();
        for (_, n) in edges(&tris) {
            assert_eq!(n, 2, "an icosahedron edge is shared by two faces");
        }
    }

    #[test]
    fn the_mesh_is_closed_wherever_the_eye_is() {
        let lod = Lod {
            ratio: 8.0,
            detail: 0.002,
            cull: false,
        };
        for eye in [
            DVec3::new(0.0, 0.0, 1.0),
            DVec3::new(0.6, 0.5, 0.62),
            DVec3::new(-0.2, 0.97, 0.1),
            DVec3::new(1.0, 1.0, 1.0).normalize(),
        ] {
            let tris = select(eye * 1000.0, 1000.0, &lod, 0.0, DVec3::Z);
            assert!(tris.len() > 1000, "{} triangles is too few", tris.len());
            let counts = edges(&tris);
            let open = counts.values().filter(|n| **n != 2).count();
            assert_eq!(open, 0, "{open} edges are not shared by two triangles");
        }
    }

    #[test]
    fn a_higher_eye_draws_fewer_triangles() {
        let lod = Lod::default();
        let dir = DVec3::new(0.3, 0.8, 0.5).normalize();
        let near = select(dir * 1000.0, 1000.0, &lod, 0.0, dir).len();
        let far = select(dir * 4000.0, 1000.0, &lod, 0.0, dir).len();
        assert!(
            far < near,
            "{far} triangles from four radii out against {near} at the surface"
        );
    }

    #[test]
    fn detail_is_where_the_eye_is() {
        let lod = Lod {
            ratio: 1.0,
            detail: 0.004,
            cull: false,
        };
        let dir = DVec3::new(0.1, 0.2, 0.97).normalize();
        let tris = select(dir * 1000.0, 1000.0, &lod, 0.0, dir);
        let side = |t: &Tri| (t[1] - t[0]).length();
        let near = tris
            .iter()
            .filter(|t| t[0].dot(dir) > 0.999)
            .map(side)
            .fold(f64::INFINITY, f64::min);
        let away = tris
            .iter()
            .filter(|t| t[0].dot(dir) < -0.5)
            .map(side)
            .fold(0.0, f64::max);
        assert!(
            near * 8.0 < away,
            "the triangles under the eye are {near} and the far side's {away}"
        );
    }

    #[test]
    fn the_horizon_drops_the_far_side() {
        let dir = DVec3::Z;
        let seen = Lod::default();
        let all = Lod {
            cull: false,
            ..Lod::default()
        };
        let culled = select(dir * 1000.0, 1000.0, &seen, 0.0, dir);
        let whole = select(dir * 1000.0, 1000.0, &all, 0.0, dir);
        assert!(
            culled.len() * 4 < whole.len() * 3,
            "the horizon dropped {} of {}",
            whole.len() - culled.len(),
            whole.len()
        );
        for t in &culled {
            let c = (t[0] + t[1] + t[2]).normalize();
            assert!(
                c.dot(dir) > -0.6,
                "a triangle well round the back was drawn"
            );
        }
    }

    #[test]
    fn a_hole_leaves_the_rim_and_nothing_inside_it() {
        let dir = DVec3::new(0.2, 0.1, 0.97).normalize();
        let lod = Lod {
            ratio: 1.0,
            detail: 0.004,
            cull: false,
        };
        let hole = (0.02_f64).cos();
        let tris = select(dir * 1000.0, 1000.0, &lod, hole, dir);
        for t in &tris {
            assert!(
                !t.iter().all(|p| p.dot(dir) > hole),
                "a triangle inside the hole was drawn"
            );
        }
        // And the rim is covered: some triangle reaches into the hole.
        assert!(
            tris.iter().any(|t| t.iter().any(|p| p.dot(dir) > hole)),
            "nothing overlaps the rim"
        );
    }

    #[test]
    fn a_tighter_ratio_draws_more() {
        let dir = DVec3::Z;
        let loose = select(
            dir * 1000.0,
            1000.0,
            &Lod {
                ratio: 0.5,
                ..Lod::default()
            },
            0.0,
            dir,
        );
        let tight = select(
            dir * 1000.0,
            1000.0,
            &Lod {
                ratio: 2.0,
                ..Lod::default()
            },
            0.0,
            dir,
        );
        assert!(
            tight.len() > loose.len(),
            "{} against {}",
            tight.len(),
            loose.len()
        );
    }

    #[test]
    fn a_cell_in_metres_is_a_share_of_the_radius() {
        assert!((Lod::for_cell(5000.0, 5.0) - 0.001).abs() < 1e-12);
    }
}

/// What the recursion costs, printed rather than asserted: the count is set
/// by `ratio` and hardly at all by `detail`, because the refinement is a
/// funnel and a finer stop only lengthens it. `cargo test -p freeport_core
/// mesh_sizes -- --ignored --nocapture`.
#[cfg(test)]
mod sizes {
    use super::*;
    #[test]
    #[ignore]
    fn mesh_sizes() {
        let dir = DVec3::Z;
        for ratio in [1.0, 2.0, 4.0, 8.0, 16.0, 32.0] {
            for detail in [0.001, 0.0002] {
                let lod = Lod {
                    ratio,
                    detail,
                    cull: true,
                };
                let n = select(dir * 1000.0, 1000.0, &lod, 0.0, dir).len();
                let up = select(dir * 3000.0, 1000.0, &lod, 0.0, dir).len();
                println!("ratio {ratio} detail {detail}: {n} on the ground, {up} at three radii");
            }
        }
    }

    /// What a planet's SIZE costs the far tier: the leaf count and the
    /// walk's own time at four radii, from a metre planetoid's worth to a
    /// thousand kilometres. The answer the harness rests on is that the
    /// count does not move, because the split test is an ANGLE and an
    /// angle has no metres in it; what moves is how deep the recursion
    /// goes to reach the same detail on the ground, and that is a
    /// logarithm.
    #[test]
    #[ignore]
    fn the_cost_of_a_bigger_planet() {
        let dir = DVec3::new(0.3, 0.7, 0.64).normalize();
        let lod = Lod {
            ratio: 6.0,
            detail: 0.0,
            cull: true,
        };
        for radius in [5.0e3, 5.0e4, 2.0e5, 1.0e6] {
            let eye = dir * (radius + 12.0);
            let clock = std::time::Instant::now();
            let leaves = select(eye, radius, &lod, 0.0, dir);
            let took = clock.elapsed().as_secs_f64() * 1e3;
            // How small the finest leaf came out, metres along an edge.
            let finest = leaves
                .iter()
                .map(|t| (t[0] - t[1]).length() * radius)
                .fold(f64::INFINITY, f64::min);
            // An icosahedron edge halved until it is that small.
            let depth = (1.107_148_7 * radius / finest).log2();
            println!(
                "radius {:>9.0} m: {} leaves in {:.2} ms, finest {:.2} m, {:.1} levels deep",
                radius,
                leaves.len(),
                took,
                finest,
                depth
            );
        }
    }
}
