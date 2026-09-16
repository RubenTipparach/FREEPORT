//! Eight independent integer corner hashes, with scalar f64 interpolation.

use glam::DVec3;
use std::arch::x86_64::*;

/// The caller must check AVX2 support. Integer arithmetic is exactly the
/// scalar mix3 sequence; interpolation retains the reference's operation order.
#[target_feature(enable = "avx2")]
pub(super) unsafe fn noise3(p: DVec3, seed: u32) -> f64 {
    let f = p.floor();
    let (x, y, z) = (f.x as i64 as i32, f.y as i64 as i32, f.z as i64 as i32);
    let vx = _mm256_add_epi32(
        _mm256_set1_epi32(x),
        _mm256_setr_epi32(0, 1, 0, 1, 0, 1, 0, 1),
    );
    let vy = _mm256_add_epi32(
        _mm256_set1_epi32(y),
        _mm256_setr_epi32(0, 0, 1, 1, 0, 0, 1, 1),
    );
    let vz = _mm256_add_epi32(
        _mm256_set1_epi32(z),
        _mm256_setr_epi32(0, 0, 0, 0, 1, 1, 1, 1),
    );
    let mut h = _mm256_xor_si256(
        _mm256_mullo_epi32(vx, _mm256_set1_epi32(0x8DA6_B343u32 as i32)),
        _mm256_mullo_epi32(vy, _mm256_set1_epi32(0xD816_3841u32 as i32)),
    );
    h = _mm256_xor_si256(
        h,
        _mm256_mullo_epi32(vz, _mm256_set1_epi32(0xCB1A_B31Fu32 as i32)),
    );
    h = _mm256_xor_si256(h, _mm256_set1_epi32(seed.wrapping_mul(0x9E37_79B9) as i32));
    h = _mm256_xor_si256(h, _mm256_srli_epi32::<15>(h));
    h = _mm256_mullo_epi32(h, _mm256_set1_epi32(0x2C1B_3C6D));
    h = _mm256_xor_si256(h, _mm256_srli_epi32::<12>(h));
    h = _mm256_mullo_epi32(h, _mm256_set1_epi32(0x297A_2D39));
    h = _mm256_xor_si256(h, _mm256_srli_epi32::<15>(h));
    let mut corners = [0u32; 8];
    _mm256_storeu_si256(corners.as_mut_ptr().cast(), h);
    let c = corners.map(|v| v as f64 / 4_294_967_296.0);
    let t = p - f;
    let (tx, ty, tz) = (super::smooth(t.x), super::smooth(t.y), super::smooth(t.z));
    let lerp = |a: f64, b: f64, t: f64| a + (b - a) * t;
    let x00 = lerp(c[0], c[1], tx);
    let x10 = lerp(c[2], c[3], tx);
    let x01 = lerp(c[4], c[5], tx);
    let x11 = lerp(c[6], c[7], tx);
    lerp(lerp(x00, x10, ty), lerp(x01, x11, ty), tz)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::hint::black_box;
    use std::time::Instant;

    fn points() -> Vec<DVec3> {
        (0..8192)
            .map(|i| {
                let a = i as f64 - 4096.0;
                DVec3::new(a * 0.139 - 1e-9, a * -1234.567, a * 981723.23456)
            })
            .collect()
    }

    #[test]
    fn avx2_hashes_keep_every_f64_noise_bit() {
        if !std::arch::is_x86_feature_detected!("avx2") {
            return;
        }
        for seed in [0, 7, 100, u32::MAX] {
            for p in points() {
                let scalar = super::super::noise3_scalar(p, seed);
                let vector = unsafe { noise3(p, seed) };
                assert_eq!(scalar.to_bits(), vector.to_bits(), "at {p:?}, seed {seed}");
            }
        }
    }

    #[test]
    #[ignore = "CPU timing benchmark; run explicitly with --release --ignored --nocapture"]
    fn measure_noise_kernels() {
        let points = points();
        for (name, noise) in [
            (
                "scalar",
                super::super::noise3_scalar as fn(DVec3, u32) -> f64,
            ),
            ("dispatch", super::super::noise3),
        ] {
            let started = Instant::now();
            let mut sum = 0.0;
            for seed in 0..128 {
                for &p in &points {
                    sum += black_box(noise)(black_box(p), seed);
                }
            }
            println!(
                "{name}: {:.2} ms, checksum {}",
                started.elapsed().as_secs_f64() * 1000.0,
                black_box(sum)
            );
        }
    }
}
