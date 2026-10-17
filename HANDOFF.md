# HANDOFF — Grand Theft Freeway-BX, end of session 1 (2026-10-17)

For the next implementing session. Read `CLAUDE.md` (it imports `docs/DOCTRINE.md`), then this
page, then `PROGRESS.md` → "Now", then `PLAN.md` §0–§4 and §10 once and afterwards only the
sections the current milestone names. `PLAN.md` stays the source of truth; this page says where
things stand and what to do next.

## Where things stand
- **Done:** M0 bootstrap, M1 platform, M2 city (acceptance evidence in `PROGRESS.md` → Log).
- **Partly done** (the milestone list in `PROGRESS.md` marks *Done* and *Left* for each):
  - **M3** player and vehicles: on-foot controller and cameras, E9 vehicles v2 in the engine with
    all 15 archetypes (`content/freeway/vehicles.toml`, feel ranges test green), GPS ribbon, radio
    skeleton.
  - **M4** population: pedestrians (`peds.toml`, population manager, crossings, flee/cower, E11
    low detail), ambience beds.
  - **M5** combat: on-foot weapons (`weapons.toml`; hitscan, aim assist, recoil, spread, reload,
    melee), vitals, hit reactions and deaths.
- **Engine extensions landed:** E1–E8 (E8 part 1 plus music/radio), E9 vehicles v2, E11 character
  low detail (`Character::lod`), E12 tools, E13 app shell, E14 regions. E10 (mission DSL) and the
  rest of E11/E12 come with M5–M6.
- **Health:** every test green (pav_core 305, pav_render 5, pav_view 43, pav_world 9, pav_audio
  10, pav_ui 18, pav_app 25, pav_tools 119, gtf_game 44); clippy `-D warnings` clean; WGSL passes
  Tint; `gtf perf view=all` within budget with the crowd; the web build boots into the city.

## Next, in value order (PLAN §2: D1, D2, D3, D4, D5, D12 first)
1. **Finish M3 (D3 driving).**
   - `LIB/gtf_vehicles` body props for the 6 most common archetypes plus a `wheel`, made through
     the Object Studio loop (`asset_edit` → `asset_preview views=true`) and published with
     `pack=gtf_vehicles`.
   - Register the pack like `gtf_city`: `props::store::SHIPPED` and `pav_core/build.rs`. Then add
     `body =` / `wheel =` to `vehicles.toml`. Until then cars draw as boxes.
   - Game side: △ enter/exit/carjack/locked cars (`sim.enter_vehicle`, `exit_vehicle`,
     `set_vehicle_door`).
   - Driving from the pad (R2 throttle, L2 brake/reverse, R1 handbrake, left stick, L3 horn/siren)
     → `VehicleInput` returned from `Game::vehicles`.
   - The vehicle chase and cinematic cameras (§5.6.2).
   - Vehicle audio loops (`engine.<class>`, `tyre`, `siren.*`, `rotor`) driven by
     `sim.vehicle_status`.
   - The radio in cars: D-pad ←/→, and the dial card via `VehicleHud` instead of the toast.
   - Helicopter controls; the `vehicle_feel` tool (wraps `pav_core::vehicle::feel::run`);
     captures and filmstrips; then the M3 acceptance.
2. **M4 traffic (D4):**
   - Traffic agents on the lane graph: signals, turns, lane changes, yielding to people, pulling
     over for sirens. Far traffic runs kinematic (`set_vehicle_kinematic`,
     `move_kinematic_vehicle`).
   - Buses on a route; parked cars on `city.lanes.parking`; scenario points for people.
   - The `traffic`, `peds` and `soak` tools and the soaks.
3. **M5 heat (D5) and the rest of combat:**
   - Cover (R1, `city.cover`), grenades, held weapon props, enemy AI.
   - Crimes and witnesses (people report what they see), stars, dispatch from stations along the
     lanes (never spawned in view), police on foot and in cars.
   - The helicopter with a searchlight (`SpotDraw`), the search zone and cones on the minimap,
     evasion, respray, Busted/Wasted.
4. M6 structure, M7 content, M8 frontend and attract, M9 polish, M10 release (PLAN §9). Attract
   mode (D9) and the final deliverables (D14–D15: README, `docs/gtf/ENGINE_ANALYSIS.md`,
   `HANDOFF.md`, single-file HTML, Windows exe, the final report with Source Information) are
   required for Done.

## Known issues
- **Dead bodies:** they hold a half-crouched last pose. The death clips play in place, so the hips
  stay up. Needs the clip's own height applied to characters played in place, or a lying pose.
- **Vehicles:**
  - Occupant busts are drawn behind opaque glass (the renderer has no transparency).
  - No turn indicators.
  - Headlight beams are one point light on the driven car.
- **Meshes and sky:**
  - Generated meshes (tapered poles, lathes) keep full detail, about 86 k triangles downtown; the
    round built-ins are already coarsened by screen size.
  - Sun through thin cloud is not applied. The patch is saved at
    `docs/gtf/patches/cloud_through.patch`; `git apply` it, then run `scripts/check-wgsl.sh` and
    take captures.
- **The tour:** `freeway/tour` has one leg with no lane route (northbound into the map's north
  edge, then onto the southbound carriageway). The camera path jumps across the median there. It
  is harmless for the streaming test.
- **Tool placement:** `citymap` and `lanes` live in `gtf_app/src/tools.rs` (the plan put them in
  `pav_tools`). They read the game's city.

## Working notes
- **Builds:**
  - 4 vCPU and a fixed disk allowance (~39 GB in all; `df` "Avail" is what is left).
    `CARGO_BUILD_JOBS=4`.
  - A full workspace build with tests takes ~8 GB. Prune duplicate artefacts in
    `target/debug/deps` (keep the newest per crate).
  - Delete a helper worktree's `target/` once its branch is merged.
- **Helpers:**
  - Run them in git worktrees (`.claude/worktrees/…`), each with its own `target/`.
  - Never hard-link one tree's `target` into another: the shared lock and fingerprint files make
    the trees rebuild each other forever.
  - Merge their branches with `--no-ff`; never rebase someone else's branch.
- **Looking at things:**
  - `gtf capture … hud=true`, `filmstrip`, `perf view=all`, `citymap`, `lanes`, `audio_capture`
    (renders events, loops and the radio).
  - Commands typed into a run: `input cmd="time 21:30"`, `weather rain`, `radio freeway_fm`,
    `weapon pistol`, `target 8` (a stranger along the view).
  - Reference views: `freeway/view/<name>`.
  - Web: `scripts/build-gtf-web.sh` then `node scripts/web-check.mjs dist/GrandTheftFreeway-BX.html
    --query "scene=freeway/dev" --expect "boot complete" --shot-timeout 200000` (SwiftShader is
    slow).
- **Determinism:** game state lives in `Freeway` (serde) or entities; randomness only from
  `sim.state.rng`; snapshots, rewind and replay are exact (tests cover them with pedestrians).

## Source Information (also in `README.md`)
Initial date 2026-10-17; SOURCE *Grand Theft Auto V*; GAME ENGINE Pavilion (Shardfall,
https://github.com/michaelcrosato/shardfall, commit `0098196`, v0.1.0); environment Claude Code
in an Anthropic cloud session on Linux. The model and effort level are stated in the session's
chat reports.
