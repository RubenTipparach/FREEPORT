# Design

## Context

`Auto::drive` steers by pure pursuit to an aim, keeps its speed to `Auto::room` for traffic and backs off when wedged.

## Goals / Non-Goals

**Goals:**
- Walls seen before they are met.

**Non-Goals:**
- Path planning round a whole block, which the street graph already does.

## Decisions

- Sample the car's field at a few points ahead along its heading and either side at its half width, over the stopping distance, and turn the nearest solid into a speed limit and a steering bias.
- Count wall contacts in the bot's report, so the sensor is measured on every errand.

## Risks / Trade-offs

- Sampling costs main thread time per sub step; it is measured with the bot and kept inside the frame's budget.
