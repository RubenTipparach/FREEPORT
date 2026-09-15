//! Validated numeric settings for the experimental hex planet harness.

use hex_planet::lod::LodBands;

#[derive(Clone, Debug)]
pub struct HexConfig {
    pub cell_radius: f64,
    pub step: f64,
    pub relief: f64,
    pub lumps: f64,
    pub octaves: u32,
    pub seed: u32,
    pub lod: LodBands,
    pub refresh: f64,
    pub far_subdivisions: u32,
    pub fly_speed: f32,
}

impl Default for HexConfig {
    fn default() -> Self {
        Self {
            cell_radius: 12.0,
            step: 0.5,
            relief: 160.0,
            lumps: 12.0,
            octaves: 5,
            seed: 7,
            lod: LodBands::default(),
            refresh: 96.0,
            far_subdivisions: 128,
            fly_speed: 120.0,
        }
    }
}

impl HexConfig {
    pub fn load() -> Result<Self, String> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/config/hex_planet.yaml");
        let source = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        Self::parse(&source)
    }

    fn parse(source: &str) -> Result<Self, String> {
        let mut config = Self::default();
        for line in source.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let (key, raw) = line.split_once(':').ok_or("expected key: number")?;
            let value: f64 = raw.trim().parse().map_err(|_| format!("invalid {key}"))?;
            if !value.is_finite() || value < 0.0 {
                return Err(format!("{key} must be finite and non-negative"));
            }
            let range = match key.trim() {
                "cell_radius" => (2.0, 64.0),
                "step" => (0.1, 4.0),
                "relief" => (1.0, 500.0),
                "lumps" => (1.0, 64.0),
                "octaves" => (1.0, 8.0),
                "seed" => (1.0, u32::MAX as f64),
                "hex_end" | "height_start" => (50.0, 2000.0),
                "refresh" => (1.0, 200.0),
                "far_subdivisions" => (16.0, 256.0),
                "fly_speed" => (1.0, 1000.0),
                _ => return Err(format!("unknown hex setting '{key}'")),
            };
            if value == 0.0 {
                continue;
            }
            if value < range.0 || value > range.1 {
                return Err(format!("{key} must be between {} and {}", range.0, range.1));
            }
            match key.trim() {
                "cell_radius" => config.cell_radius = value,
                "step" => config.step = value,
                "relief" => config.relief = value,
                "lumps" => config.lumps = value,
                "octaves" => config.octaves = integer(key, value)?,
                "seed" => config.seed = integer(key, value)?,
                "hex_end" => config.lod.hex_end = value,
                "height_start" => config.lod.height_start = value,
                "refresh" => config.refresh = value,
                "far_subdivisions" => config.far_subdivisions = integer(key, value)?,
                "fly_speed" => config.fly_speed = value as f32,
                _ => unreachable!(),
            }
        }
        if config.lod.height_start <= config.lod.hex_end || config.refresh >= config.lod.hex_end {
            return Err("hex distances must satisfy refresh < hex_end < height_start".into());
        }
        if config.rings() > 180 {
            return Err(
                "hex patch is too large; increase cell_radius or reduce height_start".into(),
            );
        }
        Ok(config)
    }

    pub fn rings(&self) -> i32 {
        // Extra coverage accounts for tangent projection, motion and cell corners.
        ((self.lod.height_start + self.refresh * 2.0) / self.cell_radius).ceil() as i32
    }
}

fn integer(key: &str, value: f64) -> Result<u32, String> {
    if value.fract() != 0.0 {
        return Err(format!("{key} needs a whole number"));
    }
    Ok(value as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_and_overrides_are_validated() {
        let defaults =
            HexConfig::parse(include_str!("../../../assets/config/hex_planet.yaml")).unwrap();
        assert_eq!(defaults.cell_radius, 12.0);
        assert_eq!(
            HexConfig::parse("cell_radius: 10\nseed: 42").unwrap().seed,
            42
        );
        for invalid in [
            "step: NaN",
            "hex_end: 1500",
            "octaves: 2.5",
            "typo: 0",
            "cell_radius: 2",
        ] {
            assert!(HexConfig::parse(invalid).is_err(), "{invalid}");
        }
    }
}
