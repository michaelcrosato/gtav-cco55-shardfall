# PROGRESS — Grand Theft Freeway-BX

The implementation tracker for [`PLAN.md`](PLAN.md). Update it as work lands: tick tasks, fill the
Definition of Done evidence, log decisions and open issues. Keep "Now" current — it is what a
resumed or compacted session reads first.

## Now
- **Phase:** planning complete (2026-10-17). **Next:** M0 — run `scripts/vendor-engine.sh`, then
  E1 (PLAN.md §9 M0, §6.5 E1).
- **Branch / PR:** the session's designated branch; the draft PR opened with the plan.

## Milestones (PLAN.md §9 has the full task lists and acceptance criteria)
- [ ] **M0 Bootstrap** — engine vendored; baseline tests; E1 registry, `pav_tools::cli`, `pav_app`
      lib split, `gtf_game` + `gtf_app`; `freeway` scene in every target; web boot check.
- [ ] **M1 Platform** — E2 input v2, E3 hook v2, E4 presentation data, E5 `pav_ui` + headless HUD
      capture, E6 camera intent + draw list, E13 app shell, E8 part 1 (bank + loops), UI gallery,
      FPS readout.
- [ ] **M2 City** — E14 world regions first (region provider, static meshes, speed-aware streaming,
      far LOD, unload policies); `pav_world` generator + graphs; city props/materials; E7 sky, time of
      day + weather, window kernel, CPU region culling, merged statics; district + dev district +
      views; `citymap`, `lanes`, `perf`.
- [ ] **M3 Player and vehicles** — on-foot controller, camera modes, E9 vehicles v2 (pitch/roll
      freed with stabilization), enter/exit, carjack, damage, helicopter, vehicle audio, GPS ribbon,
      radio skeleton, `vehicle_feel` incl. slopes and jumps.
- [ ] **M4 Population** — traffic, pedestrians, population manager, LOD, buses, parked cars,
      ambience, soaks.
- [ ] **M5 Combat and heat** — weapons, cover, vitals, enemy AI, wanted system, police AI, air unit,
      busted/wasted.
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
