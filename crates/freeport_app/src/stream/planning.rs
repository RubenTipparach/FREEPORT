//! Cull and order a complete, immutable ring layout away from the render thread.

use super::*;
use std::sync::mpsc::{channel, sync_channel, SyncSender};

pub(super) struct Request {
    pub epoch: u64,
    pub lat: Lattice,
    pub rings: Rings,
    pub eye: DVec3,
    pub world: Arc<World>,
}

pub(super) struct Layout {
    pub epoch: u64,
    pub wanted: HashMap<ChunkId, u64>,
    pub nearest: Vec<ChunkId>,
    pub ms: f32,
}

pub(super) struct Planner {
    requests: SyncSender<Request>,
    results: Mutex<Receiver<Layout>>,
}

impl Planner {
    pub fn new() -> Self {
        let (requests, incoming) = sync_channel::<Request>(1);
        let (results, finished) = channel();
        std::thread::spawn(move || {
            let mut known = Known::default();
            while let Ok(request) = incoming.recv() {
                if results.send(request.build(&mut known)).is_err() {
                    break;
                }
            }
        });
        Self {
            requests,
            results: Mutex::new(finished),
        }
    }

    pub fn request(&self, request: Request) -> bool {
        self.requests.try_send(request).is_ok()
    }

    pub fn poll(&self) -> Option<Layout> {
        self.results.lock().ok()?.try_recv().ok()
    }
}

/// Which chunks the field has already been asked about, and whether each
/// was EMPTY: wholly rock or air in the ground, and no sea through it.
///
/// The field is fixed once the world is built (a town arriving changes
/// the models on it and never the ground), so the answer for a chunk
/// never changes, and a layout whose boxes moved by a row wants the new
/// row asked about and nothing else. Asked again for every chunk of every
/// level, a layout was 2.5 to 3.1 s of a worker at 160 km/h on the
/// highway: the car covered 150 m in it, out past the finest ring the
/// layout on screen had been built for, onto cells coarse enough to stand
/// over the road. Kept per EPOCH, because a body change is another field.
#[derive(Default)]
pub(super) struct Known {
    epoch: u64,
    empty: HashMap<ChunkId, bool>,
}

/// How many answers are kept before the oldest body of them is let go,
/// which is a few minutes of fast travel and a few megabytes.
const KNOWN_MOST: usize = 400_000;

impl Request {
    pub fn build(self, known: &mut Known) -> Layout {
        let started = Instant::now();
        let ground = self.world.ground();
        let water = self.world.water(&ground);
        if known.epoch != self.epoch || known.empty.len() > KNOWN_MOST {
            known.empty.clear();
            known.epoch = self.epoch;
        }
        let mut wanted = HashMap::new();
        let mut ordered = Vec::new();
        for id in self.rings.chunks() {
            let empty = *known.empty.entry(id).or_insert_with(|| {
                let (lo, hi) = id.bounds(&self.lat, 0);
                ground.solid(lo, hi).is_some() && water.solid(lo, hi).is_some()
            });
            if empty {
                continue;
            }
            wanted.insert(id, self.rings.signature(id));
            let centre = id.corner(&self.lat) + DVec3::splat(id.size(&self.lat) * 0.5);
            ordered.push(((centre - self.eye).length_squared(), id));
        }
        ordered.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        Layout {
            epoch: self.epoch,
            wanted,
            nearest: ordered.into_iter().map(|(_, id)| id).collect(),
            ms: started.elapsed().as_secs_f32() * 1000.0,
        }
    }
}
