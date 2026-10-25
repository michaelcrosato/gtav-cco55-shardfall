# PROGRESS — Grand Theft Freeway-BX

The implementation tracker for [`PLAN.md`](PLAN.md). Update it as work lands: tick tasks, fill the
Definition of Done evidence, log decisions and open issues. Keep "Now" current — it is what a
resumed or compacted session reads first.

## Now
- **Phase (2026-10-25, session 5):** M0-M8 done; **M9 is under way** (the first polish batch
  is in: `6d0797b`-`2164b80`), then M10.
  - **Done in sessions 4-5:** D1 (wasm clippy clean); every story mission and activity, the
    radio, the landmarks and interiors, completion (M7); the frontend, the settings, About,
    the attract director and touch mode (M8: D8 and D9 done); M9's gameplay gaps (the bat and
    heavy melee, vault, fall damage, first-time tips, traffic that reacts, scenario points,
    the yelp, rounds that burst tyres), camera shake, reverb, rain splashes, the browser's
    perf line and Low on software adapters.
  - **Next:** M9's second batch (vehicle effects, Auto quality, the FPS readout's true time),
    then the art, audio and performance passes and the bug bash (Remaining work → M9), then
    M10.
  - Read `HANDOFF.md` first, then this file. **Remaining work** (below the milestones) is the
    checklist.
- **Missions and activities (2026-10-25):** the bot wins every story mission, the race and both
  hold-ups on seeds 1-3 (release `gtf`, `play bot=true seeds=3`; ignored `story_*` and
  `activity_*` tests in `tests/missions.rs`), every stunt jump from its run-up on three seeds
  (`tests/activities.rs`), and vigilante, the Purse Snatcher and Getaway Needed on seeds 1-3
  (ignored tests in `tests/activities_street.rs`). It also still wins `dev_test` and
  `dev_test2`.
- **Session 2:** M3-M5 (driving, traffic, `LIB/gtf_vehicles`, heat and the police, cover,
  grenades, armed opponents, WORKSHOP clips, guns in hand, drive-bys, the respray and car swap,
  the evasion bot, the map's edge) and M6: the mission DSL with its validator, runtime and bot
  (`docs/gtf/MISSIONS.md`), the three protagonists (switch wheel, sky-cam, abilities, life
  spots), the phone, the shops, saves and two engine additions (district `[[spot]]`s,
  `VehicleInput.grip`).
- **Soaks (release `gtf`, session 2):** traffic 600 s seeds 1-2 and peds 600 s pass every
  limit; roam 900 s seeds 2-3 pass, seed 1 gives up one stuck car at a dead-end turnaround
  (open issue). Not re-run since the traffic and landmark changes: M9.
- **Perf:** session 4's container passed `gtf perf view=all` before the session's work
  (downtown 3.6-3.9 ms a frame, 2.0-2.1 ms a tick). The landmarks helper measured every view
  at or under 3.60 ms with the landmarks in (interleaved A/B, best of 2-4). Thin margin: M9.
- **Tests (M7's end, `bd2b6db`, 2026-10-25, release):** every workspace crate green — pav_core
  313, gtf_game 147 (25 long ones ignored, run separately: M7 acceptance), pav_view 47,
  pav_audio, pav_ui, pav_render, pav_world, pav_app, gtf_app and pav_tools 202; wasm clippy
  clean; clippy `-D warnings` clean on the touched crates.
- **Helpers:** none running. (The vigilante and random-events helper was stopped by a container
  restart; its work was merged and finished in session 5.)
- **Branch / PR:** `claude/keen-cerf-g5vksy`, draft PR #2 (description current to the radio
  and landmark merges).

## Milestones (PLAN.md §9 has the full task lists and acceptance criteria)
- [x] **M0 Bootstrap** — engine vendored; baseline tests; E1 registry, `pav_tools::cli`, `pav_app`
      lib split, `gtf_game` + `gtf_app`; `freeway` scene in every target; web boot check.
- [x] **M1 Platform** — E2 input v2, E3 hook v2, E4 presentation data, E5 `pav_ui` + headless HUD
      capture, E6 camera intent + draw list, E13 app shell, E8 part 1 (bank + loops), UI gallery,
      FPS readout.
- [x] **M2 City** — E14 world regions first (region provider, static meshes, speed-aware streaming,
      far LOD, unload policies); `pav_world` generator + graphs; city props/materials; E7 sky, time of
      day + weather, window kernel, CPU region culling, merged statics; district + dev district +
      views; `citymap`, `lanes`, `perf`.
- [x] **M3 Player and vehicles** — on-foot controller, camera modes, E9 vehicles v2 (pitch/roll
      freed with stabilization), enter/exit, carjack, damage, helicopter, vehicle audio, GPS ribbon,
      radio skeleton, `vehicle_feel` incl. slopes and jumps. *Acceptance:* `tests/vehicles.rs`
      (every archetype within its feel ranges incl. the 15% grade and the kicker jump; the
      helicopters fly); `tests/driving.rs` (get in, drive, get out; locked car and carjack;
      bail-out; helicopter climb; a car throws the person it hits) and the grenade/explosion
      test; filmstrips looked at — `out/m3/drive_filmstrip.png` (12 s of autopilot at golden
      hour, no jitter or clipping) and `out/m3/walk_filmstrip.png` (a sprint into a wall: the
      camera stayed inside the wall until the shoulder fix, now it keeps out); perf in budget.
- [x] **M4 Population** — traffic, pedestrians, population manager, LOD, buses, parked cars,
      ambience, soaks. *Acceptance:* `gtf soak mode=traffic seconds=600` seeds 1-2: ok (AI
      crashes 1 and 0, stuck 0, spawn/despawn in view 0, NaN 0, fell 0); `mode=peds
      seconds=600`: ok; `tests/traffic.rs` (two minutes obeying the lights, pull-over for a
      siren, buses at their stops), `tests/peds.rs` (crossings on the walk light, replay);
      `gtf perf view=all` within budget with the population; streets looked at
      (`out/m5/downtown_lod.png`, `out/m4/peds_sheet*.png`, the drive filmstrip).
- [x] **M5 Combat and heat** — weapons, cover, vitals, enemy AI, wanted system, police AI, air unit,
      busted/wasted. *Acceptance:* `tests/combat.rs` (7) and `tests/enemies.rs` (4); `tests/heat.rs`
      (witnesses phone it in, police arrive unseen and arrest, Wasted at the hospital, the
      respray and the car swap, the evasion bot escapes 3★ within 180 s on at least 3 of seeds
      1-5 — rates in Decisions); police never spawned in view in any roam soak (counter 0);
      3★ night captures looked at — `out/m5/night3_sheet.png`, `night3_012s.png` and
      `night3_030s.png` show the searchlight's pool on the street and the vision cones on the
      radar.
- [x] **M6 Structure** — mission DSL + runtime + bots, protagonists + switching + abilities, phone,
      economy and shops, save/load. *Acceptance:* `tests/missions.rs` — the bot finishes both
      test missions on seeds 1-3 (`dev_test` drives: goto, enter_vehicle, drive_to, follow,
      race, escape, lose_heat, deliver; `dev_test2` on foot and story: kill, collect,
      photograph, interact, stealth, holdup, chase, choose, branch, survive, switch, parallel,
      defend) and the loud way; every step kind used; fail → retry → skip after three fails;
      quit; pass pays, texts and autosaves; markers start missions; replay from the pause menu;
      the validator reports file:line. `tests/protagonists.rs` (switching keeps each one's
      cash, weapons and place; the wheel and the sky-cam; abilities slow time, change damage,
      drain), `tests/phone.rs` (tiles, the map waypoint, the camera catches who is centred),
      `tests/economy.rs` (gun store, snacks, customs, a save round trip into a fresh game,
      Continue loads the newest, the safehouse bed). Release `gtf`: `play bot=true` passes
      `dev_test2` in 218.7 s of game time (gold). Captures looked at (`out/m6/`): phone home and
      camera, the switch wheel, the sky-cam over the district, Mack landed, Deadeye's grade
      (`deadeye_compare.png`), the intro card, objective and subtitle, MISSION FAILED and its
      card, MISSION PASSED, the results screen, the gun store. Fixed after looking: the HUD,
      tips and play prompts now give way to any card or menu (the minimap showed through the
      shop's price box), the camera app shows its buttons as prompts (its row printed raw
      glyph markup), the ability grade was too faint, the fail card lost "failed" once the
      banner faded, the results printed ✓/✗ in a font without them, `dev_test2` started behind a
      parked SUV.
- [x] **M7 Content** — final district, landmarks, interiors, all vehicles and props, missions m1–m3,
      s1–s3, h0, h1 (Loud + Smart), activities, radio stations, completion. *Acceptance
      (2026-10-25, release, the merged tree at `bd2b6db`):*
      - **Story:** the bots win every story mission on seeds 1-3, first time (ignored
        `story_*` in `tests/missions.rs`: m1, m2, m3, s1, s2 quiet and loud, s3, h0, h1 Loud and
        Smart; 27 of 27 runs, no retries).
      - **Activities:** the race and both hold-ups (`activity_*`, 9 of 9), vigilante, the Purse
        Snatcher and Getaway Needed (`tests/activities_street.rs`, 9 of 9). Every stunt jump is
        landed from its run-up on three seeds and the fifteen Lost Shields are taken
        (`tests/activities.rs`, default suite).
      - **Content and captures:** `tests/content.rs` validates every file and name. Every
        landmark was captured at its best hour and looked at (`out/landmarks/`, 24 views).
      - **Completion from play:** `tests/completion.rs` (ignored) has the bots play the whole
        game in one free-roam game: every mission, activity, jump and shield, to 98.0% (story,
        activities and shields done; 8 of 10 golds). 100% with gold everywhere is checked in
        `activities.rs`. This test found two bugs in missions started away from their scene
        (fixed in `bd2b6db`).
      - **Checks (release, at `bd2b6db`):** every workspace crate's tests pass (pav_core 313,
        gtf_game 147 with 25 long ones ignored, pav_view 47, pav_tools with the rest 202; run
        crate by crate to fit the disk, Log), and wasm clippy is clean.
- [x] **M8 Frontend and attract** — title, menus, settings, About, loading cards, attract director,
      touch = attract only. *Acceptance (2026-10-25):*
      - **Tests:** `tests/frontend.rs` (16: boot → title → menu → loading card → free roam,
        settings applied, About's pages, the pause tabs, save and restart, pointer focus) and
        `tests/attract.rs` (8: idle starts it, any device's input ends it, touch screens never
        leave it and show only the gear, each vignette sets up and plays, a new vignette starts
        from an empty world, the card's prompt without the FPS readout).
      - **Attract soak** (release `gtf soak mode=attract seconds=1800`, 30 minutes each): seeds
        1 and 2 clean — no stall, nothing fell through the world, no NaN, every vignette played
        6-7 times (48 and 46 plays), the entities per complete loop flat (seed 1: 561-583, seed
        2: 570-593) and the memory flat from the second loop (252-260 MiB), 0.78-0.81 ms a tick
        (`out/soak/attract_s1.json`, `attract_s2.json`). Lost chases end the pursuit vignette
        (the protagonist out of the car, or boxed in by the police for 6 s) instead of counting
        as stalls.
      - **Browser checks** (`scripts/web-check.mjs` on `dist/GrandTheftFreeway-BX.html?perf=1`,
        headless Chromium on SwiftShader at 0.1-0.3 frames a second): **desktop** — boot, the title
        at 175 s, the attract loop after its idle at 686 s (the dock fight), Space back to the
        title at 825 s; **mobile** (844×390, touch) — the attract loop straight from boot at
        179 s, taps leave it playing (`attract: a touch; the loop plays on`), the gear opens its
        overlay (sound, graphics, the frame-rate readout off, About, close) at 382 s; no panic,
        never the main menu (`out/m8/web_desktop.txt`, `web_mobile.txt`, the screenshots looked
        at). The perf line is logged throughout; the browser's sim time is measured in M10's
        final run with nothing else running.
      - **Captures with the HUD**, every frontend screen looked at (`out/m8/front/`: boot,
        title, main menu, the five settings pages, About's five pages, the four intro cards,
        play, the pause tabs, a loading card; `out/m8/attract/`: each vignette twice, the cut,
        the touch gear, its overlay and About). Fixed after looking: the main menu was never
        drawn, settings labels cut off, a "→" in a font without it, the title's bare slope,
        cine shots inside walls, the stunt banner carried into the next vignette, the switch
        vignette standing off-road, "DEMO" shown as a hint.
- [ ] **M9 Polish** — art, animation, audio mix, UX against the 10-foot rules, accessibility,
      performance, bug bash.
- [ ] **M10 Release** — builds (native, Windows `.exe`, `dist/GrandTheftFreeway-BX.html`), docs,
      final verification, delivery.

## Remaining work (gap inventory, 2026-10-24)
Two read-only audits of the code against PLAN §2, §5, §7.8 and §9 found these gaps; their main
claims were spot-checked. Tick items as they land and move the evidence to the milestone entries
and the DoD table. PLAN stays the source of truth: this list says what is missing, not the design.

- [x] **First:** fix the D1 wasm-clippy lints in one `engine:` commit (`1dfb757`, 2026-10-24:
      `cargo clippy -p pav_app -p gtf_app --target wasm32-unknown-unknown -- -D warnings` clean).

**M7 content** (in PLAN §2's order of value: the heist path first)
- [x] **Missions** (2026-10-24). The story missions are written in the DSL, validated, and
      added to `FILES`: `m1_repo_man`, `m2_midlife`, `m3_overdrive`, `s1_rush_hour`,
      `s2_heavy_metal` (quiet or loud), `s3_test_drive`, `h0_board` (the board, the cut) and
      `h1_freeway_job`.
      - **h1 Loud:** Roxy blocks Route 404 with the City Works truck, Mack breaches the Ironclad
        truck and fights off the guards, the bags, two SWAT waves at 4★, then Dez takes the
        Okami Kaze off the Harbor Spur's broken end onto the harbor road and loses the police.
      - **h1 Smart:** the fake roadwork, Dez tails the truck to the harbor warehouse, Mack swaps
        the cargo and drives out in a City Works truck at ≤ 1★.
      - **New for missions:** a `jump` step, `deck:` anchors, the `pay` and `armor` actions,
        vehicle spawn health, actors that wait for their ground, `mission story <flag>`, and
        `play seeds=N cmd=`.
      - **Evidence:** release `gtf`, `play bot=true seeds=3` (`cmd="mission story smart"` for
        Smart, `cmd="mission choose loud"` for s2 loud): 3/3 on every one. Game seconds per
        run: m1 113-115, m2 85-109, m3 236-277, s1 242-294, s2 89-117, s3 192-229, h0 21,
        h1 Loud 111-244, h1 Smart 160-163. The ignored `story_*` tests in `tests/missions.rs`
        repeat these. Captures looked at in `out/s4/`: the spur's end from above and the
        broken ramp, Mack's loot and truck, m2's crash, s3's window.
- [x] **Landmarks** (2026-10-24, the landmarks helper, merged at `2451340`).
      - **The engine kit:** landmarks are built from parts (`pav_world/src/parts.rs`; documented
        in `docs/REFERENCE.md` under "Urban kit").
      - **21 landmarks:**
        - civic and services: the bank tower, City Hall, SPPD Central, St. Brigid's with the
          Skylark on its helipad, the parking garage, the impound lot;
        - shops and food: Bang Bang Bros., Paloma Customs, Gas-N-Grab, the Grind House, the
          Freeway Diner, Luxe Motors with breakable glass;
        - industry and harbor: the Ironclad depot, the City Works yard, the harbor warehouse,
          the MV Pacific Star;
        - the pier: its neon arcade, food stalls and the turning Big Wheel;
        - the homes: Dez's, Mack's and Roxy's.
      - **Dressing:** 49 shop names on Sunset Row's sign bands (about a third in neon), and lots
        filled with their vehicles.
      - **Evidence:** `pav_world/tests/paloma.rs` (the landmarks keep their places), `gtf_game`
        tests (the landmarks at play; a car climbs the garage ramp; the armoured truck drives
        into the warehouse; a car keeps its speed through Luxe's glass). The helper's captures:
        `luxe_showroom`, `pier_night`, `wheel_diff`, `bank_tower`, and 11 on-foot interiors with
        the HUD. Perf: every view at or under 3.60 ms.
- [x] **Interiors** (2026-10-24, same merge): the gun store (counter, nine gun racks), the
      Paloma Customs bay with its lift, Gas-N-Grab, the Grind House, the Freeway Diner, the
      three homes with their beds (saves happen at the bed), the warehouse, Luxe's showroom and
      the arcade. Shops open at their counter. Rooms are `Walk::Indoor` tiles: people can be put
      and walk there, crowds don't spawn there.
- [x] **Bodies and looks** (2026-10-25, the looks helper: `dc0ea9b` engine, `78b1b3f` content,
      `14730f2` game).
      - **Helicopters:** `pav_view` draws a helicopter's body prop as it does a car's; its
        `rotor` group turns about the mast and `tail_rotor` 1.7 times as fast, `beacon*` parts
        blink, `nav*` glow and `siren_*` flash when crewed. Two bodies in `LIB/gtf_vehicles`:
        `skylark_news` (white and blue, three blades, a camera ball) and `sppd_air` (dark with a
        white band, four blades, the searchlight on the body, red and blue lamps).
      - **People:** `PuppetDef` gained hair (eight styles and a colour), beards (four),
        headwear (ballcap, beanie, hardhat, helmet, tactical helmet, hood), sunglasses, a
        backpack or shoulder bag, a badge, a vest and a tie, and `neck` (a smaller head on a
        longer neck). `gtf_game::looks` builds everyone on peds.toml's `[body]` and dresses them
        from a `wear` table per kind: the nine archetypes, officers on foot, the enemy sides,
        street crooks and robbers, and the crew (Dez's cap and goatee, Mack's grey stubble and
        sunglasses, Roxy's pink ponytail).
      - **Evidence:** pav_core, pav_view and gtf_game tests (the body's height, dressing
        chances, a dressing that draws nothing from the main random stream, the rotor parts
        turning, a crewed helicopter's beacon and sirens); captures looked at (the helper's
        and mine): `out/looks/` in its worktree, the Skylark on St. Brigid's pad and in flight,
        the air unit by day and at night at 4★, a mixed crowd of the crew, officers, SWAT and
        pedestrians, sheets of every archetype and side. New tools: `photo` (a held camera)
        and `lineup <who>`.
- [x] **Activities** (2026-10-24/25, `content/freeway/activities.toml`, `activities/`).
      - **The Pretzel Run** (`act_pretzel_run`): three laps round east Paloma Bay against three
        racers, a bet through `choose` costs; the race DSL drives racers gate to gate
        (`Agent.through`) with a rubber band and shows the lap and the place.
      - **Five stunt jumps** (the Ditch, the Freeway Gap, the Pier Ramp, the Hills, the Harbor
        Container Ramp): through the gate fast enough, slow motion and a stunt camera in the
        air, the bonus for landing inside the zone; the container yard is district `[[prop]]`s.
      - **Two hold-ups** (`act_holdup_gas`, `act_holdup_grind`): aim at the clerk, a seeded
        `loot`, out to the Kaze at the door, lose a capped two stars.
      - **Vigilante:** five levels from a police car (D-pad right / E, or the Jobs app); see
        Decisions for its rules.
      - **Random events:** the Purse Snatcher and Getaway Needed, outside missions, on a
        cooldown, where they fit.
      - **Fifteen Lost Shields** with trivia cards; all fifteen pay $10,000 and leave a gold
        Okami Kaze outside Dez's place.
      - **Evidence (release `gtf`, seeds 1-3 unless noted):** race 3/3, gas 3/3, grind 3/3
        (`play bot=true seeds=3`; `activity_*` tests); jumps 15/15
        (`every_stunt_jump_is_landed_from_its_runup_on_three_seeds`); vigilante seeds 1-8 7/8
        (6 ran out level 3's clock); the Purse Snatcher 6/6 and Getaway Needed 6/6 (seeds 1-6;
        `tests/activities_street.rs`); shields
        (`every_lost_shield_is_taken_by_walking_up_to_it_and_all_fifteen_pay`). Captures looked
        at: the stunt camera over each jump, the race's lap/place HUD, the vigilante HUD and
        the stuck moments the bot was fixed from (`out/act/`).
- [x] **Radio** (2026-10-24). Five genres with their own kits and feel
      (`pav_audio/src/music/`: synthwave, boom-bap, surf, cumbia, smooth jazz), song sections
      and a per-station ident; SPNN 1040 AM runs a program from `radio.toml` [talk.spnn]
      (sounder, 30 headlines as subtitles, 9 ads over jingles, call-ins, weather, traffic
      reports that name where the player's chase or search is); DJ cards between songs (the
      radio card); scanner chatter at 2★+ (squelch, a `voice.radio` babble loop, a subtitle
      with a code, a unit and the place); formant babble voices (`pav_audio/src/voice.rs`).
      Evidence: `cargo test -p pav_audio` (tempo of 10 songs exact from the onsets'
      autocorrelation; swing boom-bap 2.00, smooth 1.67, synthwave 1.00; choruses within
      0.5 dB; talk's beat strength 0.13 against a song's 0.36), `cargo test -p gtf_game --test
      radio` (6), captures `out/radio/*.wav` with `radio seek` (in-band tempo 107/92/161/95/73
      BPM as the songs; centroids 1272/819/2065/639/1045 Hz; SPNN 83% speech band, 4.8
      syllables/s; no clipping).
- [x] **Completion and stats** (2026-10-24/25). Completion is 50% story, 30% activities, 10%
      Lost Shields and 10% gold medals (PLAN's shares); activities and shields are saved. STATS
      shows a completion card (with the four bars) and an activities card (done of 26: the
      shields, the jumps, the street jobs, the random events) beside the crew. Test:
      `completion_reaches_100_with_everything_done`. Capture looked at:
      `out/act/stats_tab.png` (the activities card's title was cut off and two of its six lines
      never showed; fixed to four lines and a done/total title).
- [x] **Checks** (2026-10-25). The PLAN §7.8 suites that were missing (`c2f3fa5`):
      `content.rs` (5: every file loads and validates; every prop, material, clip, sound cue,
      spot and landmark named exists), `district.rs` (4: the same city whatever the seed, one
      walking network, no building on a road, every landmark's door on the network), `player.rs`
      (4: walk, run, sprint until the stamina is spent; jump; crouch; swim), `save.rs` (3: a
      round trip into a fresh game, a save from before activities, a refused newer or broken
      one), `replay.rs` (2: two minutes of roam with heat and traffic replay exactly; a snapshot
      mid-chase plays on exactly) and `ui.rs` (5: the HUD on foot, with money and stars and in
      a car; the pause menu's five tabs; the wheels; the phone; the banners; each state's UI
      model written as JSON). Every landmark captured at its best hour and looked at
      (`out/landmarks/`, 24 views: the bank tower, City Hall, SPPD Central, the hospital roof,
      the garage, the impound lot, Bang Bang Bros., Paloma Customs, Gas-N-Grab, the Grind House,
      the diner, Luxe Motors, the Ironclad depot, the City Works yard, the harbor warehouse, the
      freighter, the pier at night, Sunset Row's neon, the three homes, the hills both ways and
      the freeway). Fixed after looking: traffic-light lenses glowed when unlit; the avenue's
      quay end let cars into the sea (a view of it now, `quay_dead_end`). Art notes for M9 are
      in Open issues.

**M8 frontend and attract** (done 2026-10-25; the milestone entry has the evidence)
- [x] **Screens** (`dad4cd7`): the title is a live shot from the Mirador Hills at golden hour;
      New Game plays an intro card per protagonist and one of the three (the painterly look),
      then a loading card (halftone) over the still world; Pause → Game has Save (near the bed,
      no mission, no stars) and Restart checkpoint; the Map has its filters and snaps to the
      player; Brief has the mission log and the texts.
- [x] **Settings** (`5185f23`, `dad4cd7`): five pages — AUDIO (master, music, effects, dialogue,
      the default station), DISPLAY (fullscreen, quality Low-Ultra, render scale, dynamic
      resolution, v-sync, brightness, the FPS readout), HUD (scale, safe area), CONTROLS (aim
      assist strength, X and Y sensitivity, invert Y, sprint hold or toggle, vibration, button
      glyphs), ACCESSIBILITY (subtitles and their size, colourblind blips, reduce camera shake,
      high contrast, weapon-wheel slow motion, camera recentring); each is read by the game or
      the host (`tests/frontend.rs`).
- [x] **About and credits** (`dad4cd7`, M9): five pages with the full §1 Source Information
      block, the fonts' and the motion libraries' credits (Quaternius, Mesh2Motion, 100STYLE,
      CMU); the wording no longer says every asset is generated.
- [x] **The attract director** (`725c06c`, `dad4cd7`, M9): seven vignettes in `attract.toml`
      shuffled each loop, a cut through black with a caption card, the logo, caption and
      prompt, exit at once on any device's input (typed tool commands excepted), stall
      handling and lost chases, a population reset per vignette, `freeway/attract`,
      `tests/attract.rs`, `gtf soak mode=attract`.
- [x] **Touch:** attract only; the gear button and its overlay (About, sound, graphics, the FPS
      readout); no HUD, captions or touch controls; the browser checks (desktop and mobile).

**M9 polish**
- [x] **Gameplay** (2026-10-25; tests in `combat.rs`, `player.rs`, `traffic.rs`, `driving.rs`,
      `protagonists.rs`, `combat`'s unit tests; Decisions → M9): rounds burst tyres and break
      lamps; traffic lights up after dark; vault (running at a low wall) and fall damage; the
      bat in the fists' slot and a heavy hook on a held ○; near-miss swerves and angry drivers
      getting out (calm free roam); scenario points as data (benches, stalls, umbrellas, the
      net, tables); the siren's yelp; the sky-cam at 300 m in 3 s with its grid; first-time
      tips; the helicopter's and the police car's control help; "Lose the cops." while wanted.
- [ ] **Audio.** Reverb by place is in (`AudioScene.reverb`: indoors, under decks, the Ditch,
      downtown). Mix pass (levels, no clipping) to check with `audio_capture`.
- [ ] **Visuals.** Camera shake (honouring `reduce_shake`), rain splashes and brighter neon are
      in; the art pass at every time of day and weather with captures remains.
- [ ] **Performance.** The browser's `?perf=1` line (sim 0.1-4.5 ms a tick in SwiftShader), the
      quality presets Low-Ultra with population scaling and desktop dynamic resolution (M8),
      Low on software adapters, and `gtf perf`'s shadowed lights, particles and entities are
      in; downtown well under budget remains to be measured.
- [ ] **Bug bash.** Clear the Open issues. Run the bots on 5 seeds. Edge cases: water (swim;
      `driving.rs` sinks a car), falling (fall damage), interiors (`landmarks.rs`), switching
      mid-chase (refused: `protagonists.rs`), saving during heat (refused: `economy.rs`).

**M10 release**
- [ ] **Builds.** `scripts/build-windows.sh` doesn't zip the `.exe`. `pack_single_html.py`
      can't write the Source Information header comment or `<meta name="description">`.
      `dist/` doesn't exist yet.
- [ ] **Docs.** Write `docs/gtf/ENGINE_ANALYSIS.md` (with the D16 systems table),
      `docs/gtf/HANDOFF.md` and the final `README.md` (how to play, controls, credits). Add a game
      entry to `docs/FEEDBACK.md` → Games, and engine docs for every extension.
- [ ] **D16.** Add a view and capture for every Proving Grounds station, plus combat/weapons and
      radio signs.
- [ ] **Delivery.** Final verification of D1-D13 and D16 with fresh captures, then the PR
      description. Send the HTML and the zipped `.exe`, then the final report (§12.4).

## Definition of Done evidence (PLAN.md §2)
| Item | Status | Evidence (test, command + result, capture looked at, artifact) |
|---|---|---|
| D1 Builds and checks | ◐ | 2026-10-24: wasm clippy clean (`1dfb757`); host clippy -D warnings and the touched crates' tests green after each merge. 2026-10-25 (M7's end, `bd2b6db`): every workspace crate's tests green in release (pav_core 313, gtf_game 147, pav_view 47, the rest with pav_tools 202) and wasm clippy clean. Builds and the final full run are M10 |
| D2 The district | ◐ | 2026-10-24: 21 parts-built landmarks with interiors, Luxe's breakable glass, the turning Big Wheel, the City Works yard, the harbor warehouse, the pier and freighter, shop signs (merge `2451340`; `pav_world/tests/paloma.rs`, gtf_game landmark tests, the helper's captures); the spur's re-routed end (`out/s4/spur_above2.png`). 2026-10-25: every landmark captured at its best hour and looked at (`out/landmarks/`, 24); `tests/district.rs` (same city whatever the seed, one walking network, no building on a road, every door on the network); roads ending at the water closed off (`01bbca8`). Streaming with no holes and the weather set are re-checked in M10's final run |
| D3 Player | ◐ | M3: feel ranges for all 13 road archetypes, driving/carjack/bail-out/helicopter tests, on-foot camera with wall handling; M6: three protagonists with their own cash, weapons and places, the switch wheel and sky-cam, abilities (`tests/protagonists.rs`). 2026-10-25: `tests/player.rs` (walk/run/sprint and stamina, jump, crouch, swim); the crew's looks and both helicopters' bodies (`dc0ea9b`, `14730f2`). Vault, the bat and heavy melee, and animation polish: M9 |
| D4 Living city | ◐ | M4: traffic and pedestrian soaks within limits, buses, parked cars, struck people, witnesses; roam soak open issue (one stuck car in 2,700 s) |
| D5 Heat | ◐ | M5: `tests/heat.rs` (reporting, arrest, Wasted, respray, car swap, evasion), police never spawn in view, 3★ night captures |
| D6 Missions and activities | ✔ | M6: the mission system end to end (`tests/missions.rs`, `docs/gtf/MISSIONS.md`). 2026-10-24: the eight story missions with the heist both ways; the bot wins each on seeds 1-3 (`play bot=true seeds=3`; ignored `story_*` tests). 2026-10-25: every activity (race, 5 jumps, 2 hold-ups, vigilante, 2 random events, 15 Lost Shields), each won by the bot on seeds 1-3 (`activity_*`, `activities.rs`, `activities_street.rs`); completion and STATS cards. Economy and saves from M6 (`tests/save.rs`: a round trip, an old save, a refused newer or broken one). M7 acceptance at `bd2b6db`: 27/27 story runs, 9/9 activity missions, 9/9 street activities, all first time; the whole game by the bots to 98.0% completion (`tests/completion.rs`) |
| D7 UI | ◐ | M1 kit + M6 screens: intro card, objectives, subtitles, fail card, results, switch wheel, phone (9 apps), shop menus, save slots, captured with HUD (`out/m6/`); the 10-foot review is M9 |
| D8 Frontend | ✔ | 2026-10-25 (M8): boot splash → title (a live shot of the Mirador Hills) → main menu (Continue, New Game, Free Roam, Settings, About, Quit) → intro and loading cards (painterly and halftone looks); settings on five pages (audio, display and quality, HUD, controls, accessibility), each read by the game or the host; About on five pages with the §1 Source Information block and the fonts' and motion libraries' credits. `tests/frontend.rs` (16); every screen captured with the HUD and looked at (`out/m8/front/`) |
| D9 Attract mode | ✔ | 2026-10-25 (M8): 10 s without input on the title or a menu page starts the loop; the seven §5.8 vignettes (`attract.toml`: golden cruise with the radio, the 3★ pursuit with the SPPD helicopter, the dock gunfight in cover, the switch, big air, city life, the job's getaway), shuffled per loop, cut through black with a caption card; any device's input returns to the title at once; touch screens play it only, with the gear the only thing over the world. `tests/attract.rs` (8); 30-minute soaks on seeds 1 and 2 with no stall, fall or NaN and flat entities and memory (`out/soak/attract_s1.json`, `attract_s2.json`); the browser flows on desktop and phone (`out/m8/web_*`); captures `out/m8/attract/` looked at |
| D10 Audio | ☐ | Radio part done 2026-10-24: 5 music stations + SPNN talk, DJ cards, scanner chatter, the radio −6 dB under the effects and ducked 8 dB (dialogue) / 12 dB (menus); `cargo test -p pav_audio`, `-p gtf_game --test radio`; captures `out/radio/`. SFX, loops and the mix pass remain (M9). |
| D11 Visuals (incl. FPS readout) | ☐ | |
| D12 Performance | ◐ | `gtf perf view=all` (release) within every budget with the population, 2026-10-18 |
| D13 Verification | ◐ | 2026-10-25: bots complete every mission (both heist approaches) on 3 seeds (M7 acceptance, release); the evasion bot escapes 3★ (`the_evasion_bot_escapes_three_stars`; `evasion_rate` rolling 8/8, standing 5/8); snapshot/replay exact with the full city and population (`tests/replay.rs`); UI model captures of the HUD, pause tabs, wheels, phone and banners (`tests/ui.rs`). Soaks, the attract soak and every §5.9 screen captured: M8-M10 |
| D14 Docs | ☐ | |
| D15 Delivery | ☐ | |
| D16 Developer demo level | ☐ | |

## Decisions
- 2026-10-17 — Vehicles (E9): okami_kaze is AWD (the plan's example said RWD; RWD could not make
  3.8 s 0-100); holding the brake when nearly stopped reverses; an empty car with no input holds
  still with its brakes on; the engine explodes a burning car itself after 5 s; the engine demo's
  cars got free pitch and roll, the new tyres and softer springs (the drift room's guide updated).
- 2026-10-17 — People go low detail (E11: no character controller, one ground ray) beyond 30 m
  and back within 24 m: full detail out to 60 m cost 3.85 ms a tick downtown in the debug build;
  30 m gives 1.07 ms. The walk-map A* keeps its costs in flat arrays reused between searches.
- 2026-10-17 — Pedestrian density: a cap of 60 within 120 m, shared out by the zones around the
  player and the hour (night share 0.35); a new scene fills at once, in view too, from 15 m out;
  reference views centre their crowd on the camera.
- 2026-10-17 — Aim assist judges its 12° cone by heading (35° of pitch allowed) because the
  on-foot camera looks down at the player; with nobody in the cone, L2 levels the view to chest
  height ahead.
- 2026-10-17 — The format-2 field guide cap moved 7,200 -> 7,350 chars for the new `glow` field;
  `audio_capture`'s help was trimmed so `tools/list` stays under 48,000.
- 2026-10-17 — Aim camera at 2.0 m behind and 0.65 m right of the shoulder, its target at head height (PLAN §5.6.2 said 1.6 m / 0.6 m): at 1.6 m the stand-in body filled half the frame (`out/m3/cam_aim.png` vs `cam_aim2.png`). The on-foot camera's walls are five rays from the pivot (a stand-in for a sphere cast; the engine has rays only).
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

- 2026-10-17 — **Paloma Bay is 64 × 72 cells** (1024 × 1152 m): 8 extra rows of hills to the north
  give Mirador Drive, Lookout Drive and the PALOMA sign room (the plan's sketch had 5 rows).
- 2026-10-17 — **Terrain is a painted height grid** beside the map (one character per cell, 4 m
  steps) smoothed into 4 m samples; flat cells stay slabs so the city keeps sidewalks and markings.
  Hill roads are grid streets flattened across (switchbacks of straight runs), not splines.
- 2026-10-17 — **Route 404** is a swept deck (30.4 m, three lanes each way) at 11 m; ramps land
  on one-cell stubs off the cross streets (no frontage roads, so nothing drives under a low ramp);
  the corridor's ground sits at road level (`level = -0.15`).
- 2026-10-17 — **The Ditch** keeps the channel as its own road network (road roles never join
  channel and street cells); crossing roads keep their own characters and become bridges.
- 2026-10-17 — **Walking is a 2 m tile map** (sidewalk, crossing tied to its junction's signal,
  path, open ground) instead of a hand-built node graph: every loop, corner and crosswalk is one
  connected surface, A* over tiles for longer trips. Cover points come from building walls facing
  walkable tiles, the Ditch's walls and the freeway barriers.
- 2026-10-17 — **Game cameras get no cutaway** (walls are not cut or faded when a hook game
  supplies `PlayFrame.camera`).

- 2026-10-17 — **Vehicle feel targets (E9 design):** vehicles slower than 100 km/h (bus, armoured
  truck, utility truck, courier van) use 0 → 80% of top speed instead of 0–100 km/h; the tight
  ranges (0.9–1.1 g, 25–45° slide, climbing 15% at ≥ 60% of top speed) apply to cars, vehicles of
  2.5 t or more must still climb the grade with a looser grip range; "left" in vehicle data is the
  car's own left (driver's side, +x); the feel harness builds its own long strip, 15% slope and
  kicker (the dev district is too short for top speed); rapier keeps wheel raycasts and suspension
  while the engine applies its own tyre, brake, engine, drag and stabilising forces.

- 2026-10-18 — **Session 2 decisions:**
  - Vehicles are 2.5× tougher (`vehicles.toml` health): police contact and PITs wrecked cars in
    seconds. One crash costs once per contact (engine `crash_dv`): a car pressed into a wall lost
    its health in 0.2 s.
  - Heat: a crime escalates in front of the police only when it is as serious as the current
    level; only a police vehicle the player harmed (blame list, 15 s) counts as destroyed by them;
    the last sighting stands while unseen.
  - Paloma Customs came forward to M5 (the respray needs a door); the bat waits (the weapon
    wheel's eight slots are full).
  - The helicopter flies over the rooftops (it crashed into the downtown towers) and its crew
    see within 55° of straight down: an all-round view kept 3★ almost inescapable once it
    survived. The getaway autopilot does not brake for police cars (that is how a PIT works).
  - D13's evasion test counts escapes over seeds 1-5 (≥ 3) instead of naming seeds: any traffic
    change reshuffles which chases get away. Latest `evasion_rate`: 5/8 rolling, 2/8 standing
    (earlier this session 7/8 and 6/8; the standing start is an open issue).
  - The map has an edge: an invisible wall all round (engine hidden blocks), ROAD CLOSED
    barriers where roads leave it. Route 404's closed north end turns its traffic round 27 m
    before the barrier (it used to flow through the barrier on rails and could shove anything
    in its way through it).
  - Everything the game populates despawns with its ground (`Unload::Despawn`); nothing spawns,
    leaves its rails or switches to physics unless all the chunks under it are loaded. Wrecks
    lose their occupants.
  - Traffic: first come, first served at unsignalised junctions; with lights, green waits for a
    straggler; lanes that fan out of or merge into one lane see each other; everyone brakes for
    anything about to cross their path; mandatory lane changes for lanes that go nowhere;
    queued cars never back up; cars on rails switch to physics near any car the physics moves;
    stop lines 8 m back (turning buses); dead ends and the edge turn round on a drivable
    keyhole (11 m and 16 m short of the end).
  - AI-crash counting: an incident's when either car was fleeing, pulled over or shaken, or
    within 40 m of a police vehicle or the wanted player; a repeat of the same pair within 20 s
    counts once.
  - `gtf perf` times the median of five frames; far wheels (> 60 m) are plain cylinders; drawn
    assets tally a frame's meshes once (a hundred cars cost 9 ms a frame before).

- 2026-10-24 — **The MCP server runs `gtf mcp`** (`.mcp.json`; the key stays `shardfall`, so the
  tools are still `mcp__shardfall__*`). The engine's `cargo run -p pav_tools --bin pav` resolved
  features differently from the SessionStart hook's combined `pav` + `gtf` build. It rebuilt the
  dependency graph and timed out after 600 s. `cargo run -p gtf_app --bin gtf` is fresh after the
  hook (0.6 s to answer `initialize`) and serves 86 tools, the game's included. The engine docs
  still describe upstream's `pav mcp`.

- 2026-10-18 — **M6 decisions:**
  - Switching is open from the start (`crew.unlocked`): the story's introductions arrive with
    M7's missions. A step's `switch = [...]` limits whom the wheel offers while it runs.
  - Missions thin traffic (`{ traffic = 0.35 }`) and cap the heat (`{ heat_cap = 0 }`) where a
    set piece needs it: the bot's drive legs stalled in junction queues at full density, and the
    police killed Mack at 3★ after the range shootout.
  - The mission bot acts only through its input frame (pad presses plus the commands a player's
    tools send: `autopilot on|escape|off`, `pilot_goal x z desire`, `weapon <id>`), so replays
    and snapshots include it and nothing reaches into game state.
  - Checkpoints re-stage the mission's actors (every actor still up, with its behaviour) and put
    back the flags, heat, health, clock and the player's place and car, instead of snapshotting
    the whole sim. Skip after three fails fast-forwards to the next checkpoint: its steps are
    done for the player and their targets cleared.
  - Cars with a goal (mission drivers, the autopilot's `pilot_goal`) corner at no more than 1.15×
    the curve speed (they slid into the edge wall), and replan when a lane change leaves their
    route empty short of the goal lane; mission cars are left out of the population's counts.
  - Abilities: Slipstream (Dez, at the wheel: time 0.5, +40% tyre grip through the new
    `VehicleInput.grip`), Deadeye (Mack, on foot: time 0.35, aim-assist cone ×2, damage ×1.5),
    Overdrive (Roxy: damage ×2, taken ×0.5, fists' shove ×2); each washes the screen in its
    colour, half desaturated, with a vignette. Meters fill by play style (`protagonists.toml`).
  - Saves are JSON (`SaveData`, version 1) handed to the host through `AppRequests.save` and kept
    by the M1 host store (native files, web localStorage); `host.save` hands them back at boot.
    Three slots at the safehouses plus `auto` after each pass; Continue loads the newest.
  - Results rows read "Done" / "Missed": the UI fonts have no ✓ or ✗.
  - Engine (upstreamable, `bf15e85`): districts name `[[spot]]`s (a place and a facing) for
    missions and life spots; `VehicleInput.grip` adds tyre grip (0..2) for abilities.
- 2026-10-24 — **Radio.** The stations keep their own clock (`Radio.clock`) on the sim's real
  time: game time stops in the pause menu and slows in slow motion, which made the host's song
  jump back every 0.75 s. `AudioScene.ducking` is now decibels (it was 0..1 mapped to
  1 − 0.7·d): the radio bus sits at −6 dB (music volume × 0.501), ducked 8 dB whenever the
  mission runtime shows a subtitle and 12 dB in menus. Talk is laid out by the game
  (`radio.toml` [talk.<id>] rundown, [dj.<id>] lines) as `pav_core::radio::Segment`s on
  `Music.segments`; the host renders them; reports are worked out as they go on air and kept.
  Picks come from a station's seed and slot, never `sim.state.rng` (a test runs the same game
  with the radio on and off and compares the rng). Radio and scanner voices are the engine's
  formant babble (`pav_audio::voice`); mission dialogue keeps its `voice.babble` cue
  (`missions/` was another helper's). Stations "Westside" and "Smooth" so the card reads
  WESTSIDE 99 / SMOOTH 88.8. `radio seek <s>|end` is a check command.

- 2026-10-24 — **Session 4 decisions (M7 missions):**
  - **The Harbor Spur** ends heading west at (105.6, 11, 363) instead of over the water. The
    corridor under rows 61-63 (cols 36-43) is cleared, so a car leaving the broken end lands on
    dirt before the harbor road. h1 Loud's getaway is that jump (`jump` step: airborne at some
    point, then down near the landing).
  - **New anchors and actions for missions:**
    - `deck:x,z` anchors (the Route 404 lane nearest there, at deck height), because the
      heist's block is on the freeway;
    - `pay` (cash to one of the crew) and `armor` actions;
    - vehicle spawn health (the share of a new one's);
    - actors whose ground has not streamed in wait and spawn when it has (s1's guards fell
      through the world).
  - **The crew's bodies and the cars they drive are streaming interest points**, so a switch
    finds them standing.
  - **The wanted autopilot drives like a getaway driver.**
    - Unless told to take it easy (desire below 0.9), it ignores officers on foot, shoves
      stopped cars after 2 s boxed in, routes round lanes it found blocked (new engine
      `route_lanes_avoiding`) and honks people clear (people on the road then step off it).
    - Any pilot slows for sharp lane joints.
    - It passes at no more than 65 km/h. It ran into parked cars on the far side at 88 km/h.
    - The escape bot runs from the map's edge too (within a quarter of the map, at most 250 m).
  - **The storm channel's ramps** got a level landing, 7 m slabs, an apron over the edge and
    lanes curving in from the street (`pav_world`). Pickups fell off the old 5 m ramp and
    overshot its sharp corner.
  - **The mission bot plays like a careful player.** It still acts only through its input:
    - It loses the police before a drive longer than 220 m.
    - It fights from cover: R1 near a car or wall, then peeks and fires (L2/R2) and ducks. It
      prefers foes it can hit (clear from the camera and from the muzzle), carries its
      hardest-hitting gun and uses its fighting ability.
    - It walks round cars (A* on a half-metre grid within 14 m), checks a straight walk at
      knee height too, and drives a car that is off the roads to the nearest road it can reach
      in a straight line (glass excepted).
    - It switches by the wheel with the stick its own: the autopilot no longer writes the
      stick while a wheel is open.
  - **An enemy a mission starts below full health is not "hurt" on spawn.** Awareness compares
    with the health it started with; s2's night shift were all on alert at once.
  - **h1 Smart:** Mack drives out in a second City Works truck. The last leg starts at no stars
    and fails at two (PLAN's "≤ 1★"). With one star the police pull up behind a car stopped at a
    light, and the plan busts only players on foot.
- 2026-10-24 — **Landmarks** (the helper's calls, reviewed at the merge):
  - Luxe Motors' glass is breakable panes: an invisible destructible collider inside a frosted
    frame (the renderer has no transparency). A pane breaks 0.2 s before a vehicle doing
    3 m/s or more reaches it, so the car keeps its speed.
  - The Big Wheel is drawn from the game clock (one turn in 150 s), with no physics.
  - The freighter sits at cols 47-56 and the warehouse at rows 58-60, clear of the spur's flight
    path.
  - Lots fill to 45% within 90 m. Landmark lamps and palms are walk-through. Yards and homes
    are `crowds = false`.
  - Spots take a `y` (decks, roofs), and a vehicle at a landmark's own spot stands exactly on
    it.
- 2026-10-24 — **The store's test counts materials as a floor** (`>= 40`), as it does props and
  clips: the landmarks' chain-link was the 41st.

- 2026-10-25 — **Activities and escapes (session 5):**
  - **Bets** are `choose` options with a `cost`; the race pays double the stake. Activities
    may leave `protagonist` empty (whoever is played) and take a blip icon instead of a letter.
  - **Race:** racers drive through each gate toward the next (no stop at gates); a rubber band
    (lead over 0.75 of a gate: 0.66 of the pace; behind: 1.0; else 0.82); racing pilots pass
    mid-block only and keep their lane within 120 m of a turn or ramp; traffic thinned to 0.1.
  - **Hold-ups** cap the heat at two stars (`heat_cap`), and the getaway Kaze waits at the
    door (a Stallion was slower than the police).
  - **The h1 jump** leaves one unit (heat 1 after it): the broken ramp shakes off the rest.
  - **Busted in a car:** an officer at arm's length of a car held under 0.5 m/s for 4 s at
    ≤ 2★ (GTA's "boxed in"); the Mods counter serves during a mission's play steps.
  - **Police pursuit is paced by the stars:** one star +2.5 m/s over the player (31 m/s at
    most), two +4.5 (36), three and up +7 (42) as before; PITs stay at 3★+. At +7 everywhere
    no car could pull away at one star. A close pursuer that is not ramming takes the
    player's tail (the lead point from alongside steered it into the player's side).
  - **The escape autopilot** drives at 1.25 times the limits (1.6 crashed more, 1.4 lost the
    hold-ups), counts its jams with no goal too (it shoves a stopped or creeping queue aside
    after 2 s), and prefers a turn at a junction with the police within 110 m (35 m of reach
    bonus). The evasion bot keeps a damaged car with the police within 15 m (getting out was
    the arrest) and runs from officers on foot within 10 m before taking a car.
  - **The player's car pushed into people at under 3 m/s shoves them aside** (no injury, no
    crime): officers standing at a bumper made a car immovable.
  - **Vigilante:** five levels (1-2 cars, 1-2 suspects, armed from level 4, 300-420 s each);
    $250 a takedown, $1,000 for the last level and completion. On duty a civilian killed docks
    $250 instead of a report (the police do not come after one of their own; arrests had
    ended runs over collateral). A getaway car `escape` m off lies low instead of ending the
    run; the level's clock is the pressure. 60 s out of a police car (300 s between levels);
    a wrecked police car is replaced by a spare at the kerb (3 a run); out of it, its blip and
    the GPS point back to it. No mission is picked up while vigilante or an event is on.
  - **The bots:** on foot they route round vehicles to the first waypoint clear of them (the
    walk map runs under parked cars), jump every third time they are pinned, leave cover and
    a crouch before walking off, and go round a car's bumper only when close to it; off-road
    hand driving is for slow cars, held until back within 5 m of a lane; a burning or wrecked
    car is left.
- 2026-10-25 — **Water, heat and the bots (session 5, later):**
  - **Heat clears out of sight anywhere, at half rate inside the search zone** (PLAN §5.6.5):
    5 + 5 × stars seconds unseen outside it, twice that inside. Tests that need a chase raise the
    stars again once the police have come (`heat.rs`, `replay.rs`).
  - **Water.** A car that floods (water over 0.9 of its height) puts its driver out to swim; a
    flooded police car counts as wrecked for vigilante (a spare comes). The bots swim for the
    nearest walkable ground and climb out.
  - **Roads that end at the water close like the map's edge** (engine, `pav_world`): their
    turnaround is pulled back 16 m and DEAD END barriers stand across them. A police car in a
    chase had driven off the avenue's quay end into the sea.
  - **Busted:** getting in or out of a car starts the officer's count again; on foot, staying
    within a metre for 0.7 s counts as standing still (pinned between officers, the run cycle's
    speed hid it).
  - **The evasion bot** remembers the 24 lanes it drove last (a way back onto one costs 150 m of
    reach: it circled blocks), runs from a unit within 140 m while they search, gets out of a car
    crawling under 1.5 m/s at ≤ 2★ with an officer within 8 m (before they reach the door),
    keeps to a car a mission's `abandoned` condition names however damaged (s3's Kaze), and looks
    120 m for a car when none is within 40 m.
  - **The escape autopilot's haste by road:** 1.7 × the limit on the freeway, 1.55 on boulevards
    and avenues, 1.3 on ramps, 1.4 elsewhere (the turns still at a safe speed). This replaces the
    flat 1.25. Making for the freeway after 12 s with a unit on its tail was tried and dropped:
    it found short stretches and shuttled.
  - **Police coming the other way at ≤ 2★** pull over to their own side at 3 m/s until the car
    is past, then turn and give chase (they drove head-on into it).
  - **A car a mission placed across the road** (kept loaded, an end within 1.5 m of another lane's
    middle: h1's City Works truck on Route 404) is in the way on every lane it stands in, so
    traffic and the armoured truck stop at it. Other cars (wrecks, cars knocked about) count on
    their own lane only, so traffic gets round them instead of queueing for good, and the player's car
    on autopilot sees even a placed one on its own lane only and noses round it as a player
    would. Counting every stray car on every lane lost vigilante 0/3: the bot's rammed wrecks
    blocked its way in both lanes.
  - **Racers** move out to pass a slower car within 40 m (under 0.92 of their pace), with the
    room measured to the route's next turn.
  - **Kept bodies wait for their ground (engine, `pav_core`).** A `Keep` body (a mission's car or
    actor) over ground that has not streamed in holds still without gravity until it has; a kept
    person on foot there is not stepped. Mission actors had dropped to −17 m and a guard to −39 m
    (`pav_core/tests/regions.rs`).
  - **Vigilante's people and cars go back to the world** once their level is over (they go with
    their ground); those down are taken away out of sight or once under the ground. Dispatch
    looks wider (30-192 m, any height) after 6 s with nowhere to put the suspects (the unit up on
    the freeway).
  - **Mission tests retry like a player:** after a fail, ✕ Retry up to twice (never Skip); the
    line printed says how many.
  - **The 3★ test counts escapes over chases:** seeds 1-8 until five chases (a seed where no unit
    finds the car within 2 minutes is no chase); at least three escape.
  - Smaller: a stick tilted 0.55 or less walks (it jogged); a refused save says "Can't load this
    save"; traffic-light lenses are dark when unlit (they glowed at night in every colour).
- 2026-10-25 — **Bodies and looks** (the looks helper's calls, reviewed at the merge):
  - A look's new choices (hair, beard, accessories) come from `Rng::fork` (a stream of its own
    from the generator's state): each look draws from the main stream exactly what it did
    before, so the bots' runs keep their outcomes.
  - The new puppet fields are left out of a look's serialization while unset: older looks and
    saves keep their bytes. The new helmet kinds come after Hood, so Shardfall's loot helms keep
    their indices. Far-off people leave out parts under a pixel across (every game; orthographic
    views draw everything).
  - Helicopter sirens are a flashing light bar only: no wail, and traffic does not pull over for
    one. The air unit lights its lamps at night and its bar while it pursues.
  - A protagonist's `cap` is a baseball cap. Beards are a quarter as likely with long, bun or
    ponytail hair.
- 2026-10-25 — **M8: the frontend and the attract loop.**
  - **Attract corner:** the UI kit's attract layout puts the logo top left with the caption
    under it (PLAN draws it bottom left). "DEMO" leads the caption; "PRESS ANY BUTTON" breathes
    at the bottom right without a button glyph (as a hint it showed the on-foot map's ✕ key,
    SHIFT, in the browser). The FPS readout is hidden while the loop plays: it sat on the
    logo.
  - **What counts as a stall:** a bot without 4 m of headway for 6 s, unless something else is
    playing it: a light or a queue the autopilot waits in, a random event's own bot, a switch's
    sky-cam, a mission's cutscene, aiming in a fight, the aftermath of a fight won or a chase
    shaken off (5 s). A vignette may set its own (the chase: 10 s, backing out of tight spots).
    A cruise's `goal` must be reached (missing it is an anomaly); `toward` only heads somewhere
    (the autopilot keeps to planned roads: cruising with no goal once ran it into a closed
    lane's end).
  - **Vignettes end early** when their goal is reached, the stunt run is over, the fight is won
    or the chase shaken off (5 s later), the mission is no longer in play, or the protagonist is
    wasted or busted (3 s later). A chase is also over when it is lost: the protagonist out of
    the car, or the car boxed in for 6 s with the police within 30 m (the soak's last two
    stalls were these). Only a stall with nobody to blame counts as an anomaly.
  - **The soak's flatness** sums the vignettes' entity counts per complete loop and holds every
    loop within 10% of the first: one sample of a vignette varies with the traffic round it
    (the switch saw 48 then 58), a leak grows loop after loop.
  - **The heist excerpt starts at the jump** (`mission skip 5`): Mack at the truck at 4★, the
    sky-cam to Dez in the Okami Kaze, the broken ramp, losing the police. Skipping four
    checkpoints from the start left the truck and the bags 130 m apart once in five (skip is
    built for one checkpoint after a real fail).
  - **The dock fight** is in the open yard south of Harbor Gate Freight's warehouse (on Harbor
    Gate Road the gang stood behind the warehouse wall), with eight gang members who know where
    Mack is (`enemies alert`).
  - **Quality presets:** Low, Medium and High are the host's presets; Ultra is the host's High
    with 1.2× the traffic and crowds (Low 0.55, Medium 0.8, High 1.0). Until the player changes
    a setting the host keeps its own (phones and tablets start at Medium with dynamic
    resolution).
  - **Settings pages:** five (AUDIO, DISPLAY, HUD, CONTROLS, ACCESSIBILITY) so none passes seven
    items; "camera recentres" moved to Accessibility.
  - **Title shot:** from the Mirador Hills over Los Mirados toward downtown's towers, drifting
    east. The drone over Route 404's north end put a bare slope across half the picture. The
    clock holds at golden hour while the menus are up.
  - **The main menu was never drawn:** its card had asked for the attract corner layout
    (`small_logo`), which draws no menu, since M1; it uses the kit's title-with-menu layout now.
  - **Bodies are not solid** (`Sim::set_solid`): a fallen standing body stopped bullets and
    sight lines (the dock fight's bot stood before one, unable to shoot past it).
  - **Rumble:** the player's crashes (by impulse) and blasts near them; off with Vibration, and
    never in the attract loop.
  - **About** is five pages (GAME, SOURCE, ENGINE, MADE BY, CREDITS) so each keeps to six rows;
    "→" reads "then" (the UI font has no arrow).
  - **The UI Gallery** left the player's menus; `freeway/gallery` and the Proving Grounds keep
    it.
  - **Save from the pause menu** within 30 m of the protagonist's bed, on no mission and with no
    stars (the bed's own menu opens).
  - **Loading cards** hold the world still behind them (a still picture under the filter);
    the ground streams in at once when the card starts.

- 2026-10-25 — **M9, gameplay and city (first batch):**
  - **Traffic minds the player's driving in calm free roam only** (`Freeway::traffic_minds`:
    no mission, street job, attract loop or stars). There, a driver the player runs into gets
    out (40% square up, 35% film, 25% leave; never a mission's car, a bus or a pilot), and a
    near miss (the meter's own test: another vehicle within an arm's length at a closing speed
    over 12 m/s) makes a calm driver stop short (1.2 s) or swerve away with the horn, half and
    half. In `dev_test`'s race a driver standing in the road was run over and the bot busted.
  - **The bat** takes the fists' slot once owned (the wheel keeps eight slots): weapons.toml
    lists it after the eight (`combat::BAT`), Bang Bang Bros. sells it for $100, the HUD and the
    wheel name it. **Heavy melee:** ○ held 0.45 s after the jab swings `heavy` once a hold (the
    fists' Melee_Hook): 2.2× damage, 2.6× knock-back, 0.3 m more reach, lands at 0.36 s.
  - **Vault:** running (over 3 m/s, on the ground) at something between knee and chest height
    (0.5-1.4 m rays) presses jump; the ledge grab (already on) climbs what the jump falls short
    of. People are taller than the chest ray, so the player never vaults a person.
  - **Scenario points** are data (`peds.toml` `[[scenario]]`): benches (three seats), the food
    stalls (eating), beach umbrellas (meditating, asleep), the volleyball net (cheering),
    picnic and café tables; 30% of lingering people pick the nearest free one within 30 m.
    Which clip they play there goes by who they are, not by the sim's random generator: one
    more draw per arrival reshuffled every chase after it, and the 3★ evasion test fell from
    3+ of 5 to 0 of 5 (the bot is sensitive to the order of events; Open issues).
  - **Sirens:** L3 tapped in a police car cycles wail, yelp, off; a patrol's siren yelps within
    30 m of the listener (closing in) and wails further out.
  - **Rounds** burst the tyre they hit (within its radius + 0.12 m) or break the lamp (within
    0.3 m); traffic drives with lights on after dark and in rain, storm and fog, as the player.
  - **Camera shake** (honouring Reduce camera shake): 0.18 a shot, the rumble's low motor on
    crashes and blasts, dying away at e^-6t. **Reverb** by place: indoors 0.45, under a deck or
    roof 0.32, in the Ditch 0.3, downtown 0.14, else 0.05.
  - **Sky-cam:** 300 m up, 3.0 s in all (0.9 rise, 1.1 cross, 1.0 drop), with a map grid every
    50 m 60 m over the ground and the picture greyed while it is high (PLAN §5.6.2).
  - **Fell through:** ambient people below -30 m are removed by the per-tick sweep (the soaks
    found a few in the switch vignette each 30 minutes; the cause was not reproduced in two
    traced loops).
- 2026-10-25 — **M9, engine (first batch):**
  - `SimEvent::Fx` was declared (E4) but nothing drew it: `pav_view::fx::event_bursts` now
    draws a game's preset bursts (a dozen particles at scale 1), and a `splash` preset makes
    the game's rain splashes (positions from a hash of the tick, not the sim's generator).
  - **Auto quality is Low on a software adapter** (SwiftShader, lavapipe, WARP: `DeviceType::Cpu`)
    with a quarter-megapixel render target and dynamic resolution; the browser check's title
    drew a frame every few seconds at High.
  - `gtf perf` reports and budgets shadowed lights (≤ 4), particles (≤ 30,000) and entities
    (≤ 450) beside the old figures (PLAN §6.9).
  - Neon tubes are thicker (11 cm on landmarks, 5.5 cm half-depth on shop bands) and the tube
    that lights the wall glows 1.6 (others stay at 0.5: above it a glow becomes a light). Road
    paint is a worn white (#d2cec2); palm fronds a deeper green.
  - **Fall damage:** landing faster than 12 m/s (a drop of about 2.3 m: characters fall at
    32 m/s²) costs 12 health per m/s beyond — a 5 m drop about 70, the fastest fall (26 m/s,
    from 10.6 m up) 168 of 200; not into water, onto a ledge or a ladder, or in a seat.
  - **First-time tips** (`tips.rs`): the first time the player is wanted, swims, holds a gun,
    has a full ability meter or stands unarmed a while, the help box says what to do, once a
    game (the save keeps which were shown).
  - **The HTML's header comment** carries the Source Information block
    (`scripts/gtf-source-information.txt`, `pack_single_html.py --header`).

## Open issues
- **The evasion bot is sensitive to the order of events:** one extra random draw elsewhere
  reshuffled the chases and the 3★ test fell to 0 of 5 (the getaway car boxed in by cars pulled
  over for the sirens, then shot at). It passes as committed; a sturdier unstick (shove through,
  bail out sooner) would make it robust.
- **SwiftShader draws the browser build at 0.1 frames a second** (frame 9-16 s at 960x540, even
  at Low on the software adapter); the sim keeps to 0.1-4.5 ms a tick. The browser checks
  wait on log lines with long timeouts; a real GPU is unaffected.
- **VFX not done:** exhaust puffs, dust trails on dirt, heat haze on the freeway at noon
  (PLAN §5.11's list; the rest are in).
- Roam soak (900 s, seed 1): one car gave up stuck at a dead-end keyhole turnaround (limit 0);
  roam AI crashes 0-3 per 900 s with buses turning at junctions the most common. M9 polish.
  (Since session 5 the avenue's quay end is closed off like the map's edge; re-check in M9's
  soaks.)
- Evasion bot at 3★ (`evasion_rate`, session 5): rolling 8/8, from a standstill 4/8 (pinned by
  a PIT, or caught getting going). The 3★ test passes. M9: fast-road routing from a standstill.
- Vigilante: getaway cars take turns at 1.375 times the safe speed (only the autopilot and cars
  with a goal are held to it), so on a winding route the bot rarely closes in and a long chase
  is what costs a run (seeds 1-3 win since the placed-car rule; seed 6 of 1-8 ran out level 3's
  clock in session 5's first measure). M9: hold runners to the turn speed too, or ram sooner.
- ~~**Free roam's tip while wanted**~~ fixed in M9: free roam's objective reads "Lose the
  cops." while wanted, and the first time the police are after you a tip says how.
- **Art notes from the landmark captures (M9 art pass):** at night the palms' fronds glow
  lime green as if lit, the crosswalk paint reads near white, and the shop signs' neon barely
  shows; traffic drives without headlights (Remaining work).
- **Helicopters (the looks helper):** the canopy is opaque, so its crew cannot be seen; the
  searchlight's spot on the ground was not identified in a capture (only its origin moved to
  the body's lamp). (Fixed in M9: the helicopter's own control help replaces a car's.)
- ~~**The UI Gallery** in the player's menus~~ fixed in M8 (`freeway/gallery` only).
- DualSense button mapping through gilrs on the user's Windows machine (a user check).
- Dense traffic can lock a junction box into a queue that never clears (why missions thin
  traffic). M7/M9: give way by arrival order inside the box, or let the tail re-route.
- `dev_test` takes the bot about 395 s of game time (long drive legs at a safe pace); the whole
  missions suite runs in about a minute in the dev build. M7's missions should keep drive legs
  short or use `traffic` thinning.
- **Perf depends on the container.** `gtf perf view=downtown_street` (release) builds a frame
  in 4.7-5.4 ms (budget 4; outliers to 7) and runs a tick in 2.8-3.6 ms (budget 3) on session
  3's container. A release build of the last commit before M6 (`fbe56bf`) gave the same numbers
  there, so the cause was the machine, not a regression. On session 4's container, `perf
  view=all` passes every view: downtown builds a frame in 3.6-3.9 ms and ticks in 2.0-2.1 ms.
  Judge perf changes by A/B against a baseline build on the same machine. D12 needs margin:
  M9's performance pass should bring downtown well under 4 ms on any of these containers.
- **Earlier evidence is not on disk.** `out/` is ignored by Git and containers reset, so the
  M0-M6 captures, filmstrips and soak JSON cited above are gone. M10's final verification
  re-runs the D1-D13/D16 commands and looks at fresh captures.
- ~~**About screen wording**~~ fixed in M8: the credits name the motion libraries (and, since
  M9, the CMU database). (The billboard now reads
  FREEWAY FM 104.04 and the Big Wheel exists: both fixed by the landmarks merge.)
- **Bot test time:** each story mission takes the bot 1-5 minutes of game time, so the
  `story_*` tests are ignored (run them in release, in parallel).

## Log
- 2026-10-17 — Planning: engine studied at `0098196`; measured starter bot (3/3 wins), Drift Circuit
  6,886 ticks/s, pavilion world 649 ticks/s with 8,200 instances / 2,025 draw calls / 483k triangles /
  64 lights; single-file web build 3 min 06 s → 11.2 MB, boots in headless Chromium with the egui HUD
  visible; `scripts/vendor-engine.sh` tested on a throwaway clone (1,359 files; starter tests 4/4
  pass in the vendored tree). PLAN.md written.
- 2026-10-17 — Independent plan review (27 findings) verified and applied; `scripts/web-check.mjs`
  gained ordered `until:TEXT:TIMEOUT` steps (tested: a pass and a timeout failure).
- 2026-10-17 — **M0 done.** Engine vendored at `0098196` (`689142b`); engine baseline
  `cargo test --workspace`: 66 suites, 455 passed, 0 failed, 2 ignored. E1 (`9bd019a`): game and
  tool registries, `pav_tools::cli::{Cli, run}`, `pav_app::main_with(AppConfig)`. `gtf_game` +
  `gtf_app` (`2bf7495`): `cargo test -p gtf_game` 2/2 (every `freeway*` scene builds and steps;
  snapshot/rewind/replay exact). `gtf capture` of the lot looked at (`out/m0_lot.png`). Native
  window under Xvfb + lavapipe (`scripts/smoke-gtf.sh 15`): boot complete in 4.2 s, sim 60 tps,
  egui HUD and the lot visible (`out/smoke-gtf/screen.png`). Single-file web build
  (`scripts/build-gtf-web.sh`, 3 min 37 s): `dist/GrandTheftFreeway-BX.html` 11.2 MB (wasm 27.9 MB,
  8.3 MB gzipped); headless Chromium (`web-check.mjs --expect "boot complete" --expect "scene
  'freeway'"`): boots in 0.6 s, scene `freeway`, HUD visible (`out/web-m0/start.png`).
- 2026-10-17 — **M1 done.** E2/E3/E4/E6 (`40772ce`); E8 part 1, E12, E13, E14 (`b5de689`);
  `gtf_game` frontend, orbit camera, weapon wheel, settings, UI gallery (`7686717`); E5 `pav_ui`
  (10-foot kit on egui: Anton + Barlow Semi Condensed under OFL, procedural pad glyphs per device,
  HUD, radar and map from the `MapImage`, screens, moments, weapon wheel, phone, offscreen render)
  wired into `pav_app` (overlay, pointer → `ui.*`, gear, FPS readout) and `capture hud=true`.
  Tests: `cargo test -p pav_core -p pav_world -p pav_ui -p pav_view -p gtf_game` 360 passed,
  `cargo test -p pav_tools --lib --test ui_hud --test asset_errors` 54 passed; clippy clean.
  Captures looked at: `gtf` REPL `load scene=freeway/gallery` + `capture hud=true` at 1920×1080 and
  960×540 (`out/m1/gallery_*.png`: title, menu, map, HUD, cards — legible at both sizes). Native
  window (`scripts/smoke-gtf.sh 25 --scene freeway/roam`, Xvfb + lavapipe): boot complete in
  1.5 s, the FPS readout (software rendering, so red), money, radar with the city map, vitals,
  objective with a keycap glyph, the generated city (`out/smoke-gtf/screen.png`).
- 2026-10-17 — **M2 done.** E14 regions + `pav_world` (terrain, Route 404 + ramps, The Ditch, lane
  graph, signals, walk tiles, cover, dressing with `LIB/gtf_city`), Paloma Bay + the Proving
  Grounds, E7 merged (`97bfc96`: sky, weather, night lights, facade windows, static culling),
  facades by style (`609dcab`), the game's look in `world.toml` (`44ab6af`), `gtf lanes` + the
  city determinism test (`58af7f7`), the web pointer-lock crash fixed (`832b694`). Acceptance:
  city digest identical over two builds (1152 chunks); lane network one strongly connected
  piece; `freeway/tour` 10.6 km at 40 m/s with no holes, <= 146 active / 0 dormant chunks
  (filmstrip `out/m2/tour_filmstrip.png`, golden hour into dusk); `hills_vista` shows the
  district; `gtf perf view=all` within budget (downtown 1.51 M triangles); every view at 11:00,
  18:10 and 22:30 (`out/m2/sheet1.png`, `sheet2.png`; changed after looking: the docks view's
  eye, noon's contrast and smog line, the lit/ACES look); the web build boots into the city with
  the HUD (`out/web-m2/fixed_*`). Tests: pav_world, gtf_game, pav_tools store + store_rooms,
  pav_render + pav_view (46), WGSL in Tint; clippy clean on the touched crates. Carried: street
  lamps are dark at night (props need a night glow), mesh LOD (E7 helper); pier deck and Big Wheel
  (M7).
- 2026-10-17 — **Session 1 ends (handoff).** After M2: on-foot camera (`600975f`), radio skeleton
  (`1da78ce`), pedestrians + E11 low detail (`44f379d`), ambience (`fcf0e72`), on-foot combat
  (`ab0af1d`), E9 vehicles v2 merged (`06b7a62`), E7 night glow + mesh detail merged (`cbff7b1`).
  `HANDOFF.md` lists the state, the next steps in value order and the known issues.
- 2026-10-17/18 — **Session 2: M3, M4, M5 done.** Driving (`146240c`), traffic and the soak
  tools (`f554c62`), `LIB/gtf_vehicles` (`a8fa7f5`), heat, police, Wasted and Busted
  (`d63b142`), cover and grenades (`ce2281e`), WORKSHOP clips (`87bf78b`), the police and the
  evasion bot (`f27969e`), armed opponents (`ad6e362`), clips wired, drive-by, respray
  (`f0d5e2e`), `LIB/gtf_weapons` (`e889af5`), guns in hand (`e76ff72`, `2f8599a`), the map's
  edge, turnarounds and the looping freeway end (`4e8de11`, `3b1bb09`, keyholes in `fec29af`),
  population that never falls through the world and safer traffic (`5d15c3c`), view build time
  (`pav_view` mesh tally, far wheels), the camera's shoulder out of walls. The roam soak found
  every fall-through and most traffic faults: a heli wrecked on a tower falling through an
  unloaded chunk, wrecks with drivers that never slept, shaken cars kept on physics off the
  loaded area, parked cars straddling a chunk border, police rails cutting through houses,
  U-turns bulging into buildings, kinematic cars launching others, traffic flowing through the
  freeway's barrier. Evidence as in the milestone entries above; captures in `out/m3`,
  `out/m5`; soak JSON in `out/soak`.
- 2026-10-18 — **M6 done; session 2 ends (handoff).** Engine: district spots and
  `VehicleInput.grip` (`bf15e85`); game: missions (DSL, validator, runtime, bot), protagonists,
  phone, shops and saves (`e1d2832`); the tests, `docs/gtf/MISSIONS.md` and the fixes made
  after looking at the captures (this commit). Evidence in the M6 milestone entry; captures in
  `out/m6/` (not committed). `HANDOFF.md` rewritten for the next session (M7).
- 2026-10-18/19 — **Session 3: environment check** (no code changes). The container was ready:
  toolchain 1.98.1 with the wasm32 and windows-gnu targets, wasm-bindgen 0.2.129, the
  SessionStart hook's packages and debug `pav`/`gtf` built, lavapipe, mingw + Wine (a
  cross-built `.exe` runs), Node 22 + Playwright + Chromium (`scripts/check-wgsl.sh DIR` compiles
  WGSL in Tint), the shardfall MCP server connected. `cargo fmt --all --check` clean;
  `cargo test -p gtf_game` 98 passed, 8 ignored (3 min 40 s); host clippy `-D warnings` clean for
  `gtf_game` and `gtf_app`; wasm clippy fails (Open issues); release `gtf` built cold in 9 min
  51 s; captures of `freeway/dev`, `freeway/roam` and `freeway/view/downtown_street` with the HUD
  looked at; perf as in Open issues. Notes for later sessions: the MCP server's
  `cargo run -p pav_tools --bin pav` resolves features differently from the hook's combined
  `pav` + `gtf` build, so it compiles the dependency graph a second time at startup (a few
  minutes, harmless). A worktree must not share the main tree's target dir: path crates hash
  relative to the workspace root and `pav_core`'s build script bakes in `CARGO_MANIFEST_DIR`, so
  the worktree's build overwrites the main tree's artifacts. The fix is `cargo clean --release -p`
  for the ten workspace crates, then a rebuild.
- 2026-10-24 — **Session 4: environment check and gap inventory** (no game code changed).
  Toolchain 1.98.1 with the wasm32 and windows-gnu targets, wasm-bindgen 0.2.129, lavapipe
  (llvmpipe, LLVM 20), mingw + Wine 9.0 (a cross-built `.exe` runs), Node 22 with Playwright
  1.63.0 and Chromium 153 (a WebGPU adapter with `web-check.mjs`'s flags), Xvfb, ImageMagick,
  25 GB of disk free. `cargo fmt --all --check` was clean. Wasm clippy fails on the same three
  `pav_tools` lines (Open issues). The release `gtf` built cold in 6 min 33 s. `gtf perf
  view=all` passes all ten views (Now). The debug `gtf` captured `freeway/dev` with the HUD,
  and the capture was looked at: the Proving Grounds' signal station sign, traffic, the
  minimap and the prompts. The shardfall MCP server timed out at startup, so `.mcp.json` now
  runs `gtf mcp` (Decisions). Two read-only audits of the code against PLAN §2, §5 and §9 gave
  **Remaining work**. `HANDOFF.md` now has a shorter `/goal` prompt.
- 2026-10-24 — **Radio (M7, D10 part).** Per-genre music, idents, SPNN's program with
  heat-aware traffic, DJ cards, scanner chatter and the mix (commits `63c64c0`…`b9593dd`).
  Listened with `gtf audio_capture` in a car at 60 s into each station (`radio seek 60`), SPNN
  from its sounder, a DJ break and two stars of scanner: in-band tempo equals each song's
  (107, 92, 161, 95, 73 BPM; raw autocorrelation 184 and 80.5 for boom-bap and surf before
  folding), swing boom-bap 1.82 on sixteenths and smooth 1.91 on eighths against 1.00 for the
  straight genres, the radio's own spectral centroid 639-2065 Hz by station (surf was 5.6 kHz
  until its snare got a band-passed crack), SPNN's beat strength 0.06 (music 0.46-0.88) at
  4.8 syllables/s; peaks ≤ 0.32, no clipping. HUD captures looked at: SPNN's and dispatch's
  subtitles, the DJ card (its line now wraps in the corner instead of running into the
  objective). Cost: 2.5-4.3% of a core per station, 1.0% for talk (native release).
- 2026-10-24 — **The story missions (M7).**
  - **Written and passing:** m1-m3, s1-s3, h0 and h1 in both approaches. Each was traced with
    the bot step by step until it won on seeds 1-3. Commits: `d1c6300` engine (ramps, avoiding
    routes, `play seeds cmd`), `05f651c` game, `dfb3ecf` content, `67ab73c` docs.
  - **Merges:** the radio branch (`d9e3f7c`; 585 tests green on the merged crates) and the
    landmarks branch (`2451340`; conflicts in `views.toml` and `anchor.rs` resolved by keeping
    both sides).
  - **Re-tested after the merges:** the landmarks changed the ground under several missions:
    - the diner lot's low walls;
    - cars parked at the warehouse;
    - s2's yard, which now really exists;
    - s3's showroom.
  - **Fixes:** the bot learned to walk round cars, see low walls, leave cover and drive out of
    yards; s2 moved into the yard; s3 now leaves through Luxe's window. Commits `123445d`,
    `753bff6`, `c2d0a77`, `e8b34e5`.
  - **Store index** regenerated twice: for `LIB/gtf_weapons` and the WORKSHOP clips, then the
    landmarks' 30 props and a material.
  - **Disk:** the 38 GB allowance ran out once while a test build linked. The finished helpers'
    worktree `target/` dirs and stale test executables were deleted. The landmarks helper's
    baseline binary was deleted with them (it lived in the shared scratchpad), so its last perf
    A/B is best-of-2.
- 2026-10-24/25 — **Activities (M7).** The stunt jumps with district `[[prop]]`s (`3fa1a6c`,
  `e461db2`, `bd65cb0`), the Lost Shields and the gold Kaze (`dcfbf94`, `8ad20f1`), the race
  and the hold-ups as missions (`1ffb533`, `862400c`, `ca1dfef`), vigilante and two random
  events (`93a8a2d`, `bbed66b`), their bot tests (`c56a858`), the STATS card (`bd3bbf9`).
  Evidence in Remaining work → Activities.
- 2026-10-25 — **The §7.8 suites and the landmark captures** (`c2f3fa5`; fixes after looking:
  `01bbca8`, `4a9f866`, `7403f2d`), and **bodies and looks** merged from the looks helper
  (`a58c775`: `dc0ea9b`, `78b1b3f`, `14730f2`).
- 2026-10-25 — **M7 done.** The acceptance run on the merged tree found what it was for:
  - **Fixes:** kept bodies hold still over ground not yet streamed in (`6108b09`, engine);
    a placed car blocks every lane it stands in, the autopilot reads its own copy of the lanes,
    vigilante's spawn band widens when nothing fits (`e3df80a`); a mission started away from
    its scene streams the ground in before putting the player down, and a waiting driver is
    seated when its car wakes (`bd2b6db`).
  - **Rates at the final traffic rule** (release, seeds 1-8): h1, s3, the grind, the gas
    station and the race 8/8 each; evasion rolling 8/8 and standing 5/8; vigilante 3/3 on
    seeds 1-3.
  - **Acceptance** as in the milestone entry: 27/27 story runs, 9/9 activity missions, 9/9
    street activities, the whole game by the bots to 98.0%.
  - **Workspace tests (release, `bd2b6db`):** run crate by crate (`cargo test --workspace`
    resolves features again and duplicates every artifact, more than the disk allowance holds):
    pav_audio, pav_ui, pav_render, pav_world, pav_app and gtf_app together, pav_tools's lib
    and 22 suites one at a time (their binaries removed after each): 202 passed, 0 failed, 6
    ignored; pav_core 313 passed; gtf_game 147 passed, 25 ignored (the long bot suites, run
    above); pav_view 47 passed, 1 ignored. `cargo clippy -p pav_app -p gtf_app --target
    wasm32-unknown-unknown -- -D warnings` clean.
- 2026-10-25 — **M8 done** (`5185f23`, `725c06c`, `dad4cd7`, and the fixes in `b77fe2e`): the
  title, menus, settings, About, intro and loading cards, the attract director with its seven
  vignettes, touch = attract only. Acceptance in the milestone entry: `frontend.rs` 16,
  `attract.rs` 8, 30-minute attract soaks clean on seeds 1 and 2, the desktop and phone browser
  flows. The soaks found two pursuit stalls (boxed in; out of the car) and a switch vignette
  whose entity count wandered between plays: a lost chase now ends the vignette, and the soak
  judges entities per complete loop.
- 2026-10-25 — **M9, first batch** (`6d0797b`, `589bca1`, `3365175`, `b77fe2e`, `2164b80`;
  Decisions → M9). Checks before the push (release): gtf_game 171 passed, 0 failed, 26
  ignored (the long bot suites); pav_app, pav_view, pav_ui, pav_world and pav_render 109
  passed, 0 failed; pav_tools's lib and 22 suites one at a time 119 passed, 0 failed; clippy
  `-D warnings` clean on every touched crate; fmt clean. The scenario points' first version
  drew one more random number per arrival and the 3★ evasion test fell to 0 of 5: the clip
  now goes by who sits there.
