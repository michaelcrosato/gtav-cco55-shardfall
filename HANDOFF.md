# HANDOFF — Grand Theft Freeway-BX, end of the run (2026-10-25; M0-M10 done)

For whoever picks this up next (a session after a compaction, a maintainer, an engine agent).
Read `CLAUDE.md` (it imports `docs/DOCTRINE.md`), then this page, then `PROGRESS.md` → "Now".
`PLAN.md` is the source of truth for scope and design; `PROGRESS.md` holds the evidence, the
decisions and the open issues; `docs/gtf/HANDOFF.md` is the map for growing the game.

## Where things stand
- **Every milestone is done** (M0 bootstrap through M10 release) and every Definition of Done row
  D1-D16 in `PROGRESS.md` is checked with evidence from this run.
- **The game:** Paloma Bay, three protagonists with abilities and the switch, traffic and crowds,
  five stars of heat, the eight story missions with the heist both ways, every activity, the
  radio, the frontend with settings and About, the attract loop (touch screens play it only),
  saves and completion.
- **Deliverables:** `dist/GrandTheftFreeway-BX.html` (committed once, the single-file browser
  build with the Source Information block in its header comment) and
  `dist/GrandTheftFreeway-BX-windows.zip` (`scripts/build-windows.sh`; not committed). Both were
  sent to the user.
- **Docs:** `README.md` (the game, how to play, controls, building, Source Information),
  `docs/gtf/ENGINE_ANALYSIS.md` (what the engine gave us, what we didn't use and the one change,
  the systems table), `docs/gtf/HANDOFF.md` (architecture, how to add each kind of thing),
  `docs/gtf/MISSIONS.md` (the mission language), and the engine's own `docs/REFERENCE.md`.
- **Engine extensions:** E1-E14 (PLAN §6.5) plus the later ones logged in `PROGRESS.md` →
  Decisions; every one is an `engine:` commit, free of game code (`git log --grep '^engine:'`).

## How to check it (release `gtf`)
```sh
export CARGO_BUILD_JOBS=4
cargo build --release --locked -p gtf_app && alias gtf=./target/release/gtf
cargo test --release -p gtf_game              # the game's suites; long bot runs are #[ignore]d
GTF_SEEDS=5 cargo test --release -p gtf_game --test missions -- --ignored story_ activity_ --test-threads 3
gtf perf view=all split=true                  # every reference view within its budgets
gtf soak mode=traffic|peds|roam|attract seconds=600 seed=1
printf 'load scene=freeway/dev\ncapture hud=true out=out/dev.png\n' | gtf repl
```
`docs/gtf/HANDOFF.md` → Verification commands has the rest (the web build and its browser
checks, the Windows build).

## Known issues
`PROGRESS.md` → Open issues is the full list. The ones a player might meet:
- Dense traffic can still lock a junction box (missions thin the traffic around their drives).
- Bot chases are chaotic: any change to traffic, the police or the bots reshuffles which seeds
  get away (the 3★ evasion bot: rolling starts 3 of 8, standing 7 of 8; it can rock against a
  car stopped in a one-lane street). Re-run every mission and activity on five seeds after
  touching them (`GTF_SEEDS=5`).
- After a fall beside a building the death camera can sit against the wall.
- No heat haze; broken glass throws dust, not shards; helicopter canopies are opaque.
- The browser build draws at well under a frame a second on SwiftShader (the cloud's software
  WebGPU); a real GPU is unaffected. Frame rate on the target GPU (an RTX 3060 Ti) is for the
  user to read off the on-screen readout: this cloud has no GPU.
- The DualSense mapping through gilrs on Windows is a user check.

## Working notes
- **Builds:** 4 vCPU, a fixed disk allowance: `CARGO_BUILD_JOBS=4`; test binaries are about
  120 MB each, so run the workspace's tests crate by crate and delete each suite's binary after
  its run (`PROGRESS.md` → Log has the recipe).
- **Looking at things:** script the REPL (`printf 'load scene=...\nstep ticks=...\ncapture
  hud=true out=...\n' | gtf repl`) and open every image; `ui` prints the UI model as JSON.
- **Debugging a mission bot:** `bot_trace` in `tests/missions.rs` (ignored) prints the bot's
  plan and inputs step by step; `status` shows `traffic.pilot`, `evade` and `mbot`.
- **Determinism:** state lives in `Freeway` (serde) or entities; randomness only from
  `sim.state.rng`; snapshots and replay are exact (`tests/replay.rs`).
- **Helpers** ran in git worktrees under `.claude/worktrees/`, each with its own `target/`.

## Source Information (also in `README.md`)
Initial date 2026-10-17; SOURCE *Grand Theft Auto V*; GAME ENGINE Pavilion (Shardfall,
https://github.com/michaelcrosato/shardfall, commit `0098196`, v0.1.0); environment Claude Code
in an Anthropic cloud session on Linux. The model and effort level are stated in the session's
chat reports.
