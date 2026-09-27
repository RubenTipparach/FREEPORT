# Design

## Context

`ram.rs` exchanges momentum with a restitution and gives the struck car a spin over a rectangle's inertia, shared by `SPIN_SHARE`; `Driver::board` places a car on the ground `walker::ground` finds.

## Goals / Non-Goals

**Goals:**
- Knocks in proportion; a still boarding.

**Non-Goals:**
- A rigid body simulation.

## Decisions

- Measure first: spin against closing speed over the bot's own rams, and the car's first second after boarding, as core tests.

## Risks / Trade-offs

- Softer knocks make a ram less dramatic; the owner decides the feel from a drive.
