# PROGRESS — Grand Theft Freeway-BX

The implementation tracker for [`PLAN.md`](PLAN.md). Update it as work lands: tick tasks, fill the
Definition of Done evidence, log decisions and open issues. Keep "Now" current — it is what a
resumed or compacted session reads first.

## Now
- **Phase (2026-10-24, session 4):** M0-M6 done; **M7 (content: the vertical slice) is next**,
  then M8-M10. Read `HANDOFF.md` first, then this file. **Remaining work** (below the
  milestones) lists everything M7-M10 still needs, from a gap inventory of the code against
  PLAN. Sessions 3 and 4 only checked the environment (Log). Wasm clippy (D1) still fails on
  vendored engine code (Open issues). The `.mcp.json` server now runs `gtf mcp`; the old `pav`
  command timed out at startup. Nothing is running.
- **Session 2:** M3-M5 (driving, traffic, `LIB/gtf_vehicles`, heat and the police, cover,
  grenades, armed opponents, WORKSHOP clips, guns in hand, drive-bys, the respray and car swap,
  the evasion bot, the map's edge) and M6: the mission DSL with its validator, runtime and bot
  (`docs/gtf/MISSIONS.md`), two test missions using all 25 step kinds, the three protagonists
  (switch wheel, sky-cam, abilities, life spots), the phone (nine apps, texts, the camera), the
  shops (Bang Bang Bros., Paloma Customs, Gas-N-Grab), saves (three slots, autosave, Continue)
  and two engine additions (district `[[spot]]`s, `VehicleInput.grip`).
- **Soaks (release `gtf`):** traffic 600 s seeds 1-2 and peds 600 s pass every limit; roam
  900 s seeds 2-3 pass, seed 1 gives up one stuck car at a dead-end turnaround (open issue).
- **Perf:** `gtf perf view=all` (release) passed every budget with traffic and crowds in session
  2 (downtown: 110 vehicles, 94 people, 2.8-3.2 ms to build a frame, 1.4-2.2 ms a tick). On the
  session 3 container the same view took 4.7-5.4 ms and 2.8-3.6 ms. On session 4's it passes
  all ten views (downtown 3.6-3.9 ms and 2.0-2.1 ms; thin margin, open issue).
- **Tests:** pav_core 305, pav_view 44, pav_world 11, gtf_game 98 (+8 ignored reports), all
  green; clippy -D warnings clean on the touched crates; `cargo check --workspace --all-targets`
  clean.
- **Helpers:** none running.
- **Branch / PR:** `claude/keen-cerf-g5vksy`, draft PR #2.

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
- [x] **M3 Player and vehicles** — on-foot controller, camera modes, E9 vehicles v2 (pitch/roll
      freed with stabilization), enter/exit, carjack, damage, helicopter, vehicle audio, GPS ribbon,
      radio skeleton, `vehicle_feel` incl. slopes and jumps. *Acceptance:* `tests/vehicles.rs`
      (every archetype within its feel ranges incl. the 15% grade and the kicker jump; the
      helicopters fly); `tests/driving.rs` (get in, drive, get out; locked car and carjack;
      bail-out; helicopter climb; a car throws the person it hits) and the grenade/explosion
      test; filmstrips looked at — `out/m3/drive_filmstrip.png` (12 s of autopilot at golden
      hour, no jitter or clipping) and `out/m3/walk_filmstrip.png` (a sprint into a wall: the
      camera stayed inside the wall until the shoulder fix, now it keeps out); perf in budget.
- [x] **M4 Population** — traffic, pedestrians, population manager, LOD, buses, parked cars,
      ambience, soaks. *Acceptance:* `gtf soak mode=traffic seconds=600` seeds 1-2: ok (AI
      crashes 1 and 0, stuck 0, spawn/despawn in view 0, NaN 0, fell 0); `mode=peds
      seconds=600`: ok; `tests/traffic.rs` (two minutes obeying the lights, pull-over for a
      siren, buses at their stops), `tests/peds.rs` (crossings on the walk light, replay);
      `gtf perf view=all` within budget with the population; streets looked at
      (`out/m5/downtown_lod.png`, `out/m4/peds_sheet*.png`, the drive filmstrip).
- [x] **M5 Combat and heat** — weapons, cover, vitals, enemy AI, wanted system, police AI, air unit,
      busted/wasted. *Acceptance:* `tests/combat.rs` (7) and `tests/enemies.rs` (4); `tests/heat.rs`
      (witnesses phone it in, police arrive unseen and arrest, Wasted at the hospital, the
      respray and the car swap, the evasion bot escapes 3★ within 180 s on at least 3 of seeds
      1-5 — rates in Decisions); police never spawned in view in any roam soak (counter 0);
      3★ night captures looked at — `out/m5/night3_sheet.png`, `night3_012s.png` and
      `night3_030s.png` show the searchlight's pool on the street and the vision cones on the
      radar.
- [x] **M6 Structure** — mission DSL + runtime + bots, protagonists + switching + abilities, phone,
      economy and shops, save/load. *Acceptance:* `tests/missions.rs` — the bot finishes both
      test missions on seeds 1-3 (`dev_test` drives: goto, enter_vehicle, drive_to, follow,
      race, escape, lose_heat, deliver; `dev_test2` on foot and story: kill, collect,
      photograph, interact, stealth, holdup, chase, choose, branch, survive, switch, parallel,
      defend) and the loud way; every step kind used; fail → retry → skip after three fails;
      quit; pass pays, texts and autosaves; markers start missions; replay from the pause menu;
      the validator reports file:line. `tests/protagonists.rs` (switching keeps each one's
      cash, weapons and place; the wheel and the sky-cam; abilities slow time, change damage,
      drain), `tests/phone.rs` (tiles, the map waypoint, the camera catches who is centred),
      `tests/economy.rs` (gun store, snacks, customs, a save round trip into a fresh game,
      Continue loads the newest, the safehouse bed). Release `gtf`: `play bot=true` passes
      `dev_test2` in 218.7 s of game time (gold). Captures looked at (`out/m6/`): phone home and
      camera, the switch wheel, the sky-cam over the district, Mack landed, Deadeye's grade
      (`deadeye_compare.png`), the intro card, objective and subtitle, MISSION FAILED and its
      card, MISSION PASSED, the results screen, the gun store. Fixed after looking: the HUD,
      tips and play prompts now give way to any card or menu (the minimap showed through the
      shop's price box), the camera app shows its buttons as prompts (its row printed raw
      glyph markup), the ability grade was too faint, the fail card lost "failed" once the
      banner faded, the results printed ✓/✗ in a font without them, `dev_test2` started behind a
      parked SUV.
- [ ] **M7 Content** — final district, landmarks, interiors, all vehicles and props, missions m1–m3,
      s1–s3, h0, h1 (Loud + Smart), activities, radio stations, completion.
- [ ] **M8 Frontend and attract** — title, menus, settings, About, loading cards, attract director,
      touch = attract only.
- [ ] **M9 Polish** — art, animation, audio mix, UX against the 10-foot rules, accessibility,
      performance, bug bash.
- [ ] **M10 Release** — builds (native, Windows `.exe`, `dist/GrandTheftFreeway-BX.html`), docs,
      final verification, delivery.

## Remaining work (gap inventory, 2026-10-24)
Two read-only audits of the code against PLAN §2, §5, §7.8 and §9 found these gaps; their main
claims were spot-checked. Tick items as they land and move the evidence to the milestone entries
and the DoD table. PLAN stays the source of truth: this list says what is missing, not the design.

**First:** fix the D1 wasm-clippy lints (Open issues) in one `engine:` commit.

**M7 content** (in PLAN §2's order of value: the heist path first)
- [ ] **Missions.** Only `dev_test` and `dev_test2` exist. Write `m1_repo_man`, `m2_midlife`,
      `m3_overdrive`, `s1_rush_hour`, `s2_heavy_metal`, `s3_test_drive`, `h0_board` and
      `h1_freeway_job` (Loud and Smart) with dialogue and shots. Add each to `FILES`, validate
      it and give it a bot test on 3 seeds (both heist approaches). The `choose` step already
      shows a `mission/board` Cards screen and freezes the world; `h0` adds the crew's cut.
- [ ] **Landmarks.** These are placeholder masses with a sign (`pav_world/src/lots.rs`
      `landmark`): the bank, police, hospital, gun store, Paloma Customs, Freeway Diner, Luxe
      Motors and the Ironclad depot. City hall and the gas station are paving only. Missing
      altogether:
      - the Big Wheel (turning), the pier arcade and its food stalls, and the freighter;
      - the Grind House, the City Works yard, the harbor warehouse, a parking garage and an
        impound lot;
      - the Skylark on the hospital roof, the shop names on Sunset Row's sign bands, and neon
        signs.
- [ ] **Interiors.** None exist: shops are menus at door spots and safehouses are spots. Build the
      gun store, mod shop bay, Gas-N-Grab, Grind House, Freeway Diner, the safehouses and the
      warehouse.
- [ ] **Bodies and looks.** The two helicopters have no authored body (`pav_view` draws a
      procedural one). People vary only by colour, height and build; add hair, headwear,
      accessories and outfits.
- [ ] **Activities.** None exist yet, and there is no `activities.toml`. Needed: the Pretzel Run
      race, 5 stunt jumps (with slow-mo and the bonus), hold-ups at the Gas-N-Grab and the Grind
      House, vigilante, the Purse Snatcher and Getaway Needed random events, and 15 Lost
      Shields. Parts to build on: the `race` and `holdup` step kinds, `BannerKind::Stunt`,
      `BlipIcon::Jump` and the `[[jump]]` wedges.
- [ ] **Radio.** Every genre varies one synthwave kit (`pav_audio/src/music.rs`), and SPNN's
      `talk` genre falls through to music. Needed:
      - a patch for each of at least 4 music stations;
      - SPNN news/talk: headlines, ads and heat-aware traffic reports;
      - DJ cards, and scanner chatter text (today only a sound cue at 2★+).
- [ ] **Completion and stats.** `progress.rs` `completion()` counts story passes and gold medals
      only, and reads 0% because no story missions exist yet. Track and save activities and
      collectibles, and add the completion and collectibles cards to the STATS tab. Add a test
      that drives completion to 100%.
- [ ] **Checks.** Capture every landmark at its best time of day. Add the PLAN §7.8 suites that
      do not exist yet: `content.rs` (every file and reference validates), `district.rs`,
      `player.rs`, `save.rs`, `replay.rs` and `ui.rs`. Each may fold into an existing file if
      it says so.

**M8 frontend and attract**
- [ ] **Screens.** The title is a drone orbit over downtown; PLAN wants the hills at golden hour.
      Also needed: a new-game intro, and loading cards that use the engine's halftone and
      painterly filters (the game does not use them yet). Pause → Game lacks Save and Restart
      checkpoint, Map lacks its filter and snap, and Brief lacks the log and texts.
- [ ] **Settings.** Add aim-assist strength, separate X/Y sensitivity, sprint hold/toggle, the
      default station, the quality preset, render scale, dynamic resolution, v-sync, brightness,
      subtitle size and colourblind blips. Nothing reads these existing toggles yet: `subtitles`,
      `reduce_shake`, `high_contrast`, `vibration` and the `dialogue` volume.
- [ ] **About and credits.** Show the full §1 Source Information block (engine versions, the
      environment, the model wording) and credit the fonts and the motion libraries. Fix the line
      that says everything is generated.
- [ ] **The attract director.** Today a 10 s idle only logs "attract would start". Build:
      - `content/freeway/attract.toml` with the 7 vignettes of §5.8, each with its bots, shot
        lists and captions;
      - the logo and prompt overlay, exit at once on any input, stall handling, and a population
        reset per vignette;
      - `gtf soak mode=attract` (rejected today), the 30-minute soak and `tests/attract.rs`.
- [ ] **Touch.** Start in attract and never leave it. Handle the `gear` button with its overlay
      (`pav_ui` draws it; the game ignores it) and hide the HUD. Pass the `web-check.mjs`
      desktop and mobile flows.

**M9 polish**
- [ ] **Gameplay.** Bullets don't burst tyres (only spike strips do), and traffic never turns its
      headlights on at night. Missing:
      - vault, the bat and a heavy melee attack;
      - near-miss swerves and angry drivers getting out;
      - scenario points beyond benches, and the siren yelp;
      - a sky-cam that reaches 300 m in ≤ 3 s with its grid overlay (today 170 m in 3.6 s).
- [ ] **Audio.** Duck the mix for dialogue (only menus duck). Add reverb zones (the game never
      sets `AudioScene.reverb`). Mix pass.
- [ ] **Visuals.** Add camera shake (every `shake` is 0.0) that honours `reduce_shake`, rain
      splashes on the ground and neon. Art pass at every time of day and weather.
- [ ] **Performance.**
      - The browser `?perf=1` console line with sim ms per tick is missing.
      - The game has no quality presets (Low to Ultra, with population scaling); the engine's
        Low/Medium/High exist only in its own egui menu. Dynamic resolution runs only on touch.
      - `gtf perf` doesn't check particles, shadowed lights or active entities.
      - Downtown must sit well under budget.
- [ ] **Bug bash.** Clear the Open issues. Run the bots on 5 seeds. Test the edge cases: water,
      falling, interiors, switching mid-chase, and saving during heat (refused).

**M10 release**
- [ ] **Builds.** `scripts/build-windows.sh` doesn't zip the `.exe`. `pack_single_html.py`
      can't write the Source Information header comment or `<meta name="description">`.
      `dist/` doesn't exist yet.
- [ ] **Docs.** Write `docs/gtf/ENGINE_ANALYSIS.md` (with the D16 systems table),
      `docs/gtf/HANDOFF.md` and the final `README.md` (how to play, controls, credits). Add a game
      entry to `docs/FEEDBACK.md` → Games, and engine docs for every extension.
- [ ] **D16.** Add a view and capture for every Proving Grounds station, plus combat/weapons and
      radio signs.
- [ ] **Delivery.** Final verification of D1-D13 and D16 with fresh captures, then the PR
      description. Send the HTML and the zipped `.exe`, then the final report (§12.4).

## Definition of Done evidence (PLAN.md §2)
| Item | Status | Evidence (test, command + result, capture looked at, artifact) |
|---|---|---|
| D1 Builds and checks | ☐ | |
| D2 The district | ☐ | |
| D3 Player | ◐ | M3: feel ranges for all 13 road archetypes, driving/carjack/bail-out/helicopter tests, on-foot camera with wall handling; M6: three protagonists with their own cash, weapons and places, the switch wheel and sky-cam, abilities (`tests/protagonists.rs`); final looks and animation polish wait for M7/M9 |
| D4 Living city | ◐ | M4: traffic and pedestrian soaks within limits, buses, parked cars, struck people, witnesses; roam soak open issue (one stuck car in 2,700 s) |
| D5 Heat | ◐ | M5: `tests/heat.rs` (reporting, arrest, Wasted, respray, car swap, evasion), police never spawn in view, 3★ night captures |
| D6 Missions and activities | ◐ | M6: the mission system end to end — DSL, validator (file:line), runtime, checkpoints, fail/retry/skip, medals, results, replay, the bot that finishes both test missions on 3 seeds (`tests/missions.rs`, `docs/gtf/MISSIONS.md`); the story missions, heist and activities are M7 |
| D7 UI | ◐ | M1 kit + M6 screens: intro card, objectives, subtitles, fail card, results, switch wheel, phone (9 apps), shop menus, save slots, captured with HUD (`out/m6/`); the 10-foot review is M9 |
| D8 Frontend | ☐ | |
| D9 Attract mode | ☐ | |
| D10 Audio | ☐ | |
| D11 Visuals (incl. FPS readout) | ☐ | |
| D12 Performance | ◐ | `gtf perf view=all` (release) within every budget with the population, 2026-10-18 |
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

- 2026-10-18 — **Session 2 decisions:**
  - Vehicles are 2.5× tougher (`vehicles.toml` health): police contact and PITs wrecked cars in
    seconds. One crash costs once per contact (engine `crash_dv`): a car pressed into a wall lost
    its health in 0.2 s.
  - Heat: a crime escalates in front of the police only when it is as serious as the current
    level; only a police vehicle the player harmed (blame list, 15 s) counts as destroyed by them;
    the last sighting stands while unseen.
  - Paloma Customs came forward to M5 (the respray needs a door); the bat waits (the weapon
    wheel's eight slots are full).
  - The helicopter flies over the rooftops (it crashed into the downtown towers) and its crew
    see within 55° of straight down: an all-round view kept 3★ almost inescapable once it
    survived. The getaway autopilot does not brake for police cars (that is how a PIT works).
  - D13's evasion test counts escapes over seeds 1-5 (≥ 3) instead of naming seeds: any traffic
    change reshuffles which chases get away. Latest `evasion_rate`: 5/8 rolling, 2/8 standing
    (earlier this session 7/8 and 6/8; the standing start is an open issue).
  - The map has an edge: an invisible wall all round (engine hidden blocks), ROAD CLOSED
    barriers where roads leave it. Route 404's closed north end turns its traffic round 27 m
    before the barrier (it used to flow through the barrier on rails and could shove anything
    in its way through it).
  - Everything the game populates despawns with its ground (`Unload::Despawn`); nothing spawns,
    leaves its rails or switches to physics unless all the chunks under it are loaded. Wrecks
    lose their occupants.
  - Traffic: first come, first served at unsignalised junctions; with lights, green waits for a
    straggler; lanes that fan out of or merge into one lane see each other; everyone brakes for
    anything about to cross their path; mandatory lane changes for lanes that go nowhere;
    queued cars never back up; cars on rails switch to physics near any car the physics moves;
    stop lines 8 m back (turning buses); dead ends and the edge turn round on a drivable
    keyhole (11 m and 16 m short of the end).
  - AI-crash counting: an incident's when either car was fleeing, pulled over or shaken, or
    within 40 m of a police vehicle or the wanted player; a repeat of the same pair within 20 s
    counts once.
  - `gtf perf` times the median of five frames; far wheels (> 60 m) are plain cylinders; drawn
    assets tally a frame's meshes once (a hundred cars cost 9 ms a frame before).

- 2026-10-24 — **The MCP server runs `gtf mcp`** (`.mcp.json`; the key stays `shardfall`, so the
  tools are still `mcp__shardfall__*`). The engine's `cargo run -p pav_tools --bin pav` resolved
  features differently from the SessionStart hook's combined `pav` + `gtf` build. It rebuilt the
  dependency graph and timed out after 600 s. `cargo run -p gtf_app --bin gtf` is fresh after the
  hook (0.6 s to answer `initialize`) and serves 86 tools, the game's included. The engine docs
  still describe upstream's `pav mcp`.

- 2026-10-18 — **M6 decisions:**
  - Switching is open from the start (`crew.unlocked`): the story's introductions arrive with
    M7's missions. A step's `switch = [...]` limits whom the wheel offers while it runs.
  - Missions thin traffic (`{ traffic = 0.35 }`) and cap the heat (`{ heat_cap = 0 }`) where a
    set piece needs it: the bot's drive legs stalled in junction queues at full density, and the
    police killed Mack at 3★ after the range shootout.
  - The mission bot acts only through its input frame (pad presses plus the commands a player's
    tools send: `autopilot on|escape|off`, `pilot_goal x z desire`, `weapon <id>`), so replays
    and snapshots include it and nothing reaches into game state.
  - Checkpoints re-stage the mission's actors (every actor still up, with its behaviour) and put
    back the flags, heat, health, clock and the player's place and car, instead of snapshotting
    the whole sim. Skip after three fails fast-forwards to the next checkpoint: its steps are
    done for the player and their targets cleared.
  - Cars with a goal (mission drivers, the autopilot's `pilot_goal`) corner at no more than 1.15×
    the curve speed (they slid into the edge wall), and replan when a lane change leaves their
    route empty short of the goal lane; mission cars are left out of the population's counts.
  - Abilities: Slipstream (Dez, at the wheel: time 0.5, +40% tyre grip through the new
    `VehicleInput.grip`), Deadeye (Mack, on foot: time 0.35, aim-assist cone ×2, damage ×1.5),
    Overdrive (Roxy: damage ×2, taken ×0.5, fists' shove ×2); each washes the screen in its
    colour, half desaturated, with a vignette. Meters fill by play style (`protagonists.toml`).
  - Saves are JSON (`SaveData`, version 1) handed to the host through `AppRequests.save` and kept
    by the M1 host store (native files, web localStorage); `host.save` hands them back at boot.
    Three slots at the safehouses plus `auto` after each pass; Continue loads the newest.
  - Results rows read "Done" / "Missed": the UI fonts have no ✓ or ✗.
  - Engine (upstreamable, `bf15e85`): districts name `[[spot]]`s (a place and a facing) for
    missions and life spots; `VehicleInput.grip` adds tyre grip (0..2) for abilities.

## Open issues
- Roam soak (900 s, seed 1): one car gave up stuck at a dead-end keyhole turnaround (limit 0);
  roam AI crashes 0-3 per 900 s with buses turning at junctions the most common. M9 polish.
- Evasion bot from a standstill: 2/8 seeds (Wasted after being pinned by a PIT; it will not drive
  through the officers who get out). M9: fast-road routing, not stopping for armed officers.
- DualSense button mapping through gilrs on the user's Windows machine (a user check).
- Dense traffic can lock a junction box into a queue that never clears (why missions thin
  traffic). M7/M9: give way by arrival order inside the box, or let the tail re-route.
- `dev_test` takes the bot about 395 s of game time (long drive legs at a safe pace); the whole
  missions suite runs in about a minute in the dev build. M7's missions should keep drive legs
  short or use `traffic` thinning.
- **D1: wasm clippy is not clean** (`cargo clippy -p pav_app -p gtf_app --target
  wasm32-unknown-unknown -- -D warnings`, found 2026-10-18). There are four wasm-only lints in
  vendored engine code (`689142b`), and no earlier session recorded this check as passing.
  `clippy::drop_non_drop` fires at `pav_tools/src/animation_tools.rs:287`, `asset_tools.rs:95`
  and `creature_tools.rs:662` (on wasm `std::fs::File` has no `Drop`).
  `clippy::default_constructed_unit_structs` fires at `pav_app/src/asset_watch.rs:48` (the wasm
  `AssetWatcher` is a unit struct). With those two lints allowed, the rest is clean. Fix them in
  one `engine:` commit (a block scope instead of `drop`, and `Self` for the unit struct).
- **Perf depends on the container.** `gtf perf view=downtown_street` (release) builds a frame
  in 4.7-5.4 ms (budget 4; outliers to 7) and runs a tick in 2.8-3.6 ms (budget 3) on session
  3's container. A release build of the last commit before M6 (`fbe56bf`) gave the same numbers
  there, so the cause was the machine, not a regression. On session 4's container, `perf
  view=all` passes every view: downtown builds a frame in 3.6-3.9 ms and ticks in 2.0-2.1 ms.
  Judge perf changes by A/B against a baseline build on the same machine. D12 needs margin:
  M9's performance pass should bring downtown well under 4 ms on any of these containers.
- **Earlier evidence is not on disk.** `out/` is ignored by Git and containers reset, so the
  M0-M6 captures, filmstrips and soak JSON cited above are gone. M10's final verification
  re-runs the D1-D13/D16 commands and looks at fresh captures.
- **Small inconsistencies found in session 4:** a billboard reads "FREEWAY FM 99.4"
  (`pav_world/src/dress.rs:27`) but the station is 104.04 (`radio.toml`). The `beach_sunset`
  view's `about` names the Big Wheel, which is not built yet. The About screen says every model,
  sound and song is generated, but the walk clips come from motion libraries. Fix them with
  M7/M8.

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
- 2026-10-17/18 — **Session 2: M3, M4, M5 done.** Driving (`146240c`), traffic and the soak
  tools (`f554c62`), `LIB/gtf_vehicles` (`a8fa7f5`), heat, police, Wasted and Busted
  (`d63b142`), cover and grenades (`ce2281e`), WORKSHOP clips (`87bf78b`), the police and the
  evasion bot (`f27969e`), armed opponents (`ad6e362`), clips wired, drive-by, respray
  (`f0d5e2e`), `LIB/gtf_weapons` (`e889af5`), guns in hand (`e76ff72`, `2f8599a`), the map's
  edge, turnarounds and the looping freeway end (`4e8de11`, `3b1bb09`, keyholes in `fec29af`),
  population that never falls through the world and safer traffic (`5d15c3c`), view build time
  (`pav_view` mesh tally, far wheels), the camera's shoulder out of walls. The roam soak found
  every fall-through and most traffic faults: a heli wrecked on a tower falling through an
  unloaded chunk, wrecks with drivers that never slept, shaken cars kept on physics off the
  loaded area, parked cars straddling a chunk border, police rails cutting through houses,
  U-turns bulging into buildings, kinematic cars launching others, traffic flowing through the
  freeway's barrier. Evidence as in the milestone entries above; captures in `out/m3`,
  `out/m5`; soak JSON in `out/soak`.
- 2026-10-18 — **M6 done; session 2 ends (handoff).** Engine: district spots and
  `VehicleInput.grip` (`bf15e85`); game: missions (DSL, validator, runtime, bot), protagonists,
  phone, shops and saves (`e1d2832`); the tests, `docs/gtf/MISSIONS.md` and the fixes made
  after looking at the captures (this commit). Evidence in the M6 milestone entry; captures in
  `out/m6/` (not committed). `HANDOFF.md` rewritten for the next session (M7).
- 2026-10-18/19 — **Session 3: environment check** (no code changes). The container was ready:
  toolchain 1.98.1 with the wasm32 and windows-gnu targets, wasm-bindgen 0.2.129, the
  SessionStart hook's packages and debug `pav`/`gtf` built, lavapipe, mingw + Wine (a
  cross-built `.exe` runs), Node 22 + Playwright + Chromium (`scripts/check-wgsl.sh DIR` compiles
  WGSL in Tint), the shardfall MCP server connected. `cargo fmt --all --check` clean;
  `cargo test -p gtf_game` 98 passed, 8 ignored (3 min 40 s); host clippy `-D warnings` clean for
  `gtf_game` and `gtf_app`; wasm clippy fails (Open issues); release `gtf` built cold in 9 min
  51 s; captures of `freeway/dev`, `freeway/roam` and `freeway/view/downtown_street` with the HUD
  looked at; perf as in Open issues. Notes for later sessions: the MCP server's
  `cargo run -p pav_tools --bin pav` resolves features differently from the hook's combined
  `pav` + `gtf` build, so it compiles the dependency graph a second time at startup (a few
  minutes, harmless). A worktree must not share the main tree's target dir: path crates hash
  relative to the workspace root and `pav_core`'s build script bakes in `CARGO_MANIFEST_DIR`, so
  the worktree's build overwrites the main tree's artifacts. The fix is `cargo clean --release -p`
  for the ten workspace crates, then a rebuild.
- 2026-10-24 — **Session 4: environment check and gap inventory** (no game code changed).
  Toolchain 1.98.1 with the wasm32 and windows-gnu targets, wasm-bindgen 0.2.129, lavapipe
  (llvmpipe, LLVM 20), mingw + Wine 9.0 (a cross-built `.exe` runs), Node 22 with Playwright
  1.63.0 and Chromium 153 (a WebGPU adapter with `web-check.mjs`'s flags), Xvfb, ImageMagick,
  25 GB of disk free. `cargo fmt --all --check` was clean. Wasm clippy fails on the same three
  `pav_tools` lines (Open issues). The release `gtf` built cold in 6 min 33 s. `gtf perf
  view=all` passes all ten views (Now). The debug `gtf` captured `freeway/dev` with the HUD,
  and the capture was looked at: the Proving Grounds' signal station sign, traffic, the
  minimap and the prompts. The shardfall MCP server timed out at startup, so `.mcp.json` now
  runs `gtf mcp` (Decisions). Two read-only audits of the code against PLAN §2, §5 and §9 gave
  **Remaining work**. `HANDOFF.md` now has a shorter `/goal` prompt.
