# The Pavilion engine, evaluated by building Grand Theft Freeway-BX

A candid account from the agent that built an open-world game on the Pavilion engine (vendored
from Shardfall at `0098196`): what served, what didn't, what we added, and the one change we
would make. The game's own record is [`PROGRESS.md`](../../PROGRESS.md); how to extend it is
[`HANDOFF.md`](HANDOFF.md).

## 1. What we used, and how well it served

| System | How we used it | Verdict |
|---|---|---|
| The `Game` hook (`pav_core::games`) | The whole game is one hook implementation in its own crate (`gtf_game`, about 36,000 lines), registered at runtime (E1) | **Excellent** once extended (E1-E3): `pre`/`post`, `vehicles()`, `present()`, `time_scale()`, `world_paused()`. After M1 the game never edited the engine's tick loop. |
| Presentation as data (`pav_core::present`, E4) | Every frame the game returns a `PlayFrame`: UI model, draw list, camera intent, view intent, audio scene, map raster, host requests | **The best decision of the build.** The same data drives the window, the browser, headless captures (`capture hud=true`), `gtf ui` (the HUD as JSON) and the tests. |
| The simulation (`Sim`, rapier, determinism) | Seeds, snapshots and replays with the full city, traffic and crowds | **Solid.** Replays and mid-chase snapshots play back exactly (`tests/replay.rs`); the rule "randomness only from `sim.state.rng`" was easy to keep. |
| Characters, puppets and clips | The player, crowds, police and enemies; the store's motion libraries (Quaternius, Mesh2Motion, 100STYLE, CMU) and the engine's WORKSHOP clips; ledge grab and swimming | **Good.** We added a character LOD mode (no controller beyond 30 m), clip control for game code and richer bodies (hair, beards, headwear). |
| Vehicles (`pav_core::vehicle`) | 15 archetypes from data, AI traffic, police, a helicopter | **Rewritten (E9).** Cars were locked to yaw; v2 frees pitch and roll and adds a tyre model, damage, burst tyres, lamps, seats, kinematic traffic LOD and `VehicleInput` for any driver. Now one of the strongest parts. |
| Rendering (`pav_view`, `pav_render`) | Sky and weather, night lights and lit windows, procedural materials, static culling, decals, ribbons, 3D text, painterly and halftone looks | **Good after E7.** Static regions are culled on the CPU into the global batches; the reference views hold the §6.9 budgets on a release build. |
| Props, format 2, the asset store | Every vehicle body, city prop and weapon is a text prop (`LIB/gtf_vehicles`, `LIB/gtf_city`, `LIB/gtf_weapons`); exemplars from `assets find=` | **Very good for agents:** a car is a JSON file of parts we can read, diff and fix. |
| Audio (`pav_audio`) | A sound bank as data, loops with live parameters (engines, tyres, sirens, rotors, ambience), five music genres and talk radio, babble voices — all synthesized (E8) | **Good.** Built almost entirely in this run; the mixer's structure took it without strain. |
| Streaming and regions | Paloma Bay streams as provider regions (E14) | **Had to be built.** The engine streamed only its own pavilion world; a region provider and trimesh statics made a game city possible. |
| Tools (`pav_tools`) | `load`/`step`/`input`/`capture`/`filmstrip`/`ui`/`play`/`audio_capture` in the `gtf` CLI and REPL; game tools (`perf`, `soak`, `citymap`, `lanes`, `vehicle_feel`) through `cli::main_with` | **Excellent.** Every feature was verified headless: captures looked at, soaks counted, bots played missions on three seeds. |
| The web and Windows builds | `scripts/build-web.sh --single-file` and `build-windows.sh` | **Worked.** The single-file HTML runs from `file://`; the Windows build cross-compiles and runs under Wine. |

## 2. What we didn't use, and why
- **Shardfall's ARPG layer** (`pav_core::arpg`: bosses, genomes, loot, brains) and the creature
  systems: a different genre. Combat here is hitscan guns and fists in `gtf_game::combat`.
- **`health.rs`, `projectile.rs`, `ai.rs`, `nav.rs`, `stealth.rs`, `course.rs`, `zones.rs`:**
  their models (rapier projectiles, room-scale navigation, a stealth meter tied to rooms) didn't
  fit a 1 km city. People walk a 2 m tile walk map from `pav_world`; traffic drives a lane graph;
  stealth and witnesses are the game's.
- **Rooms and the pavilion world** for the city: rooms are hand-placed TOML spaces; a district
  generated from a map needed regions from a provider (E14). Rooms stay for the engine's demos.
- **The studios** (animation, asset, creature): GUI tools for people. Their headless twins (the
  `asset_edit`/`asset_preview` tools, the store) were what we used.
- **The engine's egui HUD and pause menu:** replaced by `pav_ui`, a 10-foot kit driven by data
  (E5); the engine's menus moved to one developer panel (F1) for hook games.
- **Rewind** in release, web and touch builds (a development tool here), and **co-op** (on hold
  upstream).

## 3. The one change
**Let games describe their presentation as data through the hook, and let game crates live
outside `pav_core`** — the planning hypothesis, now confirmed by the build. Before it, a game had
to edit `sim.rs`, `frame.rs`, `pav_view/build.rs` and `pav_app/app.rs` (the first test game's top
ask). With E1 + E4 in place:
- the game is two crates with no graphics, window or audio dependencies, and it builds for wasm;
- the window, the browser, captures, `gtf ui` and tests all read the same `PlayFrame`, so a HUD
  bug is visible to a test without pixels;
- engine work stayed upstreamable: 47 `engine:` commits, none containing game code.

The change we would make **next**, from this build's evidence, is the browser loop on slow
devices: on SwiftShader the title screen draws a frame every several seconds and the sim gets at
most 8 ticks a frame, so the game runs at about a thirtieth of real time and a 10-second idle
timer takes over ten minutes. Running the simulation on its own clock (a worker, or more ticks per
frame when rendering is the bottleneck) and picking Low quality on software adapters (done in M9:
`quality.rs`) would make the browser behave like the native build on weak machines.

## 4. Report back (`MAKE_A_GAME.md`)
- **Engine source we had to read, and why:**
  - `sim.rs`, `scenes.rs`, `world.rs`: a hook game's scene ran with streaming off and streaming
    was hard-wired to the pavilion hub (`world.rs`), so a city needed a region provider (E14).
  - `vehicle.rs`: cars could only turn about the vertical axis (`set_enabled_rotations`).
  - `pav_view/src/build.rs`: the view draws only regions whose colliders are loaded; what an
    emissive above 0.5 does (it becomes a light); how events turn into particles.
  - `pav_app/src/simhost.rs`: the browser's tick policy (at most 8 ticks a frame, time dropped).
  - `character.rs`: ledge grab, step height and the controller's cost for crowds.
  - `docs_limits.rs`: the size caps on `MAKE_A_GAME.md` and `AGENTS.md`.
- **What we wrote that the engine should have had** (now in the engine, all `engine:` commits):
  the hook v2 and presentation as data (`pav_core/src/games/mod.rs`, `present.rs`); `InputFrame`
  v2 with controller semantics (`input.rs`); a 10-foot UI kit with headless rendering
  (`crates/pav_ui`); an urban open-world kit — roads, lanes, signals, sidewalks, landmarks from
  parts, far tiles, cover points (`crates/pav_world`); vehicles v2 (`vehicle.rs`); region
  providers (`world.rs`); a sound bank as data, loops, music and voices (`pav_audio`); sky,
  weather, night lights and culling (`pav_render`); `capture hud=true`, `ui`, `play cmd=`,
  `citymap`, `lanes` (`pav_tools`); settings and looks for players (`pav_app`).
- **Wrong or missing docs:** `SimEvent::Fx` was declared but nothing drew it until M9 (fixed in
  `pav_view/src/fx.rs`); `MAKE_A_GAME.md` had no path for drawing or HUD widgets from game code
  (the reason for E4); the emissive-is-a-light rule was undocumented.
- **The change that would have helped most:** presentation as data through the hook (§3).

## 5. Numbers
| What | Size |
|---|---|
| Engine changes since the pin (`git diff 689142b -- crates/pav_*`) | about 30,100 lines added, 1,090 removed; the new crates `pav_world` 8,220 and `pav_ui` 5,990 |
| Game code (`crates/gtf_game`, `crates/gtf_app`) | about 36,500 lines of Rust |
| Game data (`content/freeway`) | about 8,300 lines of TOML |
| `engine:` commits | 47 |

Build times, budgets reached and the browser's figures are recorded in `PROGRESS.md` (D1, D12).

## 6. `engine:` commits suitable for upstreaming
Every `engine:` commit is free of game code (`git log --grep '^engine:'`). The ones worth taking
first: `9bd019a` (games and tools from other crates, E1), `40772ce` (input v2, hook v2,
presentation as data, E2-E4), `b5de689` (hook-game app shell, sound bank and loops, world
regions, E8 and E12-E14), `367da03` (sky, weather, night lights, facade windows, culling, E7),
`990907f` (vehicles v2, E9), `63c64c0` (radio genres, talk and voices, E8), `86f6570`
(landmarks built from parts), `5185f23` (player settings, painterly looks, loading cards) and
the M9 commits (Fx events drawn, software-adapter quality, perf statistics; `1667b10`, the view
no longer building what the camera cannot see; `e611720`, Auto quality for hook games and the
FPS readout's true frame time).

## 7. The engine's systems: used, extended, unused
The Proving Grounds (`freeway/dev`, D16) has a station for each system the game relies on, its
sign naming the system and the `gtf` command that checks it.

| System | Status | Station (its sign) and check |
|---|---|---|
| Streaming and regions | extended (E14) | STREETS & STREAMING: `gtf perf view=hills_vista` (and `freeway/tour`) |
| Lanes, signals and traffic | new (`pav_world`) | SIGNALS & TRAFFIC: `gtf citymap layers=lanes,signals` |
| Pedestrians and the walk map | new (`pav_world`) | PEDESTRIANS & CROSSWALKS: `gtf citymap layers=walk` |
| Parking | new (`pav_world`) | PARKING: `gtf citymap layers=parking` |
| Vehicles and the feel pad | extended (E9) | VEHICLE FEEL PAD: `gtf vehicle_feel`; STUNT JUMP: `gtf vehicle_feel key=okami_kaze` (the kicker jump) |
| Combat and weapons | game code on engine raycasts and clips | COMBAT & WEAPONS: `gtf input cmd="give p9 60" hold=l2 press=r2` |
| Cover | new cover points (`pav_world`) | COVER RANGE: `gtf citymap layers=cover` |
| Heat dispatch | game code on the lanes | HEAT DISPATCH: `gtf play cmd="wanted 3"` |
| Mission DSL | game code | MISSIONS: `gtf play cmd="mission start dev_test"` |
| 10-foot UI (`pav_ui`) | new (E5) | UI GALLERY: `gtf capture scene=freeway/gallery hud=true` |
| Audio and radio | extended (E8) | AUDIO & RADIO: `gtf audio_capture` |
| Characters, puppets and clips | used, extended (LOD, clip control, bodies) | `cargo test -p gtf_game --test player` |
| Rendering: sky, weather, culling | extended (E7) | `gtf perf view=all` |
| Props, materials, the store | used | `assets`, `asset_preview` |
| Determinism, snapshots, replays | used | `cargo test -p gtf_game --test replay` |
| Rewind | used in development only | — |
| ARPG layer, creatures, genomes | unused | — |
| `health`, `projectile`, `ai`, `nav`, `stealth`, `course`, `zones` | unused | — |
| Rooms and the pavilion hub | unused for the city | — |
| Studios (animation, asset, creature) | unused (GUI) | — |
