# Design

## Context

`Circuit::at(s)` places an agent `s` metres round its loop; `Traffic::at` takes `s = phase + speed * time`. Nothing is stored per agent but its loop, speed and phase.

## Goals / Non-Goals

**Goals:**
- Stops and turn taking without state.

**Non-Goals:**
- Traffic reacting to the player's car, which needs state.

## Decisions

- Distance round the loop becomes a function of time with flat steps at each signed crossing: time maps to distance through the cumulative stop schedule, which is still closed form.
- Turn taking by the clock: a crossing's arms get alternating windows of the clock, and an agent's phase is chosen so it arrives in its own arm's window.

## Risks / Trade-offs

- A closed form cannot queue: two cars stopping at one sign must be spaced by phase, which constrains how phases are hashed.
