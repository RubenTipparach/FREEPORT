# Spec Delta

## Purpose

How quickly the drawn terrain follows a moving eye, so that the ground a car drives on is drawn at a cell fine enough for the road to stay on top of it.

## ADDED Requirements

### Requirement: The ground under a car at speed is drawn fine enough to keep the road on top
While a car drives a highway at the car's top speed, the terrain drawn under it SHALL be at a cell of 8 m or finer on at least 95% of frames of the bot's `trip` errand.

#### Scenario: The trip errand on the highway
- **WHEN** the bot's `trip` errand runs in real time and the car is on the highway
- **THEN** the report's `drawn_level.highway.coarse_share` is under 0.05

### Requirement: Planning a new layout never re-asks the field about a chunk it has already ruled
The streamer SHALL rule each chunk wholly rock, wholly air or neither at most once per body, because the field does not change after the world is built.

#### Scenario: A layout that moved by one row
- **WHEN** the rings move by one chunk at a level and a new layout is planned
- **THEN** only chunks not ruled before are tested against the field, and the plan takes milliseconds rather than seconds
