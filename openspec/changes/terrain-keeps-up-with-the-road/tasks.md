# Tasks

## 1. Plan in milliseconds

- [x] 1.1 Cache each chunk's empty ruling in the planner thread, per epoch, with a cap
- [ ] 1.2 A test that a second plan of the same rings asks the field about nothing

## 2. Measure what the owner saw

- [x] 2.1 Record the finest drawn level under the car every frame (`drawn_level` in the bot's report and CSV)
- [ ] 2.2 `tools/bench.py` reads `drawn_level.highway.coarse_share`
- [ ] 2.3 A/B the `trip` errand, before and after, and put the numbers in the commit

## 3. If it is not enough

- [ ] 3.1 Publish the finest levels as they complete while the eye is fast, with the seam rule written first
- [ ] 3.2 Pictures from the highway (`--bot-shots`) the owner has seen
