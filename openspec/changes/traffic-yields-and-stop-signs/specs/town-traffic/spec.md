# Spec Delta

## Purpose

A town's traffic stops at signed crossings and takes turns, so its own cars never drive through each other, and it stays a closed form of the clock.

## ADDED Requirements

### Requirement: Town cars never meet inside a crossing
No two cars on a town's rails SHALL be inside the same crossing's square at the same moment.

#### Scenario: A busy crossing over a minute
- **WHEN** every agent of a test town is sampled every tenth of a second for a minute
- **THEN** no crossing ever holds two cars at once

### Requirement: Traffic stops at a stop sign
A car on the rails reaching a signed crossing SHALL stand still at its line for the stop's length before it goes on, and where it is SHALL still be a function of the clock alone.

#### Scenario: A car at a stop sign
- **WHEN** a car's loop passes a signed crossing
- **THEN** its place is constant for the stop's length at the line, and two evaluations at the same time give the same place
