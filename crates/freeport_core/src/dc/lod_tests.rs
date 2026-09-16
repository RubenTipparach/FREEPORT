use super::*;
use crate::audit::audit;
use crate::field::Planet;
use crate::lattice::Rings;

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
        sites: vec![],
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
