# HANDOFF — Grand Theft Freeway-BX, session 2 (2026-10-18)

For the next implementing session (or this one after a compaction). Read `CLAUDE.md` (it imports
`docs/DOCTRINE.md`), then this page, then `PROGRESS.md` → "Now", then `PLAN.md` §0–§4 and §10
once and afterwards only the sections the current milestone names. `PLAN.md` stays the source of
truth; this page says where things stand and what to do next.

## Where things stand
- **Done:** M0 bootstrap, M1 platform, M2 city, M3 player and vehicles, M4 population, M5 combat
  and heat (acceptance evidence in `PROGRESS.md` → Milestones and Log).
- **Engine extensions landed:** E1–E9, E11 (character low detail), E12 tools, E13 app shell,
  E14 regions, plus hidden blocks (`block_flags::HIDDEN`, the map's invisible wall) and the
  incremental mesh tally in `pav_view`. E10 (the mission DSL) comes with M6.
- **Health:** pav_core 305, pav_view 44, pav_world 11, gtf_game 74 (+6 ignored reports) green;
  clippy `-D warnings` clean on the touched crates; `gtf perf view=all` (release) within every
  budget with traffic and crowds; soaks as in `PROGRESS.md` → Now.

## Next, in value order (PLAN §2)
1. **M6 structure (D6 heist path first, D7):** the mission DSL (`PLAN.md` §6.8.6, E10) with its
   validator, runtime and per-step bot policies; markers, intro card, objectives, subtitles,
   cutscene shots with letterbox and skip, checkpoints, fail card, retry, skip after 3 fails,
   results with medals; protagonists (profiles, switch wheel, sky-cam, abilities, life spots);
   the phone apps (§5.6.9); economy and shops; save/load (3 slots + autosave) and settings
   persistence; two test missions in `freeway/dev`.
2. **M7 content:** final district dressing and interiors, missions m1–m3, s1–s3, h0, h1 Loud and
   Smart, activities, radio stations, completion.
3. **M8 frontend and attract:** title, menus, loading cards, About with the Source Information,
   the attract director (7 vignettes, 10 s idle, any input returns), touch = attract only.
4. **M9 polish, M10 release:** the deliverables D14–D15 (README, `docs/gtf/ENGINE_ANALYSIS.md`,
   `docs/gtf/HANDOFF.md`, the single-file HTML, the Windows `.exe` zipped, the final report).

## Known issues
- Roam soak seed 1: one car stuck at a dead-end keyhole turnaround in 900 s (limit 0).
- The evasion bot from a standstill escapes 2 of 8 seeds (rolling starts 5 of 8).
- Dead bodies hold a half-crouched last pose (the death clips play in place).
- Occupant busts are drawn behind opaque glass; headlight beams are one point light.
- Sun through thin cloud: the patch is saved at `docs/gtf/patches/cloud_through.patch`.
- `citymap`, `lanes`, `soak`, `traffic`, `peds` live in `gtf_app/src/tools.rs` (they read the
  game's city).

## Working notes
- **Builds:** 4 vCPU, fixed disk allowance; `CARGO_BUILD_JOBS=4`. Soaks and perf need a release
  `gtf` (`cargo build --release -p gtf_app --bin gtf`). Prune duplicate artefacts in
  `target/*/deps` when the disk runs low.
- **Looking at things:** `gtf capture … hud=true`, `filmstrip`, `perf view=all split=true`,
  `soak mode=traffic|peds|roam seconds=… seed=… [wanted=3 shots=true]`, `citymap`, `lanes`,
  `audio_capture`. Commands typed into a run: `input cmd="time 21:30"`, `weather rain`,
  `wanted 3`, `autopilot escape`, `weapon pistol`. Reference views: `freeway/view/<name>`.
- **Web:** `scripts/build-gtf-web.sh`, then `node scripts/web-check.mjs
  dist/GrandTheftFreeway-BX.html --query "scene=freeway/dev" --expect "boot complete"
  --shot-timeout 200000` (SwiftShader is slow).
- **Determinism:** game state lives in `Freeway` (serde) or entities; randomness only from
  `sim.state.rng`; snapshots, rewind and replay are exact (tests cover them with traffic and
  pedestrians).
- **Helpers:** run them in git worktrees (`.claude/worktrees/…`), each with its own `target/`;
  merge with `--no-ff`; never hard-link one tree's `target` into another.

## Source Information (also in `README.md`)
Initial date 2026-10-17; SOURCE *Grand Theft Auto V*; GAME ENGINE Pavilion (Shardfall,
https://github.com/michaelcrosato/shardfall, commit `0098196`, v0.1.0); environment Claude Code
in an Anthropic cloud session on Linux. The model and effort level are stated in the session's
chat reports.
