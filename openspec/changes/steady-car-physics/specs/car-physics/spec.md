# Spec Delta

## Purpose

A car answers a knock in proportion to it and sits still when it is boarded.

## ADDED Requirements

### Requirement: A small knock is a small turn
A knock with a closing speed under 3 m/s SHALL turn the struck car by less than a quarter turn before its tyres take the spin back.

#### Scenario: A nudge at a junction
- **WHEN** a car closes on another's flank at 2 m/s
- **THEN** the struck car turns by less than 90 degrees in total

### Requirement: Boarding a car does not move it
A car taken off the rails or re-boarded SHALL not move by more than a centimetre in its first second with nothing on the pedals.

#### Scenario: The bot takes a car
- **WHEN** the bot boards a car
- **THEN** the car's place over its first second with nothing pressed changes by under a centimetre
