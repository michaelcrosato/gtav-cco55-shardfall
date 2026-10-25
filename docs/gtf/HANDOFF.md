# Grand Theft Freeway-BX — handoff for the next agent

This is the map for growing the vertical slice toward the full game: how the code is laid out,
how data flows through a tick, and how to add each kind of thing with the command that checks
it. The plan is [`PLAN.md`](../../PLAN.md); where things stand is [`PROGRESS.md`](../../PROGRESS.md)
(Decisions and Open issues); the mission language is [`MISSIONS.md`](MISSIONS.md); the engine's
own docs are [`docs/REFERENCE.md`](../REFERENCE.md) and [`docs/MAKE_A_GAME.md`](../MAKE_A_GAME.md).

## Architecture

### Crates
```
gtf_app ──► gtf_game ──► pav_world ──► pav_core
   │            └──────────────────────► pav_core
   ├──► pav_app (window, input, audio, UI host) ──► pav_view ──► pav_render
   │                                     ├──► pav_ui (10-foot UI kit, egui)
   │                                     └──► pav_audio (synth mixer, music, voices)
   └──► pav_tools (agent CLI: capture, play, ui, input, soak...)
```
- `gtf_game` holds the rules and nothing else: no graphics, window, audio or threads (it builds
  for `wasm32`). It implements the engine's `Game` hook (`pav_core::games`) in `lib.rs`
  (`Freeway`) and describes what to show and play as data each frame: `present()` returns a
  `PlayFrame` (`pav_core::present`: the UI model, a draw list, the camera intent, the view
  intent, the audio scene, the minimap raster and requests to the host).
- `gtf_app` has two binaries: `grand_theft_freeway` (the game: `gtf_game::register()` then
  `pav_app::run`) and `gtf` (the agent CLI: the engine's tools plus the game's in
  `gtf_app/src/tools.rs`: `perf`, `soak`, `citymap`, `lanes`, `traffic`, `peds`,
  `vehicle_feel`).
- `pav_world` builds the city from `content/freeway/district.toml`: blocks, roads and lanes,
  sidewalks and the walk graph, signals, landmarks from parts, the far tiles, cover points. It
  hands the engine one region per streamed chunk (E14).

### A tick
1. The host fills an `InputFrame` (controller semantics in `pad`, typed or tool commands in
   `cmds`, `activity` for any device) and calls `Sim::step`.
2. `Freeway::pre` (`lib.rs`) runs the frontend state machine (`frontend.rs`), the attract
   director (`attract.rs`), commands (`world_cmds`), the sky (`daytime.rs`), then one tick of
   play (`play.rs`: on foot or driving, combat, the crowd, traffic, police, missions,
   activities, the radio, the phone).
3. The engine moves characters, vehicles (with the game's `vehicles()` inputs) and physics.
4. `Freeway::post` reads the tick's events (crashes, hits, explosions), the meter, heat and
   anomaly counters.
5. `Sim::frame` calls `Freeway::present`, which builds the `PlayFrame`; `pav_app` draws it with
   `pav_view`/`pav_render`, `pav_ui` and `pav_audio`. Tools read the same frame (`gtf ui`,
   `capture hud=true`).

State lives in `Freeway` (serde; snapshots and saves) and in the sim's entities. Randomness comes
only from `sim.state.rng`; nothing reads the clock; maps that reach state are `BTreeMap`s. Two
runs with the same seed and inputs are identical (`tests/replay.rs`).

### Scenes
| Scene | What it is |
|---|---|
| `freeway` | the frontend: boot splash, title, menus |
| `freeway/roam` | straight into free roam (most tests) |
| `freeway/dev` | the Proving Grounds: a 256 m test district with a station per engine system |
| `freeway/attract` | the attract loop at once |
| `freeway/gallery` | every UI widget, for captures |
| `freeway/tour` | a camera flying the district (streaming checks) |
| `freeway/mission/<key>` | a mission from `content/freeway/missions/<key>.toml` |
| `freeway/view/<name>` | a still reference view from `views.toml` |

### Modules of `gtf_game`
| Module | Concern |
|---|---|
| `lib.rs`, `play.rs` | the hook, commands, `present`, one tick of play |
| `frontend.rs`, `cards.rs`, `about.rs`, `settings.rs`, `gallery.rs` | menus, the intro and loading cards, About, the player's settings, the UI gallery |
| `attract.rs` | the attract director and the touch-screen kiosk |
| `player.rs`, `cover.rs`, `combat.rs`, `enemies.rs`, `fate.rs` | on foot, cover, weapons, armed opponents, Wasted and Busted |
| `drive.rs`, `vehicles.rs`, `glass.rs`, `struck.rs` | getting in and driving, archetypes, breakable glass, people hit by cars |
| `traffic.rs`, `peds.rs`, `looks.rs` | cars on the lanes, the crowd, people's bodies and clothes |
| `heat.rs`, `police.rs` | the wanted level, dispatch, pursuit, roadblocks, the helicopter |
| `missions/` (`mod.rs` data, `run.rs` runner, `bot.rs` mission bot) | the mission DSL |
| `activities/` | stunt jumps, Lost Shields, vigilante, random events |
| `protagonists.rs` | Dez, Mack and Roxy: their state, abilities, the switch wheel and sky-cam |
| `radio.rs`, `audio.rs`, `ambience.rs` | stations, vehicle loops, ambience beds |
| `daytime.rs`, `world.rs`, `anchor.rs`, `views.rs` | sky and weather, the city, named places, reference views |
| `hud.rs`, `phone.rs`, `shops.rs`, `gps.rs`, `progress.rs` | the HUD and wheels, the phone, shops, the route, saves |
| `camera.rs`, `bot.rs`, `pad.rs` | the camera rigs, the free-roam bot, controller reading |

## How to add things
Content is TOML under `content/freeway/`, embedded at build time; the `gtf` CLI reads the disk
copy when run from the repository root, so a content edit shows without rebuilding `gtf` (tests
use the embedded copy: rebuild them). Every file starts with a comment that documents its
format. After any content change run `cargo test -p gtf_game --test content` (every file loads
and validates; every prop, material, clip, sound cue, spot and landmark named exists).

### A district
`district.toml` is the model: the macro map (64 × 72 cells of 16 m, one character per cell for
the zone, road and height layers), zone rules (building styles, heights, facades by style),
landmarks (each a cell rectangle built from parts in its own frame: `docs/REFERENCE.md`, "Urban
kit"), named spots, the freeway's points and ramps, and stunt-jump wedges. `world.rs` loads one
district; a second would need `world::city()` to take a key and the scene names to carry it.
Check: `gtf citymap out=out/city.png` (zones, roads, lanes, landmarks, spawn and cover points),
`cargo test -p pav_world --test paloma` and `-p gtf_game --test district` (one walking network,
no building on a road, every landmark's door on the network), and the streaming tour
(`load scene=freeway/tour`, `capture`).

### A landmark or interior
A `[[landmark]]` table in `district.toml` (copy one of the 21): walls with openings for doors,
gates and glass, floors and roofs, props and lights, `[[landmark.spot]]` places (counters, beds,
doors). Rooms are `Walk::Indoor` tiles: people walk there, crowds don't spawn there. Check:
`cargo test -p gtf_game --test landmarks`, then capture it by day and night (add a view, below).

### A mission
A file in `content/freeway/missions/` and its key in `missions::FILES`. The language — anchors,
actors, the 26 step kinds, actions, fail conditions, checkpoints, branches and `parallel` — is
documented in [`MISSIONS.md`](MISSIONS.md), with the rules that keep it finishable by the bot.
Check: `gtf play scene=freeway/mission/<key> bot=true seeds=3` (release `gtf`), and add a
`story_<key>` test to `tests/missions.rs` (ignored: long).

### A vehicle
A table in `vehicles.toml`: the engine's `VehicleDef` (mass, power, grip, suspension, wheel
geometry, doors, seats, lights, siren) plus `name`, `colors`, `traffic_weight` and `sound`. Its
body and wheels are props in `assets/props/lib/gtf_vehicles/` (text JSON parts; the `assets`
tools find exemplars). Check: `gtf vehicle_feel key=<key>` (0-100, top speed, braking, skidpad,
handbrake slide, slope and jump cases on the dev district's pad) and a capture.

### A weapon
A `[[weapon]]` in `weapons.toml` (wheel order; `icon` is the wheel slot). Guns are hitscan with
spread, bloom and recoil; melee weapons play `combo` clips and a `heavy` one on a held ○. The bat
shows how a weapon can share a slot (`combat::BAT`, `Freeway::arm_index`). Sell it in
`shops.toml`, give enemies it in `enemies.toml`. Check: `cargo test -p gtf_game --test combat`
and `--lib` (`the_weapons_match_the_wheel`).

### A pedestrian archetype
A table in `peds.toml`: palettes, proportions, a walk style clip and idles, pace, zones and hours,
behaviour weights, and `wear` (hair, beards, headwear, bags...). Check: `gtf peds` (counts and
what they are doing near the player), `lineup <archetype>` in the REPL with a `capture`, and
`gtf soak mode=peds seconds=600`.

### A radio station
A `[[station]]` in `radio.toml`: name, frequency, `genre` (synthwave, boombap, surf, cumbia,
smooth, or talk), seed, slot length, and the words its song titles and artists are made from;
talk stations have a `[talk.<id>]` program (headlines, ads, call-ins). Check: `cargo test -p
gtf_game --test radio`, and `audio_capture` with `radio seek` to listen offline (tempo and band
measurements in `PROGRESS.md`, M7 Radio).

### An activity
Stunt jumps, Lost Shields, vigilante and random events are tables in `activities.toml`; races
and hold-ups are missions with `activity = true`. Check: `play bot=true seeds=3` for mission
activities; the tests in `tests/activities.rs` and `tests/activities_street.rs` for the others.

### An attract vignette
A `[[vignette]]` in `attract.toml`: where it starts (`at` anchor, `vehicle`), `heat`, `cmds`, the
bot that plays it (cruise, escape, stunt, switch, stroll, fight, mission), `seconds`, its shot list
and its caption. Check: `attract play <id>` in the REPL with captures; `cargo test -p gtf_game
--test attract`; `gtf soak mode=attract seconds=1800` (stalls 0, every vignette twice, flat
entity counts and memory).

### A reference view
A `[[view]]` in `views.toml`: eye, target, hour and weather. Check: `gtf perf view=<name>` (the
§6.9 budgets) and `capture scene=freeway/view/<name> hud=false`.

## Verification commands
```sh
export CARGO_BUILD_JOBS=4
cargo build --release -p gtf_app --bin gtf && alias gtf=./target/release/gtf
cargo test -p gtf_game                      # the game's suites (long bot runs are #[ignore])
cargo test -p gtf_game --test missions -- --ignored   # every story mission on 3 seeds
gtf perf view=all                           # budgets at the reference views
gtf soak mode=traffic|peds|roam|attract seconds=...
printf 'load scene=freeway/roam\ncapture hud=true out=out/a.png\n' | gtf repl
scripts/build-gtf-web.sh && node scripts/web-check.mjs dist/GrandTheftFreeway-BX.html ...
scripts/build-windows.sh                    # GrandTheftFreeway-BX.exe and its zip
```
Look at every capture you take; the browser runs on SwiftShader here at well under a frame a
second, so browser checks wait on log lines (`frontend: <page>`, `attract: ...`, `perf: ...`).

## Where the engine extensions live
| Extension (PLAN §6.5) | Files |
|---|---|
| E1 game crates outside `pav_core` | `pav_core/src/games/mod.rs` (`register`, `all`), `pav_tools/src/cli.rs` (`main_with`), `pav_app/src/lib.rs` (`run`, `AppConfig`) |
| E2 `InputFrame` v2 | `pav_core/src/input.rs` (`PadState`, `pad`), `pav_app/src/input.rs` |
| E3 game hook v2 | `pav_core/src/games/mod.rs` (`time_scale`, `world_paused`, `vehicles`, `present`) |
| E4 presentation as data | `pav_core/src/present.rs` |
| E5 the 10-foot UI kit | `crates/pav_ui/` |
| E6/E7 view and renderer | `pav_view/src/play.rs`, `pav_view/src/vehicles.rs`, `pav_render/src/` (sky, culling, spots, window kernel) |
| E8 audio | `pav_audio/src/` (`bank.rs`, `loops.rs`, `music/`, `voice.rs`) |
| E9 vehicles v2 | `pav_core/src/vehicle.rs` |
| E10/E14 the city and regions | `crates/pav_world/`, `pav_core/src/world.rs` (region provider) |
| E11 odds and ends | `pav_core/src/sim.rs` (clips, raycasts, `set_solid`), `character.rs` |
| E12 tools | `pav_tools/src/` (`capture hud=`, `ui`, `play cmd=`, `citymap`, `lanes`) |
| E13 the hook-game shell | `pav_app/src/` (`app.rs`, `play.rs`, `quality.rs`, `touch.rs`, `settings.rs`) |

Engine commits carry the `engine:` prefix (`git log --grep '^engine:'`) and stay free of game
code so they can go upstream as a series.

## Known issues and the stretch list
Open issues are kept in `PROGRESS.md` → Open issues. The stretch list (PLAN §13): motorbikes,
boats, planes and parachuting; a second district; clothing, barbers and tattoos; property; golf,
tennis and yoga; a first-person camera; the stock market; an in-game internet; riding the Big
Wheel; a replay editor; photo mode; controller remapping; co-op; voiced dialogue beyond babble.
