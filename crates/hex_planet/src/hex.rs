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
    HexGrid::new(up, radius, cell_radius)
        .map(|grid| grid.patch([0, 0], rings))
        .unwrap_or_default()
}

/// An anchored tangent grid. Moving the visible window keeps existing cells fixed.
#[derive(Clone, Debug)]
pub struct HexGrid {
    basis: DMat3,
    radius: f64,
    size: f64,
}

impl HexGrid {
    /// Builds a grid, rejecting non-finite, zero or negative dimensions.
    pub fn new(up: DVec3, radius: f64, cell_radius: f64) -> Option<Self> {
        let up = up.try_normalize()?;
        if !radius.is_finite() || !cell_radius.is_finite() || radius <= 0.0 || cell_radius <= 0.0 {
            return None;
        }
        let helper = if up.y.abs() < 0.9 { DVec3::Y } else { DVec3::X };
        let east = helper.cross(up).normalize();
        let north = up.cross(east).normalize();
        Some(Self {
            basis: DMat3::from_cols(east, north, up),
            radius,
            size: cell_radius,
        })
    }

    /// The cell at an axial address, independent of the visible window.
    pub fn cell(&self, axial: [i32; 2]) -> HexCell {
        let flat = axial_centre(axial[0], axial[1], self.size);
        HexCell {
            axial,
            centre: project(self.basis, flat, self.radius),
            corners: std::array::from_fn(|side| {
                let angle = std::f64::consts::TAU * side as f64 / 6.0;
                let p = flat + DVec2::new(angle.cos(), angle.sin()) * self.size;
                project(self.basis, p, self.radius)
            }),
        }
    }

    /// Finds the cell under a direction in the front half of this tangent chart.
    /// Cube-coordinate rounding keeps all six corners owned by a nearest cell.
    pub fn locate(&self, direction: DVec3) -> Option<[i32; 2]> {
        let local = self.basis.transpose() * direction.try_normalize()?;
        if local.z <= 0.25 {
            return None;
        }
        let p = local.truncate() * (self.radius / (local.z * self.size));
        let q = p.x * (2.0 / 3.0);
        let r = p.y / 3.0_f64.sqrt() - q * 0.5;
        let s = -q - r;
        let (mut rq, mut rr, rs) = (q.round(), r.round(), s.round());
        let delta = DVec3::new((rq - q).abs(), (rr - r).abs(), (rs - s).abs());
        if delta.x > delta.y && delta.x > delta.z {
            rq = -rr - rs;
        } else if delta.y > delta.z {
            rr = -rq - rs;
        }
        Some([rq as i32, rr as i32])
    }

    /// The visible window around an axial address, with stable shared corners.
    pub fn patch(&self, around: [i32; 2], rings: i32) -> Vec<HexCell> {
        let mut cells = Vec::new();
        for q in -rings..=rings {
            for r in (-rings).max(-q - rings)..=rings.min(-q + rings) {
                cells.push(self.cell([around[0] + q, around[1] + r]));
            }
        }
        cells
    }
}

impl HexCell {
    /// Intersects a radial ray with this cell's flat cap at `height` metres
    /// from the planet centre. The renderer and walker use this same plane.
    pub fn cap_radius(&self, direction: DVec3, height: f64) -> f64 {
        let normal = self.centre.normalize();
        height / direction.normalize_or(normal).dot(normal).max(f64::EPSILON)
    }
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

    #[test]
    fn moving_the_window_preserves_cells_and_inverse_addresses() {
        let grid = HexGrid::new(DVec3::Y, 5_000.0, 12.0).unwrap();
        for cell in grid.patch([4, -2], 3) {
            assert_eq!(cell, grid.cell(cell.axial));
            assert_eq!(grid.locate(cell.centre), Some(cell.axial));
            for corner in cell.corners {
                let inside = corner.lerp(cell.centre, 0.01);
                assert_eq!(grid.locate(inside), Some(cell.axial));
            }
        }
        assert!(grid.locate(DVec3::NEG_Y).is_none());
        assert!(grid.locate(DVec3::ZERO).is_none());
    }

    #[test]
    fn cap_vertices_and_collision_share_a_plane() {
        let grid = HexGrid::new(DVec3::Y, 5_000.0, 12.0).unwrap();
        let cell = grid.cell([12, -3]);
        let normal = cell.centre.normalize();
        for corner in cell.corners {
            let dir = corner.normalize();
            let point = dir * cell.cap_radius(dir, 5_013.5);
            assert!((point.dot(normal) - 5_013.5).abs() < 1e-9);
        }
    }

    #[test]
    fn invalid_grids_do_not_produce_nan_meshes() {
        assert!(HexGrid::new(DVec3::ZERO, 5_000.0, 4.0).is_none());
        assert!(HexGrid::new(DVec3::NAN, 5_000.0, 4.0).is_none());
        assert!(visible_patch(DVec3::Y, f64::NAN, 4.0, 3).is_empty());
        assert!(visible_patch(DVec3::Y, 5_000.0, 0.0, 3).is_empty());
    }
}
