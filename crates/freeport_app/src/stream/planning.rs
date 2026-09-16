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
            while let Ok(request) = incoming.recv() {
                if results.send(request.build()).is_err() {
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

impl Request {
    pub fn build(self) -> Layout {
        let started = Instant::now();
        let ground = self.world.ground();
        let water = self.world.water(&ground);
        let mut wanted = HashMap::new();
        let mut ordered = Vec::new();
        for id in self.rings.chunks() {
            let (lo, hi) = id.bounds(&self.lat, 0);
            if ground.solid(lo, hi).is_some() && water.solid(lo, hi).is_some() {
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
