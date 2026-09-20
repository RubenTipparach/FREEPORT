use super::*;
use crate::audit::audit;
use crate::field::Planet;
use crate::lattice::Rings;

/// Box culling never removes a chunk the full mesher would have put
/// polygons in, on the hardest ground there is: a town's own skirt.
///
/// The site's level is the GROUND at its middle less a stride, not a
/// round nought. A site cuts and never fills, so one whose level stands
/// over the land does nothing at all, and this body's ground at `Y` is
/// fifteen hundred metres under nought: the fixture's town was a level
/// floating a kilometre and a half over an untouched hillside, and every
/// chunk the rings held was ordinary terrain the culling cannot rule
/// either way.
#[test]
fn local_culling_preserves_owned_apron_polygons_near_town_skirts() {
    let bare = Planet {
        radius: 1_000_000.0,
        relief: 8_000.0,
        lumps: 12.0,
        octaves: 18,
        overhang: 3.0,
        ledge: 12.0,
        ..Planet::default()
    };
    let planet = Planet {
        sites: vec![crate::town::Site::round(
            DVec3::Y,
            bare.shape().height(DVec3::Y) - 2.0,
            172.0,
        )]
        .into(),
        ..bare.clone()
    };
    let lat = Lattice::new(DVec3::splat(-2_000_099.75), 0.5);
    let mut tested = 0;
    for dir in [
        DVec3::Y,
        DVec3::new(87.0, planet.radius, 3.0).normalize(),
        DVec3::new(200.0, planet.radius, 8.0).normalize(),
    ] {
        let eye = dir * crate::town::surface_radius(&planet, dir);
        let rings = Rings::around(&lat, eye, 4);
        let mut culled: Vec<_> = rings
            .chunks()
            .into_iter()
            .filter(|id| {
                let (lo, hi) = id.bounds(&lat, 0);
                let (near, far) = crate::field::box_radii(lo, hi);
                let (floor, top) = planet.band();
                // Only boxes the old planet-wide band retained, beside a coarser
                // neighbor. These exercise the newly culled seam owners.
                let sig = rings.signature(*id);
                near <= top
                    && far >= floor
                    && planet.solid(lo, hi).is_some()
                    && (0..26).any(|i| (sig >> (i * 2)) & 3 == 2)
            })
            .collect();
        culled.sort();
        for id in culled.into_iter().take(24) {
            // contour does not call solid: this is the full original mesher,
            // including its apron and coarse seam cells, without box culling.
            let reference = contour(&planet, &lat, id, &rings);
            assert_eq!(reference.triangles(), 0, "culled seam owner {id:?}");
            tested += 1;
        }
    }
    assert!(
        tested >= 12,
        "not enough newly culled seam owners: {tested}"
    );
}

#[test]
fn rough_planet_lod_boundaries_have_no_open_edges() {
    let planet = Planet {
        radius: 20.0,
        relief: 4.0,
        lumps: 6.0,
        octaves: 6,
        overhang: 1.5,
        ledge: 2.0,
        seed: 7,
        sites: vec![].into(),
    };
    let lat = Lattice::new(DVec3::splat(-40.0625), 0.125);
    for eye in [DVec3::new(0.3, 20.0, 0.2), DVec3::new(11.0, 12.0, 13.0)] {
        let rings = Rings::around(&lat, eye, 4);
        let chunks: Vec<_> = rings
            .chunks()
            .into_iter()
            .filter_map(|id| {
                let mesh = contour(&planet, &lat, id, &rings);
                (mesh.triangles() > 0).then_some((id.corner(&lat), mesh))
            })
            .collect();
        let result = audit(&planet, &chunks);
        // This checks closure, not manifoldness: the current coarse component
        // grouping can pinch when two fine crossings share an uncut coarse edge.
        println!("eye {eye}: {} triangles, {} seam polygons, {} open edges, {} missing corners, {} non-manifold edges",
            result.triangles, result.seams, result.open, result.missing, result.non_manifold);
        assert_eq!(result.open, 0, "{result:?}");
        assert_eq!(result.missing, 0, "{result:?}");
    }
}
