# Design

## Context

`stream.rs` plans on a worker (`stream/planning.rs`), builds hidden, and publishes a whole layout at once so no seam is ever left open. While a layout builds, `want` returns early and the rings stay where they were planned.

## Goals / Non-Goals

**Goals:**
- Plan a moved layout in milliseconds.
- Keep the car on ground drawn at 8 m cells or finer on the highway.

**Non-Goals:**
- Changing the atomic publish's seam guarantee.
- Changing what a chunk's mesh is.

## Decisions

- Cache the empty ruling per `ChunkId` in the planner thread, per epoch, capped at 400,000 entries. A ruling is a pure function of the fixed field and the chunk's bounds, so the cache cannot go stale inside an epoch.
- Measure with the bot: `Streamer::drawn_level` is the finest published level whose chunk holds the car's ground point.
- If the cache is not enough: publish levels finest first as each level's chunks and its seam neighbours complete, while pace is high.

## Risks / Trade-offs

- Memory: a few megabytes of cache on a long drive; cleared at the cap.
- Publishing per level would weaken the one-swap guarantee and needs the seam rule written down before it is tried.
