//! Pedestrians (PLAN.md §5.6.7, §6.6.8, §6.6.10): people on Paloma Bay's sidewalks, crossings,
//! paths, parks and beach. The population manager keeps the zone's and the hour's share of
//! `peds.toml`'s cap around the player, spawning people out of view 60–115 m away and letting the
//! far, unseen ones go; each person walks to a destination over the walk tiles (A* on the
//! `WalkMap`), waits at a signalled crossing for its walk light, and lingers a while (an idle
//! clip) before the next walk. Driven like the player, one `InputFrame` per tick; every choice
//! draws on the sim's rng, so a run replays exactly.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use glam::{Vec2, Vec3};
use pav_core::entity::EntityId;
use pav_core::input::buttons;
use pav_core::present::CameraIntent;
use pav_core::puppet::PuppetDef;
use pav_core::rng::Rng;
use pav_core::{InputFrame, Sim};
use pav_world::City;
use pav_world::walk::Walk;
use serde::{Deserialize, Serialize};

use crate::looks::{Body, Wear};

/// `peds.toml`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PedsDef {
    pub density: Density,
    /// The proportions everyone in the game is built on (`crate::looks::Body`).
    #[serde(default)]
    pub body: Body,
    /// SPPD officers on foot (`police::uniform`).
    #[serde(default)]
    pub police: Option<Officers>,
    pub archetype: Vec<Archetype>,
}

/// The SPPD's uniform: skin tones, the uniform's colours and what officers wear with it.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Officers {
    pub skin: Vec<String>,
    pub shirt: String,
    pub pants: String,
    pub shoes: String,
    /// Standing height (m) and build (1 = average).
    pub height: [f32; 2],
    pub build: [f32; 2],
    #[serde(default)]
    pub wear: Wear,
}

/// How many people, where and when.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Density {
    pub max: usize,
    pub radius: f32,
    pub spawn: [f32; 2],
    pub despawn: f32,
    /// The share of the day's crowd still out at 03:00.
    pub night: f32,
    /// Crowd by zone, a share of `max` (others: 0.3).
    #[serde(default)]
    pub zones: BTreeMap<String, f32>,
}

/// A kind of person: palettes and proportions to draw a look from, walks, idles, pace, places.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Archetype {
    pub id: String,
    pub weight: f32,
    #[serde(default)]
    pub zones: Vec<String>,
    pub skin: Vec<String>,
    pub shirt: Vec<String>,
    pub pants: Vec<String>,
    pub shoes: Vec<String>,
    /// Standing height (m) and build (1 = average).
    pub height: [f32; 2],
    pub build: [f32; 2],
    #[serde(default)]
    pub walks: Vec<String>,
    #[serde(default)]
    pub idles: Vec<String>,
    /// Speed (m/s): walking, or running when `run`.
    pub pace: [f32; 2],
    #[serde(default)]
    pub run: bool,
    /// Hair, beards, headwear and accessories (`crate::looks::Wear`).
    #[serde(default)]
    pub wear: Wear,
}

/// The game's pedestrians (cached; re-read when the file changes during native hot reload).
pub fn def() -> Result<Arc<PedsDef>> {
    static CACHE: Mutex<Option<(String, Arc<PedsDef>)>> = Mutex::new(None);
    let text = crate::content::text("peds.toml", include_str!("../../../content/freeway/peds.toml"));
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((t, d)) = cache.as_ref() {
        if *t == *text {
            return Ok(d.clone());
        }
    }
    let d: PedsDef = toml::from_str(&text).map_err(|e| anyhow!("content/freeway/peds.toml: {e}"))?;
    if d.archetype.is_empty() {
        return Err(anyhow!("content/freeway/peds.toml: no archetypes"));
    }
    for a in &d.archetype {
        if a.skin.is_empty() || a.shirt.is_empty() || a.pants.is_empty() || a.shoes.is_empty() {
            return Err(anyhow!("content/freeway/peds.toml: '{}' needs skin, shirt, pants and shoes", a.id));
        }
        a.wear.check().map_err(|e| anyhow!("content/freeway/peds.toml: '{}' wear: {e}", a.id))?;
    }
    if let Some(o) = &d.police {
        if o.skin.is_empty() {
            return Err(anyhow!("content/freeway/peds.toml: [police] needs skin"));
        }
        o.wear.check().map_err(|e| anyhow!("content/freeway/peds.toml: [police] wear: {e}"))?;
    }
    let d = Arc::new(d);
    *cache = Some((text.into_owned(), d.clone()));
    Ok(d)
}

/// The walking speed the movement tuning gives with FOCUS held, and the running speed.
const WALK: f32 = crate::player::WALK;
const RUN: f32 = crate::player::RUN;
/// People farther than this from the player go low detail (no character controller), and come
/// back to full detail nearer than `LOD_NEAR`.
const LOD_FAR: f32 = 30.0;
const LOD_NEAR: f32 = 24.0;

/// What a person is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Doing {
    Walk,
    /// At a crossing, waiting for the walk light.
    Wait,
    Linger,
    /// Running away from a threat (gunfire, a fight), or crouched where it could not get away.
    Flee,
    Cower,
    /// Sitting on a bench (a scenario point).
    Sit,
    /// Filming an incident on the phone (a witness).
    Film,
    /// Going for the player with fists (tough people who were hit).
    Fight,
    /// On the phone to the police about a crime (stopped where they fled to).
    Phone,
    /// Knocked down (struck by a car, a heavy blow): back up, then away.
    Down,
}

/// One person.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ped {
    pub id: EntityId,
    pub arch: u16,
    pub doing: Doing,
    /// Tile centres to walk through, and the next one.
    pub path: Vec<Vec2>,
    pub next: usize,
    /// Seconds left lingering; since the last progress check, and where it stood then.
    pub timer: f32,
    pub check_t: f32,
    pub check: Vec2,
    pub run: bool,
    /// What they film or fight (a point; the player for a fight), and the bench they head for.
    #[serde(default)]
    pub focus: Option<Vec3>,
    #[serde(default)]
    pub seat: Option<(Vec3, f32)>,
    /// The idle clip they had before sitting or filming (put back afterwards).
    #[serde(default)]
    pub idle: Option<String>,
    /// Seconds to the next punch.
    #[serde(default)]
    pub swing: f32,
}

/// Everyone out on foot (part of the game's state).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Peds {
    pub list: Vec<Ped>,
    /// Seconds to the next population pass.
    pub pass: f32,
    /// Counters for soaks (PLAN.md §7.5): spawned, despawned, stuck and rerouted, failed spawns.
    pub spawned: u32,
    pub despawned: u32,
    pub stuck: u32,
    pub misses: u32,
    /// The graphics quality's share of the crowds (none: all of them).
    #[serde(default)]
    pub crowd: Option<f32>,
}

/// What the population manager needs to know about the moment.
pub struct Moment<'a> {
    pub city: &'a City,
    pub player: Vec3,
    pub camera: CameraIntent,
    /// Game seconds (the signal clock) and the hour of the day.
    pub time: f32,
    pub hour: f32,
    pub dt: f32,
    /// The player's feet when on foot (fights go for them), and whether a gun is drawn on
    /// the people (they give up a fight and run).
    pub player_on_foot: Option<Vec3>,
    pub gun_out: bool,
}

/// What people did to the player this tick: punches landed (damage each).
#[derive(Clone, Debug, Default)]
pub struct Deeds {
    pub punches: Vec<f32>,
}

/// Clips for the reactions.
const SIT_IDLE: &str = "MESH2MOTION/Sitting_Idle";
const PHONE_IDLE: &str = "MESH2MOTION/Idle_TalkingPhone";
const COWER_IDLE: &str = "WORKSHOP/Cower";
const REPORT_IDLE: &str = "WORKSHOP/Phone_Report";
const PUNCHES: [&str; 2] = ["MESH2MOTION/Punch_Jab", "MESH2MOTION/Punch_Cross"];
const KNOCKED: &str = "MESH2MOTION/Hit_Knockback";
/// Archetypes that film incidents, and those that fight back when hit.
const FILMERS: [&str; 2] = ["influencer", "tourist"];
const FIGHTERS: [&str; 3] = ["worker", "security", "local"];

fn pick<'a>(v: &'a [String], rng: &mut Rng) -> &'a str {
    &v[rng.below(v.len() as u32) as usize]
}

/// Scales `p` (the game's biped) to stand `height` m tall and widens it by `build`.
pub fn shape(p: &mut PuppetDef, body: &Body, height: f32, build: f32) {
    p.scale *= height / body.height();
    p.torso_radius *= build;
    p.hip_width *= build;
    p.shoulder_width *= build.sqrt();
    p.limb_radius *= build.sqrt();
}

/// A look drawn from an archetype: the clothes' colours, height, build, walk and idle from
/// `rng`; hair and accessories from a fork of it (`crate::looks`).
pub fn look(a: &Archetype, rng: &mut Rng) -> PuppetDef {
    let body = crate::looks::body();
    let mut p = body.biped();
    p.skin = pick(&a.skin, rng).into();
    p.shirt = pick(&a.shirt, rng).into();
    p.pants = pick(&a.pants, rng).into();
    p.shoes = pick(&a.shoes, rng).into();
    let height = rng.range(a.height[0], a.height[1]);
    let build = rng.range(a.build[0], a.build[1]);
    shape(&mut p, &body, height, build);
    if !a.walks.is_empty() {
        p.walk_clip = pick(&a.walks, rng).into();
    }
    if !a.idles.is_empty() {
        p.idle_clip = pick(&a.idles, rng).into();
    }
    a.wear.dress(&mut p, rng);
    p
}

/// The day's crowd at `hour`: all of it in the afternoon, `night` of it in the small hours.
pub fn time_share(hour: f32, night: f32) -> f32 {
    let day = 0.5 + 0.5 * ((hour - 14.0) / 24.0 * std::f32::consts::TAU).cos();
    night + (1.0 - night) * day.sqrt()
}

/// Whether the camera sees a point (in front, within a wide cone, not far off).
fn seen(cam: &CameraIntent, p: Vec3) -> bool {
    let d = p - cam.eye;
    let forward = (cam.target - cam.eye).normalize_or(Vec3::NEG_Z);
    d.length() < 260.0 && d.normalize_or(Vec3::ZERO).dot(forward) > 0.45
}

impl Peds {
    /// People per tick: the population pass twice a second, then everyone's next step.
    pub fn tick(&mut self, sim: &mut Sim, m: &Moment, out: &mut BTreeMap<EntityId, InputFrame>) -> Deeds {
        let mut deeds = Deeds::default();
        let Ok(def) = def() else { return deeds };
        self.pass -= m.dt;
        if self.pass <= 0.0 {
            self.pass = 0.5;
            self.populate(sim, &def, m);
        }
        let here = Vec2::new(m.player.x, m.player.z);
        for i in 0..self.list.len() {
            let id = self.list[i].id;
            let Some(feet) = sim.feet(id) else { continue };
            let at = Vec2::new(feet.x, feet.z);
            // Low detail (E11) beyond 30 m, full detail again within 24 m: only the people near
            // the player need the character controller (bumping, stepping round props).
            if let Some(c) = sim.state.entities.get_mut(id).and_then(|e| e.character.as_mut()) {
                let d = at.distance(here);
                c.lod = if c.lod { d > LOD_NEAR } else { d > LOD_FAR };
            }
            let frame = self.step(i, sim, &def, m, at, &mut deeds);
            out.insert(id, frame);
        }
        deeds
    }

    /// Sets (or puts back) a person's idle clip.
    fn set_idle(sim: &mut Sim, p: &mut Ped, clip: Option<&str>) {
        let Some(c) = sim.state.entities.get_mut(p.id).and_then(|e| e.character.as_mut()) else { return };
        let Some(look) = c.puppet.as_deref() else { return };
        let idle = match clip {
            Some(clip) => {
                if p.idle.is_none() {
                    p.idle = Some(look.idle_clip.clone());
                }
                clip.to_string()
            }
            None => match p.idle.take() {
                Some(back) => back,
                None => return,
            },
        };
        c.puppet = Some(Arc::new(PuppetDef { idle_clip: idle, ..look.clone() }));
    }

    /// Faces a point (turning in place).
    fn face(sim: &mut Sim, id: EntityId, from: Vec2, to: Vec3) {
        if let Some(c) = sim.state.entities.get_mut(id).and_then(|e| e.character.as_mut()) {
            let d = Vec2::new(to.x, to.z) - from;
            if d.length() > 0.1 {
                c.face = Some(d.x.atan2(d.y));
            }
        }
    }

    /// Lets a person go back to walking about (clearing any held facing and clip).
    fn release(sim: &mut Sim, p: &mut Ped, def: &PedsDef) {
        Self::set_idle(sim, p, None);
        if let Some(c) = sim.state.entities.get_mut(p.id).and_then(|e| e.character.as_mut()) {
            c.face = None;
        }
        p.focus = None;
        p.seat = None;
        p.doing = Doing::Linger;
        p.timer = 0.5;
        p.run = def.archetype[p.arch as usize].run;
    }

    /// The reactions' steps: sitting, filming, fighting, getting up. None: walk as usual.
    fn react(&mut self, i: usize, sim: &mut Sim, def: &PedsDef, m: &Moment, pos: Vec2, deeds: &mut Deeds) -> Option<InputFrame> {
        let walk = &m.city.walk;
        let p = &mut self.list[i];
        match p.doing {
            Doing::Sit => {
                p.timer -= m.dt;
                if p.timer <= 0.0 {
                    Self::release(sim, p, def);
                }
                Some(InputFrame::default())
            }
            Doing::Film => {
                p.timer -= m.dt;
                if let Some(f) = p.focus {
                    Self::face(sim, p.id, pos, f);
                }
                if p.timer <= 0.0 {
                    Self::release(sim, p, def);
                }
                Some(InputFrame::default())
            }
            // The game ends the call (`phoning`); a fallback in case it does not.
            Doing::Phone => {
                p.timer -= m.dt;
                if p.timer <= 0.0 {
                    Self::release(sim, p, def);
                }
                Some(InputFrame::default())
            }
            Doing::Down => {
                p.timer -= m.dt;
                if p.timer <= 0.0 {
                    let at = p.focus.unwrap_or(Vec3::new(pos.x, 0.0, pos.y));
                    p.doing = Doing::Walk;
                    self.scare_one(i, sim, walk, at);
                }
                Some(InputFrame::default())
            }
            Doing::Fight => {
                p.timer -= m.dt;
                p.swing -= m.dt;
                let Some(target) = m.player_on_foot else {
                    Self::release(sim, p, def);
                    return Some(InputFrame::default());
                };
                let to = Vec2::new(target.x, target.z) - pos;
                if p.timer <= 0.0 || to.length() > 25.0 || m.gun_out {
                    // Gives up (or a gun came out): away from the player.
                    p.doing = Doing::Walk;
                    self.scare_one(i, sim, walk, target);
                    return Some(InputFrame::default());
                }
                if to.length() < 1.3 {
                    Self::face(sim, p.id, pos, target);
                    if p.swing <= 0.0 {
                        p.swing = 1.1;
                        let clip = pav_core::clips::find_cached(PUNCHES[sim.state.rng.below(2) as usize]);
                        if let Some(c) = sim.state.entities.get_mut(p.id).and_then(|e| e.character.as_mut())
                            && clip != 0
                        {
                            c.anim.replay_clip(clip, pav_core::clips::ONCE, 1.0);
                        }
                        deeds.punches.push(8.0);
                    }
                    return Some(InputFrame::default());
                }
                if let Some(c) = sim.state.entities.get_mut(p.id).and_then(|e| e.character.as_mut()) {
                    c.face = None;
                }
                Some(InputFrame { move_dir: to.normalize_or(Vec2::ZERO), ..Default::default() })
            }
            _ => None,
        }
    }

    /// One person runs from `at` (after a fight, getting up).
    fn scare_one(&mut self, i: usize, sim: &mut Sim, walk: &pav_world::walk::WalkMap, at: Vec3) {
        let mut one = Peds { list: vec![self.list[i].clone()], ..Default::default() };
        one.list[0].doing = Doing::Walk;
        one.scare(sim, walk, at, f32::MAX);
        self.list[i] = one.list.pop().unwrap();
    }

    /// Someone took a hit from the player at `at`: tough people fight back, the rest run.
    pub fn assaulted(&mut self, sim: &mut Sim, walk: &pav_world::walk::WalkMap, who: EntityId, at: Vec3) {
        let Ok(def) = def() else { return };
        let Some(i) = self.list.iter().position(|p| p.id == who) else { return };
        let arch = def.archetype[self.list[i].arch as usize].id.clone();
        if FIGHTERS.contains(&arch.as_str()) && sim.state.rng.f32() < 0.6 {
            let p = &mut self.list[i];
            Self::set_idle(sim, p, None);
            p.doing = Doing::Fight;
            p.timer = 12.0;
            p.swing = 0.6;
            p.run = true;
        } else {
            self.scare_one(i, sim, walk, at);
        }
    }

    /// Someone was knocked down (struck by a car): on the ground a moment, then away from `at`.
    pub fn knocked_down(&mut self, sim: &mut Sim, who: EntityId, at: Vec3) {
        let Some(i) = self.list.iter().position(|p| p.id == who) else { return };
        let p = &mut self.list[i];
        Self::set_idle(sim, p, None);
        p.doing = Doing::Down;
        p.timer = 2.4;
        p.focus = Some(at);
        let clip = pav_core::clips::find_cached(KNOCKED);
        if let Some(c) = sim.state.entities.get_mut(who).and_then(|e| e.character.as_mut())
            && clip != 0
        {
            c.anim.replay_clip(clip, pav_core::clips::ONCE, 1.0);
        }
    }

    /// An incident at `at` (a crash, a fight, a shooting heard from afar): people who film
    /// things stop to film it from where they are (witnesses), within `radius`.
    pub fn incident(&mut self, sim: &mut Sim, at: Vec3, radius: f32) {
        let Ok(def) = def() else { return };
        for i in 0..self.list.len() {
            let p = &self.list[i];
            if !matches!(p.doing, Doing::Walk | Doing::Wait | Doing::Linger) {
                continue;
            }
            let Some(feet) = sim.feet(p.id) else { continue };
            let d = feet.distance(at);
            if d > radius || d < 6.0 {
                continue;
            }
            let arch = def.archetype[p.arch as usize].id.as_str();
            if !FILMERS.contains(&arch) || sim.state.rng.f32() > 0.7 {
                continue;
            }
            let p = &mut self.list[i];
            p.doing = Doing::Film;
            p.timer = sim.state.rng.range(7.0, 12.0);
            p.focus = Some(at);
            Self::set_idle(sim, p, Some(PHONE_IDLE));
        }
    }

    /// People filming right now (witnesses for heat).
    pub fn filming(&self) -> impl Iterator<Item = &Ped> {
        self.list.iter().filter(|p| p.doing == Doing::Film)
    }

    /// Those in `callers` stop and talk to the police on the phone; anyone on the phone who is
    /// not among them any more hangs up and carries on.
    pub fn phoning(&mut self, sim: &mut Sim, callers: &[EntityId]) {
        let Ok(def) = def() else { return };
        for p in &mut self.list {
            let calling = callers.contains(&p.id);
            match (p.doing, calling) {
                (Doing::Phone, false) => Self::release(sim, p, &def),
                (Doing::Walk | Doing::Wait | Doing::Linger | Doing::Flee, true) => {
                    p.doing = Doing::Phone;
                    p.timer = 6.0;
                    p.path.clear();
                    Self::set_idle(sim, p, Some(REPORT_IDLE));
                }
                _ => {}
            }
        }
    }

    /// A bench within 30 m nobody else sits on: (where to sit, facing).
    fn bench(&self, city: &City, at: Vec3) -> Option<(Vec3, f32)> {
        let taken: Vec<Vec3> = self.list.iter().filter_map(|p| p.seat.map(|s| s.0)).collect();
        city.props_near(at, 30.0).into_iter().filter(|s| s.asset.ends_with("/bench")).find_map(|s| {
            let fwd = s.rot * Vec3::Z;
            let spot = s.pos + fwd * 0.25;
            (!taken.iter().any(|t| t.distance(spot) < 0.8)).then(|| (spot, fwd.x.atan2(fwd.z)))
        })
    }

    /// One person's next move.
    fn step(&mut self, i: usize, sim: &mut Sim, def: &PedsDef, m: &Moment, pos: Vec2, deeds: &mut Deeds) -> InputFrame {
        if let Some(f) = self.react(i, sim, def, m, pos, deeds) {
            return f;
        }
        // At a bench they were walking to: sit.
        if let Some((spot, facing)) = self.list[i].seat
            && self.list[i].doing == Doing::Walk
            && Vec2::new(spot.x, spot.z).distance(pos) < 0.9
        {
            let p = &mut self.list[i];
            sim.set_position(p.id, spot + Vec3::Y * 0.02);
            if let Some(c) = sim.state.entities.get_mut(p.id).and_then(|e| e.character.as_mut()) {
                c.face = Some(facing);
            }
            Self::set_idle(sim, p, Some(SIT_IDLE));
            p.doing = Doing::Sit;
            p.timer = sim.state.rng.range(10.0, 25.0);
            return InputFrame::default();
        }
        let walk = &m.city.walk;
        let p = &mut self.list[i];
        if p.doing == Doing::Cower {
            p.timer -= m.dt;
            if p.timer <= 0.0 {
                p.doing = Doing::Linger;
                p.timer = 1.0;
                p.run = def.archetype[p.arch as usize].run;
                Self::set_idle(sim, p, None);
            }
            // The cower clip is a crouch of its own; without it, a plain crouch.
            let held = if pav_core::clips::find_cached(COWER_IDLE) != 0 { 0 } else { buttons::CROUCH };
            return InputFrame { held, ..Default::default() };
        }
        if p.doing == Doing::Linger {
            p.timer -= m.dt;
            if p.timer > 0.0 {
                return InputFrame::default();
            }
            // Sometimes a bench nearby instead of a walk somewhere.
            let sit = sim.state.rng.f32() < 0.3;
            let feet = Vec3::new(pos.x, m.city.ground_y(pos), pos.y);
            let bench = if sit { self.bench(m.city, feet) } else { None };
            let p = &mut self.list[i];
            // Knocked off the pavement (into the road): back onto the nearest walkable tile first.
            let from = if walk.walkable(walk.tile_of(pos)) { pos } else { walk.nearest_walkable(pos, 8).unwrap_or(pos) };
            let walk_path = |to: Vec2, budget: usize| {
                let mut path = walk.path(from, to, budget);
                if from != pos && !path.is_empty() {
                    path.insert(0, pos);
                }
                path
            };
            if let Some((spot, facing)) = bench {
                let path = walk_path(Vec2::new(spot.x, spot.z), 3000);
                if path.len() >= 2 {
                    p.path = path;
                    p.next = 1;
                    p.doing = Doing::Walk;
                    p.seat = Some((spot, facing));
                    p.check = pos;
                    p.check_t = 0.0;
                    return InputFrame::default();
                }
            }
            let to = destination(walk, from, &mut sim.state.rng);
            match to.map(|to| walk_path(to, 6000)) {
                Some(path) if path.len() >= 2 => {
                    p.path = path;
                    p.next = 1;
                    p.doing = Doing::Walk;
                    p.check = pos;
                    p.check_t = 0.0;
                }
                _ => p.timer = 2.0,
            }
            return InputFrame::default();
        }
        while p.next < p.path.len() && p.path[p.next].distance(pos) < 0.7 {
            p.next += 1;
        }
        if p.next >= p.path.len() {
            if let Some((spot, _)) = p.seat {
                // The walk ends at the bench: the last step onto it.
                let to = Vec2::new(spot.x, spot.z) - pos;
                if to.length() < 2.5 {
                    return InputFrame { move_dir: to.normalize_or(Vec2::ZERO), held: buttons::FOCUS, ..Default::default() };
                }
                p.seat = None;
            }
            let a = &def.archetype[p.arch as usize];
            // A flight ends in a breather, then the walk resumes at the usual pace.
            p.run = a.run;
            p.doing = Doing::Linger;
            p.timer = sim.state.rng.range(4.0, 16.0) * if a.run { 0.3 } else { 1.0 };
            return InputFrame::default();
        }
        let w = p.path[p.next];
        let fleeing = p.doing == Doing::Flee;
        // A signalled crossing ahead: step off the kerb only on the walk light (unless fleeing).
        if let (Walk::Crossing { junction, ns }, false, false) =
            (walk.at(w), matches!(walk.at(pos), Walk::Crossing { .. }), fleeing)
        {
            let plan = m.city.lanes.junctions.get(junction as usize).and_then(|j| j.signal.as_ref());
            if plan.is_some_and(|s| s.walk(ns, m.time).is_none()) {
                p.doing = Doing::Wait;
                p.check = pos;
                p.check_t = 0.0;
                return InputFrame::default();
            }
        }
        if !fleeing {
            p.doing = Doing::Walk;
        }
        // Stuck: little progress for four seconds means a new destination.
        p.check_t += m.dt;
        if p.check_t > 4.0 {
            if p.check.distance(pos) < 0.4 {
                self.stuck += 1;
                let p = &mut self.list[i];
                p.doing = Doing::Linger;
                p.timer = 0.5;
                return InputFrame::default();
            }
            p.check = pos;
            p.check_t = 0.0;
        }
        let dir = (w - pos).normalize_or(Vec2::ZERO);
        let held = if p.run { 0 } else { buttons::FOCUS };
        InputFrame { move_dir: dir, held, ..Default::default() }
    }

    /// The population pass: drop people who are gone or far and unseen, then spawn toward the
    /// zone's and hour's share of the cap.
    fn populate(&mut self, sim: &mut Sim, def: &PedsDef, m: &Moment) {
        let d = &def.density;
        let here = Vec2::new(m.player.x, m.player.z);
        let mut kept = Vec::with_capacity(self.list.len());
        for p in std::mem::take(&mut self.list) {
            let Some(feet) = sim.feet(p.id) else { continue };
            let far = Vec2::new(feet.x, feet.z).distance(here) > d.despawn;
            if far && !seen(&m.camera, feet + Vec3::Y) {
                sim.despawn(p.id);
                self.despawned += 1;
            } else {
                kept.push(p);
            }
        }
        self.list = kept;
        let share = area_share(m.city, here, d) * time_share(m.hour, d.night);
        let cap = ((d.max as f32) * self.crowd.unwrap_or(1.0)).round() as usize;
        let target = ((cap as f32 * share).round() as usize).min(cap);
        // A new scene fills its streets at once, in view too (from 20 m out); afterwards people
        // arrive out of view in the spawn band, a couple per pass.
        let fill = self.spawned == 0;
        let (mut tries, mut made) = (0, 0);
        let (max_tries, max_made) = if fill { (600, target) } else { (24, 2) };
        while self.list.len() < target && tries < max_tries && made < max_made {
            tries += 1;
            if self.spawn(sim, def, m, fill) {
                made += 1;
            } else {
                self.misses += 1;
            }
        }
    }

    /// One person somewhere out of view in the spawn band (or anywhere from 20 m out when filling a
    /// new scene), of a kind that belongs there.
    fn spawn(&mut self, sim: &mut Sim, def: &PedsDef, m: &Moment, fill: bool) -> bool {
        let d = &def.density;
        let rng = &mut sim.state.rng;
        let a = rng.f32() * std::f32::consts::TAU;
        // Filling: evenly in distance, so the streets nearest the player are the busiest.
        let r = if fill { rng.range(15.0, d.radius) } else { rng.range(d.spawn[0], d.spawn[1]) };
        let p = Vec2::new(m.player.x, m.player.z) + Vec2::new(a.cos(), a.sin()) * r;
        let walk = &m.city.walk;
        let tile = walk.tile_of(p);
        if !matches!(walk.get(tile), Walk::Sidewalk | Walk::Path | Walk::Open) {
            return false;
        }
        let at = walk.centre(tile);
        let feet = Vec3::new(at.x, m.city.ground_y(at), at.y);
        if !fill && seen(&m.camera, feet + Vec3::Y) {
            return false;
        }
        // Only on loaded ground (a body spawned elsewhere falls through).
        if !crate::world::ground_loaded(sim, feet) {
            return false;
        }
        let rng = &mut sim.state.rng;
        let zone = m.city.zone_at(at).unwrap_or("");
        let fits: Vec<(usize, f32)> = def
            .archetype
            .iter()
            .enumerate()
            .filter(|(_, a)| a.zones.is_empty() || a.zones.iter().any(|z| z == zone))
            .map(|(i, a)| (i, a.weight.max(0.0)))
            .collect();
        let total: f32 = fits.iter().map(|f| f.1).sum();
        if total <= 0.0 {
            return false;
        }
        let mut roll = rng.f32() * total;
        let arch = fits.iter().find(|(_, w)| {
            roll -= w;
            roll <= 0.0
        });
        let arch = arch.map_or(fits[0].0, |f| f.0);
        let a = &def.archetype[arch];
        let puppet = look(a, rng);
        let pace = rng.range(a.pace[0], a.pace[1]);
        let facing = rng.f32() * std::f32::consts::TAU;
        let linger = rng.range(0.0, 3.0);
        let id = sim.spawn_npc(&format!("ped:{}", a.id), feet, facing, puppet, None, None);
        crate::world::ambient(sim, id);
        if let Some(c) = sim.state.entities.get_mut(id).and_then(|e| e.character.as_mut()) {
            c.haste = pace / if a.run { RUN } else { WALK } - 1.0;
        }
        self.list.push(Ped {
            id,
            arch: arch as u16,
            doing: Doing::Linger,
            path: Vec::new(),
            next: 0,
            timer: linger,
            check_t: 0.0,
            check: at,
            run: a.run,
            focus: None,
            seat: None,
            idle: None,
            swing: 0.0,
        });
        self.spawned += 1;
        true
    }

    /// A threat at `at`: everyone within `radius` runs away from it along the walk tiles (or
    /// cowers where there is nowhere to go).
    pub fn scare(&mut self, sim: &mut Sim, walk: &pav_world::walk::WalkMap, at: Vec3, radius: f32) {
        let threat = Vec2::new(at.x, at.z);
        for p in &mut self.list {
            if matches!(p.doing, Doing::Flee | Doing::Cower | Doing::Fight | Doing::Down) {
                continue;
            }
            if matches!(p.doing, Doing::Sit | Doing::Film) {
                let at3 = sim.feet(p.id).unwrap_or(at);
                if at3.distance(at) > radius {
                    continue;
                }
                Self::set_idle(sim, p, None);
                if let Some(c) = sim.state.entities.get_mut(p.id).and_then(|e| e.character.as_mut()) {
                    c.face = None;
                }
                p.seat = None;
                p.focus = None;
            }
            let Some(feet) = sim.feet(p.id) else { continue };
            let pos = Vec2::new(feet.x, feet.z);
            if pos.distance(threat) > radius {
                continue;
            }
            let away = (pos - threat).normalize_or(Vec2::X);
            let rng = &mut sim.state.rng;
            let mut route = Vec::new();
            for _ in 0..6 {
                let a = away.y.atan2(away.x) + (rng.f32() - 0.5) * 2.4;
                let to = pos + Vec2::new(a.cos(), a.sin()) * rng.range(35.0, 80.0);
                let t = walk.tile_of(to);
                if matches!(walk.get(t), Walk::Sidewalk | Walk::Path | Walk::Open) {
                    route = walk.path(pos, walk.centre(t), 4000);
                    if route.len() >= 2 {
                        break;
                    }
                }
            }
            // Right next to it, some freeze where they are.
            let freeze = pos.distance(threat) < 9.0 && rng.f32() < 0.4;
            if route.len() >= 2 && !freeze {
                p.path = route;
                p.next = 1;
                p.doing = Doing::Flee;
                p.run = true;
            } else {
                p.doing = Doing::Cower;
                p.timer = rng.range(6.0, 12.0);
                Self::set_idle(sim, p, Some(COWER_IDLE));
            }
            p.check = pos;
            p.check_t = 0.0;
        }
    }

    /// A car bearing down at `at` (it honks): whoever stands on the road there gets off it, to
    /// the nearest sidewalk away from the car's line, and never freezes in its way.
    pub fn dodge(&mut self, sim: &mut Sim, walk: &pav_world::walk::WalkMap, at: Vec3, radius: f32) {
        let threat = Vec2::new(at.x, at.z);
        for p in &mut self.list {
            if matches!(p.doing, Doing::Flee | Doing::Fight | Doing::Down) {
                continue;
            }
            let Some(feet) = sim.feet(p.id) else { continue };
            let pos = Vec2::new(feet.x, feet.z);
            if pos.distance(threat) > radius || matches!(walk.at(pos), Walk::Sidewalk | Walk::Path) {
                continue;
            }
            // The nearest sidewalk or path tile, ring by ring (2 m tiles), out to about 14 m.
            let here = walk.tile_of(pos);
            let mut best: Option<(f32, Vec2)> = None;
            for r in 1..=7i32 {
                for dz in -r..=r {
                    for dx in -r..=r {
                        if dx.abs() != r && dz.abs() != r {
                            continue;
                        }
                        let t = here + glam::IVec2::new(dx, dz);
                        if matches!(walk.get(t), Walk::Sidewalk | Walk::Path) {
                            let c = walk.centre(t);
                            let d = c.distance(pos);
                            if best.is_none_or(|(b, _)| d < b) {
                                best = Some((d, c));
                            }
                        }
                    }
                }
                if best.is_some() {
                    break;
                }
            }
            let Some((_, to)) = best else { continue };
            if p.doing == Doing::Cower {
                Self::set_idle(sim, p, None);
            }
            p.path = vec![pos, to];
            p.next = 1;
            p.doing = Doing::Flee;
            p.run = true;
            p.check = pos;
            p.check_t = 0.0;
        }
    }

    /// Someone outside the crowd joins it, running from `from` (a driver pulled from a car).
    pub fn adopt(&mut self, sim: &mut Sim, walk: &pav_world::walk::WalkMap, id: EntityId, from: Vec3) {
        if self.list.iter().any(|p| p.id == id) || sim.state.entities.get(id).is_none() {
            return;
        }
        self.list.push(Ped {
            id,
            arch: 0,
            doing: Doing::Walk,
            path: Vec::new(),
            next: 0,
            timer: 0.0,
            check_t: 0.0,
            check: Vec2::ZERO,
            run: false,
            focus: None,
            seat: None,
            idle: None,
            swing: 0.0,
        });
        self.scare(sim, walk, from, 30.0);
    }

    /// Forgets `id` if they died (an explosion, a crash).
    pub fn forget_if_dead(&mut self, sim: &Sim, id: EntityId) {
        if sim.state.entities.get(id).is_some_and(|e| e.name.starts_with("dead:")) {
            self.forget(id);
        }
    }

    /// Someone left the crowd (killed, arrested): no longer walked by the manager.
    pub fn forget(&mut self, id: EntityId) {
        self.list.retain(|p| p.id != id);
    }

    /// How many are doing what: (walking, waiting at a crossing, lingering).
    pub fn counts(&self) -> (usize, usize, usize) {
        let n = |d: Doing| self.list.iter().filter(|p| p.doing == d).count();
        (n(Doing::Walk), n(Doing::Wait), n(Doing::Linger))
    }
}

/// The crowd share of the zones around a point (a 5 × 5 grid 30 m apart, roads skipped).
fn area_share(city: &City, at: Vec2, d: &Density) -> f32 {
    let (mut sum, mut n) = (0.0, 0);
    for z in -2..=2 {
        for x in -2..=2 {
            if let Some(zone) = city.zone_at(at + Vec2::new(x as f32, z as f32) * 30.0) {
                sum += d.zones.get(zone).copied().unwrap_or(0.3);
                n += 1;
            }
        }
    }
    if n == 0 { 0.3 } else { sum / n as f32 }
}

/// A walkable tile 30–150 m from `from` (a few tries), as a place to walk to.
fn destination(walk: &pav_world::walk::WalkMap, from: Vec2, rng: &mut Rng) -> Option<Vec2> {
    for _ in 0..8 {
        let a = rng.f32() * std::f32::consts::TAU;
        let r = rng.range(30.0, 150.0);
        let p = from + Vec2::new(a.cos(), a.sin()) * r;
        let t = walk.tile_of(p);
        if matches!(walk.get(t), Walk::Sidewalk | Walk::Path | Walk::Open) {
            return Some(walk.centre(t));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_archetypes_load_and_make_many_distinct_people() {
        let def = def().unwrap();
        assert!(def.archetype.len() >= 8);
        let mut rng = Rng::new(5);
        let mut looks = std::collections::BTreeSet::new();
        for a in &def.archetype {
            for _ in 0..20 {
                let p = look(a, &mut rng);
                looks.insert(format!("{}{}{}{}{:.2}", p.skin, p.shirt, p.pants, p.shoes, p.scale));
            }
        }
        assert!(looks.len() >= 30, "{} distinct looks", looks.len());
    }

    #[test]
    fn fewer_people_are_out_at_night() {
        assert!((time_share(14.0, 0.35) - 1.0).abs() < 1e-3);
        assert!((time_share(2.0, 0.35) - 0.35).abs() < 1e-3);
        assert!(time_share(20.0, 0.35) > time_share(23.0, 0.35));
    }
}
