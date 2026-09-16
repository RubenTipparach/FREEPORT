//! The contour's tests: closed shells at one level and four, a built
//! planet, the pinch rule, and a coarse cell the fine surface crosses only
//! on a face.

use super::*;
use crate::audit::audit;
use crate::field::{Block, Built, Planet, Sphere, CONCRETE, TERRAIN};
use crate::lattice::{Flat, Rings};
use crate::tables::EDGE_TABLE;

#[test]
fn precomputed_samples_preserve_surface_and_invalid_samples_fall_back() {
    let field = Sphere { radius: 6.0 };
    let lat = Lattice::new(DVec3::splat(-8.125), 0.5);
    let id = ChunkId {
        level: 0,
        at: [0; 3],
    };
    let mut samples = Vec::new();
    for k in -MARGIN..=CH + MARGIN {
        for j in -MARGIN..=CH + MARGIN {
            for i in -MARGIN..=CH + MARGIN {
                samples.push(field.at(lat.point([i, j, k])) as f32);
            }
        }
    }
    let want = contour(&field, &lat, id, &Flat(0));
    for values in [
        samples,
        vec![f32::NAN; (STRIDE * STRIDE * STRIDE) as usize],
        vec![0.0],
    ] {
        let mesh = contour_sampled(&field, &lat, id, &Flat(0), values);
        assert_eq!(want.positions, mesh.positions);
        assert_eq!(want.indices, mesh.indices);
        assert_eq!(want.normals, mesh.normals);
    }
}

/// Every chunk the rings ask for that the field cannot rule out, with
/// triangles in it.
fn contour_rings(field: &dyn Density, lat: &Lattice, rings: &Rings) -> Vec<(DVec3, DcMesh)> {
    let mut out = Vec::new();
    for id in rings.chunks() {
        let (lo, hi) = id.bounds(lat, 0);
        if field.solid(lo, hi).is_some() {
            continue;
        }
        let m = contour(field, lat, id, rings);
        if m.triangles() > 0 {
            out.push((id.corner(lat), m));
        }
    }
    out
}

/// Every level 0 chunk in a cube of `n` a side from `at`.
fn contour_flat(field: &dyn Density, lat: &Lattice, at: i64, n: i64) -> Vec<(DVec3, DcMesh)> {
    let mut out = Vec::new();
    for z in at..at + n {
        for y in at..at + n {
            for x in at..at + n {
                let id = ChunkId {
                    level: 0,
                    at: [x, y, z],
                };
                let m = contour(field, lat, id, &Flat(0));
                if m.triangles() > 0 {
                    out.push((id.corner(lat), m));
                }
            }
        }
    }
    out
}

#[test]
fn every_component_row_partitions_the_crossed_edges() {
    for (config, edges) in EDGE_TABLE.iter().enumerate() {
        let comp = components(config);
        for (e, component) in comp.iter().enumerate() {
            assert_eq!(
                *component >= 0,
                edges & (1 << e) != 0,
                "config {config} edge {e}"
            );
        }
        let count = comp.iter().max().copied().unwrap_or(-1) + 1;
        assert!(
            count <= 4,
            "config {config} exceeds the cell vertex capacity"
        );
        for k in 0..count {
            assert!(comp.contains(&k), "config {config} skips surface {k}");
        }
    }
    let two = components(0b0100_0001);
    assert_eq!(two.iter().max(), Some(&1));
    assert_eq!(two.iter().filter(|&&k| k == 0).count(), 3);
}

#[test]
fn a_sphere_contours_to_a_closed_shell_at_one_level() {
    let ball = Sphere { radius: 6.0 };
    let lat = Lattice::new(DVec3::splat(-8.0 + 0.25), 0.5);
    let chunks = contour_flat(&ball, &lat, 0, 2);
    let a = audit(&ball, &chunks);
    assert_eq!(a.open, 0, "{a:?}");
    assert_eq!(a.non_manifold, 0, "{a:?}");
    assert_eq!(a.facing_in, 0, "{a:?}");
    assert_eq!(a.seams, 0);
    let want = 4.0 * std::f64::consts::PI * 36.0;
    assert!(
        (a.area - want).abs() / want < 0.03,
        "area {} against {want}",
        a.area
    );
    assert!(a.triangles > 1000);
}

#[test]
fn a_sphere_contours_to_a_closed_shell_across_four_levels() {
    // Cells of two and a half centimetres over the top of the ball,
    // then five, ten and twenty, the join at every orientation and at
    // three level boundaries.
    let ball = Sphere { radius: 6.0 };
    let lat = Lattice::new(DVec3::splat(-9.0 + 0.0125), 0.025);
    let rings = Rings::around(&lat, DVec3::new(0.3, 6.0, 0.2), 4);
    let chunks = contour_rings(&ball, &lat, &rings);
    let a = audit(&ball, &chunks);
    assert_eq!(a.open, 0, "{a:?}");
    assert_eq!(a.non_manifold, 0, "{a:?}");
    assert_eq!(a.facing_in, 0, "{a:?}");
    assert_eq!(a.missing, 0);
    assert!(a.seams > 100, "{a:?}");
    let mut per_level = [0usize; 4];
    for (_, m) in &chunks {
        for &l in &m.levels {
            per_level[l as usize] += 1;
        }
    }
    assert!(
        per_level.iter().all(|&n| n > 100),
        "vertices per level {per_level:?}"
    );
    let want = 4.0 * std::f64::consts::PI * 36.0;
    assert!(
        (a.area - want).abs() / want < 0.03,
        "area {} against {want}",
        a.area
    );
}

#[test]
fn a_chunk_holds_its_vertices_to_the_sphere() {
    let ball = Sphere { radius: 6.0 };
    let lat = Lattice::new(DVec3::splat(-9.0 + 0.025), 0.05);
    let rings = Rings::around(&lat, DVec3::new(0.0, 6.0, 0.0), 4);
    for (corner, m) in contour_rings(&ball, &lat, &rings) {
        for ((p, n), level) in m.positions.iter().zip(&m.normals).zip(&m.levels) {
            let w = corner + DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64);
            let r = w.length();
            let cell = lat.cell(*level);
            assert!(
                (r - 6.0).abs() < 0.06 * cell + 0.01,
                "a vertex at radius {r} at level {level}"
            );
            let radial = w / r;
            let nn = DVec3::new(n[0] as f64, n[1] as f64, n[2] as f64);
            assert!(
                radial.dot(nn) > 0.97,
                "a normal {} off radial",
                radial.dot(nn)
            );
        }
    }
}

/// A ball smaller than a coarse cell, centred on the plane where the
/// fine box meets the coarse: the fine chunk sees it and the coarse
/// cell's corners do not, so the seam has no coarse vertex to end on
/// unless one is made from the fine crossings on the cell's faces.
#[test]
fn a_coarse_cell_the_fine_surface_crosses_only_on_a_face_still_closes() {
    let lat = Lattice::new(DVec3::ZERO, 0.05);
    let rings = Rings::around(&lat, DVec3::ZERO, 3);
    let edge = (rings.centre[0][0] + crate::lattice::HALF) as f64 * lat.cell(0) * CH as f64;
    let ball = Sphere { radius: 0.04 };
    let field = Shifted {
        inner: &ball,
        by: DVec3::new(edge, 0.05, 0.05),
    };
    let chunks = contour_rings(&field, &lat, &rings);
    let a = audit(&field, &chunks);
    assert!(a.triangles > 4, "{a:?}");
    assert!(a.seams > 0, "{a:?}");
    assert_eq!(a.missing, 0, "{a:?}");
    assert_eq!(a.open, 0, "{a:?}");
}

struct Shifted<'a> {
    inner: &'a dyn Density,
    by: DVec3,
}

impl Density for Shifted<'_> {
    fn at(&self, p: DVec3) -> f64 {
        self.inner.at(p - self.by)
    }
}

/// A small planet with a slab and a wall built on its top, the rings
/// round the site, as the app draws it.
fn built_planet() -> (Planet, Vec<Block>, Lattice, Rings) {
    let planet = Planet {
        radius: 20.0,
        relief: 2.0,
        lumps: 3.0,
        octaves: 4,
        overhang: 0.6,
        ledge: 3.0,
        seed: 7,
        sites: vec![],
    };
    let top = planet.at(DVec3::new(0.0, 20.0, 0.0)) + 20.0;
    // Half extents along the block's own axes: east, north, up.
    let slab = Block {
        centre: DVec3::new(0.0, top - 0.1, 0.0),
        half: DVec3::new(3.0, 3.0, 0.25),
        axes: [DVec3::X, DVec3::Z, DVec3::Y],
        material: CONCRETE,
    };
    let wall = Block {
        centre: DVec3::new(2.0, top + 1.0, 0.0),
        half: DVec3::new(0.2, 2.5, 1.1),
        axes: [DVec3::X, DVec3::Z, DVec3::Y],
        material: CONCRETE,
    };
    let lat = Lattice::new(DVec3::splat(-24.0 + 0.125), 0.25);
    let rings = Rings::around(&lat, DVec3::new(0.0, top, 0.0), 3);
    (planet, vec![slab, wall], lat, rings)
}

#[test]
fn a_planet_with_a_slab_and_a_wall_is_closed_and_the_slab_is_flat() {
    let (planet, blocks, lat, rings) = built_planet();
    let slab = blocks[0].clone();
    let built = Built {
        ground: &planet,
        blocks: blocks.iter().collect(),
    };
    let chunks = contour_rings(&built, &lat, &rings);
    let a = audit(&built, &chunks);
    assert_eq!(a.open, 0, "{a:?}");
    assert_eq!(a.non_manifold, 0, "{a:?}");
    assert_eq!(a.missing, 0, "{a:?}");
    // A few slivers face in, all of them in the finger's width of gap
    // under the slab's overhanging edge, where its underside and the
    // ground share one cell: a hundredth of a square metre in five
    // thousand, and nothing of any size.
    assert!(a.facing_area < a.area * 1e-4, "{a:?}");
    assert!(a.seams > 100, "{a:?}");
    let top = slab.centre.y + slab.half.z;
    let (mut on_top, mut worst) = (0, 0.0f64);
    for (corner, m) in &chunks {
        for ((p, n), level) in m.positions.iter().zip(&m.normals).zip(&m.levels) {
            let w = *corner + DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64);
            let over = w.x.abs() < slab.half.x - 0.4 && w.z.abs() < slab.half.y - 0.4;
            let up = n[1] > 0.99;
            if *level == 0
                && over
                && up
                && (w.y - top).abs() < 0.3
                && (w.x - 2.0).abs() > 0.6
                && planet.at(w) < -0.05
            {
                on_top += 1;
                worst = worst.max((w.y - top).abs());
            }
        }
    }
    assert!(on_top > 50, "vertices over the slab {on_top}");
    assert!(worst < 0.002, "a slab vertex {worst} m off the plane");
    let (mut concrete, mut terrain) = (0, 0);
    for (corner, m) in &chunks {
        for (t, mat) in m.indices.chunks(3).zip(&m.materials) {
            let mid = t
                .iter()
                .map(|&i| DVec3::from(m.positions[i as usize].map(f64::from)))
                .sum::<DVec3>()
                / 3.0
                + *corner;
            let over = mid.x.abs() < 2.0
                && mid.z.abs() < 2.0
                && (mid.y - top).abs() < 0.1
                && (mid.x - 2.0).abs() > 0.6;
            if over {
                assert_eq!(*mat, CONCRETE, "a slab triangle at {mid}");
                concrete += 1;
            } else if mid.y < top - 3.0 {
                assert_eq!(*mat, TERRAIN, "a ground triangle at {mid}");
                terrain += 1;
            }
        }
    }
    assert!(
        concrete > 50 && terrain > 1000,
        "{concrete} concrete, {terrain} terrain"
    );
}

/// A box face laid exactly on a lattice plane puts its crease on a
/// lattice edge, and the two cells either side of that edge both solve
/// to the same point on the crease: two vertices in one place, which is
/// a pinch. The rule that keeps a build off the lattice is half a fine
/// cell of offset between the build grid and the lattice's corner, and
/// this holds it: the same pad on a lattice it coincides with pinches,
/// and on one offset by half a fine cell it is clean.
#[test]
fn a_face_on_a_lattice_plane_pinches_and_half_a_cell_of_offset_does_not() {
    let ground = Sphere { radius: 5.0 };
    let pad = Block {
        centre: DVec3::new(0.0, 5.0, 0.0),
        half: DVec3::new(1.0, 1.0, 0.3),
        axes: [DVec3::X, DVec3::Z, DVec3::Y],
        material: CONCRETE,
    };
    let built = Built {
        ground: &ground,
        blocks: vec![&pad],
    };
    let mut pinched = Vec::new();
    for offset in [0.0, 0.125] {
        let lat = Lattice::new(DVec3::splat(-8.0 + offset), 0.25);
        let a = audit(&built, &contour_flat(&built, &lat, 0, 4));
        assert_eq!(a.open, 0, "{a:?}");
        pinched.push(a.non_manifold);
    }
    assert!(
        pinched[0] > 0,
        "the pad on the lattice's own planes: {pinched:?}"
    );
    assert_eq!(pinched[1], 0, "the pad half a cell off them: {pinched:?}");
}
