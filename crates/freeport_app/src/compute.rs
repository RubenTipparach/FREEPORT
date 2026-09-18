//! Batched GPU density sampling. Readback waits release the device's locks so
//! the sampling worker cannot block render submission or presentation.

use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderAdapterInfo, RenderDevice, RenderQueue};
use bytemuck::{Pod, Zeroable};
use freeport_core::biome;
use freeport_core::dc::SAMPLE_STRIDE;
use freeport_core::field::{Density, Planet};
use freeport_core::lattice::{ChunkId, Lattice, MARGIN};

pub const BATCH: usize = 8;
const POINTS: usize = (SAMPLE_STRIDE * SAMPLE_STRIDE * SAMPLE_STRIDE) as usize;

#[derive(Resource, Default)]
pub struct Compute(pub Option<Sampler>);

pub fn init_compute(
    mut commands: Commands,
    args: Res<crate::Args>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    adapter: Res<RenderAdapterInfo>,
    tuning: Res<crate::tuning::Tuning>,
) {
    let supported = device.limits().max_compute_workgroups_per_dimension > 0
        && device.limits().max_storage_buffer_binding_size as usize >= BATCH * POINTS * 64
        && device.limits().max_storage_buffers_per_shader_stage >= 2;
    let enabled = supported && !args.cpu_terrain && format!("{:?}", adapter.device_type) != "Cpu";
    info!(
        "terrain sampling: {}",
        if enabled { "GPU compute" } else { "CPU" }
    );
    commands.insert_resource(Compute(enabled.then(|| {
        let mut sampler = Sampler::new(device.clone(), queue.clone());
        sampler.batch_limit = tuning.terrain_compute_batch.clamp(1, BATCH);
        sampler
    })));
}

/// The relief's constants as the sampler's uniform reads them: five
/// `vec4<f32>`, three `vec4<u32>` and the shelf's own `vec4<f32>` last,
/// which is the order `sampling.wgsl` declares them in. The core cannot
/// hand these over as bytes, because it depends on nothing but `std` and
/// `glam`, so the one place they are laid out is here.
const SHAPE_WORDS: usize = 36;

fn shape_words(g: &biome::Gpu) -> [u32; SHAPE_WORDS] {
    let mut w = [0u32; SHAPE_WORDS];
    let floats = [g.shares, g.freqs, g.channel, g.belt, g.fbm];
    for (i, v) in floats.iter().flatten().enumerate() {
        w[i] = v.to_bits();
    }
    w[20..24].copy_from_slice(&g.octaves);
    w[24..28].copy_from_slice(&g.salts);
    w[28..32].copy_from_slice(&g.hills);
    for (i, v) in g.shelf.iter().enumerate() {
        w[32 + i] = v.to_bits();
    }
    w
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Point {
    relief_hi: [f32; 4],
    relief_lo: [f32; 4],
    carve_hi: [f32; 4],
    carve_lo: [f32; 4],
}

fn split(p: DVec3, hi_w: f64, lo_w: f64) -> ([f32; 4], [f32; 4]) {
    let hi = p.as_vec3();
    (
        hi.extend(hi_w as f32).to_array(),
        (p - hi.as_dvec3()).as_vec3().extend(lo_w as f32).to_array(),
    )
}

impl Point {
    fn new(planet: &Planet, p: DVec3) -> Self {
        let r = p.length();
        if r == 0.0 {
            let mut point = Self::zeroed();
            point.relief_hi[3] = planet.radius as f32;
            return point;
        }
        let dir = p / r;
        let (bias, keep) = planet.surface_blend(dir);
        // The low lane carries KEEP rather than an amplitude now: the
        // sampler works the relief's own terms out itself, and every one
        // of them is scaled by how much of the relief this direction has
        // left after its town site is levelled.
        let (relief_hi, relief_lo) = split(dir * planet.lumps, planet.radius - r + bias, keep);
        let (carve_hi, carve_lo) = if planet.ledge > 0.0 && planet.overhang > 0.0 {
            split(p / planet.ledge, planet.overhang * keep, 0.0)
        } else {
            ([0.0; 4], [0.0; 4])
        };
        Self {
            relief_hi,
            relief_lo,
            carve_hi,
            carve_lo,
        }
    }
}

/// The planet as this chunk sees it: the same body with only the town
/// sites whose levelling can reach into it.
fn local_planet(planet: &Planet, lat: &Lattice, id: ChunkId) -> Planet {
    let (lo, hi) = id.bounds(lat, MARGIN);
    let centre = (lo + hi) * 0.5;
    let radius = centre.length();
    if radius <= 0.0 || !radius.is_finite() {
        return planet.clone();
    }
    // The chord a chunk's own corners can be from its middle's direction,
    // which is its half diagonal over the radius it stands at, and a
    // little over rather than under.
    let span = (hi - lo).length() * 0.5 / radius + 1e-12;
    planet.around(centre / radius, span)
}

fn point_at(lat: &Lattice, id: ChunkId, i: usize) -> DVec3 {
    let stride = SAMPLE_STRIDE as usize;
    let xyz = [i % stride, i / stride % stride, i / (stride * stride)];
    let base = id.f0();
    lat.point(std::array::from_fn(|a| {
        base[a] + (xyz[a] as i64 - MARGIN) * id.scale()
    }))
}

pub struct Sampler {
    pub batch_limit: usize,
    device: RenderDevice,
    queue: RenderQueue,
    pipeline: ComputePipeline,
    bind_group: BindGroup,
    input: Buffer,
    output: Buffer,
    readback: Buffer,
    settings: Buffer,
    shape: Buffer,
}

impl Sampler {
    pub fn new(device: RenderDevice, queue: RenderQueue) -> Self {
        let shader = device.create_and_validate_shader_module(ShaderModuleDescriptor {
            label: Some("terrain density sampling"),
            source: ShaderSource::Wgsl(include_str!("sampling.wgsl").into()),
        });
        let pipeline = device.create_compute_pipeline(&RawComputePipelineDescriptor {
            label: Some("terrain density sampling"),
            layout: None,
            module: &shader,
            entry_point: Some("sample"),
            compilation_options: default(),
            cache: None,
        });
        let buffer = |label, size, usage| {
            device.create_buffer(&BufferDescriptor {
                label: Some(label),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        let count = (BATCH * POINTS) as u64;
        let input = buffer(
            "density points",
            count * 64,
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );
        let output = buffer(
            "densities",
            count * 4,
            BufferUsages::STORAGE | BufferUsages::COPY_SRC,
        );
        let readback = buffer(
            "density readback",
            count * 4,
            BufferUsages::MAP_READ | BufferUsages::COPY_DST,
        );
        let settings = buffer(
            "density settings",
            16,
            BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        );
        // The relief's own constants, so `sampling.wgsl` carries the shape
        // of the function and nothing that could drift from the core's.
        let shape = buffer(
            "relief shape",
            SHAPE_WORDS as u64 * 4,
            BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        );
        let layout = BindGroupLayout::from(pipeline.get_bind_group_layout(0));
        let bind_group = device.create_bind_group(
            "density inputs",
            &layout,
            &[
                BindGroupEntry {
                    binding: 0,
                    resource: input.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: output.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: settings.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: shape.as_entire_binding(),
                },
            ],
        );
        Self {
            batch_limit: BATCH,
            device,
            queue,
            pipeline,
            bind_group,
            input,
            output,
            readback,
            settings,
            shape,
        }
    }

    /// One bounded dispatch and readback for several chunks. These are signed
    /// contour samples: the GPU stops once the remaining octaves cannot change
    /// a sign, so their magnitudes need not equal the full field. All coordinates
    /// come from the common fine lattice, even at mixed LOD boundaries.
    pub fn sample(
        &self,
        planet: &Planet,
        chunks: &[(Lattice, ChunkId)],
    ) -> Result<Vec<Vec<f32>>, String> {
        if chunks.is_empty() || chunks.len() > BATCH {
            return Err("invalid density batch".into());
        }
        // One filter of the planet's towns per CHUNK rather than per
        // sample point: every point in a chunk is within its own few
        // metres of every other, so the sites that can level any of them
        // are the same short list.
        let points: Vec<_> = chunks
            .iter()
            .flat_map(|(lat, id)| {
                let local = local_planet(planet, lat, *id);
                (0..POINTS).map(move |i| Point::new(&local, point_at(lat, *id, i)))
            })
            .collect();
        let tolerance = 0.002 + (planet.relief.abs() + planet.overhang.abs()) * 0.000002;
        let settings = [
            planet.seed,
            planet.octaves.max(1),
            points.len() as u32,
            (tolerance as f32).to_bits(),
        ];
        self.queue
            .write_buffer(&self.input, 0, bytemuck::cast_slice(&points));
        self.queue
            .write_buffer(&self.settings, 0, bytemuck::cast_slice(&settings));
        self.queue.write_buffer(
            &self.shape,
            0,
            bytemuck::cast_slice(&shape_words(&planet.shape().gpu())),
        );
        let bytes = (points.len() * 4) as u64;
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor::default());
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups((points.len() as u32).div_ceil(64), 1, 1);
        }
        encoder.copy_buffer_to_buffer(&self.output, 0, &self.readback, 0, bytes);
        self.queue.submit([encoder.finish()]);
        let slice = self.readback.slice(..bytes);
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        slice.map_async(MapMode::Read, move |result| {
            let _ = send.send(result);
        });
        if let Err(error) = self.wait_readback(receive) {
            // The worker switches to CPU after an error. Destroy also aborts
            // any pending map without leaving this failed buffer reusable.
            self.readback.destroy();
            return Err(error);
        }
        let mapped = slice.get_mapped_range();
        let mut values: Vec<Vec<f32>> = bytemuck::cast_slice::<u8, f32>(&mapped)
            .chunks_exact(POINTS)
            .map(|v| v.to_vec())
            .collect();
        drop(mapped);
        self.readback.unmap();
        // Near zero, even a small f32 noise error could change topology. Ask
        // the reference field for those signs; root finding remains f64 too.
        for ((lat, id), samples) in chunks.iter().zip(&mut values) {
            for (i, v) in samples.iter_mut().enumerate() {
                if !v.is_finite() || (*v as f64).abs() < tolerance {
                    *v = planet.at(point_at(lat, *id, i)) as f32;
                }
            }
        }
        Ok(values)
    }

    fn wait_readback(
        &self,
        receive: std::sync::mpsc::Receiver<Result<(), BufferAsyncError>>,
    ) -> Result<(), String> {
        use std::sync::mpsc::TryRecvError;
        use std::time::{Duration, Instant};
        let started = Instant::now();
        loop {
            // wgpu 27's blocking poll holds the shared device fence and
            // snatch locks across the GPU wait. Queue::submit and present
            // need their write locks even when poll runs on another thread.
            // Poll only completed work, then wait without any GPU lock held.
            self.device
                .poll(PollType::Poll)
                .map_err(|e| e.to_string())?;
            match receive.try_recv() {
                Ok(result) => return result.map_err(|e| e.to_string()),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    return Err("GPU density readback callback disconnected".into());
                }
            }
            let remaining = Duration::from_secs(15)
                .checked_sub(started.elapsed())
                .ok_or_else(|| "GPU density readback timed out after 15 seconds".to_owned())?;
            // Rust's Windows sleep uses a high resolution waitable timer.
            // Channel timeouts use WaitOnAddress and rounded a 1 ms wait to
            // about 16 ms here. A short sleep avoids both that delay and spin.
            std::thread::sleep(remaining.min(Duration::from_micros(250)));
        }
    }
}

#[cfg(test)]
mod tests;
