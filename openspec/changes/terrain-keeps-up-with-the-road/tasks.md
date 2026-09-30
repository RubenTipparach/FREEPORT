# Tasks

## 1. Plan in milliseconds

- [x] 1.1 Cache each chunk's empty ruling in the planner thread, per epoch, with a cap
- [ ] 1.2 A test that a second plan of the same rings asks the field about nothing

## 2. Measure what the owner saw

- [x] 2.1 Record the finest drawn level under the car every frame (`drawn_level` in the bot's report and CSV)
- [x] 2.2 `tools/bench.py` reads `drawn_level.highway.coarse_share`
- [x] 2.3 A/B the `trip` errand, before and after, and put the numbers in the commit: 41.7% of highway frames under coarse ground, 10.2% with the cache, 0.9% with `lattice::DWELL` at eight seconds

## 3. It was not enough: the town exit

- [x] 3.1 Find what the 0.9% hid: the bot filed the drive out of a city under `streets` (a disc of `OUTLINE` radii), and there the car ran on 16 m cells for 17 to 28 s at 160 km/h, layouts growing 3 s, 7 s, 10 s
- [x] 3.2 Measure the churn (`churn_at_speed`): 6 to 7 chunks a metre of travel, half of them rebuilt for a neighbour's level
- [x] 3.3 The finest ring outlasts four of the streamer's own measured builds (`Rings::OUTLAST`, `HOLD`): nought fast frames at level 5 or coarser against 1,043 to 1,663
- [x] 3.4 Label `streets` by the town's own levelled edge (`Site::level_r`)
- [x] 3.5 Draw the tarmac half a per cent of its range nearer the eye (`roads::PULL`) for the coarse cells 0.5 to 4 km out that stand over the road even when streaming keeps up
- [ ] 3.6 Publish the finest levels as they complete while the eye is fast, with the seam rule written first (not needed now; kept in case)
- [ ] 3.7 Pictures from the highway (`--bot-shots`) the owner has seen
