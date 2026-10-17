# CLAUDE.md — Grand Theft Freeway-BX

This repository builds **Grand Theft Freeway-BX**, an original spiritual successor to *Grand Theft
Auto V*, on the Pavilion engine (vendored from https://github.com/michaelcrosato/shardfall at the
commit pinned in `scripts/vendor-engine.sh`).

## Start here, every session
1. `PROGRESS.md` — where things stand and what is next. Read it first (also after a context
   compaction or a resumed session).
2. `PLAN.md` — the source of truth: scope, design, architecture, engine extensions, milestones and
   the Definition of Done (§2). Read §0–§4 and §10 once; afterwards read the sections the current
   milestone names, not the whole file.
3. Until milestone M0 runs, the repository holds only these docs and `scripts/vendor-engine.sh`.
   After M0: also follow `docs/DOCTRINE.md` (engine principles — add `@docs/DOCTRINE.md` to this
   file in M0), the "Key rules" of `AGENTS.md`, and `docs/MAKE_A_GAME.md` (the game hook).

## Rules (PLAN.md §10 is the full version)
- **Git:** work on the session's designated branch; push often (cloud containers are temporary);
  keep the existing draft PR updated; never merge it, never force-push. `AGENTS.md`'s
  "merge into main" convention does not apply in this repository.
- **Commits:** prefixes `engine:` (engine crates and engine docs only — keep them upstreamable),
  `game:`, `content:`, `docs:`, `build:`. Apart from the session's prescribed attribution lines, no
  model identifiers in commits, PRs, code or docs (state the model in chat only).
- **Assets:** no binary source assets except fonts (OFL, licence files kept). Props, materials,
  animation, sound and music are text or code. Never commit images or downloaded models.
- **Determinism:** gameplay state lives in the game struct or entities; randomness only from
  `sim.state.rng`; never the clock; no `HashMap`/`HashSet` iteration reaching state.
- **Layering:** `pav_core`, `pav_world` and `gtf_game` have no graphics, window or audio
  dependencies and stay wasm-safe (no threads, blocking calls or file access except native hot
  reload). The game describes what to show and play as data (PLAN.md §6.5 E4).
- **Checks:** before each push `cargo fmt --all`, `cargo clippy -p <touched> --all-targets -- -D
  warnings`, the touched crates' tests, `scripts/check-wgsl.sh` after shader edits. At milestone
  ends the full workspace tests, wasm clippy, the milestone's acceptance commands.
- **Verify with tools, then look:** `gtf capture ... hud=true`, `filmstrip`, `play bot=true`,
  `soak`, `perf`, `ui`; open every capture you take. Record evidence in `PROGRESS.md`.
- **Decide** what is not genuinely the user's call and log it in `PROGRESS.md` → Decisions.
  Escalate only what PLAN.md §10.6 lists.
- **Keep upstream behaviour working:** Shardfall, the pavilion demo and the starter game keep
  their tests green.

## Environment notes (from planning, 2026-10-17)
- 4 vCPU, ~15 GB RAM, no GPU, fixed disk allowance. Use `CARGO_BUILD_JOBS=4`. Captures render on
  lavapipe; the web build runs in headless Chromium with SwiftShader (slow: 0.5–2 frames/s).
- Toolchain 1.98.1 with `wasm32-unknown-unknown` and `x86_64-pc-windows-gnu` targets and
  `wasm-bindgen` 0.2.129 were present at planning time; re-check after a container reset.
- Playwright is installed globally (`/opt/node22/lib/node_modules/playwright`), Chromium under
  `/opt/pw-browsers`; Xvfb, `xwd` and ImageMagick are available for native window screenshots.
