# Tasks

## 1. Find them

- [x] 1.1 Measure coincident faces on the meshes rather than in pictures: `model/tests/coplanar.rs` for every parametric kind and every street piece, `tools/coplanar.py` for the baked library. The old bake had 63 to 98 visible coplanar pairs in every variant but the towers (a floor slab's sides in the walls' planes, gable ends reaching into the ceiling, a vault springing from the walls' own faces); the parametric kinds had pillars flush with the walls and gables overhanging into the next lot's roof
- [ ] 1.2 Rule out shadow acne by rendering with shadows off
- [x] 1.3 Check no tile draws two levels of detail in one frame: read `city/detail.rs`, where a tile's block is hidden or made `ShadowOnly` in the same command batch that shows its grade, and `district::follow` swaps a district and its blocks in one batch too
- [x] 1.4 Where things MEET: a whole laid town through `fights::in_town`, parametric in the core and baked in the app. Found 113 road markings within a millimetre of their tarmac on graded ground, 78 gable eave wedges in the plane of the wall behind, and 4,563 baked window frames back to back across party walls
- [x] 1.5 A camera moved 4 mm between two frames at the port's first lot and 15 m back: no difference before and after there, because those views hold none of the fixed pairs; the measure is the meshes

## 2. Fix them

- [x] 2.1 Separate each coincident pair found: floor slabs `INSET` from the walls, pillars `PROUD` of them and trim, gable ends flush with the lot, the vault and the gable reach `0.02` inside the walls, and the library re-baked
- [x] 2.3 `PAINT_UP` 4 mm to 1.5 cm; parametric gable eave wedges `INSET`; baked frames 1 cm shy of the street face; baked vault ends 3 cm in; re-baked: nought fights in the port at every grade
- [ ] 2.2 Before and after pictures the owner has seen
