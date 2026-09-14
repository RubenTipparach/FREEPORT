//! CPU reference for the ray-marched sky and its ambient-light sampling.

use glam::DVec3;

/// Single-scattering atmosphere parameters, in metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Atmosphere {
    pub planet_radius: f64,
    pub top_radius: f64,
    pub rayleigh_height: f64,
    pub mie_height: f64,
    pub rayleigh: DVec3,
    pub mie: DVec3,
}

impl Atmosphere {
    /// Earth-like scattering scaled for the ten kilometre prototype.
    pub fn prototype() -> Self {
        Self {
            planet_radius: 5_000.0,
            top_radius: 5_480.0,
            rayleigh_height: 120.0,
            mie_height: 45.0,
            rayleigh: DVec3::new(5.8, 13.5, 33.1) * 1.0e-5,
            mie: DVec3::splat(2.1e-4),
        }
    }

    /// Marches view and sun optical depth to produce sky radiance.
    pub fn radiance(self, eye: DVec3, view: DVec3, sun: DVec3, steps: usize) -> DVec3 {
        let view = view.normalize();
        let sun = sun.normalize();
        let Some((near, far)) = shell_segment(eye, view, self.top_radius) else {
            return DVec3::ZERO;
        };
        let start = near.max(0.0);
        let step = (far - start) / steps.max(1) as f64;
        let mut optical = DVec3::ZERO;
        let mut light = DVec3::ZERO;
        for i in 0..steps.max(1) {
            let p = eye + view * (start + (i as f64 + 0.5) * step);
            let height = (p.length() - self.planet_radius).max(0.0);
            let density = DVec3::new(
                (-height / self.rayleigh_height).exp(),
                (-height / self.mie_height).exp(),
                0.0,
            );
            optical += density * step;
            let sun_depth = sun_optical_depth(self, p, sun, 8);
            let extinction =
                self.rayleigh * (optical.x + sun_depth.x) + self.mie * (optical.y + sun_depth.y);
            let mu = view.dot(sun);
            let ray_phase = 3.0 * (1.0 + mu * mu) / (16.0 * std::f64::consts::PI);
            let mie_phase = 0.119 * (1.0 - 0.76) / (1.0 + 0.76 - 2.0 * 0.87 * mu).powf(1.5);
            light += (-extinction).exp()
                * (self.rayleigh * density.x * ray_phase + self.mie * density.y * mie_phase)
                * step;
        }
        light
    }

    /// Samples the same marched sky over the upper hemisphere for ambient light.
    pub fn ambient(self, point: DVec3, sun: DVec3, samples: usize) -> DVec3 {
        let up = point.normalize();
        let helper = if up.y.abs() < 0.9 { DVec3::Y } else { DVec3::X };
        let east = helper.cross(up).normalize();
        let north = up.cross(east);
        let mut sum = DVec3::ZERO;
        for i in 0..samples.max(1) {
            let u = (i as f64 + 0.5) / samples.max(1) as f64;
            let phi = i as f64 * 2.399_963_229_728_653;
            let dir = (east * phi.cos() + north * phi.sin()) * u.sqrt() + up * (1.0 - u).sqrt();
            sum += self.radiance(point + up * 0.1, dir, sun, 12);
        }
        sum / samples.max(1) as f64
    }
}

fn shell_segment(origin: DVec3, dir: DVec3, radius: f64) -> Option<(f64, f64)> {
    let b = origin.dot(dir);
    let c = origin.length_squared() - radius * radius;
    let d = b * b - c;
    (d >= 0.0).then(|| (-b - d.sqrt(), -b + d.sqrt()))
}

fn sun_optical_depth(atmosphere: Atmosphere, point: DVec3, sun: DVec3, steps: usize) -> DVec3 {
    let Some((_, far)) = shell_segment(point, sun, atmosphere.top_radius) else {
        return DVec3::ZERO;
    };
    let step = far.max(0.0) / steps as f64;
    let mut depth = DVec3::ZERO;
    for i in 0..steps {
        let p = point + sun * ((i as f64 + 0.5) * step);
        let h = (p.length() - atmosphere.planet_radius).max(0.0);
        depth += DVec3::new(
            (-h / atmosphere.rayleigh_height).exp(),
            (-h / atmosphere.mie_height).exp(),
            0.0,
        ) * step;
    }
    depth
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sky_and_ambient_are_finite_and_blue_weighted() {
        let a = Atmosphere::prototype();
        let eye = DVec3::Y * 5_002.0;
        let sky = a.radiance(eye, DVec3::X, DVec3::new(1.0, 0.4, 0.0), 24);
        let ambient = a.ambient(eye, DVec3::new(1.0, 0.4, 0.0), 16);
        assert!(sky.is_finite() && ambient.is_finite());
        assert!(sky.z > sky.x && ambient.max_element() > 0.0);
    }
}
