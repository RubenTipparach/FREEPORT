//! Static Blender bakes, validated once and reused for every lot and LOD.
use bevy::math::DVec3;
use bevy::prelude::*;
use freeport_core::dc::DcMesh;
use freeport_core::model::{self, Model, Solid};
use freeport_core::town::{Lot, LOT};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Deserialize)]
struct Manifest {
    schema: u32,
    models: Vec<Entry>,
}
#[derive(Deserialize)]
struct Entry {
    kind: String,
    storeys: u32,
    file: String,
}
#[derive(Deserialize)]
struct Bake {
    schema: u32,
    kind: String,
    storeys: u32,
    width: f64,
    depth: f64,
    solids: Vec<BoxData>,
    lamps: Vec<[f64; 3]>,
    lods: Vec<MeshData>,
}
#[derive(Deserialize)]
struct BoxData {
    centre: [f64; 3],
    half: [f64; 3],
    yaw: f64,
    material: u8,
}
#[derive(Deserialize)]
struct MeshData {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    indices: Vec<u32>,
    materials: Vec<u8>,
}

#[derive(Default)]
pub struct Library(HashMap<(String, u32), [Model; 3]>);

fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    serde_json::from_slice(&std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?)
        .map_err(|e| format!("{}: {e}", path.display()))
}

impl Library {
    pub fn load() -> Self {
        let loaded = crate::terrain::assets_dir()
            .ok_or("assets directory missing".into())
            .and_then(|root| Self::from_dir(&root.join("models/buildings")));
        match loaded {
            Ok(library) => {
                info!(
                    "{} Blender building variants loaded with three LODs",
                    library.0.len()
                );
                library
            }
            Err(e) => {
                warn!("building bakes unavailable ({e}); using procedural fallback; run tools/bake_buildings.py");
                Self::default()
            }
        }
    }

    fn from_dir(dir: &Path) -> Result<Self, String> {
        let manifest: Manifest = read(&dir.join("manifest.json"))?;
        if manifest.schema != 1 {
            return Err("unknown building manifest schema".into());
        }
        let mut library = Self::default();
        for entry in manifest.models {
            let bake: Bake = read(&dir.join(entry.file))?;
            if bake.kind != entry.kind || bake.storeys != entry.storeys {
                return Err("building identity mismatch".into());
            }
            let kind = model::Kind::all()
                .into_iter()
                .find(|k| k.name() == entry.kind)
                .ok_or_else(|| format!("unknown building kind {}", entry.kind))?;
            library
                .0
                .insert((entry.kind, entry.storeys), bake.models(kind)?);
        }
        for kind in model::Kind::all().into_iter().filter(|k| k.baked()) {
            let (lo, hi) = kind.storeys();
            for n in lo..=hi {
                if !library.0.contains_key(&(kind.name().into(), n)) {
                    return Err(format!("missing {}_{n}", kind.name()));
                }
            }
        }
        Ok(library)
    }

    /// The model for a lot, at a LOD, RE-SKINNED into the trade this
    /// particular building is put up in.
    ///
    /// A bake is authored in concrete and there are thirteen of them, so
    /// a skin baked in would mean sixty five; which trade a building is
    /// built in is a fact about the LOT and not about the variant, and
    /// `model::Kind::skin` is the one table that answers it. The
    /// procedural fallback below builds its skin in directly, because it
    /// is making the walls anyway and knows a wall from a floor.
    ///
    /// A bake is ONE lot square, so a lot of two by two (the large
    /// buildings downtown and round the square) is built parametrically
    /// at its own footprint, as is any kind the library does not carry.
    pub fn model(&self, lot: &Lot, lod: usize, seed: u32) -> Model {
        let (lo, hi) = lot.kind.storeys();
        let dice = seed ^ lot.id;
        let baked = ((lot.w - LOT).abs() < 1e-6)
            .then(|| {
                self.0
                    .get(&(lot.kind.name().into(), lot.storeys.clamp(lo, hi)))
            })
            .flatten();
        match baked {
            Some(models) => {
                let mut m = models[lod.min(2)].clone();
                m.reskin(freeport_core::field::CONCRETE, lot.kind.skin(dice));
                m
            }
            None => {
                let w = lot.kind.covers() * lot.w;
                model::building(lot.kind, w, w, lot.storeys, dice)
            }
        }
    }
}

impl Bake {
    /// A bake's three LODs, refused unless it fits the block its kind
    /// COVERS.
    ///
    /// `Kind::covers` is what `town::plot` bounds a lot's own setback by,
    /// so a variant baked wider than its kind claims is a wall standing
    /// in the street with nothing to say so: the plan would leave room
    /// the building does not have. The two are one number and this is
    /// where they are held together.
    fn models(self, kind: model::Kind) -> Result<[Model; 3], String> {
        let covers = kind.covers() * LOT;
        if self.schema != 1
            || self.lods.len() != 3
            || !self.width.is_finite()
            || !self.depth.is_finite()
            || self.width <= 0.0
            || self.depth <= 0.0
            || self.width > covers
            || self.depth > covers
        {
            return Err("invalid building schema, LOD count, or footprint".into());
        }
        let mut solids = Vec::new();
        for b in self.solids {
            let centre = DVec3::from(b.centre);
            let half = DVec3::from(b.half);
            if !centre.is_finite()
                || !half.is_finite()
                || half.min_element() <= 0.0
                || !b.yaw.is_finite()
            {
                return Err("invalid building collider".into());
            }
            solids.push(Solid {
                centre,
                half,
                yaw: b.yaw,
                material: b.material,
            });
        }
        let lamps: Vec<_> = self.lamps.into_iter().map(DVec3::from).collect();
        if lamps.iter().any(|p| !p.is_finite()) {
            return Err("invalid building lamp".into());
        }
        let mut models = Vec::new();
        for mesh in self.lods {
            models.push(Model {
                mesh: mesh.mesh()?,
                solids: solids.clone(),
                lamps: lamps.clone(),
            });
        }
        models
            .try_into()
            .map_err(|_| "expected three building LODs".into())
    }
}

impl MeshData {
    fn mesh(self) -> Result<DcMesh, String> {
        let n = self.positions.len();
        if n == 0
            || self.normals.len() != n
            || !self.indices.len().is_multiple_of(3)
            || self.materials.len() != self.indices.len() / 3
            || self.indices.iter().any(|&i| i as usize >= n)
            || self
                .positions
                .iter()
                .chain(&self.normals)
                .flatten()
                .any(|v| !v.is_finite())
            || self
                .normals
                .iter()
                .any(|&v| Vec3::from(v).length_squared() < 0.5)
            || self.materials.iter().any(|m| !(1..=4).contains(m))
        {
            return Err("invalid baked building mesh".into());
        }
        Ok(DcMesh {
            positions: self.positions,
            normals: self.normals,
            indices: self.indices,
            materials: self.materials,
            levels: vec![0; n],
            ..default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use freeport_core::field::{Density, GLASS};
    #[test]
    fn committed_buildings_have_real_holes_and_reduced_lods() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/models/buildings");
        let library = Library::from_dir(&root).expect("committed Blender library must load");
        for ((kind, _), models) in library.0 {
            assert!(
                models[0].mesh.triangles() > models[2].mesh.triangles(),
                "{kind}"
            );
            assert_eq!(
                models[0].solids, models[2].solids,
                "LOD must not change collision"
            );
            assert!(
                models[0].mesh.materials.contains(&3),
                "{kind} has no glazing"
            );
            assert!(!models[2].mesh.materials.contains(&3));
            let frame = freeport_core::town::Frame {
                dir: DVec3::Z,
                east: DVec3::X,
                north: DVec3::Y,
                base: 0.0,
            };
            let blocks = models[0].blocks(&frame);
            // The front door is clear from outside to the middle of the room.
            for step in 0..=40 {
                let p = DVec3::new(0.0, -7.0 + step as f64 * 0.175, 1.5);
                assert!(
                    blocks.iter().all(|b| b.at(p) <= 0.0),
                    "blocked {kind} door at {p}"
                );
            }
            // A pane occupies an actual opening in the collision wall too.
            for glass in blocks.iter().filter(|b| b.material == GLASS) {
                assert!(
                    blocks
                        .iter()
                        .filter(|b| b.material != GLASS)
                        .all(|b| b.at(glass.centre) < 0.0),
                    "opaque collider behind {kind} glass"
                );
            }
        }
    }
}
