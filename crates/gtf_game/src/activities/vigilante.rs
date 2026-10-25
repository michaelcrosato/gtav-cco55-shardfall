//! Vigilante (PLAN.md §5.6.11): in a police car, D-pad right (keyboard E) or the phone's Jobs app
//! starts it (`content/freeway/activities.toml` [vigilante]). Each level sends suspects in
//! getaway cars somewhere near the player; they drive about until the police car comes close (or
//! its siren, or a hit), then run (far enough off, they think they have lost it and lie low).
//! Ram or shoot a getaway car until they give it up (or box it in) and they bail out and run
//! (unarmed) or fight (the armed levels); a suspect down — or burnt out in the car — is a
//! takedown ($250). The last level done pays a bonus and counts for completion (`vigilante`). It
//! ends on death, arrest, a level's time running out, a suspect on foot getting away, or a while
//! out of a police car (chasing a suspect on foot does not count). Harming a suspect is no crime,
//! an officer on duty fires without a report, and harm to the public is not the police's to
//! chase: each civilian killed docks the run's pay (`fine`) instead. The bot plays a run under
//! way (`Game::bot`); `activity vigilante` puts the player in a police car and starts one (`play
//! bot=true seeds=3 cmd="activity vigilante"`).

use std::collections::BTreeMap;

use glam::{Vec2, Vec3};
use pav_core::Color;
use pav_core::InputFrame;
use pav_core::entity::EntityId;
use pav_core::frame::SimEvent;
use pav_core::input::pad;
use pav_core::present::*;
use pav_core::sim::Sim;
use serde::{Deserialize, Serialize};

use super::ActivitiesDef;
use super::events::{Flash, LookDef, Runner, Trial, detail, place_name};
use crate::Freeway;
use crate::drive::Car;
use crate::missions::bot::MissionBot;

/// Within this of the player (m) a suspect on foot keeps the run going while the player is out
/// of a police car (a chase on foot).
const FOOT_CHASE: f32 = 90.0;
/// An armed suspect gives up below this share of their health.
const GIVE_UP: f32 = 0.5;
/// A getaway car stopped this long (s) with the police car this near (m) is boxed in.
const BOXED_FOR: f32 = 3.0;
const BOXED_NEAR: f32 = 25.0;
/// Seconds the level's call shows before its suspects turn up.
const CALL: f32 = 1.5;
/// Seconds of finding nowhere to put them before looking wider (and at any level).
const WIDEN_AFTER: f32 = 6.0;
/// Seconds a getaway car runs with the police car well behind before it slows down again.
const CALM: f32 = 8.0;
/// Seconds suspects stand dazed after scrambling out of their car.
const DAZE: f32 = 1.5;
/// Seconds on the autopilot with no headway (no nearer the target, nowhere much else) before
/// the bot turns about and goes another way.
const NO_HEADWAY: f32 = 12.0;
/// Spare police cars dispatch sends on a run (for wrecked ones).
const SPARES: u32 = 3;

/// `[vigilante]` in `activities.toml`.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VigilanteDef {
    pub title: String,
    /// The vehicles it starts from (vehicles.toml keys).
    pub cars: Vec<String>,
    /// Cash a takedown, for the last level done, and docked for a civilian killed on duty.
    pub pay: i64,
    pub bonus: i64,
    pub fine: i64,
    /// Seconds out of a police car before it ends.
    pub leave: f32,
    /// Suspects turn up this far from the player (m); they run once the police car is `spot` m
    /// off; a getaway car `escape` m off thinks it has lost the police and lies low. On foot they run from the police within
    /// `hide` m and lie low farther off.
    pub spawn: [f32; 2],
    pub spot: f32,
    pub escape: f32,
    pub hide: f32,
    /// A getaway car at this share of its health or less is given up; a ram does `ram` times the
    /// crash's own damage to it. The police car on a run takes `tough` of the damage it would
    /// (of a ram's, its push bar's: half that).
    pub bail: f32,
    pub ram: f32,
    pub tough: f32,
    /// Seconds between levels.
    pub gap: f32,
    /// Suspects' health on foot (the armed ones' `armed_hp`), running speed and once tired
    /// (m/s), seconds before tiring, and the side the armed ones fight as (enemies.toml).
    pub hp: f32,
    pub armed_hp: f32,
    pub run: [f32; 2],
    pub tire: f32,
    pub side: String,
    pub look: LookDef,
    pub level: Vec<LevelDef>,
}

/// A level.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LevelDef {
    /// Getaway cars, and suspects in each.
    pub cars: u32,
    pub riders: u32,
    /// The cars' kinds (one picked for each), their share of a new one's health, and how hard
    /// they drive once running (× the limits).
    pub kinds: Vec<String>,
    pub health: f32,
    pub pace: f32,
    /// A weapon id: they fight once out of the car.
    #[serde(default)]
    pub armed: Option<String>,
    /// Seconds to take them all down.
    pub time: f32,
    /// What the dispatcher says.
    pub call: String,
}

impl VigilanteDef {
    /// What is wrong with it (empty: nothing).
    pub fn check(&self) -> Vec<String> {
        let mut errs = Vec::new();
        let have = |k: &str| crate::vehicles::get(k).is_ok();
        for k in &self.cars {
            if !have(k) {
                errs.push(format!("vigilante: no vehicle '{k}'"));
            }
        }
        if self.level.is_empty() {
            errs.push("vigilante: no levels".into());
        }
        if let Err(e) = self.look.wear.check() {
            errs.push(format!("vigilante: look wear: {e}"));
        }
        let weapons = crate::combat::def().ok();
        for (i, l) in self.level.iter().enumerate() {
            for k in &l.kinds {
                if !have(k) {
                    errs.push(format!("vigilante level {}: no vehicle '{k}'", i + 1));
                }
            }
            if l.kinds.is_empty() || l.cars == 0 || l.riders == 0 {
                errs.push(format!("vigilante level {}: needs cars, riders and kinds", i + 1));
            }
            if let Some(w) = &l.armed
                && !weapons.as_ref().is_some_and(|d| d.weapon.iter().any(|x| &x.id == w))
            {
                errs.push(format!("vigilante level {}: no weapon '{w}'", i + 1));
            }
            if !(0.05..=1.0).contains(&l.health) || l.health <= self.bail {
                errs.push(format!("vigilante level {}: health must be over bail and at most 1", i + 1));
            }
        }
        if crate::enemies::def().ok().is_none_or(|d| !d.side.contains_key(&self.side)) {
            errs.push(format!("vigilante: no side '{}' in enemies.toml", self.side));
        }
        errs
    }
}

/// A getaway car of the level.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Wheels {
    pub id: EntityId,
    /// Running from the police (it saw them), given up (its suspects bailed), its health last
    /// tick (a ram's damage is multiplied), seconds stopped with the police car near.
    pub fled: bool,
    pub given_up: bool,
    pub health: f32,
    pub still: f32,
    /// Seconds running with the police car far behind (they calm down and blend in again).
    #[serde(default)]
    pub far: f32,
}

/// A suspect.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Suspect {
    pub id: EntityId,
    /// The car they ride in (None: on foot).
    pub car: Option<EntityId>,
    pub armed: bool,
    pub down: bool,
    pub runner: Runner,
    /// Their health at the start (unarmed, any hit makes them give up), and given up: on their
    /// knees, hands behind the head, until taken away.
    #[serde(default)]
    pub hp: f32,
    #[serde(default)]
    pub cuffed: bool,
    /// Seconds dazed after scrambling out of a crashed car.
    #[serde(default)]
    pub daze: f32,
}

/// Where a level is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stage {
    /// The dispatcher's call; the suspects turn up after it (once the player sits in a police car).
    Call,
    Chase,
    /// All down: the next level after a pause (or the last one done).
    Clear,
}

/// A run under way.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Run {
    /// The level (0 first) and where it is.
    pub level: usize,
    pub stage: Stage,
    pub stage_t: f32,
    /// Seconds of this level's chase.
    pub t: f32,
    pub cars: Vec<Wheels>,
    pub suspects: Vec<Suspect>,
    /// The police car last driven, and seconds out of one.
    pub police: Option<EntityId>,
    pub out: f32,
    /// Takedowns and cash this run.
    pub downs: u32,
    pub paid: i64,
    /// Getaway cars left behind: with nobody at the wheel they roll free (no brakes) while the
    /// player's car is up against one, so it can be shoved out of the way.
    #[serde(default)]
    pub left: Vec<EntityId>,
    /// The police car's health as last noted (its push bar's share of a ram is given back).
    #[serde(default)]
    pub police_hp: Option<(EntityId, f32)>,
    /// Where the police car last driven was last seen.
    #[serde(default)]
    pub police_at: Option<Vec3>,
    /// Police cars dispatch has sent to replace one wrecked on the run.
    #[serde(default)]
    pub spares: u32,
}

/// Vigilante (part of the game's state).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VigilanteState {
    pub run: Option<Run>,
    /// The highest level reached, takedowns ever, runs begun and finished.
    pub best: usize,
    pub takedowns: u32,
    pub runs: u32,
    pub finished: u32,
    /// How the last run ended (`status`, tests).
    pub last: String,
    /// Started with `activity vigilante`: how it went.
    pub trial: Trial,
    pub banner: Option<Flash>,
    /// The bot: its memory, and its ramming (seconds left in a ram, of the pause after one,
    /// stuck against something, backing off).
    pub bot: MissionBot,
    pub ram: f32,
    pub cool: f32,
    pub stuck: f32,
    pub back: f32,
    /// Seconds stalled on the autopilot, and left turning round by hand (toward `about` when
    /// turning about: the way it came).
    pub stall: f32,
    pub turn: f32,
    pub about: Option<Vec2>,
    /// Its headway on the autopilot: where it was and how far off the target then, and seconds
    /// since it got much nearer or anywhere much else.
    pub prog_at: Vec3,
    pub prog_d: f32,
    pub prog: f32,
    pub prog_tick: u32,
    /// Times it has had no headway since it last made some, and whether it is pushing on by
    /// hand (else turning about) now.
    pub tries: u32,
    pub push: bool,
    /// Which way it last turned the wheels backing off a stuck spot off the road.
    pub side: f32,
    /// Seconds by the police car trying to get in, and left on foot after a runner the road
    /// would get no nearer.
    pub board: f32,
    pub afoot: f32,
    /// The armed suspect it fights, their health when it last landed a hit, and seconds since.
    pub fight: Option<EntityId>,
    pub fight_hp: f32,
    pub fight_t: f32,
    /// The getaway car the bot is after, and its headway on foot.
    pub target: Option<EntityId>,
    pub headway: super::events::Headway,
    /// What the bot is up to (`status`; not kept).
    #[serde(skip)]
    pub trace: String,
}

/// How a run's tick ended it.
enum Ending {
    Done,
    Over(String),
}

impl Freeway {
    /// Whether `car` is one of the police cars vigilante starts from (not a wreck).
    fn vigilante_car(&self, sim: &Sim, vd: &VigilanteDef, car: EntityId) -> bool {
        sim.state.entities.get(car).is_some_and(|e| {
            vd.cars.contains(&e.name) && e.vehicle.as_ref().is_some_and(|v| !v.state.wrecked && !v.state.flooded)
        })
    }

    /// The police car the player sits in (one vigilante starts from).
    fn in_police_car(&self, sim: &Sim, vd: &VigilanteDef) -> Option<EntityId> {
        self.drive.vehicle().filter(|c| self.vigilante_car(sim, vd, *c))
    }

    /// Whether a run may start now: in a police car, alive, no mission or police after the player.
    fn vigilante_ready(&self, sim: &Sim, vd: &VigilanteDef) -> bool {
        self.activities.vigilante.run.is_none()
            && self.in_police_car(sim, vd).is_some()
            && self.fate.alive()
            && self.missions.run.is_none()
            && self.heat.stars == 0
            && self.activities.events.on.is_none()
            && self.crew.switch.is_none()
    }

    /// The vigilante prompt is up in a police car: D-pad right is its button (not the radio's).
    pub(crate) fn vigilante_takes_right(&self, sim: &Sim) -> bool {
        super::def().ok().is_some_and(|d| d.vigilante.as_ref().is_some_and(|vd| self.vigilante_ready(sim, vd)))
    }

    /// Whether `id` is one of the run's suspects (fair game).
    pub(crate) fn suspect(&self, id: EntityId) -> bool {
        self.activities.vigilante.run.as_ref().is_some_and(|r| r.suspects.iter().any(|s| s.id == id))
    }

    /// On duty: an officer firing on a run is not reported.
    pub(crate) fn on_duty(&self) -> bool {
        self.activities.vigilante.run.is_some()
    }

    /// The phone's Jobs app: vigilante's row (its title, and what picking it does).
    pub(crate) fn vigilante_job(&self, sim: &Sim) -> Option<(String, String)> {
        let d = super::def().ok()?;
        let vd = d.vigilante.as_ref()?;
        let detail = match &self.activities.vigilante.run {
            Some(r) => format!("Level {} · end it", r.level + 1),
            None if self.in_police_car(sim, vd).is_some() => "Start".into(),
            None => "Needs a police car".into(),
        };
        Some((vd.title.clone(), detail))
    }

    /// Picked in the Jobs app: start it (in a police car), end it (on a run), or a waypoint to
    /// SPPD Central's cars.
    pub(crate) fn vigilante_pick(&mut self, sim: &mut Sim, events: &mut Vec<SimEvent>) {
        let Some(vd) = super::def().ok().and_then(|d| d.vigilante.clone()) else { return };
        if self.activities.vigilante.run.is_some() {
            self.vigilante_over(sim, &vd, "You called it a day.".into(), events);
        } else if self.vigilante_ready(sim, &vd) {
            self.vigilante_start(&vd, events);
        } else if let Some(door) = self.city.as_ref().and_then(|c| crate::world::door(c, "police")) {
            self.front.waypoint = Some(Vec2::new(door.x, door.z));
            self.note("police", &vd.title, "Take a police car: SPPD Central keeps a few out front.");
        }
    }

    /// A run begins (level 1).
    fn vigilante_start(&mut self, vd: &VigilanteDef, events: &mut Vec<SimEvent>) {
        let st = &mut self.activities.vigilante;
        st.run = Some(Run {
            level: 0,
            stage: Stage::Call,
            stage_t: 0.0,
            t: 0.0,
            cars: Vec::new(),
            suspects: Vec::new(),
            police: None,
            out: 0.0,
            downs: 0,
            paid: 0,
            left: Vec::new(),
            police_hp: None,
            police_at: None,
            spares: 0,
        });
        st.runs += 1;
        st.bot = MissionBot::default();
        st.ram = 0.0;
        st.cool = 0.0;
        self.vigilante_call(vd, 0, events);
    }

    /// A level's call: its banner, the dispatcher's words, and a fresh vest from the trunk (body
    /// armour full).
    fn vigilante_call(&mut self, vd: &VigilanteDef, level: usize, events: &mut Vec<SimEvent>) {
        self.combat.armor = crate::combat::MAX_ARMOR;
        let n = vd.level.len();
        self.activities.vigilante.banner =
            Some(Flash::new(BannerKind::Info, &vd.title.to_uppercase(), Some(format!("Level {} of {n}", level + 1))));
        self.activities.vigilante.best = self.activities.vigilante.best.max(level + 1);
        events.push(SimEvent::Sound { cue: "police.scanner".into(), pos: None, gain: 0.9, pitch: 1.0 });
    }

    /// Commands: `activity vigilante` (into a police car and a run, as a trial), `vigilante
    /// start|stop`, `vigilante level <n>` (on to level n now).
    fn vigilante_cmds(&mut self, sim: &mut Sim, vd: &VigilanteDef, input: &InputFrame, events: &mut Vec<SimEvent>) {
        for c in &input.cmds {
            let mut w = c.split_whitespace();
            match (w.next(), w.next(), w.next()) {
                (Some("activity"), Some("vigilante"), None) => {
                    if self.in_police_car(sim, vd).is_none() {
                        // Out of any other car, into a police car.
                        if let (Some(pid), Some(car)) = (sim.state.player, self.drive.vehicle()) {
                            sim.exit_vehicle(pid, car);
                            self.drive.phase = crate::drive::Phase::Foot;
                        }
                        let key = vd.cars.first().cloned().unwrap_or_default();
                        self.car_cmd(sim, &key, None, true);
                    }
                    if self.vigilante_ready(sim, vd) {
                        self.vigilante_start(vd, events);
                        self.activities.vigilante.trial = Trial::Playing;
                    } else {
                        log::warn!("activity vigilante: not ready (a mission, the police, a menu?)");
                        self.activities.vigilante.trial = Trial::Lost;
                    }
                }
                (Some("vigilante"), Some("start"), None) => {
                    if self.vigilante_ready(sim, vd) {
                        self.vigilante_start(vd, events);
                    } else {
                        log::warn!("vigilante start: get in a police car ({}) first", vd.cars.join(", "));
                    }
                }
                (Some("vigilante"), Some("stop"), None) if self.activities.vigilante.run.is_some() => {
                    self.vigilante_over(sim, vd, "Stopped.".into(), events);
                }
                (Some("vigilante"), Some("level"), Some(n)) => {
                    let Ok(n) = n.parse::<usize>() else { continue };
                    if let Some(mut run) = self.activities.vigilante.run.take() {
                        self.vigilante_release(sim, &mut run);
                        run.level = n.clamp(1, vd.level.len().max(1)) - 1;
                        run.stage = Stage::Call;
                        run.stage_t = 0.0;
                        run.t = 0.0;
                        self.activities.vigilante.run = Some(run);
                        self.vigilante_call(vd, n.clamp(1, vd.level.len().max(1)) - 1, events);
                    }
                }
                _ => {}
            }
        }
    }

    /// Vigilante's tick (outside missions): commands, the button, the run, the bot.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn vigilante_tick(
        &mut self,
        sim: &mut Sim,
        d: &ActivitiesDef,
        input: &InputFrame,
        dt: f32,
        events: &mut Vec<SimEvent>,
        out: &mut BTreeMap<EntityId, InputFrame>,
    ) {
        let Some(vd) = d.vigilante.as_ref() else { return };
        if let Some(b) = &mut self.activities.vigilante.banner {
            b.age += dt;
            if b.age > super::events::FLASH {
                self.activities.vigilante.banner = None;
            }
        }
        self.vigilante_cmds(sim, vd, input, events);
        // The button: D-pad right in a police car (not while the phone, a menu or a wheel has it).
        let free = !self.phone.open && self.shop.is_none() && self.radio_wheel.is_none() && self.crew.wheel.is_none();
        if free && input.pad.pressed & pad::RIGHT != 0 && self.vigilante_ready(sim, vd) {
            self.vigilante_start(vd, events);
        }
        let Some(mut run) = self.activities.vigilante.run.take() else {
            self.activities.vigilante.bot.frame = InputFrame::default();
            return;
        };
        match self.vigilante_run(sim, vd, &mut run, dt, events, out) {
            None => {
                self.activities.vigilante.run = Some(run);
                self.vigilante_bot(sim, vd, dt);
            }
            Some(Ending::Done) => {
                self.activities.vigilante.run = Some(run);
                self.vigilante_done(sim, vd, events);
            }
            Some(Ending::Over(why)) => {
                self.activities.vigilante.run = Some(run);
                self.vigilante_over(sim, vd, why, events);
            }
        }
    }

    /// One tick of a run: the police car, the level's stage, the getaway cars and the suspects.
    fn vigilante_run(
        &mut self,
        sim: &mut Sim,
        vd: &VigilanteDef,
        run: &mut Run,
        dt: f32,
        events: &mut Vec<SimEvent>,
        out: &mut BTreeMap<EntityId, InputFrame>,
    ) -> Option<Ending> {
        if !self.fate.alive() {
            return Some(Ending::Over(
                if matches!(self.fate, crate::fate::Fate::Busted { .. }) { "You were busted." } else { "You died." }.into(),
            ));
        }
        let lv = vd.level.get(run.level)?.clone();
        let me = self.player_point(sim);
        // Getaway cars left behind roll free beside the player's car.
        run.left.retain(|id| sim.state.entities.get(*id).is_some());
        for &id in &run.left {
            if sim.state.entities.get(id).is_some_and(|e| e.pos.distance(me) < 12.0) {
                self.drive.coasting.insert(id, 0.5);
            }
        }
        // Out of a police car (unless chasing a suspect on foot): the clock runs. (Where the one
        // last driven was left is noted: far behind a chase on foot it sleeps with its ground.)
        if let Some(p) = run.police.and_then(|c| sim.state.entities.get(c)).map(|e| e.pos) {
            run.police_at = Some(p);
        }
        match self.in_police_car(sim, vd) {
            Some(car) => {
                run.police = Some(car);
                run.out = 0.0;
            }
            None => {
                // The police car wrecked (blown up beside a burning getaway car, shot to bits) or
                // drowned (into the sea): dispatch leaves another at the kerb near by.
                let wrecked = run.police.is_some_and(|c| {
                    sim.state
                        .entities
                        .get(c)
                        .is_some_and(|e| e.vehicle.as_ref().is_some_and(|v| v.state.wrecked || v.state.flooded))
                });
                if wrecked
                    && run.spares < SPARES
                    && let Some(car) = self.vigilante_spare(sim, vd, me)
                {
                    run.spares += 1;
                    run.police = Some(car);
                    run.out = 0.0;
                    let place = self.city.as_ref().map(|c| place_name(c, me)).unwrap_or_default();
                    self.note("police", "Dispatch", &format!("Your unit's done for. A spare car is parked {place}."));
                }
                let chasing = run
                    .suspects
                    .iter()
                    .filter(|s| !s.down && s.car.is_none())
                    .any(|s| sim.feet(s.id).is_some_and(|f| f.distance(me) < FOOT_CHASE));
                if !chasing {
                    run.out += dt;
                }
                if run.out > self.vigilante_leave(vd, run) {
                    let wrecked = run.police.is_none_or(|c| !self.vigilante_car(sim, vd, c));
                    return Some(Ending::Over(
                        if wrecked { "The police car was wrecked." } else { "You left the police car." }.into(),
                    ));
                }
            }
        }
        run.stage_t += dt;
        match run.stage {
            Stage::Call => {
                if run.stage_t >= CALL && self.in_police_car(sim, vd).is_some() {
                    if self.vigilante_spawn(sim, vd, &lv, run) {
                        run.stage = Stage::Chase;
                        run.stage_t = 0.0;
                        run.t = 0.0;
                        let near = run.cars.first().and_then(|w| sim.state.entities.get(w.id)).map(|e| e.pos).unwrap_or(me);
                        let place = self.city.as_ref().map(|c| place_name(c, near)).unwrap_or_default();
                        self.note("police", "Dispatch", &format!("{}, {place}.", lv.call));
                    } else if run.stage_t > 20.0 {
                        return Some(Ending::Over("Dispatch lost the suspects.".into()));
                    }
                }
            }
            Stage::Chase => {
                run.t += dt;
                if let Some(why) = self.vigilante_chase(sim, vd, &lv, run, me, dt, events, out) {
                    return Some(Ending::Over(why));
                }
                if run.suspects.iter().all(|s| s.down) {
                    run.stage = Stage::Clear;
                    run.stage_t = 0.0;
                    for w in &run.cars {
                        crate::world::ambient(sim, w.id);
                    }
                    run.cars.clear();
                    if run.level + 1 >= vd.level.len() {
                        return Some(Ending::Done);
                    }
                    // Between calls the unit is patched up (it is the ram that takes them down).
                    if let Some(car) = run.police {
                        sim.repair_vehicle(car);
                    }
                    self.activities.vigilante.banner = Some(Flash::new(
                        BannerKind::Passed,
                        &format!("LEVEL {} COMPLETE", run.level + 1),
                        Some("Your car is patched up".into()),
                    ));
                    events.push(SimEvent::Sound { cue: "mission.step".into(), pos: None, gain: 1.0, pitch: 1.0 });
                } else if run.t > lv.time {
                    return Some(Ending::Over("Time's up: the suspects got away.".into()));
                }
            }
            Stage::Clear => {
                if run.stage_t >= vd.gap {
                    run.level += 1;
                    run.stage = Stage::Call;
                    run.stage_t = 0.0;
                    // (Those taken in stay where they are and go with their ground.)
                    for s in &run.suspects {
                        crate::world::ambient(sim, s.id);
                    }
                    run.suspects.clear();
                    self.vigilante_call(vd, run.level, events);
                }
            }
        }
        None
    }

    /// Whether the straight way from `a` to `b` stays on the road (every few metres within a
    /// lane's reach: not over a sidewalk, a yard or a building).
    fn on_road(&self, a: Vec3, b: Vec3) -> bool {
        let Some(city) = self.city.as_ref() else { return false };
        let keep = |l: &pav_world::lanes::Lane| !l.closed && l.kind != pav_world::lanes::LaneKind::Channel;
        let n = (a.distance(b) / 4.0).ceil().max(1.0) as usize;
        (0..=n).all(|k| {
            let p = a.lerp(b, k as f32 / n as f32);
            city.lanes.nearest(p, keep).is_some_and(|(_, _, off)| off < 5.0)
        })
    }

    /// A point on the lane nearest `p` that runs against `fwd` (the other side of the road).
    fn lane_toward(&self, p: Vec3, fwd: Vec3) -> Option<Vec3> {
        let city = self.city.as_ref()?;
        let keep = |l: &pav_world::lanes::Lane| {
            !l.closed
                && l.kind != pav_world::lanes::LaneKind::Channel
                && l.kind != pav_world::lanes::LaneKind::Connector
                && l.points.len() >= 2
                && (l.points[l.points.len() - 1] - l.points[0]).normalize_or_zero().dot(fwd) < -0.3
        };
        let (lane, along, off) = city.lanes.nearest(p, keep)?;
        (off < 25.0).then(|| city.lanes.lanes[lane as usize].at(along).0)
    }

    /// Where a getaway car that has lost the police pulls over: on its own side of the road a
    /// little way ahead (None: no such road near).
    fn pull_over(&self, c: &Car) -> Option<Vec3> {
        let city = self.city.as_ref()?;
        let fwd = c.forward();
        let keep = |l: &pav_world::lanes::Lane| {
            !l.closed
                && l.kind != pav_world::lanes::LaneKind::Channel
                && l.kind != pav_world::lanes::LaneKind::Connector
                && l.points.len() >= 2
                && (l.points[l.points.len() - 1] - l.points[0]).normalize_or_zero().dot(fwd) > 0.5
        };
        let (lane, along, off) = city.lanes.nearest(c.feet() + fwd * 35.0, keep)?;
        (off < 15.0).then(|| city.lanes.lanes[lane as usize].at(along).0)
    }

    /// Seconds out of a police car before the run ends: during a chase `leave`; between calls
    /// (walking back after a chase on foot) five times that.
    fn vigilante_leave(&self, vd: &VigilanteDef, run: &Run) -> f32 {
        if run.stage == Stage::Chase { vd.leave } else { vd.leave * 5.0 }
    }

    /// A spare police car at the kerb 15-80 m from `me` (out of view where one will do; on a
    /// street when no kerb is free).
    fn vigilante_spare(&mut self, sim: &mut Sim, vd: &VigilanteDef, me: Vec3) -> Option<EntityId> {
        use pav_world::lanes::LaneKind::*;
        let city = self.city.clone()?;
        let cam = self.camera(sim);
        let ok = |sim: &Sim, p: Vec3| {
            let d = Vec2::new(p.x - me.x, p.z - me.z).length();
            (15.0..=80.0).contains(&d)
                && (p.y - me.y).abs() < 6.0
                && crate::world::ground_loaded(sim, p)
                && !sim.state.entities.iter().any(|e| e.vehicle.is_some() && e.pos.distance(p) < 7.0)
        };
        let kerb = city
            .lanes
            .parking
            .iter()
            .filter(|s| ok(sim, s.pos))
            .map(|s| (crate::traffic::seen(&cam, s.pos), s.pos.distance(me), s.pos, s.dir.x.atan2(s.dir.z)))
            .min_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
        let street = || {
            let keep = |l: &pav_world::lanes::Lane| !l.closed && matches!(l.kind, Street | Avenue | Boulevard);
            let (lane, along, off) = city.lanes.nearest(me, keep)?;
            let ln = &city.lanes.lanes[lane as usize];
            (off < 60.0).then(|| {
                let (p, dir) = ln.at((along + 25.0).min(ln.length));
                (p, dir.x.atan2(dir.z))
            })
        };
        let (at, yaw) = kerb.map(|k| (k.2, k.3)).or_else(street)?;
        let key = vd.cars.first()?;
        crate::vehicles::spawn(sim, key, at + Vec3::Y * 0.05, yaw, None).ok()
    }

    /// The level's suspects in their getaway cars: parked at the kerb somewhere near with the
    /// engine running, lying low (out of view where a spot is; on a road when no spot will do).
    /// False when there was nowhere to put them yet.
    fn vigilante_spawn(&mut self, sim: &mut Sim, vd: &VigilanteDef, lv: &LevelDef, run: &mut Run) -> bool {
        use pav_world::lanes::LaneKind::*;
        let Some(city) = self.city.clone() else { return false };
        let me = self.player_point(sim);
        let cam = self.camera(sim);
        // Nowhere in the band a while (the unit up on the freeway, out on a spit by the water):
        // a wider one, at any level.
        let wide = run.stage_t > CALL + WIDEN_AFTER;
        let (near, far) = if wide { (vd.spawn[0] * 0.5, vd.spawn[1] * 1.6) } else { (vd.spawn[0], vd.spawn[1]) };
        let band = |p: Vec3| {
            let d = Vec2::new(p.x - me.x, p.z - me.z).length();
            d >= near && d <= far && (wide || (p.y - me.y).abs() < 6.0)
        };
        let free = |sim: &Sim, p: Vec3| {
            crate::world::ground_loaded(sim, p)
                && !sim.state.entities.iter().any(|e| e.vehicle.is_some() && e.pos.distance(p) < 6.0)
        };
        // Kerb spots: those out of view first, in a random order.
        let mut spots: Vec<(Vec3, f32, bool)> = city
            .lanes
            .parking
            .iter()
            .filter(|s| band(s.pos) && free(sim, s.pos))
            .map(|s| (s.pos, s.dir.x.atan2(s.dir.z), crate::traffic::seen(&cam, s.pos)))
            .collect();
        for i in (1..spots.len()).rev() {
            let j = sim.state.rng.below(i as u32 + 1) as usize;
            spots.swap(i, j);
        }
        spots.sort_by_key(|s| s.2);
        let mut places: Vec<(Vec3, f32)> = Vec::new();
        for (p, yaw, _) in spots {
            if places.len() >= lv.cars as usize {
                break;
            }
            if places.iter().all(|(q, _)| q.distance(p) > 40.0) {
                places.push((p, yaw));
            }
        }
        // No kerb: on a road (out of view first).
        for pass in 0..2 {
            for _ in 0..60 {
                if places.len() >= lv.cars as usize {
                    break;
                }
                let a = sim.state.rng.f32() * std::f32::consts::TAU;
                let r = sim.state.rng.range(near, far);
                let want = me + Vec3::new(a.cos() * r, 0.0, a.sin() * r);
                let keep = |l: &pav_world::lanes::Lane| !l.closed && matches!(l.kind, Street | Avenue | Boulevard);
                let Some((lane, along, off)) = city.lanes.nearest(want, keep) else { continue };
                let ln = &city.lanes.lanes[lane as usize];
                if off > 20.0 || ln.length < 24.0 {
                    continue;
                }
                let (p, dir) = ln.at(along.clamp(8.0, ln.length - 10.0));
                if !band(p) || (pass == 0 && crate::traffic::seen(&cam, p)) || !free(sim, p) {
                    continue;
                }
                if places.iter().any(|(q, _)| q.distance(p) < 40.0) {
                    continue;
                }
                places.push((p, dir.x.atan2(dir.z)));
            }
        }
        if places.is_empty() {
            return false;
        }
        // The last level's people and cars (those taken in, the wrecks) go back to the world:
        // they go with their ground.
        for id in run.cars.iter().map(|w| w.id).chain(run.suspects.iter().map(|s| s.id)) {
            crate::world::ambient(sim, id);
        }
        run.cars.clear();
        run.suspects.clear();
        for (p, yaw) in places {
            let key = lv.kinds[sim.state.rng.below(lv.kinds.len() as u32) as usize].clone();
            let paint = Some(sim.state.rng.below(8) as usize);
            let Ok(car) = crate::vehicles::spawn(sim, &key, p + Vec3::Y * 0.05, yaw, paint) else { continue };
            if let Some(e) = sim.state.entities.get_mut(car) {
                e.unload = pav_core::entity::Unload::Keep;
                if let Some(v) = e.vehicle.as_deref_mut() {
                    v.state.health = v.def.health * lv.health;
                }
            }
            let seats = 1 + sim.state.entities.get(car).and_then(|e| e.vehicle.as_ref()).map_or(0, |v| v.passengers.len());
            let mut driver = None;
            for k in 0..(lv.riders as usize).min(seats) {
                let id = self.suspect_person(sim, vd, lv, p, yaw);
                if sim.enter_vehicle_seat(id, car, k) {
                    if k == 0 {
                        driver = Some(id);
                    }
                    let hp = if lv.armed.is_some() { vd.armed_hp } else { vd.hp };
                    run.suspects.push(Suspect {
                        id,
                        car: Some(car),
                        armed: lv.armed.is_some(),
                        down: false,
                        runner: Runner::default(),
                        hp,
                        cuffed: false,
                        daze: 0.0,
                    });
                } else {
                    sim.despawn(id);
                }
            }
            let health = sim.state.entities.get(car).and_then(|e| e.vehicle.as_ref()).map_or(0.0, |v| v.state.health);
            // (Waiting at the kerb, engine running: commanded once they run.)
            let _ = driver;
            run.cars.push(Wheels { id: car, fled: false, given_up: false, health, still: 0.0, far: 0.0 });
        }
        !run.cars.is_empty()
    }

    /// A suspect (on the street at `at`, about to get in).
    fn suspect_person(&mut self, sim: &mut Sim, vd: &VigilanteDef, lv: &LevelDef, at: Vec3, yaw: f32) -> EntityId {
        let mut look = vd.look.puppet(&mut sim.state.rng);
        if let Some(w) = &lv.armed {
            look.weapon.asset = crate::combat::asset_of(w);
            if !look.weapon.asset.is_empty() {
                look.weapon.kind = pav_core::puppet::WeaponKind::Dagger;
            }
        }
        let id = sim.spawn_npc("suspect", at, yaw, look, None, None);
        if let Some(e) = sim.state.entities.get_mut(id) {
            e.unload = pav_core::entity::Unload::Keep;
        }
        self.combat.hp.insert(id.0, if lv.armed.is_some() { vd.armed_hp } else { vd.hp });
        id
    }

    /// The chase: rams, sightings, bail-outs, those on foot, takedowns, escapes.
    #[allow(clippy::too_many_arguments)]
    fn vigilante_chase(
        &mut self,
        sim: &mut Sim,
        vd: &VigilanteDef,
        lv: &LevelDef,
        run: &mut Run,
        me: Vec3,
        dt: f32,
        events: &mut Vec<SimEvent>,
        out: &mut BTreeMap<EntityId, InputFrame>,
    ) -> Option<String> {
        let city = self.city.clone()?;
        let mine = self.drive.vehicle().and_then(|c| Car::read(sim, c));
        let siren = self.drive.vehicle().and_then(|c| sim.vehicle_status(c)).is_some_and(|s| s.siren_on);
        let mut any_ram = false;
        for w in run.cars.iter_mut().filter(|w| !w.given_up) {
            let Some(c) = Car::read(sim, w.id) else {
                w.given_up = true;
                continue;
            };
            let st = sim.vehicle_status(w.id);
            let full = sim.state.entities.get(w.id).and_then(|e| e.vehicle.as_ref()).map_or(1.0, |v| v.def.health.max(1.0));
            let mut health = st.as_ref().map_or(0.0, |s| s.health) * full;
            // A ram (or shots) from the police car right there: the getaway car takes more, and
            // the police car's push bar takes most of its own share.
            let rammed =
                health < w.health - 0.5 && mine.as_ref().is_some_and(|m| m.center.distance(c.center) < m.half.z + c.half.z + 3.0);
            if rammed && !c.wrecked {
                let extra = (w.health - health) * (vd.ram - 1.0).max(0.0);
                sim.damage_vehicle(w.id, extra);
                health = (health - extra).max(0.0);
                any_ram = true;
            }
            let hit = health < w.health - 0.5;
            w.health = health;
            // Seen: they run.
            let d = Vec2::new(c.center.x - me.x, c.center.z - me.z).length();
            let sees = d < vd.spot && crate::heat::sight(sim, me + Vec3::Y * 1.2, c.center + Vec3::Y * 0.6);
            if !w.fled && (sees || hit || (siren && d < vd.spot * 2.0)) {
                w.fled = true;
                let driver = c.driver;
                self.traffic.command(sim, &city, w.id, driver, None, Some(me), lv.pace);
                events.push(SimEvent::Sound { cue: "police.scanner".into(), pos: None, gain: 0.7, pitch: 1.1 });
            }
            if w.fled {
                self.traffic.run_from(w.id, me);
                // Well clear of the police car a while (or `escape` m off at once): they think
                // they have lost it, and pull over a little way on to lie low (until they see it
                // again; the level's clock is what runs out).
                w.far = if d > vd.spot * 3.0 { w.far + dt } else { 0.0 };
                if w.far > CALM || d > vd.escape {
                    w.fled = false;
                    w.far = 0.0;
                    let stop = self.pull_over(&c);
                    self.traffic.command(sim, &city, w.id, c.driver, stop, None, 0.8);
                }
            }
            // Boxed in, done for, burning, in the water, on its roof: out they get.
            let near = d < BOXED_NEAR;
            w.still = if w.fled && near && c.vel.length() < 1.0 { w.still + dt } else { 0.0 };
            let beaten = st.as_ref().is_some_and(|s| s.burning || s.in_water || s.upside_down || s.flooded)
                || health <= full * vd.bail
                || w.still > BOXED_FOR
                || c.driver.is_none();
            if beaten && !c.wrecked {
                w.given_up = true;
                self.traffic.dismiss(w.id);
                self.traffic.loose.push(w.id);
                crate::world::ambient(sim, w.id);
                run.left.push(w.id);
                for s in run.suspects.iter_mut().filter(|s| s.car == Some(w.id) && !s.down) {
                    if sim.state.entities.get(s.id).is_some() {
                        sim.exit_vehicle(s.id, w.id);
                        crate::play::play_clip(sim, s.id, "MESH2MOTION/Dizzy", pav_core::clips::ONCE);
                    }
                    s.car = None;
                    s.daze = DAZE;
                    if s.armed {
                        // Out with their guns: they fight (as the side's own).
                        let weapon = lv.armed.clone().unwrap_or_else(|| "p9".into());
                        let feet = sim.feet(s.id).unwrap_or(c.center);
                        self.enemies.list.push(crate::enemies::Enemy {
                            id: s.id,
                            side: vd.side.clone(),
                            weapon,
                            act: crate::enemies::Act::Idle,
                            aware: true,
                            cooldown: 1.0,
                            grenades: 0,
                            retreated: false,
                            anchor: feet,
                            stuck: 0.0,
                            best: f32::MAX,
                            start: 0.0,
                        });
                    }
                }
            }
        }
        // The suspects: down, wrecked, on foot.
        let mut downs = 0;
        let cam = self.camera(sim);
        for s in run.suspects.iter_mut() {
            if s.down {
                // Those who gave up stay on their knees, taken away once out of sight (the fallen
                // too: kept loaded, they would drop through the ground once it unloads).
                if let Some(f) = sim.feet(s.id) {
                    if s.cuffed {
                        out.insert(s.id, InputFrame::default());
                    }
                    let below = f.y < city.ground_y(Vec2::new(f.x, f.z)) - 8.0;
                    if below || (f.distance(me) > 50.0 && !crate::traffic::seen(&cam, f)) {
                        sim.despawn(s.id);
                    }
                }
                continue;
            }
            let alive = sim.state.entities.get(s.id).is_some_and(|e| !e.name.starts_with("dead:"));
            let riding = s.car.filter(|c| sim.state.entities.get(*c).is_some());
            let wreck = riding
                .is_some_and(|c| sim.state.entities.get(c).and_then(|e| e.vehicle.as_ref()).is_some_and(|v| v.state.wrecked));
            let gone = sim.state.entities.get(s.id).is_none();
            if gone && s.car.is_none() {
                return Some("A suspect got away.".into());
            }
            if !alive || wreck || (gone && s.car.is_some()) {
                s.down = true;
                if wreck && !gone {
                    sim.despawn(s.id);
                }
                downs += 1;
                continue;
            }
            if s.car.is_some() {
                continue;
            }
            detail(sim, s.id, me);
            let feet = sim.feet(s.id).unwrap_or(me);
            if feet.y < city.ground_y(Vec2::new(feet.x, feet.z)) - 8.0 {
                // Out of the world (thrown through it by a blast): gone for good.
                sim.despawn(s.id);
                s.down = true;
                downs += 1;
                continue;
            }
            // Unarmed and hit (a punch, a shot, a bump from the car), or armed and badly hurt:
            // they give up on the spot.
            let hp = self.combat.hp.get(&s.id.0).copied();
            let beaten = if s.armed { hp.is_some_and(|h| h < s.hp * GIVE_UP) } else { hp.is_some_and(|h| h < s.hp - 0.01) };
            if beaten {
                s.down = true;
                s.cuffed = true;
                self.enemies.list.retain(|e| e.id != s.id);
                crate::combat::hold(sim, s.id, "");
                crate::play::play_clip(sim, s.id, "WORKSHOP/Arrested", pav_core::clips::CLAMP);
                if let Some(c) = sim.state.entities.get_mut(s.id).and_then(|e| e.character.as_mut()) {
                    c.face = None;
                    c.haste = 0.0;
                }
                out.insert(s.id, InputFrame::default());
                downs += 1;
                continue;
            }
            if s.daze > 0.0 {
                s.daze -= dt;
                out.insert(s.id, InputFrame::default());
            } else if !s.armed {
                // Running from the police near; lying low (crouched where they are) farther off.
                let f = if Vec2::new(feet.x - me.x, feet.z - me.z).length() < vd.hide {
                    s.runner.flee(sim, &city, s.id, me, vd.run, vd.tire, dt)
                } else {
                    if let Some(c) = sim.state.entities.get_mut(s.id).and_then(|e| e.character.as_mut()) {
                        c.haste = 0.0;
                    }
                    s.runner.path.clear();
                    InputFrame { held: pav_core::input::buttons::CROUCH, ..Default::default() }
                };
                out.insert(s.id, f);
            }
        }
        // The police car is built for it: most of what it lost since the last tick is given back
        // (a fire it would have caught put out at once), and its health noted.
        if let Some(m) = mine.as_ref()
            && let Some(v) = sim.state.entities.get_mut(m.id).and_then(|e| e.vehicle.as_deref_mut())
            && !v.state.wrecked
        {
            let before = run.police_hp.filter(|(id, _)| *id == m.id).map_or(v.state.health, |(_, hp)| hp);
            let lost = before - v.state.health;
            if lost > 0.0 {
                let take = if any_ram { vd.tough * 0.5 } else { vd.tough };
                v.state.health = (v.state.health + lost * (1.0 - take)).min(v.def.health);
                if v.state.burning && v.state.burn_time < 0.5 && v.health_share() >= pav_core::vehicle::FIRE_BELOW {
                    v.state.burning = false;
                    v.state.burn_time = 0.0;
                }
            }
            run.police_hp = Some((m.id, v.state.health));
        }
        // Takedowns pay.
        if downs > 0 {
            run.downs += downs;
            run.paid += vd.pay * downs as i64;
            self.activities.vigilante.takedowns += downs;
            self.pay(vd.pay * downs as i64);
            events.push(SimEvent::Sound { cue: "ui.cash".into(), pos: None, gain: 1.0, pitch: 1.0 });
            let left = run.suspects.iter().filter(|x| !x.down).count();
            let title = if downs == 1 { "Suspect down".to_string() } else { format!("{downs} suspects down") };
            let body = if left == 0 {
                format!("+${}", vd.pay * downs as i64)
            } else {
                format!("+${} · {left} to go", vd.pay * downs as i64)
            };
            self.note("police", &title, &body);
        }
        None
    }

    /// The last level done: the bonus, completion, the banner.
    fn vigilante_done(&mut self, sim: &mut Sim, vd: &VigilanteDef, events: &mut Vec<SimEvent>) {
        let Some(mut run) = self.activities.vigilante.run.take() else { return };
        self.vigilante_release(sim, &mut run);
        self.pay(vd.bonus);
        run.paid += vd.bonus;
        let st = &mut self.activities.vigilante;
        st.finished += 1;
        st.last = format!("done: {} takedowns, ${}", run.downs, run.paid);
        if st.trial == Trial::Playing {
            st.trial = Trial::Won;
        }
        st.banner = Some(Flash::new(
            BannerKind::Passed,
            &format!("{} COMPLETE", vd.title.to_uppercase()),
            Some(format!("${} earned", run.paid)),
        ));
        self.activity_done("vigilante");
        self.autosave = true;
        events.push(SimEvent::Sound { cue: "mission.pass".into(), pos: None, gain: 1.0, pitch: 1.0 });
        self.vigilante_pilot_off(sim);
    }

    /// The run ends early: the banner, the suspects let go.
    fn vigilante_over(&mut self, sim: &mut Sim, vd: &VigilanteDef, why: String, events: &mut Vec<SimEvent>) {
        let Some(mut run) = self.activities.vigilante.run.take() else { return };
        self.vigilante_release(sim, &mut run);
        let st = &mut self.activities.vigilante;
        st.last = format!("over at level {}: {why}", run.level + 1);
        if st.trial == Trial::Playing {
            st.trial = Trial::Lost;
        }
        st.banner = Some(Flash::new(BannerKind::Failed, &format!("{} OVER", vd.title.to_uppercase()), Some(why)));
        events.push(SimEvent::Sound { cue: "mission.fail".into(), pos: None, gain: 0.8, pitch: 1.0 });
        self.vigilante_pilot_off(sim);
    }

    /// Civilians killed on duty (no report: the run's pay docked).
    pub(crate) fn vigilante_casualties(&mut self, deaths: usize) {
        let Some(fine) = super::def().ok().and_then(|d| d.vigilante.as_ref().map(|vd| vd.fine * deaths as i64)) else { return };
        let Some(run) = self.activities.vigilante.run.as_mut() else { return };
        run.paid -= fine;
        self.pay(-fine);
        self.note("police", "Civilian casualty", &format!("-${fine}"));
    }

    /// Ends a run without a word (a mission begins).
    pub(crate) fn vigilante_abort(&mut self, sim: &mut Sim) {
        if let Some(mut run) = self.activities.vigilante.run.take() {
            self.vigilante_release(sim, &mut run);
            self.activities.vigilante.last = "ended by a mission".into();
            if self.activities.vigilante.trial == Trial::Playing {
                self.activities.vigilante.trial = Trial::Lost;
            }
        }
    }

    /// The bot's autopilot off once a run is over (the evasion bot's aside).
    fn vigilante_pilot_off(&mut self, sim: &mut Sim) {
        if !self.bot.escape
            && self.traffic.cars.iter().any(|a| a.pilot)
            && let Some(city) = self.city.clone()
        {
            self.traffic.autopilot(sim, &city, None);
        }
        self.activities.vigilante.bot = MissionBot::default();
    }

    /// The suspects still up go back to the street: the getaway cars into traffic, those on
    /// foot to the crowd (running), the armed ones out of the fight.
    fn vigilante_release(&mut self, sim: &mut Sim, run: &mut Run) {
        let Some(city) = self.city.clone() else { return };
        for w in &run.cars {
            if let Some(a) = self.traffic.cars.iter_mut().find(|a| a.car == w.id) {
                a.mission = false;
                a.away = None;
                a.goal = None;
                a.desire = 1.0;
            }
            crate::world::ambient(sim, w.id);
        }
        for s in &run.suspects {
            // (Those taken in or down stay where they are, and go with their ground.)
            crate::world::ambient(sim, s.id);
            if s.down {
                continue;
            }
            self.enemies.list.retain(|e| e.id != s.id);
            self.combat.hp.remove(&s.id.0);
            if sim.state.entities.get(s.id).is_none() {
                continue;
            }
            if s.car.is_none() {
                self.back_to_crowd(sim, &city, s.id, None, "", Some(Vec::new()));
            }
        }
        run.cars.clear();
        run.suspects.clear();
    }

    // ---- The bot ----

    /// Plans the vigilante bot's next frame.
    fn vigilante_bot(&mut self, sim: &Sim, vd: &VigilanteDef, dt: f32) {
        let mut b = std::mem::take(&mut self.activities.vigilante.bot);
        b.frame = InputFrame::default();
        b.tick = b.tick.wrapping_add(1);
        if let Some(run) = self.activities.vigilante.run.clone() {
            self.vigilante_plan(&mut b, sim, vd, &run, dt);
        }
        self.activities.vigilante.bot = b;
    }

    /// Into the police car — from its far side when its door is blocked a while (parked hard
    /// against something, the car beside it the nearer one there).
    fn vigilante_enter(&mut self, b: &mut MissionBot, sim: &Sim, car: EntityId, dt: f32) {
        let me = self.player_point(sim);
        let Some(c) = Car::read(sim, car) else { return };
        let flat = |p: Vec3| Vec2::new(p.x, p.z);
        let near = flat(c.center).distance(flat(me)) < c.half.z + 3.0 && !self.drive.busy();
        let st = &mut self.activities.vigilante;
        st.board = if near { st.board + dt } else { 0.0 };
        if st.board > 12.0 {
            st.board = 0.0;
        }
        if st.board > 4.0 {
            if crate::drive::Drive::nearest(sim, me).is_some_and(|(n, _)| n == car) {
                b.tap(pad::NORTH);
                return;
            }
            let side = flat(c.door_point() - c.feet()).normalize_or_zero();
            let spot = flat(c.feet()) - side * (c.half.x + 0.7);
            self.bot_walk(b, sim, Vec3::new(spot.x, me.y, spot.y), false, dt);
            return;
        }
        self.bot_enter(b, sim, car, dt);
    }

    /// The policy: suspects on foot first (out of the car after them: fists for the unarmed, a
    /// gun for the armed); else back in a police car; else after the nearest getaway car — on
    /// the autopilot, and straight into it once close behind.
    fn vigilante_plan(&mut self, b: &mut MissionBot, sim: &Sim, vd: &VigilanteDef, run: &Run, dt: f32) {
        if self.drive.busy() {
            return;
        }
        let me = self.player_point(sim);
        let flat = |p: Vec3| Vec2::new(p.x, p.z);
        let piloted = self.traffic.cars.iter().any(|a| a.pilot);
        let car = self.drive.vehicle().and_then(|c| Car::read(sim, c));
        let police = self.in_police_car(sim, vd);
        if self.phone.open {
            b.tap(pad::EAST);
            return;
        }
        // A burning car: out of it before it goes up (△ held at speed bails out; a tap stops and
        // gets out).
        let burning = |id: EntityId| sim.vehicle_status(id).is_some_and(|v| v.burning);
        if let Some(c) = car.as_ref().filter(|c| burning(c.id)) {
            if piloted {
                b.cmd("autopilot off".into());
            }
            if c.speed().abs() >= crate::drive::BAIL_SPEED {
                b.frame.pad.held |= pad::NORTH;
            } else {
                b.tap(pad::NORTH);
            }
            self.activities.vigilante.trace = "out of a burning car".into();
            return;
        }
        // On foot by a burning car: clear of it first.
        if car.is_none()
            && let Some(fire) = sim
                .state
                .entities
                .iter()
                .filter(|e| e.vehicle.as_ref().is_some_and(|v| v.state.burning && !v.state.wrecked))
                .map(|e| e.pos)
                .filter(|p| flat(*p).distance(flat(me)) < 12.0)
                .min_by(|a, c| a.distance(me).total_cmp(&c.distance(me)))
        {
            let away = me + (me - fire).with_y(0.0).normalize_or(Vec3::X) * 10.0;
            self.bot_walk(b, sim, away, true, dt);
            self.activities.vigilante.trace = "clear of a burning car".into();
            return;
        }
        // Suspects on foot, nearest first (those far off once no getaway car is left).
        let cars_left = run.cars.iter().any(|w| !w.given_up);
        let foot = run
            .suspects
            .iter()
            .filter(|s| !s.down && s.car.is_none())
            .filter_map(|s| Some((s.id, sim.feet(s.id)?, s.armed)))
            .filter(|(_, f, _)| !cars_left || f.distance(me) < FOOT_CHASE + 20.0)
            .min_by(|a, c| a.1.distance(me).total_cmp(&c.1.distance(me)));
        if let Some((id, at, armed)) = foot {
            let d = flat(at).distance(flat(me));
            // A runner well off: after them in the police car first (out once close, or once the
            // road gets no nearer: the autopilot has stopped where it ends nearest them).
            // (Once out for that, on foot a while: not back in to find the road no nearer again.)
            let rolling = car.as_ref().is_some_and(|c| c.vel.length() > 1.0);
            let st = &mut self.activities.vigilante;
            st.stall = if car.is_some() && !rolling { st.stall + dt } else { 0.0 };
            let road_ends = self.traffic.pilot_arrived() || st.stall > 3.0;
            if road_ends {
                st.afoot = 15.0;
            }
            st.afoot -= dt;
            let afoot = st.afoot > 0.0;
            if let (Some(_), Some(_)) = (&car, police)
                && d > if armed { 60.0 } else { 28.0 }
                && !road_ends
                && !afoot
            {
                self.bot_drive_moving(b, at, 1.1, dt);
                self.activities.vigilante.trace = format!("drive to a runner d={d:.0}");
                return;
            }
            if car.is_none()
                && d > if armed { 80.0 } else { 45.0 }
                && !afoot
                && let Some(pc) = run.police.filter(|c| self.vigilante_car(sim, vd, *c) && !burning(*c))
                && sim.state.entities.get(pc).is_some_and(|e| e.pos.distance(me) < 15.0)
            {
                self.vigilante_enter(b, sim, pc, dt);
                self.activities.vigilante.trace = format!("back in for a runner d={d:.0}");
                return;
            }
            if let Some(c) = &car {
                // Stop, then out.
                if piloted {
                    b.cmd("autopilot off".into());
                }
                if c.speed() > 2.5 {
                    b.frame.pad.l2 = 1.0;
                    b.frame.pad.held |= pad::L2;
                } else {
                    b.tap(pad::NORTH);
                }
                return;
            }
            let hp = self.combat.hp.get(&id.0).copied().unwrap_or(-1.0);
            self.activities.vigilante.trace = format!(
                "foot d={:.1} armed={armed} weapon={} hits={} hp={hp:.0} me={:?} them={:?}",
                flat(at).distance(flat(me)),
                self.weapon,
                self.combat.hits,
                (me.x as i32, me.z as i32),
                (at.x as i32, at.z as i32)
            );
            // An armed one nothing lands on a while (cover between them): up close with fists a
            // while instead, then the gun again.
            let st = &mut self.activities.vigilante;
            let hp_now = self.combat.hp.get(&id.0).copied().unwrap_or(0.0);
            if st.fight != Some(id) || hp_now < st.fight_hp - 0.01 {
                st.fight = Some(id);
                st.fight_hp = hp_now;
                st.fight_t = 0.0;
            } else {
                st.fight_t += dt;
            }
            if st.fight_t > 40.0 {
                st.fight_t = 0.0;
            }
            let close_in = armed && st.fight_t > 12.0;
            if armed && !close_in {
                self.bot_shoot(b, sim, id, at, dt);
            } else {
                if self.cover.at.is_some() && b.tick.is_multiple_of(20) {
                    b.tap(pad::R1);
                }
                if self.weapon != 0 && b.tick.is_multiple_of(20) {
                    b.cmd("weapon fists".into());
                }
                let d = flat(at).distance(flat(me));
                if d < 1.7 && self.weapon == 0 {
                    b.frame.move_dir = (flat(at) - flat(me)).normalize_or(Vec2::ZERO) * 0.3;
                    b.tap(pad::EAST);
                } else {
                    let mut way = self.activities.vigilante.headway;
                    way.check(b, flat(me), dt);
                    self.activities.vigilante.headway = way;
                    self.bot_walk(b, sim, at, true, dt);
                    // (Flat out right up to them: a runner is as quick as a jog.)
                    b.frame.held |= pav_core::input::buttons::SPRINT;
                }
            }
            return;
        }
        // Into a police car.
        let Some(mycar) = police else {
            if car.is_some() {
                b.tap(pad::NORTH);
                return;
            }
            // Out of the last fight's cover first (in it, the stick only slides along the wall),
            // and up from a crouch.
            if self.cover.at.is_some() {
                b.tap(pad::R1);
                return;
            }
            if self.foot.crouched && b.tick.is_multiple_of(30) {
                b.tap(pad::L3);
            }
            let back = run.police.filter(|c| self.vigilante_car(sim, vd, *c) && !burning(*c)).or_else(|| {
                sim.state
                    .entities
                    .iter()
                    .filter(|e| {
                        vd.cars.contains(&e.name)
                            && e.vehicle
                                .as_ref()
                                .is_some_and(|v| !v.state.wrecked && !v.state.burning && !v.state.flooded && v.driver.is_none())
                    })
                    .map(|e| (e.id, e.pos.distance(me)))
                    .filter(|x| x.1 < 120.0)
                    .min_by(|a, c| a.1.total_cmp(&c.1))
                    .map(|x| x.0)
            });
            if let Some(c) = back {
                self.vigilante_enter(b, sim, c, dt);
                self.activities.vigilante.trace = format!("into the police car {}", c.0);
            } else if let Some(at) = run.police_at.filter(|_| run.police.is_some_and(|c| sim.state.entities.get(c).is_none())) {
                // Asleep with its ground far behind: back to where it was left (it wakes on the way).
                self.bot_walk(b, sim, at, true, dt);
                self.activities.vigilante.trace = format!("back to the police car at {:?}", (at.x as i32, at.z as i32));
            } else {
                self.activities.vigilante.trace = "no police car near".into();
            }
            return;
        };
        let Some(c) = car.filter(|c| c.id == mycar) else { return };
        // Off the roads (a yard, a lot: the autopilot knows only the roads): by hand to the
        // nearest road in sight first.
        if let Some(to) = self.road_in_sight(sim, b, &c, dt) {
            if piloted {
                b.cmd("autopilot off".into());
            }
            let want = (flat(to) - flat(c.feet())).normalize_or_zero();
            let fwd = flat(c.forward()).normalize_or_zero();
            let cross = fwd.x * want.y - fwd.y * want.x;
            let st = &mut self.activities.vigilante;
            st.stuck = if c.vel.length() < 0.8 && st.back <= 0.0 { st.stuck + dt } else { 0.0 };
            if st.stuck > 1.5 {
                st.stuck = 0.0;
                st.back = 1.2;
                st.side = -st.side;
            }
            if st.back > 0.0 {
                // (Out the way it came, wheels the other way from the last time.)
                st.back -= dt;
                let into = fwd.dot(want) > -0.3;
                b.frame.pad.stick_l.x = if st.side >= 0.0 { 1.0 } else { -1.0 };
                if into {
                    b.frame.pad.l2 = 0.8;
                    b.frame.pad.held |= pad::L2;
                } else {
                    b.frame.pad.r2 = 0.6;
                    b.frame.pad.held |= pad::R2;
                }
                st.trace = format!("off road, stuck: backing off to {:?}", (to.x as i32, to.z as i32));
                return;
            }
            if fwd.dot(want) > -0.3 {
                b.frame.pad.stick_l.x = (cross * 2.5).clamp(-1.0, 1.0);
                b.frame.pad.r2 = 0.45;
                b.frame.pad.held |= pad::R2;
            } else {
                b.frame.pad.stick_l.x = -(cross.signum());
                b.frame.pad.l2 = 0.6;
                b.frame.pad.held |= pad::L2;
            }
            self.activities.vigilante.trace = format!("off road: to {:?}", (to.x as i32, to.z as i32));
            return;
        }
        // The getaway car it is after (the same one until they give it up), else the nearest.
        let chasing = |id: EntityId| run.cars.iter().any(|w| w.id == id && !w.given_up);
        let locked =
            self.activities.vigilante.target.filter(|id| chasing(*id)).and_then(|id| Car::read(sim, id)).filter(|t| !t.wrecked);
        let target = locked.or_else(|| {
            run.cars
                .iter()
                .filter(|w| !w.given_up)
                .filter_map(|w| Car::read(sim, w.id))
                .filter(|t| !t.wrecked)
                .min_by(|a, x| a.center.distance(me).total_cmp(&x.center.distance(me)))
        });
        self.activities.vigilante.target = target.as_ref().map(|t| t.id);
        let Some(t) = target else {
            // Between levels: wait here.
            if piloted {
                b.cmd("autopilot off".into());
            }
            if c.speed() > 1.0 {
                b.frame.pad.l2 = 1.0;
                b.frame.pad.held |= pad::L2;
            }
            self.activities.vigilante.ram = 0.0;
            return;
        };
        let lead = t.center + t.vel * 0.35;
        let to = lead - c.center;
        let d = Vec2::new(to.x, to.z).length();
        let fwd = flat(c.forward()).normalize_or(Vec2::Y);
        let want = flat(to).normalize_or(fwd);
        let ahead = fwd.dot(want);
        // In sight, and the straight way there on the road (never across a yard or a sidewalk).
        let clear = crate::heat::sight(sim, c.center + Vec3::Y * 0.4, t.center + Vec3::Y * 0.4) && self.on_road(c.feet(), lead);
        let st = &mut self.activities.vigilante;
        st.cool -= dt;
        // No headway a while (back and forth at the foot of a hill too steep for it, boxed in at
        // a jam): by hand, first flat out the way the autopilot wanted to go (up the hill, shoving
        // through), the next time turned about (the autopilot takes it from there another way).
        let fresh = st.prog_tick.wrapping_add(1) != b.tick;
        st.prog_tick = b.tick;
        if fresh || c.center.distance(st.prog_at) > 25.0 || d < st.prog_d - 10.0 {
            if !fresh {
                st.tries = 0;
            }
            st.prog_at = c.center;
            st.prog_d = d;
            st.prog = 0.0;
        } else {
            st.prog += dt;
        }
        if st.prog > NO_HEADWAY {
            st.prog = 0.0;
            st.prog_d = d;
            st.ram = 0.0;
            st.stuck = 0.0;
            st.tries += 1;
            st.push = st.tries % 2 == 1;
            if st.push {
                let city = self.city.as_ref();
                let ahead_on_lane = self.traffic.cars.iter().find(|a| a.pilot).and_then(|a| {
                    let ln = city?.lanes.lanes.get(a.lane as usize)?;
                    Some(ln.at((a.along + 15.0).min(ln.length)).0)
                });
                st.about = Some(ahead_on_lane.map_or(fwd, |p| (flat(p) - flat(c.feet())).normalize_or(fwd)));
                st.turn = 5.0;
            } else {
                st.about = Some(-fwd);
                st.turn = 7.0;
            }
        }
        // Ram: close and in front, or straight ahead a little farther (tailing it on the road).
        if st.ram <= 0.0 && st.cool <= 0.0 && clear && ((d < 24.0 && ahead > 0.65) || (d < 48.0 && ahead > 0.9)) {
            st.ram = 8.0;
            st.stuck = 0.0;
        }
        if st.ram > 0.0 {
            st.ram -= dt;
            if d > 60.0 || ahead < 0.1 || !clear || st.ram <= 0.0 {
                st.ram = 0.0;
                st.cool = 2.5;
            }
        }
        // Stalled on the autopilot with it behind (the autopilot stops for good at a goal behind
        // it on its own lane): round by hand until it is in front.
        if st.ram <= 0.0 && st.turn <= 0.0 {
            st.stall = if c.vel.length() < 1.0 { st.stall + dt } else { 0.0 };
            if st.stall > 4.0 && ahead < 0.5 && d < 150.0 {
                st.turn = 7.0;
                st.stall = 0.0;
                st.stuck = 0.0;
            }
        }
        let (want, ahead) = match st.about.filter(|_| st.turn > 0.0) {
            Some(w) => (w, fwd.dot(w)),
            None => (want, ahead),
        };
        if st.turn > 0.0 {
            st.turn -= dt;
            if ahead > 0.8 && !st.push {
                st.turn = 0.0;
            }
        }
        if st.turn <= 0.0 {
            st.about = None;
            st.push = false;
        }
        if st.ram > 0.0 || st.turn > 0.0 {
            if piloted {
                b.cmd("autopilot off".into());
            }
            // Backing off a stuck nose, else at it (easy while turning round, flat out in line).
            st.stuck = if c.vel.length() < 1.0 { st.stuck + dt } else { 0.0 };
            if st.stuck > 1.5 {
                st.stuck = 0.0;
                st.back = 1.0;
            }
            let cross = fwd.x * want.y - fwd.y * want.x;
            let mode = if st.ram > 0.0 {
                "ram"
            } else if st.push {
                "push"
            } else if st.about.is_some() {
                "about"
            } else {
                "turn"
            };
            st.trace =
                format!("{mode} d={d:.0} ahead={ahead:.2} speed={:.1} stuck={:.1} back={:.1}", c.speed(), st.stuck, st.back);
            if st.back > 0.0 {
                st.back -= dt;
                b.frame.pad.stick_l.x = -(cross * 2.5).clamp(-1.0, 1.0);
                b.frame.pad.l2 = 1.0;
                b.frame.pad.held |= pad::L2;
            } else {
                // (Behind: full lock, the way round it is; slowing first to turn tight.)
                let steer = if ahead < 0.0 { cross.signum() } else { (cross * 2.5).clamp(-1.0, 1.0) };
                b.frame.pad.stick_l.x = steer;
                if st.ram <= 0.0 && !st.push && c.speed() > 8.0 {
                    b.frame.pad.l2 = 1.0;
                    b.frame.pad.held |= pad::L2;
                } else {
                    b.frame.pad.r2 = if ahead > 0.7 || (st.push && ahead > 0.3) { 1.0 } else { 0.55 };
                    b.frame.pad.held |= pad::R2;
                }
            }
            return;
        }
        // On the autopilot after it, in a hurry: to a point a lane or two along its own route
        // (the same roads), else ahead of it.
        // (Ahead of the bot, that is; behind or beside it: to the car itself, round the block.)
        let on_route = self.traffic.cars.iter().find(|a| a.car == t.id).filter(|_| ahead > 0.3).and_then(|a| {
            let lane = a.route.get(1).or(a.route.first()).copied().unwrap_or(a.lane);
            self.city.as_ref()?.lanes.lanes.get(lane as usize)?.points.last().copied()
        });
        // Behind it is: the lane beside it running the other way (the autopilot plans round the
        // block, or a U-turn, to get there; a goal behind on its own lane it would just stop at).
        let back_way = (ahead < 0.3).then(|| self.lane_toward(t.feet(), c.forward())).flatten();
        let goal = on_route.or(back_way).unwrap_or(t.feet());
        self.bot_drive_moving(b, goal, 1.35, dt);
        let pilot = self.traffic.cars.iter().find(|a| a.pilot).map(|a| {
            format!(
                "{} {:.1} arrived {} goal {:?} route {} lane {} in {:?} drive {:?} blocked {:.1} rev {:.1}",
                a.why,
                a.speed,
                a.arrived,
                a.goal.map(|g| (g.x as i32, g.z as i32)),
                a.route.len(),
                a.lane,
                self.traffic.pilot_input.map(|i| (i.throttle, i.brake, i.steer)),
                self.drive.input.map(|i| (i.1.throttle, i.1.brake)),
                a.blocked,
                a.reverse
            )
        });
        let vs = sim.vehicle_status(c.id);
        let state = vs.map(|v| {
            format!(
                "hp {:.2} wheels {} upside {} engine {} burst {:?}",
                v.health, v.wheels_down, v.upside_down, v.engine_on, v.burst
            )
        });
        self.activities.vigilante.trace =
            format!("chase d={d:.0} ahead={ahead:.2} clear={clear} speed={:.1} pilot={pilot:?} car={state:?}", c.speed());
    }

    // ---- What the HUD shows ----

    pub(crate) fn vigilante_ui(&self, sim: &Sim, ui: &mut UiModel, draw: &mut DrawList) {
        let _ = draw;
        if let Some(b) = &self.activities.vigilante.banner {
            ui.banners.push(b.banner());
        }
        let Ok(d) = super::def() else { return };
        let Some(vd) = d.vigilante.as_ref() else { return };
        let Some(run) = &self.activities.vigilante.run else {
            if self.vigilante_ready(sim, vd) && !self.phone.open {
                ui.prompts.push(Prompt { button: PadButton::Right, label: vd.title.clone(), hold: None });
            }
            return;
        };
        let lv = vd.level.get(run.level);
        let me = self.player_point(sim);
        let in_car = self.in_police_car(sim, vd).is_some();
        let left = run.suspects.iter().filter(|s| !s.down).count();
        let on_foot = run.suspects.iter().any(|s| !s.down && s.car.is_none());
        let objective = match run.stage {
            Stage::Call if !in_car => "Get back in a ~b~police car~s~.".to_string(),
            Stage::Call => format!("Dispatch: {}. Stand by.", lv.map_or("", |l| l.call.as_str())),
            Stage::Chase if !in_car && !on_foot => "Get back in a ~b~police car~s~.".to_string(),
            Stage::Chase if on_foot => "Take down the ~r~suspects~s~ on foot.".to_string(),
            Stage::Chase => format!("Take down the ~r~suspects~s~: ram their car until they bail ({left} left)."),
            Stage::Clear => format!("Level {} complete.", run.level + 1),
        };
        if let Some(h) = &mut ui.hud {
            h.objective = Some(objective);
            if run.out > 0.0 {
                let s = (self.vigilante_leave(vd, run) - run.out).max(0.0);
                h.timer = Some(TimerHud { label: "POLICE CAR".into(), seconds: s, warn: s < 5.0 });
            } else if let (Stage::Chase, Some(l)) = (run.stage, lv) {
                let s = (l.time - run.t).max(0.0);
                h.timer = Some(TimerHud { label: "TIME".into(), seconds: s, warn: s < 20.0 });
            }
            let total = run.suspects.len().max(1);
            h.meters.push(Meter {
                label: format!("LEVEL {}/{}", run.level + 1, vd.level.len()),
                value: if run.stage == Stage::Chase { (total - left) as f32 / total as f32 } else { 0.0 },
                color: Color::hex("#3d8bff"),
            });
        }
        // The suspects: their cars (until they bail), then themselves.
        let red = Color::hex("#e8423f");
        let mark = |ui: &mut UiModel, p: Vec3, icon: BlipIcon, label: &str, pulse: bool, lift: f32| {
            if let Some(h) = &mut ui.hud
                && let Some(m) = &mut h.minimap
            {
                m.blips.push(Blip { pos: Vec2::new(p.x, p.z), icon, color: red, label: None, clamp: true, height: 0, pulse });
            }
            if p.distance(me) < 250.0 {
                ui.markers.push(ScreenMarker {
                    pos: p + Vec3::Y * lift,
                    icon,
                    color: red,
                    label: Some(label.into()),
                    distance: true,
                });
            }
        };
        for w in run.cars.iter().filter(|w| !w.given_up) {
            if let Some(c) = Car::read(sim, w.id) {
                mark(ui, c.feet(), BlipIcon::Vehicle, "Suspects", w.fled, 2.6);
            }
        }
        for s in run.suspects.iter().filter(|s| !s.down && s.car.is_none()) {
            if let Some(f) = sim.feet(s.id) {
                mark(ui, f, BlipIcon::Enemy, "Suspect", true, 2.2);
            }
        }
        // Out of it: the police car (where it was left).
        if !in_car && let Some(p) = self.vigilante_police_at(sim, run) {
            let blue = Color::hex("#3d8bff");
            if let Some(h) = &mut ui.hud
                && let Some(m) = &mut h.minimap
            {
                m.blips.push(Blip {
                    pos: Vec2::new(p.x, p.z),
                    icon: BlipIcon::Vehicle,
                    color: blue,
                    label: None,
                    clamp: true,
                    height: 0,
                    pulse: false,
                });
            }
            if p.distance(me) < 250.0 {
                ui.markers.push(ScreenMarker {
                    pos: p + Vec3::Y * 2.4,
                    icon: BlipIcon::Vehicle,
                    color: blue,
                    label: Some("Police car".into()),
                    distance: true,
                });
            }
        }
    }

    /// Where the run's police car is (or was last seen, asleep with its ground far behind).
    fn vigilante_police_at(&self, sim: &Sim, run: &Run) -> Option<Vec3> {
        let car = run.police?;
        match sim.state.entities.get(car) {
            Some(e) if e.vehicle.as_ref().is_some_and(|v| !v.state.wrecked) => Some(e.pos),
            Some(_) => None,
            None => run.police_at,
        }
    }

    /// Where the GPS points on a run: the nearest suspect (on a 10 m grid, as missions do), or the
    /// police car once out of it with nobody on foot to chase.
    pub(crate) fn vigilante_waypoint(&self, sim: &Sim) -> Option<Vec2> {
        let run = self.activities.vigilante.run.as_ref()?;
        let me = self.player_point(sim);
        // Out of the police car with nobody on foot to chase: back to it.
        let afoot = run.suspects.iter().any(|s| !s.down && s.car.is_none());
        let in_car = super::def().ok().and_then(|d| d.vigilante.clone()).is_some_and(|vd| self.in_police_car(sim, &vd).is_some());
        if !in_car
            && !afoot
            && let Some(p) = self.vigilante_police_at(sim, run)
        {
            return Some(Vec2::new(p.x, p.z));
        }
        let p = run
            .suspects
            .iter()
            .filter(|s| !s.down)
            .filter_map(|s| match s.car {
                Some(c) => sim.state.entities.get(c).map(|e| e.pos),
                None => sim.feet(s.id),
            })
            .min_by(|a, b| a.distance(me).total_cmp(&b.distance(me)))?;
        let q = |v: f32| (v / 10.0).round() * 10.0;
        Some(Vec2::new(q(p.x), q(p.z)))
    }

    /// The run for `status`.
    pub(crate) fn vigilante_status(&self, sim: &Sim) -> serde_json::Value {
        let st = &self.activities.vigilante;
        let run = st.run.as_ref().map(|r| {
            serde_json::json!({
                "level": r.level + 1,
                "stage": format!("{:?}", r.stage).to_lowercase(),
                "t": (r.t * 10.0).round() / 10.0,
                "left": r.suspects.iter().filter(|s| !s.down).count(),
                "on_foot": r.suspects.iter().filter(|s| !s.down && s.car.is_none()).count(),
                "fled": r.cars.iter().filter(|w| w.fled && !w.given_up).count(),
                "downs": r.downs,
                "out": (r.out * 10.0).round() / 10.0,
                "cars": r.cars.iter().map(|w| serde_json::json!({ "id": w.id.0, "fled": w.fled, "given_up": w.given_up, "health": w.health.round(), "still": (w.still * 10.0).round() / 10.0 })).collect::<Vec<_>>(),
                "suspects": r.suspects.iter().map(|s| {
                    let at = sim.feet(s.id).map(|f| [(f.x * 10.0).round() / 10.0, (f.y * 10.0).round() / 10.0, (f.z * 10.0).round() / 10.0]);
                    serde_json::json!({ "id": s.id.0, "car": s.car.map(|c| c.0), "down": s.down, "cuffed": s.cuffed, "armed": s.armed, "at": at })
                }).collect::<Vec<_>>(),
            })
        });
        let walk = st.bot.path.iter().take(3).map(|p| [p.x.round(), p.y.round()]).collect::<Vec<_>>();
        serde_json::json!({ "run": run, "best": st.best, "takedowns": st.takedowns, "runs": st.runs, "finished": st.finished, "last": st.last, "bot": st.trace, "walk": walk })
    }
}
