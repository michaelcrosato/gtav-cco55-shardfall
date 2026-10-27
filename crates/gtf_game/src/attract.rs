//! The attract loop (PLAN.md §5.8): after 10 s without input on the title or a main-menu page
//! (at once in `freeway/attract` and on touch screens) the game plays itself — the vignettes of
//! `content/freeway/attract.toml`, in an order the sim's random generator shuffles each loop,
//! each set up from scratch (the population and whatever the last one left go) and played by
//! one of the game's own bots under a shot list, with a cut through black and a caption card
//! between them. Any input returns to the title at once and does nothing else there; the game
//! comes back as it stood (its save, kept on the way in). A vignette whose bot stalls ends early
//! and counts as an anomaly (`status` → `attract`, the attract soak).
//!
//! Touch screens never leave the loop: the only thing drawn over the world is the gear (the
//! host draws it), which opens a small overlay (sound, graphics, the frame-rate readout, About).

use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use glam::{Vec2, Vec3};
use pav_core::InputFrame;
use pav_core::entity::EntityId;
use pav_core::frame::SimEvent;
use pav_core::input::pad;
use pav_core::present::*;
use pav_core::sim::Sim;
use serde::{Deserialize, Serialize};

use crate::Freeway;
use crate::frontend::Page;
use crate::missions::bot::MissionBot;

/// The loop's settings.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoopDef {
    /// Seconds of the cut through black (with the caption card) between vignettes.
    pub cut: f32,
    /// Seconds a bot may go without getting anywhere before its vignette ends early.
    pub stall: f32,
    /// Seconds each shot of a vignette's list holds.
    pub shot_secs: f32,
}

/// Who plays a vignette.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BotKind {
    /// The autopilot, to `goal` (else wherever the roads go).
    Cruise,
    /// The evasion bot, losing the police.
    Escape,
    /// A run at the stunt jump `stunt`.
    Stunt,
    /// The autopilot, and a switch to the next protagonist every `switch_every` s.
    Switch,
    /// On foot toward `goal`.
    Stroll,
    /// On foot, shooting whoever comes (`enemies` in `cmds`).
    Fight,
    /// The mission bot, playing the mission `cmds` start.
    Mission,
}

/// One vignette.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VignetteDef {
    pub id: String,
    /// The caption card's line (and the corner's, under the logo).
    pub caption: String,
    pub seconds: f32,
    /// The hour (HH:MM) and the weather.
    pub time: String,
    #[serde(default = "clear")]
    pub weather: String,
    /// Who is played (a protagonist's key).
    pub protagonist: String,
    /// Where they start (an anchor), and the vehicle they start in.
    #[serde(default)]
    pub at: Option<String>,
    #[serde(default)]
    pub vehicle: Option<String>,
    /// The station on (a station's id; none: the car's own).
    #[serde(default)]
    pub radio: Option<String>,
    /// Stars to start with.
    #[serde(default)]
    pub heat: u8,
    pub bot: BotKind,
    #[serde(default)]
    pub goal: Option<String>,
    /// Where a cruise heads (an anchor), not needing to arrive: it keeps the autopilot on
    /// planned roads.
    #[serde(default)]
    pub toward: Option<String>,
    #[serde(default)]
    pub stunt: Option<String>,
    #[serde(default)]
    pub switch_every: Option<f32>,
    /// Commands typed into the run once it is set up (`enemies 4 gang`, `clock 40`, ...).
    #[serde(default)]
    pub cmds: Vec<String>,
    /// The camera's shots, one every `shot_secs`: `chase` (behind, as played), `cine` (the
    /// chase camera's cinematic cycle, as ○ held in a car gives it), `drone` (a slow orbit over
    /// the player).
    #[serde(default)]
    pub shots: Vec<String>,
    /// Show the normal HUD's radar and stars (chases).
    #[serde(default)]
    pub hud: bool,
    /// Seconds without headway that count as stalled here (none: the loop's `stall`): a
    /// mission's stand waits for its attackers.
    #[serde(default)]
    pub stall: Option<f32>,
}

fn clear() -> String {
    "clear".into()
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttractDef {
    pub attract: LoopDef,
    pub vignette: Vec<VignetteDef>,
}

impl AttractDef {
    /// Every vignette's places, stations, protagonists, jumps and shots exist.
    pub fn check(&self, city: &pav_world::City) -> Result<()> {
        let crew = crate::protagonists::def()?;
        let radio = crate::radio::def()?;
        let jumps = crate::activities::def()?;
        if self.vignette.is_empty() {
            return Err(anyhow!("attract.toml: no vignettes"));
        }
        for v in &self.vignette {
            let at = |what: &str, a: &str| -> Result<()> {
                let anchor = crate::anchor::Anchor::parse(a).map_err(|e| anyhow!("attract.toml: {}: {what}: {e}", v.id))?;
                if !anchor.exists(city) {
                    return Err(anyhow!("attract.toml: {}: {what} '{a}' is not in the district", v.id));
                }
                Ok(())
            };
            if let Some(a) = &v.at {
                at("at", a)?;
            }
            if let Some(a) = &v.goal {
                at("goal", a)?;
            }
            if let Some(a) = &v.toward {
                at("toward", a)?;
            }
            if crew.index(&v.protagonist).is_none() {
                return Err(anyhow!("attract.toml: {}: no protagonist '{}'", v.id, v.protagonist));
            }
            if let Some(r) = &v.radio
                && !radio.station.iter().any(|s| &s.id == r)
            {
                return Err(anyhow!("attract.toml: {}: no station '{r}'", v.id));
            }
            if let Some(k) = &v.vehicle
                && crate::vehicles::get(k).is_err()
            {
                return Err(anyhow!("attract.toml: {}: no vehicle '{k}'", v.id));
            }
            if v.bot == BotKind::Stunt && !v.stunt.as_ref().is_some_and(|s| jumps.jump.iter().any(|j| &j.id == s)) {
                return Err(anyhow!("attract.toml: {}: a stunt vignette names one of the jumps", v.id));
            }
            if crate::daytime::parse_time(&v.time).is_none() {
                return Err(anyhow!("attract.toml: {}: time '{}' is not HH:MM", v.id, v.time));
            }
            if let Some(s) = v.shots.iter().find(|s| !matches!(s.as_str(), "chase" | "cine" | "drone")) {
                return Err(anyhow!("attract.toml: {}: shot '{s}' (chase, cine or drone)", v.id));
            }
            if !(20.0..=90.0).contains(&v.seconds) {
                return Err(anyhow!("attract.toml: {}: {} s (vignettes run 20-90 s)", v.id, v.seconds));
            }
        }
        Ok(())
    }
}

pub fn def() -> Result<Arc<AttractDef>> {
    static CACHE: Mutex<Option<(String, Arc<AttractDef>)>> = Mutex::new(None);
    let text = crate::content::text("attract.toml", include_str!("../../../content/freeway/attract.toml"));
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((t, d)) = cache.as_ref()
        && *t == *text
    {
        return Ok(d.clone());
    }
    let d: AttractDef = toml::from_str(&text).map_err(|e| anyhow!("content/freeway/attract.toml: {e}"))?;
    let d = Arc::new(d);
    *cache = Some((text.into_owned(), d.clone()));
    Ok(d)
}

/// The loop's state (part of the game's).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Attract {
    /// Playing itself.
    pub on: bool,
    /// Never leaves (touch screens).
    pub kiosk: bool,
    /// This loop's order (vignette indices) and where in it.
    pub order: Vec<usize>,
    pub at: usize,
    /// Seconds into the vignette (game time), and of the cut before it still to go (real).
    pub t: f32,
    pub cut: f32,
    /// Its world is set up.
    pub ready: bool,
    /// Seconds without headway (a stalled bot), from where.
    pub still: f32,
    /// Seconds since the last foe of a fight fell, or since a chase was shaken off (the vignette
    /// lingers on the aftermath).
    pub quiet: f32,
    pub anchor: Vec3,
    /// Its next switch (the switch vignette), in seconds into it.
    pub next_switch: f32,
    /// Vignettes played, loops, and those that ended early (`id: why`).
    pub played: u32,
    pub loops: u32,
    pub anomalies: Vec<String>,
    /// The game as it stood (a save), put back on the way out.
    pub kept: Option<String>,
    /// The bot for the vignettes on foot.
    pub bot: MissionBot,
    /// The last tick had a touch (the browser checks see each new one logged).
    pub touched: bool,
    /// The touch gear's overlay: open, its focus, and About open over it (with its page).
    pub gear: bool,
    pub gear_focus: usize,
    pub gear_about: bool,
    pub about_page: usize,
}

/// The touch overlay's rows.
const GEAR: [&str; 5] = ["sound", "quality", "fps", "about", "close"];

/// Input from a device (a key, the mouse, a pad, a touch, a click on the UI) — not a command a
/// tool or a test typed, which the tools send as activity too.
fn device_input(f: &InputFrame) -> bool {
    let typed = !f.cmds.is_empty()
        && f.cmds.iter().all(|c| !c.starts_with("ui."))
        && f.pad.held == 0
        && f.pad.pressed == 0
        && f.held == 0
        && f.pressed == 0
        && f.move_dir == Vec2::ZERO
        && f.mouse_delta == Vec2::ZERO;
    f.activity && !typed
}

impl Freeway {
    /// The vignette under way.
    fn vignette(&self) -> Option<VignetteDef> {
        if !self.attract.on {
            return None;
        }
        let d = def().ok()?;
        let i = *self.attract.order.get(self.attract.at)?;
        d.vignette.get(i).cloned()
    }

    /// The id of the vignette under way (or about to be set up).
    pub fn attract_vignette(&self) -> Option<String> {
        self.vignette().map(|v| v.id)
    }

    /// `attract on|kiosk|off|play <vignette>` (tests and tools): start the loop (as a touch screen
    /// would: never leaving it), leave it, or play a vignette next, at once.
    pub(crate) fn attract_cmds(&mut self, sim: &mut Sim, input: &InputFrame) {
        for c in &input.cmds {
            let mut w = c.split_whitespace();
            if w.next() != Some("attract") {
                continue;
            }
            match (w.next(), w.next()) {
                (Some("on"), _) => self.attract_start(sim, false),
                (Some("kiosk"), _) => self.attract_start(sim, true),
                (Some("off"), _) if self.attract.on => self.attract_stop(sim),
                (Some("play"), Some(id)) => {
                    let Some(i) = def().ok().and_then(|d| d.vignette.iter().position(|v| v.id == id)) else {
                        log::warn!("attract: no vignette '{id}'");
                        continue;
                    };
                    self.attract_start(sim, false);
                    let at = self.attract.at.min(self.attract.order.len().saturating_sub(1));
                    if let Some(k) = self.attract.order.iter().position(|x| *x == i) {
                        self.attract.order.swap(k, at);
                    }
                    self.attract.at = at;
                    self.attract.ready = false;
                    self.attract.cut = 0.0;
                }
                _ => log::warn!("attract: on, kiosk, off or play <vignette>"),
            }
        }
    }

    /// Starts the loop (`kiosk`: never leaving it).
    pub(crate) fn attract_start(&mut self, sim: &mut Sim, kiosk: bool) {
        if self.attract.on {
            return;
        }
        let kept = serde_json::to_string(&self.save_data(sim)).ok();
        self.attract = Attract { on: true, kiosk, kept, ..Default::default() };
        self.attract_shuffle(sim);
        self.front.go(Page::Attract, self.ui_time);
        self.front.attract_due = false;
    }

    /// A new order for the loop (the last one played never first again).
    fn attract_shuffle(&mut self, sim: &mut Sim) {
        let n = def().map(|d| d.vignette.len()).unwrap_or(0);
        let last = self.attract.order.get(self.attract.at.saturating_sub(1)).copied();
        let mut order: Vec<usize> = (0..n).collect();
        for i in (1..order.len()).rev() {
            let j = sim.state.rng.below(i as u32 + 1) as usize;
            order.swap(i, j);
        }
        if n > 1 && order.first() == last.as_ref() {
            order.swap(0, 1);
        }
        self.attract.order = order;
        self.attract.at = 0;
        self.attract.cut = def().map(|d| d.attract.cut).unwrap_or(1.0);
        self.attract.ready = false;
    }

    /// Leaves the loop for the title, the game as it stood.
    pub(crate) fn attract_stop(&mut self, sim: &mut Sim) {
        let kept = self.attract.kept.take();
        self.attract = Attract::default();
        self.clear_world(sim);
        if let Some(json) = kept
            && let Err(e) = self.load_game(sim, &json)
        {
            log::warn!("attract: the game kept on the way in: {e:#}");
        }
        self.front.go(Page::Title, self.ui_time);
        self.front.idle = 0.0;
        self.front.attract_due = false;
    }

    /// The world emptied for a vignette (or the way out): the population, the police, a
    /// mission's or an activity's people, wrecks and the crew's bodies go; nothing sleeps
    /// waiting for its ground to come back (so a loop that runs for ever never grows).
    pub(crate) fn clear_world(&mut self, sim: &mut Sim) {
        if self.missions.run.is_some() {
            self.end_mission(sim);
        }
        self.street_end(sim);
        self.activities.jumps.run = None;
        self.activities.jumps.flight = None;
        self.activities.jumps.banner = None;
        self.notes.clear();
        self.enemies.clear(sim);
        self.police.clear(sim);
        self.heat.clear();
        self.bot.stop();
        self.bot.escape = false;
        if let Some(city) = self.city.clone() {
            self.traffic.autopilot(sim, &city, None);
        }
        let player = sim.state.player;
        if let (Some(p), Some(car)) = (player, self.drive.vehicle()) {
            sim.exit_vehicle(p, car);
        }
        self.drive = Default::default();
        let gone: Vec<EntityId> = sim
            .state
            .entities
            .iter()
            .filter(|e| Some(e.id) != player && (e.vehicle.is_some() || e.character.is_some()))
            .map(|e| e.id)
            .collect();
        for id in gone {
            sim.despawn(id);
        }
        sim.state.world.dormant_entities.clear();
        // (Traffic and the crowds fill in again round the new place; their counters run on.)
        let t = &mut self.traffic;
        t.cars.clear();
        t.parked.clear();
        t.lots.clear();
        t.loose.clear();
        t.crashed.clear();
        t.filled = false;
        t.thin = None;
        t.pilot_input = None;
        t.escape = None;
        t.escape_planned = None;
        self.peds.list.clear();
        self.struck = Default::default();
        self.crew.bodies.clear();
        self.crew.switch = None;
        self.crew.wheel = None;
        self.cover = Default::default();
        self.wheel = None;
        self.radio_wheel = None;
        self.phone.open = false;
        self.shop = None;
        self.fate = Default::default();
        self.combat.health = self.combat.health.max(100.0);
    }

    /// Sets the vignette up: the clock, the weather, who, where, in what, the radio, the stars,
    /// its commands and its bot.
    fn attract_setup(&mut self, sim: &mut Sim, v: &VignetteDef, events: &mut Vec<SimEvent>) -> Vec<String> {
        self.clear_world(sim);
        let mut cmds = vec![format!("time {}", v.time), format!("weather {}", v.weather), "clock 1".into()];
        if let Ok(crew) = crate::protagonists::def()
            && let Some(i) = crew.index(&v.protagonist)
        {
            self.stash_member(sim);
            self.load_member(i);
            self.dress_player(sim);
        }
        let city = self.city.clone();
        let place = v.at.as_deref().and_then(|a| crate::anchor::Anchor::parse(a).ok()).and_then(|a| {
            let c = city.as_ref()?;
            if v.vehicle.is_some() { crate::anchor::car(c, &a) } else { crate::anchor::foot(c, &a) }
        });
        if let Some((p, yaw)) = place {
            self.put_player(sim, p + Vec3::Y * 0.05, yaw);
        }
        if let Some(k) = &v.vehicle {
            self.car_cmd(sim, k, None, true);
            if let (Some(car), Some((p, yaw))) = (self.drive.vehicle(), place) {
                sim.place_vehicle(car, p, yaw);
                self.chase.place_behind(yaw);
            }
        }
        if let Some(r) = &v.radio {
            cmds.push(format!("radio {r}"));
        }
        if v.heat > 0 {
            cmds.push(format!("wanted {}", v.heat));
        }
        match v.bot {
            BotKind::Cruise | BotKind::Switch => cmds.push("autopilot on".into()),
            BotKind::Escape => cmds.push("autopilot escape".into()),
            BotKind::Stunt => cmds.extend(v.stunt.iter().map(|s| format!("stunt {s}"))),
            BotKind::Stroll | BotKind::Fight | BotKind::Mission => {}
        }
        // (No random event by chance, nor a mission picked up on the way: each vignette is its own.)
        cmds.push("events off".into());
        cmds.extend(v.cmds.iter().cloned());
        let goal = v
            .goal
            .as_deref()
            .or(v.toward.as_deref())
            .and_then(|a| crate::anchor::Anchor::parse(a).ok())
            .and_then(|a| crate::anchor::point(city.as_ref()?, &a));
        if let (Some(g), BotKind::Cruise) = (goal, v.bot) {
            cmds.push(format!("pilot_goal {:.1} {:.1} 1.0 {:.1}", g.x, g.z, g.y));
        }
        self.attract.bot = MissionBot::default();
        self.attract.next_switch = v.switch_every.unwrap_or(f32::MAX);
        self.attract.anchor = sim.state.player.and_then(|id| sim.feet(id)).unwrap_or(Vec3::ZERO);
        self.attract.still = 0.0;
        self.attract.quiet = 0.0;
        events.push(SimEvent::Sound { cue: "ui.tab".into(), pos: None, gain: 0.6, pitch: 0.8 });
        // (Browser checks wait for this line, PLAN §7.4.)
        log::info!("frontend: attract {}", v.id);
        cmds
    }

    /// The loop's tick, before the game's: leaves on any input, runs the cut, sets vignettes
    /// up and moves on from them. Returns the frame the game plays this tick in place of the
    /// host's (None: the loop is over, or not on).
    pub(crate) fn attract_input(
        &mut self,
        sim: &mut Sim,
        input: &InputFrame,
        dt: f32,
        real: f32,
        events: &mut Vec<SimEvent>,
    ) -> Option<InputFrame> {
        if !self.attract.on {
            return None;
        }
        if self.attract.kiosk {
            if input.activity && !self.attract.touched {
                log::info!("attract: a touch; the loop plays on");
            }
            self.attract.touched = input.activity;
            self.gear_input(input);
        } else if device_input(input) {
            // Back to the title; the input that ended the loop does nothing else.
            self.attract_stop(sim);
            return Some(InputFrame::default());
        }
        let Ok(d) = def() else {
            self.attract_stop(sim);
            return None;
        };
        let mut frame = InputFrame::default();
        if !self.attract.ready {
            self.attract.cut -= real;
            if self.attract.cut > 0.0 {
                return Some(frame);
            }
            if let Some(v) = self.vignette() {
                frame.cmds = self.attract_setup(sim, &v, events);
            }
            self.attract.ready = true;
            self.attract.t = 0.0;
            return Some(frame);
        }
        let Some(v) = self.vignette() else {
            self.attract_shuffle(sim);
            return Some(frame);
        };
        self.attract.t += dt;
        // Getting somewhere? (A fight is getting somewhere while shots are fired.)
        let feet = sim.state.player.and_then(|id| sim.feet(id)).unwrap_or(self.attract.anchor);
        let busy = self.crew.switch.is_some()
            || self.traffic.pilot_waiting()
            // (A random event's own bot plays it: it waits for the thief, hands the bag back.)
            || (v.bot == BotKind::Stroll && self.street_busy())
            || !self.fate.alive()
            || self.activities.jumps.flight.is_some()
            || v.bot == BotKind::Switch
            || (v.bot == BotKind::Mission && self.mission_holds_controls());
        let fighting = matches!(v.bot, BotKind::Fight | BotKind::Mission) && self.combat.aiming;
        // (A fight won or a chase shaken off lingers on the aftermath, still on purpose.)
        let aftermath = self.attract.quiet > 0.0;
        if feet.distance(self.attract.anchor) > 4.0 || busy || fighting || aftermath {
            self.attract.anchor = feet;
            self.attract.still = 0.0;
        } else {
            self.attract.still += dt;
        }
        // The switch vignette's switches, in turn.
        if v.bot == BotKind::Switch && self.attract.t >= self.attract.next_switch {
            self.attract.next_switch += v.switch_every.unwrap_or(f32::MAX);
            if let Ok(crew) = crate::protagonists::def()
                && let Some(next) = crew.protagonist.get((self.crew.current + 1) % crew.protagonist.len())
            {
                frame.cmds.push(format!("switch {}", next.key));
            }
        }
        // Over: its time is up, its goal reached, the stunt run, the fight or the mission done,
        // the protagonist wasted or busted — or the bot stalled, or a cruise missed its goal (an
        // anomaly).
        let goal = self.attract_goal(&v);
        let reach = if self.drive.vehicle().is_some() { 14.0 } else { 3.5 };
        let at_goal = goal.is_some_and(|g| Vec2::new(g.x - feet.x, g.z - feet.z).length() < reach);
        let stunt_done = v.bot == BotKind::Stunt && self.attract.t > 3.0 && self.activities.jumps.run.is_none();
        let fight_over = v.bot == BotKind::Fight && !self.enemies.list.iter().any(|e| self.foe_up(sim, e.id));
        let chase_over = v.bot == BotKind::Escape && self.heat.stars == 0;
        if self.attract.t > 5.0 && (fight_over || chase_over) {
            self.attract.quiet += dt;
        } else {
            self.attract.quiet = 0.0;
        }
        let fight_won = self.attract.quiet > 5.0;
        let mission_over = v.bot == BotKind::Mission
            && self.attract.t > 3.0
            && self.missions.run.as_ref().is_none_or(|r| !matches!(r.phase, crate::missions::Phase::Play));
        let fallen = !self.fate.alive() && self.fate.since() > 3.0;
        // A chase can also be lost: the protagonist out of the car (bailed or thrown), or the
        // car boxed in by the police for a few seconds — the director cuts away, as a stalled
        // shot would be cut; only a stall with nobody to blame is an anomaly.
        let police_near = self
            .police
            .units
            .iter()
            .filter_map(|u| sim.state.entities.get(u.vehicle).map(|e| e.pos))
            .chain(self.police.officers.iter().filter_map(|o| sim.state.entities.get(o.id).map(|e| e.pos)))
            .any(|p| p.distance(feet) < 30.0);
        let chase_lost = v.bot == BotKind::Escape
            && self.attract.t > 5.0
            && ((self.drive.vehicle().is_none() && !self.drive.busy()) || (self.attract.still > 6.0 && police_near));
        let stalled = self.attract.still > v.stall.unwrap_or(d.attract.stall) && !chase_lost;
        let missed = self.attract.t >= v.seconds && v.bot == BotKind::Cruise && goal.is_some() && !at_goal;
        if self.attract.t >= v.seconds || at_goal || stunt_done || fight_won || mission_over || fallen || chase_lost || stalled {
            if stalled || missed {
                let why = if stalled { "stalled" } else { "missed its goal" };
                // Where, and how (on foot or driving, the stars): what a fix starts from.
                let how = match self.drive.vehicle() {
                    Some(car) => format!("driving {}", sim.state.entities.get(car).map_or("?", |e| e.name.as_str())),
                    None => "on foot".to_string(),
                };
                let what = format!(
                    "{}: {why} at {:.0} s ({how} at {:.0},{:.0}, {}★)",
                    v.id, self.attract.t, feet.x, feet.z, self.heat.stars
                );
                log::warn!("attract: {what}");
                self.attract.anomalies.push(what);
            }
            self.attract.played += 1;
            self.attract.at += 1;
            self.attract.ready = false;
            self.attract.cut = d.attract.cut;
            if self.attract.at >= self.attract.order.len() {
                self.attract.loops += 1;
                self.attract_shuffle(sim);
            }
            return Some(frame);
        }
        // The bot's presses (the autopilot's, the evasion bot's, the stunt run's; on foot, the
        // loop's own walker and fighter).
        let mut f = pav_core::games::Game::bot(&*self, sim);
        if matches!(v.bot, BotKind::Stroll | BotKind::Fight) && !self.street_busy() {
            self.attract_walk(sim, &v, dt);
            f = self.attract.bot.frame.clone();
        }
        // The cinematic shots: ○ held, in a car.
        if self.attract_shot(&v, &d.attract) == "cine" && self.drive.vehicle().is_some() {
            f.pad.held |= pad::EAST;
        }
        f.cmds.extend(frame.cmds);
        f.activity = false;
        Some(f)
    }

    /// Whether a foe is still up (not a body; no health kept yet: unhurt).
    fn foe_up(&self, sim: &Sim, id: EntityId) -> bool {
        sim.state.entities.get(id).is_some_and(|e| !e.name.starts_with("dead:"))
            && self.combat.hp.get(&id.0).is_none_or(|h| *h > 0.0)
    }

    /// Where the vignette's goal is (its anchor in the district).
    fn attract_goal(&self, v: &VignetteDef) -> Option<Vec3> {
        let a = crate::anchor::Anchor::parse(v.goal.as_deref()?).ok()?;
        crate::anchor::point(self.city.as_ref()?, &a)
    }

    /// The shot for now from the vignette's list.
    fn attract_shot<'a>(&self, v: &'a VignetteDef, l: &LoopDef) -> &'a str {
        if v.shots.is_empty() {
            return "chase";
        }
        let i = (self.attract.t / l.shot_secs.max(1.0)) as usize % v.shots.len();
        &v.shots[i]
    }

    /// On foot: toward the goal (a stroll), or at the nearest foe from where one can be hit (a
    /// fight).
    fn attract_walk(&mut self, sim: &Sim, v: &VignetteDef, dt: f32) {
        let mut b = std::mem::take(&mut self.attract.bot);
        b.frame = InputFrame::default();
        b.tick = b.tick.wrapping_add(1);
        let me = sim.state.player.and_then(|id| sim.feet(id)).unwrap_or(Vec3::ZERO);
        match v.bot {
            BotKind::Fight => {
                let foe = self
                    .enemies
                    .list
                    .iter()
                    .filter_map(|e| Some((e.id, sim.feet(e.id)?)))
                    .filter(|(id, _)| self.foe_up(sim, *id))
                    .min_by(|a, c| a.1.distance(me).total_cmp(&c.1.distance(me)));
                if let Some((id, at)) = foe {
                    self.bot_shoot(&mut b, sim, id, at, dt);
                }
            }
            _ => {
                if let Some(g) = self.attract_goal(v)
                    && Vec2::new(g.x - me.x, g.z - me.z).length() > 3.0
                {
                    self.bot_walk(&mut b, sim, g, false, dt);
                }
            }
        }
        self.attract.bot = b;
    }

    /// The camera for a drone shot (None: the game's own).
    pub(crate) fn attract_camera(&self, sim: &Sim) -> Option<CameraIntent> {
        if !self.attract.on || !self.attract.ready {
            return None;
        }
        let (v, d) = (self.vignette()?, def().ok()?);
        (self.attract_shot(&v, &d.attract) == "drone").then(|| {
            let feet = sim.state.player.and_then(|id| sim.feet(id)).unwrap_or(Vec3::ZERO);
            let mut c = crate::camera::drone(feet, self.attract.t * 1.6, 16.0, 7.0);
            c.target = feet + Vec3::Y * 1.0;
            c.fov = 48.0;
            c.near = 0.2;
            // (Not from inside a wall: in front of whatever is in the way.)
            let d = c.eye - c.target;
            if let Some(h) = sim.raycast_static(c.target, d.normalize_or(Vec3::Y), d.length()) {
                c.eye = c.target + d.normalize_or(Vec3::Y) * (h.point.distance(c.target) - 0.4).max(2.0);
            }
            c
        })
    }

    /// The UI while the loop plays: the logo and the vignette's caption in the corner, "DEMO"
    /// and "PRESS ANY BUTTON" bottom right, the radar and stars in a chase; between vignettes
    /// the caption card over black. Touch screens: nothing but the host's gear (and its overlay
    /// when open).
    pub(crate) fn attract_ui(&self, ui: &mut UiModel) {
        if !self.attract.on {
            return;
        }
        let v = self.vignette();
        let hud = v.as_ref().is_some_and(|v| v.hud) && self.attract.ready && !self.attract.kiosk;
        match ui.hud.as_mut() {
            Some(h) if hud => {
                *h = Hud { minimap: h.minimap.take(), wanted: h.wanted.take(), opacity: h.opacity, ..Default::default() };
            }
            _ => ui.hud = None,
        }
        ui.help = None;
        ui.prompts.clear();
        ui.toasts.clear();
        ui.markers.clear();
        ui.wheel = None;
        ui.phone = None;
        ui.subtitle = None;
        if self.attract.kiosk {
            ui.screen = self.attract.gear.then(|| self.gear_screen());
            ui.banners.clear();
            return;
        }
        let caption = v.as_ref().map(|v| v.caption.clone()).unwrap_or_default();
        let title = "GRAND THEFT FREEWAY".to_string();
        // (The logo takes the top-left corner, where the FPS readout sits.)
        ui.perf = false;
        if !self.attract.ready {
            ui.overlay.fade = 1.0;
            ui.banners.clear();
            ui.screen = Some(Screen {
                id: "attract/cut".into(),
                title,
                tabs: Vec::new(),
                tab: 0,
                hints: Vec::new(),
                body: ScreenBody::Title(TitleCard { prompt: None, menu: None, caption: Some(caption), small_logo: false }),
                backdrop: Backdrop::Solid,
            });
            return;
        }
        // (In from black over the vignette's first moments, while cameras settle.)
        ui.overlay.fade = ui.overlay.fade.max(1.0 - self.attract.t / 0.6).clamp(0.0, 1.0);
        ui.screen = Some(Screen {
            id: "attract".into(),
            title,
            tabs: Vec::new(),
            tab: 0,
            hints: Vec::new(),
            body: ScreenBody::Title(TitleCard {
                // (Any button at all: no glyph for one of them.)
                prompt: Some("PRESS ANY BUTTON".into()),
                menu: None,
                caption: Some(format!("DEMO · {caption}")),
                small_logo: true,
            }),
            backdrop: Backdrop::Clear,
        });
    }

    // ---- the touch gear ----

    /// The gear's taps: the gear opens and closes the overlay (and leaves About for it), rows
    /// do what they say, About's tabs turn its pages.
    fn gear_input(&mut self, input: &InputFrame) {
        for c in &input.cmds {
            if c == "ui.gear" {
                if self.attract.gear_about {
                    self.attract.gear_about = false;
                } else {
                    self.attract.gear = !self.attract.gear;
                }
                self.attract.gear_focus = 0;
                log::info!("attract: gear overlay {}", if self.attract.gear { "open" } else { "closed" });
                continue;
            }
            if !self.attract.gear {
                continue;
            }
            let tab = c.strip_prefix("ui.accept tab:").or(c.strip_prefix("ui.focus tab:"));
            if let Some(n) = tab.and_then(|n| n.parse::<usize>().ok()) {
                self.attract.about_page = n % crate::about::PAGES.len();
                continue;
            }
            if let Some(i) = c.strip_prefix("ui.focus ").and_then(|id| GEAR.iter().position(|g| *g == id)) {
                self.attract.gear_focus = i;
            }
            let Some(id) = c.strip_prefix("ui.accept ") else { continue };
            if let Some(i) = GEAR.iter().position(|g| *g == id) {
                self.attract.gear_focus = i;
            }
            match id {
                "sound" => {
                    self.settings.master = if self.settings.master > 0.0 { 0.0 } else { 0.8 };
                    self.settings_changed();
                }
                "quality" => {
                    self.settings.adjust("quality", 1);
                    self.settings_changed();
                }
                "fps" => {
                    self.settings.fps = !self.settings.fps;
                    self.settings_changed();
                }
                "about" => {
                    self.attract.gear_about = true;
                    self.attract.about_page = 0;
                }
                "close" => self.attract.gear = false,
                _ => {}
            }
        }
    }

    fn gear_screen(&self) -> Screen {
        if self.attract.gear_about {
            let (heading, lines) = crate::about::page(self.attract.about_page);
            return Screen {
                id: "attract/about".into(),
                title: "ABOUT".into(),
                tabs: crate::about::PAGES.iter().map(|t| t.to_string()).collect(),
                tab: self.attract.about_page % crate::about::PAGES.len(),
                hints: Vec::new(),
                body: ScreenBody::Text(TextCard { heading, lines }),
                backdrop: Backdrop::Dim,
            };
        }
        let toggle = |id: &str, label: &str, on: bool| MenuItem {
            id: id.into(),
            label: label.into(),
            value: ItemValue::Toggle { on },
            enabled: true,
            note: None,
        };
        let button = |id: &str, label: &str| MenuItem {
            id: id.into(),
            label: label.into(),
            value: ItemValue::Button,
            enabled: true,
            note: None,
        };
        let items = vec![
            toggle(GEAR[0], "Sound", self.settings.master > 0.0),
            button(GEAR[1], &format!("Graphics: {}", self.settings.quality_name())),
            toggle(GEAR[2], "Frame rate readout", self.settings.fps),
            button(GEAR[3], "About"),
            button(GEAR[4], "Close"),
        ];
        Screen {
            id: "attract/gear".into(),
            title: "SETTINGS".into(),
            tabs: Vec::new(),
            tab: 0,
            hints: Vec::new(),
            body: ScreenBody::Menu(MenuList { items, focus: self.attract.gear_focus.min(4) }),
            backdrop: Backdrop::Dim,
        }
    }

    /// The loop for `status`.
    pub(crate) fn attract_status(&self) -> serde_json::Value {
        serde_json::json!({
            "on": self.attract.on,
            "kiosk": self.attract.kiosk,
            "vignette": self.vignette().map(|v| v.id),
            "t": (self.attract.t * 10.0).round() / 10.0,
            "played": self.attract.played,
            "loops": self.attract.loops,
            "anomalies": self.attract.anomalies,
        })
    }
}
