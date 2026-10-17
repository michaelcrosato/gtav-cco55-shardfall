# Grand Theft Freeway-BX

**Three crooks, one freeway, one perfect job.** An original spiritual successor to *Grand Theft
Auto V*: a third-person open-world crime caper in **Paloma Bay**, a sun-bleached coastal district of
the fictional city of San Paloma, built on the **Pavilion engine** (the Shardfall repository).

> **Status: in development.** Milestones M0–M2 are done: the engine is vendored and extended, and
> Paloma Bay is generated, streamed, lit and dressed, with time of day and weather. M3–M5 are
> partly done: on-foot play, cameras, vehicles in the engine, pedestrians, the radio and on-foot
> combat. The game is built by executing [`PLAN.md`](PLAN.md). Progress is tracked in
> [`PROGRESS.md`](PROGRESS.md), and the next steps are in [`HANDOFF.md`](HANDOFF.md).

## Build and run
- `cargo run --release -p gtf_app --bin grand_theft_freeway` starts the game (a WebGPU-capable GPU
  through Vulkan, Metal or DX12).
- `cargo run -p gtf_app --bin gtf -- repl` opens the headless agent tools: `load
  scene=freeway/roam`, `capture`, `perf view=all`, `citymap`, `lanes`, `audio_capture`.
- `scripts/build-gtf-web.sh` writes the single-file browser build to
  `dist/GrandTheftFreeway-BX.html`.

## What the vertical slice will be
- **One polished district** (~1 km²): downtown towers, the Sunset Row strip, hillside bungalows and
  the PALOMA sign, a beach with a pier and a Ferris wheel, industrial flats with a concrete storm
  channel, docks, and the elevated Route 404 freeway with its "Pretzel" interchange.
- **A living city:** traffic that follows lanes and signals, pedestrians with routines and
  reactions, day/night and weather, procedural radio stations that never repeat.
- **On foot and behind the wheel:** sprint, climb, cover, aim and shoot; steal any car, drive with
  analog triggers and a handbrake, crash, burn, bail out.
- **Heat:** a five-star wanted system with witnesses, police dispatched along the road network,
  helicopters with searchlights, search zones and vision cones, and real escapes.
- **The Freeway Job:** three protagonists with special abilities and sky-cam switching, three heist
  setups, a planning board with Loud and Smart approaches, plus races, stunt jumps, hold-ups,
  vigilante jobs, random events and collectibles.
- **10-foot, controller-first UI** (PlayStation glyphs by default; keyboard and mouse map onto the
  same model) and an on-screen FPS readout.
- **Attract mode:** after 10 seconds idle on the title or menus, an AI plays the game in a kiosk
  loop until any input; phones and tablets run the attract loop only.

## Repository
| File | What it is |
|---|---|
| [`PLAN.md`](PLAN.md) | The implementation plan: design, architecture, engine extensions, milestones M0–M10, Definition of Done |
| [`PROGRESS.md`](PROGRESS.md) | Milestone checklist, Definition of Done evidence, decisions, open issues |
| [`HANDOFF.md`](HANDOFF.md) | Where the work stands, what to do next, known issues, working notes |
| [`CLAUDE.md`](CLAUDE.md) | Instructions for the implementing agent |
| [`scripts/vendor-engine.sh`](scripts/vendor-engine.sh) | Vendors the engine at the pinned commit (milestone M0; tested in planning) |

## Source information
- **Initial Date:** 2026-10-17
- **SOURCE:** *Grand Theft Auto V* — Rockstar North / Rockstar Games, 2013 (PlayStation 3, Xbox 360;
  later PlayStation 4, Xbox One, PC, PlayStation 5, Xbox Series X|S).
- **GAME ENGINE:** Pavilion engine (Shardfall repository), https://github.com/michaelcrosato/shardfall,
  commit `0098196140d279979b622ee4373cf498d95fc9e1` ("Co-op plan on hold (#74)", 2026-10-16),
  workspace version 0.1.0 — Rust 1.98.1, wgpu 30, rapier3d 0.36, egui 0.36, glam 0.33, cpal 0.18,
  gilrs 0.11. Handoff route: `START_HERE.md` → `docs/MAKE_A_GAME.md`.
- **Environment:** Claude Code in an Anthropic-managed cloud session started from the Claude desktop
  app; Linux (Ubuntu 24.04.5 LTS), 4 vCPU, 15 GB RAM, no GPU (Mesa lavapipe software Vulkan for
  headless captures; headless Chromium with SwiftShader WebGPU for browser checks; Xvfb for native
  window checks). Player targets: Windows on an NVIDIA RTX 3060 Ti, and WebGPU browsers.
- **Model and Effort Level:** Anthropic (Claude). Planning ran at reasoning effort **max**;
  implementation is handed off at **Extra** effort. The exact model is stated in the session's chat
  reports rather than here, because this environment keeps model identifiers out of repository files.

The engine evaluation (what was used, what wasn't, the one change) will live separately in
`docs/gtf/ENGINE_ANALYSIS.md` once the game is built.
