# PROGRESS — Grand Theft Freeway-BX

The implementation tracker for [`PLAN.md`](PLAN.md). Update it as work lands: tick tasks, fill the
Definition of Done evidence, log decisions and open issues. Keep "Now" current — it is what a
resumed or compacted session reads first.

## Now
- **Phase (2026-10-17, end of session 1):** M0-M2 done. M3, M4 and M5 are partly done; the next
  session continues them in the order of `HANDOFF.md`. Read `HANDOFF.md` first, then this file.
- **Landed since M2:** the on-foot camera (shoulder, walls, recentre, sprint FOV, aim and R3
  shoulder swap); the radio skeleton (live stations from `radio.toml`, procedural songs, the
  `audio_capture` mix); E9 vehicles v2 in the engine (free pitch and roll, tyres, damage to fire
  and explosions, seats and doors, kinematic traffic mode, helicopter flight, water) with all 15
  archetypes in `vehicles.toml` and the feel test; pedestrians (archetypes in `peds.toml`,
  population manager, walk-tile routes, crossings on the walk light, flee and cower, E11 low
  detail); ambience beds by place, crowd and weather; on-foot combat (weapons in `weapons.toml`,
  hitscan, aim assist and magnetism, melee combos, vitals, deaths); E7 night glow for prop parts
  (street lamps light the street at night) and mesh detail by screen size.
- **Perf:** `gtf perf view=all` passes every budget with the crowd (downtown_street 0.24 M
  triangles, sim 0.99 ms; hills_vista 0.43 M). Night downtown: `out/final/downtown_night.png`.
- **Tests:** pav_core 305, pav_render 5, pav_view 43, pav_world 9, pav_audio 10, pav_ui 18,
  pav_app 25, pav_tools 119, gtf_game 44, all green; clippy -D warnings clean; WGSL in Tint.
- **Web:** boots into the city in headless Chromium (`out/web-m2/fixed_*`); `scripts/web-check.mjs`
  keeps its console log when a page crashes and takes `--shot-timeout`.
- **Helpers:** none running. Both worktrees are merged and their build caches removed.
- **Disk:** a fixed allowance (~39 GB in all); a workspace build plus tests needs ~8 GB. Prune
  duplicate artefacts (keep the newest per crate) and never run workspace-wide test builds in
  helper worktrees.
- **Branch / PR:** session 1 merged its draft PR into `main` at the user's request; a new session
  works on its own designated branch and opens a new draft PR.

## Milestones (PLAN.md §9 has the full task lists and acceptance criteria)
- [x] **M0 Bootstrap** — engine vendored; baseline tests; E1 registry, `pav_tools::cli`, `pav_app`
      lib split, `gtf_game` + `gtf_app`; `freeway` scene in every target; web boot check.
- [x] **M1 Platform** — E2 input v2, E3 hook v2, E4 presentation data, E5 `pav_ui` + headless HUD
      capture, E6 camera intent + draw list, E13 app shell, E8 part 1 (bank + loops), UI gallery,
      FPS readout.
- [x] **M2 City** — E14 world regions first (region provider, static meshes, speed-aware streaming,
      far LOD, unload policies); `pav_world` generator + graphs; city props/materials; E7 sky, time of
      day + weather, window kernel, CPU region culling, merged statics; district + dev district +
      views; `citymap`, `lanes`, `perf`.
- [ ] **M3 Player and vehicles** — on-foot controller, camera modes, E9 vehicles v2 (pitch/roll
      freed with stabilization), enter/exit, carjack, damage, helicopter, vehicle audio, GPS ribbon,
      radio skeleton, `vehicle_feel` incl. slopes and jumps.
      *Done:* on-foot controller (sprint, crouch; the engine's jump, vault, climb, swim), on-foot
      and aim cameras, E9 vehicles v2 + 15 archetypes + feel ranges test (all 13 road archetypes
      in range), GPS ribbon, radio skeleton. *Left:* `gtf_vehicles` body props, game-side
      enter/exit/carjack/locked cars, driving input and the vehicle chase/cinematic cameras,
      vehicle audio loops from `vehicle_status`, the `vehicle_feel` tool, helicopter controls,
      captures and filmstrips.
- [ ] **M4 Population** — traffic, pedestrians, population manager, LOD, buses, parked cars,
      ambience, soaks.
      *Done:* pedestrians (archetypes, population manager, crossings, flee/cower, E11 low
      detail), ambience beds. *Left:* traffic agents, buses, parked cars, scenario points,
      `traffic`/`peds`/`soak` tools.
- [ ] **M5 Combat and heat** — weapons, cover, vitals, enemy AI, wanted system, police AI, air unit,
      busted/wasted.
      *Done:* on-foot weapons (hitscan, aim assist, recoil, spread, reload, melee), vitals,
      people's hit reactions and deaths. *Left:* grenades, cover, held weapon props, enemy AI,
      heat and police, Busted/Wasted.
- [ ] **M6 Structure** — mission DSL + runtime + bots, protagonists + switching + abilities, phone,
      economy and shops, save/load.
- [ ] **M7 Content** — final district, landmarks, interiors, all vehicles and props, missions m1–m3,
      s1–s3, h0, h1 (Loud + Smart), activities, radio stations, completion.
- [ ] **M8 Frontend and attract** — title, menus, settings, About, loading cards, attract director,
      touch = attract only.
- [ ] **M9 Polish** — art, animation, audio mix, UX against the 10-foot rules, accessibility,
      performance, bug bash.
- [ ] **M10 Release** — builds (native, Windows `.exe`, `dist/GrandTheftFreeway-BX.html`), docs,
      final verification, delivery.

## Definition of Done evidence (PLAN.md §2)
| Item | Status | Evidence (test, command + result, capture looked at, artifact) |
|---|---|---|
| D1 Builds and checks | ☐ | |
| D2 The district | ☐ | |
| D3 Player | ☐ | |
| D4 Living city | ☐ | |
| D5 Heat | ☐ | |
| D6 Missions and activities | ☐ | |
| D7 UI | ☐ | |
| D8 Frontend | ☐ | |
| D9 Attract mode | ☐ | |
| D10 Audio | ☐ | |
| D11 Visuals (incl. FPS readout) | ☐ | |
| D12 Performance | ☐ | |
| D13 Verification | ☐ | |
| D14 Docs | ☐ | |
| D15 Delivery | ☐ | |
| D16 Developer demo level | ☐ | |

## Decisions
- 2026-10-17 — Vehicles (E9): okami_kaze is AWD (the plan's example said RWD; RWD could not make
  3.8 s 0-100); holding the brake when nearly stopped reverses; an empty car with no input holds
  still with its brakes on; the engine explodes a burning car itself after 5 s; the engine demo's
  cars got free pitch and roll, the new tyres and softer springs (the drift room's guide updated).
- 2026-10-17 — People go low detail (E11: no character controller, one ground ray) beyond 30 m
  and back within 24 m: full detail out to 60 m cost 3.85 ms a tick downtown in the debug build;
  30 m gives 1.07 ms. The walk-map A* keeps its costs in flat arrays reused between searches.
- 2026-10-17 — Pedestrian density: a cap of 60 within 120 m, shared out by the zones around the
  player and the hour (night share 0.35); a new scene fills at once, in view too, from 15 m out;
  reference views centre their crowd on the camera.
- 2026-10-17 — Aim assist judges its 12° cone by heading (35° of pitch allowed) because the
  on-foot camera looks down at the player; with nobody in the cone, L2 levels the view to chest
  height ahead.
- 2026-10-17 — The format-2 field guide cap moved 7,200 -> 7,350 chars for the new `glow` field;
  `audio_capture`'s help was trimmed so `tools/list` stays under 48,000.
- 2026-10-17 — Aim camera at 2.0 m behind and 0.65 m right of the shoulder, its target at head height (PLAN §5.6.2 said 1.6 m / 0.6 m): at 1.6 m the stand-in body filled half the frame (`out/m3/cam_aim.png` vs `cam_aim2.png`). The on-foot camera's walls are five rays from the pivot (a stand-in for a sphere cast; the engine has rays only).
- 2026-10-17 — **Name:** *Grand Theft Freeway-BX* (title #25 of 50 drawn with `secrets.randbelow`,
  ID "BX" from `secrets.choice` over A–Z0–9; PLAN.md §3).
- 2026-10-17 — **Engine in the repo as a vendored copy** of upstream `0098196` minus
  `pavilion-lite/` (not a submodule or subtree), paths unchanged so engine scripts and tests work;
  engine README/CLAUDE moved to `docs/ENGINE_*.md`; pin recorded in `docs/ENGINE_UPSTREAM.md`.
- 2026-10-17 — **Game in its own crates** (`gtf_game` sim-only, `gtf_app` binaries) registered at
  runtime (E1), so game edits don't rebuild `pav_tools`/`pav_app`.
- 2026-10-17 — **Presentation as data** through the hook (E4): UI model, draw list, camera intent,
  view intent, audio scene, app requests; engine crates draw and play them.
- 2026-10-17 — **Sim-side camera** (deterministic, visible to captures and bots); the view only
  interpolates and adds cosmetic shake.
- 2026-10-17 — **UI on egui** through a new `pav_ui` 10-foot kit with headless capture (works native
  and web; HTML/CSS UI would be web-only).
- 2026-10-17 — **New engine crates** `pav_ui` and `pav_world` (urban open-world kit) instead of
  growing `pav_core` or putting reusable city systems in the game crate.
- 2026-10-17 — **Model identifiers stay out of repository files** (environment rule); README, About
  screen and HTML give the lab and effort levels and point to the chat report for the exact model.
- 2026-10-17 — **Rewind** is a dev tool: off in release, web and touch builds, ~10 s in dev tools.
- 2026-10-17 — **Planning review applied.** An independent review raised 27 findings; the key ones
  were verified in the engine source and folded into PLAN.md: E14 (game scenes run with streaming
  off, streaming is hard-wired to the pavilion, the view draws only loaded regions), E9 frees vehicle
  pitch/roll (cars are locked to yaw today), a character LOD mode, CPU region culling into the
  global batches instead of per-region buffers, the `docs_limits.rs` caps (MAKE_A_GAME.md < 16 KiB),
  prop publish `pack=` and `gtf` material embedding, game tools through `pav_tools::cli::main_with`,
  `until:` polling for browser checks, D16 (developer demo level), and touch devices showing only
  the gear button.

- 2026-10-17 — **Paloma Bay is 64 × 72 cells** (1024 × 1152 m): 8 extra rows of hills to the north
  give Mirador Drive, Lookout Drive and the PALOMA sign room (the plan's sketch had 5 rows).
- 2026-10-17 — **Terrain is a painted height grid** beside the map (one character per cell, 4 m
  steps) smoothed into 4 m samples; flat cells stay slabs so the city keeps sidewalks and markings.
  Hill roads are grid streets flattened across (switchbacks of straight runs), not splines.
- 2026-10-17 — **Route 404** is a swept deck (30.4 m, three lanes each way) at 11 m; ramps land
  on one-cell stubs off the cross streets (no frontage roads, so nothing drives under a low ramp);
  the corridor's ground sits at road level (`level = -0.15`).
- 2026-10-17 — **The Ditch** keeps the channel as its own road network (road roles never join
  channel and street cells); crossing roads keep their own characters and become bridges.
- 2026-10-17 — **Walking is a 2 m tile map** (sidewalk, crossing tied to its junction's signal,
  path, open ground) instead of a hand-built node graph: every loop, corner and crosswalk is one
  connected surface, A* over tiles for longer trips. Cover points come from building walls facing
  walkable tiles, the Ditch's walls and the freeway barriers.
- 2026-10-17 — **Game cameras get no cutaway** (walls are not cut or faded when a hook game
  supplies `PlayFrame.camera`).

- 2026-10-17 — **Vehicle feel targets (E9 design):** vehicles slower than 100 km/h (bus, armoured
  truck, utility truck, courier van) use 0 → 80% of top speed instead of 0–100 km/h; the tight
  ranges (0.9–1.1 g, 25–45° slide, climbing 15% at ≥ 60% of top speed) apply to cars, vehicles of
  2.5 t or more must still climb the grade with a looser grip range; "left" in vehicle data is the
  car's own left (driver's side, +x); the feel harness builds its own long strip, 15% slope and
  kicker (the dev district is too short for top speed); rapier keeps wheel raycasts and suspension
  while the engine applies its own tyre, brake, engine, drag and stabilising forces.

## Open issues
- None yet. (Planning-time unknowns to verify early: rapier body-type switching for vehicle LOD in
  M4; egui-wgpu offscreen rendering for `capture hud=true` in M1; DualSense button mapping through
  gilrs on the user's Windows machine — a user check.)

## Log
- 2026-10-17 — Planning: engine studied at `0098196`; measured starter bot (3/3 wins), Drift Circuit
  6,886 ticks/s, pavilion world 649 ticks/s with 8,200 instances / 2,025 draw calls / 483k triangles /
  64 lights; single-file web build 3 min 06 s → 11.2 MB, boots in headless Chromium with the egui HUD
  visible; `scripts/vendor-engine.sh` tested on a throwaway clone (1,359 files; starter tests 4/4
  pass in the vendored tree). PLAN.md written.
- 2026-10-17 — Independent plan review (27 findings) verified and applied; `scripts/web-check.mjs`
  gained ordered `until:TEXT:TIMEOUT` steps (tested: a pass and a timeout failure).
- 2026-10-17 — **M0 done.** Engine vendored at `0098196` (`689142b`); engine baseline
  `cargo test --workspace`: 66 suites, 455 passed, 0 failed, 2 ignored. E1 (`9bd019a`): game and
  tool registries, `pav_tools::cli::{Cli, run}`, `pav_app::main_with(AppConfig)`. `gtf_game` +
  `gtf_app` (`2bf7495`): `cargo test -p gtf_game` 2/2 (every `freeway*` scene builds and steps;
  snapshot/rewind/replay exact). `gtf capture` of the lot looked at (`out/m0_lot.png`). Native
  window under Xvfb + lavapipe (`scripts/smoke-gtf.sh 15`): boot complete in 4.2 s, sim 60 tps,
  egui HUD and the lot visible (`out/smoke-gtf/screen.png`). Single-file web build
  (`scripts/build-gtf-web.sh`, 3 min 37 s): `dist/GrandTheftFreeway-BX.html` 11.2 MB (wasm 27.9 MB,
  8.3 MB gzipped); headless Chromium (`web-check.mjs --expect "boot complete" --expect "scene
  'freeway'"`): boots in 0.6 s, scene `freeway`, HUD visible (`out/web-m0/start.png`).
- 2026-10-17 — **M1 done.** E2/E3/E4/E6 (`40772ce`); E8 part 1, E12, E13, E14 (`b5de689`);
  `gtf_game` frontend, orbit camera, weapon wheel, settings, UI gallery (`7686717`); E5 `pav_ui`
  (10-foot kit on egui: Anton + Barlow Semi Condensed under OFL, procedural pad glyphs per device,
  HUD, radar and map from the `MapImage`, screens, moments, weapon wheel, phone, offscreen render)
  wired into `pav_app` (overlay, pointer → `ui.*`, gear, FPS readout) and `capture hud=true`.
  Tests: `cargo test -p pav_core -p pav_world -p pav_ui -p pav_view -p gtf_game` 360 passed,
  `cargo test -p pav_tools --lib --test ui_hud --test asset_errors` 54 passed; clippy clean.
  Captures looked at: `gtf` REPL `load scene=freeway/gallery` + `capture hud=true` at 1920×1080 and
  960×540 (`out/m1/gallery_*.png`: title, menu, map, HUD, cards — legible at both sizes). Native
  window (`scripts/smoke-gtf.sh 25 --scene freeway/roam`, Xvfb + lavapipe): boot complete in
  1.5 s, the FPS readout (software rendering, so red), money, radar with the city map, vitals,
  objective with a keycap glyph, the generated city (`out/smoke-gtf/screen.png`).
- 2026-10-17 — **M2 done.** E14 regions + `pav_world` (terrain, Route 404 + ramps, The Ditch, lane
  graph, signals, walk tiles, cover, dressing with `LIB/gtf_city`), Paloma Bay + the Proving
  Grounds, E7 merged (`97bfc96`: sky, weather, night lights, facade windows, static culling),
  facades by style (`609dcab`), the game's look in `world.toml` (`44ab6af`), `gtf lanes` + the
  city determinism test (`58af7f7`), the web pointer-lock crash fixed (`832b694`). Acceptance:
  city digest identical over two builds (1152 chunks); lane network one strongly connected
  piece; `freeway/tour` 10.6 km at 40 m/s with no holes, <= 146 active / 0 dormant chunks
  (filmstrip `out/m2/tour_filmstrip.png`, golden hour into dusk); `hills_vista` shows the
  district; `gtf perf view=all` within budget (downtown 1.51 M triangles); every view at 11:00,
  18:10 and 22:30 (`out/m2/sheet1.png`, `sheet2.png`; changed after looking: the docks view's
  eye, noon's contrast and smog line, the lit/ACES look); the web build boots into the city with
  the HUD (`out/web-m2/fixed_*`). Tests: pav_world, gtf_game, pav_tools store + store_rooms,
  pav_render + pav_view (46), WGSL in Tint; clippy clean on the touched crates. Carried: street
  lamps are dark at night (props need a night glow), mesh LOD (E7 helper); pier deck and Big Wheel
  (M7).
- 2026-10-17 — **Session 1 ends (handoff).** After M2: on-foot camera (`600975f`), radio skeleton
  (`1da78ce`), pedestrians + E11 low detail (`44f379d`), ambience (`fcf0e72`), on-foot combat
  (`ab0af1d`), E9 vehicles v2 merged (`06b7a62`), E7 night glow + mesh detail merged (`cbff7b1`).
  `HANDOFF.md` lists the state, the next steps in value order and the known issues.

