//! Random events (PLAN.md §5.6.11), met in the street outside missions and not too often
//! (`content/freeway/activities.toml` `[[event]]`). *Purse Snatcher* (`kind = "snatch"`): on a
//! busy sidewalk someone near the player has their purse snatched; chase the thief down on foot
//! (one hit does it), pick the purse up, then give it back (D-pad right by its owner) or keep it.
//! *Getaway Needed* (`kind = "getaway"`): a robber outside a shop flags the passing player down;
//! stop by them and they get in with the police after them; lose the police, drop them off. Either
//! way it counts once for completion (`event:<id>`). The bot plays an event under way
//! (`Game::bot`), and `activity <id>` starts one at once (`play bot=true seeds=3
//! cmd="activity purse_snatcher"` reports wins). This file also holds what vigilante and the
//! events share (the clothes, a runner on foot, lines said in the street) and the glue the game's
//! hook calls (the HUD, the bot's frame, the outcome of a trial, the GPS).

use std::collections::BTreeMap;

use glam::{Quat, Vec2, Vec3};
use pav_core::InputFrame;
use pav_core::entity::EntityId;
use pav_core::frame::SimEvent;
use pav_core::input::pad;
use pav_core::present::*;
use pav_core::puppet::PuppetDef;
use pav_core::rng::Rng;
use pav_core::sim::Sim;
use pav_core::{Color, games::Outcome};
use pav_world::City;
use pav_world::walk::Walk;
use serde::{Deserialize, Serialize};

use super::ActivitiesDef;
use crate::Freeway;
use crate::anchor::{self, Anchor};
use crate::drive::Car;
use crate::missions::bot::MissionBot;
use crate::peds::{Doing, Ped};

/// Seconds between checks for a random event.
const CHECK: f32 = 3.0;
/// How near the player someone may be snatched from (m), and how far a thief starts from them.
const OWNER_NEAR: [f32; 2] = [9.0, 28.0];
const THIEF_NEAR: [f32; 2] = [3.0, 15.0];
/// Within this of the owner (m), D-pad right gives the purse back; this far off it is kept.
const GIVE_REACH: f32 = 2.4;
const KEEP_AWAY: f32 = 60.0;
/// The purse is picked up within this (m).
const PICK_UP: f32 = 1.5;
/// A car this near the robber (m) and this slow (m/s) has stopped for them; they get in from
/// this near its door.
const BOARD_NEAR: f32 = 12.0;
const BOARD_SLOW: f32 = 2.5;
const DOOR_REACH: f32 = 1.6;
/// Stopped this near the drop-off (m), under this speed.
const DROP_NEAR: f32 = 14.0;
/// A robber left this far behind for this long (s) gives up.
const BEHIND_FOR: f32 = 8.0;

/// Clothes for people an activity puts in the street (`look` tables): each picked at random,
/// and what they wear (`wear`: hoods, beanies, sunglasses... as `crate::looks::Wear`).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookDef {
    #[serde(default)]
    pub skin: Vec<String>,
    #[serde(default)]
    pub shirt: Vec<String>,
    #[serde(default)]
    pub pants: Vec<String>,
    #[serde(default)]
    pub shoes: Vec<String>,
    #[serde(default)]
    pub wear: crate::looks::Wear,
}

fn pick(v: &[String], rng: &mut Rng) -> Option<String> {
    (!v.is_empty()).then(|| v[rng.below(v.len() as u32) as usize].clone())
}

impl LookDef {
    /// Someone dressed from the palettes (a missing palette keeps the biped's own) on the game's
    /// biped; what they wear from a fork of `rng` (`crate::looks`).
    pub fn puppet(&self, rng: &mut Rng) -> PuppetDef {
        let mut p = crate::looks::person();
        if let Some(c) = pick(&self.skin, rng) {
            p.skin = c;
        }
        if let Some(c) = pick(&self.shirt, rng) {
            p.shirt = c;
        }
        if let Some(c) = pick(&self.pants, rng) {
            p.pants = c;
        }
        if let Some(c) = pick(&self.shoes, rng) {
            p.shoes = c;
        }
        p.scale *= rng.range(0.96, 1.06);
        let build = rng.range(0.92, 1.12);
        p.torso_radius *= build;
        p.hip_width *= build;
        p.idle_clip = "MESH2MOTION/Idle_A".into();
        self.wear.dress(&mut p, rng);
        p
    }
}

/// `[[event]]` in `activities.toml` (the fields a kind does not use stay at their defaults).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventDef {
    pub id: String,
    /// `snatch` (a purse snatched) or `getaway` (a robber needs a ride).
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub title: String,
    /// Seconds of free roam without an event before this one may happen, and its chance at each
    /// check where it fits.
    #[serde(default)]
    pub after: f32,
    #[serde(default)]
    pub chance: f32,
    /// A snatch: the zones around the player, and people within 30 m.
    #[serde(default)]
    pub zones: Vec<String>,
    #[serde(default)]
    pub crowd: usize,
    /// A getaway: the shops' doors (anchors) and how far the player (in a car) is from one; the
    /// drop-offs (anchors) and how far they lie from the shop; the stars that come with it.
    #[serde(default)]
    pub near: Vec<String>,
    #[serde(default)]
    pub reach: [f32; 2],
    #[serde(default)]
    pub drops: Vec<String>,
    #[serde(default)]
    pub drop: [f32; 2],
    #[serde(default)]
    pub stars: u8,
    /// Cash: the purse given back or the ride; the purse kept.
    #[serde(default)]
    pub reward: i64,
    #[serde(default)]
    pub keep: i64,
    /// On foot: running speed and once tired (m/s); seconds before tiring; a thief's health.
    #[serde(default)]
    pub run: [f32; 2],
    #[serde(default)]
    pub tire: f32,
    #[serde(default)]
    pub hp: f32,
    /// This far from the player (m) the thief has got away, the robber is left behind.
    #[serde(default)]
    pub escape: f32,
    /// Seconds before it is given up (the thief's run, the robber's wait).
    #[serde(default)]
    pub wait: f32,
    /// The speaker's name on the subtitles, and the lines by moment.
    #[serde(default)]
    pub speaker: String,
    #[serde(default)]
    pub lines: BTreeMap<String, String>,
    /// The robber's clothes.
    #[serde(default)]
    pub look: LookDef,
}

impl EventDef {
    fn line(&self, moment: &str) -> Option<String> {
        self.lines.get(moment).cloned()
    }

    /// What is wrong with it against the district (empty: nothing).
    pub fn check(&self, city: &City) -> Vec<String> {
        let mut errs = Vec::new();
        let at = |a: &str, errs: &mut Vec<String>| match Anchor::parse(a) {
            Ok(x) if x.exists(city) => {}
            Ok(_) => errs.push(format!("event {}: '{a}' is not in the district", self.id)),
            Err(e) => errs.push(format!("event {}: {e}", self.id)),
        };
        match self.kind.as_str() {
            "snatch" => {
                if self.run[0] <= 0.0 || self.hp <= 0.0 || self.escape <= 0.0 {
                    errs.push(format!("event {}: a snatch needs run, hp and escape", self.id));
                }
            }
            "getaway" => {
                if self.near.is_empty() || self.drops.is_empty() {
                    errs.push(format!("event {}: a getaway needs near (shops) and drops", self.id));
                }
                for a in self.near.iter().chain(&self.drops) {
                    at(a, &mut errs);
                }
                if self.stars == 0 || self.stars > 5 {
                    errs.push(format!("event {}: stars must be 1-5", self.id));
                }
            }
            k => errs.push(format!("event {}: kind '{k}' is not snatch or getaway", self.id)),
        }
        if self.title.is_empty() {
            errs.push(format!("event {}: no title", self.id));
        }
        if let Err(e) = self.look.wear.check() {
            errs.push(format!("event {}: look wear: {e}", self.id));
        }
        errs
    }
}

/// An activity played at once with `activity <id>` (tools, tests): how it went.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Trial {
    #[default]
    No,
    Playing,
    Won,
    Lost,
}

/// Someone running from the player on foot (a thief, a suspect out of their car): along the walk
/// map to a point well away, a new one when there, every few seconds or with no headway; slower
/// once tired.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Runner {
    pub path: Vec<Vec2>,
    pub replan: f32,
    /// Seconds running.
    pub t: f32,
    /// Where it stood a moment ago, and seconds since.
    pub check: Vec2,
    pub check_t: f32,
}

impl Runner {
    /// This tick's input for `id` running from `from`.
    #[allow(clippy::too_many_arguments)]
    pub fn flee(
        &mut self,
        sim: &mut Sim,
        city: &City,
        id: EntityId,
        from: Vec3,
        run: [f32; 2],
        tire: f32,
        dt: f32,
    ) -> InputFrame {
        let Some(feet) = sim.feet(id) else { return InputFrame::default() };
        let here = Vec2::new(feet.x, feet.z);
        let threat = Vec2::new(from.x, from.z);
        self.t += dt;
        self.replan -= dt;
        self.check_t += dt;
        if self.check_t > 1.5 {
            if self.check.distance(here) < 1.0 {
                self.replan = 0.0;
            }
            self.check = here;
            self.check_t = 0.0;
        }
        while self.path.len() > 1 && self.path[0].distance(here) < 1.0 {
            self.path.remove(0);
        }
        let there = self.path.len() <= 1 && self.path.first().is_none_or(|p| p.distance(here) < 2.0);
        if self.replan <= 0.0 || there {
            self.replan = 4.0;
            let walk = &city.walk;
            let start = walk.nearest_walkable(here, 6).unwrap_or(here);
            let away = (here - threat).normalize_or(Vec2::X);
            for _ in 0..8 {
                let a = away.y.atan2(away.x) + (sim.state.rng.f32() - 0.5) * 2.2;
                let to = here + Vec2::new(a.cos(), a.sin()) * sim.state.rng.range(45.0, 80.0);
                let t = walk.tile_of(to);
                if matches!(walk.get(t), Walk::Sidewalk | Walk::Path | Walk::Open) {
                    let p = walk.path(start, walk.centre(t), 4000);
                    if p.len() >= 2 {
                        self.path = p;
                        break;
                    }
                }
            }
        }
        let speed = if self.t > tire { run[1] } else { run[0] };
        if let Some(c) = sim.state.entities.get_mut(id).and_then(|e| e.character.as_mut()) {
            c.haste = speed / crate::player::RUN - 1.0;
            c.face = None;
        }
        let dir = match self.path.first() {
            Some(p) if p.distance(here) > 0.2 => (*p - here).normalize_or(Vec2::ZERO),
            _ => (here - threat).normalize_or(Vec2::X),
        };
        InputFrame { move_dir: dir, ..Default::default() }
    }
}

/// Walks (or runs, `speed` m/s) `id` toward `to`: over the walk map from afar, straight there
/// when near.
pub(crate) fn walk_to(sim: &mut Sim, city: &City, id: EntityId, to: Vec3, speed: f32) -> InputFrame {
    let Some(feet) = sim.feet(id) else { return InputFrame::default() };
    let here = Vec2::new(feet.x, feet.z);
    let goal = Vec2::new(to.x, to.z);
    if here.distance(goal) < 0.4 {
        return InputFrame::default();
    }
    let next = if here.distance(goal) > 12.0 {
        match (city.walk.nearest_walkable(here, 4), city.walk.nearest_walkable(goal, 6)) {
            (Some(a), Some(b)) => city.walk.path(a, b, 2500).into_iter().find(|p| p.distance(here) > 1.2).unwrap_or(goal),
            _ => goal,
        }
    } else {
        goal
    };
    if let Some(c) = sim.state.entities.get_mut(id).and_then(|e| e.character.as_mut()) {
        c.haste = speed / crate::player::RUN - 1.0;
        c.face = None;
    }
    InputFrame { move_dir: (next - here).normalize_or(Vec2::ZERO), ..Default::default() }
}

/// The on-foot bot's headway: where it stood a moment ago and seconds since.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Headway {
    pub at: Vec2,
    pub t: f32,
}

impl Headway {
    /// Chasing someone who runs, the walk's own stuck check never fires (its goal keeps moving):
    /// no headway for a second and a half (a parked car in the way) is a step aside, the other
    /// way each time, or every third time a jump at it.
    pub fn check(&mut self, b: &mut MissionBot, here: Vec2, dt: f32) {
        self.t += dt;
        if self.at.distance(here) > 1.0 {
            self.at = here;
            self.t = 0.0;
            b.tries = 0;
        } else if self.t > 1.5 && b.detour <= 0.0 && b.hop <= 0.0 {
            self.t = 0.0;
            b.unstick();
        }
    }
}

/// Full detail near the player, low detail beyond 30 m (the crowd's rule, E11).
pub(crate) fn detail(sim: &mut Sim, id: EntityId, player: Vec3) {
    let Some(f) = sim.feet(id) else { return };
    let d = Vec2::new(f.x - player.x, f.z - player.z).length();
    if let Some(c) = sim.state.entities.get_mut(id).and_then(|e| e.character.as_mut()) {
        c.lod = if c.lod { d > 24.0 } else { d > 30.0 };
    }
}

/// Sets someone's standing clip (an empty name: the procedural idle).
pub(crate) fn set_idle(sim: &mut Sim, id: EntityId, clip: &str) {
    let Some(c) = sim.state.entities.get_mut(id).and_then(|e| e.character.as_mut()) else { return };
    let Some(look) = c.puppet.as_deref() else { return };
    if look.idle_clip != clip {
        c.puppet = Some(std::sync::Arc::new(PuppetDef { idle_clip: clip.to_string(), ..look.clone() }));
    }
}

/// The idle clip someone stands with now.
pub(crate) fn idle_of(sim: &Sim, id: EntityId) -> String {
    sim.state
        .entities
        .get(id)
        .and_then(|e| e.character.as_ref()?.puppet.as_ref().map(|p| p.idle_clip.clone()))
        .unwrap_or_default()
}

/// Turns someone to face a point.
pub(crate) fn face(sim: &mut Sim, id: EntityId, to: Vec3) {
    let Some(f) = sim.feet(id) else { return };
    let d = Vec2::new(to.x - f.x, to.z - f.z);
    if let Some(c) = sim.state.entities.get_mut(id).and_then(|e| e.character.as_mut())
        && d.length() > 0.3
    {
        c.face = Some(d.x.atan2(d.y));
    }
}

/// Whether someone is out of it: gone, or dead.
pub(crate) fn gone_or_dead(sim: &Sim, id: EntityId) -> bool {
    sim.state.entities.get(id).is_none_or(|e| e.name.starts_with("dead:"))
}

/// The zone the player is in (roads have none: the most common one round about).
pub(crate) fn area_zone(city: &City, at: Vec3) -> Option<String> {
    let mut counts: BTreeMap<&str, u32> = BTreeMap::new();
    for z in -2..=2 {
        for x in -2..=2 {
            if let Some(zone) = city.zone_at(Vec2::new(at.x + x as f32 * 12.0, at.z + z as f32 * 12.0)) {
                *counts.entry(zone).or_default() += 1;
            }
        }
    }
    counts.into_iter().max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(a.0))).map(|(z, _)| z.to_string())
}

/// A line said in the street: a subtitle with a babble voice.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Said {
    pub who: String,
    pub text: String,
    /// Seconds into it, and the voice's pitch.
    pub t: f32,
    pub voice: f32,
}

impl Said {
    pub fn new(who: &str, text: String, voice: f32) -> Said {
        Said { who: who.to_uppercase(), text, t: 0.0, voice }
    }

    /// How long it takes to say.
    pub fn length(&self) -> f32 {
        (0.9 + self.text.chars().count() as f32 * 0.055).clamp(1.6, 5.0)
    }

    /// One tick: a syllable every so often while it is said. False once it has been shown long
    /// enough.
    pub fn tick(&mut self, dt: f32, events: &mut Vec<SimEvent>) -> bool {
        let syl = |x: f32| (x / 0.11) as usize;
        let before = self.t;
        self.t += dt;
        if syl(self.t) != syl(before) && self.t < self.length() - 0.3 {
            let wobble = ((syl(self.t) * 7919) % 13) as f32 / 13.0 - 0.5;
            events.push(SimEvent::Sound {
                cue: "voice.babble".into(),
                pos: None,
                gain: 0.5,
                pitch: self.voice * (1.0 + wobble * 0.25),
            });
        }
        self.t < self.length() + 0.6
    }

    pub fn subtitle(&self) -> Subtitle {
        Subtitle { speaker: Some(self.who.clone()), text: self.text.clone(), color: Some(Color::hex("#f2efe6")) }
    }
}

/// A banner over play for a few seconds.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Flash {
    pub kind: BannerKind,
    pub title: String,
    pub sub: Option<String>,
    pub age: f32,
}

/// How long a banner shows (s).
pub const FLASH: f32 = 3.5;

impl Flash {
    pub fn new(kind: BannerKind, title: &str, sub: Option<String>) -> Flash {
        Flash { kind, title: title.into(), sub, age: 0.0 }
    }

    pub fn banner(&self) -> Banner {
        Banner { kind: self.kind, title: self.title.clone(), subtitle: self.sub.clone(), age: self.age, duration: FLASH }
    }
}

/// Where a purse snatching has got to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SnatchStep {
    /// The thief closes in on the owner.
    Approach,
    /// The thief runs with the purse.
    Run,
    /// The thief is down and the purse lies on the ground.
    Purse,
    /// The player has it: back to its owner, or away with it.
    Carry,
    /// Over: the owner thanks the player (or stands there), then everyone carries on.
    After,
}

/// A purse snatching.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snatch {
    pub owner: EntityId,
    pub thief: EntityId,
    /// Their places in the crowd (they go back to it after; None: made for the event).
    pub owner_ped: Option<Ped>,
    pub thief_ped: Option<Ped>,
    /// Their own standing clips.
    pub owner_idle: String,
    pub thief_idle: String,
    pub step: SnatchStep,
    /// Seconds in this step.
    pub t: f32,
    pub runner: Runner,
    /// Where it happened (the owner waits there), and where the purse lies.
    pub spot: Vec3,
    pub purse: Vec3,
    /// The thief's health at the start (a hit takes them down), and seconds they stay down.
    pub hp: f32,
    pub thief_down: f32,
    /// The thief is off empty-handed (back with the crowd).
    pub thief_free: bool,
    /// The purse went back to its owner.
    pub given: bool,
}

/// Where a getaway has got to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RideStep {
    /// Waving by the shop for a ride.
    Wait,
    /// Running to the car that stopped.
    Board,
    /// Riding (or following the player to the next car): lose the police, then the drop-off.
    Ride,
    /// Out at the drop-off, walking off.
    Off,
}

/// A robber who needs a getaway driver.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Getaway {
    pub robber: EntityId,
    pub step: RideStep,
    pub t: f32,
    /// The shop (its name, its door, the road beside it), the drop-off (its name, a point on
    /// its road).
    pub shop: String,
    pub door: Vec3,
    pub kerb: Vec3,
    pub drop_name: String,
    pub drop: Vec3,
    /// The car they ride in (None: on foot).
    pub car: Option<EntityId>,
    /// Seconds left too far behind the player.
    pub behind: f32,
    /// Told to lose the police first (once each time at the drop-off).
    pub told: bool,
}

/// What kind of event, and its own state.
#[allow(clippy::large_enum_variant)] // one at a time
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Happening {
    Snatch(Snatch),
    Getaway(Getaway),
}

/// The event under way.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Live {
    pub id: String,
    /// Seconds since it began.
    pub age: f32,
    pub what: Happening,
    /// A line being said.
    pub said: Option<Said>,
    /// Finished (done or not): seconds of its aftermath left before everyone carries on.
    pub over: Option<f32>,
}

/// The events (part of the game's state).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct EventState {
    pub on: Option<Live>,
    /// Seconds of free roam since the last event ended (or the start), and to the next check.
    pub quiet: f32,
    pub check: f32,
    /// `events off`: none happen by chance (tests, the attract mode).
    pub off: bool,
    /// The bot's way with a purse: keep it (`event keep`), else give it back.
    pub keep: bool,
    /// Counts: begun, done, failed.
    pub started: u32,
    pub finished: u32,
    pub failed: u32,
    /// The last one's end, for `status` and tests.
    pub last: String,
    /// An event started with `activity <id>`: how it went.
    pub trial: Trial,
    pub banner: Option<Flash>,
    /// The bot: its memory, and where it walks with a purse it keeps.
    pub bot: MissionBot,
    pub away: Option<Vec3>,
    pub headway: Headway,
}

/// What a tick of an event decided.
enum Flow {
    On,
    /// Finished: done (true) or not, the toast's words, then this many seconds of aftermath.
    End(bool, String, f32),
}

impl Freeway {
    /// Commands: `activity <event id>` (start it now as a trial), `event start <id>`, `event
    /// stop`, `events on|off|ready` (ready: the quiet time is up), `event keep|give` (the bot's
    /// way with a purse).
    fn event_cmds(&mut self, sim: &mut Sim, d: &ActivitiesDef, input: &InputFrame, events: &mut Vec<SimEvent>) {
        for c in &input.cmds {
            let mut w = c.split_whitespace();
            match (w.next(), w.next(), w.next()) {
                (Some("activity"), Some(id), None) if d.event.iter().any(|e| e.id == id) => {
                    if let Some(e) = d.event.iter().find(|e| e.id == id) {
                        if e.kind == "getaway" && self.drive.vehicle().is_none() {
                            self.car_cmd(sim, "fenwick_commuter", None, true);
                        }
                        let ok = self.event_start(sim, e, true, events);
                        self.activities.events.trial = if ok { Trial::Playing } else { Trial::Lost };
                    }
                }
                (Some("event"), Some("start"), Some(id)) => match d.event.iter().find(|e| e.id == id) {
                    Some(e) => {
                        self.event_start(sim, e, true, events);
                    }
                    None => log::warn!("event: no event '{id}' (activities.toml)"),
                },
                (Some("event"), Some("stop"), None) => {
                    if let Some(live) = self.activities.events.on.take() {
                        self.event_release(sim, live);
                        self.activities.events.quiet = 0.0;
                    }
                }
                (Some("event"), Some(way @ ("keep" | "give")), None) => self.activities.events.keep = way == "keep",
                (Some("events"), Some(on @ ("on" | "off")), None) => self.activities.events.off = on == "off",
                // The quiet time up: one may happen at the next check where it fits (tests).
                (Some("events"), Some("ready"), None) => {
                    self.activities.events.quiet = 1.0e6;
                    self.activities.events.check = 0.0;
                }
                _ => {}
            }
        }
    }

    /// The random events' tick (outside missions): commands, the chance of one, the one under
    /// way, the bot.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn events_tick(
        &mut self,
        sim: &mut Sim,
        d: &ActivitiesDef,
        input: &InputFrame,
        dt: f32,
        events: &mut Vec<SimEvent>,
        out: &mut BTreeMap<EntityId, InputFrame>,
    ) {
        self.event_cmds(sim, d, input, events);
        if let Some(b) = &mut self.activities.events.banner {
            b.age += dt;
            if b.age > FLASH {
                self.activities.events.banner = None;
            }
        }
        if self.activities.events.on.is_none() {
            self.activities.events.quiet += dt;
            self.event_chance(sim, d, dt, events);
        }
        let Some(mut live) = self.activities.events.on.take() else {
            self.activities.events.bot.frame = InputFrame::default();
            return;
        };
        let Some(def) = d.event.iter().find(|e| e.id == live.id).cloned() else {
            self.event_release(sim, live);
            return;
        };
        live.age += dt;
        if let Some(s) = &mut live.said
            && !s.tick(dt, events)
        {
            live.said = None;
        }
        if let Some(left) = &mut live.over {
            *left -= dt;
        }
        let flow = match &mut live.what {
            Happening::Snatch(s) => self.snatch_tick(sim, &def, s, &mut live.said, live.over.is_some(), input, dt, events, out),
            Happening::Getaway(g) => self.getaway_tick(sim, &def, g, &mut live.said, live.over.is_some(), dt, events, out),
        };
        if let Flow::End(done, words, after) = flow
            && live.over.is_none()
        {
            live.over = Some(after);
            self.event_ended(&def, done, &words, events);
        }
        if live.over.is_some_and(|t| t <= 0.0) {
            self.event_release(sim, live);
            self.activities.events.bot.frame = InputFrame::default();
            return;
        }
        self.activities.events.on = Some(live);
        self.events_bot(sim, &def, dt);
    }

    /// Done or not: the record, the toast, completion, a trial's outcome.
    fn event_ended(&mut self, def: &EventDef, done: bool, words: &str, events: &mut Vec<SimEvent>) {
        let st = &mut self.activities.events;
        st.last = format!("{}: {}", def.id, if done { "done" } else { "failed" });
        if st.trial == Trial::Playing {
            st.trial = if done { Trial::Won } else { Trial::Lost };
        }
        if words.is_empty() {
            // (It never came to anything the player saw.)
            return;
        }
        if done {
            st.finished += 1;
            self.activity_done(&format!("event:{}", def.id));
            self.autosave = true;
            events.push(SimEvent::Sound { cue: "mission.pass".into(), pos: None, gain: 0.8, pitch: 1.0 });
        } else {
            self.activities.events.failed += 1;
            events.push(SimEvent::Sound { cue: "mission.fail".into(), pos: None, gain: 0.6, pitch: 1.0 });
        }
        self.note(if done { "star" } else { "info" }, &def.title, words);
    }

    /// Whether the street is free for something to happen (alive, nothing else on, no police
    /// after the player, nobody's menu or switch, not on autopilot).
    fn street_quiet(&self) -> bool {
        self.fate.alive()
            && self.missions.run.is_none()
            && self.activities.vigilante.run.is_none()
            && self.heat.stars == 0
            && self.crew.switch.is_none()
            && self.shop.is_none()
            && !self.phone.open
            && !self.drive.busy()
            && !self.bot.escape
            && !self.traffic.cars.iter().any(|a| a.pilot)
            && self.front.page == crate::frontend::Page::Play
            && self.view.is_none()
            && self.mode != "tour"
    }

    /// Every few seconds of quiet: maybe an event that fits here.
    fn event_chance(&mut self, sim: &mut Sim, d: &ActivitiesDef, dt: f32, events: &mut Vec<SimEvent>) {
        let st = &mut self.activities.events;
        st.check -= dt;
        if st.off || st.check > 0.0 {
            return;
        }
        st.check = CHECK;
        if !self.street_quiet() {
            return;
        }
        for e in &d.event {
            if self.activities.events.quiet < e.after || !self.event_fits(sim, e) {
                continue;
            }
            if sim.state.rng.f32() < e.chance && self.event_start(sim, e, false, events) {
                return;
            }
        }
    }

    /// Whether the player is where event `e` happens.
    fn event_fits(&self, sim: &Sim, e: &EventDef) -> bool {
        let Some(city) = self.city.as_ref() else { return false };
        match e.kind.as_str() {
            "snatch" => {
                let feet = Self::feet(sim);
                if self.drive.vehicle().is_some()
                    || !matches!(city.walk.at(Vec2::new(feet.x, feet.z)), Walk::Sidewalk | Walk::Path | Walk::Open)
                {
                    return false;
                }
                let zone = area_zone(city, feet);
                let near = self.peds.list.iter().filter_map(|p| sim.feet(p.id)).filter(|f| f.distance(feet) < 30.0).count();
                zone.is_some_and(|z| e.zones.is_empty() || e.zones.contains(&z)) && near >= e.crowd
            }
            "getaway" => {
                let Some(car) = self.drive.vehicle() else { return false };
                if self.police_car(sim, car) {
                    return false;
                }
                let me = self.player_point(sim);
                e.near.iter().any(|a| {
                    Anchor::parse(a)
                        .ok()
                        .and_then(|x| anchor::point(city, &x))
                        .is_some_and(|p| (e.reach[0]..e.reach[1]).contains(&Vec2::new(p.x - me.x, p.z - me.z).length()))
                })
            }
            _ => false,
        }
    }

    /// Whether `car` is a police vehicle (robbers do not get into those).
    pub(crate) fn police_car(&self, sim: &Sim, car: EntityId) -> bool {
        Car::read(sim, car).is_some_and(|c| c.siren)
    }

    /// Starts event `e` here and now (`forced`: wherever the player is, ignoring where it should
    /// happen). False when there is no way to stage it.
    pub(crate) fn event_start(&mut self, sim: &mut Sim, e: &EventDef, forced: bool, events: &mut Vec<SimEvent>) -> bool {
        if let Some(old) = self.activities.events.on.take() {
            self.event_release(sim, old);
        }
        let what = match e.kind.as_str() {
            "snatch" => self.snatch_start(sim, e, forced).map(Happening::Snatch),
            "getaway" => self.getaway_start(sim, e, forced).map(Happening::Getaway),
            _ => None,
        };
        let Some(what) = what else { return false };
        let said = match &what {
            Happening::Getaway(_) => e.line("beg").map(|t| Said::new(&e.speaker, t, 0.85)),
            _ => None,
        };
        self.activities.events.on = Some(Live { id: e.id.clone(), age: 0.0, what, said, over: None });
        self.activities.events.started += 1;
        self.activities.events.bot = MissionBot::default();
        self.activities.events.away = None;
        if matches!(e.kind.as_str(), "getaway") {
            self.activities.events.banner = Some(Flash::new(BannerKind::Info, &e.title.to_uppercase(), None));
            events.push(SimEvent::Sound { cue: "mission.start".into(), pos: None, gain: 0.7, pitch: 1.0 });
        }
        true
    }

    /// Everyone the event took back to the street: the owner and the thief to the crowd, the
    /// robber off on foot.
    fn event_release(&mut self, sim: &mut Sim, live: Live) {
        self.activities.events.quiet = 0.0;
        self.activities.events.check = CHECK;
        let Some(city) = self.city.clone() else { return };
        let me = Self::feet(sim);
        match live.what {
            Happening::Snatch(s) => {
                self.back_to_crowd(sim, &city, s.owner, s.owner_ped, &s.owner_idle, None);
                if !s.thief_free && !gone_or_dead(sim, s.thief) {
                    let path = if s.runner.path.is_empty() { None } else { Some(s.runner.path.clone()) };
                    self.back_to_crowd(sim, &city, s.thief, s.thief_ped, &s.thief_idle, path.or_else(|| Some(Vec::new())));
                }
                self.combat.hp.remove(&s.thief.0);
                let _ = me;
            }
            Happening::Getaway(g) => {
                if let Some(car) = g.car
                    && sim.state.entities.get(g.robber).and_then(|e| e.character.as_ref()).is_some_and(|c| c.riding == Some(car))
                {
                    sim.exit_vehicle(g.robber, car);
                }
                if !gone_or_dead(sim, g.robber) {
                    self.back_to_crowd(sim, &city, g.robber, None, "", Some(Vec::new()));
                }
            }
        }
    }

    /// Someone an activity held goes back to the crowd: lingering a moment, or running off
    /// (`run`: along that way, or one away from the player when empty). `ped`: their old place
    /// (None: they join as a local).
    pub(crate) fn back_to_crowd(
        &mut self,
        sim: &mut Sim,
        city: &City,
        id: EntityId,
        ped: Option<Ped>,
        idle: &str,
        run: Option<Vec<Vec2>>,
    ) {
        let Some(feet) = sim.feet(id) else { return };
        if sim.state.entities.get(id).is_some_and(|e| e.name.starts_with("dead:")) {
            return;
        }
        if sim.state.entities.get(id).and_then(|e| e.character.as_ref()).is_some_and(|c| c.riding.is_some()) {
            return;
        }
        let def = crate::peds::def().ok();
        let arch = ped
            .as_ref()
            .map(|p| p.arch)
            .unwrap_or_else(|| def.as_ref().and_then(|d| d.archetype.iter().position(|a| a.id == "local")).unwrap_or(0) as u16);
        if !idle.is_empty() {
            set_idle(sim, id, idle);
        }
        if let Some(c) = sim.state.entities.get_mut(id).and_then(|e| e.character.as_mut()) {
            c.anim.stop_clip();
        }
        let pace = def.as_ref().and_then(|d| d.archetype.get(arch as usize)).map_or(1.4, |a| (a.pace[0] + a.pace[1]) * 0.5);
        let walks_fast = def.as_ref().and_then(|d| d.archetype.get(arch as usize)).is_some_and(|a| a.run);
        let here = Vec2::new(feet.x, feet.z);
        let (doing, path, running) = match run {
            Some(mut p) => {
                if p.len() < 2 {
                    let me = Self::feet(sim);
                    let away = (here - Vec2::new(me.x, me.z)).normalize_or(Vec2::X);
                    let to = here + away * 60.0;
                    if let (Some(a), Some(b)) = (city.walk.nearest_walkable(here, 6), city.walk.nearest_walkable(to, 8)) {
                        p = city.walk.path(a, b, 4000);
                    }
                }
                (if p.len() >= 2 { Doing::Flee } else { Doing::Linger }, p, true)
            }
            None => (Doing::Linger, Vec::new(), walks_fast),
        };
        if let Some(c) = sim.state.entities.get_mut(id).and_then(|e| e.character.as_mut()) {
            c.face = None;
            c.haste = if running { 0.1 } else { pace / if walks_fast { crate::player::RUN } else { crate::player::WALK } - 1.0 };
        }
        crate::world::ambient(sim, id);
        self.peds.list.retain(|p| p.id != id);
        let base = ped.unwrap_or(Ped {
            id,
            arch,
            doing: Doing::Linger,
            path: Vec::new(),
            next: 0,
            timer: 0.0,
            check_t: 0.0,
            check: here,
            run: walks_fast,
            focus: None,
            seat: None,
            idle: None,
            swing: 0.0,
        });
        self.peds.list.push(Ped {
            id,
            doing,
            next: 1.min(path.len()),
            path,
            timer: 1.5,
            check_t: 0.0,
            check: here,
            run: running || walks_fast,
            focus: None,
            seat: None,
            idle: None,
            swing: 0.0,
            ..base
        });
    }

    /// Takes someone out of the crowd for an event (their place, to give back after).
    fn out_of_crowd(&mut self, sim: &mut Sim, id: EntityId) -> Option<Ped> {
        let p = self.peds.list.iter().find(|p| p.id == id).cloned()?;
        self.peds.forget(id);
        if let Some(c) = sim.state.entities.get_mut(id).and_then(|e| e.character.as_mut()) {
            c.face = None;
            c.lod = false;
        }
        if let Some(e) = sim.state.entities.get_mut(id) {
            e.unload = pav_core::entity::Unload::Keep;
        }
        Some(p)
    }

    /// Someone new for an event, standing at `at` facing `facing`.
    fn street_person(&mut self, sim: &mut Sim, name: &str, at: Vec3, facing: f32, look: &LookDef) -> EntityId {
        let puppet = if look.shirt.is_empty() {
            crate::peds::def()
                .ok()
                .and_then(|d| d.archetype.iter().find(|a| a.id == "local").cloned())
                .map(|a| crate::peds::look(&a, &mut sim.state.rng))
                .unwrap_or_default()
        } else {
            look.puppet(&mut sim.state.rng)
        };
        let id = sim.spawn_npc(name, at, facing, puppet, None, None);
        if let Some(e) = sim.state.entities.get_mut(id) {
            e.unload = pav_core::entity::Unload::Keep;
        }
        id
    }

    // ---- Purse Snatcher ----

    /// The owner (someone in the crowd near the player, in front, on the pavement) and the thief
    /// (another close by them, farther from the player); forced, people made for it where the
    /// crowd has none.
    fn snatch_start(&mut self, sim: &mut Sim, e: &EventDef, forced: bool) -> Option<Snatch> {
        let city = self.city.clone()?;
        let me = Self::feet(sim);
        let cam = self.camera(sim);
        let look = Vec2::new(cam.target.x - cam.eye.x, cam.target.z - cam.eye.z).normalize_or(Vec2::Y);
        let flat = |p: Vec3| Vec2::new(p.x, p.z);
        let free = |p: &Ped| matches!(p.doing, Doing::Walk | Doing::Linger | Doing::Wait) && p.idle.is_none() && p.seat.is_none();
        let ok_tile = |p: Vec3| matches!(city.walk.at(flat(p)), Walk::Sidewalk | Walk::Path | Walk::Open);
        let owners: Vec<(EntityId, Vec3)> = self
            .peds
            .list
            .iter()
            .filter(|p| free(p))
            .filter_map(|p| Some((p.id, sim.feet(p.id)?)))
            .filter(|(_, f)| {
                let d = flat(*f - me);
                (OWNER_NEAR[0]..OWNER_NEAR[1]).contains(&d.length())
                    && d.normalize_or(Vec2::ZERO).dot(look) > 0.25
                    && ok_tile(*f)
                    && (f.y - me.y).abs() < 2.0
                    && crate::heat::sight(sim, me + Vec3::Y * 1.6, *f + Vec3::Y)
            })
            .collect();
        let mut pairs: Vec<(EntityId, Vec3, EntityId, Vec3)> = Vec::new();
        for (o, of) in &owners {
            for p in self.peds.list.iter().filter(|p| free(p) && p.id != *o) {
                let Some(tf) = sim.feet(p.id) else { continue };
                let d = flat(tf - *of).length();
                if (THIEF_NEAR[0]..THIEF_NEAR[1]).contains(&d)
                    && flat(tf - me).length() > flat(*of - me).length() - 3.0
                    && ok_tile(tf)
                {
                    pairs.push((*o, *of, p.id, tf));
                }
            }
        }
        let (owner, spot, owner_ped, thief, thief_ped) = if !pairs.is_empty() {
            let (o, of, t, _) = pairs[sim.state.rng.below(pairs.len() as u32) as usize];
            let op = self.out_of_crowd(sim, o);
            let tp = self.out_of_crowd(sim, t);
            (o, of, op, t, tp)
        } else if forced {
            // Nobody suitable about: someone walking ahead, and a thief behind them.
            let ahead = flat(me) + look * 14.0;
            let at = city.walk.nearest_walkable(ahead, 8)?;
            let of = Vec3::new(at.x, city.ground_y(at), at.y);
            let behind = city.walk.nearest_walkable(at + look * 9.0, 8).unwrap_or(at + look * 9.0);
            let tf = Vec3::new(behind.x, city.ground_y(behind), behind.y);
            let o = self.street_person(sim, "ped:local", of, std::f32::consts::PI, &LookDef::default());
            let t = self.street_person(sim, "ped:local", tf, 0.0, &e.look);
            (o, of, None, t, None)
        } else {
            return None;
        };
        let owner_idle = idle_of(sim, owner);
        let thief_idle = idle_of(sim, thief);
        Some(Snatch {
            owner,
            thief,
            owner_ped,
            thief_ped,
            owner_idle,
            thief_idle,
            step: SnatchStep::Approach,
            t: 0.0,
            runner: Runner::default(),
            spot,
            purse: spot,
            hp: e.hp,
            thief_down: 0.0,
            thief_free: false,
            given: false,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn snatch_tick(
        &mut self,
        sim: &mut Sim,
        e: &EventDef,
        s: &mut Snatch,
        said: &mut Option<Said>,
        over: bool,
        input: &InputFrame,
        dt: f32,
        events: &mut Vec<SimEvent>,
        out: &mut BTreeMap<EntityId, InputFrame>,
    ) -> Flow {
        let Some(city) = self.city.clone() else { return Flow::End(false, "The thief got away.".into(), 0.0) };
        let me = Self::feet(sim);
        s.t += dt;
        detail(sim, s.owner, me);
        detail(sim, s.thief, me);
        let owner_ok = !gone_or_dead(sim, s.owner);
        let thief_ok = !gone_or_dead(sim, s.thief);
        // The owner stays where it happened (turned to the player once they come near).
        if owner_ok && s.step != SnatchStep::Approach {
            out.insert(s.owner, InputFrame::default());
            if sim.feet(s.owner).is_some_and(|f| f.distance(me) < 20.0) {
                face(sim, s.owner, me);
            }
        }
        if !self.fate.alive() && !over {
            return Flow::End(false, "The thief got away.".into(), 0.0);
        }
        // The thief, down a moment after the hit, then off empty-handed.
        if s.thief_down > 0.0 && thief_ok {
            s.thief_down -= dt;
            out.insert(s.thief, InputFrame::default());
            if s.thief_down <= 0.0 && !s.thief_free {
                s.thief_free = true;
                let ped = s.thief_ped.take();
                let idle = s.thief_idle.clone();
                self.back_to_crowd(sim, &city, s.thief, ped, &idle, Some(Vec::new()));
            }
        }
        match s.step {
            SnatchStep::Approach => {
                if !owner_ok || !thief_ok {
                    return Flow::End(false, String::new(), 0.0);
                }
                out.insert(s.owner, InputFrame::default());
                let Some(of) = sim.feet(s.owner) else { return Flow::On };
                let tf = sim.feet(s.thief).unwrap_or(of);
                if tf.distance(of) < 1.3 || (s.t > 8.0 && tf.distance(of) < 5.0) {
                    // The snatch: a shove, the purse gone, a shout; people near scatter.
                    s.step = SnatchStep::Run;
                    s.t = 0.0;
                    s.spot = of;
                    crate::play::play_clip(sim, s.owner, "QUATERNIUS/Hit_Chest", pav_core::clips::ONCE);
                    set_idle(sim, s.owner, "MESH2MOTION/Help_One_Arm");
                    self.combat.hp.insert(s.thief.0, s.hp);
                    *said = e.line("snatch").map(|t| Said::new(&e.speaker, t, 1.25));
                    self.peds.scare(sim, &city.walk, of, 9.0);
                    self.activities.events.banner = Some(Flash::new(BannerKind::Info, &e.title.to_uppercase(), None));
                    events.push(SimEvent::Sound { cue: "mission.start".into(), pos: None, gain: 0.7, pitch: 1.0 });
                } else if s.t > 12.0 {
                    return Flow::End(false, String::new(), 0.0);
                } else {
                    let f = walk_to(sim, &city, s.thief, of, e.run[0]);
                    out.insert(s.thief, f);
                }
            }
            SnatchStep::Run => {
                let hp = self.combat.hp.get(&s.thief.0).copied().unwrap_or(s.hp);
                let hit = !thief_ok || hp < s.hp - 0.01;
                if hit {
                    // Down: the purse drops at their feet.
                    let at = sim.feet(s.thief).unwrap_or(me);
                    s.purse = at;
                    s.step = SnatchStep::Purse;
                    s.t = 0.0;
                    if thief_ok {
                        crate::play::play_clip(sim, s.thief, "MESH2MOTION/Hit_Knockback", pav_core::clips::ONCE);
                        s.thief_down = 2.6;
                    }
                    set_idle(sim, s.owner, &s.owner_idle.clone());
                    events.push(SimEvent::Sound { cue: "melee.hit".into(), pos: Some(at), gain: 0.8, pitch: 0.8 });
                } else if sim.feet(s.thief).is_none_or(|f| f.distance(me) > e.escape) || s.t > e.wait {
                    *said = e.line("gone").map(|t| Said::new(&e.speaker, t, 1.25));
                    return Flow::End(false, "The thief got away.".into(), 4.0);
                } else {
                    let f = s.runner.flee(sim, &city, s.thief, me, e.run, e.tire, dt);
                    out.insert(s.thief, f);
                }
            }
            SnatchStep::Purse => {
                let on_foot = self.drive.vehicle().is_none();
                if on_foot && Vec2::new(s.purse.x - me.x, s.purse.z - me.z).length() < PICK_UP && (s.purse.y - me.y).abs() < 2.0 {
                    s.step = SnatchStep::Carry;
                    s.t = 0.0;
                    events.push(SimEvent::Sound { cue: "ui.pickup".into(), pos: None, gain: 1.0, pitch: 1.0 });
                    self.note("cash", "Purse", "Give it back to its owner, or keep it.");
                } else if s.t > e.wait {
                    return Flow::End(false, "Someone else picked the purse up.".into(), 0.0);
                }
            }
            SnatchStep::Carry => {
                let near = owner_ok && sim.feet(s.owner).is_some_and(|f| Vec2::new(f.x - me.x, f.z - me.z).length() < GIVE_REACH);
                let give = near
                    && self.drive.vehicle().is_none()
                    && !self.phone.open
                    && self.shop.is_none()
                    && (input.pad.pressed & pad::RIGHT != 0 || input.cmds.iter().any(|c| c == "ui.accept give"));
                if give {
                    s.step = SnatchStep::After;
                    s.given = true;
                    s.t = 0.0;
                    self.pay(e.reward);
                    events.push(SimEvent::Sound { cue: "ui.cash".into(), pos: None, gain: 1.0, pitch: 1.0 });
                    crate::play::play_clip(sim, s.owner, "MESH2MOTION/Cheering_Two_Hands", pav_core::clips::ONCE);
                    *said = e.line("thanks").map(|t| Said::new(&e.speaker, t, 1.25));
                    return Flow::End(true, format!("You gave the purse back. +${}", e.reward), 4.0);
                }
                let far = !owner_ok || sim.feet(s.owner).is_none_or(|f| Vec2::new(f.x - me.x, f.z - me.z).length() > KEEP_AWAY);
                if far {
                    s.step = SnatchStep::After;
                    s.t = 0.0;
                    self.pay(e.keep);
                    events.push(SimEvent::Sound { cue: "ui.cash".into(), pos: None, gain: 1.0, pitch: 1.0 });
                    return Flow::End(true, format!("You kept the purse. +${}", e.keep), 0.0);
                }
            }
            SnatchStep::After => {}
        }
        Flow::On
    }

    // ---- Getaway Needed ----

    /// A robber at the shop door nearest the player (within its reach unless `forced`), and a
    /// drop-off the right distance from it.
    fn getaway_start(&mut self, sim: &mut Sim, e: &EventDef, forced: bool) -> Option<Getaway> {
        let city = self.city.clone()?;
        let me = self.player_point(sim);
        let dist = |p: Vec3| Vec2::new(p.x - me.x, p.z - me.z).length();
        let mut shops: Vec<(f32, String, Vec3, Vec3)> = e
            .near
            .iter()
            .filter_map(|a| {
                let x = Anchor::parse(a).ok()?;
                let (door, _) = anchor::foot(&city, &x)?;
                let (kerb, _) = anchor::car(&city, &x)?;
                let name = match &x {
                    Anchor::Landmark(k, _) => city.def.landmark.iter().find(|l| &l.key == k).map(|l| l.name.clone()),
                    _ => None,
                }
                .unwrap_or_else(|| "the shop".into());
                Some((dist(door), name, door, kerb))
            })
            .filter(|(d, ..)| forced || (e.reach[0]..e.reach[1]).contains(d))
            .collect();
        shops.sort_by(|a, b| a.0.total_cmp(&b.0));
        let (_, shop, door, kerb) = shops.into_iter().next()?;
        if !crate::world::ground_loaded(sim, door) {
            return None;
        }
        let drops: Vec<(String, Vec3)> = e
            .drops
            .iter()
            .filter_map(|a| {
                let x = Anchor::parse(a).ok()?;
                let (p, _) = anchor::car(&city, &x)?;
                let d = Vec2::new(p.x - door.x, p.z - door.z).length();
                // (On the road beside it: the kerb outside a lot.)
                let keep = |l: &pav_world::lanes::Lane| {
                    !l.closed && l.kind != pav_world::lanes::LaneKind::Channel && l.kind != pav_world::lanes::LaneKind::Connector
                };
                let road = city
                    .lanes
                    .nearest(p, keep)
                    .filter(|(_, _, off)| *off < 40.0)
                    .map(|(l, along, _)| city.lanes.lanes[l as usize].at(along).0);
                (e.drop[0]..e.drop[1]).contains(&d).then(|| (place_name(&city, p), road.unwrap_or(p)))
            })
            .collect();
        if drops.is_empty() {
            return None;
        }
        let (drop_name, drop) = drops[sim.state.rng.below(drops.len() as u32) as usize].clone();
        let facing = (kerb.x - door.x).atan2(kerb.z - door.z);
        let robber = self.street_person(sim, "robber", door + Vec3::Y * 0.05, facing, &e.look);
        set_idle(sim, robber, "MESH2MOTION/Help_Two_Arms");
        Some(Getaway {
            robber,
            step: RideStep::Wait,
            t: 0.0,
            shop,
            door,
            kerb,
            drop_name,
            drop,
            car: None,
            behind: 0.0,
            told: false,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn getaway_tick(
        &mut self,
        sim: &mut Sim,
        e: &EventDef,
        g: &mut Getaway,
        said: &mut Option<Said>,
        over: bool,
        dt: f32,
        events: &mut Vec<SimEvent>,
        out: &mut BTreeMap<EntityId, InputFrame>,
    ) -> Flow {
        let Some(city) = self.city.clone() else { return Flow::End(false, "The robber found another ride.".into(), 0.0) };
        let me = self.player_point(sim);
        g.t += dt;
        detail(sim, g.robber, me);
        if gone_or_dead(sim, g.robber) {
            return if over { Flow::On } else { Flow::End(false, "The robber is gone.".into(), 0.0) };
        }
        if !self.fate.alive() && !over {
            return Flow::End(false, "The robber got away without you.".into(), 0.0);
        }
        let mine = self.drive.vehicle().filter(|c| !self.police_car(sim, *c));
        let riding = sim.state.entities.get(g.robber).and_then(|x| x.character.as_ref()).and_then(|c| c.riding);
        let rf = sim.feet(g.robber).unwrap_or(g.door);
        match g.step {
            RideStep::Wait => {
                out.insert(g.robber, InputFrame::default());
                face(sim, g.robber, me);
                let stopped = mine.and_then(|c| Car::read(sim, c)).filter(|c| {
                    c.vel.length() < BOARD_SLOW && Vec2::new(c.center.x - rf.x, c.center.z - rf.z).length() < BOARD_NEAR
                });
                if let Some(c) = stopped {
                    g.step = RideStep::Board;
                    g.t = 0.0;
                    g.car = Some(c.id);
                    set_idle(sim, g.robber, "MESH2MOTION/Idle_A");
                } else if g.t > e.wait || Vec2::new(rf.x - me.x, rf.z - me.z).length() > 240.0 {
                    *said = e.line("gone").map(|t| Said::new(&e.speaker, t, 0.85));
                    return Flow::End(false, "The robber found another ride.".into(), 3.0);
                }
            }
            RideStep::Board => {
                let Some(car) = g.car else {
                    g.step = RideStep::Wait;
                    return Flow::On;
                };
                if riding == Some(car) {
                    g.step = RideStep::Ride;
                    g.t = 0.0;
                    *said = e.line("board").map(|t| Said::new(&e.speaker, t, 0.85));
                    self.heat.raise(e.stars, rf);
                    self.heat.news = Some("Getaway car spotted".into());
                    return Flow::On;
                }
                let Some(c) = Car::read(sim, car) else {
                    g.step = RideStep::Wait;
                    g.car = None;
                    return Flow::On;
                };
                let door = passenger_door(&c);
                let far = Vec2::new(c.center.x - rf.x, c.center.z - rf.z).length() > 25.0;
                if mine != Some(car) || far {
                    g.step = RideStep::Wait;
                    g.car = None;
                    g.t = 0.0;
                    set_idle(sim, g.robber, "MESH2MOTION/Help_Two_Arms");
                } else if (Vec2::new(door.x - rf.x, door.z - rf.z).length() < DOOR_REACH || g.t > 7.0) && c.vel.length() < 4.0 {
                    if !board(sim, g.robber, car) {
                        // No seat for them: they give up.
                        return Flow::End(false, "There was no room for the robber.".into(), 0.0);
                    }
                } else {
                    let f = walk_to(sim, &city, g.robber, door, e.run[0]);
                    out.insert(g.robber, f);
                }
            }
            RideStep::Ride => {
                // In the player's car: on to the drop-off once the police are lost.
                let aboard = riding.is_some() && riding == mine;
                if let Some(car) = riding
                    && Some(car) != mine
                {
                    // The player got out (or into a police car): out after them.
                    sim.exit_vehicle(g.robber, car);
                    g.car = None;
                    *said = e.line("wait").map(|t| Said::new(&e.speaker, t, 0.85));
                }
                if aboard {
                    g.behind = 0.0;
                    let c = mine.and_then(|c| Car::read(sim, c));
                    let at_drop = c.as_ref().is_some_and(|c| {
                        Vec2::new(c.center.x - g.drop.x, c.center.z - g.drop.z).length() < DROP_NEAR && c.vel.length() < 2.0
                    });
                    if at_drop && self.heat.stars == 0 {
                        if let Some(car) = mine {
                            sim.exit_vehicle(g.robber, car);
                        }
                        g.step = RideStep::Off;
                        g.t = 0.0;
                        g.car = None;
                        self.pay(e.reward);
                        events.push(SimEvent::Sound { cue: "ui.cash".into(), pos: None, gain: 1.0, pitch: 1.0 });
                        *said = e.line("thanks").map(|t| Said::new(&e.speaker, t, 0.85));
                        let (prep, name) = split_place(&g.drop_name);
                        return Flow::End(true, format!("You dropped the robber off {prep} {name}. +${}", e.reward), 5.0);
                    }
                    if at_drop && !g.told {
                        g.told = true;
                        *said = e.line("heat").map(|t| Said::new(&e.speaker, t, 0.85));
                    } else if !at_drop {
                        g.told = false;
                    }
                } else {
                    // On foot: after the player, into the car they are in.
                    let far = Vec2::new(rf.x - me.x, rf.z - me.z).length() > e.escape;
                    g.behind = if far { g.behind + dt } else { 0.0 };
                    if g.behind > BEHIND_FOR {
                        return Flow::End(false, "You left the robber behind.".into(), 0.0);
                    }
                    let car = mine.and_then(|c| Car::read(sim, c));
                    match car {
                        Some(c) => {
                            let door = passenger_door(&c);
                            if Vec2::new(door.x - rf.x, door.z - rf.z).length() < DOOR_REACH * 1.5 && c.vel.length() < 4.0 {
                                if board(sim, g.robber, c.id) {
                                    g.car = Some(c.id);
                                }
                            } else {
                                let f = walk_to(sim, &city, g.robber, door, e.run[0]);
                                out.insert(g.robber, f);
                            }
                        }
                        None => {
                            let f = if Vec2::new(rf.x - me.x, rf.z - me.z).length() > 3.0 {
                                walk_to(sim, &city, g.robber, me, e.run[1])
                            } else {
                                InputFrame::default()
                            };
                            out.insert(g.robber, f);
                        }
                    }
                }
            }
            RideStep::Off => {
                // Off along the pavement, away from the road.
                let away = rf + (rf - me).normalize_or(Vec3::X) * 10.0;
                let f = walk_to(sim, &city, g.robber, away, 2.0);
                out.insert(g.robber, f);
            }
        }
        Flow::On
    }

    // ---- The bot ----

    /// Plans the event bot's next frame.
    fn events_bot(&mut self, sim: &Sim, def: &EventDef, dt: f32) {
        let mut b = std::mem::take(&mut self.activities.events.bot);
        b.frame = InputFrame::default();
        b.tick = b.tick.wrapping_add(1);
        b.t += dt;
        if let Some(live) = self.activities.events.on.clone()
            && live.over.is_none()
        {
            match &live.what {
                Happening::Snatch(s) => self.snatch_bot(&mut b, sim, s, dt),
                Happening::Getaway(g) => self.getaway_bot(&mut b, sim, def, g, dt),
            }
        }
        self.activities.events.bot = b;
    }

    fn snatch_bot(&mut self, b: &mut MissionBot, sim: &Sim, s: &Snatch, dt: f32) {
        let me = Self::feet(sim);
        if self.drive.vehicle().is_some() {
            if !self.drive.busy() {
                b.tap(pad::NORTH);
            }
            return;
        }
        if self.phone.open {
            b.tap(pad::EAST);
            return;
        }
        match s.step {
            SnatchStep::Approach => {}
            SnatchStep::Run => {
                // Fists (a gun in a crowd is a crime), then after them and a punch.
                if self.weapon != 0 && b.tick.is_multiple_of(20) {
                    b.cmd("weapon fists".into());
                }
                let Some(tf) = sim.feet(s.thief) else { return };
                let d = Vec2::new(tf.x - me.x, tf.z - me.z).length();
                if d < 1.7 && self.weapon == 0 {
                    // Facing them: the swing lands in front.
                    b.frame.move_dir = Vec2::new(tf.x - me.x, tf.z - me.z).normalize_or(Vec2::ZERO) * 0.3;
                    b.tap(pad::EAST);
                } else {
                    let mut way = self.activities.events.headway;
                    way.check(b, Vec2::new(me.x, me.z), dt);
                    self.activities.events.headway = way;
                    self.bot_walk(b, sim, tf, true, dt);
                    b.frame.held |= pav_core::input::buttons::SPRINT;
                }
            }
            SnatchStep::Purse => self.bot_walk(b, sim, s.purse, true, dt),
            SnatchStep::Carry => {
                if self.activities.events.keep {
                    // Off with it: well away from its owner.
                    let away = match self.activities.events.away {
                        Some(a) => a,
                        None => {
                            let of = sim.feet(s.owner).unwrap_or(s.spot);
                            let dir = Vec2::new(me.x - of.x, me.z - of.z).normalize_or(Vec2::X);
                            let want = Vec2::new(me.x, me.z) + dir * 85.0;
                            let to = self.city.as_ref().and_then(|c| c.walk.nearest_walkable(want, 10)).unwrap_or(want);
                            let a = Vec3::new(to.x, me.y, to.y);
                            self.activities.events.away = Some(a);
                            a
                        }
                    };
                    self.bot_walk(b, sim, away, true, dt);
                } else if let Some(of) = sim.feet(s.owner) {
                    if Vec2::new(of.x - me.x, of.z - me.z).length() < GIVE_REACH * 0.8 {
                        b.tap(pad::RIGHT);
                    } else {
                        self.bot_walk(b, sim, of, true, dt);
                    }
                }
            }
            SnatchStep::After => {}
        }
    }

    fn getaway_bot(&self, b: &mut MissionBot, sim: &Sim, _def: &EventDef, g: &Getaway, dt: f32) {
        let mine = self.drive.vehicle();
        let piloted = self.traffic.cars.iter().any(|a| a.pilot);
        let car = mine.and_then(|c| Car::read(sim, c));
        let stop = |b: &mut MissionBot| {
            if piloted || self.bot.escape {
                b.cmd("autopilot off".into());
            }
            if let Some(c) = &car
                && c.speed() > 0.8
            {
                b.frame.pad.l2 = 1.0;
                b.frame.pad.held |= pad::L2;
            }
        };
        if self.drive.busy() {
            return;
        }
        let riding = sim.state.entities.get(g.robber).and_then(|x| x.character.as_ref()).and_then(|c| c.riding);
        match g.step {
            RideStep::Wait | RideStep::Board => {
                let Some(c) = &car else {
                    if let Some((near, _)) = self.nearest_car(sim) {
                        self.bot_enter(b, sim, near, dt);
                    }
                    return;
                };
                let rf = sim.feet(g.robber).unwrap_or(g.door);
                let d = Vec2::new(c.center.x - rf.x, c.center.z - rf.z).length();
                if g.step == RideStep::Board || d < BOARD_NEAR * 0.75 {
                    stop(b);
                } else {
                    self.bot_drive(b, g.kerb, 0.8);
                }
            }
            RideStep::Ride => {
                let aboard = riding.is_some() && riding == mine;
                if !aboard {
                    // Wait for them (in a car), or get one.
                    if car.is_some() {
                        stop(b);
                    } else if let Some((near, _)) = self.nearest_car(sim) {
                        self.bot_enter(b, sim, near, dt);
                    }
                    return;
                }
                if self.heat.stars > 0 {
                    if !self.bot.escape {
                        b.cmd("autopilot escape".into());
                    }
                    b.pilot = None;
                    return;
                }
                if self.bot.escape {
                    b.cmd("autopilot off".into());
                    return;
                }
                let off = car.as_ref().map_or(f32::MAX, |c| Vec2::new(c.center.x - g.drop.x, c.center.z - g.drop.z).length());
                if off < DROP_NEAR * 0.7 {
                    stop(b);
                } else if let Some(c) = car.as_ref().filter(|_| off < 40.0 && (self.traffic.pilot_arrived() || !piloted)) {
                    // The autopilot stopped short (the road's nearest point to it): the rest by hand.
                    if piloted {
                        b.cmd("autopilot off".into());
                    }
                    let want = Vec2::new(g.drop.x - c.center.x, g.drop.z - c.center.z).normalize_or_zero();
                    let fwd = Vec2::new(c.forward().x, c.forward().z).normalize_or_zero();
                    let cross = fwd.x * want.y - fwd.y * want.x;
                    if fwd.dot(want) > -0.3 {
                        b.frame.pad.stick_l.x = (cross * 2.5).clamp(-1.0, 1.0);
                        b.frame.pad.r2 = if c.speed() < 5.0 { 0.4 } else { 0.0 };
                        b.frame.pad.held |= pad::R2;
                    } else {
                        b.frame.pad.stick_l.x = -cross.signum();
                        b.frame.pad.l2 = 0.5;
                        b.frame.pad.held |= pad::L2;
                    }
                } else {
                    self.bot_drive(b, g.drop, 0.9);
                }
            }
            RideStep::Off => {}
        }
    }

    // ---- What the HUD shows ----

    pub(crate) fn events_ui(&self, sim: &Sim, ui: &mut UiModel, draw: &mut DrawList) {
        if let Some(b) = &self.activities.events.banner {
            ui.banners.push(b.banner());
        }
        let Some(live) = &self.activities.events.on else { return };
        if let Some(s) = &live.said {
            ui.subtitle = Some(s.subtitle());
        }
        if live.over.is_some() {
            return;
        }
        let me = Self::feet(sim);
        let blip = |ui: &mut UiModel, p: Vec3, icon: BlipIcon, hex: &str, pulse: bool| {
            if let Some(h) = &mut ui.hud
                && let Some(m) = &mut h.minimap
            {
                m.blips.push(Blip {
                    pos: Vec2::new(p.x, p.z),
                    icon,
                    color: Color::hex(hex),
                    label: None,
                    clamp: true,
                    height: 0,
                    pulse,
                });
            }
        };
        let marker = |ui: &mut UiModel, p: Vec3, icon: BlipIcon, hex: &str, label: &str| {
            ui.markers.push(ScreenMarker { pos: p, icon, color: Color::hex(hex), label: Some(label.into()), distance: true });
        };
        let objective = |ui: &mut UiModel, text: &str| {
            if let Some(h) = &mut ui.hud {
                h.objective = Some(text.into());
            }
        };
        match &live.what {
            Happening::Snatch(s) => match s.step {
                SnatchStep::Approach => {}
                SnatchStep::Run => {
                    objective(ui, "Chase down the ~r~purse snatcher~s~.");
                    if let Some(f) = sim.feet(s.thief) {
                        blip(ui, f, BlipIcon::Enemy, "#e8423f", true);
                        marker(ui, f + Vec3::Y * 2.2, BlipIcon::Enemy, "#e8423f", "Thief");
                    }
                }
                SnatchStep::Purse => {
                    objective(ui, "Pick up the ~g~purse~s~.");
                    blip(ui, s.purse, BlipIcon::Collectible, "#6fe28a", true);
                    marker(ui, s.purse + Vec3::Y * 1.2, BlipIcon::Collectible, "#6fe28a", "Purse");
                    draw.prims.push(purse_prim(s.purse, self.ui_time));
                }
                SnatchStep::Carry => {
                    objective(ui, "Give the purse back to ~b~its owner~s~, or walk off with it.");
                    if let Some(f) = sim.feet(s.owner) {
                        blip(ui, f, BlipIcon::Ally, "#3d8bff", false);
                        marker(ui, f + Vec3::Y * 2.2, BlipIcon::Ally, "#3d8bff", "Owner");
                        if Vec2::new(f.x - me.x, f.z - me.z).length() < GIVE_REACH && self.drive.vehicle().is_none() {
                            ui.prompts.push(Prompt { button: PadButton::Right, label: "Give back the purse".into(), hold: None });
                        }
                    }
                }
                SnatchStep::After => {}
            },
            Happening::Getaway(g) => {
                let riding = sim.state.entities.get(g.robber).and_then(|x| x.character.as_ref()).and_then(|c| c.riding);
                let rf = sim.feet(g.robber).unwrap_or(g.door);
                match g.step {
                    RideStep::Wait | RideStep::Board => {
                        objective(ui, &format!("A ~b~robber~s~ outside {} needs a getaway driver: stop by them.", g.shop));
                        blip(ui, rf, BlipIcon::Activity, "#3d8bff", true);
                        marker(ui, rf + Vec3::Y * 2.2, BlipIcon::Activity, "#3d8bff", "Robber");
                        if g.kerb.distance(me) < 150.0 {
                            draw.prims.push(ring(g.kerb, 3.0, Color::hex("#3d8bff")));
                        }
                    }
                    RideStep::Ride if riding.is_some() && riding == self.drive.vehicle() => {
                        if self.heat.stars > 0 {
                            objective(ui, "Lose the cops.");
                        } else {
                            let (prep, name) = split_place(&g.drop_name);
                            objective(ui, &format!("Drop the robber off {prep} ~y~{name}~s~."));
                            blip(ui, g.drop, BlipIcon::Objective, "#ffd23f", true);
                            marker(ui, g.drop + Vec3::Y * 2.0, BlipIcon::Objective, "#ffd23f", "Drop-off");
                            if g.drop.distance(me) < 160.0 {
                                draw.prims.push(ring(g.drop, 4.5, Color::hex("#ffd23f")));
                            }
                        }
                    }
                    RideStep::Ride => {
                        objective(
                            ui,
                            if self.drive.vehicle().is_some() {
                                "Wait for the ~b~robber~s~ to get in."
                            } else {
                                "Get a car: the ~b~robber~s~ follows you."
                            },
                        );
                        blip(ui, rf, BlipIcon::Ally, "#3d8bff", true);
                        marker(ui, rf + Vec3::Y * 2.2, BlipIcon::Ally, "#3d8bff", "Robber");
                    }
                    RideStep::Off => {}
                }
            }
        }
    }

    /// Where the GPS points for the event under way.
    fn events_waypoint(&self, sim: &Sim) -> Option<Vec2> {
        let live = self.activities.events.on.as_ref().filter(|l| l.over.is_none())?;
        match &live.what {
            Happening::Getaway(g) => match g.step {
                RideStep::Wait | RideStep::Board => Some(Vec2::new(g.kerb.x, g.kerb.z)),
                RideStep::Ride if self.heat.stars == 0 && self.drive.vehicle().is_some() => Some(Vec2::new(g.drop.x, g.drop.z)),
                _ => None,
            },
            Happening::Snatch(_) => {
                let _ = sim;
                None
            }
        }
    }

    /// The event under way for `status`.
    fn events_status(&self) -> serde_json::Value {
        let st = &self.activities.events;
        let step = st.on.as_ref().map(|l| match &l.what {
            Happening::Snatch(s) => format!("{:?}", s.step).to_lowercase(),
            Happening::Getaway(g) => format!("{:?}", g.step).to_lowercase(),
        });
        serde_json::json!({
            "on": st.on.as_ref().map(|l| l.id.clone()),
            "step": step,
            "over": st.on.as_ref().is_some_and(|l| l.over.is_some()),
            "quiet": (st.quiet * 10.0).round() / 10.0,
            "started": st.started,
            "finished": st.finished,
            "failed": st.failed,
            "last": st.last,
            "off": st.off,
        })
    }
}

/// Where someone gets in on the passenger side (the car's right, as the driver sits on its left).
fn passenger_door(c: &Car) -> Vec3 {
    c.feet() - c.left() * (c.half.x + 0.6)
}

/// Puts `who` in the first free passenger seat of `car`; false when there is none.
fn board(sim: &mut Sim, who: EntityId, car: EntityId) -> bool {
    let seats = sim.state.entities.get(car).and_then(|e| e.vehicle.as_ref()).map_or(0, |v| v.passengers.len());
    (1..=seats).any(|k| sim.enter_vehicle_seat(who, car, k))
}

/// Where a place is, as the radio says it ("on Sunset Row", "near the pier", "in Los Mirados").
pub(crate) fn place_name(city: &City, p: Vec3) -> String {
    crate::radio::def().map(|d| d.places.name(city, p)).unwrap_or_else(|_| "at the drop-off".into())
}

/// A place's phrase split into its preposition and the name ("in Los Mirados": "in", "Los
/// Mirados"); "at" when it has none.
pub(crate) fn split_place(phrase: &str) -> (&str, &str) {
    for prep in ["at", "in", "on", "near", "by", "outside", "under", "off"] {
        if let Some(rest) = phrase.strip_prefix(prep).and_then(|r| r.strip_prefix(' ')) {
            return (prep, rest);
        }
    }
    ("at", phrase)
}

/// A glowing ring on the ground (where to stop).
pub(crate) fn ring(p: Vec3, r: f32, color: Color) -> PrimDraw {
    PrimDraw {
        shape: pav_core::shape::Shape::Cylinder { half_height: 0.6, radius: r },
        pos: p + Vec3::Y * 0.6,
        rot: Quat::IDENTITY,
        color,
        emissive: 0.8,
        look: pav_core::shape::Look::Unlit,
        shadow: false,
        glow: Default::default(),
    }
}

/// The purse lying on the ground: a small bag that bobs and turns, glowing (as a mission's
/// pickups do).
fn purse_prim(at: Vec3, t: f32) -> PrimDraw {
    PrimDraw {
        shape: pav_core::shape::Shape::Box { half: Vec3::new(0.16, 0.11, 0.06) },
        pos: at + Vec3::Y * (0.45 + (t * 2.0).sin() * 0.06),
        rot: Quat::from_rotation_y(t),
        color: Color::hex("#b8344a"),
        emissive: 1.4,
        look: pav_core::shape::Look::Unlit,
        shadow: false,
        glow: Default::default(),
    }
}

// ---- The street activities (vigilante and the random events) for the game's hook ----

impl Freeway {
    /// The bot's frame while an activity runs (vigilante, an event), else nothing.
    pub(crate) fn street_frame(&self) -> InputFrame {
        if self.activities.vigilante.run.is_some() {
            self.activities.vigilante.bot.frame.clone()
        } else if self.activities.events.on.is_some() {
            self.activities.events.bot.frame.clone()
        } else {
            InputFrame::default()
        }
    }

    /// Whether vigilante or an event is on (no mission is picked up meanwhile).
    pub(crate) fn street_busy(&self) -> bool {
        self.activities.vigilante.run.is_some() || self.activities.events.on.is_some()
    }

    /// An activity played with `activity <id>`: won when done, lost when it ended without.
    pub(crate) fn street_outcome(&self) -> Option<Outcome> {
        let t = match (self.activities.vigilante.trial, self.activities.events.trial) {
            (Trial::No, t) => t,
            (t, _) => t,
        };
        match t {
            Trial::No => None,
            Trial::Playing => Some(Outcome::Playing),
            Trial::Won => Some(Outcome::Won),
            Trial::Lost => Some(Outcome::Lost),
        }
    }

    /// The HUD, the markers and the banners of the activities.
    pub(crate) fn street_ui(&self, sim: &Sim, ui: &mut UiModel, draw: &mut DrawList) {
        if self.missions.run.is_some() {
            return;
        }
        self.vigilante_ui(sim, ui, draw);
        self.events_ui(sim, ui, draw);
    }

    /// Where the GPS points for an activity.
    pub(crate) fn street_waypoint(&self, sim: &Sim) -> Option<Vec2> {
        if self.missions.run.is_some() {
            return None;
        }
        self.vigilante_waypoint(sim).or_else(|| self.events_waypoint(sim))
    }

    /// The activities for `status`.
    pub(crate) fn street_status(&self, sim: &Sim) -> serde_json::Value {
        serde_json::json!({ "vigilante": self.vigilante_status(sim), "event": self.events_status(), "done": self.activities.done })
    }

    /// Whether hurting `who` is no crime: a vigilante's suspect, a purse snatcher.
    pub(crate) fn fair_game(&self, who: EntityId) -> bool {
        let thief = self.activities.events.on.as_ref().is_some_and(|l| matches!(&l.what, Happening::Snatch(s) if s.thief == who));
        thief || self.suspect(who)
    }

    /// Ends the street activities (a mission begins: its world is its own).
    pub(crate) fn street_end(&mut self, sim: &mut Sim) {
        if let Some(live) = self.activities.events.on.take() {
            self.event_release(sim, live);
        }
        self.vigilante_abort(sim);
    }
}
