# HANDOFF — Grand Theft Freeway-BX, end of session 2 (2026-10-18; environment checked in sessions 3 and 4, 2026-10-19 and 2026-10-24)

For the next implementing session (or this one after a compaction). Read `CLAUDE.md` (it imports
`docs/DOCTRINE.md`), then this page, then `PROGRESS.md` → "Now", then `PLAN.md` §0–§4 and §10
once and afterwards only the sections the current milestone names. `PLAN.md` stays the source of
truth; this page says where things stand and what to do next.

## Where things stand
- **Done:** M0 bootstrap, M1 platform, M2 city, M3 player and vehicles, M4 population, M5 combat
  and heat, M6 structure (acceptance evidence in `PROGRESS.md` → Milestones and Log). Session 2
  ended at the user's request after M6; the branch is pushed and the tree is clean.
- **Engine extensions landed:** E1–E9, E10 (`pav_world`, with M2), E11 (character low detail),
  E12 tools, E13 app shell, E14 regions, plus hidden blocks (`block_flags::HIDDEN`, the map's
  invisible wall), the incremental mesh tally in `pav_view`, districts' named `[[spot]]`s and
  `VehicleInput.grip` (M6). Engine commits stay upstreamable (`engine:` prefix).
- **Health:** pav_core 305, pav_view 44, pav_world 11, gtf_game 98 (+8 ignored reports) green;
  clippy `-D warnings` clean on the touched crates; `gtf perf view=all` (release) within every budget
  with traffic and crowds; soaks as in `PROGRESS.md` → Now.
- **Sessions 3 and 4 (2026-10-19, 2026-10-24)** only checked the environment and took stock
  (no game code changed). Everything is installed. Wasm clippy is still not clean (below). On
  the session 4 container every reference view is within budget again, but downtown's margin is
  thin. The shardfall MCP server timed out at startup; `.mcp.json` now runs `gtf mcp`. The
  gap inventory of everything M7–M10 still needs is in `PROGRESS.md` → Remaining work.

## The M6 systems (where to look)
- **Missions** — `crates/gtf_game/src/missions/`: `mod.rs` (the DSL types, the validator with
  file:line errors, the mission list `FILES`), `run.rs` (the runtime: steps, actors,
  checkpoints, fail/retry/skip, medals, results, markers, cutscene camera), `bot.rs` (the bot
  that plays any mission through its input frame). **`docs/gtf/MISSIONS.md` is the reference for
  writing missions** (anchors, spawns, behaviours, the 25 step kinds with their bot policies,
  actions, fails, checkpoints, bot-friendly layout). The worked examples are
  `content/freeway/missions/dev_test.toml` (driving kinds) and `dev_test2.toml` (on foot and
  story).
- **Places** — `anchor.rs` (`spot:`, `landmark:`, `at:`, `lane_near:`, `player`, `actor:`);
  spots live in `content/freeway/district.toml` / `dev_district.toml` (`[[spot]]`, cell
  coordinates: world = cell × 16 − 128 in the dev district; `yaw` is degrees from north and +Z
  is south, so 180 faces +Z).
- **Protagonists** — `protagonists.rs` + `content/freeway/protagonists.toml` (Dez, Mack, Roxy:
  cash, weapons, stats, looks, homes, life spots; the abilities and how their meters fill).
- **Phone** — `phone.rs` (nine apps, texts, the camera and photo targets). **Shops** —
  `shops.rs` + `content/freeway/shops.toml` (gun store, Paloma Customs, Gas-N-Grab, the
  safehouse save/load menus, the replay list). **Saves** — `progress.rs` (`SaveData`, slots,
  autosave, Continue).
- **Commands typed into a run** (also what the bot uses): `mission start|replay|quit|fail|choose
  <x>`, `switch <who>`, `meter <0..1>`, `give <weapon> [ammo]`, `cash <n>`, `save <slot>`,
  `load <slot>`, `pilot_goal x z [desire]|off`, `phone <app>|home|close`, `menu
  gun|mods|store|save|load|replay`.

## Next, in value order (PLAN §2)
`PROGRESS.md` → **Remaining work** is the full checklist (a gap inventory of the code against
PLAN, 2026-10-24). In short:
0. **D1:** fix the four wasm-clippy lints (one `engine:` commit, below).
1. **M7 content, heist path first:** missions `m1`–`m3`, `s1`–`s3`, the `h0` board and `h1`
   Loud and Smart (§5.7) in the DSL. Add each to `FILES` in `missions/mod.rs` and validate it
   (`cargo test -p gtf_game --test missions every_mission_validates`). Then give it a bot test on
   3 seeds like `the_bot_finishes_both_test_missions_on_three_seeds` in `tests/missions.rs` (both
   heist approaches; the `choose` step takes `mission choose <id>`). The `choose` step already
   shows a `mission/board` Cards screen; `h0` adds the cut. Then the activities (none exist yet),
   the radio (per-genre patches, SPNN talk), completion and the stats screen, then the
   landmarks that are still placeholders or missing (the Big Wheel among them) and the interiors
   (none yet).
2. **M8 frontend and attract:** the hills title, intro and loading cards, the missing settings
   and the toggles nothing reads, About with the full Source Information, and the attract
   director (7 vignettes, 10 s idle, any input returns, a 30-minute soak). Today a 10 s idle
   only logs "attract would start". Touch = attract only, with the gear overlay.
3. **M9 polish:** the gameplay, audio, visual and performance gaps in Remaining work
   (`?perf=1`, quality presets, camera shake, reverb, dialogue ducking, tyres, headlights) and
   the open issues.
4. **M10 release:** the deliverables D14–D15. Write `README.md`, `docs/gtf/ENGINE_ANALYSIS.md`
   and `docs/gtf/HANDOFF.md`. Build the single-file HTML with its header comment and the zipped
   Windows `.exe`. Write the final report.

## Known issues
- Dense traffic can lock a junction box (missions thin traffic to 0.35 around drive legs).
- Roam soak seed 1: one car stuck at a dead-end keyhole turnaround in 900 s (limit 0).
- The evasion bot from a standstill escapes 2 of 8 seeds (rolling starts 5 of 8).
- `dev_test` takes the bot ~395 s of game time; keep M7's drive legs short.
- Dead bodies hold a half-crouched last pose (the death clips play in place).
- Occupant busts are drawn behind opaque glass; headlight beams are one point light.
- Sun through thin cloud: the patch is saved at `docs/gtf/patches/cloud_through.patch`.
- `citymap`, `lanes`, `soak`, `traffic`, `peds` live in `gtf_app/src/tools.rs` (they read the
  game's city).
- **Wasm clippy (D1) fails** on four wasm-only lints in vendored engine code: `drop_non_drop` in
  `pav_tools` (`animation_tools.rs:287`, `asset_tools.rs:95`, `creature_tools.rs:662`) and
  `default_constructed_unit_structs` in `pav_app/src/asset_watch.rs:48`. Session 4 got the same
  three `pav_tools` errors. Fix them in one `engine:` commit (details in `PROGRESS.md` → Open
  issues).
- **Perf depends on the container:** downtown built a frame in 2.8-3.2 ms in session 2,
  4.7-5.4 ms in session 3 (budget 4) and 3.6-3.9 ms in session 4. Ticks took 1.4-2.2,
  2.8-3.6 and 2.0-2.1 ms (budget 3). Judge changes by A/B against a baseline build on the
  same machine. M9 must leave margin.
- **Earlier evidence is not on disk:** `out/` is ignored by Git and containers reset, so the
  captures and soak JSON cited for M0-M6 are gone. The final verification (M10) re-runs and
  looks at them.

## Working notes
- **Builds:** 4 vCPU, fixed disk allowance; `CARGO_BUILD_JOBS=4`. Soaks, perf and quick looks
  need a release `gtf` (`cargo build --release -p gtf_app --bin gtf`, ~45 s incremental). Prune
  duplicate artefacts in `target/*/deps` when the disk runs low.
- **Looking at things:** script the REPL — `printf 'load scene=freeway/mission/dev_test2\nstep
  ticks=400\ncapture width=1280 height=720 hud=true out=/path/a.png\n' | ./target/release/gtf
  repl` — then open every image. `ui` prints the UI model as JSON (overlay, prompts, screens);
  `play` the game's status; `play bot=true ticks=40000` runs the mission bot to the outcome.
  Also `filmstrip`, `perf view=all split=true`, `soak mode=traffic|peds|roam seconds=… seed=…`,
  `citymap`, `lanes`, `audio_capture`. Reference views: `freeway/view/<name>`.
- **Debugging a mission bot:** `GTF_SCENE=freeway/mission/<key> GTF_STEP=<n> GTF_SECS=<s> cargo
  test -p gtf_game --test missions bot_trace -- --ignored --nocapture` prints the bot's plan,
  position and inputs step by step; `bot_progress_report` (ignored) shows how far each seed got.
- **Web:** `scripts/build-gtf-web.sh`, then `node scripts/web-check.mjs
  dist/GrandTheftFreeway-BX.html --query "scene=freeway/dev" --expect "boot complete"
  --shot-timeout 200000` (SwiftShader is slow).
- **Determinism:** game state lives in `Freeway` (serde) or entities; randomness only from
  `sim.state.rng`; snapshots, rewind and replay are exact (tests cover them with traffic,
  pedestrians and missions).
- **Helpers:** run them in git worktrees (`.claude/worktrees/…`), each with its own `target/`;
  merge with `--no-ff`; never hard-link one tree's `target` into another, and never point a
  worktree's `CARGO_TARGET_DIR` at the main tree's. Cargo hashes path crates relative to the
  workspace root, so the two trees share artifacts, and `pav_core`'s build script bakes in its
  `CARGO_MANIFEST_DIR`. If that happens, run `cargo clean --release -p` for the ten workspace
  crates, then rebuild.
- **Startup:** the shardfall MCP server (`.mcp.json`) runs `cargo run -p gtf_app --bin gtf --
  mcp`: every engine tool plus the game's, as `mcp__shardfall__*`. The hook's combined `pav` +
  `gtf` build leaves it fresh, so it starts in under a second. The old `pav` command resolved
  features differently, rebuilt the dependency graph and timed out after 10 minutes in session 4.
  If the server still fails to connect, use the `gtf` CLI; it has the same tools.

## The `/goal` prompt to finish (M7–M10)
`PLAN.md` Appendix A is the original prompt for the whole run (M0→M10). To resume from here:
```
/goal Finish Grand Theft Freeway-BX: PLAN.md milestones M7→M10 on this branch (M0–M6 are done).
Read HANDOFF.md, then PROGRESS.md (Now, Remaining work, Open issues); PLAN.md is the source of
truth. First fix the D1 wasm-clippy lints (one engine: commit). Then clear PROGRESS.md →
Remaining work in order: M7 content (heist path first; every mission and activity bot-tested on
3 seeds), M8 frontend and attract, M9 polish with perf margin, M10 release. Follow CLAUDE.md:
checks green before each push, look at every capture, push often, keep draft PR #2 current,
record evidence and decisions in PROGRESS.md. Done = every D1–D16 row in PROGRESS.md checked
with evidence from this run; dist/GrandTheftFreeway-BX.html and the zipped Windows .exe sent to
me; README.md, docs/gtf/ENGINE_ANALYSIS.md and docs/gtf/HANDOFF.md written; final report per
PLAN §12.4.
```

## Source Information (also in `README.md`)
Initial date 2026-10-17; SOURCE *Grand Theft Auto V*; GAME ENGINE Pavilion (Shardfall,
https://github.com/michaelcrosato/shardfall, commit `0098196`, v0.1.0); environment Claude Code
in an Anthropic cloud session on Linux. The model and effort level are stated in the session's
chat reports.
