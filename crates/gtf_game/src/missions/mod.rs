//! Missions (PLAN.md §5.6.8, §6.8.6): data in `content/freeway/missions/<key>.toml` — who
//! plays it, where it starts, the actors it spawns, its steps, fail conditions, checkpoints,
//! medals and reward — checked by [`validate`] and played by the runtime (`run.rs`). Every step
//! kind has a bot policy (`bot.rs`), so `gtf play bot=true` finishes any mission.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use glam::Vec3;
use pav_core::entity::EntityId;
use pav_core::present::Medal;
use serde::{Deserialize, Serialize};

use crate::anchor::Anchor;

pub mod bot;
mod run;

/// Whether actor `id` of the mission under way is done for (gone, dead, wrecked, taken).
pub fn down_of(sim: &pav_core::sim::Sim, run: &Run, id: &str) -> bool {
    run::down(sim, run, id)
}

/// Every mission file, embedded (the browser and a lone executable have no files); running from
/// the repository, the file on disk wins (hot reload).
pub const FILES: &[(&str, &str)] = &[
    ("dev_test", include_str!("../../../../content/freeway/missions/dev_test.toml")),
    ("dev_test2", include_str!("../../../../content/freeway/missions/dev_test2.toml")),
    ("m1_repo_man", include_str!("../../../../content/freeway/missions/m1_repo_man.toml")),
    ("m2_midlife", include_str!("../../../../content/freeway/missions/m2_midlife.toml")),
    ("m3_overdrive", include_str!("../../../../content/freeway/missions/m3_overdrive.toml")),
    ("s1_rush_hour", include_str!("../../../../content/freeway/missions/s1_rush_hour.toml")),
    ("s2_heavy_metal", include_str!("../../../../content/freeway/missions/s2_heavy_metal.toml")),
    ("s3_test_drive", include_str!("../../../../content/freeway/missions/s3_test_drive.toml")),
    ("h0_board", include_str!("../../../../content/freeway/missions/h0_board.toml")),
    ("h1_freeway_job", include_str!("../../../../content/freeway/missions/h1_freeway_job.toml")),
];

fn yes() -> bool {
    true
}

/// A mission file.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionDef {
    pub key: String,
    pub title: String,
    /// Who plays it (`dez`, `mack`, `roxy`).
    pub protagonist: String,
    /// The district it is played in: "" for Paloma Bay, `dev` for the Proving Grounds.
    #[serde(default)]
    pub district: String,
    /// A line for the phone's Jobs app and the replay list.
    #[serde(default)]
    pub text: String,
    /// Who gives it on the phone (calling them starts it).
    #[serde(default)]
    pub contact: Option<String>,
    /// The map letter and where the mission is picked up (none: from the phone or a script).
    #[serde(default)]
    pub blip: Option<BlipDef>,
    /// Where the player stands when the mission is played straight away (a scene, a replay):
    /// the blip's place when none.
    #[serde(default)]
    pub start: Option<String>,
    /// Missions that must be passed first.
    #[serde(default)]
    pub unlock: Vec<String>,
    #[serde(default)]
    pub time: Option<TimeDef>,
    #[serde(default)]
    pub weather: Option<String>,
    #[serde(default)]
    pub reward: Reward,
    #[serde(default)]
    pub medals: Vec<MedalDef>,
    /// Speakers' names on the subtitles (protagonists are named already).
    #[serde(default)]
    pub names: BTreeMap<String, String>,
    #[serde(default)]
    pub spawn: Vec<SpawnDef>,
    pub step: Vec<StepDef>,
    #[serde(default)]
    pub fail: Vec<FailDef>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlipDef {
    pub letter: char,
    pub at: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimeDef {
    pub at: String,
    #[serde(default)]
    pub lock: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reward {
    #[serde(default)]
    pub cash: i64,
    /// Features opened: `switch` (the switch wheel).
    #[serde(default)]
    pub unlock: Vec<String>,
    /// A text that arrives after the mission (from, body).
    #[serde(default)]
    pub text: Option<(String, String)>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActorValue {
    pub actor: String,
    pub value: f32,
}

/// A medal goal: one condition each.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MedalDef {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub time_under: Option<f32>,
    #[serde(default)]
    pub vehicle_health_over: Option<ActorValue>,
    #[serde(default)]
    pub health_over: Option<f32>,
    #[serde(default)]
    pub headshots: Option<u32>,
    #[serde(default)]
    pub accuracy_over: Option<f32>,
    #[serde(default)]
    pub undetected: bool,
    #[serde(default)]
    pub no_heat: bool,
    #[serde(default)]
    pub flag: Option<String>,
}

/// An actor: a vehicle, a person, an armed opponent, a protagonist's body or a pickup.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpawnDef {
    pub id: String,
    #[serde(default)]
    pub vehicle: Option<String>,
    /// A pedestrian archetype (peds.toml id).
    #[serde(default)]
    pub ped: Option<String>,
    /// An enemies.toml side.
    #[serde(default)]
    pub enemy: Option<String>,
    #[serde(default)]
    pub protagonist: Option<String>,
    /// Something to pick up: `package`, `cash`, `evidence`.
    #[serde(default)]
    pub pickup: Option<String>,
    pub at: String,
    /// Facing (degrees from north), over the anchor's own.
    #[serde(default)]
    pub yaw: Option<f32>,
    /// Paint (vehicles), by index into the archetype's colours.
    #[serde(default)]
    pub paint: Option<usize>,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub weapon: Option<String>,
    /// What it does (see [`Act`]).
    #[serde(default)]
    pub behaviour: Option<String>,
    /// For a vehicle: the actor (a person spawned with it) at the wheel.
    #[serde(default)]
    pub driver: Option<String>,
    /// Made by a `spawn` action later, not at the start.
    #[serde(default)]
    pub later: bool,
    /// Its blip: `target`, `enemy`, `ally`, `vehicle`, `pickup`.
    #[serde(default)]
    pub blip: Option<String>,
    /// Health: a person's hit points, a vehicle's share (0..1).
    #[serde(default)]
    pub health: Option<f32>,
    /// The name its marker shows.
    #[serde(default)]
    pub name: Option<String>,
}

impl SpawnDef {
    pub fn kind(&self) -> &'static str {
        match () {
            _ if self.vehicle.is_some() => "vehicle",
            _ if self.enemy.is_some() => "enemy",
            _ if self.protagonist.is_some() => "protagonist",
            _ if self.pickup.is_some() => "pickup",
            _ => "ped",
        }
    }
}

/// The step kinds (PLAN.md §6.8.6).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    #[default]
    Wait,
    Cutscene,
    Dialogue,
    Goto,
    EnterVehicle,
    DriveTo,
    Follow,
    Chase,
    Escape,
    LoseHeat,
    Kill,
    Survive,
    Defend,
    Collect,
    Photograph,
    Interact,
    Switch,
    Race,
    Stealth,
    Deliver,
    Holdup,
    Choose,
    Branch,
    Set,
    Parallel,
    Jump,
}

impl StepKind {
    pub const ALL: [StepKind; 26] = [
        StepKind::Wait,
        StepKind::Cutscene,
        StepKind::Dialogue,
        StepKind::Goto,
        StepKind::EnterVehicle,
        StepKind::DriveTo,
        StepKind::Follow,
        StepKind::Chase,
        StepKind::Escape,
        StepKind::LoseHeat,
        StepKind::Kill,
        StepKind::Survive,
        StepKind::Defend,
        StepKind::Collect,
        StepKind::Photograph,
        StepKind::Interact,
        StepKind::Switch,
        StepKind::Race,
        StepKind::Stealth,
        StepKind::Deliver,
        StepKind::Holdup,
        StepKind::Choose,
        StepKind::Branch,
        StepKind::Set,
        StepKind::Parallel,
        StepKind::Jump,
    ];

    pub fn name(self) -> &'static str {
        match self {
            StepKind::Wait => "wait",
            StepKind::Cutscene => "cutscene",
            StepKind::Dialogue => "dialogue",
            StepKind::Goto => "goto",
            StepKind::EnterVehicle => "enter_vehicle",
            StepKind::DriveTo => "drive_to",
            StepKind::Follow => "follow",
            StepKind::Chase => "chase",
            StepKind::Escape => "escape",
            StepKind::LoseHeat => "lose_heat",
            StepKind::Kill => "kill",
            StepKind::Survive => "survive",
            StepKind::Defend => "defend",
            StepKind::Collect => "collect",
            StepKind::Photograph => "photograph",
            StepKind::Interact => "interact",
            StepKind::Switch => "switch",
            StepKind::Race => "race",
            StepKind::Stealth => "stealth",
            StepKind::Deliver => "deliver",
            StepKind::Holdup => "holdup",
            StepKind::Choose => "choose",
            StepKind::Branch => "branch",
            StepKind::Set => "set",
            StepKind::Parallel => "parallel",
            StepKind::Jump => "jump",
        }
    }
}

/// A camera shot in a cutscene: from `offset` in the anchor's frame (x right, y up, z ahead;
/// world axes for anchors without a facing) looking at `look` (plus `look_offset`), dollying to
/// `to` over its `seconds`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shot {
    pub anchor: String,
    #[serde(default)]
    pub offset: [f32; 3],
    #[serde(default)]
    pub to: Option<[f32; 3]>,
    #[serde(default)]
    pub look: Option<String>,
    #[serde(default)]
    pub look_offset: [f32; 3],
    #[serde(default = "fov")]
    pub fov: f32,
    pub seconds: f32,
}

fn fov() -> f32 {
    45.0
}

/// A line of dialogue (a subtitle and babble).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Line {
    pub who: String,
    pub text: String,
    #[serde(default)]
    pub seconds: Option<f32>,
}

impl Line {
    /// How long it shows: as given, else by its length.
    pub fn length(&self) -> f32 {
        self.seconds.unwrap_or(1.2 + self.text.chars().count() as f32 * 0.055)
    }
}

/// A choice on the planning board.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceDef {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub lines: Vec<String>,
}

/// What a step or the mission can do when a step starts or ends.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Spawn(String),
    Despawn(String),
    Behaviour {
        actor: String,
        act: String,
    },
    Blip {
        actor: String,
        kind: String,
    },
    Unblip(String),
    /// The wanted level at least this (0 clears it).
    Heat(u8),
    HeatFloor(u8),
    HeatCap(u8),
    Dispatch(bool),
    /// The share of the usual moving traffic (1: all of it) until the mission ends.
    Traffic(f32),
    Flag(String),
    Unflag(String),
    /// Kept after the mission (the heist approach).
    Story(String),
    Give {
        weapon: String,
        #[serde(default)]
        ammo: u32,
    },
    Cash(i64),
    /// Body armour on the player (0–100).
    Armor(f32),
    /// Cash to one of the crew, played or not (the heist's cut).
    Pay {
        who: String,
        cash: i64,
    },
    Text {
        from: String,
        body: String,
    },
    Note {
        title: String,
        body: String,
    },
    Help(String),
    Time(String),
    Weather(String),
    Lock {
        actor: String,
        locked: bool,
    },
    /// The player (and their vehicle, when in one) to an anchor.
    Teleport(String),
    /// Every armed opponent turns to the fight.
    Alert(bool),
    Explode(String),
    /// A feature opens: `switch`.
    Unlock(String),
}

/// A fail condition.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailDef {
    pub when: When,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum When {
    /// An actor wrecked (a vehicle) or dead (a person).
    Destroyed(String),
    /// `player` or a person.
    Died(String),
    Abandoned {
        actor: String,
        distance: f32,
    },
    /// Seen by any of these (a stealth meter fills).
    Spotted(Vec<String>),
    /// Seconds since the step began (step fails) or the mission (mission fails).
    Timer(f32),
    HeatOver(u8),
    LeftArea {
        at: String,
        radius: f32,
    },
    Flag(String),
}

/// A step.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepDef {
    pub kind: StepKind,
    /// A name `branch` steps jump to.
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub objective: Option<String>,
    /// A tutorial tip shown as the step begins.
    #[serde(default)]
    pub help: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub targets: Vec<String>,
    #[serde(default)]
    pub radius: Option<f32>,
    #[serde(default)]
    pub seconds: Option<f32>,
    #[serde(default)]
    pub distance: Option<f32>,
    /// Follow: no nearer than this (m), or they notice.
    #[serde(default)]
    pub min: Option<f32>,
    #[serde(default)]
    pub pursuers: Vec<String>,
    /// The vehicle the step is about (drive it there, deliver it).
    #[serde(default)]
    pub vehicle: Option<String>,
    /// Chase: wreck or kill the target rather than catch it.
    #[serde(default)]
    pub destroy: bool,
    #[serde(default)]
    pub checkpoint: bool,
    #[serde(default)]
    pub on_start: Vec<Action>,
    #[serde(default)]
    pub on_done: Vec<Action>,
    #[serde(default)]
    pub shots: Vec<Shot>,
    #[serde(default)]
    pub lines: Vec<Line>,
    /// Cutscenes: ✕ skips.
    #[serde(default = "yes")]
    pub skip: bool,
    /// Switch: to whom. Branch: the label when `flag` is set.
    #[serde(default)]
    pub to: Option<String>,
    /// Protagonists the switch wheel may pick during this step.
    #[serde(default)]
    pub switch: Vec<String>,
    /// Race: the gates (anchors), laps and the other racers (vehicle actors).
    #[serde(default)]
    pub gates: Vec<String>,
    #[serde(default)]
    pub laps: Option<u32>,
    #[serde(default)]
    pub racers: Vec<String>,
    /// Choose: the cards.
    #[serde(default)]
    pub options: Vec<ChoiceDef>,
    /// Branch: the flag tested, and the label when it is not set (else the next step).
    #[serde(default)]
    pub flag: Option<String>,
    #[serde(default)]
    pub otherwise: Option<String>,
    /// Parallel: the sub-steps, each for its protagonist (`who`).
    #[serde(default)]
    pub steps: Vec<StepDef>,
    #[serde(default)]
    pub who: Option<String>,
    /// Survive and defend: the actors that come at the player, wave by wave.
    #[serde(default)]
    pub waves: Vec<Vec<String>>,
    /// Stealth: who must not see the player.
    #[serde(default)]
    pub watchers: Vec<String>,
    /// Interact: what holding D-pad right does ("Plant the bug").
    #[serde(default)]
    pub prompt: Option<String>,
    /// Lose heat: down to this many stars.
    #[serde(default)]
    pub stars: Option<u8>,
    #[serde(default)]
    pub fail: Vec<FailDef>,
}

/// What an actor does.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Act {
    /// Stands (a vehicle stays parked).
    Idle,
    /// Walks about (a vehicle with a driver cruises).
    Wander,
    /// Runs (drives) from the player.
    Flee,
    /// Runs at the player and fights (a vehicle drives at the player).
    Chase,
    /// Keeps up with the player (allies).
    Follow,
    /// Armed opponents: in the fight.
    Fight,
    /// Armed opponents: at their post, until something alerts them.
    Guard,
    /// Down on the ground, hands over the head.
    Cower,
    /// Walks (drives) to an anchor and waits there.
    Goto(String),
}

impl Act {
    pub fn parse(s: &str) -> Result<Act, String> {
        Ok(match s {
            "idle" => Act::Idle,
            "wander" | "cruise" => Act::Wander,
            "flee" => Act::Flee,
            "chase_player" | "chase" => Act::Chase,
            "follow_player" | "follow" => Act::Follow,
            "fight" => Act::Fight,
            "guard" => Act::Guard,
            "cower" => Act::Cower,
            _ => match s.split_once(':') {
                Some(("goto" | "drive_to", a)) => {
                    Anchor::parse(a)?;
                    Act::Goto(a.into())
                }
                _ => {
                    return Err(format!(
                        "'{s}' is not a behaviour (idle, wander, flee, chase_player, follow_player, fight, guard, cower, goto:<anchor>)"
                    ));
                }
            },
        })
    }
}

/// Mission files parsed so far, by key and text (re-read when a file changes on disk).
static CACHE: Mutex<Vec<(String, String, Arc<MissionDef>)>> = Mutex::new(Vec::new());

/// The mission `key`.
pub fn get(key: &str) -> Result<Arc<MissionDef>> {
    let (_, built_in) = FILES
        .iter()
        .find(|(k, _)| *k == key)
        .ok_or_else(|| anyhow!("no mission '{key}' (missions: {})", FILES.iter().map(|f| f.0).collect::<Vec<_>>().join(", ")))?;
    let rel = format!("missions/{key}.toml");
    let text = crate::content::text(&rel, built_in);
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, _, d)) = cache.iter().find(|(k, t, _)| k == key && *t == *text) {
        return Ok(d.clone());
    }
    let d = Arc::new(parse(&text).map_err(|e| anyhow!("content/freeway/{rel}: {e}"))?);
    cache.retain(|(k, _, _)| k != key);
    cache.push((key.to_string(), text.into_owned(), d.clone()));
    Ok(d)
}

/// Every mission that reads (a broken file is logged and left out).
pub fn all() -> Vec<Arc<MissionDef>> {
    FILES
        .iter()
        .filter_map(|(k, _)| match get(k) {
            Ok(d) => Some(d),
            Err(e) => {
                log::error!("{e:#}");
                None
            }
        })
        .collect()
}

pub fn parse(text: &str) -> Result<MissionDef, String> {
    toml::from_str::<MissionDef>(text).map_err(|e| e.to_string())
}

/// The line of the `n`th `[[header]]` table in a TOML text (1-based), for error messages.
fn line_of(text: &str, header: &str, n: usize) -> usize {
    text.lines()
        .enumerate()
        .filter(|(_, l)| l.trim_start().starts_with(header) && l.trim_start()[header.len()..].trim_start().starts_with("]]"))
        .nth(n)
        .map_or(1, |(i, _)| i + 1)
}

/// Checks a mission against the game's data and its district: every actor, anchor, vehicle,
/// person, side, weapon, protagonist and label it names exists, and every step has what its
/// kind needs. Errors read `missions/<key>.toml:<line>: <what>`.
pub fn validate(d: &MissionDef, text: &str, city: &pav_world::City) -> Vec<String> {
    let mut errs: Vec<String> = Vec::new();
    let file = format!("missions/{}.toml", d.key);
    let crew = crate::protagonists::def().ok();
    let protagonist = |k: &str| crew.as_ref().is_some_and(|c| c.index(k).is_some());
    let vehicles = crate::vehicles::all().ok();
    let peds = crate::peds::def().ok();
    let sides = crate::enemies::def().ok();
    let weapons = crate::combat::def().ok();
    let ids: BTreeSet<&str> = d.spawn.iter().map(|s| s.id.as_str()).collect();
    let labels: BTreeSet<&str> = d.step.iter().filter_map(|s| s.label.as_deref()).collect();
    let anchor = |a: &str, errs: &mut Vec<String>, at: usize| match Anchor::parse(a) {
        Ok(Anchor::Actor(id)) if !ids.contains(id.as_str()) && !protagonist(&id) => {
            errs.push(format!("{file}:{at}: no actor '{id}'"))
        }
        Ok(x) if !x.exists(city) => errs.push(format!("{file}:{at}: '{a}' is not in {}", city.name)),
        Ok(_) => {}
        Err(e) => errs.push(format!("{file}:{at}: {e}")),
    };
    let actor = |id: &str, errs: &mut Vec<String>, at: usize| {
        if !ids.contains(id) {
            errs.push(format!("{file}:{at}: no actor '{id}'"));
        }
    };
    if !protagonist(&d.protagonist) {
        errs.push(format!("{file}:1: no protagonist '{}'", d.protagonist));
    }
    if d.step.is_empty() {
        errs.push(format!("{file}:1: no steps"));
    }
    if let Some(b) = &d.blip {
        anchor(&b.at, &mut errs, 1);
    }
    if let Some(s) = &d.start {
        anchor(s, &mut errs, 1);
    }
    if d.blip.is_none() && d.start.is_none() {
        errs.push(format!("{file}:1: needs a blip or a start"));
    }
    if let Some(t) = &d.time
        && crate::daytime::parse_time(&t.at).is_none()
    {
        errs.push(format!("{file}:1: time '{}' is not HH:MM", t.at));
    }
    for u in &d.unlock {
        if !FILES.iter().any(|f| f.0 == u) {
            errs.push(format!("{file}:1: unlock names no mission '{u}'"));
        }
    }
    let mut seen = BTreeSet::new();
    for (i, s) in d.spawn.iter().enumerate() {
        let at = line_of(text, "[[spawn", i);
        if !seen.insert(s.id.as_str()) {
            errs.push(format!("{file}:{at}: actor '{}' twice", s.id));
        }
        let kinds = [s.vehicle.is_some(), s.ped.is_some(), s.enemy.is_some(), s.protagonist.is_some(), s.pickup.is_some()];
        if kinds.iter().filter(|k| **k).count() != 1 {
            errs.push(format!("{file}:{at}: '{}' must be one of vehicle, ped, enemy, protagonist, pickup", s.id));
        }
        anchor(&s.at, &mut errs, at);
        if let (Some(v), Some(all)) = (&s.vehicle, &vehicles)
            && !all.iter().any(|a| &a.key == v)
        {
            errs.push(format!("{file}:{at}: no vehicle '{v}'"));
        }
        if let (Some(p), Some(pd)) = (&s.ped, &peds)
            && !pd.archetype.iter().any(|a| &a.id == p)
        {
            errs.push(format!("{file}:{at}: no pedestrian '{p}'"));
        }
        if let (Some(e), Some(sd)) = (&s.enemy, &sides)
            && !sd.side.contains_key(e)
        {
            errs.push(format!("{file}:{at}: no side '{e}'"));
        }
        if let Some(p) = &s.protagonist
            && !protagonist(p)
        {
            errs.push(format!("{file}:{at}: no protagonist '{p}'"));
        }
        if let (Some(w), Some(wd)) = (&s.weapon, &weapons)
            && !wd.weapon.iter().any(|x| &x.id == w)
        {
            errs.push(format!("{file}:{at}: no weapon '{w}'"));
        }
        if let Some(b) = &s.behaviour
            && let Err(e) = Act::parse(b)
        {
            errs.push(format!("{file}:{at}: {e}"));
        }
        if let Some(b) = &s.behaviour
            && let Ok(Act::Goto(a)) = Act::parse(b)
        {
            anchor(&a, &mut errs, at);
        }
        if let Some(dr) = &s.driver {
            actor(dr, &mut errs, at);
            if s.vehicle.is_none() {
                errs.push(format!("{file}:{at}: only a vehicle has a driver"));
            }
        }
        if let Some(b) = &s.blip
            && !["target", "enemy", "ally", "vehicle", "pickup"].contains(&b.as_str())
        {
            errs.push(format!("{file}:{at}: blip '{b}' (target, enemy, ally, vehicle, pickup)"));
        }
    }
    let actions = |list: &[Action], errs: &mut Vec<String>, at: usize| {
        for a in list {
            match a {
                Action::Spawn(id) | Action::Despawn(id) | Action::Unblip(id) | Action::Explode(id) => actor(id, errs, at),
                Action::Behaviour { actor: id, act } => {
                    actor(id, errs, at);
                    match Act::parse(act) {
                        Ok(Act::Goto(a)) => anchor(&a, errs, at),
                        Ok(_) => {}
                        Err(e) => errs.push(format!("{file}:{at}: {e}")),
                    }
                }
                Action::Blip { actor: id, .. } | Action::Lock { actor: id, .. } => actor(id, errs, at),
                Action::Give { weapon, .. } => {
                    if weapons.as_ref().is_some_and(|wd| !wd.weapon.iter().any(|x| &x.id == weapon)) {
                        errs.push(format!("{file}:{at}: no weapon '{weapon}'"));
                    }
                }
                Action::Pay { who, .. } if !protagonist(who) => {
                    errs.push(format!("{file}:{at}: pay names no protagonist '{who}'"))
                }
                Action::Time(t) if crate::daytime::parse_time(t).is_none() => {
                    errs.push(format!("{file}:{at}: time '{t}' is not HH:MM"))
                }
                Action::Teleport(a) => anchor(a, errs, at),
                Action::Armor(a) if !(0.0..=crate::combat::MAX_ARMOR).contains(a) => {
                    errs.push(format!("{file}:{at}: armor {a} is not 0-100"))
                }
                Action::Unlock(f) if f != "switch" => errs.push(format!("{file}:{at}: unlock '{f}' (switch)")),
                _ => {}
            }
        }
    };
    let fails = |list: &[FailDef], errs: &mut Vec<String>, at: usize| {
        for f in list {
            match &f.when {
                When::Destroyed(id) | When::Abandoned { actor: id, .. } => actor(id, errs, at),
                When::Died(id) if id != "player" => actor(id, errs, at),
                When::Spotted(list) => list.iter().for_each(|id| actor(id, errs, at)),
                When::LeftArea { at: a, .. } => anchor(a, errs, at),
                _ => {}
            }
        }
    };
    fails(&d.fail, &mut errs, 1);
    for (i, s) in d.step.iter().enumerate() {
        let at = line_of(text, "[[step", i);
        check_step(s, &file, at, false, &mut errs, &anchor, &actor, &labels, &protagonist);
        actions(&s.on_start, &mut errs, at);
        actions(&s.on_done, &mut errs, at);
        fails(&s.fail, &mut errs, at);
        for sub in &s.steps {
            check_step(sub, &file, at, true, &mut errs, &anchor, &actor, &labels, &protagonist);
            actions(&sub.on_start, &mut errs, at);
            actions(&sub.on_done, &mut errs, at);
        }
    }
    for m in &d.medals {
        let n = [
            m.time_under.is_some(),
            m.vehicle_health_over.is_some(),
            m.health_over.is_some(),
            m.headshots.is_some(),
            m.accuracy_over.is_some(),
            m.undetected,
            m.no_heat,
            m.flag.is_some(),
        ]
        .iter()
        .filter(|x| **x)
        .count();
        if n != 1 {
            errs.push(format!("{file}:1: medal '{}' needs exactly one goal", m.id));
        }
        if let Some(v) = &m.vehicle_health_over {
            actor(&v.actor, &mut errs, 1);
        }
    }
    errs
}

/// What each step kind needs.
#[allow(clippy::too_many_arguments)]
fn check_step(
    s: &StepDef,
    file: &str,
    at: usize,
    sub: bool,
    errs: &mut Vec<String>,
    anchor: &dyn Fn(&str, &mut Vec<String>, usize),
    actor: &dyn Fn(&str, &mut Vec<String>, usize),
    labels: &BTreeSet<&str>,
    protagonist: &dyn Fn(&str) -> bool,
) {
    use StepKind::*;
    let k = s.kind.name();
    let need_target = |errs: &mut Vec<String>| match &s.target {
        Some(t) => anchor(t, errs, at),
        None => errs.push(format!("{file}:{at}: {k} needs a target")),
    };
    let need_actor_target = |errs: &mut Vec<String>| match s.target.as_deref().map(Anchor::parse) {
        Some(Ok(Anchor::Actor(id))) => actor(&id, errs, at),
        _ => errs.push(format!("{file}:{at}: {k} needs a target actor (actor:<id>)")),
    };
    if sub && !matches!(s.kind, Goto | Interact | Kill | Wait | DriveTo | Collect | EnterVehicle | Photograph) {
        errs.push(format!("{file}:{at}: a parallel step cannot hold a {k} step"));
    }
    if sub && s.who.as_deref().is_none_or(|w| !protagonist(w)) {
        errs.push(format!("{file}:{at}: a parallel sub-step needs `who` (a protagonist)"));
    }
    match s.kind {
        Cutscene => {
            if s.shots.is_empty() {
                errs.push(format!("{file}:{at}: a cutscene needs shots"));
            }
            for shot in &s.shots {
                anchor(&shot.anchor, errs, at);
                if let Some(l) = &shot.look {
                    anchor(l, errs, at);
                }
            }
        }
        Dialogue if s.lines.is_empty() => errs.push(format!("{file}:{at}: dialogue needs lines")),
        Goto | DriveTo | Interact | Stealth => need_target(errs),
        EnterVehicle | Follow | Chase | Holdup => need_actor_target(errs),
        Deliver => {
            need_target(errs);
            match &s.vehicle {
                Some(v) => actor(v, errs, at),
                None => errs.push(format!("{file}:{at}: deliver needs a vehicle actor")),
            }
        }
        Escape => {
            if s.pursuers.is_empty() {
                errs.push(format!("{file}:{at}: escape needs pursuers"));
            }
            s.pursuers.iter().for_each(|p| actor(p, errs, at));
        }
        Kill | Collect | Photograph => {
            if s.targets.is_empty() {
                errs.push(format!("{file}:{at}: {k} needs targets"));
            }
            s.targets.iter().for_each(|t| actor(t, errs, at));
        }
        Survive | Defend => {
            if s.seconds.is_none() && s.waves.is_empty() {
                errs.push(format!("{file}:{at}: {k} needs seconds or waves"));
            }
            s.waves.iter().flatten().for_each(|t| actor(t, errs, at));
            if s.kind == Defend {
                need_target(errs);
            }
        }
        Switch => match &s.to {
            Some(p) if protagonist(p) => {}
            _ => errs.push(format!("{file}:{at}: switch needs `to` (a protagonist)")),
        },
        Race => {
            if s.gates.is_empty() {
                errs.push(format!("{file}:{at}: a race needs gates"));
            }
            s.gates.iter().for_each(|g| anchor(g, errs, at));
            s.racers.iter().for_each(|r| actor(r, errs, at));
        }
        Choose if s.options.len() < 2 => errs.push(format!("{file}:{at}: choose needs two options or more")),
        Branch => {
            if s.flag.is_none() {
                errs.push(format!("{file}:{at}: branch needs a flag"));
            }
            for l in [&s.to, &s.otherwise].into_iter().flatten() {
                if !labels.contains(l.as_str()) {
                    errs.push(format!("{file}:{at}: no step labelled '{l}'"));
                }
            }
            if s.to.is_none() {
                errs.push(format!("{file}:{at}: branch needs `to`"));
            }
        }
        Parallel if s.steps.len() < 2 => errs.push(format!("{file}:{at}: parallel needs two sub-steps or more")),
        Jump => {
            need_target(errs);
            if s.gates.len() != 2 {
                errs.push(format!("{file}:{at}: a jump needs gates = [run-up, lip]"));
            }
            s.gates.iter().for_each(|g| anchor(g, errs, at));
        }
        _ => {}
    }
    for w in &s.watchers {
        actor(w, errs, at);
    }
    for p in &s.switch {
        if !protagonist(p) {
            errs.push(format!("{file}:{at}: switch names no protagonist '{p}'"));
        }
    }
}

// ---------------------------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------------------------

/// One mission's record (the save keeps them).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Record {
    pub passed: bool,
    pub medal: Option<Medal>,
    /// The quickest pass (s).
    pub best: Option<f32>,
    pub plays: u32,
}

/// Missions played and the one under way (part of the game's state).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Missions {
    pub run: Option<Run>,
    pub records: BTreeMap<String, Record>,
    /// Choices kept after the mission that made them (the heist approach).
    pub story: BTreeSet<String>,
    /// Seconds before a mission marker starts a mission again (just quit one).
    pub cooldown: f32,
    /// The bot's pick on choice steps (`mission choose <id>`; none: the first).
    pub bot_choice: Option<String>,
    /// Counts for tests.
    pub passed: u32,
    pub failed: u32,
    pub retries: u32,
    pub skips: u32,
}

impl Missions {
    pub fn passed(&self, key: &str) -> bool {
        self.records.get(key).is_some_and(|r| r.passed)
    }

    /// Whether `d` may be picked up: its prerequisites passed, itself not yet.
    pub fn open(&self, d: &MissionDef) -> bool {
        !self.passed(&d.key) && d.unlock.iter().all(|u| self.passed(u))
    }
}

/// Where a mission is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Phase {
    /// The title card, seconds in.
    Intro {
        t: f32,
    },
    Play,
    /// MISSION FAILED (the reason), then the card: Retry, Skip, Quit.
    Failed {
        t: f32,
    },
    /// MISSION PASSED, then the results.
    Passed {
        t: f32,
    },
}

/// A spawned actor.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Actor {
    /// Its entity (none for a pickup, which is drawn where it lies).
    pub entity: Option<EntityId>,
    /// Where a pickup lies.
    pub at: Vec3,
    /// Its spawn definition (index into the mission's).
    pub spawn: usize,
    pub act: Act,
    pub blip: Option<String>,
    /// A pickup collected.
    pub taken: bool,
    /// The orders for this behaviour have gone to its vehicle's driver; a person's way over the
    /// walk map; seconds to the next replan (or punch).
    pub ordered: bool,
    pub path: Vec<glam::Vec2>,
    pub timer: f32,
    /// A racer's next gate and laps done.
    pub gate: usize,
    pub lap: u32,
    /// Made while its ground was not loaded yet (far from the player): it appears at `at`,
    /// facing `yaw`, once the ground is there (until then it counts as up, not down).
    #[serde(default)]
    pub waiting: bool,
    #[serde(default)]
    pub yaw: f32,
}

/// A step's own progress.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Progress {
    /// The line showing and seconds into it; the shot and seconds into it.
    pub line: usize,
    pub line_t: f32,
    pub shot: usize,
    pub shot_t: f32,
    /// Targets done (killed, collected, photographed, gates passed) by index.
    pub done: Vec<bool>,
    pub wave: usize,
    /// A hold, a bag filling, a stealth meter (0..1).
    pub meter: f32,
    /// Seconds too far from (or too near) a target.
    pub away: f32,
    pub near: f32,
    pub lap: u32,
    pub gate: usize,
    pub focus: usize,
    /// The step's actions ran.
    pub started: bool,
}

/// A parallel step's sub-step.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Sub {
    pub who: usize,
    pub t: f32,
    pub prog: Progress,
    pub done: bool,
}

/// An actor as a checkpoint keeps it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActorSave {
    pub id: String,
    pub spawn: usize,
    pub pos: Vec3,
    pub yaw: f32,
    pub act: Act,
    pub blip: Option<String>,
    pub health: f32,
    /// The vehicle actor it sits in.
    pub riding: Option<String>,
    pub taken: bool,
}

/// What a retry puts back.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Checkpoint {
    pub step: usize,
    pub elapsed: f32,
    pub protagonist: usize,
    pub feet: Vec3,
    pub yaw: f32,
    /// The vehicle the player sat in: an actor, or an archetype and its place.
    pub vehicle: Option<(Option<String>, String, Vec3, f32)>,
    pub actors: Vec<ActorSave>,
    pub flags: BTreeSet<String>,
    pub stars: u8,
    pub health: f32,
    pub armor: f32,
    pub hour: f32,
}

/// What the medals and the results need.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RunStats {
    pub shots: u32,
    pub hits: u32,
    pub headshots: u32,
    pub kills: u32,
    pub peak_stars: u8,
    pub spotted: bool,
}

/// The mission under way.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Run {
    pub key: String,
    pub phase: Phase,
    pub step: usize,
    /// Seconds in the step, and since the mission began (game time).
    pub t: f32,
    pub elapsed: f32,
    pub actors: BTreeMap<String, Actor>,
    pub flags: BTreeSet<String>,
    pub prog: Progress,
    pub subs: Vec<Sub>,
    pub checkpoint: Option<Checkpoint>,
    /// Fails at the current checkpoint (three offer Skip).
    pub fails: u32,
    pub reason: String,
    /// The fail card's focus.
    pub menu: usize,
    pub stats: RunStats,
    /// Played again from the replay list (no reward).
    pub replay: bool,
    /// The results: medal, goals met, cash paid.
    pub medal: Option<Medal>,
    pub met: Vec<bool>,
    pub paid: i64,
    /// The combat counters when the mission began (the stats count from them).
    pub base: (u32, u32, u32, u32),
    /// A tutorial tip and seconds since it appeared.
    pub help: Option<(String, f32)>,
    /// The clock's speed before the mission held it.
    pub clock: Option<f32>,
    /// The phone's photo count when the step began (photographs count from it).
    pub photos: u64,
    /// Menu navigation on the mission's cards.
    #[serde(default)]
    pub nav: crate::pad::Nav,
}
