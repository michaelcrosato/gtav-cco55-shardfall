# Missions — the DSL reference

Missions are TOML files in `content/freeway/missions/<key>.toml`, listed in
`crates/gtf_game/src/missions/mod.rs` (`FILES`, embedded with `include_str!`; running from the
repository the file on disk wins, so edits show at the next `load`). The runtime is
`missions/run.rs`, the bot that plays them `missions/bot.rs`. The two Proving Grounds test
missions (`dev_test`, `dev_test2`) use every step kind and are the worked examples.

## Checking and playing

| What | Command |
|---|---|
| Validate every mission (file:line errors) | `cargo test -p gtf_game --test missions every_mission_validates` |
| Play one straight away | `gtf` REPL: `load scene=freeway/mission/<key>` (or `play cmd="mission start <key>"`) |
| Let the bot finish it, 3 seeds | `gtf` REPL: `load scene=freeway/mission/<key>` then `play bot=true seeds=3 ticks=36000` |
| Watch the bot step by step | `GTF_SCENE=freeway/mission/<key> GTF_STEP=<n> cargo test -p gtf_game --test missions bot_trace -- --ignored --nocapture` |
| Other commands | `mission quit`, `mission fail` (tests), `mission choose <option>` (the bot's pick on boards), `mission replay <key>` |

## The file

```toml
key = "m1_repo_man"            # = the file name
title = "Repo Man"
protagonist = "dez"            # who plays it (switched to at the start)
district = ""                  # "" Paloma Bay, "dev" the Proving Grounds
text = "Steal back a Stallion."  # the phone's Jobs app and the replay list
contact = "mack"               # calling them on the phone starts it (optional)
blip = { letter = "D", at = "spot:los_mirados_driveway" }   # the map letter and the marker
start = "at:-30,-26"           # where the player stands when it is played straight away
unlock = ["m0_intro"]          # missions to pass first
time = { at = "17:30", lock = true }   # the clock (held while it runs)
weather = "clear"
reward = { cash = 1500, unlock = ["switch"], text = ["mack", "Nice work, kid."] }
names = { owner = "HAL" }      # subtitle names for speakers that are not protagonists

[[medals]]                     # one goal each; all met = gold, half = silver, else bronze
id = "fast"
text = "Deliver within 4:00"
time_under = 240.0             # or vehicle_health_over = { actor, value }, health_over,
                               # headshots, accuracy_over, undetected = true, no_heat = true, flag

[[spawn]] ...                  # actors
[[step]] ...                   # the steps, in order
[[fail]] ...                   # fail conditions for the whole mission
```

## Anchors (places)

`spot:<name>` (a district's `[[spot]]`), `landmark:<key>` (its door), `landmark:<key>:centre`,
`landmark:<key>:<point>` (the spot `<key>_<point>`), `at:<x>,<z>` (world metres, exact),
`lane_near:<x>,<z>` (the nearest road lane there), `player`, `actor:<id>` (an actor; a
protagonist's key names their body). People are put on the nearest walkable ground; vehicles on
the nearest lane, along it (facing the spot's `yaw` when it has one).

## Actors (`[[spawn]]`)

```toml
[[spawn]]
id = "repo_car"
vehicle = "bravura_stallion"   # exactly one of: vehicle, ped (peds.toml id), enemy (enemies.toml
                               # side), protagonist (their body: a parallel step's other half),
                               # pickup (package, cash, evidence: a glowing box to collect)
at = "spot:driveway_3"
yaw = 90.0                     # facing, degrees from north (over the anchor's)
paint = 1                      # vehicles: index into the archetype's colours
locked = false
driver = "owner"               # vehicles: the person actor at the wheel
weapon = "p9"                  # people and enemies
behaviour = "idle"             # see below
health = 40.0                  # people: hit points; vehicles: not used
blip = "target"                # target, enemy, ally, vehicle, pickup
name = "Stallion"              # the marker's label
later = true                   # made by a `spawn` action, not at the start
```

Behaviours (`behaviour = ...`, or the `behaviour` action): `idle`, `wander` (vehicles: cruise),
`flee` (people run, vehicles drive away from the player), `chase_player` (people run at the
player and punch; vehicles drive at the player), `follow_player`, `fight` / `guard` (enemies in
the fight / at their post until alerted), `cower`, `goto:<anchor>` (people walk, vehicles drive
there and stop: a follow step's target). Vehicles need a `driver` to move.

## Steps (`[[step]]`)

Every step may have `objective` (bottom-centre text; `~y~ ~b~ ~r~ ~g~` colour words, `~s~` resets,
`~btn:down~` draws a glyph), `help` (a tip, top left), `label` (for `branch`), `checkpoint = true`
(Retry comes back here), `on_start` / `on_done` (actions), `switch = ["roxy"]` (whom the wheel may
pick during it) and `fail` (fail conditions for this step only; `timer` counts from the step's
start and shows a countdown).

| Kind | Needs | Done when | The bot |
|---|---|---|---|
| `cutscene` | `shots` (`anchor`, `offset` [x right, y up, z ahead of the anchor], `to` (dolly), `look`, `look_offset`, `fov`, `seconds`), `lines` | the shots and lines are over (✕ skips if `skip`, default true) | waits |
| `dialogue` | `lines` (`who`, `text`, `seconds`) | the lines are over; the player keeps control | carries on |
| `wait` | `seconds` (3) | the time is up | waits |
| `set` | `on_start` actions | at once | — |
| `goto` | `target`, `radius` (2.5 on foot, 6 in a car) | the player is that near | walks (drives when in a car); to a vehicle's door |
| `enter_vehicle` | `target` actor | the player sits in it | walks round to the door, △ |
| `drive_to` | `target`, `radius` (7), `vehicle` (optional: that one) | in a vehicle, that near, under 6 m/s | autopilot to the target |
| `follow` | `target` actor (give it `goto:`), `distance` (70), `min` | the target arrives (or `seconds`); fails if farther than `distance` for 5 s or nearer than `min` for 3 s | autopilot after it, slowing when close |
| `chase` | `target` actor, `radius` (2.5), `destroy`, `distance` (220) | caught (or destroyed); fails if they get away | runs or drives at it; shoots with `destroy` |
| `escape` | `pursuers`, `distance` (160) | every pursuer that far (or down) | drives to a road far from them |
| `lose_heat` | `stars` (0) | the wanted level is down to it | the evasion autopilot |
| `kill` | `targets` | all down | picks a gun, closes in, aims (the assist snaps), fires |
| `survive` | `waves` (actor lists, next when the last is down) and/or `seconds` | the time is up, else every wave down | fights the nearest enemy |
| `defend` | `target` (who to keep alive), `waves`, `seconds` | as survive; fails if the target goes down | fights |
| `collect` | `targets` (pickups) | all picked up (walk over them) | the nearest first |
| `photograph` | `targets` | each caught in a phone photo (centred within 12°, near, in sight) | phone up, the camera tile, aims, ✕ |
| `interact` | `target`, `prompt`, `seconds` (1.5), `radius` (2.2) | D-pad right held that long there | walks there, holds → |
| `switch` | `to` (a protagonist) | the player plays them | holds D-pad down toward them |
| `race` | `gates` (anchors), `laps` (1), `racers` (vehicle actors with drivers), `radius` (12) | every gate in order, every lap; fails if a racer finishes first | autopilot gate to gate, in a hurry |
| `stealth` | `target`, `watchers`, `radius` (3) | there without a watcher's meter filling (18 m, 100° cone, half crouched) | crouches, walks |
| `deliver` | `target`, `vehicle` actor, `radius` (7) | that vehicle there and the player out of it | drives there, stops, △ |
| `holdup` | `target` (the clerk), `seconds` (6) | the bag fills while a gun is aimed at them (twice as fast within 4 m) | walks up, aims |
| `choose` | `options` (`id`, `title`, `lines`) | an option is picked: its `id` becomes a flag (kept in the story) | `mission choose <id>`, else the first |
| `branch` | `flag`, `to` (a label), `otherwise` | at once: to `to` when the flag is set | — |
| `parallel` | `steps` (`[[step.steps]]`, each with `who`) | every sub-step done, each by its protagonist (the wheel switches between them) | does its own, then switches |

Parallel sub-steps may be `goto`, `interact`, `kill`, `wait`, `drive_to`, `collect`,
`enter_vehicle` or `photograph`.

## Actions (`on_start`, `on_done`)

`{ spawn = "id" }`, `{ despawn = "id" }`, `{ behaviour = { actor = "id", act = "flee" } }`,
`{ blip = { actor = "id", kind = "enemy" } }`, `{ unblip = "id" }`, `{ heat = 2 }` (0 clears),
`{ heat_floor = 1 }`, `{ heat_cap = 0 }`, `{ dispatch = false }`, `{ traffic = 0.4 }` (the share of
moving traffic until the mission ends), `{ flag = "x" }`, `{ unflag = "x" }`, `{ story = "x" }`
(kept after the mission), `{ give = { weapon = "p9", ammo = 24 } }`, `{ cash = 500 }`,
`{ text = { from = "mack", body = "..." } }`, `{ note = { title = "...", body = "..." } }`,
`{ help = "..." }`, `{ time = "21:00" }`, `{ weather = "rain" }`,
`{ lock = { actor = "id", locked = true } }`, `{ teleport = "anchor" }`, `{ alert = true }`,
`{ explode = "id" }`, `{ unlock = "switch" }`.

## Fail conditions (`[[fail]]`)

`when = { destroyed = "id" }`, `{ died = "player" }` (or a person), `{ abandoned = { actor = "id",
distance = 120.0 } }`, `{ spotted = ["guard1"] }`, `{ timer = 90.0 }`, `{ heat_over = 2 }`,
`{ left_area = { at = "anchor", radius = 200.0 } }`, `{ flag = "x" }` — each with a `reason`
(the fail card's line). Dying and arrest always fail a mission.

## Checkpoints, Retry and Skip

A checkpoint is kept when a step marked `checkpoint = true` begins (and at the start): the
player's place and vehicle, every actor still up (with its behaviour), the flags, heat, health
and clock. Retry puts all of it back and begins that step again. After three fails at one
checkpoint the card offers Skip: from the checkpoint, every step up to the next checkpoint is
done for the player (put where it ends, its targets cleared) and play goes on there.

## Writing missions that bots can finish

- Lay driving legs out along the way traffic runs (no U-turns): give vehicles `spot:` anchors with
  a `yaw`, put follow and race targets ahead.
- Thin traffic for set pieces (`{ traffic = 0.35 }`) and cap the heat (`{ heat_cap = 0 }`) where a
  shootout should not bring the police.
- Keep on-foot targets on walkable ground (the anchor snaps them there; check the snapped place
  with `bot_trace`).
