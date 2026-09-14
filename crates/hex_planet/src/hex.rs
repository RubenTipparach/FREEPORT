//! Camera-local hex patches laid onto a spherical surface.

use glam::{DMat3, DVec2, DVec3};

/// A rendered hex cell in planet-local coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct HexCell {
    pub axial: [i32; 2],
    pub centre: DVec3,
    pub corners: [DVec3; 6],
}

/// Generates the visible axial grid around a surface direction.
///
/// Points are projected from the tangent plane back to the sphere. Keeping
/// the patch camera-local makes density independent from the global Goldberg
/// resolution while the twelve unavoidable pentagons remain in the far mesh.
pub fn visible_patch(up: DVec3, radius: f64, cell_radius: f64, rings: i32) -> Vec<HexCell> {
    let up = up.normalize_or_zero();
    let helper = if up.y.abs() < 0.9 { DVec3::Y } else { DVec3::X };
    let east = helper.cross(up).normalize();
    let north = up.cross(east).normalize();
    let basis = DMat3::from_cols(east, north, up);
    let mut cells = Vec::new();
    for q in -rings..=rings {
        let r0 = (-rings).max(-q - rings);
        let r1 = rings.min(-q + rings);
        for r in r0..=r1 {
            let flat = axial_centre(q, r, cell_radius);
            let centre = project(basis, flat, radius);
            let corners = std::array::from_fn(|side| {
                let angle = std::f64::consts::TAU * side as f64 / 6.0;
                let p = flat + DVec2::new(angle.cos(), angle.sin()) * cell_radius;
                project(basis, p, radius)
            });
            cells.push(HexCell {
                axial: [q, r],
                centre,
                corners,
            });
        }
    }
    cells
}

fn axial_centre(q: i32, r: i32, size: f64) -> DVec2 {
    DVec2::new(1.5 * q as f64, 3.0_f64.sqrt() * (r as f64 + q as f64 * 0.5)) * size
}

fn project(basis: DMat3, p: DVec2, radius: f64) -> DVec3 {
    (basis * DVec3::new(p.x / radius, p.y / radius, 1.0)).normalize() * radius
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_has_expected_hex_count_and_radius() {
        let cells = visible_patch(DVec3::Y, 5_000.0, 4.0, 3);
        assert_eq!(cells.len(), 37);
        for cell in cells {
            assert!((cell.centre.length() - 5_000.0).abs() < 1.0e-9);
        }
    }
}
