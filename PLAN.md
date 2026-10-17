# Grand Theft Freeway-BX — Implementation Plan

> An original spiritual successor to **Grand Theft Auto V**, built as a one-district, gameplay-complete
> vertical slice on the **Pavilion engine** (the Shardfall repository). This file is the single source
> of truth for scope, design, architecture, milestones and the Definition of Done.
>
> Written in the planning phase (2026-10-17) for the implementing agent (Claude at Extra effort),
> working in this repository on the session's designated branch.

---

## 0. How to use this plan

1. Read §1–§4 once, then §10 (workflow rules), then execute the milestones in §9 **in order**.
   Re-read the design (§5) and architecture (§6) sections that a milestone names before starting it.
2. Track everything in `PROGRESS.md`: tick milestone tasks, record evidence for every Definition of
   Done item (§2), log decisions and open issues. `PROGRESS.md` is what the stop hook and the next
   agent read to know what is left.
3. The game must be **playable and green at the end of every milestone** (builds, tests, bots).
4. When this plan is wrong about the engine (it was written from a reading of the code, not from
   building every feature), decide, write the decision in `PROGRESS.md` → Decisions, and keep
   going. Do not stall on a question that is not genuinely the user's (§10.6).
5. If something must give, the order is: Definition of Done items > polish > stretch (§13).
6. Section numbers are stable; cite them in commits and `PROGRESS.md` (e.g. "M4, §6.6.3").

---

## 1. Source information

Kept here for reference; the same block goes in `README.md`, the in-game About screen and the
single-file HTML build (§12), separate from the engine evaluation.

- **Initial Date:** 2026-10-17 (the first prompt of the planning session; the session was created at
  2026-10-17 00:13 UTC).
- **SOURCE:** *Grand Theft Auto V* — Rockstar North / Rockstar Games, 2013 (PlayStation 3, Xbox 360;
  later PlayStation 4, Xbox One, PC, PlayStation 5, Xbox Series X|S).
- **GAME ENGINE:** Pavilion engine from the Shardfall repository,
  https://github.com/michaelcrosato/shardfall, pinned at commit
  `0098196140d279979b622ee4373cf498d95fc9e1` ("Co-op plan on hold (#74)", 2026-10-16), workspace
  version 0.1.0: Rust 1.98.1, wgpu 30 (WebGPU in the browser, Vulkan/DX12 native), rapier3d 0.36,
  egui 0.36, glam 0.33, cpal 0.18, gilrs 0.11. Handoff route used: `START_HERE.md` →
  `docs/MAKE_A_GAME.md` (the "Make a game" route), extended per §6.
- **Environment:** Claude Code (container build 2.1.302) in an Anthropic-managed cloud session started
  from the Claude desktop app. Linux: Ubuntu 24.04.5 LTS (kernel 6.18), 4 vCPU, 15 GB RAM, no GPU —
  headless captures render on Mesa llvmpipe/lavapipe (software Vulkan); browser checks run in headless
  Chromium build 1194 via Playwright 1.56.1 (SwiftShader WebGPU); native window smoke tests run under
  Xvfb. Player targets: Windows build on an NVIDIA RTX 3060 Ti, and WebGPU browsers (Chrome / Edge).
- **Model and Effort Level:** AI lab: Anthropic (Claude). Planning ran at reasoning effort **max**;
  implementation runs at **Extra** effort. Both were confirmed from the session's metadata
  (`get_session`: `session_context.model`, `last_served_model`, `effort_level`). The exact model is
  stated in the chat reports (planning and delivery), **not in repository files**: this environment
  forbids model identifiers in anything pushed to a repository (commits, PRs, docs, code). The
  implementing session confirms its own model and effort with `get_session` and states them in its
  final chat report; repository files (README, About screen, HTML) say "Anthropic Claude — exact
  model and effort level in the delivery report" plus the effort levels. Never guess.

---

## 2. Definition of Done

The goal is complete when **every** item below is true and `PROGRESS.md` records the evidence (a
test name, a command and its output summary, a capture path that was looked at, a build artifact).
Items are deliberately checkable without a display.

**D1. Builds and checks.** From a clean checkout: the game (`cargo build --locked -p gtf_app`), the
agent CLI (`gtf`), the single-file web build `dist/GrandTheftFreeway-BX.html`, and the Windows
`.exe` (`scripts/build-windows.sh`, cross-compiled) all build. `cargo test --workspace --locked`
passes (GPU-only and network-only tests may stay opt-in exactly as upstream marks them);
`cargo clippy --all-targets -- -D warnings` is clean for every crate touched; `cargo fmt --all
--check` is clean; `scripts/check-wgsl.sh` passes after any shader change; `cargo clippy -p pav_app
-p gtf_app --target wasm32-unknown-unknown` is clean.

**D2. The district.** Paloma Bay (≈1 km², §5.3) is generated from data, streams while driving with no
holes or floating geometry, and contains every landmark and interior in §5.3 and §5.7: downtown
towers, Sunset Row shops, Los Mirados bungalows, Mirador Hills road + PALOMA sign + Lookout, Palisade
Beach + Pier with the Big Wheel, The Flats + The Ditch storm channel, Harbor Gate docks, elevated
Route 404 + the Pretzel interchange + the broken construction ramp. Day/night cycle and weather
(clear, hazy, overcast, rain, storm, fog) run with a sky dome.

**D3. Player.** On foot: walk, run, sprint with stamina, jump, vault/climb, swim, crouch, melee, cover
(snap, peek, blind fire, move along), aim with assist, fire, reload, weapon wheel with slow-mo.
Vehicles: ≥ 11 archetypes from data (§5.6.3) incl. police cruiser, bus and a flyable helicopter;
animated enter/exit, carjacking, locked cars, analog throttle/brake/steer, handbrake drift,
damage → smoke → fire → explosion, burst tyres, broken lights, horn, headlights, police
lights/siren, radio. Three protagonists with abilities (Slipstream, Deadeye, Overdrive) and a switch
wheel with the sky-cam transition.

**D4. Living city.** Traffic follows lanes and signals, turns at intersections, changes lanes,
yields to pedestrians, pulls over for sirens, reacts to the player; buses run a route; parked cars
line the streets. Pedestrians walk sidewalks, cross at crosswalks on the walk signal, use scenario
points, and react (flee, cower, fight back, film, report crimes). Densities vary by zone and time.
AI level-of-detail keeps the sim inside budget (§6.9).

**D5. Heat.** Five-star wanted system (§5.6.5): witnesses and reporting, police dispatched from
stations along the road network (never spawned in view), pursuit, roadblocks and spike strips,
helicopter with a searchlight at night, search zone with vision cones on the minimap, flashing
stars, evasion, respray clears heat; Wasted and Busted flows with respawn and fees.

**D6. Missions and activities.** Data-driven missions (§6.8.6): 3 intro missions, 3 heist setups,
the planning board, and the Freeway Job heist in two approaches (Loud, Smart) with character
switching; checkpoints, instant retry, skip after three fails, medals, rewards, replay. Activities:
street race, 5 stunt jumps, 2 store hold-ups, vigilante, 2 random events, 15 collectibles. Economy
(gun store, mod shop, store) and save/load (3 slots + autosave).

**D7. UI.** 10-foot, controller-first UI everywhere (§5.9): HUD (minimap with blips, GPS route and
vision cones; health, armour, ability; money; wanted stars; weapon/ammo; objectives; subtitles;
prompts with controller glyphs), pause menu (map, brief, stats, settings, game), phone, weapon /
switch / radio wheels, planning board, shop menus, banners. PlayStation glyphs by default,
auto-switching to Xbox or keyboard glyphs by last device. Keyboard and mouse drive the same focus
model. No dense, web-style data screens.

**D8. Frontend.** Boot splash → title → main menu (Continue, New Game, Free Roam, Settings, About,
Quit) → loading cards; settings (audio, display/quality, controls, accessibility); About/credits
showing the Source Information block (§1) and asset credits.

**D9. Attract mode.** After 10 seconds without input on the title or any main-menu page, an AI
plays the game: ≥ 6 vignettes (§5.8) covering driving + radio, a police chase with the helicopter,
a gunfight with cover, a character switch, a stunt jump, city life, and the heist getaway. It loops
indefinitely (a 30-minute headless soak reports zero errors and zero stuck vignettes) and any input
from any device returns to the title at once. On touch devices the game runs the attract loop only,
with a single settings button on screen.

**D10. Audio.** Synthesized SFX for every action and event; continuous loops with live parameters
(engines by RPM, tyre squeal, sirens, rotor, rain, wind, crowd and city beds); ≥ 4 procedural music
stations plus a news/talk station (§5.12); mixing with ducking for dialogue and menus.

**D11. Visuals.** Art direction applied (§5.11): time-of-day and weather presets, lit materials,
neon and lit windows at night, VFX for combat, vehicles and weather, camera polish. An on-screen FPS
and frame-time readout is shown by default and can be hidden in settings.

**D12. Performance.** The budgets in §6.9 hold at all six reference views (`gtf perf view=all`) and
in the browser build (sim ms/tick from the headless Chromium stats overlay, §7.4). Quality presets
and dynamic resolution work. Expected frame rate on an RTX 3060 Ti: 60 FPS target, never below 25
on average (user-verified with the on-screen readout; record that the cloud has no GPU).

**D13. Verification.** Bots complete every mission (both heist approaches) on 3 seeds; the
heat-evasion bot escapes 3★ within the time limit; traffic and pedestrian soaks pass their anomaly
limits (§7.5); attract soak passes; snapshot/replay determinism holds with the full city and
population; HUD captures of every screen in §5.9 were taken and looked at.

**D14. Docs.** `README.md` (game, how to play, controls, build/run, Source Information),
`docs/gtf/ENGINE_ANALYSIS.md` (§12.2), `docs/gtf/HANDOFF.md` (how to add districts, missions,
vehicles, weapons, peds, stations, vignettes), engine docs updated for every engine extension
(`docs/MAKE_A_GAME.md`, `docs/REFERENCE.md`, `AGENTS.md` within its size limits), `PROGRESS.md`
complete.

**D15. Delivery.** Branch pushed, draft PR description current, the single-file HTML and the Windows
`.exe` (zipped) sent to the user with `SendUserFile`, and a final chat report with the Source
Information block and the engine analysis summary.

---

## 3. The name

The user asked for a playful version of the source's name: generate 50 titles, pick one at random,
append a 2-character alphanumeric ID as `[GameTitle-XX]` for unique naming in `michaelcrosato/ai-hub`.

The 50 candidates:

| # | Title | # | Title | # | Title | # | Title | # | Title |
|---|---|---|---|---|---|---|---|---|---|
| 1 | Grand Theft Avocado | 11 | Grand Raft Auto | 21 | Grand Theft Espresso | 31 | Grand Theft Roundabout | 41 | Grand Theft Smoothie |
| 2 | Grand Heft Auto | 12 | Grand Graft Auto | 22 | Grand Theft Disco | 32 | Grand Left Auto | 42 | Grand Theft Jalopy |
| 3 | Bland Theft Auto | 13 | Grand Theft Lotto | 23 | Grand Theft Nacho | 33 | Grand Theft Golden Hour | 43 | Grand Theft Joyride |
| 4 | Grand Theft Autopilot | 14 | Grand Theft Moto | 24 | Grand Theft Vinyl | 34 | Grand Theft Auto-Tune | 44 | Grand Theft Mango |
| 5 | Grand Theft Ottoman | 15 | Grand Theft Burrito | 25 | **Grand Theft Freeway** | 35 | Grand Theft Hubcap | 45 | Grand Theft Limo |
| 6 | Grand Daft Auto | 16 | Brand Theft Auto | 26 | Grand Theft Palm Tree | 36 | Grand Theft Cabrio | 46 | Grand Theft Flamingo |
| 7 | Grand Theft Aloha | 17 | Grand Shift Auto | 27 | Sand Theft Auto | 37 | Grand Theft Boulevard | 47 | Grand Theft Neon |
| 8 | Grand Theft Gelato | 18 | Grand Theft Astro | 28 | Grand Theft Nitro | 38 | Grand Theft Loco | 48 | Grand Theft Ego |
| 9 | Grand Theft Taco | 19 | Grand Theft Turbo | 29 | Grand Theft Mojito | 39 | Grand Theft Sunburn | 49 | Grand Theft Piñata |
| 10 | Granite Theft Auto | 20 | Grand Theft Alto | 30 | Grand Hefty Auto | 40 | Grand Theft Avenue | 50 | Grand Theft Ricochet |

**Pick:** Python `secrets.randbelow(50)` chose **#25, Grand Theft Freeway**; `secrets.choice` over
`A–Z0–9` twice gave the ID **BX**.

**The game is _Grand Theft Freeway-BX_.**

| Use | Form |
|---|---|
| Display name / window title / logo | Grand Theft Freeway-BX (logo: "GRAND THEFT FREEWAY", small "BX" badge) |
| Single-file web build and ai-hub file | `GrandTheftFreeway-BX.html` |
| Slug | `grand-theft-freeway-bx` |
| Engine scene key | `freeway` (sub-scenes `freeway/roam`, `freeway/dev`, `freeway/attract`, `freeway/mission/<key>`, `freeway/view/<name>`) |
| Crates | `gtf_game` (rules), `gtf_app` (binaries) |
| Binaries | `grand_theft_freeway` (the game), `gtf` (agent CLI with every `pav` tool plus the game's) |

The title fits the design: the freeway is the slice's spine — the heist happens on it, chases end on
it, and the attract loop opens on it at golden hour.

---

## 4. Engine assessment (planning phase)

Read from the code at the pin and measured in this container (debug build, llvmpipe):

- The starter game's bot wins 3/3 seeds in 0.32 s of wall time; the Drift Circuit room runs at
  **6,886 ticks/s** (22 entities); the pavilion world at **649 ticks/s** (369 entities, 29 terrain
  chunks) — large headroom over 60 ticks/s.
- Pavilion world, one frame: **8,200 mesh instances, 1,136 SDFs, 2,025 draw calls, 483k triangles,
  64 lights** (`capture stats=only`).
- The single-file web build (`scripts/build-web.sh --single-file --scene starter`) builds here in
  **3 min 06 s** and weighs **11.2 MB** (27.8 MB wasm, 8.3 MB gzipped). It boots in headless Chromium
  (SwiftShader WebGPU) in 0.94 s, and the screenshot **includes the egui HUD** (0.34 ms/tick).
- `pav capture` renders the 3D scene only (no egui): the HUD is invisible to fast captures.

| Need for a GTA-like game | Engine at the pin | What this plan does |
|---|---|---|
| A game outside the engine crate | Games compile into `pav_core/src/games/`; `GAMES` is a static list | **E1** runtime registry; the game lives in its own crates (`gtf_game`, `gtf_app`) |
| Rich input | `InputFrame`: world-space `move_dir`, 15 button bits, aim point; gilrs pad, **triggers read as on/off** | **E2** `InputFrame` v2: raw pad (sticks, analog L2/R2, semantic buttons), mouse delta, activity flag, device, game commands |
| Pause, slow-mo | Time scale exists only for Shardfall (`arpg`) | **E3** hook v2: `time_scale()`, `world_paused()` |
| Camera | View-side rig follows the player; "no per-game camera code" | **E4/E6** sim-side game camera publishes a `CameraIntent`; engine camera helpers (spring arm with collision, chase, cinematic shots, sky-cam) |
| HUD and menus | egui in `pav_app`, Shardfall-specific; hook games get text lines only | **E4** `UiModel` (pure data) + **E5** `pav_ui` 10-foot kit; HUD in headless captures |
| Drawing from game state | `pav_view::assets::emit_asset` (code outside `pav_core`) | **E4** `DrawList` (pure data) drawn by `pav_view` |
| Vehicles | Raycast car steering *toward the stick direction*, driven only by the player; arcade helicopter; toy visuals (box + cabin) | **E9** vehicle v2: analog controls for any driver, damage, doors/seats, archetypes as data, prop bodies, dynamic↔kinematic switch for AI LOD |
| A city | 32 m chunk streaming, axis-aligned static blocks, terraced terrain, room files | **E10** `pav_world`: city generator from a macro map, lane and sidewalk graphs, traffic, pedestrians, spawning with LOD |
| AI | Wander/patrol/follow/guard brains, 0.5 m nav grid (A*, flow fields), guard sight cones | Graph-based traffic and pedestrian AI (`pav_world`); police and combat AI in the game; nav grid for local on-foot movement |
| Combat | Lightweight projectiles; `health.rs` for rapier props; no character vitals | Game combat: hitscan via rapier queries, weapons data, vitals, cover |
| Rendering | WebGPU forward renderer: MSAA, cel/lit, 1 sun shadow region + 4 shadowed point lights, 64 lights, bloom, haze/shafts, SSGI, filters, 65k GPU particles, procedural materials. **Flat sky colour, no culling, all instances uploaded every frame** | **E7** sky dome + time of day, frustum/distance culling with persistent region buffers, shadows that follow the view, fake spotlights, world-text fonts |
| Audio | One-shot ADSR synth voices; closed `SimEvent → patch` match | **E8** looping voices with live parameters, data sound bank, FX bus, procedural music + radio, babble voice |
| Fonts | egui default fonts (Ubuntu Light) for UI and world text | OFL fonts (the one binary asset type the doctrine allows) |
| Tools | `pav` CLI/REPL/MCP: capture, filmstrip, bench, record/replay, `play bot=true` | **E12** `capture hud=true`, `ui`, `citymap`, `perf`, `soak`, `vehicle_feel`, `play cmd=` |
| Builds | Linux, Windows `.exe`, web single file | Game binaries, branding, `dist/` HTML |

Strengths to lean on: deterministic fixed-tick sim with snapshots/rewind/replays, headless
everything, the agent tools, a capable renderer, rapier vehicles, a large motion-clip library (pistol
idle/aim/shoot/reload, punches and kicks, sitting, phone talk, dancing, deaths, sprints, 100 walk
styles, climbs), and props made of named primitives (box, rounded box, wedge, extrude, lathe,
cylinder, cone, tube, torus, ellipsoid, capsule, sphere) with procedural materials — enough to author
cars (extruded side profiles), palms, signals and cranes as text.

Constraints to respect (doctrine, `docs/DOCTRINE.md` after vendoring): no binary source assets but
fonts; procedural/text assets; gameplay deterministic and on the CPU; WebGPU only; `pav_core` has no
graphics/window/audio dependencies; game code must run in the browser (no threads, blocking calls or
file access; embed data).

---

## 5. Game design

### 5.1 High concept and pillars

**Three crooks, one freeway, one perfect job.** *Grand Theft Freeway-BX* is a third-person
open-world crime caper in **Paloma Bay**, a sun-bleached coastal district of the fictional city of
**San Paloma**. A getaway driver, a burnt-out heist planner and a stunt performer turned wrecking
ball pull the **Freeway Job** — an armoured-truck heist on the elevated **Route 404** — while a living
city of traffic, pedestrians, police, radio and day/night carries on around them.

Pillars (every feature must serve one):
1. **A city that lives and reacts.** Traffic obeys signals, pedestrians have somewhere to be, crimes
   have witnesses, the sun sets over the pier, the radio never repeats.
2. **Every vehicle is yours.** Seamless on foot ↔ in car: walk up, open the door, drive. Cars feel
   heavy, slide on the handbrake, crumple and burn.
3. **The chase.** Heat escalates readably and fairly; escaping is a skill game of line of sight,
   routes and hiding.
4. **Plan it, then pull it.** Setups, a planning board with two approaches that play differently,
   and a heist that switches between three protagonists.
5. **Golden-hour satire.** Style and humour: billboards, brands, radio, news; magic-hour light.

### 5.2 What we keep from GTA V and what we modernize

| GTA V signature | Keep | Modernize / improve |
|---|---|---|
| Wanted stars, police cones | 5 levels, search zone, vision cones on the radar, flashing stars | Witnesses must report crimes (intimidate or outrun them); police are **dispatched from stations along roads**, never spawned in view; respray and hide-spots are explicit; heat reasons shown on screen ("Reported: car theft") |
| Three protagonists, sky-cam switch | Switch wheel, sky-cam | Switch in ≤ 3 s with preloaded streaming; mission switch prompts flash the wheel slot |
| Heists | Setups, approach choice, payout | Two approaches with visibly different gameplay; planning board as a big-card 10-foot screen |
| Arcade driving | Handbrake slides, chase cam, radio | Analog triggers, steering assist option, speed-sensitive steering, weight transfer you can feel; cinematic cam; drift and near-miss feedback |
| Cover shooting | Cover, aim assist, weapon wheel slow-mo | Cover snap preview (outline where you will land), adjustable aim assist (off/light/standard/strong), hit markers, clearer damage direction |
| Missions | Markers, yellow objectives, medals, checkpoints, fail screen, skip | Medal goals shown **before** the mission, ≤ 1 s checkpoint retry, skip offer after 3 fails, missions start near their marker (no 3-minute empty drives), optional "skip trip" taxi from the phone (stretch) |
| Phone | Contacts, texts, camera | Big-tile phone sized for the TV; the camera is used in a setup mission |
| GPS | Minimap route | Route **ribbon on the road surface** in the world plus the minimap route; off-screen objective arrows |
| Radio | Stations, DJs, ads | Procedural stations that never repeat; satirical news/talk as subtitles; song titles on the HUD |
| HUD | Minimal | Contextual fading, HUD scale and safe-area settings, colourblind-safe blip palette |
| Wasted / Busted | Slow-mo banner, hospital/station respawn with fee | Same, with "Retry from checkpoint" first in missions |
| (none) | — | **Attract mode**: the real game, AI-played, as a kiosk loop |

### 5.3 Setting: San Paloma and the Paloma Bay district

San Paloma is an original Southern-California-style metropolis — palms, stucco, smog, freeways,
beaches, hills with a giant sign — seen through satire of car culture, tech hype, influencers, real
estate and traffic. No real brands, places or people.

**Paloma Bay** is the slice: **1024 m × 1024 m** (32 × 32 streaming chunks of 32 m). Coarse zoning
sketch, 16 × 16 tiles of 64 m, north up (engine: x east, z south):

```
      0 1 2 3 4 5 6 7 8 9 A B C D E F
  0   H H H H H H H S S H H H H H F F    H  Mirador Hills (terrain to +45 m), winding Mirador Drive
  1   H H H r r r r r r H H H H H F F    S  PALOMA sign + Lookout Point (vista, collectible, jump)
  2   r r r r r r r r r r r r r c F F    r  Los Mirados bungalows (driveways, palms)
  3   r r r r p p r r r r r r c c F F    p  Mirador Park
  4   c c c c c c c c c c c c c c P F    c  Sunset Row commercial strip (W-E boulevard)
  5   c c D D D D D D D c c d d d F F    P  Pretzel interchange (Sunset Row <-> Route 404 ramps)
  6   c c D D B B D D D c c t t t F F    D  Downtown towers   B  Paloma Savings & Trust + plaza
  7   c c D D S D D D M c t t t t F F    S  SPPD Central   M  St. Brigid Medical (helipad)
  8   b b c c c c c c c c t t C C F F    d  Luxe Motors and auto row   t  The Flats (industrial)
  9   b b b c c G c c c t t t C t F F    C  The Ditch (concrete storm channel, NE to harbor)
  A   s s b b c c c c t t t C t t F F    G  Bang Bang Bros. + Gas-N-Grab   b  beach front
  B   s s s b b c c t t t C t h h F F    s  Palisade Beach sand   h  Harbor Gate docks
  C   w s s s b b Q t t C t h h h F F    Q  Palisade Pier (Big Wheel)
  D   w w s s s Q Q Q t C h h h h F h    F  Route 404 (elevated, east edge, turns west at row E)
  E   w w w s s w w w w h h F F F F h    F  ...the Harbor Spur over the docks, ending at the
  F   w w w w w w w w w w w w w w w w       broken construction ramp above the water (row E, col B)
```

The implementer authors the real 64 × 64 macro map (16 m cells, §6.8.1) from this sketch; exact
placement may change for road connectivity and gameplay. Boundaries: ocean south and west, steep
hills north, the freeway corridor and a "ROAD CLOSED — FREEWAY WIDENING" barrier east.

**Landmarks and their gameplay roles**

| Landmark | Zone | Role |
|---|---|---|
| Paloma Savings & Trust tower + City Hall plaza | Downtown | Skyline anchor; plaza crowds; parking garage (multi-level, verticality); future-heist teaser |
| SPPD Central | Downtown | Busted respawn; police dispatch origin; impound lot |
| St. Brigid Medical + helipad | Downtown | Wasted respawn; news helicopter on the roof (flyable) |
| Bang Bang Bros. (interior) | Sunset Row | Gun store: weapons, ammo, armour |
| Paloma Customs (interior bay) | Sunset Row | Respray (clears heat if unseen), repair, paint |
| Gas-N-Grab (interior) + pumps | Sunset Row | Store hold-up; pumps explode |
| Grind House coffee (interior) | Sunset Row | Second hold-up |
| Freeway Diner | Sunset Row | Mack's hangout; mission M2 |
| Luxe Motors showroom (glass) | Auto row | Setup S3 smash-and-grab |
| Ironclad Transit depot | The Flats | Armoured trucks' base; heist route origin; setup S1 |
| City Works yard | The Flats | Utility trucks; setup S2 |
| The Ditch | The Flats → harbor | Concrete channel with ramps: chases, jumps, Smart escape route |
| Harbor Gate: containers, gantry cranes, warehouses, a moored freighter hull | Docks | Night gunfights; heist Smart-approach warehouse |
| Palisade Pier + the Big Wheel (rotating), arcade facade, food stalls | Beach | Crowds, attract shots, M3 |
| Palisade Beach: lifeguard towers, volleyball, muscle beach, boardwalk | Beach | City life; jumps |
| Mirador Drive, Lookout Point, the PALOMA sign | Hills | Vista, collectible, jump, attract shot |
| Route 404, the Pretzel, the Harbor Spur, the broken ramp | Freeway | Chases, race, heist, stunt jump |
| Safehouses: Dez's apartment (Los Mirados), Mack's house (hills), Roxy's trailer (The Flats) | — | Save, wardrobe (palette), character idle spots |

### 5.4 Protagonists

| | **Dez Ortega** | **Mack Kincaid** | **Roxy Vance** |
|---|---|---|---|
| Who | 24, repo driver with a five-year plan and a lowrider dream | 52, retired heist planner turned strip-mall "success coach" | 33, stunt performer fired from *Explosion 7* for "excessive realism" |
| Role in heists | Wheel | Brain | Muscle |
| Ability | **Slipstream** — driving slow-mo (time × 0.5), +40% grip, steady camera | **Deadeye** — on-foot bullet time (time × 0.35), aim assist boost, damage × 1.5 | **Overdrive** — damage dealt × 2, taken × 0.5, melee knockback × 2, red-edged screen |
| Meter fills by | Near misses, drifts, airtime | Headshots, kills, cover takedowns | Damage dealt and taken, explosions |
| Stat bias | Driving | Shooting | Strength, stamina |
| Signature vehicle | Bravura Stallion (muscle) | Daimon Titan (SUV) | Marlin Bay (pickup) |
| Safehouse | Los Mirados apartment | Mirador Hills house | Trailer in The Flats |
| Look (puppet) | Slim; denim jacket, white tee, cap | Broad; navy blazer over polo, grey stubble | Athletic; red racing jacket, ponytail, aviators |

Abilities last up to 10 s, drain while active and refill by play style. Activation: L3 + R3 (§5.10).
Each protagonist keeps their own cash, weapons, stats and position. Inactive protagonists are parked
at a "life spot" (an activity at a scenario point) chosen from their profile; switching streams their
area in and cuts to them mid-activity.

### 5.5 Core loops

- **Moment (seconds):** move or drive → spot an opportunity or threat → act (steal, shoot, evade,
  drift) → feedback (heat, cash, meter, juice).
- **Session (minutes):** pick a blip (mission, activity, shop) → route with the GPS ribbon → execute
  → escape heat → reward → autosave; save at a safehouse.
- **Slice progression (60–90 min of content plus free roam):** one intro mission per protagonist →
  switching unlocked → three heist setups → planning board → the Freeway Job → payout → free roam
  with every activity, collectibles, 100% completion.

### 5.6 Systems design

#### 5.6.1 On foot
- Speeds: walk 1.6 m/s (light stick), run 4.6 m/s, sprint 7.0 m/s while Cross is held or tapped
  (stamina 10 s at stamina stat 0, +50% at 100; regenerates 2 s per s after 1 s rest).
- Jump 1.1 m; vault obstacles ≤ 1.2 m automatically while running into them; ledge grab and climb up
  to 2.2 m (engine ledge grab + `QUATERNIUS/ClimbUp_1m` / `MESH2MOTION/Climb Wall`); swim (engine);
  stealth crouch halves detection range and footstep noise.
- Melee: light combo (jab, cross, hook: `Punch_Jab`, `Punch_Cross`, a kick), heavy on hold; bat
  swings; counters are stretch.
- Cover (R1): from ≤ 3 m of a wall or prop edge (pre-generated cover points, §6.6.6), a preview
  outline shows the landing spot; slide in; move along with the stick; corners swap sides; L2 peeks
  and aims; R2 without L2 blind-fires (spread × 3); R1 again leaves.
- Interact (D-pad right): context prompts (shop counters, save bed, mission givers, pick-ups).
- Falling: from > 6 m the character tumbles (fall clip) and takes damage (> 15 m lethal).

#### 5.6.2 Camera (sim-side, §6.5 E4/E6)
- **On foot:** orbit at 3.8 m behind and 1.5 m up, 0.45 m right shoulder; right stick yaw/pitch
  with acceleration curves (settings: sensitivity, invert); recentres behind movement after 2 s
  without look input (setting); sphere-cast collision pulls in smoothly; sprint widens FOV by 5°.
- **Aim (L2):** 1.6 m distance, FOV 45°, 0.6 m shoulder offset (R3 swaps shoulder), crosshair dot
  that blooms with spread.
- **Vehicle chase:** distance and height from the vehicle's size (5.5–9 m, 1.8–2.6 m), yaw spring
  with lag, velocity look-ahead, FOV 60° → 75° with speed, slight roll in drifts, collision pull-in;
  right stick looks around and returns after 1.5 s; R3 looks behind.
- **Cinematic (hold Circle in a vehicle):** cycles shot types (bumper, low tracking, side dolly,
  aerial chase, roadside tripod) every 3–6 s.
- **Sky-cam switch:** rise to 300 m, pan across the map with a desaturated grid overlay, descend on
  the target protagonist: 3 stages, ≤ 3 s total.
- **Scripted shots:** cutscenes and the attract director give anchor, offset, look-at, FOV, duration
  and easing (§6.8.6).
- **Wasted/Busted:** slow orbit around the player at 0.3× time.

#### 5.6.3 Vehicles
Roster (all from `content/freeway/vehicles.toml`, bodies authored as props, §6.8.2):

| Key | Name | Class | Top speed | 0–100 km/h | Notes |
|---|---|---|---|---|---|
| `fenwick_commuter` | Fenwick Commuter | compact | 30 m/s | 9.0 s | Most common traffic |
| `zephyr_wagon` | Zephyr Wagon | wagon | 31 m/s | 8.5 s | "Dad car" |
| `okami_kaze` | Okami Kaze | sports | 42 m/s | 3.8 s | S3 getaway car |
| `bravura_stallion` | Bravura Stallion | muscle | 38 m/s | 4.6 s | Dez's car; burnouts |
| `daimon_titan` | Daimon Titan | SUV | 31 m/s | 7.0 s | Mack's car; heavy |
| `marlin_bay` | Marlin Bay | pickup | 30 m/s | 7.5 s | Roxy's truck; open bed |
| `fenwick_courier` | Fenwick Courier | van | 26 m/s | 11 s | Deliveries |
| `paloma_cab` | Paloma Cab | taxi | 32 m/s | 8.0 s | Yellow sedan, roof sign |
| `sppd_interceptor` | SPPD Interceptor | police | 40 m/s | 5.0 s | Light bar, siren, vigilante |
| `metro_bus` | Metro Bus | bus | 20 m/s | 18 s | Runs a route; long chase obstacle |
| `ironclad_armored` | Ironclad Armored Truck | armoured | 25 m/s | 14 s | Heist target; rams cars aside |
| `city_works` | City Works Utility Truck | utility | 24 m/s | 13 s | Setup S2; Loud blockade |
| `sppd_tactical` | SPPD Tactical Van | police | 30 m/s | 9.0 s | Appears at 4★ |
| `skylark_news` | Skylark News Helicopter | helicopter | 28 m/s | — | Flyable from the hospital roof |
| `sppd_air` | SPPD Air Unit | helicopter | 30 m/s | — | AI at 3★+, searchlight |

- **Controls:** R2 throttle and L2 brake/reverse (analog), stick steers with speed-sensitive lock
  (full lock below 8 m/s, 35% at top speed; "steering assist" setting adds counter-steer), R1
  handbrake (rear grip drops to `drift_grip`), burnout when R2 + L2 at standstill (muscle cars).
  Keyboard: W/S throttle/brake ramp to full in 0.15 s; A/D steer with a 0.12 s ramp.
- **Feel targets** (checked by `vehicle_feel`, §7.3): 0–100 km/h and top speed within ±10% of the
  table; braking 100 → 0 km/h in 38–50 m (compact) and 32–40 m (sports); lateral grip 0.9–1.1 g at
  25 m/s; a held handbrake at 20 m/s gives a 25–45° slide angle; no roll-overs on flat roads (cars
  roll only on ramps and big impacts; a car upside down for 3 s rights itself, unless it is burning,
  in which case it explodes).
- **Enter/exit (≤ 1.2 s, cancellable):** walk to the nearest door (auto-steer ≤ 1 s), the door part
  swings open, the character slides in (sitting clip), the door closes. Exit reverses; holding
  Triangle above 15 m/s bails out with a roll.
- **Carjacking:** an occupied car's door opens, the driver is pulled out (knock-down, then flees or
  fights by archetype); police see it → 1★.
- **Parked cars:** 70% unlocked; a locked car's window breaks (glass VFX, noise: nearby peds may
  report). Hotwire is a 1 s shake of the camera.
- **Damage:** health 1000 (by class). Collisions deal damage ∝ impulse above a threshold; bullets
  hit the engine area. < 40% smoke, < 10% fire, then explosion after 5 s (radius 6 m, kills
  occupants, chains to nearby cars after 0.5 s). Four crumple zones (front/back/left/right) offset
  and tilt body parts; headlights/tail lights break past a front/back crumple threshold; tyres burst
  when shot (grip × 0.4, sparks on the rim).
- **Lights and sounds:** headlights on at dusk (driven car gets a real light; others emissive +
  pooled lights), brake lights, reverse lights, indicators on AI turns; horn; police light bar with
  red/blue point lights and a siren (wail/yelp, toggled with L3 in police cars).
- **Occupants:** every occupied vehicle shows seated puppets (sitting clip, upper body) behind the
  glass: drivers, passengers, police.
- **Water:** vehicles that enter deep water stall, float 3 s and sink; the occupant swims out.
- **Helicopter:** engine arcade model with game controls (L2/R2 descend/climb, stick moves, right
  stick yaws); AI helicopters orbit, track and light their target.

#### 5.6.4 Combat
| Weapon | Slot | Damage | Rate | Magazine | Range | Notes |
|---|---|---|---|---|---|---|
| Fists | melee | 15 | — | — | 1.5 m | 3-hit combo |
| Bat | melee | 35 | — | — | 2.0 m | knock-down |
| P9 pistol | pistol | 26 | 300 rpm | 12 | 40 m | head × 3 |
| Buzz SMG | smg | 18 | 800 rpm | 30 | 30 m | drive-by |
| Pump shotgun | shotgun | 9 × 10 pellets | 70 rpm | 8 | 15 m | knock-back |
| C-4R carbine | rifle | 30 | 600 rpm | 30 | 70 m | |
| Grenade | thrown | 200, 6 m radius | — | 5 carried | arc | 3 s fuse, preview arc |
| Breach charge | special | doors / truck | — | mission | — | heist Loud |

- Hitscan from the camera's aim ray to the hit point, then a muzzle-to-hit-point check for walls;
  tracers, muzzle flash light and particles, impact sparks/dust/decals, stylized red puffs on people
  (no gore). Recoil kicks the camera pitch; spread grows with fire and movement, shrinks in cover.
- Aim assist (setting off/light/standard/strong): on L2 press the reticle snaps to the nearest
  target in a 12° cone within range; friction slows the camera on targets; magnetism tracks gently.
- Player health 200 (regenerates to 50% after 5 s unharmed), armour 100 absorbs 70% of damage;
  NPCs 100 (police 120, guards 150 with armour). Hit reactions (flinch), knock-downs, death clips
  (`Death01`, `Death_A…D`, `Fall_On_Face`).
- Enemy AI: take cover points facing the threat, peek-and-shoot cycles, flank when the player stays
  in one cover > 8 s, throw grenades at 4★+ (police) or in missions, retreat when hurt (gangs).
- Explosions: vehicles, gas pumps, propane tanks, grenades; knock-back and screen shake by distance.

#### 5.6.5 Heat (wanted level)
- **Crimes** (each has a base star value): assault (1), witnessed car theft or break-in (1), weapon
  brandished near police (1), shooting near people (2), killing a civilian (2), hold-up (2), attacking
  police (3), killing police (4), destroying a police vehicle or helicopter (4–5).
- **Reporting:** police who see a crime report it instantly. Civilians who see it (line of sight,
  ≤ 30 m) flee and phone it in after 4–8 s (phone clip, a "reporting" icon over them) unless
  intimidated (aimed at), killed, or out of range when the call ends. Unreported crimes add no heat.
- **Levels:**

| Stars | Response | Tactics | Search radius |
|---|---|---|---|
| 1★ | 1 patrol car | Try to arrest; non-lethal unless fired upon | 120 m |
| 2★ | 2–3 cars | Lethal force; block the player's car | 160 m |
| 3★ | 4 cars + SPPD Air (searchlight at night) | Ramming (PIT), occasional roadblocks | 220 m |
| 4★ | 6 cars + tactical van + air with a marksman | Roadblocks, spike strips, foot units with rifles | 300 m |
| 5★ | 8 cars, 2 air units, tactical SUVs | Freeway roadblocks, grenades | 400 m |

- **Dispatch:** units spawn **out of view** at SPPD Central, at lane spawn points on the network edge
  of the active area, or as already-patrolling cars; they path along the lane graph to the last known
  position. Never spawn inside the camera frustum or within 80 m.
- **Search:** when no officer has had line of sight for 2 s the stars flash and a search zone opens
  on the last known position (radius by level, drifting with sightings). Officers search waypoints in
  the zone with vision cones (minimap). Leaving the zone and staying unseen for 5 + 5 × stars
  seconds — or staying hidden inside it for twice that — clears the heat. A new sighting re-centres
  the zone and makes the stars solid.
- **Evade aids:** respray at Paloma Customs while unseen clears heat (fee $200); switching to a car
  the police haven't seen while unseen drops one star; hide spots (alleys, under the freeway, garages,
  the Ditch's culverts) halve detection range.
- **Busted:** at ≤ 2★ an officer within 2 m of a slow or stopped player on foot for 2 s arrests
  them (hands-up clip, authored in M5) → "BUSTED" → respawn at SPPD Central, fee 10% of cash (max
  $500), lose carried ammo except 1 magazine.
- **Wasted:** health 0 → 0.3× time, desaturation, "WASTED" → respawn at St. Brigid, fee $250.
- **UI:** stars top-right (flashing when searching), the reason of the last report as a short
  toast, cones and search circle on the minimap, police blips (red/blue flicker).
- Missions can floor or cap heat and disable dispatch (e.g. during a scripted getaway).

#### 5.6.6 Traffic
- Lanes from the generator (§6.6.2). Speed limits: alley 8, street 14, avenue 17, hill road 12,
  freeway 28 m/s; AI drives at 85–110% of the limit by driver personality.
- Car following with the Intelligent Driver Model; lane changes on multi-lane roads (gap acceptance
  and a politeness factor); traffic signals (two phases plus all-red; left turns on permissive green
  yield to oncoming traffic with gap acceptance); stop lines; yielding to pedestrians in crosswalks;
  horn after 3 s blocked; reverse-and-go-around after 8 s blocked; give up and despawn when hidden and
  blocked for 30 s.
- Reactions: near miss by the player → brake hard or swerve; gunfire within 40 m → flee at speed or
  bail out on foot; collision with the player → stop, driver gets out angry (fight, film or flee by
  archetype); sirens behind → pull to the curb lane and stop until they pass.
- Density by zone and time (downtown rush hours highest, 03:00–05:00 lowest): 30–45 active cars
  within 220 m. Spawn on lanes 120–220 m away and out of view; despawn beyond 260 m when not visible.
- **AI LOD:** full rapier vehicle within 120 m of the player or the camera target, or while in a
  pursuit or collision; **kinematic "rail" movement** beyond (positions follow the lane at IDM speed,
  colliders still present); switch back to dynamic on any contact.
- Buses follow a route with bus stops (pedestrians board and alight as a visual: they despawn at the
  door and spawn when leaving). Parked cars fill parking lanes and lots as sleeping bodies.

#### 5.6.7 Pedestrians
- Sidewalk graph around every block with crosswalk edges (signal-controlled at intersections) and
  scenario points: benches, bus stops, ATMs, shop windows, food stalls, beach towels, volleyball nets,
  viewpoints, street performers.
- Archetypes (`content/freeway/peds.toml`): local, tourist, jogger, businessperson, beachgoer,
  skater (stretch), influencer, worker, security guard, gang member, police officer, paramedic
  (stretch). Each has palettes, proportions, accessories, a walk style (`STYLE100/*` clips: phone,
  hands in pockets, proud, old, swat...) and behaviour weights.
- Behaviours: walk to a destination, jog, talk on the phone, chat in groups (`Idle_Talking`), sit
  (`Sitting_*`), take photos (tourists), dance on the beach, watch a performer.
- Reactions: gunfire within 40 m or a weapon aimed → flee along the graph away from the threat;
  cornered → cower (crouch clip); tough archetypes fight back (fists or a pistol); influencers film
  (phone clip) — and count as witnesses; struck by a car → knock-down (fall clip) then get up, or die
  above 12 m/s; bumped → angry line.
- Density by zone and time: 40–70 active within 120 m; spawn 60–120 m away out of view.
- **AI LOD:** full character controller within 60 m; beyond that, kinematic movement along the graph
  with no rapier controller; animation drops to every other tick beyond 40 m.

#### 5.6.8 Missions and the heist
- Missions are data (§6.8.6): spawns, steps, fail conditions, checkpoints, medals, dialogue, shots.
- Starting: a coloured letter blip (D, M, R for the protagonists; H for heist) and a ground marker;
  enter it on foot or in a vehicle → intro card with title, protagonist and medal goals → go.
- Objectives: one line, bottom-centre, key words highlighted (yellow targets, blue destinations,
  red enemies, green allies). Dialogue as subtitles with the speaker's name and a babble voice.
- Checkpoints at step boundaries; fail → reason card → **Retry** (≤ 1 s, from the checkpoint) or
  **Quit**; after 3 fails at one checkpoint, **Skip** is offered.
- Medals (gold/silver/bronze by goals met), rewards (cash, unlocks), replay from the pause menu.
- Character switching in missions: the step names allowed switches; the wheel slot pulses.

#### 5.6.9 Phone (D-pad up)
A tall phone, bottom-right, sized for the TV (≈ 45% of screen height), with a 3 × 3 grid of big
tiles: Contacts (call to start missions or the heist board), Messages (unlocks, story texts, spam
satire), Camera (framed photo mode; mission targets register when centred and in range), Map (set a
waypoint to the nearest shop or activity), Jobs (available missions and activities with distance),
Radio, Settings (opens pause settings), Gallery (taken photos, metadata only), and a satirical app
(*LifeHakr* tips). D-pad/stick moves, Cross opens, Circle backs out.

#### 5.6.10 Economy
- Starting cash: Dez $350, Mack $2,500, Roxy $120. Income: missions ($800–$3,000), the heist (total
  take $180,000, cut by approach and choices), hold-ups ($200–$900), races ($500/$1,000 bets),
  vigilante ($250 per takedown), collectibles (all 15: $10,000 and a gold Okami Kaze in Dez's
  garage).
- Costs: weapons $250–$3,500, ammo, armour $500, respray $200, repair $150–$500, hospital $250,
  busted fee, snacks $5–$20 (heal).

#### 5.6.11 Activities
- **Street race — "Pretzel Run":** 3 laps through the interchange, Sunset Row and downtown against 3
  AI racers (moderate rubber-banding), bets $500 or $1,000.
- **Stunt jumps × 5:** the Ditch jump, the freeway gap, the pier ramp, the hills jump, the harbor
  container ramp. Airborne through the gate above the minimum speed → slow-mo camera; landing inside
  the zone → "UNIQUE STUNT BONUS".
- **Store hold-ups × 2:** aim at the clerk → the bag fills (aiming close speeds it up) → 2★ on exit.
- **Vigilante:** in a police car press L3 + Cross (keyboard: B) to start; take down fleeing suspects
  in cars or on foot, levels of rising difficulty; ends on death, arrest of the player or leaving the car.
- **Random events × 2:** *Purse Snatcher* (chase on foot, return the purse or keep it) and
  *Getaway Needed* (a robber begs for a ride; lose the cops).
- **Collectibles × 15 "Lost Shields":** Route 404 shield signs hidden around the district; each
  shows a satirical highway-trivia card.

#### 5.6.12 World: time and weather
- 1 game minute = 2 real seconds (48-minute day). New games start at 17:20 (golden hour soon).
- Weather states: clear, hazy (smog), overcast, rain, storm (lightning flashes, thunder), fog, with
  2-minute transitions every 3–6 game hours; missions and vignettes may lock time and weather.
- Rain: GPU particles near the camera, wet roads (darker, glossier, puddle mask), splashes, wipers
  (stretch), fewer pedestrians (umbrella walk style if available).

#### 5.6.13 Radio
- In any vehicle with a radio: the last station plays; D-pad left/right steps stations, holding
  D-pad left opens the radio wheel. On entering a car the station name and the current song card
  ("*Freeway Dreams* — The Tarmacs") show for 4 s.
- Stations (§5.12): Freeway FM 104.04, Westside 99, K-SURF 101.5, Radio Sol 92.3, Smooth 88.8 (≥ 4
  music stations required; 5 planned) and SPNN 1040 AM (news and talk).
- Police scanner chatter (babble + beeps + text) under the music at 2★+.

#### 5.6.14 Saving
- Save at a safehouse bed (D-pad right) into 3 slots; autosave after each mission, activity and
  heist step. Save data (§6.10): per protagonist (cash, weapons, ammo, stats, position, outfit
  palette), mission progress and medals, activities, collectibles, completion, settings snapshot,
  time and weather, game version.

### 5.7 Content list (the vertical slice)

**Story missions** (protagonist; teaches; key systems):

| ID | Title | Who | Beats | Teaches / shows |
|---|---|---|---|---|
| `m1_repo_man` | Repo Man | Dez | Steal a Bravura from a Los Mirados driveway, outrun the furious owner, deliver to Paloma Customs | Driving, handbrake, GPS ribbon, radio, respray |
| `m2_midlife` | Midlife Crisis | Mack | A deal at the Freeway Diner goes bad: melee, cover shootout in the parking lot, lose 2★ | Cover, aim, weapon wheel, heat evasion |
| `m3_overdrive` | Overdrive | Roxy | Wreck a rival's taco trucks on the boardwalk, 3★ with the helicopter, escape down the Ditch; switch to Dez for the getaway | Abilities, chaos, helicopter, first switch |
| `s1_rush_hour` | Rush Hour | Mack | Tail an Ironclad truck through traffic without being spotted, photograph its route sheet and guards at the depot | Follow, stealth, phone camera |
| `s2_heavy_metal` | Heavy Metal | Roxy | Steal a City Works utility truck from the yard (quiet or loud) and park it under the freeway | Stealth/loud choice, heavy vehicles |
| `s3_test_drive` | Test Drive | Dez | Smash through the Luxe Motors window in the Okami Kaze, lose 2★, stash it at the construction ramp | Glass, chase, stunt |
| `h0_board` | The Board | Mack | The planning board: choose **Loud** or **Smart**, see the cut | Board UI |
| `h1_freeway_job` | The Freeway Job | All | **Loud:** block the truck on Route 404 with the utility truck, breach it, grab the bags, hold off 4★, jump the broken ramp into the harbor road, switch to Dez to lose the cops. **Smart:** reroute the truck with fake roadwork in the Flats, take it quietly into the harbor warehouse, swap the cargo, leave in disguise at ≤ 1★ | Everything; switching under pressure |

**Side content:** Pretzel Run race; 5 stunt jumps; hold-ups at Gas-N-Grab and Grind House; vigilante;
Purse Snatcher and Getaway Needed random events; 15 Lost Shields; ambient life (billboards, the Big
Wheel turning, buskers, beach volleyball, the news helicopter over downtown at noon).

**Completion %:** story 50%, activities 30%, collectibles 10%, medals 10%.

### 5.8 Attract mode (store demo)

- **Trigger:** 10 s (600 ticks) without any input on the title screen or any main-menu page. Input
  means any device activity the host sees: a key, a mouse move or click, a pad button or stick past
  its dead zone, a touch (§6.5 E2 `activity`).
- **Exit:** any input during attract returns to the title at once (≤ 0.25 s fade). The input that
  exits does not also act on the title menu.
- **Content:** a playlist of vignettes (`content/freeway/attract.toml`). Each sets a place, time,
  weather, protagonist, vehicle, heat, population seed, a bot policy and a shot list for the camera
  director, and runs 30–60 s. Between vignettes a 1 s cut through black with a caption card.

| Vignette | Place / time | Shows |
|---|---|---|
| `golden_cruise` | Route 404 → Sunset Row, golden hour | Driving, traffic, radio card, chase cam + cinematic shots |
| `hot_pursuit` | Downtown → The Ditch, night | 3★ chase, roadblock, helicopter searchlight, evasion with flashing stars |
| `dock_fight` | Harbor Gate, dusk, rain | Cover shooting, grenades, Deadeye, police arriving |
| `switch` | Lookout → pier → Flats, afternoon | Sky-cam switching between the three protagonists mid-activity |
| `big_air` | The broken ramp, sunset | Stunt jump with slow-mo camera, Slipstream |
| `city_life` | Boardwalk and pier, morning → noon time-lapse | Crowds, the Big Wheel, buskers, beach, pedestrians reacting to a purse snatcher |
| `the_job` | Route 404, night | Heist getaway excerpt: armoured truck, bags, 4★, switch to the driver |

- Order shuffles per loop from the sim's random generator; seeds vary; captions call out features
  ("A CITY THAT REACTS", "THREE CRIMINALS. ONE JOB.").
- HUD: the game logo bottom-left, "DEMO" and "PRESS ANY BUTTON" (with the device's glyph) bottom-
  right; the normal HUD (minimap, stars) appears during chases to show it off; the FPS readout stays.
- **Touch devices (phones, tablets):** start straight into the attract loop and never leave it; the
  only UI is a small gear button (top-right, 56 px tap target) opening an overlay with Sound on/off,
  Graphics quality, About, and Close. No touch controls. (The engine's touch stick and button fan
  are disabled for this game.)
- Robustness: a vignette that stalls (bot stuck > 6 s, or a goal not reached in its time) ends early
  and logs an anomaly; the attract soak (§7.5) counts them.

### 5.9 Frontend and the 10-foot UI

**Screens**
- Boot splash (Pavilion engine mark, then the game logo, ≤ 3 s, skippable).
- Title: the live city at golden hour from the hills (slow drone shot over the freeway), the logo,
  "PRESS ANY BUTTON". Then the main menu: **Continue**, **New Game**, **Free Roam** (after the intro
  or always in this slice), **Settings**, **About**, **Quit** (native only).
- Loading/intro cards: full-screen illustration made in-engine (a protagonist pose under the
  halftone or painterly filter, §5.11) with a tip line.
- Pause (Options): tabs **Map | Brief | Stats | Settings | Game**, cycled with L1/R1.
  - Map: the generated map, zoom (L2/R2), pan (left stick), cursor snaps to blips, Cross sets a
    waypoint, Square toggles the legend, Triangle filters blip types.
  - Brief: current objective, mission log, recent texts — large type, ≤ 6 rows.
  - Stats: big cards (one per protagonist: cash and 5 stat bars; one for completion with 4 numbers;
    one for collectibles). No tables.
  - Settings: Audio (master, radio, SFX, dialogue, default station), Display (quality preset,
    render scale, v-sync, FPS readout, HUD scale, HUD safe-area margin, brightness), Controls (aim
    assist, camera sensitivity X/Y, invert Y, vibration, sprint hold/toggle, glyph style auto/PS/
    Xbox/keyboard), Accessibility (subtitles on/off and size, colourblind-safe blips, reduce camera
    shake, high-contrast HUD, weapon-wheel slow-mo on/off).
  - Game: Save (at safehouses), Load, Restart checkpoint (in missions), Quit to title.
- Phone, wheels, planning board, shop menus (gun store: categories as big tiles, a 3D turntable of
  the weapon, price and stats as 3 bars), banners (MISSION PASSED + medal + cash, WASTED, BUSTED,
  UNIQUE STUNT BONUS), loading cards, credits/About.

**HUD layout** (16:9, inside a 90% safe area; positions scale with HUD scale)
```
+------------------------------------------------------------------------------+
| 60 FPS 16.6 ms                                         $12,480   +$1,500      |
| [help box: tutorial tips, 2 lines max]                 ★★★☆☆   (flash = search)|
|                                                        [P9  12 / 48]          |
|                                                        [notification toast]   |
|                                  ( + )   crosshair                            |
|                                                                               |
|  .-------------.                                                              |
| |   MINIMAP     |          Lose the cops.                         △ Enter      |
| |  route, cones |   MACK: "Nobody panic. I'm panicking."         Okami Kaze    |
|  '-------------'                                                 FREEWAY FM   |
|  [health][armour][ability]                                                    |
+------------------------------------------------------------------------------+
```
- Contextual: money shows on change and in menus; weapon/ammo when armed, aiming or changed; stars
  only when wanted; the minimap always (it fades to 70% when idle on foot).

**10-foot rules** (D7; check every screen against them)
- Legible at 3 m on a 55" TV at 1080p: body text ≥ 28 px, HUD numbers ≥ 40 px, titles 64–120 px
  (scale with window height, 1080p reference); heavy weights; text on translucent dark backplates or
  with a 2 px shadow over the world.
- 5% margins (HUD safe-area setting 0–10%); critical text inside 85% title-safe.
- Every interactive element reachable with the D-pad / left stick; exactly one focused element,
  shown by accent fill + 1.05 scale + glow; wrap-around lists; Cross accepts, Circle backs, L1/R1
  switch tabs, L2/R2 page or zoom; a button-hint bar (glyph + verb) bottom-right of every menu.
- Short lists (≤ 7 items), big tiles and cards; **no tables, no dense grids, no scrolling walls of
  text**; numbers as bars or big digits.
- Radial wheels with 8 segments: left stick selects (≥ 0.5 deflection), release confirms; time slows
  to 25% (setting).
- Motion: 150–250 ms ease-out transitions; synthesized UI sounds for move, accept, back, error.
- Keyboard and mouse map onto the same model: arrows/WASD move focus, Enter/Space accept, Esc back,
  Q/E tabs; the mouse hovers to focus and clicks to accept; never a desktop-style widget.
- Glyphs: drawn procedurally (no images): PlayStation (✕ ○ □ △, L1/R1/L2/R2, Options, Create,
  touchpad), Xbox (A B X Y, LB/RB/LT/RT, Menu, View) and keyboard keycaps; PlayStation by default;
  the set follows the last device used (setting can force one).
- Fonts (OFL, `assets/fonts/` with licence files): a condensed display face for the logo, banners and
  big numbers (candidates: Anton, Bebas Neue, Passion One — pick by a capture comparison) and a sturdy
  UI face for body text (Barlow Semi Condensed SemiBold/Bold). World signage may use a script face
  (Pacifico) for neon and a highway face (Barlow) for road signs.
- Colour: dark slate backplates (#0e1116 at 72%), sunset accent (#ff7a3d → #ff3e8a gradient),
  text #f4f1ea, positive #6fe28a, warning #ffc145, danger #ff4d4d, police #3d8bff / #ff3d3d.

### 5.10 Controls (PlayStation 5 primary; Xbox and keyboard map onto the same actions)

| Action | On foot | In a vehicle | Keyboard / mouse |
|---|---|---|---|
| Move / steer | Left stick (tilt = walk → run) | Left stick X | W A S D |
| Camera / look | Right stick | Right stick (returns behind) | Mouse |
| Sprint | ✕ (hold or tap) | — | Shift |
| Jump / climb / vault | □ | — | Space |
| Throttle | — | R2 (analog) | W |
| Brake / reverse | — | L2 (analog) | S |
| Handbrake | — | R1 | Space |
| Enter / exit / carjack | △ | △ (hold to bail) | F |
| Melee / reload | ○ (melee unarmed or close; reload when armed) | — | R (reload), LMB unarmed (melee) |
| Cover | R1 | — | Q |
| Aim | L2 (snap to target) | L1 hold (drive-by) | RMB |
| Fire | R2 | R1 while aiming | LMB |
| Weapon wheel | L1 hold (slow-mo); tap = last weapon | — | Tab hold |
| Crouch / stealth | L3 | — | C |
| Horn / siren | — | L3 (police: tap toggles siren) | H |
| Look behind / swap shoulder | R3 (aiming: swap shoulder) | R3 | V (behind) / X (shoulder) |
| Cinematic camera | — | ○ hold | Middle mouse hold |
| Special ability | L3 + R3 | L3 + R3 | Caps Lock |
| Radio | — | D-pad ←/→ step, hold ← wheel | , and . / hold , |
| Phone | D-pad ↑ | D-pad ↑ | ↑ (Up arrow) |
| Switch wheel | D-pad ↓ hold | D-pad ↓ hold | Alt hold |
| Interact / context | D-pad → | — | E |
| Pause menu | Options | Options | Esc |
| Map | Touchpad press (Create too, where the touchpad click isn't reported) | same | M |
| Menus | D-pad/stick move, ✕ accept, ○ back, L1/R1 tabs, L2/R2 zoom/page | same | arrows/WASD, Enter, Esc, Q/E |

Engine system keys stay for development (F1 tuning panel, F2 rooms, F3 boot log, F12 screenshot,
hold Backspace rewind) but are disabled in release game builds except F12; the engine's Esc menu
moves to F1 for hook games (§6.5 E13).

### 5.11 Art direction

- **Look: stylized realism, golden-hour Southern California.** Real-world scale and lighting
  (`view.style = lit`, ACES tonemap, bloom, sun shafts and haze in moderation, SSGI on Medium+),
  simplified geometry, rich grading. Think "architectural diorama at magic hour". Outlines off on the
  environment; optional thin outlines on characters and vehicles if captures show they help
  readability at distance.
- **Measured starting point** (planning captures; haze above ~0.3 washes everything out):
  `style=lit outlines=false tonemap=aces exposure=1.2 saturation=1.2 bloom=0.6 bloom_threshold=1.0
  haze=0.12 haze_height=10 shafts=1.2 gi=0.5 filter.temperature=0.3 filter.vignette=0.3
  light.sun_elevation=14 light.sun_intensity=1.25 light.ambient=0.75 light.shadow_radius=60
  fog_start=90 fog_end=220` — then the sky dome (E7) replaces the flat clear colour.
- **Palette:** stucco pastels (peach #f2c6a0, mint #a8d5ba, cream #f3e7cf, coral #f08a6b),
  terracotta roofs #c8643c, asphalt #2c2e33, concrete #9a9a96, palms #3f7d3a, ocean teal #1f6f8b,
  sunset gradient #ffb35c → #ff5e7e → #6a4c93, night neon magenta #ff3ea5 and cyan #3ef0ff.
- **Time-of-day presets** (`content/freeway/world.toml`): dawn, morning, noon (high sun, bleached,
  smog haze), golden hour (17:30–19:00), magic hour/dusk (pink-purple sky, lights on), night (sodium
  street lights, neon, lit windows), late night (sparse lights, blue grade). Each sets sun/moon
  direction and colour, sky zenith/horizon/glow colours, ambient sky/ground, fog colour and range,
  exposure, grading, haze, street-light level. Weather modifies them.
- **Buildings** by zone: downtown glass-and-concrete towers with setbacks and rooftop clutter
  (water tanks, antennas, AC units, helipads, billboards); Sunset Row two-storey stucco shops with
  awnings and big signs; Los Mirados bungalows with terracotta roofs, garages, palms, fences;
  industrial sheds with sawtooth roofs and loading doors; docks containers in brand colours.
  Facades use procedural materials (§6.6.4) whose windows light up at night with random occupancy.
- **Characters:** puppets with believable proportions (smaller heads than the engine default,
  longer legs), clothing palettes per archetype, accessories (caps, sunglasses, bags, helmets for
  police tactical units), walk styles from clips. Police uniforms dark navy with badges.
- **Vehicles:** extruded side-profile bodies with separate glass, bumpers, trims, wheels with hubs,
  lights; metallic paint materials; police livery; taxi roof sign; bus livery with the route number.
- **VFX:** muzzle flashes (light + particles), tracers, impact sparks and dust, glass shards, tyre
  smoke, skid marks (decals, faded over 20 s), sparks from rims and scraping, exhaust puffs, dust
  trails on dirt, water splashes, rain streaks and splashes, explosions (fireball, shockwave
  distortion, smoke column, debris), fire on burning cars, police light flares, searchlight cone,
  lightning, heat haze on the freeway at noon, money pickup sparkle.
- **Camera polish:** shake on explosions and collisions (setting to reduce), FOV kick on boosts and
  sprint, slow-mo on stunts and ability, letterbox in cutscenes.

### 5.12 Audio direction

- Everything synthesized (doctrine): no audio files.
- **SFX palette:** footsteps by surface, punches, gun families (layered noise burst + low thump +
  tail, distinct per weapon), shells (tiny tings), reload clicks, explosions (noise + sub sweep),
  glass, metal crunches scaled by impulse, tyre squeal (filtered noise driven by slip), engine loops
  (saw + square harmonics with RPM pitch, load-dependent filter, per class: compact buzzy, muscle
  low burble, sports high scream, bus diesel), horns (two-tone squares), sirens (wail/yelp
  sweeps), helicopter rotor (pulsed noise), UI clicks, phone ring and texts, pickup chimes, mission
  stingers (passed, failed, medal), ambience beds (city hum, crowd murmur near peds, surf at the beach,
  wind on the hills, harbor horns, rain and thunder).
- **Radio stations** (procedural songs from seeds: structure, key, tempo, progression, patterns,
  instruments; a new title and artist per song; a station ident jingle between songs):
  - **Freeway FM 104.04** — synthwave/outrun, 95–118 BPM, gated snares, arpeggios, saw leads. DJ:
    "Tarmac Tina" (text cards).
  - **Westside 99** — boom-bap and lo-fi hip-hop instrumentals, 82–94 BPM, swung drums, vinyl
    noise, mellow keys, sub bass.
  - **K-SURF 101.5** — surf and garage rock, 140–170 BPM, distorted square guitars with spring-
    style delay, driving drums.
  - **Radio Sol 92.3** — cumbia/latin pop, 90–105 BPM, güiro (noise scrapes), accordion-like
    squares, bouncy bass.
  - **Smooth 88.8** — smooth jazz, 70–90 BPM, 7th chords, soft "sax" (filtered saw with vibrato),
    brushed drums.
  - **SPNN 1040 AM** — news and talk: chimes, a babble voice, headline subtitles (satire: "Route 404
    widened to 47 lanes; commute unchanged"), traffic reports that reference the player's actual
    heat ("police pursuit reported near the Pretzel").
- **Mix:** radio at −6 dB under engine and SFX while driving, muffled (low-pass) outside a car with
  the door open; ducking −8 dB for dialogue, −12 dB in menus; distance attenuation and stereo pan;
  short reverb in tunnels, under the freeway and in interiors.

### 5.13 Tone and content guidelines
- Comedic crime caper; satire aimed at institutions and trends, never at protected groups.
- Stylized violence: no gore, no dismemberment, no torture; red puffs only. No sexual content, no
  slurs, no hate symbols, no drug use depicted (satire may mention "artisanal kale smoothies").
- Original names only: no real brands, people, songs or places.
- Police are part of the genre (pursuers); avoid references to real-world incidents.
- Target rating: comparable to a Teen / M-lite action game.

---

## 6. Technical architecture

### 6.1 Repository layout (after M0)

```
/                                  this repository
  README.md  PLAN.md  PROGRESS.md  CLAUDE.md      ours (never overwritten by the engine)
  AGENTS.md  START_HERE.md  Cargo.toml  Cargo.lock  rust-toolchain.toml  vercel.json   engine (vendored)
  crates/
    pav_core  pav_render  pav_view  pav_audio  pav_tools  pav_app       engine, extended (§6.5)
    pav_ui        NEW engine crate: 10-foot UI kit over egui; renders UiModel; headless capture
    pav_world     NEW engine crate: urban open-world kit (city generator, lane/sidewalk graphs,
                  signals, traffic, pedestrians, population/LOD, map raster, cover points)
    gtf_game      NEW game crate: Grand Theft Freeway-BX rules and content runtime (sim only)
    gtf_app       NEW binaries: grand_theft_freeway (the game) and gtf (agent CLI)
  content/freeway/                  game data, TOML, embedded at build time, hot-reloaded natively
    district.toml  dev_district.toml  interiors/*.toml (engine room files)  vehicles.toml
    weapons.toml  peds.toml  protagonists.toml  heat.toml  shops.toml  world.toml  radio.toml
    sounds.toml  text.toml  attract.toml  views.toml  missions/*.toml  activities.toml
  assets/props/lib/gtf_city/*.json  assets/props/lib/gtf_vehicles/*.json   format-2 props (PACK.json, CC0 own work)
  assets/materials/gtf/*.json       procedural materials (asphalt, concrete, stucco, glass facades...)
  assets/fonts/*.ttf + OFL licence files
  anim/  rooms/  game/  store/  tools/  web/   engine data (Shardfall's stays untouched)
  docs/                             engine docs + docs/ENGINE_README.md, ENGINE_CLAUDE.md, ENGINE_UPSTREAM.md
  docs/gtf/                         game docs: HANDOFF.md, ENGINE_ANALYSIS.md, notes
  scripts/                          engine scripts + vendor-engine.sh, build-gtf-web.sh, web-check.mjs,
                                    smoke-gtf.sh
  dist/GrandTheftFreeway-BX.html    the single-file web build (committed at M10 only)
```

### 6.2 Crate graph

```
pav_core  <-  pav_world  <-  gtf_game
   ^             ^              ^
   |             |              |
pav_render <- pav_view          |
   ^             ^              |
   |   pav_ui ---+ (egui)       |
   |      ^                     |
pav_audio |                     |
   ^      |                     |
pav_tools (lib, + bin pav) -----+---- gtf_app (bins: grand_theft_freeway, gtf)
pav_app   (lib, + bin shardfall) ----/
```
- `gtf_game` depends on `pav_core` and `pav_world` only: **no graphics, window or audio**. Everything
  the player sees and hears is described as data (§6.5 E4) and drawn by engine crates.
- Engine crates never depend on `gtf_*`. Only `gtf_app` links the game in (so a game change rebuilds
  `gtf_game` + two tiny binaries, not `pav_tools`/`pav_app`: minutes saved per iteration).
- Shardfall (`pav_core::arpg`) and the pavilion demo keep working; `cargo test --workspace` covers
  them.

### 6.3 Runtime data flow

```
host (pav_app / pav_tools)                       sim (pav_core::Sim, fixed 60 Hz)
  devices -> InputFrame v2 (pad, mouse, activity, cmds) --> Sim::step
                                                   |  world_paused? -> only game pre/post (menus)
                                                   |  behaviours, vehicle interact (legacy), game pre,
                                                   |  characters (game inputs replace own),
                                                   |  game vehicles() -> VehicleInput, drive_vehicles,
                                                   |  physics (dt x time_scale), projectiles, game post,
                                                   |  zones, streaming (interest = player + camera)
  RenderFrame <------------------------------------ Sim::frame (+ Game::present -> PlayFrame)
     |-> pav_view: CameraIntent, ViewIntent, DrawList, entities, statics (culled batches) -> Scene
     |-> pav_render: sky, shadows, scene, post -> swapchain / capture
     |-> pav_ui: UiModel (+ map raster, glyph style, perf readout) -> egui -> overlay
     |-> pav_audio: SimEvents (one-shots via bank), AudioScene (loops, music station, mix)
     |-> AppRequests: save/settings persistence, vibration, quit
```

### 6.4 Determinism, state and snapshots
- All gameplay state lives in the `Freeway` game struct (inside `SimState::play`) or in entities.
  `Clone + Serialize + Deserialize`. Caches that follow from content + seed (the generated city, lane
  and sidewalk graphs, map raster, nav grids) are `#[serde(skip)]` behind `Arc` and rebuilt when
  missing (after a snapshot load), as the starter does with its nav grid.
- Randomness: `sim.state.rng` only (or a hash of the seed for content generation). Never the clock.
  No `HashMap`/`HashSet` iteration that reaches state: `BTreeMap`, sorted `Vec`s, index arenas.
- The camera is sim-side (game state), so captures, bots, attract and tests see exactly what a player
  sees; the view only interpolates between ticks and adds cosmetic shake.
- Host-only, non-deterministic by design: audio synthesis (incl. music rendering), egui layout and
  animation, particles, decal fading, dynamic resolution. Nothing there feeds back into the sim
  except through recorded `InputFrame`s.
- Rewind is a development tool: set `history_seconds` to 10 for the game (snapshots of a city with
  thousands of colliders are large); measure `SimState::approx_bytes` in M2 and adjust.
- Content data (TOML) is part of the build; a replay is valid for the same build and content.

### 6.5 Engine extensions

Each extension lands in the engine crates as generic, documented API (`docs/MAKE_A_GAME.md`,
`docs/REFERENCE.md`), in separate `engine:` commits, with tests. Sketches below are targets, not
exact signatures; follow the code's existing idioms.

**E1 — Game crates outside `pav_core`; library apps**
```rust
// pav_core/src/games/mod.rs
/// Adds a game defined in another crate. Binaries call it before building a scene or loading a
/// snapshot; tests call it in setup. Idempotent by key.
pub fn register(entry: &'static Entry);
/// Built-in games (`GAMES`), then registered ones.
pub fn all() -> Vec<&'static Entry>;
pub fn find(key: &str) -> Option<&'static Entry>;   // searches both
```
- Replace every `GAMES.iter()` (scenes.rs, play_tools.rs, tools.rs) with `games::all()`.
- `pav_tools`: move the CLI's `main` into `pav_tools::cli::main()`; `src/bin/pav.rs` calls it.
- `pav_app`: split into `lib.rs` (`pub fn run(settings, AppConfig)`) and `main.rs` (Shardfall's
  defaults). `AppConfig { title, default_scene, settings_file, save_prefix, web_storage_prefix,
  dev_keys: bool, touch_mode: TouchMode::{Controls, AttractOnly}, branding }`.
- `crates/gtf_app`: `src/bin/grand_theft_freeway.rs` (`gtf_game::register(); pav_app::run(..)`)
  and `src/bin/gtf.rs` (`gtf_game::register(); pav_tools::cli::main()`).
- Scripts: `build-web.sh` gains `--package`, `--bin` (output name follows the bin);
  `scripts/build-gtf-web.sh` wraps it for the game; `build-windows.sh` builds `gtf_app` too; the
  session-start hook builds `gtf` as well as `pav`.

**E2 — `InputFrame` v2**
```rust
pub struct InputFrame {
    /* existing fields kept: move_dir, vertical, aim, held, pressed, cmd */
    #[serde(default, skip_serializing_if = "PadState::is_idle")]
    pub pad: PadState,          // every device maps onto controller semantics
    #[serde(default)]
    pub mouse_delta: Vec2,      // pixels since last tick (x right, y down)
    #[serde(default)]
    pub activity: bool,         // any device activity this tick (keys, mouse move, pad, touch)
    #[serde(default)]
    pub device: DeviceInfo,     // last device: keyboard+mouse | gamepad(family) | touch
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cmds: Vec<String>,      // game commands (menus from tools, platform facts, loads); replayed
}
pub struct PadState { pub held: u32, pub pressed: u32, pub stick_l: Vec2, pub stick_r: Vec2,
                      pub l2: f32, pub r2: f32 }
pub mod pad { SOUTH, EAST, WEST, NORTH, L1, R1, L2, R2, L3, R3, UP, DOWN, LEFT, RIGHT, START,
              SELECT, TOUCHPAD }   // bit constants; names: cross/a, circle/b, square/x, triangle/y
```
- `InputFrame` stops being `Copy` (it gains a `Vec`): fix the call sites the compiler lists
  (`.clone()`), keep serde defaults so old replays still load.
- `pav_app` fills it from keyboard/mouse (the control table §5.10), gilrs (analog L2/R2 from
  `button_data(..).value()`, sticks, the pad family from the device name: "DualSense", "Wireless
  Controller", "PS5", "PS4" → PlayStation; "Xbox", "XInput" → Xbox; else generic) and touch.
- `pav_tools` `input` gains `pad=`/`pad_press=` (names), `stick_l=`, `stick_r=`, `l2=`, `r2=`,
  `mouse=`, `cmd=`, `activity=`; `pad_script` style multi-step input stays in the app.

**E3 — Game hook v2**
```rust
trait Game {
    /* existing: pre, post, hud, status, outcome, bot */
    fn time_scale(&self) -> f32 { 1.0 }          // physics/characters/behaviours dt multiplier
    fn world_paused(&self) -> bool { false }     // title, pause, loading: only pre/post run
    fn vehicles(&mut self, sim: &mut Sim, dt: f32) -> BTreeMap<EntityId, VehicleInput> { .. }
    fn present(&self, sim: &Sim) -> Option<PlayFrame> { None }   // §E4; called in Sim::frame
    fn history_seconds(&self) -> Option<f32> { None }            // rewind window override
    fn legacy_controls(&self) -> bool { true }   // false: no engine-demo player actions
}
```
- `Sim::step_inner`: honour `world_paused` (skip behaviours, characters, vehicles, physics,
  projectiles, zones, streaming; still run game `pre`/`post` and count ticks) and multiply dt by
  `time_scale` (as Shardfall's hit-stop does); call `vehicles()` just before `drive_vehicles`.
- `legacy_controls() == false` turns off the engine demo's own player actions that read the raw
  host input — instant vehicle enter/exit (`vehicle_interact` on INTERACT), bomb throws (USE),
  crawl toggles — so the game's own mapping is the only one; `pav_app` also stops setting the
  legacy button bits for such games (it still fills `pad`).
- `RenderFrame.present: Option<Arc<PlayFrame>>`; the `play` tool reports a summary of it.

**E4 — Presentation as data (`pav_core::present`)**
```rust
pub struct PlayFrame {
    pub ui: UiModel,                  // HUD, screens, wheels, phone, banners, toasts, subtitles, prompts
    pub draw: DrawList,               // assets, prims, SDFs, lights, spots, decals, ribbons, tracers, 3D text
    pub camera: Option<CameraIntent>, // eye, target, up, fov, cut, shake
    pub view: Option<ViewIntent>,     // sun, moon, sky, ambient, fog, exposure, grading, haze, wetness,
                                      // rain, wind, street-light and window levels, shadow radius
    pub audio: AudioScene,            // listener, loop sources (id, cue, pos, gain, pitch, param),
                                      // music (station, song, offset, muffle), ducking, reverb
    pub map: Option<Arc<MapImage>>,   // minimap / pause-map raster (same Arc every tick, versioned)
    pub app: AppRequests,             // save (serial, slot, json), settings (serial, json), quit, vibrate
}
```
- `UiModel` is plain data with enums for every widget in §5.9: `Hud { minimap, vitals, money,
  wanted, weapon, objective, timer, vehicle, crosshair, meters, opacity }`, `Screen { kind, title,
  tabs, tab, hints, body: Menu | Cards | Map | Text | Title | Loading | Results }`, `Wheel`, `Phone`,
  `Banner`, `Toast`, `Subtitle`, `Prompt { button: PadButton, label, hold_progress }`, `Help`,
  `ScreenMarker { world_pos, icon, label }` (off-screen arrows), `Overlay { fade, letterbox,
  desaturate, tint, vignette }`, `perf: bool`, `glyphs: GlyphStyle`, `scale`, `safe_margin`.
  Text uses light markup: `~y~ ~b~ ~r~ ~g~ ~s~` colours and `~btn:south~` glyph tokens.
- Generic events: `SimEvent::Sound { cue, pos, gain, pitch }`, `SimEvent::Fx { preset, pos, dir,
  scale }`, `SimEvent::Crash { vehicle, other, impulse, pos }`.
- Everything is serde: `gtf ui` dumps it as JSON so tests and agents read the HUD without pixels.

**E5 — `pav_ui`: the 10-foot UI kit**
- `pav_ui::draw(ctx: &egui::Context, model: &UiModel, map: Option<&MapTexture>, camera:
  &CameraData, extra: &HostOverlay /* fps, now-playing card, touch gear */, theme: &Theme)`.
- Widgets for every `UiModel` element; procedural glyphs (PlayStation, Xbox, keyboard); fonts
  embedded from `assets/fonts`; theme tokens (§5.9 colours and sizes, scaled to the window height);
  animation by egui ids (in/out, focus pulse); minimap as a rotated textured mesh with clipped blips,
  route polyline, cones and search circle.
- **Headless:** `pav_ui::render_offscreen(device, queue, target, size, model, ..)` with egui-wgpu,
  used by `pav_tools` for `capture hud=true` and `filmstrip hud=true`. This closes the engine's open
  issue "the HUD is invisible to the tools" for every hook game.
- `pav_app` keeps its Shardfall UI; hook games that publish a `PlayFrame` get `pav_ui`.

**E6 — `pav_view`**
- Use `CameraIntent` when present (interpolate eye/target/fov between frames unless `cut`); the
  `CameraRig` path stays for everything else.
- Apply `ViewIntent` over `ViewSettings` (quality trims still apply); feed the sky (E7).
- Draw `DrawList`: assets through `emit_asset`, prims through `emit_shape`, lights, fake spots
  (E7), decals (flat quads offset along the normal), ribbons (strip meshes, e.g. the GPS route on the
  road), tracers, 3D text with a font id.
- Vehicles: body from the def's prop asset (`body_asset`), wheels from a wheel asset with spin and
  steer, doors as named parts rotated by the door state, crumple offsets per zone, light states, and
  **seated occupants** (puppet with a sitting clip, upper body) for every rider.
- Emit `StaticBatch`es per region for E7 culling instead of pushing every static instance into
  `scene.meshes` each frame; per-object sphere-vs-frustum tests for dynamic objects.

**E7 — `pav_render`**
- **Sky pass:** full-screen triangle at the far plane: zenith/horizon/ground gradient, sun disc and
  glow, moon, stars, procedural clouds (coverage, speed, colour), lightning flash; fog blends into the
  horizon colour. Replaces the flat clear colour when the scene has a sky.
- **Culling and persistent buffers:** GPU instance buffers per static batch keyed by (region,
  version), uploaded only when the version changes; frustum culling by batch AABB (and the shadow
  frustum for the shadow pass); distance classes (`flags::SMALL` props culled beyond ~90 m, decor
  beyond ~150 m, buildings to the fog end).
- **Shadows:** centre the sun shadow region ahead of the camera along the view direction; radius
  60–80 m; a second far cascade only if captures show it is needed.
- **Fake spotlights** (`Scene.spots`): an additive soft cone mesh plus a point light where the beam
  lands — searchlights, headlight beams.
- **World text fonts:** `Text3d.font` picks one of several SDF atlases (default + game fonts).
- **Window material kernel:** a facade layer that draws a window grid whose cells light up by a
  per-cell hash against a night level (globals), in `shaders/materials.wgsl` **and** the CPU
  reference `pav_core::material::eval` (keep `material_parity.rs` green; it runs on lavapipe).
- Run `scripts/check-wgsl.sh` after every WGSL change.

**E8 — `pav_audio`**
- **Sound bank as data:** cues → layered patches with random pitch/volume, priority, voice limits
  (`content/freeway/sounds.toml` over an engine default bank); `SimEvent::Sound` plays cues; the
  closed `sounds_for` match stays for the engine's own events.
- **Loops with live parameters:** `Mixer::set_loops(&[LoopSource])` reconciles by id (fade in/out,
  smooth gain/pitch/filter/param). Loop types: engine (stacked saw/square harmonics, RPM pitch,
  load-dependent filter, class presets), tyre squeal (filtered noise by slip), siren (wail/yelp
  sweeps), rotor (amplitude-modulated noise), ambience beds (city, crowd, surf, wind, harbor, rain).
- **FX bus:** stereo delay, a small reverb (Freeverb-style), low-pass for muffling, a master
  limiter.
- **Music and radio:** `pav_core::radio` (pure, shared by sim and host) turns a station definition
  and a song index into `SongMeta { seed, title, artist, bpm, key, bars, seconds }`; the sim keeps a
  station clock from game time (stations are "live": switching resumes mid-song, like the source);
  `pav_audio::music` renders a `SongMeta` with a sample-accurate sequencer (sections, chords,
  drums, bass, lead, pads, genre instrument patches) starting at the given offset.
- **Babble voice:** formant synthesis (vowel formants, syllable rhythm, per-speaker pitch contour)
  for dialogue, DJs, police scanner and pedestrian barks; duration follows the subtitle length.

**E9 — Vehicles v2 (`pav_core::vehicle`)**
- `VehicleDef` adds: class, drive (fwd/rwd/awd), brake force, speed-sensitive steering, downforce,
  centre of mass, wheel geometry, health, `body_asset`, `wheel_asset`, doors (hinges, sides), seats,
  light and siren positions, radio flag.
- `VehicleInput { throttle, brake, steer, handbrake, horn, siren }` for any vehicle: the game's
  `vehicles()` map first, then the legacy player path (steer-toward-stick, for the demo rooms), else
  idle. NPC drivers become possible; the player's car uses R2/L2/stick through the game.
- State: health, 4 crumple zones, burst tyres, broken lights, fire, engine on/off, siren, door open
  amounts, occupants by seat. Collision impulses → `SimEvent::Crash`.
- `Sim::set_vehicle_kinematic(id, on)` switches the body between dynamic (raycast controller) and
  kinematic (positions set by the traffic LOD).
- Helicopter: `VehicleInput` maps to move/climb/yaw; AI helicopters use the same input.

**E10 — `pav_world`** (design in §6.6).

**E11 — `pav_core` odds and ends**
- Per-character movement overrides (`Character.speed_scale`, jump/climb flags) so sprint, stamina
  and abilities need no global parameter changes.
- Public clip control for game code (`Sim::play_clip(id, clip, flags, speed)`, upper-body aim and
  cover poses, sitting, hands-up, knock-down and get-up).
- Query helpers: extend the existing `Sim::raycast(origin, dir, max, exclude)` (`world.rs`; it
  always skips the player and returns point, normal, entity) with a `RayFilter` (include the player,
  exclude a list, statics only, characters only) and return the distance and the static block hit;
  add `Sim::sphere_cast(..)` and `Sim::overlap_sphere(..)` the same way.
- Public streaming interest points for games (`Sim::set_interest(&[Vec3])`: player, camera target,
  mission anchors).
- Font-agnostic `Text3d` in the frame.

**E12 — `pav_tools` (generic, for any hook game)**
- `capture hud=true`, `filmstrip hud=true` (pav_ui offscreen).
- `ui` (the current `UiModel` as compact JSON; `ui focus=true` for the focused element only).
- `play cmd="..."` pushes game commands; `play` summarises `present` (screen, objective, HUD).
- `input` pad semantics (E2).
- `perf view=<name>|all` (the game names its reference views; reports the §6.9 budgets pass/fail),
  `soak seconds= mode=attract|roam|traffic|peds` (runs and reports anomalies the game counts),
  `citymap` (pav_world raster PNG with zones, roads, lanes, landmarks, spawn and cover points),
  `traffic`, `peds`, `lanes at=[x,z]`, `vehicle_feel key=<archetype>` (on the dev district's test
  pad: 0–100, top speed, braking, skidpad g, handbrake slide angle).
- Tool rules from `AGENTS.md` ("Adding a tool"): help ≤ 50 tokens, args ≤ 12 words, reject unknown
  args with the nearest name, test on `Session::new("empty", 1)`.

**E13 — `pav_app` (library + hook-game shell)**
- `AppConfig` (E1), window title and branding, settings and save files named for the game.
- When the frame has a `PlayFrame`: camera from the intent, `pav_ui` overlay, audio scene, view
  intent; Esc/Options/menus go to the game as pad buttons; the engine's pause menu and tuning panel
  move to F1 (dev builds) and the F-key system layer is limited to F12 in release game builds.
- Activity detection for any device event (mouse moves included); device family for glyphs.
- Touch devices with `TouchMode::AttractOnly`: no touch controls; a gear button (pav_ui) and its
  overlay; send `platform touch` once at start.
- Persistence: `AppRequests.save/settings` → files next to the executable (native) or
  `localStorage` (web); on start, saves and settings go to the game as commands in the first frame.
- FPS readout (frame-time EMA) handed to `pav_ui`; vibration through gilrs force feedback (native,
  best effort).

### 6.6 `pav_world`: the urban open-world kit

#### 6.6.1 Macro map
- A district file (§6.8.1) holds a character grid (16 m cells, 64 × 64 for Paloma Bay), a legend
  (zone or road class per character), zone rules, landmarks, spline roads, freeway alignments,
  terrain, water and interiors.
- Road classes and cross-sections: alley (1 cell, two 3 m lanes, no sidewalks), street (1 cell:
  2 lanes, 2 parking lanes, sidewalks), avenue (2 cells: 4 lanes + turn lane + parking + sidewalks),
  boulevard (2 cells: Sunset Row, medians with palms), freeway (separate elevated alignments with
  ramps), hill road (spline on terrain), beach path, pier deck, channel (The Ditch: sunken with
  sloped walls and ramps).
- Parsing: road cells form the network; intersections where road cells meet (T, cross, bends);
  blocks are flood-filled non-road regions; the zone comes from the block's characters.

#### 6.6.2 Lane graph and routing
- Lanes: polylines with direction, width, speed limit, road id, lane index; intersection connectors
  (straight, left, right) as sampled quadratic curves; lane-change neighbours; freeway lanes and ramp
  splines join ground lanes at ramp ends; parking lanes and lots hold parking spots.
- Stored as index arenas (`Vec`s); A* over lanes for routing (GPS ribbon, police dispatch, AI
  destinations); nearest-lane queries through a uniform spatial grid.

#### 6.6.3 Signals
- Each signalized intersection runs phases (N–S green, all-red, E–W green, all-red; protected lefts
  on avenues) with timings by road class and an offset (a green wave along Sunset Row); pedestrian
  walk phases with countdown. State is a pure function of sim time and the intersection, so it
  costs no snapshot space. Traffic-light props show it (emissive lamp parts via DrawList overrides).

#### 6.6.4 Blocks, lots and buildings
- Blocks split into lots by zone rules (lot widths, setbacks); buildings by zone style: podium +
  tower with setbacks (downtown), stucco shops with awnings and sign bands (Sunset Row), bungalows
  with hip roofs from wedges, garages and fences (Los Mirados), sheds with sawtooth roofs
  (industrial), warehouses, containers and cranes (docks).
- Geometry: a few static boxes per building mass (one collider each), roofs and trims as decor or
  baked props; facades through procedural materials (E7 window kernel): glass curtain wall,
  punched-window stucco, brick, concrete panels, corrugated metal, storefront.
- Landmarks replace generated lots with authored assemblies; interiors are engine room files
  (`content/freeway/interiors/*.toml`) placed at the landmark's door.

#### 6.6.5 Street dressing
- Curbs (0.15 m), crosswalk stripes, lane markings (ghost blocks merged per chunk), street lamps,
  traffic lights, stop and speed signs, street-name signs (3D text), hydrants, bins, benches, bus
  stops, newspaper boxes, palms and trees, parking meters, power poles with sagging wires, billboards
  with satirical ads (3D text + panels), fences, walls, painted parking stalls. All from
  `LIB/gtf_city` props, placed by deterministic rules per zone, baked into chunk statics.

#### 6.6.6 Cover points
- Generated along building walls, low walls, planters, containers and big props facing open space
  (1.5 m spacing, high/low cover flag). Stopped vehicles add dynamic cover points at runtime.

#### 6.6.7 Sidewalk graph
- Offset loops around every block, crosswalk edges across intersections (gated by the signal's walk
  phase), links to building doors, bus stops and scenario points; beach paths and the pier deck.

#### 6.6.8 Population manager
- Target counts per zone and time (data); spawn rules: out of view, distance bands (§5.6.6, §5.6.7),
  not near the player, valid lane/sidewalk point; despawn when far and unseen; LOD tiers with
  hysteresis; hard caps.

#### 6.6.9 Traffic agents
- State per car: route (lane sequence, random turns or a destination), IDM parameters, lane-change
  state, personality, LOD tier. Each tick: find the leader on the lane and through connectors
  (per-lane sorted occupancy), stop for red/amber lines, reserve intersection boxes to avoid
  conflicts, yield to peds in crosswalks, then output `VehicleInput` (dynamic tier) or a position
  along the lane (rail tier).
- Hooks the game calls: `panic(pos, radius)`, `pull_over(siren_pos, dir)`, `block(at)`,
  `release(car)`, `drive_to(car, target)`, `follow(car, target)`.

#### 6.6.10 Pedestrian agents
- Path on the sidewalk graph, behaviour state machine (walk, wait for walk signal, scenario,
  flee, cower, react, board bus), output `InputFrame` (full tier) or a direct position (low tier).
- Hooks: `scare(pos, radius, kind)`, `witness_query(pos, radius)`, `send_to(ped, target)`.

#### 6.6.11 Map raster
- `MapImage` (1 px per metre, 1024 × 1024 for Paloma Bay; land, roads, sidewalks, buildings, parks,
  water, freeway outline, landmark icons) generated deterministically from the district; versioned;
  shared by the minimap and the pause map through `PlayFrame.map`.

#### 6.6.12 Queries and storage
- `zone_at`, `height_at` (terrain), `is_road`, `nearest_lane`, `route(from, to)`,
  `random_sidewalk_point(zone)`, `landmark(key)`, `spawn_points(kind)`, `interior_at`,
  `cover_near(pos, threat)`.
- The generated city is immutable per content + seed: `Arc<City>`, `#[serde(skip)]`, rebuilt on
  load. Dynamic state (agents, parked-car occupancy, damaged props) lives in game state keyed by
  entity id in `BTreeMap`s.
- Chunks become engine `StaticWorld` regions (`RegionKey::Chunk`) so streaming, dormancy and the
  view's region cache work unchanged.

### 6.7 `gtf_game` module map

| Module | Responsibility |
|---|---|
| `lib.rs` | `Freeway` (the `Game` impl), `register()`, scene build for `freeway`, `freeway/roam`, `freeway/dev`, `freeway/attract`, `freeway/mission/<key>`, `freeway/view/<name>` |
| `content/` | Load and validate every TOML (embedded with `include_str!`/build script; read from disk when running from the repo for hot reload); `validate` command lists errors with file:line |
| `frontend/` | State machine: boot → title → menus → loading → playing ⇄ paused; attract; about; settings screens; focus models; builds `Screen`s |
| `attract/` | Idle timer, playlist, vignette setup, bot policies, camera shot director, overlays, anomaly counters |
| `world/` | District instance (`pav_world::City`), time of day, weather, `ViewIntent`, streaming interest, interiors |
| `player/` | On-foot mapping from the pad to the character, stamina, vault/climb, cover, aim, melee, interaction prompts |
| `camera/` | Orbit, aim, chase, cinematic, sky-cam, scripted shots, collision; builds `CameraIntent` |
| `vehicles/` | Archetypes, spawning, enter/exit/carjack, damage, explosions, radio state, police lights/siren, helicopters |
| `combat/` | Weapons, firing (hitscan), aim assist, damage, vitals, pickups, explosions, enemy combat AI |
| `heat/` | Crimes, witnesses, wanted level, search zone, dispatch, police AI (cars, foot, air), roadblocks, busted/wasted |
| `npc/` | Archetypes, looks, reactions, mission actors, gangs, guards |
| `missions/` | DSL types, validator, runtime, triggers, checkpoints, medals, rewards, replay, mission bots |
| `cutscene/` | Shot sequences, actor actions, letterbox, skip |
| `protagonists/` | Profiles, switching (sky-cam), abilities, idle "life spots" |
| `phone/` | Apps: contacts, messages, camera, map, jobs, radio, settings, gallery, LifeHakr |
| `economy/` | Cash, shops, prices, payouts, fees |
| `activities/` | Race, stunt jumps, hold-ups, vigilante, random events, collectibles |
| `progress/` | Save data, completion, stats growth, settings model |
| `hud/` | Builds `UiModel` each tick (HUD, prompts, toasts, subtitles, markers, map data) |
| `audio/` | Builds `AudioScene` (engine loops, sirens, ambience, radio station clock), sound events |
| `bot/` | Player bots: drive-to, follow, evade heat, fight, walk-to, interact; used by missions, attract, tests |
| `dev/` | Dev district support, debug overlays (lanes, nav, heat zone, cover), dev-only cheats |

### 6.8 Data formats

All game data is TOML under `content/freeway/`, embedded at build time and re-read from disk at
each `load` when running from the repository (the engine's `level_text` pattern), validated with
`deny_unknown_fields` and clear errors.

#### 6.8.1 District (`district.toml`)
```toml
name = "Paloma Bay"
cell = 16.0                     # metres per map character
origin = [-512.0, 0.0, -512.0]  # world position of the map's top-left corner (x east, z south)
seed = 404

[legend]                        # one entry per character used in `map`
"." = { zone = "residential" }
"c" = { zone = "commercial" }
"D" = { zone = "downtown" }
"t" = { zone = "industrial" }
"h" = { zone = "docks" }
"p" = { zone = "park" }
"s" = { zone = "beach" }
"w" = { zone = "water" }
"H" = { zone = "hills" }
"-" = { road = "street" }
"=" = { road = "avenue" }       # roads two cells wide are drawn two characters wide
"B" = { road = "boulevard" }
"a" = { road = "alley" }
"C" = { road = "channel" }
# Landmarks own the cells of their [[landmark]] rectangle, whatever characters the map shows there.

[zones.downtown]
lot = [24, 48]                  # lot width range (m)
height = [36, 140]
styles = ["tower_glass", "tower_concrete"]
props = { lamp = 0.8, tree = 0.15, bench = 0.3, bin = 0.5, billboard = 0.1 }
traffic = { day = 1.0, night = 0.45 }
peds = { day = 1.0, night = 0.35, archetypes = { business = 3, local = 3, tourist = 1, influencer = 1 } }

[[landmark]]
key = "bank"
name = "Paloma Savings & Trust"
cells = [[22, 24], [27, 28]]    # map cell rectangle (inclusive)
assembly = "tower_bank"         # authored assembly in pav_world/gtf assemblies
interior = "interiors/bank_lobby.toml"   # optional engine room file
blip = "bank"

[[freeway]]
key = "route_404"
height = 9.0
lanes = 3                       # per direction
points = [[30.5, -1], [30.5, 56], [29, 60], [18, 60]]   # cell coordinates; curves are smoothed
ramps = [{ at = [30.5, 9], to = "sunset_row_east", side = "west" }]

[[spline_road]]
key = "mirador_drive"
class = "hill"
points = [[6, 3, 4.0], [5, 2, 12.0], [8, 1, 24.0], [8, 0, 40.0]]   # cell x, cell z, height (m)

[terrain]
hills = [{ cells = [[0, 0], [63, 6]], height = 45.0, falloff = 3 }]

map = '''
(64 lines of 64 characters)
'''
```

#### 6.8.2 Vehicles (`vehicles.toml`)
```toml
[okami_kaze]
name = "Okami Kaze"
class = "sports"
body = "LIB/gtf_vehicles/okami_kaze"     # format-2 prop: the shell, glass, trims, light parts
wheel = "LIB/gtf_vehicles/wheel_sport"
size = [0.95, 0.55, 2.2]                 # collider half extents (m)
mass = 1250
drive = "rwd"
power = 5200                             # engine force per driven wheel (N)
max_speed = 42.0
brake = 9000
grip = 3.4
drift_grip = 0.9
steer = 34.0                             # lock (deg) at low speed
steer_at_top = 0.35                      # fraction of lock at top speed
wheels = { radius = 0.34, track = 0.82, front_z = 1.35, rear_z = -1.30 }
doors = [{ part = "door_l", hinge = [-0.9, 0.2, 0.75], side = "left" }, { part = "door_r", hinge = [0.9, 0.2, 0.75], side = "right" }]
seats = [[-0.35, 0.1, 0.0], [0.35, 0.1, 0.0]]
lights = { head = [[-0.62, 0.05, 2.15], [0.62, 0.05, 2.15]], tail = [[-0.65, 0.12, -2.18], [0.65, 0.12, -2.18]] }
health = 900
colors = ["#c81e2b", "#f2f2f2", "#1b1d22", "#2a6fdb", "#f2b705"]
traffic_weight = 0.05                    # share of ambient traffic
radio = true
sound = "engine.sports"
```

#### 6.8.3 Weapons (`weapons.toml`)
```toml
[p9]
name = "P9"
slot = "pistol"
damage = 26
headshot = 3.0
rpm = 300
magazine = 12
reserve_max = 240
range = 40.0
spread = [0.6, 4.0]          # degrees: min (aimed, still) / max (sustained, moving)
recoil = 1.2                 # camera pitch kick (deg)
sound = "gun.pistol"
fx = "muzzle.small"
model = "LIB/gtf_city/p9"    # held prop
clips = { idle = "MESH2MOTION/Pistol_Idle", shoot = "QUATERNIUS/Pistol_Shoot", reload = "QUATERNIUS/Pistol_Reload" }
price = 600
ammo_price = 40              # per magazine
```

#### 6.8.4 Pedestrians (`peds.toml`)
```toml
[tourist]
weight = 1.0
proportions = { scale = [0.95, 1.05], leg_length = [0.95, 1.05] }
skin = ["#8d5524", "#c68642", "#e0ac69", "#f1c27d", "#ffdbac"]
shirt = ["#ff6f61", "#ffd166", "#06d6a0", "#118ab2", "#ffffff"]
pants = ["#e9d8a6", "#264653", "#f4f1de"]
accessories = { cap = 0.4, sunglasses = 0.6, backpack = 0.3 }
walk = ["STYLE100/Neutral_Walk", "STYLE100/OnPhoneRight_Walk"]   # names from `pav clips find=walk`
behaviours = { walk = 4, photo = 2, sit = 1, chat = 1 }
reaction = { flee = 0.8, cower = 0.15, fight = 0.0, film = 0.05 }
bravery = 0.2
```

#### 6.8.5 Other data files
- `protagonists.toml` — looks, stats, starting cash/weapons, ability tuning, safehouse, life spots,
  signature vehicle, babble voice profile.
- `heat.toml` — crime values, witness timings, level table (§5.6.5), dispatch rules, search radii
  and timers, busted/wasted fees.
- `shops.toml` — gun store stock and prices, mod shop services, store snacks.
- `world.toml` — time-of-day presets, weather presets and transition weights, day length.
- `radio.toml` — stations: genre template (tempo range, scales, progressions, drum patterns,
  instrument patches, song structure), ident jingle, DJ lines, ads, news headlines, word lists for
  titles and artists.
- `sounds.toml` — sound cues (one-shots and loops) as layered synth patches.
- `text.toml` — every user-facing string (menus, tips, help, toasts), keyed, for later localization.
- `activities.toml` — race checkpoints and racers, stunt jump gates, hold-up stores, vigilante
  levels, random events, collectible positions and trivia.
- `views.toml` — reference views for `perf` and look checks (§7.2).
- `attract.toml` — the vignette playlist (§5.8).

#### 6.8.6 Missions (`missions/<key>.toml`)
```toml
key = "m1_repo_man"
title = "Repo Man"
protagonist = "dez"
blip = { letter = "D", at = "landmark:paloma_customs" }
unlock = []                                  # mission keys that must be passed first
time = { at = "17:30", lock = false }
reward = { cash = 1500 }
medals = [
  { id = "fast", text = "Deliver the car within 4:00", time_under = 240 },
  { id = "clean", text = "Deliver with less than 20% damage", vehicle_health_over = { actor = "repo_car", value = 0.8 } },
]

[[spawn]]                                    # actors and vehicles, created at mission start or a step
id = "repo_car"
vehicle = "bravura_stallion"
at = "spot:los_mirados_driveway_3"
color = "#2b5fd9"
locked = false

[[spawn]]
id = "owner"
ped = "local"
look = { shirt = "#ffffff", cap = true }
at = "spot:los_mirados_porch_3"
weapons = ["bat"]
behaviour = "idle"

[[step]]
kind = "cutscene"
shots = [
  { anchor = "actor:dez", offset = [1.8, 1.6, 2.4], look = "actor:dez", fov = 40, seconds = 3.0 },
  { anchor = "landmark:paloma_customs", offset = [-12, 6, 10], look = "landmark:paloma_customs", seconds = 2.5 },
]
lines = [
  { who = "mack", text = "Kid. Blue Bravura, Mirador Drive. The owner stopped paying in May." },
  { who = "dez", text = "And he's just going to hand me the keys?" },
]

[[step]]
kind = "goto"
target = "actor:repo_car"
radius = 6.0
objective = "Go to the ~b~Bravura~s~ on Mirador Drive."

[[step]]
kind = "enter_vehicle"
target = "actor:repo_car"
objective = "Get in the ~b~Bravura~s~."
on_start = [{ set_behaviour = { actor = "owner", behaviour = "chase_player", vehicle = "marlin_bay" } }]
checkpoint = true

[[step]]
kind = "escape"
pursuers = ["owner"]
distance = 180.0
objective = "Lose the ~r~owner~s~."

[[step]]
kind = "drive_to"
target = "landmark:paloma_customs:bay"
radius = 4.0
objective = "Deliver the car to ~y~Paloma Customs~s~."
checkpoint = true

[[fail]]
when = { destroyed = "repo_car" }
reason = "The car was wrecked."

[[fail]]
when = { abandoned = { actor = "repo_car", distance = 120.0 } }
reason = "You abandoned the car."
```
- Step kinds: `cutscene`, `dialogue`, `goto`, `enter_vehicle`, `drive_to`, `follow`, `chase`
  (catch/destroy), `escape` (lose pursuers), `lose_heat`, `kill` (group), `survive` (time/waves),
  `defend`, `collect`, `photograph`, `interact` (hold), `wait`, `switch` (to a protagonist),
  `race` (checkpoints), `stealth` (stay unseen), `deliver`, `holdup`, `choose` (board),
  `branch` (on a flag or choice), `set` (flags, heat floor/cap, time, weather, traffic, dispatch),
  `parallel` (sub-steps for different protagonists in the heist).
- Fail conditions: `destroyed`, `died`, `abandoned`, `spotted`, `timer`, `heat_over`,
  `left_area`, `custom flag`. Every step kind has a **bot policy** so `gtf play bot=true scene=
  freeway/mission/<key> seeds=3` can finish it (D13).
- Anchors: `actor:<id>`, `landmark:<key>[:<point>]`, `spot:<name>` (named points in the district
  file or interiors), `lane_near:<x,z>`, `player`.

### 6.9 Performance budgets and strategy

Measured with `gtf perf view=all` (render statistics from `capture stats=only` at each reference view
with the full population, plus a 600-tick sim bench) and in the browser build:

| Metric | Budget (reference views; `hills_vista` may use the second figure) |
|---|---|
| Draw calls | ≤ 2,000 (≤ 2,500) |
| Mesh instances | ≤ 40,000 (≤ 55,000) |
| Triangles | ≤ 2.5 M (≤ 3.5 M) |
| SDF primitives | ≤ 3,000 |
| Point lights / shadowed | ≤ 64 / ≤ 4 |
| GPU particles alive | ≤ 30,000 |
| View build (CPU) | ≤ 4 ms per frame (debug-build measurement, relative) |
| Sim tick, full population | ≤ 3 ms native release; ≤ 8 ms in the browser (stats overlay ms/tick) |
| Active entities | ≤ 450 (cars ≤ 45 + police, peds ≤ 70, props, pickups) |
| Snapshot size (`approx_bytes`) | measured in M2; the history window shrinks to fit ≤ 400 MB |

Strategy: persistent region batches + culling (E7) first; then chunk-merged decor meshes if draw
calls exceed budget; AI LOD (§5.6.6–7); population caps; light pooling (nearest 64); small-prop
distance culling; dynamic resolution (existing `quality.rs`) and presets (Low/Medium/High/Ultra:
render scale, shadows radius, SSGI, haze, MSAA, draw distance, population ×0.6/0.8/1/1.1). The
user's RTX 3060 Ti should run High at 1080p/60; the on-screen readout lets them confirm.

### 6.10 Persistence
- `SaveData` (serde JSON, versioned): per protagonist (cash, weapons + ammo, stats, outfit palette,
  position, last vehicle), current protagonist, mission states and medals, heist choice and cut,
  activities, collectibles, completion, time and weather, settings snapshot, playtime, game version.
- Slots: `slot1..3` and `auto`. Native: files next to the executable
  (`grand_theft_freeway_<slot>.json`); web: `localStorage` keys with the game prefix.
- Settings (audio, display, controls, accessibility) persist separately and apply at boot; app-level
  ones (quality, v-sync, fullscreen) are applied by the host from `AppRequests.settings`.

### 6.11 Builds and platforms
- **Native (primary for the user):** Windows `.exe` cross-compiled here (`scripts/setup-linux.sh`
  once for mingw + Wine; `scripts/build-windows.sh`), Linux binary. Vulkan default, DX12 option.
- **Web (ai-hub deliverable):** `scripts/build-gtf-web.sh` → `dist/GrandTheftFreeway-BX.html`
  (single file, runs from `file://` in Chrome/Edge with WebGPU). The page carries the Source
  Information block (§1) in an HTML comment at the top and in its `<meta name="description">`, and
  the game shows it on the About screen. Optional Vercel deploy keeps working (`vercel.json` builds
  the game's page).
- **Phones/tablets:** the same web build; touch → attract only (§5.8).
- Controllers: gilrs 0.11 / gilrs-core 0.6.8. On Windows its default `wgi` backend enumerates
  Windows Gaming Input `RawGameController`s, so a DualSense works without XInput wrappers (checked
  in the crate source during planning); Linux uses evdev; the browser uses the Gamepad API's
  "standard" mapping. Triggers arrive as analog button values (`button_data(..).value()`). Testing
  a real DualSense is outside this environment: record it as a user check, and keep the glyph
  override setting as the fallback.

---

## 7. Tools and verification

The agent works without a display. Every system gets a way to be checked by tools, and checks are
cheap enough to repeat (AGENTS.md "Testing budget": test to change a decision; tool checks with no
model calls may scale up).

### 7.1 The `gtf` CLI
`gtf` is `pav` with the game registered: every engine tool (§ `pav help`) plus the E12 additions.
Game-specific actions go through `play cmd="..."` (the same commands menus and tests use), e.g.
`play cmd="teleport landmark:bank"`, `play cmd="heat 3"`, `play cmd="time 21:30"`,
`play cmd="weather rain"`, `play cmd="mission start m1_repo_man"`, `play cmd="switch roxy"`,
`play cmd="validate"` (content errors), `play cmd="give weapon p9"`. Dev-only commands are refused
in release builds.

### 7.2 Reference views (`content/freeway/views.toml`)

| View | Where / when | Why |
|---|---|---|
| `downtown_street` | Street level at the bank plaza looking down the avenue, 13:00, rush traffic | Towers, density, peds |
| `freeway_chase` | Chase cam on Route 404 at 30 m/s, 18:10, 2★ | Speed, traffic, LOD switch |
| `beach_sunset` | Boardwalk toward the pier and Big Wheel, 18:40 | Golden hour, crowds, water |
| `hills_vista` | Lookout Point over the city, 20:15 | Longest draw distance (heaviest) |
| `docks_night_rain` | Harbor Gate containers, 23:00, rain | Lights, wet, particles |
| `ditch_pursuit` | The Ditch, 01:00, 3★ with SPPD Air | Searchlight, sirens, police AI |

`gtf perf view=all` checks §6.9 budgets at each; `gtf capture scene=freeway/view/<name> hud=true
width=960 height=540 out=...` gives the look check. Keep a dated comparison set of captures under
`out/` (not committed) and note in `PROGRESS.md` what changed and why.

### 7.3 Feel checks
- `gtf vehicle_feel key=<archetype>` on the dev district's test pad: 0–100 km/h, top speed,
  braking distance, skidpad g, handbrake slide angle, against §5.6.3 targets (a test asserts the
  ranges for every archetype).
- On foot: the engine's `feel` tool (response ticks, time to top speed, turnaround) with the game's
  movement; targets: response ≤ 2 ticks, run in ≤ 0.25 s, sprint in ≤ 0.4 s.
- Camera: `filmstrip hud=true` while driving a scripted slalom and while orbiting on foot; look for
  jitter, clipping into walls, late recentring.

### 7.4 Browser checks (`scripts/web-check.mjs`, Playwright, headless Chromium)
`scripts/web-check.mjs` is provided and was tested in planning on the starter's single-file build
(desktop and mobile emulation; steps `press:KEY:MS`, `tap:X:Y:MS`, `wait:MS`; `--expect`/`--forbid`
console lines; screenshots and `console.log` per run). Its launch flags are the ones that work here:
`--enable-unsafe-webgpu --enable-features=Vulkan --use-vulkan=swiftshader --use-angle=swiftshader
--ignore-gpu-blocklist` (SwiftShader runs about 0.5–2 frames per second: use long waits and read
state from logs, not from smooth video). The game must log frontend state changes (`frontend:
title`, `frontend: attract <vignette>`, ...) and, with `?perf=1`, a perf line every 5 s (fps, frame
ms, sim ms/tick) to the console. Planning observed that mobile emulation shows the engine's default
touch controls — the game's touch mode must remove them (§5.8, E13).
1. Desktop: load the HTML → boot ok → title → wait ≥ 11 s → log shows attract running → press a
   key → log shows title within 1 s of the next frame; screenshots at each step.
2. Mobile emulation (viewport 844 × 390, `isMobile`, `hasTouch`) → attract starts by itself →
   a tap anywhere does not leave attract → tapping the gear opens the overlay → screenshot.
3. Perf line: sim ms/tick within budget at a busy vignette.

### 7.5 Soaks and anomaly counters
The game keeps counters in `status` (and `gtf soak` reports them): `stuck_cars` (no progress > 30 s
while not parked), `stuck_peds`, `spawn_in_view`, `despawn_in_view`, `ped_on_road` (outside
crosswalks), `ai_crashes` (AI–AI collisions), `nan_positions`, `fell_through_world`, `vignette_stalls`,
`mission_bot_failures`, `content_errors`.

| Soak | Command | Limits |
|---|---|---|
| Traffic | `gtf soak mode=traffic seconds=600` (bot cruising the district) | stuck_cars after recovery 0, ai_crashes ≤ 3, spawn/despawn_in_view 0, NaN 0, fell_through 0 |
| Pedestrians | `gtf soak mode=peds seconds=600` | stuck_peds ≤ 2, ped_on_road ≤ 5, NaN 0 |
| Roam | `gtf soak mode=roam seconds=900` (bot drives, fights, gets heat, evades) | all of the above + no panic, sim within budget |
| Attract | `gtf soak mode=attract seconds=1800` | vignette_stalls 0, all vignettes played ≥ 2 times, no panic |

### 7.6 Native smoke (`scripts/smoke-gtf.sh`)
Xvfb + lavapipe + `--pad-script` (the engine's scripted controller): title → new game → walk to a
car → drive 10 s → pause → settings → back; `xwd | convert` screenshots per step (the full window,
egui included).

### 7.7 Determinism
- Record 2 minutes of roam with heat and traffic (`record_save`), `replay` it: identical hash.
- Snapshot mid-chase (`snapshot_save`), load it in a fresh session, step 600 ticks: identical hash to
  the uninterrupted run.
- Generate the district twice from the same seed: identical city hash; a different seed changes
  dressing but never landmarks or roads.

### 7.8 Test suites (`crates/gtf_game/tests/`, plus engine suites for engine changes)
`content.rs` (every file parses and validates; every referenced asset, clip, sound cue and spot
exists), `district.rs` (generation determinism, connectivity: every lane reachable, every sidewalk
loop connected, landmarks placed, no building on a road), `frontend.rs` (boot → title; 600 idle
ticks → attract; any input → title; menus navigable with pad semantics; touch platform → attract
only), `player.rs`, `vehicles.rs` (enter/exit, carjack, damage → explosion, feel ranges),
`traffic.rs` (signals obeyed, no AI–AI crashes in 2 minutes at an intersection, pull-over for
sirens), `peds.rs` (crosswalks on walk phase, flee on gunfire, witness report), `combat.rs`,
`heat.rs` (crime → report → stars; evade 3★ bot; busted; wasted), `missions.rs` (every mission by
bot on 3 seeds, both heist approaches; fail → retry → skip after 3 fails), `protagonists.rs`
(switch keeps each character's state), `save.rs` (round trip), `attract.rs` (every vignette starts,
runs, ends; loop order deterministic per seed), `replay.rs` (§7.7), `ui.rs` (UiModel snapshots of
key states: HUD values, pause tabs, wheels, phone, banners).

---

## 8. Content production pipeline
- **Props (city and vehicles):** the engine's Object Studio loop, headless: `assets find=` first
  (reuse `LIB/core` where it fits: lamp_post, park_bench, bench, fence, signpost, barrel, crate
  stacks), then `asset_edit action=create` from a brief, `check` to zero findings, a 4-view sheet
  at the game camera (`asset_preview views=...`), at most two fix rounds, publish into
  `assets/props/lib/gtf_city` or `gtf_vehicles` with `PACK.json` (CC0, own work), add the packs to
  `props::store::SHIPPED` and `pav_core/build.rs` so every build (and the browser) carries them.
  (AGENTS.md: assets made through this loop need no human review.)
- **Vehicles:** one prop per body: an `extrude` side profile across the width for the shell, separate
  glass, bumpers, trims, light parts named `head_l/r`, `tail_l/r`, doors as named parts (`door_l`,
  `door_r`), roof sign or light bar; a wheel prop per class. Check silhouettes at the chase camera.
- **Materials:** `assets/materials/gtf/*.json` (asphalt with aggregate, concrete, stucco, brick,
  corrugated metal, glass curtain wall with the window kernel, sand, grass, terracotta, car paint
  with metallic), checked with `asset_preview kind=material` swatches.
- **Animation:** reuse clips (§4); author missing poses (hands up, cower, drive-seated, cover lean,
  aim pistol low/high blend) with `anim_edit` into `anim/` WORKSHOP sets; credits kept.
- **Sound and music:** cues in `sounds.toml`, stations in `radio.toml`. Listening is not possible in
  this environment: render with `audio_capture` (.wav) and check with a script — peak and RMS levels,
  clipping, DC offset, silence gaps, and a coarse spectrum (bass present, no harsh high-end buzz).
- **Missions:** TOML + `play cmd="validate"` + mission bot runs; author in `missions/`.
- **Text:** all strings in `text.toml`; satire lists (billboards, headlines, DJ lines, texts) too.
- Subagents may take independent content batches (e.g. "author 8 street props from these briefs")
  with the loop above; the main agent integrates and verifies (§10.5).

---

## 9. Milestones

Each milestone ends with: all tests green, the game runs (native under Xvfb smoke + `gtf` bots),
`PROGRESS.md` updated (tasks, evidence, decisions), commits pushed, the PR description's checklist
updated. Effort: S < M < L < XL (relative).

### M0 — Bootstrap (S)
Goal: the engine is in the repository, builds, tests pass, and an empty `freeway` scene runs in every
build target.
- [ ] Install system packages (`START_HERE.md` list) and `scripts/setup-linux.sh` parts needed for
      Windows cross-builds (mingw, Wine) — or defer Wine to M10 and note it.
- [ ] `scripts/vendor-engine.sh` (tested in planning: 1,359 files, starter tests pass), commit as
      `engine: vendor Pavilion at 0098196`.
- [ ] Merge `docs/ENGINE_CLAUDE.md` imports into our `CLAUDE.md` (`@docs/DOCTRINE.md`), keep ours on
      top; note in `CLAUDE.md` that `AGENTS.md`'s merge-to-main convention does not apply here.
- [ ] Baseline: `cargo build --locked -p pav_tools --bin pav`, `cargo test --workspace --locked`
      (record which suites need a GPU/network and how upstream marks them), `pav play scene=starter
      bot=true seeds=5`.
- [ ] E1: registry (`games::register/all/find`), `pav_tools::cli::main`, `pav_app` lib/bin split
      with `AppConfig`, crates `gtf_game` (a `Freeway` game: flat ground + player) and `gtf_app`
      (two bins), scripts (`build-web.sh --package/--bin`, `build-gtf-web.sh`, `build-windows.sh`,
      the session-start hook building `gtf`).
- [ ] Single-file web build of `freeway` boots in headless Chromium (`scripts/web-check.mjs` basic).
- Acceptance: `gtf load scene=freeway` + `gtf play` work; `grand_theft_freeway` opens a window under
  Xvfb; `dist`-style HTML boots; engine tests still pass; Shardfall's `shardfall` binary unchanged.

### M1 — Platform: hook v2, input v2, presentation, UI kit, camera (L)
Goal: a game can pause, slow time, drive vehicles, describe its camera, UI, drawing, view and audio
as data; the 10-foot UI kit renders it in the window and in headless captures.
- [ ] E2 `InputFrame` v2 (+ host mapping for keyboard/mouse/gilrs/touch; `input` tool args).
- [ ] E3 hook v2 (`time_scale`, `world_paused`, `vehicles`, `present`, `history_seconds`).
- [ ] E4 `pav_core::present` types (+ serde, `ui` tool JSON dump), generic `SimEvent`s.
- [ ] E5 `pav_ui`: theme, fonts (OFL: display + UI + script, each with its `OFL.txt`, from
      `https://raw.githubusercontent.com/google/fonts/main/ofl/<family>/<File>.ttf` — reachable from
      this environment in planning), glyphs, every widget type, minimap with a placeholder raster,
      headless render; `capture hud=true`.
- [ ] E6 camera intent + DrawList rendering in `pav_view`; E13 hook-game shell in `pav_app`
      (camera, UI, Esc routing, F1 dev menu, activity, device family, FPS readout, persistence of
      `AppRequests`).
- [ ] E8 part 1: sound bank as data + `SimEvent::Sound`; loop voices with live params (engine,
      siren); FX bus skeleton.
- [ ] `gtf_game`: frontend skeleton (boot → title → main menu → free roam), a **UI gallery** dev
      screen showing every widget with sample data, the orbit camera, pause menu that freezes the
      world, weapon-wheel slow-mo demo, idle timer stub logging "attract would start".
- Acceptance: HUD captures (`hud=true`) of the gallery look right at 1920 × 1080 and 960 × 540;
  pause freezes physics (test); time scale 0.25 moves the world at a quarter speed (test); old replays
  (Shardfall, starter) still replay; `ui` JSON matches the gallery; the FPS readout shows in the
  window smoke screenshot.

### M2 — The city (XL)
Goal: Paloma Bay generated from data, streamed, lit by a sky and time of day, inside budgets; the
dev district for tests.
- [ ] `pav_world`: macro map parser, roads and intersections, blocks/lots/buildings by zone, street
      dressing, terrain (hills), spline roads, elevated freeway + ramps (spline meshes with trimesh
      colliders), the Ditch, beach and ocean, docks; lane graph, signals, sidewalk graph, parking,
      spawn points, cover points; map raster; queries; chunk regions for streaming.
- [ ] First `LIB/gtf_city` props needed by dressing (lamp, signal, hydrant, bin, bench, palm, tree,
      sign, bus stop, billboard, newspaper box, meter, power pole, AC unit, water tank, antenna,
      container, crane, barrier, cone) and `MAT/gtf` materials.
- [ ] E7: sky pass, time of day + `ViewIntent` from `world.toml` presets, window kernel, culling with
      persistent region batches, shadow follow; E6 `StaticBatch` emission.
- [ ] `content/freeway/district.toml` (first full macro map), `dev_district.toml` (256 m: a signalized
      4-way, a T-junction, an on-ramp, a parking lot, a cover range, a jump ramp, the vehicle test
      pad), `views.toml`. Each dev-district area gets a 3D-text sign naming the system it exercises
      and the `gtf` command that checks it, so `freeway/dev` doubles as the engine's urban tech demo
      (the "developer demo level") and as the fast fixture for tests.
- [ ] Tools: `citymap`, `lanes`, `perf`; the `freeway/view/<name>` scenes.
- Acceptance: city hash stable across runs; connectivity tests pass; drive (scripted) across the
  whole district with streaming and no holes (filmstrip); `perf view=all` within budget (no
  population yet); captures of all six views at three times of day look like a SoCal city (record
  what you changed after looking); web build boots into the city.

### M3 — Player and vehicles (L)
Goal: walking, sprinting, climbing and driving feel good; cars are cars.
- [ ] Player controller (§5.6.1 minus combat), stamina, vault, ledge climb, swim, crouch,
      interaction prompts; camera modes (§5.6.2) with collision; aim camera.
- [ ] E9 vehicles v2; `vehicles.toml` with all archetypes; prop bodies for at least the 6 most common
      (others may use temporary bodies until M7); enter/exit animation, carjack, locked cars, damage
      states, explosions, tyres, lights, horn, police lights/siren, occupants drawn; helicopter
      flying; water.
- [ ] Audio: engine loops per class, tyre squeal, horn, siren, rotor; collision sounds by impulse.
- [ ] GPS route ribbon + minimap route (pav_world routing).
- [ ] Radio skeleton: station clock in the sim, one placeholder station rendering in `pav_audio`.
- [ ] `vehicle_feel` tool + ranges test.
- Acceptance: feel tests pass for every archetype; enter/exit/carjack/explosion tests pass;
  filmstrips of driving and on-foot movement show no jitter or clipping; perf still in budget.

### M4 — Population (XL)
Goal: the city lives.
- [ ] Traffic agents (§5.6.6, §6.6.9) with signals, turns, lane changes, yielding, reactions, buses,
      parked cars, LOD switching.
- [ ] Pedestrian agents (§5.6.7, §6.6.10) with crosswalks, scenarios, reactions, LOD; `peds.toml`
      archetypes with ≥ 30 distinct looks.
- [ ] Population manager with zone/time densities; anomaly counters; `traffic`, `peds`, `soak` tools.
- [ ] Ambience beds (city, crowd, surf, harbor) driven by population and place.
- Acceptance: traffic and pedestrian soaks within limits (§7.5); signal/crosswalk/pull-over tests;
  `perf view=all` within budget **with** population; captures show believable streets.

### M5 — Combat and heat (L)
Goal: shoot, fight, get wanted, escape.
- [ ] Weapons (§5.6.4) from `weapons.toml`, held props, clips, hitscan, aim assist, recoil, spread,
      reload, weapon wheel, pickups, grenades, melee combos, cover system with cover points,
      vitals, hit reactions, deaths, drive-by, explosions and chain reactions; VFX and sounds.
- [ ] Enemy combat AI (cover, peek, flank, grenades, retreat).
- [ ] Heat (§5.6.5): crimes, witnesses, levels, dispatch along lanes, police car pursuit (ram, box,
      roadblocks, spike strips), foot units, SPPD Air with searchlight, search zone + cones, flashing
      stars, evasion, respray, hide spots; Busted and Wasted flows; police scanner audio.
- [ ] Hands-up/cower/arrest clips authored.
- Acceptance: combat tests; heat tests including the 3★ evasion bot (≤ 180 s on 3 seeds) and
  busted/wasted; police never spawn in view (counter 0 in a roam soak); captures at 3★ night show
  the searchlight and cones.

### M6 — Missions, protagonists, phone, economy, save (L)
Goal: the game's structure works end to end with placeholder content.
- [ ] Mission DSL types + validator + runtime + bot policies per step kind; markers, intro card with
      medals, objectives, subtitles + babble, cutscene shots + letterbox + skip, checkpoints, fail
      card, retry, skip after 3 fails, results screen with medals and cash, replay from the menu.
- [ ] Protagonists: profiles, switch wheel, sky-cam, abilities with meters and effects, life spots.
- [ ] Phone with all apps (§5.6.9); messages system; camera app (photo targets).
- [ ] Economy and shops (gun store, mod shop, store) as 10-foot menus; fees; payouts.
- [ ] Save/load (slots + autosave), settings persistence (native and web).
- [ ] Two test missions in the dev district exercising every step kind.
- Acceptance: mission tests (bot completes both test missions on 3 seeds; fail/retry/skip); switching
  keeps state; save round trip; phone and shop screens captured with HUD.

### M7 — Content: the vertical slice (XL)
Goal: Paloma Bay is finished and full of things to do.
- [ ] Final district map, landmarks and assemblies, interiors (gun store, mod shop bay, Gas-N-Grab,
      Grind House, Freeway Diner, safehouses, warehouse), signage and billboards (satire), the Big
      Wheel turning, cranes, the PALOMA sign.
- [ ] All vehicle bodies (§5.6.3), all city props needed, ped look variety.
- [ ] Missions: `m1`–`m3`, `s1`–`s3`, `h0` board, `h1` Loud and Smart (§5.7), with dialogue and shots.
- [ ] Activities: race, 5 stunt jumps, 2 hold-ups, vigilante, 2 random events, 15 Lost Shields.
- [ ] Radio: 5 music stations + SPNN (songs, idents, DJ cards, headlines, ads); scanner chatter.
- [ ] Completion tracking and the stats screen.
- Acceptance: every mission and activity completed by bots on 3 seeds (both heist approaches);
  `content.rs` validation clean; captures of every landmark at its best time of day; completion can
  reach 100% (test drives the bots through everything).

### M8 — Frontend, attract mode, touch (M)
Goal: the full player journey from boot to credits, and the kiosk loop.
- [ ] Boot splash, title with live background, main menu, new-game intro, loading cards (in-engine
      art with halftone/painterly filters), pause menu tabs, settings (all of §5.9), About/credits
      with the Source Information block and asset/font credits.
- [ ] Attract director (§5.8): playlist, vignette setups, bots, shot lists, captions, logo/prompt
      overlay, instant exit on any input, anomaly handling; `freeway/attract` scene.
- [ ] Touch mode: attract only + gear overlay; touch controls disabled for the game.
- Acceptance: `frontend.rs` and `attract.rs` pass; attract soak 30 min clean; `web-check.mjs`
  desktop and mobile flows pass with screenshots; every frontend screen captured with HUD.

### M9 — Polish (L)
Goal: it feels finished.
- [ ] Art pass with the reference views at every time of day and weather; grading; signage; night
      lights; wet roads; VFX completeness (§5.11 list); camera polish.
- [ ] Animation polish (transitions, enter/exit timing, aim/cover poses, occupants).
- [ ] Audio mix (levels, ducking, reverb zones, distance curves), radio polish, UI sounds.
- [ ] UX pass against the 10-foot rules (§5.9) for every screen; accessibility settings working;
      tutorial help texts for first-time actions; prompts everywhere.
- [ ] Performance pass: budgets with margin, quality presets, dynamic resolution, browser sim time.
- [ ] Bug bash: roam/attract/traffic soaks, mission bots on more seeds (5), edge cases (water,
      falling, interiors, switching mid-chase, saving during heat is refused).
- Acceptance: all D-items except D14–D15 demonstrably true in `PROGRESS.md` with evidence.

### M10 — Release and handoff (S)
- [ ] Builds: native, Windows `.exe` (zip), `dist/GrandTheftFreeway-BX.html` with the Source
      Information block in its header comment and `<meta name="description">` (commit it once; note
      its size), optional Vercel config updated.
- [ ] Docs: `README.md` (final), `docs/gtf/ENGINE_ANALYSIS.md`, `docs/gtf/HANDOFF.md`, engine docs for
      every extension, `PROGRESS.md` final with the DoD evidence table complete.
- [ ] Final verification run of D1–D13 commands; fix anything red.
- [ ] Push, update the PR description (summary, how to play, checklist), send the HTML and the zipped
      `.exe` with `SendUserFile`, final chat report (§12.4).

---

## 10. Workflow rules for the implementer

### 10.1 Git and the pull request
- Work on the session's designated branch (`claude/keen-cerf-g5vksy` at planning time). Push with
  `git push -u origin <branch>`; on network errors retry up to 4 times (2 s, 4 s, 8 s, 16 s).
- A draft PR for the branch already exists (opened with this plan). Keep pushing to it; update its
  description at every milestone end (milestone checklist, what is playable, how to try it). Never
  merge it, never force-push, never rewrite pushed history.
- The engine's `AGENTS.md` says to merge PRs into `main` when finishing: **that convention does not
  apply in this repository** (its "Key rules" do).
- Commit small, coherent changes with prefixes: `engine:` (engine crates and engine docs — keep these
  free of game code so they can be offered upstream), `game:` (`gtf_*`), `content:` (TOML, props,
  materials, clips), `docs:`, `build:` (scripts, CI). End every commit message with the session's
  prescribed attribution lines; apart from those lines, no model identifiers in commits, PRs, code
  or docs.
- Push at least at every milestone end and whenever a meaningful chunk is done (cloud containers are
  temporary). Commit `dist/GrandTheftFreeway-BX.html` once, at M10 (it is ~11+ MB).

### 10.2 Checks
- Before each push: `cargo fmt --all`, `cargo clippy -p <touched crates> --all-targets -- -D
  warnings`, the tests of touched crates, `scripts/check-wgsl.sh` after shader edits.
- At each milestone end: `cargo test --workspace --locked`, wasm clippy for `pav_app` and `gtf_app`,
  the milestone's acceptance commands, a native smoke screenshot, a web build boot check.
- Disk is a fixed allowance: dev builds already use line-table debug info and no incremental cache;
  delete stale `target/` artefacts (e.g. old `target/debug/deps` binaries, `target/wasm32-*`) when
  space runs low.

### 10.3 Progress tracking
- `PROGRESS.md`: tick tasks as they land, fill the DoD evidence table, add decisions (date, choice,
  why) and open issues. Keep its "Now / Next" section current: it is what a resumed or compacted
  session reads first.
- Engine docs: when an engine extension lands, update `docs/MAKE_A_GAME.md` / `docs/REFERENCE.md`
  in the same `engine:` commit (respect `docs_limits.rs`: `AGENTS.md` < 32 KiB and ~250 lines,
  `docs/PROGRESS.md` ≤ 60 lines — the engine's own PROGRESS, not ours).

### 10.4 Verify with tools, look at results
- Prefer `gtf` tools over reading engine internals (doctrine 2–3; `MAKE_A_GAME.md` "Check with the
  tools"). If you had to read engine source to use the engine, note it for the engine analysis.
- Captures: small sizes (960 × 540 or 640 × 360) with `hud=true` when UI matters; look at every
  capture you take; write one line in `PROGRESS.md` about what you saw and changed for looks work.
- Bots are the gameplay test: teach the bot every rule you add (starter-game principle).

### 10.5 Subagents
- Use subagents for independent, verifiable batches: prop and vehicle authoring through the asset
  loop, material swatches, sound cue tuning, radio station data, mission TOML drafts validated by
  `play cmd="validate"` and bot runs, adversarial review of a milestone diff (`/code-review`), docs
  drafts. Give each a self-contained brief (files, rules, acceptance) and integrate their output
  yourself.
- Do not run parallel subagents that edit the same files; use `isolation: "worktree"` for parallel
  code work and merge deliberately.
- Multi-agent workflows (Ultracode) are not required; content milestones (M7) are where fan-out
  pays most if the user enables it.

### 10.6 Decisions and escalation
- Decide everything that is not genuinely the user's call; record it in `PROGRESS.md` → Decisions.
- The user's calls: spending money or creating accounts, publishing outside this repository and the
  PR (e.g. pushing to `ai-hub`), licences beyond OFL fonts and CC0/own-work assets, anything the
  doctrine says needs approval (binary assets other than fonts). Ask in chat with a default, set a
  15-minute check-in if `send_later` is available, continue with other work, and apply the default
  if there is no answer (doctrine "Escalation"); log it in `docs/ESCALATIONS.md`.

### 10.7 Doctrine recap (binding)
- No image, model, texture or audio files; fonts allowed (OFL, licence files kept). Props, materials,
  animation, sound, music are text or code. Never commit downloaded models or images.
- `pav_core` and `pav_world` and `gtf_game`: no graphics, window or audio dependencies; no threads,
  blocking calls or file access on wasm (embed data; disk reads only for native hot reload).
- Deterministic gameplay (§6.4). WebGPU only. "It must run well on a modern PC" (RTX 3060 Ti).
- Keep upstream engine behaviour working (Shardfall, the pavilion, the starter) — their tests stay
  green.

### 10.8 Cost and context discipline
- Keep tool outputs small (compact JSON, `limit=`, small captures); don't dump big files into the
  context; read the plan section you need, not the whole plan, after the first read.
- Follow `AGENTS.md` "Testing budget" for any agent-run evaluations (judges, benchmarks): small
  batches, decide on clear gaps, stop near 50/50.

---

## 11. Risks and mitigations

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| City rendering too heavy (no culling today, per-frame uploads, many draw calls) | High | High | E7 batches + culling early in M2; measure at reference views before adding detail; chunk-merged decor meshes; distance classes; fog distance; quality presets |
| Simulation cost of traffic and peds (rapier controllers, raycast vehicles) | Medium | High | AI LOD tiers from the first agent version; population caps; profile with `bench` per system; kinematic rails |
| Browser main-thread sim (no threads in wasm) | Medium | Medium | Same LOD; browser budget 8 ms; lower population preset on web/touch; attract vignettes chosen for cost |
| egui can't reach a console-quality 10-foot look | Medium | Medium | Custom painting (meshes, gradients, shadows), real fonts, animation; compare captures; keep layouts simple and big |
| Headless HUD capture (egui-wgpu offscreen) harder than expected | Low | Medium | Fall back to the web build in headless Chromium and the native Xvfb smoke (both show egui today, slowly) |
| Sim-side camera feels laggy at high refresh rates | Medium | Medium | Interpolate between ticks (alpha), consider 120 Hz ticks for the camera only if needed; mouse look ≤ 1 tick latency |
| Vehicle handling never feels right with the raycast controller | Medium | High | `vehicle_feel` targets; arcade helpers (yaw assist, anti-roll, downforce, slip curves) on top of rapier; tune by data |
| Procedural music sounds bad | Medium | Medium | Strong genre templates (progressions, patterns), FX bus, mixing; ≥ 4 stations required, cut the weakest |
| Mission bots brittle | High | Medium | Bot policy per step kind with timeouts and recovery; missions authored for bot-ability (clear anchors, no tiny targets) |
| Scope too large for one run | Medium | High | Milestone order keeps a playable game at each step; DoD is fixed, stretch is separate; content is data-driven so M7 is mostly authoring |
| `InputFrame` loses `Copy` → wide refactor | Medium | Low | Mechanical; compiler-guided; do it first in M1 |
| Snapshot memory with a big city | Medium | Medium | `history_seconds` 10 s; city caches `Arc` + `serde(skip)`; measure `approx_bytes` |
| DualSense mapping on the user's Windows/browser differs | Medium | Low | gilrs WGI + SDL mappings, web standard mapping; glyph override setting; list it as a user check |
| Disk allowance fills with build artefacts | Medium | Low | Clean stale targets; avoid release builds except for packaging |

---

## 12. Deliverables and handoff

### 12.1 `README.md` (final)
Title and one-paragraph pitch; feature list; **how to play** (web: open the HTML in Chrome/Edge with
WebGPU; Windows: run the `.exe`; controller recommended, PS5 glyphs by default); controls tables
(§5.10); building from source (Linux packages, `cargo run -p gtf_app --bin grand_theft_freeway`,
web, Windows); the `gtf` CLI in one paragraph; project layout; credits (fonts, motion libraries
credited by the engine's store, engine); the **Source Information** block (§1 — lab and effort
levels; the exact model goes in the delivery chat report, not in repository files); a link to
`docs/gtf/ENGINE_ANALYSIS.md`.

### 12.2 `docs/gtf/ENGINE_ANALYSIS.md` (the engine evaluation, separate from the source info)
Answer from experience, candidly:
1. **What we used** (engine systems, tools, data formats) and how well each served.
2. **What we didn't use** and why (e.g. Shardfall's arpg layer, studios, rooms for the city).
3. **The one change** we would make to the engine, with the reason and the evidence.
4. The `MAKE_A_GAME.md` "Report back" items: engine source we had to read and why; what we wrote
   that the engine should have had (with files); wrong or missing docs; the change that would have
   helped most.
5. Numbers: lines of engine changes vs game code vs content data; build times; budgets reached.
6. A list of `engine:` commits suitable for upstreaming.
Also append a short entry for this game to the vendored `docs/FEEDBACK.md` "Games" section in its
format.

Planning-phase hypothesis for item 3 (confirm or replace with what the build taught): *let games
describe their presentation as data through the hook — UI model, camera intent, draw list, view and
audio scene — and let game crates live outside `pav_core`*. That one change removes most of the
reasons a game had to edit `pav_app`, `pav_view` and `pav_core` (the first test game's top asks).

### 12.3 `docs/gtf/HANDOFF.md`
For the next agent expanding toward the full game: architecture overview (crate graph, data flow);
how to add a district (macro map + zones + landmarks), a landmark or interior, a mission (DSL
reference with every step kind and fail condition, bot policies), a vehicle, a weapon, a ped
archetype, a radio station, an activity, an attract vignette, a reference view; how to verify each
(commands); known issues and the stretch list (§13); where the engine extensions live.

### 12.4 Final chat report
Short: what was built, how to play (files sent), the Source Information block (§1, with the
implementation's own confirmed model and effort), the engine analysis summary (used / not used /
the one change), known issues, and the PR link.

---

## 13. Stretch goals (not part of the Definition of Done)
Motorbikes, boats, planes; a second district; clothing stores, barbers, tattoos; property purchases;
golf, tennis, yoga; first-person camera; a stock market; in-game internet; the Big Wheel as a ride;
cinematic replay editor; photo mode with filters; controller remapping UI beyond presets; co-op
(the engine's on-hold plan); voice-like dialogue beyond babble.

---

## Appendix A — the `/goal` prompt for the implementation run

```
/goal Implement Grand Theft Freeway-BX in this repository by executing PLAN.md to completion.
Read CLAUDE.md, PLAN.md (§0-§4, §10 first) and PROGRESS.md, then work milestones M0→M10 in order:
vendor the engine (scripts/vendor-engine.sh), build the engine extensions, the city, player and
vehicles, traffic and pedestrians, combat and heat, missions with the three-protagonist heist,
the 10-foot controller-first UI, attract mode (10 s idle → AI demo loop; any input returns; touch
devices = attract only) and polish. Keep every milestone playable and green, push to the
designated branch, keep the draft PR updated, and record evidence in PROGRESS.md.
Done = every Definition of Done item D1-D15 in PLAN.md §2 is checked in PROGRESS.md with
evidence, dist/GrandTheftFreeway-BX.html and the Windows .exe are built and sent to me,
README.md carries the Source Information block and docs/gtf/ENGINE_ANALYSIS.md answers what you
used, didn't use and the one engine change — and your final message reports both.
```

## Appendix B — command cheat sheet (after M0/M1)
```sh
. /tmp/repo-setup/<repo>.env 2>/dev/null; export CARGO_BUILD_JOBS=4
cargo build --locked -p gtf_app                      # both binaries: grand_theft_freeway, gtf
alias gtf=./target/debug/gtf
gtf play scene=freeway/roam bot=true ticks=3600      # roam bot
gtf play scene=freeway/mission/m1_repo_man bot=true seeds=3
gtf capture scene=freeway/view/beach_sunset hud=true width=960 height=540 out=out/beach.png
gtf perf view=all
gtf soak mode=attract seconds=1800
gtf citymap out=out/city.png
gtf vehicle_feel key=okami_kaze
printf 'load scene=freeway/roam\nplay cmd="heat 3"\ninput pad=r2 stick_l=[0.3,0] ticks=120\ncapture hud=true out=out/a.png\n' | gtf repl
cargo test -p gtf_game                               # game suites
cargo test --workspace --locked                      # everything (milestone ends)
scripts/build-gtf-web.sh                             # -> dist/GrandTheftFreeway-BX.html
node scripts/web-check.mjs dist/GrandTheftFreeway-BX.html
scripts/smoke-gtf.sh 20                              # native window under Xvfb, screenshots
scripts/build-windows.sh                             # Windows .exe (after scripts/setup-linux.sh)
```

## Appendix C — where to change what (engine file map at the pin)
| Concern | Files |
|---|---|
| Game hook, registry | `crates/pav_core/src/games/mod.rs`, `games/starter.rs` (template), `scenes.rs` |
| Tick pipeline | `crates/pav_core/src/sim.rs` (`step`, `step_inner`, `frame`) |
| Input | `crates/pav_core/src/input.rs`; host mapping `crates/pav_app/src/input.rs` (gilrs, pad scripts) |
| Frame to the view | `crates/pav_core/src/frame.rs` (`RenderFrame`, `SimEvent`, `HudFrame`) |
| Vehicles | `crates/pav_core/src/vehicle.rs`; drawing `crates/pav_view/src/vehicles.rs` |
| Characters, puppets, clips | `character.rs`, `puppet.rs`, `clips.rs`, `moves.rs`, `ai.rs`, `nav.rs` in `pav_core/src` |
| Streaming, statics, terrain | `world.rs`, `statics.rs`, `terrain.rs` in `pav_core/src` |
| Props and materials | `pav_core/src/props/` (`store.rs` SHIPPED), `pav_core/build.rs` (embedding), `pav_view/src/assets.rs` (`emit_asset`), `pav_view/src/materials.rs`, `pav_render/src/shaders/materials.wgsl` |
| View building, camera | `crates/pav_view/src/build.rs`, `camera.rs`, `look.rs`, `fx.rs` |
| Renderer | `crates/pav_render/src/renderer.rs`, `scene.rs`, `shadows.rs`, `fx.rs`, `text.rs`, `shaders/` |
| Audio | `crates/pav_audio/src/lib.rs` |
| App shell, UI | `crates/pav_app/src/app.rs` (frame loop), `ui.rs`, `hud.rs`, `quality.rs`, `save.rs`, `settings.rs`, `touch.rs`, `platform.rs` |
| Tools | `crates/pav_tools/src/tools.rs` (registry), `session.rs`, `play_tools.rs`, `bin/pav.rs` |
| Web packaging | `scripts/build-web.sh`, `scripts/pack_single_html.py`, `web/index.html` |
