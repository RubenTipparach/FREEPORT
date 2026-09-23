# The mockups

A mockup before a large feature, which is this project's rule: a new
screen, a mechanic's feel, a mesher or a plan is rendered here in a
browser, published as an artifact, approved, and then built. Each page
opens straight off the checkout and is republished in the same change as
the file it was made from, so the link and the file never disagree.

| page | what it decided | published |
| --- | --- | --- |
| `marching-cubes.html` | the terrain: a density field dual contoured, with towns built of brushes in it (what the game did and does not now) | https://claude.ai/code/artifact/342b7f52-5a1f-4000-94b3-1d3967b527d1 |
| `hex-terrain.html` | the world that was set aside: Goldberg columns and a planet tessellation on the GPU | https://claude.ai/code/artifact/87905fd8-d47b-4f2e-8cc3-8c226251a799 |
| `road-mound.html` | a highway on its own embankment, the mound drawn from the field it stands on | https://claude.ai/artifact/FMimcpVizLa3yttJYdBPtd |
| `city-blocks.html` | a city block is four by four lots, downtown and the square are two by two buildings, the highways merge into the town's edge, against six real plans off OpenStreetMap (`city-grids.js`, fetched by `tools/fetch_city_grids.py`) | https://claude.ai/artifact/CamvaxSusg8UXXRYb3UUfW |
| `driving-hud.html` | the driver's HUD (speed, fuel as range, wallet, next pump, frame counter, a compass strip to the first marker, no key legend and G only as a prompt at a pump) over a moving road, and the MAP: a plan of the roads, towns and pumps where a click sets numbered route markers and the legs are summed against the tank; A* between markers is the feature after it | https://claude.ai/artifact/2i1EWdyyK5pkHbxB1KH16P |

`common.js` is the field shared with `field.rs` line for line, `kit.js` the
brush kit the marched page's buildings are made of, `mc_tables.js` the
marching cubes tables extracted from the same source as `tables.rs`, and
`mockup.css` the one stylesheet. `CLAUDE.md` at the root is the record of
what each page found and why the game took what it took.
