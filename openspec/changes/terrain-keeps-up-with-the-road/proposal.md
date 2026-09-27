# Proposal

## Why

The owner watched the bot drive the highway at 160 km/h and saw the terrain fall behind the road: "highest level LOD build time causing ground to overlap with roads". The streamer plans a whole layout on a worker, builds every chunk of it hidden and swaps it in at once, and while it builds the rings do not follow the eye. On the `trip` errand each layout took 2.5 to 3.1 s just to plan, so the car covered 150 m or more on a layout built for where it had been: past the finest ring (about 128 m at driving speed) and onto cells coarse enough to stand over the road. The errand ended with 1,440 chunks built and never shown.

## What Changes

- The planner remembers, per chunk, whether the field ruled it empty. The field is fixed once the world is built, so a new layout asks only about the chunks it did not have before.
- The bot records, every frame, the finest level of ground actually DRAWN under the car, and the report gives its median, its worst and the share of frames at a 16 m cell or coarser (`drawn_level`), which is the owner's complaint as a number.
- If planning alone does not close it, the next step is publishing the finest rings as they complete while the eye is moving fast, instead of waiting for the whole layout.

## Capabilities

### New Capabilities
- `terrain-streaming`: how quickly the drawn ground follows a moving eye, and what the ground under a car is drawn at.

### Modified Capabilities

## Impact

`crates/freeport_app/src/stream.rs`, `stream/planning.rs`, the bot's report (`bot/report.rs`) and `tools/bench.py`'s metrics.
