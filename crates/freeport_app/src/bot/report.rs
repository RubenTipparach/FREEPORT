//! What the bot writes down: the report `tools/bench.py` reads, and one
//! line a frame for a graph and for finding what a spike was.

use super::{Bot, Context, Row};
use crate::flight_bench::distribution;
use std::fmt::Write as _;

/// The coarsest ground a road is still drawn over rather than under:
/// level 5 is a 16 m cell, where `examples/lod_over_road` measured half of
/// a road's centreline buried in the terrain a chunk that coarse draws. A
/// frame with the ground under the car this coarse or coarser, or none
/// drawn at all, is a frame the owner sees the ground overlapping the road.
const COARSE: u8 = 5;

/// What the bot was doing, in the order the report lists it.
const LABELS: [&str; 4] = ["walk", "hail", "streets", "highway"];

impl Bot {
    /// Everything measured, as the JSON `tools/bench.py` reads.
    pub(super) fn report(&self, c: &Context) -> serde_json::Value {
        let rows: Vec<&Row> = self
            .rows
            .iter()
            .filter(|r| !matches!(r.label, "settle" | "done" | ""))
            .collect();
        let by = |label: Option<&str>, of: fn(&Row) -> f64| {
            let v: Vec<f64> = rows
                .iter()
                .filter(|r| label.is_none_or(|l| r.label == l))
                .map(|r| of(r))
                .collect();
            if v.is_empty() {
                serde_json::Value::Null
            } else {
                distribution(&v)
            }
        };
        let spread = |of: fn(&Row) -> f64| {
            let mut map = serde_json::Map::new();
            map.insert("all".into(), by(None, of));
            for l in LABELS {
                map.insert(l.into(), by(Some(l), of));
            }
            serde_json::Value::Object(map)
        };
        let wall_s = |l: &str| -> f64 {
            rows.iter()
                .filter(|r| r.label == l)
                .map(|r| r.wall_ms)
                .sum::<f64>()
                / 1e3
        };
        let window = c.windows.single().ok();
        // `tools/bench.py` tells a binary with the bot from one without by
        // the `wall_seconds_by_phase` key's own literal: renamed, it has to
        // be renamed there too (`BOT_MARK`).
        serde_json::json!({
            "outcome": self.outcome, "goal_town": self.goal.map(|g| g.0),
            "goal_km_at_boarding": self.goal.map(|g| g.1 / 1000.0),
            "to_goal_km_at_end": rows.last().and_then(|r| r.to_goal).map(|d| d / 1000.0),
            "sim_seconds": self.sim, "settle_seconds": self.settled_s,
            "wall_seconds": self.walking_from.map(|t| t.elapsed().as_secs_f64()),
            "wall_seconds_by_phase": LABELS.iter()
                .map(|l| (l.to_string(), serde_json::Value::from(wall_s(l))))
                .collect::<serde_json::Map<_, _>>(),
            "walked_m": self.walked, "driven_m": self.driven, "stuck": self.stuck,
            "phases": self.marks.iter().map(|(p, s)| serde_json::json!([p, s])).collect::<Vec<_>>(),
            "frames": spread(|r| r.wall_ms), "update_cpu": spread(|r| r.update_ms),
            "drawn_level": drawn(&rows),
            "drive": if c.args.bot_fast { "a second a frame" } else { "a sixtieth a frame" },
            "terrain": c.streamer.as_ref().map(|s| s.measurement()), "towns": c.fabric.drawing,
            "gpu": c.adapter.name, "backend": format!("{:?}", c.adapter.backend),
            "resolution": window.map(|w| [w.physical_width(), w.physical_height()]),
            "window_focused_at_finish": window.map(|w| w.focused),
            "levels": c.args.levels, "compute_batch": c.tuning.terrain_compute_batch,
            "cell_size_m": c.args.cell_size.unwrap_or(c.tuning.terrain_cell_size),
            "end": c.eye.0 .0.to_array(), "simulation_hz": 60,
        })
    }

    /// One line a frame, for a graph and for finding what a spike was.
    pub(super) fn csv(&self) -> String {
        let mut out = String::from(
            "frame,label,wall_ms,update_ms,speed_mps,to_goal_m,loaded,pending,drawn_level\n",
        );
        for (k, r) in self.rows.iter().enumerate() {
            let goal = r.to_goal.map_or(String::new(), |d| format!("{d:.1}"));
            let drawn = r.drawn.map_or(String::new(), |l| l.to_string());
            let _ = writeln!(
                out,
                "{k},{},{:.3},{:.3},{:.2},{goal},{},{},{drawn}",
                r.label, r.wall_ms, r.update_ms, r.speed, r.loaded, r.pending
            );
        }
        out
    }
}

/// The finest level of ground DRAWN under the bot, by what it was doing:
/// the median, the coarsest, and the share of frames at `COARSE` or worse
/// (or with nothing drawn there), which is the terrain not having caught up
/// with the eye.
fn drawn(rows: &[&Row]) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for l in LABELS {
        let levels: Vec<Option<u8>> = rows
            .iter()
            .filter(|r| r.label == l)
            .map(|r| r.drawn)
            .collect();
        if levels.is_empty() {
            continue;
        }
        let mut known: Vec<u8> = levels.iter().flatten().copied().collect();
        known.sort_unstable();
        let coarse = levels
            .iter()
            .filter(|d| d.is_none_or(|d| d >= COARSE))
            .count();
        map.insert(
            l.into(),
            serde_json::json!({
                "p50": known.get(known.len() / 2), "max": known.last(),
                "coarse_share": coarse as f64 / levels.len() as f64,
                "undrawn_frames": levels.iter().filter(|d| d.is_none()).count(),
            }),
        );
    }
    serde_json::Value::Object(map)
}
