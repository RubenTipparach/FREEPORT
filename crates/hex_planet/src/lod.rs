//! One distance rule shared by the hex overlay and height-map surface.

/// Visual representation selected for a surface patch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SurfaceLod {
    /// Full extruded cells and their borders.
    Hex,
    /// A blend of cells into the continuous surface.
    Transition,
    /// Continuous displaced sphere only.
    HeightMap,
}

/// Distance thresholds for the hex to height-map transition, in metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LodBands {
    pub hex_end: f64,
    pub height_start: f64,
}

impl Default for LodBands {
    fn default() -> Self {
        Self {
            hex_end: 900.0,
            height_start: 1_300.0,
        }
    }
}

impl LodBands {
    /// Selects the only surface representation needed at `distance`.
    pub fn select(self, distance: f64) -> SurfaceLod {
        if distance <= self.hex_end {
            SurfaceLod::Hex
        } else if distance < self.height_start {
            SurfaceLod::Transition
        } else {
            SurfaceLod::HeightMap
        }
    }

    /// Returns height-map opacity from zero near the eye to one far away.
    pub fn height_weight(self, distance: f64) -> f32 {
        let width = (self.height_start - self.hex_end).max(f64::EPSILON);
        let t = ((distance - self.hex_end) / width).clamp(0.0, 1.0);
        (t * t * (3.0 - 2.0 * t)) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bands_have_no_visibility_gap() {
        let b = LodBands::default();
        assert_eq!(b.select(0.0), SurfaceLod::Hex);
        assert_eq!(b.select(1_000.0), SurfaceLod::Transition);
        assert_eq!(b.select(2_000.0), SurfaceLod::HeightMap);
        assert_eq!(b.height_weight(b.hex_end), 0.0);
        assert_eq!(b.height_weight(b.height_start), 1.0);
    }
}
