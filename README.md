# Grand Theft Freeway-BX

**Three crooks, one freeway, one perfect job.** An original spiritual successor to *Grand Theft
Auto V*: a third-person open-world crime caper in **Paloma Bay**, a sun-bleached coastal district of
the fictional city of San Paloma, built on the **Pavilion engine** (the Shardfall repository).
Steal any car, outrun five stars of police, switch between three protagonists in a sky-cam swoop,
and pull the Freeway Job — loud with a breach charge and a SWAT gauntlet, or smart with a fake
roadwork and a swap in a harbor warehouse. Leave it alone on the title screen and it plays itself.

## Features
- **Paloma Bay** (about 1 km²), generated from data: downtown towers and City Hall, the Sunset
  Row strip with its neon, the Los Mirados bungalows, Mirador Hills with the PALOMA sign and the
  Lookout, Palisade Beach and the pier with its turning Big Wheel, The Flats and the Ditch storm
  channel, the Harbor Gate docks with a freighter, and the elevated Route 404 with the Pretzel
  interchange and its broken construction ramp. 21 landmarks, interiors you can walk into, a day
  and night cycle and six weathers.
- **On foot:** walk, run, sprint on stamina, jump, vault and climb, swim, crouch, fists with a
  combo and a heavy hook, the bat, cover with peeking and blind fire, aim assist, five guns and
  grenades on a slow-motion weapon wheel.
- **Behind the wheel:** 15 vehicles from data (cars, a bus, an armoured truck, police cruisers
  and a flyable helicopter) with analog throttle and brakes, a handbrake for drifting, pitch and
  roll over hills and ramps, carjacking, locked cars, damage to fire, burst tyres, broken lamps,
  headlights, sirens and five procedural radio stations plus SPNN talk radio.
- **A living city:** traffic that follows lanes and lights, pulls over for sirens, swerves from
  near misses and gets out angry when you hit it; buses on their route; crowds that sit, eat,
  sunbathe, cheer, film you, fight back and call the police.
- **Heat:** five stars, witnesses who report you, police dispatched along the roads, roadblocks
  and spike strips, a helicopter with a searchlight, search zones with vision cones on the radar,
  a respray to lose them; Wasted and Busted.
- **The story:** three intro missions, three heist setups, the planning board and the Freeway
  Job in two approaches, with checkpoints, retries, skips after three fails, medals and replays;
  a street race, five stunt jumps, two hold-ups, vigilante, two random events and fifteen Lost
  Shields; a gun store, a body shop, a convenience store, safehouses and three save slots.
- **10-foot, controller-first UI:** PlayStation glyphs by default (Xbox and keyboard glyphs
  follow the last device), a HUD with a radar and GPS route, the pause menu with the map, the
  phone, the wheels and big type everywhere; an on-screen FPS readout you can hide.
- **Attract mode:** ten seconds idle on the title or menus and the game plays itself through
  seven vignettes (a golden-hour cruise, a police chase, a dock gunfight, a character switch, a
  stunt jump, city life, the heist getaway); any input returns at once. Phones and tablets run
  the attract loop only, with a settings gear.

## How to play
- **In a browser:** open `GrandTheftFreeway-BX.html` in Chrome or Edge (WebGPU). It runs from
  the file, offline; give it a few seconds to boot.
- **On Windows:** unzip `GrandTheftFreeway-BX-windows.zip` and run `GrandTheftFreeway-BX.exe`
  (a GPU with Vulkan or DirectX 12).
- A controller is recommended (DualSense, DualShock or Xbox); keyboard and mouse drive the same
  actions. From the title press any button, then **New Game** for the story or **Free Roam**.

### Controls
| Action | Controller (PlayStation) | Keyboard and mouse |
|---|---|---|
| Move / look | left stick / right stick | W A S D / mouse |
| Sprint (hold or tap) | ✕ | Shift |
| Jump, vault, climb | □ (running into a low wall vaults it) | Space |
| Enter or leave a vehicle (hold at speed: bail out) | △ | F |
| Melee (hold for a heavy hook) / reload | ○ | R |
| Aim / fire | L2 / R2 | right / left mouse button |
| Cover | R1 | Q |
| Weapon wheel (hold) | L1 | Tab |
| Crouch | L3 | C |
| Special ability | L3 + R3 | Caps Lock |
| Phone / switch character (hold) / interact | D-pad ↑ / ↓ / → | ↑ / Alt / E |
| Pause / map | Options / touchpad | Esc / M |
| Throttle / brake and reverse / handbrake | R2 / L2 / R1 | W / S / Space |
| Horn (police car: tap for the siren, wail then yelp) | L3 | H |
| Radio station | D-pad ← → (hold ← for the wheel) | , and . |
| Helicopter | R2 climbs, L2 descends, sticks fly and turn | W / S, A D, I K |

The three protagonists' abilities: Dez's **Slipstream** (driving slow-motion and grip), Mack's
**Deadeye** (on-foot bullet time) and Roxy's **Overdrive** (double damage, half taken).

## Building from source
Linux (Ubuntu 24.04) with Rust 1.98.1; `scripts/setup-linux.sh` installs the system packages.
- `cargo run --release -p gtf_app --bin grand_theft_freeway` starts the game.
- `scripts/build-gtf-web.sh` writes the single-file browser build to
  `dist/GrandTheftFreeway-BX.html` (needs the `wasm32-unknown-unknown` target and
  `wasm-bindgen-cli` 0.2.129).
- `scripts/build-windows.sh` cross-compiles the Windows `.exe` and zips it (mingw-w64).
- `cargo test -p gtf_game` runs the game's suites; the long bot runs are `#[ignore]`d (`cargo
  test -p gtf_game --test missions -- --ignored`).

**The `gtf` CLI** (`cargo run --release -p gtf_app --bin gtf -- <tool>` or `gtf repl`) is how
agents build and check the game without a display: `load scene=freeway/roam`, `step`, `input`
(controller semantics), `capture hud=true` (a PNG with the HUD), `filmstrip`, `ui` (the HUD as
JSON), `play bot=true seeds=3` (mission bots), `perf view=all` (budgets at the reference views),
`soak mode=traffic|peds|roam|attract`, `citymap`, `lanes`, `vehicle_feel` and `audio_capture`.

## Project layout
| Path | What it is |
|---|---|
| `crates/gtf_game` | the game's rules: a `Game` hook that describes what to draw and play as data |
| `crates/gtf_app` | the binaries `grand_theft_freeway` (the game) and `gtf` (the agent CLI and its tools) |
| `content/freeway/` | the game's data: the district, missions, vehicles, weapons, people, radio, attract vignettes |
| `crates/pav_*` | the Pavilion engine, vendored and extended (`docs/ENGINE_UPSTREAM.md`) |
| `assets/props/lib/gtf_*` | the game's props as text (cars, city furniture, weapons) |
| [`PLAN.md`](PLAN.md), [`PROGRESS.md`](PROGRESS.md) | the plan and the record of its execution |
| [`docs/gtf/HANDOFF.md`](docs/gtf/HANDOFF.md) | how to extend the game: architecture and how to add each kind of thing |
| [`docs/gtf/MISSIONS.md`](docs/gtf/MISSIONS.md) | the mission language |
| [`docs/gtf/ENGINE_ANALYSIS.md`](docs/gtf/ENGINE_ANALYSIS.md) | the engine evaluation |

## Credits
- **Engine:** Pavilion, from the Shardfall repository (https://github.com/michaelcrosato/shardfall).
- **Fonts:** Anton (The Anton Project Authors) and Barlow Semi Condensed (The Barlow Project
  Authors), under the SIL Open Font License 1.1 (licence files in `assets/fonts`).
- **Motion:** Quaternius' Universal Animation Library and Mesh2Motion (CC0 1.0); 100STYLE by Ian
  Mason, Sebastian Starke and Taku Komura (CC BY 4.0); the CMU Graphics Lab Motion Capture
  Database. The engine's store credits each clip.
- Everything else — the city, the vehicles, the people, the props, the sounds, the voices and
  the music — is generated by code and data in this repository.

*Grand Theft Freeway-BX* is an original work inspired by *Grand Theft Auto V*. It is not
affiliated with or endorsed by Rockstar Games or Take-Two Interactive; all names, places and
brands in it are fictional.

## Source information
- **Initial Date:** 2026-10-17 (the first prompt of the planning session; created 00:13 UTC).
- **SOURCE:** *Grand Theft Auto V* — Rockstar North / Rockstar Games, 2013 (PlayStation 3, Xbox 360;
  later PlayStation 4, Xbox One, PC, PlayStation 5, Xbox Series X|S).
- **GAME ENGINE:** Pavilion engine from the Shardfall repository,
  https://github.com/michaelcrosato/shardfall, pinned at commit
  `0098196140d279979b622ee4373cf498d95fc9e1` ("Co-op plan on hold (#74)", 2026-10-16), workspace
  version 0.1.0: Rust 1.98.1, wgpu 30 (WebGPU in the browser, Vulkan/DX12 native), rapier3d 0.36,
  egui 0.36, glam 0.33, cpal 0.18, gilrs 0.11. Handoff route: `START_HERE.md` →
  `docs/MAKE_A_GAME.md`, extended per PLAN.md §6.
- **Environment:** Claude Code (container builds 2.1.302 for planning, 2.1.310 for building) in
  Anthropic-managed cloud sessions started from the Claude desktop app. Linux: Ubuntu 24.04.5 LTS,
  4 vCPU, 15 GB RAM, no GPU — headless captures render on Mesa lavapipe (software Vulkan); browser
  checks run in headless Chromium via Playwright (SwiftShader WebGPU); native window checks run
  under Xvfb; the Windows build is cross-compiled with mingw-w64 and smoke-tested under Wine.
  Player targets: Windows on an NVIDIA RTX 3060 Ti, and WebGPU browsers (Chrome, Edge).
- **Model and Effort Level:** AI lab: Anthropic (Claude). Planning and building both ran at
  reasoning effort **max** (confirmed from the sessions' metadata). The exact model is stated in
  the delivery chat report, not here: this environment keeps model identifiers out of repository
  files.

The engine evaluation — what was used, what wasn't and the one change we would make — is in
[`docs/gtf/ENGINE_ANALYSIS.md`](docs/gtf/ENGINE_ANALYSIS.md).
