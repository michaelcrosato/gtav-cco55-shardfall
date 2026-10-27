//! The mission runtime: starting a mission (its protagonist, clock, actors), the title card,
//! each step kind, the actors' behaviours, fail conditions, checkpoints with Retry (and Skip after
//! three fails), MISSION PASSED with medals and the reward, and what the HUD, the map and the
//! camera show of it all.

use std::collections::BTreeMap;
use std::f32::consts::PI;

use anyhow::{Result, anyhow};
use glam::{Quat, Vec2, Vec3};
use pav_core::Color;
use pav_core::InputFrame;
use pav_core::entity::EntityId;
use pav_core::frame::SimEvent;
use pav_core::input::{buttons, pad};
use pav_core::present::*;
use pav_core::sim::Sim;

use super::*;
use crate::Freeway;
use crate::anchor::{self, Anchor};

/// The title card's seconds (✕ starts sooner), MISSION FAILED's before the card, MISSION
/// PASSED's before the results.
const INTRO: f32 = 3.5;
const FAIL_BANNER: f32 = 2.5;
const PASS_BANNER: f32 = 3.0;
/// The map's colour for activities.
const ACTIVITY_PINK: &str = "#ff3e8a";
/// Seconds between babble syllables.
const BABBLE: f32 = 0.11;

/// What a step's tick decided.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Flow {
    On,
    Done,
    Jump(usize),
    Fail(&'static str),
}

fn flat(v: Vec3) -> Vec2 {
    Vec2::new(v.x, v.z)
}

fn sound(cue: &str, gain: f32, pitch: f32) -> SimEvent {
    SimEvent::Sound { cue: cue.into(), pos: None, gain, pitch }
}

/// A character's yaw from degrees off north.
fn yaw_of(deg: f32) -> f32 {
    let a = deg.to_radians();
    a.sin().atan2(-a.cos())
}

/// A small deterministic hash (babble pitch, cosmetic only).
fn wobble(a: usize, b: usize) -> f32 {
    let x = (a as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ (b as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    ((x >> 40) as f32 / (1u64 << 24) as f32) - 0.5
}

/// The entity of actor `id`, if it is still in the world.
pub(crate) fn entity(sim: &Sim, run: &Run, id: &str) -> Option<EntityId> {
    run.actors.get(id).and_then(|a| a.entity).filter(|e| sim.state.entities.get(*e).is_some())
}

/// Whether actor `id` is done for: gone, dead, wrecked, or (a pickup) taken.
pub(crate) fn down(sim: &Sim, run: &Run, id: &str) -> bool {
    let Some(a) = run.actors.get(id) else { return true };
    if a.entity.is_none() {
        return a.taken && !a.waiting;
    }
    match entity(sim, run, id).and_then(|e| sim.state.entities.get(e)) {
        None => true,
        Some(e) => e.name.starts_with("dead:") || e.vehicle.as_ref().is_some_and(|v| v.state.wrecked),
    }
}

/// Where actor `id` is (a person's feet, a vehicle's ground point, a pickup's spot) and which
/// way it faces.
pub(crate) fn actor_at(sim: &Sim, run: &Run, id: &str) -> Option<(Vec3, f32)> {
    let a = run.actors.get(id)?;
    let Some(e) = a.entity else { return (!a.taken).then_some((a.at, a.yaw)) };
    let ent = sim.state.entities.get(e)?;
    if ent.vehicle.is_some() {
        let c = crate::drive::Car::read(sim, e)?;
        return Some((c.feet(), c.yaw));
    }
    let facing = ent.character.as_ref().map_or(0.0, |c| c.facing);
    Some((sim.feet(e)?, facing))
}

impl Freeway {
    /// The player's point: their vehicle's when in one, else their feet.
    pub fn player_point(&self, sim: &Sim) -> Vec3 {
        match self.drive.vehicle().and_then(|c| crate::drive::Car::read(sim, c)) {
            Some(c) => c.feet(),
            None => Self::feet(sim),
        }
    }

    /// An anchor's place and facing (actors and the player too).
    pub(crate) fn anchor_at(&self, sim: &Sim, run: &Run, a: &str) -> Option<(Vec3, f32)> {
        let city = self.city.as_ref()?;
        match Anchor::parse(a).ok()? {
            Anchor::Actor(id) => match actor_at(sim, run, &id) {
                Some(x) => Some(x),
                // A protagonist named as an actor: their body (or the player).
                None => {
                    let i = crate::protagonists::def().ok()?.index(&id)?;
                    let e = if i == self.crew.current { sim.state.player? } else { *self.crew.bodies.get(&i)? };
                    let facing = sim.state.entities.get(e)?.character.as_ref().map_or(0.0, |c| c.facing);
                    Some((sim.feet(e)?, facing))
                }
            },
            Anchor::Player => {
                let facing = sim.player().and_then(|p| p.character.as_ref()).map_or(0.0, |c| c.facing);
                Some((self.player_point(sim), facing))
            }
            Anchor::Deck(p) => anchor::deck_at(city, p),
            x => {
                let (p, yaw) = anchor::raw(city, &x)?;
                Some((anchor::ground(city, p), yaw.unwrap_or(0.0)))
            }
        }
    }

    /// Puts the player on foot at `feet`, facing `yaw`, the camera behind; the ground there is
    /// loaded at once (a deck or a roof is there to stand on, not streaming in under them).
    pub(crate) fn put_player(&mut self, sim: &mut Sim, feet: Vec3, yaw: f32) {
        let Some(pid) = sim.state.player else { return };
        if let Some(car) = self.drive.vehicle() {
            sim.exit_vehicle(pid, car);
        }
        self.drive.phase = crate::drive::Phase::Foot;
        self.cover = Default::default();
        sim.set_position(pid, feet + Vec3::Y * 0.05);
        sim.stream_now();
        if let Some(c) = sim.state.entities.get_mut(pid).and_then(|e| e.character.as_mut()) {
            c.facing = yaw;
            c.anim.facing = yaw;
            c.face = None;
        }
        self.cam.face(PI - yaw);
        self.radio.tune(None);
        if let Some(city) = self.city.clone() {
            self.traffic.autopilot(sim, &city, None);
        }
    }

    /// Starts mission `key` (`replay`: from the replay list, no reward; `place`: the player is
    /// put at its start, as when played from a scene or the menu).
    pub fn start_mission(
        &mut self,
        sim: &mut Sim,
        key: &str,
        replay: bool,
        place: bool,
        events: &mut Vec<SimEvent>,
    ) -> Result<()> {
        let def = super::get(key)?;
        if self.missions.run.is_some() {
            self.end_mission(sim);
        }
        let city = self.city.clone().ok_or_else(|| anyhow!("no district"))?;
        if let Ok(crew) = crate::protagonists::def()
            && let Some(i) = crew.index(&def.protagonist)
            && i != self.crew.current
        {
            self.force_protagonist(sim, i);
        }
        if place {
            let at = def.start.as_deref().or(def.blip.as_ref().map(|b| b.at.as_str()));
            if let Some((feet, yaw)) = at.and_then(|a| Anchor::parse(a).ok()).and_then(|a| anchor::foot(&city, &a)) {
                self.put_player(sim, feet, yaw);
            }
        }
        let mut clock = None;
        if let Some(t) = &def.time {
            if let Some(h) = crate::daytime::parse_time(&t.at) {
                self.sky.hour = h;
            }
            if t.lock {
                clock = Some(self.sky.speed);
                self.sky.speed = 0.0;
            }
        }
        if let (Some(w), Ok(world)) = (&def.weather, crate::daytime::def()) {
            let _ = self.sky.set_weather(&world, w, true);
        }
        self.heat.clear();
        self.police.stand_down();
        self.wheel = None;
        self.crew.wheel = None;
        let mut run = Run {
            key: key.to_string(),
            phase: Phase::Intro { t: 0.0 },
            step: 0,
            t: 0.0,
            elapsed: 0.0,
            actors: BTreeMap::new(),
            flags: Default::default(),
            prog: Progress::default(),
            subs: Vec::new(),
            checkpoint: None,
            fails: 0,
            reason: String::new(),
            menu: 0,
            stats: RunStats::default(),
            replay,
            medal: None,
            met: Vec::new(),
            paid: 0,
            base: (self.combat.shots, self.combat.hits, self.combat.headshots, self.combat.kills),
            help: None,
            clock,
            photos: self.phone.photos,
            nav: Default::default(),
        };
        for i in 0..def.spawn.len() {
            if !def.spawn[i].later {
                self.spawn_actor(sim, &def, &mut run, i, None);
            }
        }
        run.checkpoint = Some(self.save_checkpoint(sim, &run));
        self.missions.records.entry(key.to_string()).or_default().plays += 1;
        self.missions.run = Some(run);
        events.push(sound("mission.start", 1.0, 1.0));
        Ok(())
    }

    /// Makes actor `i` of the mission (at a checkpoint's saved place when given).
    fn spawn_actor(&mut self, sim: &mut Sim, def: &MissionDef, run: &mut Run, i: usize, saved: Option<&ActorSave>) {
        let s = &def.spawn[i];
        if run.actors.contains_key(&s.id) {
            return;
        }
        let Some(city) = self.city.clone() else { return };
        let a = Anchor::parse(&s.at).ok();
        let act = saved
            .map(|x| x.act.clone())
            .unwrap_or_else(|| s.behaviour.as_deref().and_then(|b| Act::parse(b).ok()).unwrap_or(Act::Idle));
        let blip = saved.map_or(s.blip.clone(), |x| x.blip.clone());
        let place = |vehicle: bool, me: &Freeway| -> Option<(Vec3, f32)> {
            if let Some(x) = saved {
                return Some((x.pos, x.yaw));
            }
            let (p, yaw) = match a.as_ref()? {
                Anchor::Actor(_) | Anchor::Player => me.anchor_at(sim, run, &s.at)?,
                x if vehicle => anchor::car(&city, x)?,
                x => anchor::foot(&city, x)?,
            };
            Some((p, s.yaw.map(yaw_of).unwrap_or(yaw)))
        };
        let mut actor = Actor {
            entity: None,
            at: Vec3::ZERO,
            spawn: i,
            act: act.clone(),
            blip,
            taken: saved.is_some_and(|x| x.taken),
            ordered: false,
            path: Vec::new(),
            timer: 0.0,
            gate: 0,
            lap: 0,
            waiting: false,
            yaw: 0.0,
        };
        if s.pickup.is_some() {
            let Some((p, _)) = place(false, self) else { return };
            actor.at = p;
            run.actors.insert(s.id.clone(), actor);
            return;
        }
        // Far from the player its ground may not be there yet: it waits (and is made when the
        // ground loads) rather than fall through the world.
        let player_body =
            s.protagonist.as_deref().and_then(|w| crate::protagonists::def().ok()?.index(w)) == Some(self.crew.current);
        if !player_body {
            let Some((p, yaw)) = place(s.vehicle.is_some(), self) else { return };
            if !crate::world::ground_loaded(sim, p) {
                actor.at = p;
                actor.yaw = yaw;
                actor.waiting = true;
                run.actors.insert(s.id.clone(), actor);
                return;
            }
        }
        let keep = |sim: &mut Sim, id: EntityId| {
            if let Some(e) = sim.state.entities.get_mut(id) {
                e.unload = pav_core::entity::Unload::Keep;
            }
        };
        let id = if let Some(key) = &s.vehicle {
            let Some((p, yaw)) = place(true, self) else { return };
            match crate::vehicles::spawn(sim, key, p + Vec3::Y * 0.05, yaw, s.paint) {
                Ok(id) => {
                    if s.locked {
                        self.drive.locked.insert(id);
                    }
                    Some(id)
                }
                Err(e) => {
                    log::warn!("mission {}: {e:#}", def.key);
                    None
                }
            }
        } else if let Some(side) = &s.enemy {
            let Some((p, yaw)) = place(false, self) else { return };
            let id = self.enemies.spawn(sim, side, p, yaw, s.weapon.as_deref());
            if let Some(id) = id
                && let Some(e) = self.enemies.list.iter_mut().find(|e| e.id == id)
            {
                e.aware = matches!(act, Act::Fight | Act::Chase);
            }
            id
        } else if let Some(who) = &s.protagonist {
            let crew = crate::protagonists::def().ok();
            let idx = crew.as_ref().and_then(|c| c.index(who));
            match (idx, crew) {
                (Some(i), _) if i == self.crew.current => sim.state.player,
                (Some(i), Some(c)) => {
                    let Some((p, yaw)) = place(false, self) else { return };
                    let id = sim.spawn_npc(&c.protagonist[i].key, p, yaw, c.protagonist[i].puppet(), None, None);
                    self.crew.bodies.insert(i, id);
                    Some(id)
                }
                _ => None,
            }
        } else {
            let Some((p, yaw)) = place(false, self) else { return };
            let look = crate::peds::def().ok().and_then(|d| {
                let a = d.archetype.iter().find(|a| Some(&a.id) == s.ped.as_ref())?;
                Some(crate::peds::look(a, &mut sim.state.rng))
            });
            let mut look = look.unwrap_or_default();
            if let Some(w) = &s.weapon {
                look.weapon.asset = crate::combat::asset_of(w);
                if !look.weapon.asset.is_empty() {
                    look.weapon.kind = pav_core::puppet::WeaponKind::Dagger;
                }
            }
            Some(sim.spawn_npc(&format!("mission:{}", s.id), p, yaw, look, None, None))
        };
        let Some(id) = id else { return };
        if Some(id) != sim.state.player {
            keep(sim, id);
        }
        if let Some(hp) = s.health.or(saved.map(|x| x.health)).filter(|_| s.vehicle.is_none() && s.protagonist.is_none()) {
            self.combat.hp.insert(id.0, hp);
        }
        // A vehicle's health: the share of a new one's (a wreck to be, a worn getaway car).
        if s.vehicle.is_some()
            && let Some(h) = s.health
            && let Some(v) = sim.state.entities.get_mut(id).and_then(|e| e.vehicle.as_deref_mut())
        {
            v.state.health = h.clamp(0.01, 1.0);
        }
        actor.entity = Some(id);
        run.actors.insert(s.id.clone(), actor);
        // The driver at the wheel (a checkpoint puts its own people back in their seats). One
        // still waiting for the ground with the vehicle is made now, in its seat: woken apart,
        // it would stand beside a vehicle no one drives.
        if let Some(dr) = &s.driver
            && saved.is_none()
        {
            if run.actors.get(dr).is_some_and(|d| d.waiting) {
                run.actors.remove(dr);
            }
            if let Some(j) = def.spawn.iter().position(|x| &x.id == dr) {
                self.spawn_actor(sim, def, run, j, None);
            }
            if let Some(d) = entity(sim, run, dr) {
                sim.enter_vehicle_seat(d, id, 0);
            }
        }
    }

    /// Actors waiting for their ground: made now where it has loaded (with the behaviour and
    /// blip they were given while they waited).
    fn wake_waiting(&mut self, sim: &mut Sim, def: &MissionDef, run: &mut Run) {
        let ready: Vec<String> = run
            .actors
            .iter()
            .filter(|(_, a)| a.waiting && crate::world::ground_loaded(sim, a.at))
            .map(|(k, _)| k.clone())
            .collect();
        for id in ready {
            // (A driver its vehicle woke already sits at the wheel.)
            if !run.actors.get(&id).is_some_and(|a| a.waiting) {
                continue;
            }
            let Some(a) = run.actors.remove(&id) else { continue };
            self.spawn_actor(sim, def, run, a.spawn, None);
            if let Some(x) = run.actors.get_mut(&id) {
                x.act = a.act;
                x.blip = a.blip;
                x.ordered = false;
            }
        }
    }

    /// Takes actor `id` out of the world.
    fn despawn_actor(&mut self, sim: &mut Sim, run: &mut Run, id: &str) {
        let Some(a) = run.actors.remove(id) else { return };
        let Some(e) = a.entity else { return };
        if Some(e) == sim.state.player {
            return;
        }
        if let Some(car) = self.drive.vehicle()
            && car == e
        {
            // The player's own seat stays (the car is theirs now).
            return;
        }
        self.traffic.dismiss(e);
        self.enemies.list.retain(|x| x.id != e);
        self.crew.bodies.retain(|_, b| *b != e);
        for p in crate::traffic::occupants(sim, e) {
            if Some(p) != sim.state.player {
                sim.despawn(p);
                run.actors.retain(|_, a| a.entity != Some(p));
            }
        }
        sim.despawn(e);
    }

    /// The mission's actions.
    fn act(&mut self, sim: &mut Sim, def: &MissionDef, run: &mut Run, a: &Action, events: &mut Vec<SimEvent>) {
        match a {
            Action::Spawn(id) => {
                if let Some(i) = def.spawn.iter().position(|s| &s.id == id) {
                    self.spawn_actor(sim, def, run, i, None);
                }
            }
            Action::Despawn(id) => self.despawn_actor(sim, run, id),
            Action::Behaviour { actor, act } => {
                if let (Some(x), Ok(b)) = (run.actors.get_mut(actor), Act::parse(act)) {
                    x.act = b.clone();
                    x.ordered = false;
                    x.path.clear();
                    if let Some(e) = x.entity
                        && let Some(en) = self.enemies.list.iter_mut().find(|en| en.id == e)
                    {
                        en.aware = matches!(b, Act::Fight | Act::Chase);
                    }
                }
            }
            Action::Blip { actor, kind } => {
                if let Some(x) = run.actors.get_mut(actor) {
                    x.blip = Some(kind.clone());
                }
            }
            Action::Unblip(actor) => {
                if let Some(x) = run.actors.get_mut(actor) {
                    x.blip = None;
                }
            }
            Action::Heat(0) => {
                self.heat.clear();
                self.police.stand_down();
            }
            Action::Heat(n) => {
                let at = Self::feet(sim);
                self.heat.raise(*n, at);
            }
            Action::HeatFloor(n) => self.heat.floor = *n,
            Action::HeatCap(n) => self.heat.cap = *n,
            Action::Dispatch(on) => self.heat.dispatch = *on,
            Action::Search(true) => {
                let at = Self::feet(sim);
                self.heat.lose_sight(at);
            }
            Action::Search(false) => {}
            Action::Traffic(k) => self.traffic.thin = Some(k.clamp(0.0, 1.0)),
            Action::Flag(f) => {
                run.flags.insert(f.clone());
            }
            Action::Unflag(f) => {
                run.flags.remove(f);
            }
            Action::Story(f) => {
                self.missions.story.insert(f.clone());
            }
            Action::Give { weapon, ammo } => {
                self.give_weapon(weapon, *ammo);
                // In hand: what the mission hands over is what to use.
                if let Some(i) = crate::combat::def().ok().and_then(|d| d.weapon.iter().position(|w| &w.id == weapon)) {
                    self.weapon = if i == crate::combat::BAT { 0 } else { i };
                }
            }
            Action::Cash(n) => self.pay(*n),
            Action::Loot([lo, hi]) => {
                let n = lo + sim.state.rng.below((hi - lo + 1).clamp(1, u32::MAX as i64) as u32) as i64;
                self.pay(n);
            }
            Action::Armor(a) => self.combat.armor = a.clamp(0.0, crate::combat::MAX_ARMOR),
            Action::Pay { who, cash } => {
                let i = crate::protagonists::def().ok().and_then(|d| d.index(who));
                match i {
                    Some(i) if i == self.crew.current => self.pay(*cash),
                    Some(i) => {
                        if let Some(m) = self.crew.members.get_mut(i) {
                            m.cash += cash;
                        }
                    }
                    None => {}
                }
            }
            Action::Text { from, body } => self.text_message(from, body),
            Action::Note { title, body } => self.note("info", title, body),
            Action::Help(text) => run.help = Some((text.clone(), 0.0)),
            Action::Time(t) => {
                if let Some(h) = crate::daytime::parse_time(t) {
                    self.sky.hour = h;
                }
            }
            Action::Weather(w) => {
                if let Ok(world) = crate::daytime::def() {
                    let _ = self.sky.set_weather(&world, w, true);
                }
            }
            Action::Lock { actor, locked } => {
                if let Some(e) = entity(sim, run, actor) {
                    if *locked {
                        self.drive.locked.insert(e);
                    } else {
                        self.drive.locked.remove(&e);
                    }
                }
            }
            Action::Teleport(to) => {
                if let Some((p, yaw)) = self.anchor_at(sim, run, to) {
                    self.teleport_player(sim, p, yaw);
                }
            }
            Action::Alert(true) => self.enemies.alert(),
            Action::Alert(false) => {}
            Action::Explode(id) => {
                if let Some(e) = entity(sim, run, id) {
                    sim.explode_vehicle(e, events);
                }
            }
            Action::Unlock(f) => {
                if f == "switch" {
                    self.crew.unlocked = true;
                }
            }
        }
    }

    /// The player (and the vehicle they sit in) to `p`, facing `yaw`.
    pub(crate) fn teleport_player(&mut self, sim: &mut Sim, p: Vec3, yaw: f32) {
        match self.drive.vehicle() {
            Some(car) => {
                let (q, y) = self.city.as_ref().map_or((p, yaw), |c| anchor::car_at(c, flat(p), yaw));
                sim.set_position(car, q + Vec3::Y * 0.05);
                if let Some(e) = sim.state.entities.get_mut(car) {
                    e.rot = Quat::from_rotation_y(y);
                    if let Some(v) = e.vehicle.as_mut() {
                        v.yaw = y;
                    }
                }
                if let Some(b) = sim.state.entities.get(car).and_then(|e| e.body)
                    && let Some(body) = sim.state.physics.bodies.get_mut(b)
                {
                    body.set_rotation(Quat::from_rotation_y(y), true);
                }
                self.chase.place_behind(y);
            }
            None => self.put_player(sim, p, yaw),
        }
    }

    /// Gives the player weapon `id` (and `ammo` rounds, or a full reserve).
    pub(crate) fn give_weapon(&mut self, id: &str, ammo: u32) {
        let Ok(wd) = crate::combat::def() else { return };
        let Some(i) = wd.weapon.iter().position(|w| w.id == id || w.icon == id) else { return };
        self.combat.arm(&wd);
        let w = &wd.weapon[i];
        let cur = self.crew.current;
        if let Some(m) = self.crew.members.get_mut(cur) {
            if m.owned.len() < wd.weapon.len() {
                m.owned.resize(wd.weapon.len(), false);
            }
            m.owned[i] = true;
        }
        self.combat.mag[i] = self.combat.mag[i].max(w.magazine);
        self.combat.reserve[i] += if ammo > 0 { ammo } else { w.reserve };
    }

    /// A checkpoint of the mission as it stands.
    fn save_checkpoint(&self, sim: &Sim, run: &Run) -> Checkpoint {
        let feet = Self::feet(sim);
        let yaw = sim.player().and_then(|p| p.character.as_ref()).map_or(0.0, |c| c.facing);
        let vehicle = self.drive.vehicle().and_then(|car| {
            let c = crate::drive::Car::read(sim, car)?;
            let actor = run.actors.iter().find(|(_, a)| a.entity == Some(car)).map(|(k, _)| k.clone());
            Some((actor, c.key.clone(), c.feet(), c.yaw))
        });
        let actors = run
            .actors
            .iter()
            .filter(|(k, _)| !down(sim, run, k) || run.actors[*k].entity.is_none())
            .filter(|(_, a)| a.entity != sim.state.player || a.entity.is_none())
            .map(|(k, a)| {
                let (pos, yaw) = actor_at(sim, run, k).unwrap_or((a.at, 0.0));
                let riding =
                    a.entity.and_then(|e| sim.state.entities.get(e)).and_then(|e| e.character.as_ref()).and_then(|c| c.riding);
                let riding =
                    riding.and_then(|car| run.actors.iter().find(|(_, x)| x.entity == Some(car)).map(|(k, _)| k.clone()));
                ActorSave {
                    id: k.clone(),
                    spawn: a.spawn,
                    pos,
                    yaw,
                    act: a.act.clone(),
                    blip: a.blip.clone(),
                    health: a.entity.and_then(|e| self.combat.hp.get(&e.0).copied()).unwrap_or(100.0),
                    riding,
                    taken: a.taken,
                }
            })
            .collect();
        Checkpoint {
            step: run.step,
            elapsed: run.elapsed,
            protagonist: self.crew.current,
            feet,
            yaw,
            vehicle,
            actors,
            flags: run.flags.clone(),
            stars: self.heat.stars,
            health: self.combat.health,
            armor: self.combat.armor,
            hour: self.sky.hour,
        }
    }

    /// Back to the checkpoint: the actors as they were, the player where they were (in their
    /// vehicle), the heat, health and clock as then; the step begins again.
    fn restore(&mut self, sim: &mut Sim, def: &MissionDef, run: &mut Run, events: &mut Vec<SimEvent>) {
        let Some(cp) = run.checkpoint.clone() else { return };
        // Back on their feet.
        if !self.fate.alive()
            && let Some(pid) = sim.state.player
            && let Some(c) = sim.state.entities.get_mut(pid).and_then(|e| e.character.as_mut())
        {
            c.anim.stop_clip();
            c.vel = Vec3::ZERO;
        }
        self.fate = crate::fate::Fate::Alive;
        if cp.protagonist != self.crew.current {
            self.force_protagonist(sim, cp.protagonist);
        }
        // Out of the car first: an actor's vehicle the player sat in goes with the other actors
        // (left behind, it stood in the road the retry drives — a heist truck's way blocked).
        if let (Some(pid), Some(car)) = (sim.state.player, self.drive.vehicle()) {
            sim.exit_vehicle(pid, car);
        }
        self.drive.phase = crate::drive::Phase::Foot;
        let ids: Vec<String> = run.actors.keys().cloned().collect();
        for id in ids {
            self.despawn_actor(sim, run, &id);
        }
        self.heat.clear();
        self.police.clear(sim);
        self.enemies.camp = 0.0;
        // The actors, vehicles first (people may sit in them).
        let mut order: Vec<&ActorSave> = cp.actors.iter().collect();
        order.sort_by_key(|a| def.spawn.get(a.spawn).is_none_or(|s| s.vehicle.is_none()));
        for a in order {
            self.spawn_actor(sim, def, run, a.spawn, Some(a));
            if let (Some(car), Some(me)) = (a.riding.as_ref().and_then(|r| entity(sim, run, r)), entity(sim, run, &a.id)) {
                sim.enter_vehicle_seat(me, car, 0);
            }
        }
        self.put_player(sim, cp.feet, cp.yaw);
        if let Some((actor, key, pos, yaw)) = &cp.vehicle {
            let car = match actor.as_ref().and_then(|a| entity(sim, run, a)) {
                Some(c) => Some(c),
                None => crate::vehicles::spawn(sim, key, *pos + Vec3::Y * 0.05, *yaw, None).ok(),
            };
            if let (Some(car), Some(pid)) = (car, sim.state.player) {
                sim.set_position(car, *pos + Vec3::Y * 0.05);
                if self.drive.seat(sim, pid, car) {
                    self.chase.place_behind(*yaw);
                }
            }
        }
        if cp.stars > 0 {
            self.heat.raise(cp.stars, cp.feet);
        }
        self.combat.health = cp.health.max(crate::combat::MAX_HEALTH * 0.5);
        self.combat.armor = cp.armor;
        self.sky.hour = cp.hour;
        self.foot = Default::default();
        run.flags = cp.flags.clone();
        run.step = cp.step;
        run.elapsed = cp.elapsed;
        run.t = 0.0;
        run.prog = Progress::default();
        run.subs.clear();
        run.phase = Phase::Play;
        self.front.fade_in(self.ui_time);
        self.start_step(sim, def, run, events);
    }

    /// The step `run.step` begins: its actions, its tip, who may be switched to, the behaviours
    /// it sets going.
    fn start_step(&mut self, sim: &mut Sim, def: &MissionDef, run: &mut Run, events: &mut Vec<SimEvent>) {
        let Some(step) = def.step.get(run.step).cloned() else { return };
        run.t = 0.0;
        run.prog = Progress { started: true, ..Default::default() };
        run.photos = self.phone.photos;
        for a in &step.on_start {
            self.act(sim, def, run, a, events);
        }
        if let Some(h) = &step.help {
            run.help = Some((h.clone(), 0.0));
        }
        let crew = crate::protagonists::def().ok();
        let index = |k: &str| crew.as_ref().and_then(|c| c.index(k));
        self.crew.allowed = match step.kind {
            StepKind::Switch => step.to.as_deref().and_then(index).map(|i| vec![i]),
            StepKind::Parallel => Some(step.steps.iter().filter_map(|s| s.who.as_deref().and_then(index)).collect()),
            _ if !step.switch.is_empty() => Some(step.switch.iter().filter_map(|k| index(k)).collect()),
            _ => Some(Vec::new()),
        };
        run.prog.done = vec![false; step.targets.len().max(step.gates.len())];
        match step.kind {
            StepKind::Chase => {
                if let Some(Ok(Anchor::Actor(id))) = step.target.as_deref().map(Anchor::parse)
                    && let Some(a) = run.actors.get_mut(&id)
                    && a.act != Act::Flee
                {
                    a.act = Act::Flee;
                    a.ordered = false;
                }
            }
            StepKind::Escape => {
                for p in &step.pursuers {
                    if let Some(a) = run.actors.get_mut(p)
                        && !matches!(a.act, Act::Chase | Act::Fight)
                    {
                        a.act = Act::Chase;
                        a.ordered = false;
                    }
                }
            }
            StepKind::Survive | StepKind::Defend => {
                if let Some(w) = step.waves.first() {
                    for id in w.clone() {
                        self.act(sim, def, run, &Action::Spawn(id.clone()), events);
                        self.act(sim, def, run, &Action::Behaviour { actor: id, act: "fight".into() }, events);
                    }
                }
            }
            StepKind::Race => {
                for r in &step.racers {
                    if let Some(a) = run.actors.get_mut(r) {
                        a.gate = 0;
                        a.lap = 0;
                        a.ordered = false;
                    }
                }
            }
            StepKind::Parallel => {
                run.subs = step
                    .steps
                    .iter()
                    .map(|s| Sub {
                        who: s.who.as_deref().and_then(index).unwrap_or(0),
                        t: 0.0,
                        prog: Progress { done: vec![false; s.targets.len()], ..Default::default() },
                        done: false,
                    })
                    .collect();
            }
            _ => {}
        }
    }

    /// The step is over: its closing actions run, and the next begins (a checkpoint is kept
    /// at those marked so), or the mission is passed.
    fn advance(&mut self, sim: &mut Sim, def: &MissionDef, run: &mut Run, to: Option<usize>, events: &mut Vec<SimEvent>) {
        if let Some(step) = def.step.get(run.step).cloned() {
            for a in &step.on_done {
                self.act(sim, def, run, a, events);
            }
        }
        let next = to.unwrap_or(run.step + 1);
        if next >= def.step.len() {
            self.pass(sim, def, run, events);
            return;
        }
        run.step = next;
        if def.step[next].checkpoint {
            run.checkpoint = Some(self.save_checkpoint(sim, run));
            run.fails = 0;
        }
        events.push(sound("mission.step", 0.6, 1.0));
        self.start_step(sim, def, run, events);
    }

    /// The fail conditions in force (the mission's, the step's, and dying or arrest).
    fn failing(&self, sim: &Sim, run: &Run, list: &[FailDef], step_t: f32, mission: bool) -> Option<String> {
        let me = self.player_point(sim);
        for f in list {
            let hit = match &f.when {
                When::Destroyed(id) => down(sim, run, id),
                When::Died(id) if id == "player" => !self.fate.alive(),
                When::Died(id) => down(sim, run, id),
                When::Abandoned { actor, distance } => {
                    !down(sim, run, actor) && actor_at(sim, run, actor).is_some_and(|(p, _)| p.distance(me) > *distance)
                }
                When::Spotted(list) => list.iter().any(|w| self.watcher_sees(sim, run, w)),
                When::Timer(s) => (if mission { run.elapsed } else { step_t }) > *s,
                When::HeatOver(n) => self.heat.stars > *n,
                When::LeftArea { at, radius } => {
                    self.anchor_at(sim, run, at).is_some_and(|(p, _)| flat(p).distance(flat(me)) > *radius)
                }
                When::Flag(f) => run.flags.contains(f),
            };
            if hit {
                return Some(f.reason.clone());
            }
        }
        None
    }

    /// Whether watcher `id` sees the player now: near, in the cone ahead of it, nothing between
    /// (crouching halves how far it sees).
    pub(crate) fn watcher_sees(&self, sim: &Sim, run: &Run, id: &str) -> bool {
        if down(sim, run, id) {
            return false;
        }
        let Some((p, facing)) = actor_at(sim, run, id) else { return false };
        let me = Self::feet(sim);
        let reach = if self.foot.crouched { 9.0 } else { 18.0 };
        let to = me - p;
        let d = flat(to).length();
        if d > reach || to.y.abs() > 4.0 {
            return false;
        }
        let ahead = Vec2::new(facing.sin(), facing.cos());
        let close = d < 1.5;
        (close || flat(to).normalize_or_zero().dot(ahead) > (50f32).to_radians().cos())
            && crate::heat::sight(sim, p + Vec3::Y * 1.6, me + Vec3::Y * 1.2)
    }

    /// MISSION FAILED, for `reason`.
    fn fail(&mut self, run: &mut Run, reason: String, events: &mut Vec<SimEvent>) {
        let title = super::get(&run.key).map(|d| d.title.clone()).unwrap_or_default();
        self.mission_log(format!("~r~Failed~s~ {title}: {reason}"));
        run.phase = Phase::Failed { t: 0.0 };
        run.reason = reason;
        run.fails += 1;
        run.menu = 0;
        self.missions.failed += 1;
        self.wheel = None;
        self.crew.wheel = None;
        self.crew.ability = false;
        events.push(sound("mission.fail", 1.0, 1.0));
    }

    /// MISSION PASSED: the medals, the reward, the record.
    fn pass(&mut self, sim: &mut Sim, def: &MissionDef, run: &mut Run, events: &mut Vec<SimEvent>) {
        let met: Vec<bool> = def.medals.iter().map(|m| self.medal_met(sim, run, m)).collect();
        let n = met.iter().filter(|x| **x).count();
        run.medal = (!met.is_empty()).then(|| {
            if n == met.len() {
                Medal::Gold
            } else if n * 2 >= met.len() {
                Medal::Silver
            } else {
                Medal::Bronze
            }
        });
        run.met = met;
        run.paid = if run.replay { 0 } else { def.reward.cash };
        if run.paid != 0 {
            self.pay(run.paid);
        }
        if !run.replay {
            for f in &def.reward.unlock {
                if f == "switch" {
                    self.crew.unlocked = true;
                }
            }
            if let Some((from, body)) = &def.reward.text {
                self.text_message(from, body);
            }
        }
        let r = self.missions.records.entry(def.key.clone()).or_default();
        r.passed = true;
        r.best = Some(r.best.map_or(run.elapsed, |b| b.min(run.elapsed)));
        let rank = |m: Option<Medal>| match m {
            Some(Medal::Gold) => 3,
            Some(Medal::Silver) => 2,
            Some(Medal::Bronze) => 1,
            None => 0,
        };
        if rank(run.medal) > rank(r.medal) {
            r.medal = run.medal;
        }
        self.missions.passed += 1;
        let medal = match run.medal {
            Some(Medal::Gold) => " (gold)",
            Some(Medal::Silver) => " (silver)",
            Some(Medal::Bronze) => " (bronze)",
            None => "",
        };
        self.mission_log(format!("~g~Passed~s~ {}{medal}", def.title));
        run.phase = Phase::Passed { t: 0.0 };
        self.crew.allowed = None;
        events.push(sound("mission.pass", 1.0, 1.0));
    }

    fn medal_met(&self, sim: &Sim, run: &Run, m: &MedalDef) -> bool {
        if let Some(t) = m.time_under {
            return run.elapsed < t;
        }
        if let Some(v) = &m.vehicle_health_over {
            return entity(sim, run, &v.actor).and_then(|e| sim.vehicle_status(e)).is_some_and(|s| s.health >= v.value);
        }
        if let Some(h) = m.health_over {
            return self.combat.health / crate::combat::MAX_HEALTH >= h;
        }
        if let Some(n) = m.headshots {
            return run.stats.headshots >= n;
        }
        if let Some(a) = m.accuracy_over {
            return run.stats.shots > 0 && run.stats.hits as f32 / run.stats.shots as f32 >= a;
        }
        if m.undetected {
            return !run.stats.spotted;
        }
        if m.no_heat {
            return run.stats.peak_stars == 0;
        }
        if let Some(f) = &m.flag {
            return run.flags.contains(f);
        }
        false
    }

    /// The mission ends (passed and seen, quit, or another begins): its actors go, the switch
    /// wheel opens up again, the clock runs, the heat limits lift.
    pub fn end_mission(&mut self, sim: &mut Sim) {
        let Some(mut run) = self.missions.run.take() else { return };
        let ids: Vec<String> = run.actors.keys().cloned().collect();
        for id in ids {
            // Vehicles stay where they are (out of the mission's hands); people and pickups go.
            let car = run
                .actors
                .get(&id)
                .and_then(|a| a.entity)
                .filter(|e| sim.state.entities.get(*e).is_some_and(|x| x.vehicle.is_some()));
            match car {
                Some(c) => {
                    self.traffic.dismiss(c);
                    for p in crate::traffic::occupants(sim, c) {
                        if Some(p) != sim.state.player {
                            sim.despawn(p);
                        }
                    }
                    if Some(c) != self.drive.vehicle() {
                        self.traffic.loose.push(c);
                    }
                    run.actors.remove(&id);
                }
                None => self.despawn_actor(sim, &mut run, &id),
            }
        }
        for (_, b) in std::mem::take(&mut self.crew.bodies) {
            if Some(b) != sim.state.player {
                sim.despawn(b);
            }
        }
        self.crew.allowed = None;
        self.heat.floor = 0;
        self.heat.cap = 5;
        self.heat.dispatch = true;
        self.traffic.thin = None;
        if let Some(speed) = run.clock {
            self.sky.speed = speed;
        }
        if let Some(city) = self.city.clone() {
            self.traffic.autopilot(sim, &city, None);
        }
        self.bot.stop();
        self.missions.cooldown = 6.0;
    }

    /// One tick of the mission under way (and of the mission markers when there is none). The
    /// actors' moves go into `out`.
    pub(crate) fn mission_tick(
        &mut self,
        sim: &mut Sim,
        input: &InputFrame,
        dt: f32,
        real: f32,
        events: &mut Vec<SimEvent>,
        out: &mut BTreeMap<EntityId, InputFrame>,
    ) {
        self.missions.cooldown = (self.missions.cooldown - real).max(0.0);
        let Some(mut run) = self.missions.run.take() else {
            self.mission_markers(sim, events);
            return;
        };
        let def = match super::get(&run.key) {
            Ok(d) => d,
            Err(e) => {
                log::error!("{e:#}");
                return;
            }
        };
        if let Some(h) = &mut run.help {
            h.1 += real;
            if h.1 > 10.0 {
                run.help = None;
            }
        }
        let accept = input.pad.pressed & pad::SOUTH != 0 || input.cmds.iter().any(|c| c.starts_with("ui.accept"));
        match run.phase.clone() {
            Phase::Intro { t } => {
                let t = t + real;
                run.phase = Phase::Intro { t };
                if t >= INTRO || (accept && t > 0.6) {
                    run.phase = Phase::Play;
                    self.start_step(sim, &def, &mut run, events);
                }
            }
            Phase::Play => {
                run.t += dt;
                run.elapsed += dt;
                run.stats.peak_stars = run.stats.peak_stars.max(self.heat.stars);
                let b = run.base;
                run.stats.shots = self.combat.shots - b.0;
                run.stats.hits = self.combat.hits - b.1;
                run.stats.headshots = self.combat.headshots - b.2;
                run.stats.kills = self.combat.kills - b.3;
                self.wake_waiting(sim, &def, &mut run);
                self.drive_actors(sim, &def, &mut run, out, dt, events);
                let step = def.step[run.step].clone();
                let reason = if !self.fate.alive() {
                    Some(
                        if matches!(self.fate, crate::fate::Fate::Busted { .. }) { "You were busted." } else { "You died." }
                            .to_string(),
                    )
                } else {
                    self.failing(sim, &run, &def.fail, run.t, true).or_else(|| self.failing(sim, &run, &step.fail, run.t, false))
                };
                match reason {
                    Some(r) => self.fail(&mut run, r, events),
                    None => {
                        let mut prog = std::mem::take(&mut run.prog);
                        let t = run.t;
                        let flow = self.step_tick(sim, &def, &step, &mut prog, t, &mut run, input, dt, real, events);
                        run.prog = prog;
                        match flow {
                            Flow::On => {}
                            Flow::Done => self.advance(sim, &def, &mut run, None, events),
                            Flow::Jump(i) => self.advance(sim, &def, &mut run, Some(i), events),
                            Flow::Fail(why) => self.fail(&mut run, why.to_string(), events),
                        }
                    }
                }
            }
            Phase::Failed { t } => {
                let t = t + real;
                run.phase = Phase::Failed { t };
                if t > FAIL_BANNER {
                    let items = fail_items(run.fails);
                    let (_, dy) = run.nav.step(input, real);
                    run.menu = crate::pad::move_focus(run.menu, items.len(), dy, |_| true);
                    for c in &input.cmds {
                        if let Some(id) = c.strip_prefix("ui.focus ").or(c.strip_prefix("ui.accept "))
                            && let Some(i) = items.iter().position(|x| x.0 == id)
                        {
                            run.menu = i;
                        }
                    }
                    if accept {
                        match items[run.menu.min(items.len() - 1)].0 {
                            "retry" => {
                                self.missions.retries += 1;
                                self.restore(sim, &def, &mut run, events);
                            }
                            "skip" => {
                                self.missions.skips += 1;
                                self.skip(sim, &def, &mut run, events);
                            }
                            _ => {
                                self.missions.run = Some(run);
                                self.quit_mission(sim);
                                return;
                            }
                        }
                    }
                }
            }
            Phase::Passed { t } => {
                let t = t + real;
                run.phase = Phase::Passed { t };
                if t > PASS_BANNER + 0.8 && accept {
                    self.missions.run = Some(run);
                    self.end_mission(sim);
                    self.autosave = true;
                    return;
                }
            }
        }
        self.missions.run = Some(run);
    }

    /// A line for the mission log (the newest few are kept).
    fn mission_log(&mut self, line: String) {
        self.missions.log.push(line);
        let n = self.missions.log.len();
        if n > 8 {
            self.missions.log.drain(..n - 8);
        }
    }

    /// Restart Checkpoint (the pause menu): back to the last checkpoint, as a retry would.
    pub fn restart_checkpoint(&mut self, sim: &mut Sim, events: &mut Vec<SimEvent>) {
        let Some(mut run) = self.missions.run.take() else { return };
        if let Ok(def) = super::get(&run.key)
            && run.phase == Phase::Play
        {
            self.missions.retries += 1;
            self.restore(sim, &def, &mut run, events);
        }
        self.missions.run = Some(run);
    }

    /// Whether the pause menu may restart a checkpoint now.
    pub fn may_restart(&self) -> bool {
        self.missions.run.as_ref().is_some_and(|r| r.phase == Phase::Play && r.checkpoint.is_some())
    }

    /// The pause map's mission blips: the missions to pick up (free roam), else where the one
    /// under way goes next.
    pub(crate) fn mission_map_blips(&self, sim: &Sim) -> Vec<Blip> {
        let mut out = Vec::new();
        if self.missions.run.is_none() {
            for (key, p) in self.open_missions() {
                let Ok(d) = super::get(&key) else { continue };
                let color = if d.activity { Color::hex(ACTIVITY_PINK) } else { self.protagonist_color(&d.protagonist) };
                out.push(Blip {
                    pos: flat(p),
                    icon: d.blip.as_ref().map_or(BlipIcon::Letter('?'), |b| b.map_icon()),
                    color,
                    label: Some(d.title.clone()),
                    clamp: true,
                    height: 0,
                    pulse: false,
                });
            }
        } else if let Some(w) = self.mission_waypoint(sim) {
            out.push(Blip {
                pos: w,
                icon: BlipIcon::Objective,
                color: Color::hex("#ffc145"),
                label: Some("Objective".into()),
                clamp: true,
                height: 0,
                pulse: true,
            });
        }
        out
    }

    /// Quits the mission (from the fail card or the pause menu): the player is left where they
    /// are (a death or arrest is seen through first).
    pub fn quit_mission(&mut self, sim: &mut Sim) {
        self.end_mission(sim);
        if !self.fate.alive()
            && let Some(city) = self.city.clone()
        {
            self.respawn(sim, &city);
        }
    }

    /// `mission skip [n]`: on to the next checkpoint `n` times, as the fail card's Skip goes
    /// (tests; the attract loop's heist excerpt starts at the stand-off).
    pub(crate) fn skip_checkpoints(&mut self, sim: &mut Sim, n: usize, events: &mut Vec<SimEvent>) {
        let Some(mut run) = self.missions.run.take() else { return };
        let Ok(def) = super::get(&run.key) else {
            self.missions.run = Some(run);
            return;
        };
        for _ in 0..n {
            run.phase = Phase::Play;
            self.skip(sim, &def, &mut run, events);
            if self.missions.run.is_some() || run.phase != Phase::Play {
                break;
            }
        }
        if self.missions.run.is_none() {
            self.missions.run = Some(run);
        }
    }

    /// Skip (after three fails): back to the checkpoint, then every step up to the next
    /// checkpoint is done for the player (they are put where it ends), and play goes on there.
    fn skip(&mut self, sim: &mut Sim, def: &MissionDef, run: &mut Run, events: &mut Vec<SimEvent>) {
        self.restore(sim, def, run, events);
        for _ in 0..def.step.len() * 2 + 4 {
            let step = def.step[run.step].clone();
            let jump = self.fast_forward(sim, def, run, &step);
            if let Some(s) = def.step.get(run.step) {
                for a in &s.on_done {
                    self.act(sim, def, run, a, events);
                }
            }
            let next = jump.unwrap_or(run.step + 1);
            if next >= def.step.len() {
                self.pass(sim, def, run, events);
                return;
            }
            run.step = next;
            run.prog = Progress::default();
            run.t = 0.0;
            if def.step[next].checkpoint {
                run.checkpoint = Some(self.save_checkpoint(sim, run));
                run.fails = 0;
                self.start_step(sim, def, run, events);
                return;
            }
            self.start_step(sim, def, run, events);
        }
    }

    /// Does a step for the player (Skip): where it ends, what it leaves done. Returns a branch's
    /// jump.
    fn fast_forward(&mut self, sim: &mut Sim, def: &MissionDef, run: &mut Run, step: &StepDef) -> Option<usize> {
        use StepKind::*;
        let target = step.target.as_deref().and_then(|t| self.anchor_at(sim, run, t));
        match step.kind {
            Goto | DriveTo | Interact | Stealth | Jump => {
                if let Some((p, yaw)) = target {
                    self.teleport_player(sim, p, yaw);
                }
            }
            EnterVehicle => {
                if let (Some(Ok(Anchor::Actor(id))), Some(pid)) = (step.target.as_deref().map(Anchor::parse), sim.state.player)
                    && let Some(car) = entity(sim, run, &id)
                {
                    if let Some(c) = self.drive.vehicle() {
                        sim.exit_vehicle(pid, c);
                    }
                    self.drive.seat(sim, pid, car);
                }
            }
            Deliver => {
                if let (Some((p, yaw)), Some(v)) = (target, step.vehicle.as_deref().and_then(|v| entity(sim, run, v))) {
                    if self.drive.vehicle() == Some(v) {
                        self.teleport_player(sim, p, yaw);
                    }
                    let side = p + Vec3::new(yaw.cos(), 0.0, -yaw.sin()) * 3.0;
                    self.put_player(sim, side, yaw);
                }
            }
            Kill | Chase | Escape | Collect => {
                let ids: Vec<String> = step.targets.iter().chain(&step.pursuers).cloned().collect();
                let ids = if step.kind == Chase {
                    step.target.as_deref().and_then(|t| t.strip_prefix("actor:")).map(|t| vec![t.to_string()]).unwrap_or_default()
                } else {
                    ids
                };
                for id in ids {
                    if step.kind == Collect {
                        if let Some(a) = run.actors.get_mut(&id) {
                            a.taken = true;
                        }
                    } else {
                        self.despawn_actor(sim, run, &id);
                    }
                }
            }
            Survive | Defend => {
                for id in step.waves.iter().flatten() {
                    self.despawn_actor(sim, run, id);
                }
            }
            LoseHeat => {
                self.heat.clear();
                self.police.stand_down();
            }
            Switch => {
                if let Some(i) = step.to.as_deref().and_then(|k| crate::protagonists::def().ok()?.index(k)) {
                    self.force_protagonist(sim, i);
                }
            }
            Choose => {
                let money = self.money;
                let pick =
                    self.missions.bot_choice.clone().filter(|c| step.options.iter().any(|o| &o.id == c && o.cost <= money));
                let first = step.options.iter().find(|o| o.cost <= money).map(|o| o.id.clone());
                let id = pick.or(first).unwrap_or_default();
                self.choose(def, run, step, &id);
            }
            Branch => return self.branch(def, run, step),
            Parallel => {
                for sub in &step.steps {
                    let _ = self.fast_forward(sim, def, run, sub);
                }
            }
            _ => {}
        }
        None
    }

    /// Where a branch goes: `to` when its flag is set, else `otherwise` (else on).
    /// An option picked on the board: its cost paid and its flag set (kept after the mission
    /// in the story, but an activity's: a bet is for that run).
    fn choose(&mut self, def: &MissionDef, run: &mut Run, step: &StepDef, id: &str) {
        if let Some(o) = step.options.iter().find(|o| o.id == id)
            && o.cost > 0
        {
            self.pay(-o.cost);
        }
        run.flags.insert(id.to_string());
        if !def.activity {
            self.missions.story.insert(id.to_string());
        }
    }

    fn branch(&self, def: &MissionDef, run: &Run, step: &StepDef) -> Option<usize> {
        let set = step.flag.as_ref().is_some_and(|f| run.flags.contains(f) || self.missions.story.contains(f));
        let label = if set { step.to.as_ref() } else { step.otherwise.as_ref() }?;
        def.step.iter().position(|s| s.label.as_ref() == Some(label))
    }

    /// A step's tick.
    #[allow(clippy::too_many_arguments)]
    fn step_tick(
        &mut self,
        sim: &mut Sim,
        def: &MissionDef,
        step: &StepDef,
        prog: &mut Progress,
        t: f32,
        run: &mut Run,
        input: &InputFrame,
        dt: f32,
        real: f32,
        events: &mut Vec<SimEvent>,
    ) -> Flow {
        use StepKind::*;
        let me = self.player_point(sim);
        let target = step.target.as_deref().and_then(|a| self.anchor_at(sim, run, a));
        let target_id = step.target.as_deref().and_then(|a| a.strip_prefix("actor:")).map(str::to_string);
        let near = |p: Vec3, r: f32| flat(p).distance(flat(me)) < r && (p.y - me.y).abs() < 4.0;
        match step.kind {
            Wait => {
                if t >= step.seconds.unwrap_or(3.0) {
                    return Flow::Done;
                }
            }
            Set => return Flow::Done,
            Cutscene | Dialogue => {
                // Cutscenes run on real time (their camera) and skip on ✕; dialogue plays over
                // the game.
                let clock = if step.kind == Cutscene { real } else { dt };
                prog.shot_t += clock;
                while prog.shot < step.shots.len() && prog.shot_t >= step.shots[prog.shot].seconds {
                    prog.shot_t -= step.shots[prog.shot].seconds;
                    prog.shot += 1;
                }
                let before = prog.line_t;
                prog.line_t += clock;
                if let Some(line) = step.lines.get(prog.line) {
                    // Babble: a syllable every so often while the line shows.
                    let voice = self.voice_of(&line.who);
                    let syl = |x: f32| (x / BABBLE) as usize;
                    if syl(prog.line_t) != syl(before) && prog.line_t < line.length() - 0.3 {
                        let k = syl(prog.line_t);
                        events.push(sound("voice.babble", 0.5, voice * (1.0 + wobble(prog.line, k) * 0.25)));
                    }
                    if prog.line_t >= line.length() {
                        prog.line_t = 0.0;
                        prog.line += 1;
                    }
                }
                let shots_done = prog.shot >= step.shots.len();
                let lines_done = prog.line >= step.lines.len();
                let skip = step.kind == Cutscene && step.skip && t > 0.3 && input.pad.pressed & (pad::SOUTH | pad::NORTH) != 0;
                if (shots_done && lines_done) || skip {
                    return Flow::Done;
                }
            }
            Goto => {
                let r = step.radius.unwrap_or(if self.drive.vehicle().is_some() { 6.0 } else { 2.5 });
                if target.is_some_and(|(p, _)| near(p, r)) {
                    return Flow::Done;
                }
            }
            EnterVehicle => {
                let car = target_id.as_deref().and_then(|id| entity(sim, run, id));
                if car.is_some() && self.drive.vehicle() == car && !self.drive.busy() {
                    return Flow::Done;
                }
                if target_id.as_deref().is_some_and(|id| down(sim, run, id)) {
                    return Flow::Fail("The vehicle was destroyed.");
                }
            }
            DriveTo => {
                let r = step.radius.unwrap_or(7.0);
                let car = self.drive.vehicle();
                let right = step.vehicle.as_deref().is_none_or(|v| entity(sim, run, v) == car);
                let slow = car.and_then(|c| crate::drive::Car::read(sim, c)).is_some_and(|c| c.vel.length() < 6.0);
                if car.is_some() && right && slow && target.is_some_and(|(p, _)| near(p, r)) {
                    return Flow::Done;
                }
            }
            Follow => {
                let Some(id) = target_id.as_deref() else { return Flow::Done };
                if down(sim, run, id) {
                    return Flow::Done;
                }
                let Some((p, _)) = actor_at(sim, run, id) else { return Flow::Done };
                let d = flat(p).distance(flat(me));
                prog.away = if d > step.distance.unwrap_or(70.0) { prog.away + dt } else { 0.0 };
                prog.near = if step.min.is_some_and(|m| d < m) { prog.near + dt } else { 0.0 };
                if prog.away > 5.0 {
                    return Flow::Fail("You lost them.");
                }
                if prog.near > 3.0 {
                    return Flow::Fail("They spotted you.");
                }
                let arrived = match entity(sim, run, id) {
                    Some(e) if sim.state.entities.get(e).is_some_and(|x| x.vehicle.is_some()) => self.traffic.arrived(e),
                    _ => run.actors.get(id).is_some_and(|a| matches!(a.act, Act::Goto(_)) && a.path.is_empty() && a.ordered),
                };
                if arrived || step.seconds.is_some_and(|s| t >= s) {
                    return Flow::Done;
                }
            }
            Chase => {
                let Some(id) = target_id.as_deref() else { return Flow::Done };
                if step.destroy {
                    if down(sim, run, id) {
                        return Flow::Done;
                    }
                } else if actor_at(sim, run, id).is_some_and(|(p, _)| near(p, step.radius.unwrap_or(2.5))) {
                    // Caught: they give up.
                    if let Some(a) = run.actors.get_mut(id) {
                        a.act = Act::Cower;
                        a.ordered = false;
                    }
                    return Flow::Done;
                } else if down(sim, run, id) {
                    return Flow::Fail("They are dead.");
                }
                let d = actor_at(sim, run, id).map_or(0.0, |(p, _)| flat(p).distance(flat(me)));
                prog.away = if d > step.distance.unwrap_or(220.0) { prog.away + dt } else { 0.0 };
                if prog.away > 6.0 {
                    return Flow::Fail("They got away.");
                }
            }
            Escape => {
                let far = step.distance.unwrap_or(160.0);
                if step
                    .pursuers
                    .iter()
                    .all(|p| down(sim, run, p) || actor_at(sim, run, p).is_none_or(|(q, _)| flat(q).distance(flat(me)) > far))
                {
                    return Flow::Done;
                }
            }
            LoseHeat => {
                if self.heat.stars <= step.stars.unwrap_or(0) {
                    return Flow::Done;
                }
            }
            Kill => {
                for (i, id) in step.targets.iter().enumerate() {
                    if let Some(d) = prog.done.get_mut(i) {
                        *d = down(sim, run, id);
                    }
                }
                if prog.done.iter().all(|d| *d) {
                    return Flow::Done;
                }
            }
            Survive | Defend => {
                if step.kind == Defend
                    && let Some(id) = target_id.as_deref()
                    && down(sim, run, id)
                {
                    return Flow::Fail("They didn't make it.");
                }
                // The next wave once this one is down.
                let wave_down = step.waves.get(prog.wave).is_none_or(|w| w.iter().all(|id| down(sim, run, id)));
                if wave_down && prog.wave + 1 < step.waves.len() {
                    prog.wave += 1;
                    for id in step.waves[prog.wave].clone() {
                        self.act(sim, def, run, &Action::Spawn(id.clone()), events);
                        self.act(sim, def, run, &Action::Behaviour { actor: id, act: "fight".into() }, events);
                    }
                }
                let cleared = prog.wave + 1 >= step.waves.len() && wave_down;
                let done = match step.seconds {
                    Some(s) => t >= s,
                    None => cleared,
                };
                if done {
                    return Flow::Done;
                }
            }
            Collect => {
                for (i, id) in step.targets.iter().enumerate() {
                    let reach = if self.drive.vehicle().is_some() { 3.5 } else { 1.8 };
                    let got = run.actors.get(id).is_some_and(|a| a.taken);
                    if !got && actor_at(sim, run, id).is_some_and(|(p, _)| near(p, reach)) {
                        if let Some(a) = run.actors.get_mut(id) {
                            a.taken = true;
                        }
                        events.push(sound("ui.pickup", 1.0, 1.0));
                    }
                    if let Some(d) = prog.done.get_mut(i) {
                        *d = run.actors.get(id).is_none_or(|a| a.taken);
                    }
                }
                if prog.done.iter().all(|d| *d) {
                    return Flow::Done;
                }
            }
            Photograph => {
                if self.phone.photos > run.photos {
                    for (i, id) in step.targets.iter().enumerate() {
                        if let Some(e) = entity(sim, run, id)
                            && self.phone.subjects.contains(&e)
                            && let Some(d) = prog.done.get_mut(i)
                        {
                            *d = true;
                        }
                    }
                    run.photos = self.phone.photos;
                }
                if prog.done.iter().all(|d| *d) {
                    return Flow::Done;
                }
            }
            Interact => {
                let close = target.is_some_and(|(p, _)| near(p, step.radius.unwrap_or(2.2))) && self.drive.vehicle().is_none();
                let hold = input.pad.held & pad::RIGHT != 0;
                let secs = step.seconds.unwrap_or(1.5).max(0.1);
                prog.meter = if close && hold { prog.meter + dt / secs } else { (prog.meter - dt).max(0.0) };
                if prog.meter >= 1.0 {
                    events.push(sound("ui.accept", 0.8, 1.0));
                    return Flow::Done;
                }
            }
            Switch => {
                let to = step.to.as_deref().and_then(|k| crate::protagonists::def().ok()?.index(k));
                if to == Some(self.crew.current) && self.crew.switch.is_none() {
                    return Flow::Done;
                }
            }
            Race => return self.race_tick(sim, run, step, prog),
            Jump => {
                // Airborne at some point in the step, then down on the wheels near the landing.
                let car = self.drive.vehicle();
                let st = car.and_then(|c| sim.vehicle_status(c));
                if st.as_ref().is_some_and(|s| s.airborne) {
                    prog.meter = 1.0;
                }
                let down = st.as_ref().is_some_and(|s| !s.airborne);
                if prog.meter >= 1.0 && down && target.is_some_and(|(p, _)| near(p, step.radius.unwrap_or(16.0))) {
                    events.push(sound("mission.step", 0.8, 1.3));
                    return Flow::Done;
                }
                if car.is_some_and(|c| down && prog.meter >= 1.0 && sim.vehicle_status(c).is_some_and(|s| s.wrecked)) {
                    return Flow::Fail("The car didn't make it.");
                }
            }
            Stealth => {
                let mut seen = false;
                for w in &step.watchers {
                    if self.watcher_sees(sim, run, w) {
                        seen = true;
                    }
                }
                prog.meter = if seen {
                    prog.meter + dt * if self.foot.crouched { 0.8 } else { 1.6 }
                } else {
                    (prog.meter - dt * 0.4).max(0.0)
                };
                if prog.meter >= 1.0 {
                    run.stats.spotted = true;
                    return Flow::Fail("You were spotted.");
                }
                if target.is_some_and(|(p, _)| near(p, step.radius.unwrap_or(3.0))) {
                    return Flow::Done;
                }
            }
            Deliver => {
                let Some(v) = step.vehicle.as_deref() else { return Flow::Done };
                if down(sim, run, v) {
                    return Flow::Fail("The vehicle was destroyed.");
                }
                let r = step.radius.unwrap_or(7.0);
                let there = actor_at(sim, run, v).zip(target).is_some_and(|((p, _), (q, _))| flat(p).distance(flat(q)) < r);
                let out = self.drive.vehicle().is_none() && !self.drive.busy();
                if there && out {
                    return Flow::Done;
                }
            }
            Holdup => {
                let Some(id) = target_id.as_deref() else { return Flow::Done };
                if down(sim, run, id) {
                    return Flow::Fail("The clerk is dead.");
                }
                let gun = crate::combat::def()
                    .ok()
                    .and_then(|d| d.weapon.get(self.arm_index()).map(|w| w.kind == crate::combat::Kind::Gun))
                    .unwrap_or(false);
                let aimed = gun
                    && self.combat.aiming
                    && actor_at(sim, run, id).is_some_and(|(p, _)| {
                        let cam = self.cam.intent();
                        let look = flat(cam.target - cam.eye).normalize_or_zero();
                        let to = flat(p) - flat(me);
                        to.length() < 9.0 && to.normalize_or_zero().dot(look) > 15f32.to_radians().cos()
                    });
                if aimed {
                    let close = actor_at(sim, run, id).is_some_and(|(p, _)| near(p, 4.0));
                    prog.meter += dt / step.seconds.unwrap_or(6.0).max(0.5) * if close { 2.0 } else { 1.0 };
                    if let Some(a) = run.actors.get_mut(id)
                        && a.act != Act::Cower
                    {
                        a.act = Act::Cower;
                        a.ordered = false;
                    }
                }
                if prog.meter >= 1.0 {
                    events.push(sound("ui.cash", 1.0, 1.0));
                    return Flow::Done;
                }
            }
            Choose => {
                let n = step.options.len().max(1);
                let (dx, _) = run.nav.step(input, real);
                let money = self.money;
                let affordable = |i: usize| step.options.get(i).is_some_and(|o| o.cost <= money);
                prog.focus = crate::pad::move_focus(prog.focus, n, dx, affordable);
                if !affordable(prog.focus)
                    && let Some(i) = (0..n).find(|&i| affordable(i))
                {
                    prog.focus = i;
                }
                for c in &input.cmds {
                    if let Some(id) = c.strip_prefix("ui.focus ").or(c.strip_prefix("ui.accept "))
                        && let Some(i) = step.options.iter().position(|o| o.id == id)
                    {
                        prog.focus = i;
                    }
                }
                // (The world stands still on the board: its clock is real time.)
                prog.shot_t += real;
                let accept = input.pad.pressed & pad::SOUTH != 0 || input.cmds.iter().any(|c| c.starts_with("ui.accept "));
                if accept && prog.shot_t > 0.4 && affordable(prog.focus.min(n - 1)) {
                    let id = step.options[prog.focus.min(n - 1)].id.clone();
                    self.choose(def, run, step, &id);
                    events.push(sound("ui.accept", 1.0, 1.0));
                    return Flow::Done;
                }
            }
            Branch => {
                return match self.branch(def, run, step) {
                    Some(i) => Flow::Jump(i),
                    None => Flow::Done,
                };
            }
            Parallel => {
                let cur = self.crew.current;
                let mut subs = std::mem::take(&mut run.subs);
                for (k, sub) in subs.iter_mut().enumerate() {
                    if sub.done || sub.who != cur || self.crew.switch.is_some() {
                        continue;
                    }
                    let Some(s) = step.steps.get(k) else { continue };
                    if !sub.prog.started {
                        sub.prog.started = true;
                        for a in &s.on_start {
                            self.act(sim, def, run, a, events);
                        }
                    }
                    sub.t += dt;
                    let mut p = std::mem::take(&mut sub.prog);
                    let f = self.step_tick(sim, def, s, &mut p, sub.t, run, input, dt, real, events);
                    sub.prog = p;
                    match f {
                        Flow::Done => {
                            sub.done = true;
                            for a in &s.on_done {
                                self.act(sim, def, run, a, events);
                            }
                            events.push(sound("mission.step", 0.6, 1.2));
                        }
                        Flow::Fail(why) => {
                            run.subs = subs;
                            return Flow::Fail(why);
                        }
                        _ => {}
                    }
                }
                let all = subs.iter().all(|s| s.done);
                run.subs = subs;
                if all {
                    return Flow::Done;
                }
            }
        }
        Flow::On
    }

    /// A race: the player through the gates in order, lap by lap; the racers make for theirs.
    fn race_tick(&mut self, sim: &mut Sim, run: &mut Run, step: &StepDef, prog: &mut Progress) -> Flow {
        let laps = step.laps.unwrap_or(1).max(1);
        let r = step.radius.unwrap_or(12.0);
        let me = self.player_point(sim);
        if let Some((p, _)) = step.gates.get(prog.gate).and_then(|g| self.anchor_at(sim, run, g))
            && flat(p).distance(flat(me)) < r
            && self.drive.vehicle().is_some()
        {
            prog.gate += 1;
            if prog.gate >= step.gates.len() {
                prog.gate = 0;
                prog.lap += 1;
            }
            if prog.lap >= laps {
                return Flow::Done;
            }
        }
        // The racers: on to the next gate when they reach theirs; one finishing first wins.
        let gates: Vec<Option<Vec3>> = step.gates.iter().map(|g| self.anchor_at(sim, run, g).map(|x| x.0)).collect();
        for id in &step.racers {
            let Some(car) = entity(sim, run, id) else { continue };
            let Some((p, _)) = actor_at(sim, run, id) else { continue };
            let Some(a) = run.actors.get_mut(id) else { continue };
            if let Some(Some(g)) = gates.get(a.gate)
                && flat(*g).distance(flat(p)) < r
            {
                a.gate += 1;
                if a.gate >= gates.len() {
                    a.gate = 0;
                    a.lap += 1;
                }
                a.ordered = false;
            }
            if a.lap >= laps {
                return Flow::Fail("You lost the race.");
            }
            if !a.ordered
                && let (Some(Some(g)), Some(city)) = (gates.get(a.gate), self.city.clone())
            {
                let driver = crate::traffic::occupants(sim, car).first().copied();
                // A racer drives like a getaway driver (through lights, past cars), a little
                // slower than the limits allow.
                self.traffic.command(sim, &city, car, driver, Some(*g), Some(*g), 0.9);
                let then = gates.get((a.gate + 1) % gates.len()).copied().flatten();
                self.traffic.drive_through(Some(car), then);
                a.ordered = true;
            }
            // Moderate rubber-banding: well ahead of the player a racer eases off, well behind it
            // pushes.
            let lead = race_score(&gates, a.lap, a.gate, p) - race_score(&gates, prog.lap, prog.gate, me);
            let desire = if lead > RUBBER {
                0.66
            } else if lead < -RUBBER {
                1.0
            } else {
                0.82
            };
            self.traffic.urge(car, desire);
        }
        Flow::On
    }

    /// The player's place in the race under way (1 = leading) and the field's size.
    pub(crate) fn race_place(&self, sim: &Sim, run: &Run, step: &StepDef, prog: &Progress) -> (usize, usize) {
        let gates: Vec<Option<Vec3>> = step.gates.iter().map(|g| self.anchor_at(sim, run, g).map(|x| x.0)).collect();
        let mine = race_score(&gates, prog.lap, prog.gate, self.player_point(sim));
        let ahead = step
            .racers
            .iter()
            .filter(|id| {
                let Some((p, _)) = actor_at(sim, run, id) else { return false };
                run.actors.get(*id).is_some_and(|a| race_score(&gates, a.lap, a.gate, p) > mine)
            })
            .count();
        (ahead + 1, step.racers.len() + 1)
    }

    /// The actors' behaviours: inputs for the people, orders for the vehicles' drivers.
    fn drive_actors(
        &mut self,
        sim: &mut Sim,
        def: &MissionDef,
        run: &mut Run,
        out: &mut BTreeMap<EntityId, InputFrame>,
        dt: f32,
        events: &mut Vec<SimEvent>,
    ) {
        let Some(city) = self.city.clone() else { return };
        let me = self.player_point(sim);
        let racing = def.step.get(run.step).is_some_and(|s| s.kind == StepKind::Race);
        let ids: Vec<String> = run.actors.keys().cloned().collect();
        for id in ids {
            if down(sim, run, &id) {
                continue;
            }
            let Some(e) = entity(sim, run, &id) else { continue };
            if Some(e) == sim.state.player || self.enemies.is_enemy(e) {
                continue;
            }
            let is_car = sim.state.entities.get(e).is_some_and(|x| x.vehicle.is_some());
            let racer = racing && def.step[run.step].racers.contains(&id);
            let Some(a) = run.actors.get_mut(&id) else { continue };
            a.timer -= dt;
            if is_car {
                if racer {
                    continue;
                }
                let driver = crate::traffic::occupants(sim, e).first().copied();
                match a.act.clone() {
                    Act::Idle | Act::Cower => {
                        if !a.ordered {
                            self.traffic.dismiss(e);
                            a.ordered = true;
                        }
                    }
                    _ if driver.is_none() => {}
                    Act::Goto(to) => {
                        if !a.ordered {
                            let goal = Anchor::parse(&to).ok().and_then(|x| match x {
                                Anchor::Actor(_) | Anchor::Player => None,
                                x => anchor::point(&city, &x),
                            });
                            self.traffic.command(sim, &city, e, driver, goal, None, 0.85);
                            a.ordered = true;
                        }
                    }
                    Act::Flee => {
                        if !a.ordered {
                            self.traffic.command(sim, &city, e, driver, None, Some(me), 0.75);
                            a.ordered = true;
                        }
                        self.traffic.run_from(e, me);
                    }
                    Act::Chase | Act::Follow | Act::Fight => {
                        if !a.ordered || a.timer <= 0.0 {
                            self.traffic.command(sim, &city, e, driver, Some(me), None, 0.8);
                            a.ordered = true;
                            a.timer = 2.0;
                        }
                    }
                    Act::Wander | Act::Guard => {
                        if !a.ordered {
                            self.traffic.command(sim, &city, e, driver, None, None, 0.9);
                            a.ordered = true;
                        }
                    }
                }
                continue;
            }
            // People.
            let Some(feet) = sim.feet(e) else { continue };
            if sim.state.entities.get(e).and_then(|x| x.character.as_ref()).is_some_and(|c| c.riding.is_some()) {
                continue;
            }
            let here = flat(feet);
            let to_me = flat(me) - here;
            let mut f = InputFrame::default();
            let mut haste = 0.0;
            match a.act.clone() {
                Act::Idle | Act::Guard | Act::Fight => {}
                Act::Cower => {
                    if !a.ordered {
                        crate::play::play_clip(sim, e, "WORKSHOP/Hands_Up", 0);
                        a.ordered = true;
                    }
                }
                Act::Flee => {
                    let away = (-to_me).normalize_or(Vec2::X);
                    // Along the walk map's ground where it can (round buildings).
                    let ahead = here + away * 6.0;
                    let dir = match city.walk.nearest_walkable(ahead, 3) {
                        Some(w) => (w - here).normalize_or(away),
                        None => away,
                    };
                    f.move_dir = dir;
                    haste = 0.35;
                }
                Act::Chase => {
                    if to_me.length() > 1.3 {
                        f.move_dir = to_me.normalize_or_zero();
                        haste = 0.2;
                    } else if a.timer <= 0.0 && self.drive.vehicle().is_none() {
                        a.timer = 0.9;
                        crate::play::play_clip(sim, e, "QUATERNIUS/Punch_Jab", pav_core::clips::ONCE);
                        self.combat.hurt_player(6.0);
                        events.push(SimEvent::Sound { cue: "melee.hit".into(), pos: Some(feet), gain: 1.0, pitch: 1.0 });
                    }
                }
                Act::Follow => {
                    if to_me.length() > 4.0 {
                        f.move_dir = to_me.normalize_or_zero();
                        if to_me.length() < 8.0 {
                            f.held |= buttons::FOCUS;
                        }
                    }
                }
                Act::Wander => {
                    if a.path.is_empty() || a.timer <= 0.0 {
                        a.timer = 6.0;
                        let k = (e.0 as usize).wrapping_mul(31) + (run.elapsed as usize);
                        let ang = wobble(k, 7) * std::f32::consts::TAU;
                        let goal = here + Vec2::new(ang.cos(), ang.sin()) * 10.0;
                        a.path = city.walk.nearest_walkable(goal, 4).map(|g| vec![g]).unwrap_or_default();
                    }
                    if let Some(&p) = a.path.first() {
                        if p.distance(here) < 1.0 {
                            a.path.clear();
                        } else {
                            f.move_dir = (p - here).normalize_or_zero();
                            f.held |= buttons::FOCUS;
                        }
                    }
                }
                Act::Goto(to) => {
                    if !a.ordered {
                        a.ordered = true;
                        let goal = Anchor::parse(&to).ok().and_then(|x| match x {
                            Anchor::Actor(_) | Anchor::Player => None,
                            x => anchor::point(&city, &x),
                        });
                        if let Some(g) = goal {
                            let (s, t) = (city.walk.nearest_walkable(here, 6), city.walk.nearest_walkable(flat(g), 8));
                            let mut path = match (s, t) {
                                (Some(s), Some(t)) => city.walk.path(s, t, 6000),
                                _ => Vec::new(),
                            };
                            path.push(flat(g));
                            a.path = path;
                        }
                    }
                    while a.path.len() > 1 && a.path[0].distance(here) < 1.0 {
                        a.path.remove(0);
                    }
                    if let Some(&p) = a.path.first() {
                        if a.path.len() == 1 && p.distance(here) < 1.2 {
                            a.path.clear();
                        } else {
                            f.move_dir = (p - here).normalize_or_zero();
                        }
                    }
                }
            }
            if let Some(c) = sim.state.entities.get_mut(e).and_then(|x| x.character.as_mut()) {
                c.haste = haste;
            }
            out.insert(e, f);
        }
    }

    /// A speaker's babble pitch: protagonists' from their profiles, others by their name.
    fn voice_of(&self, who: &str) -> f32 {
        if let Ok(d) = crate::protagonists::def()
            && let Some(i) = d.index(who)
        {
            return d.protagonist[i].voice;
        }
        0.9 + (who.bytes().map(|b| b as usize).sum::<usize>() % 7) as f32 * 0.06
    }

    /// A speaker's name on the subtitles.
    fn speaker_name(def: &MissionDef, who: &str) -> String {
        if let Some(n) = def.names.get(who) {
            return n.clone();
        }
        if let Ok(d) = crate::protagonists::def()
            && let Some(i) = d.index(who)
        {
            return d.protagonist[i].short.clone();
        }
        who.to_uppercase()
    }

    /// Mission markers in free roam: the letters on the map and a ring at each pickup point;
    /// walking or driving into one starts its mission.
    fn mission_markers(&mut self, sim: &mut Sim, events: &mut Vec<SimEvent>) {
        if self.missions.cooldown > 0.0
            || !self.fate.alive()
            || self.heat.stars > 0
            || self.crew.switch.is_some()
            || self.mode.starts_with("mission/")
            || self.street_busy()
            || self.attract.on
        {
            return;
        }
        let me = self.player_point(sim);
        let reach = if self.drive.vehicle().is_some() { 4.5 } else { 2.2 };
        let hit = self.open_missions().into_iter().find(|(_, p)| flat(*p).distance(flat(me)) < reach && (p.y - me.y).abs() < 3.0);
        if let Some((key, _)) = hit
            && let Err(e) = self.start_mission(sim, &key, false, false, events)
        {
            log::warn!("{e:#}");
        }
    }

    /// The missions that may be picked up in this district now, and where.
    pub fn open_missions(&self) -> Vec<(String, Vec3)> {
        let Some(city) = self.city.as_ref() else { return Vec::new() };
        let district = if self.mode == "dev" { "dev" } else { "" };
        super::all()
            .iter()
            .filter(|d| d.district == district && self.missions.open(d))
            .filter_map(|d| {
                let b = d.blip.as_ref()?;
                let p = anchor::point(city, &Anchor::parse(&b.at).ok()?)?;
                Some((d.key.clone(), p))
            })
            .collect()
    }

    /// What the mission under way shows: the cards, the objective, subtitles, markers, blips,
    /// rings on the ground, the timer and meters, the letterbox.
    pub(crate) fn mission_ui(&self, sim: &Sim, ui: &mut UiModel, draw: &mut DrawList) {
        // Markers for missions to pick up (free roam; none while vigilante or an event is on).
        if self.missions.run.is_none() {
            if self.street_busy() {
                return;
            }
            for (key, p) in self.open_missions() {
                let Ok(d) = super::get(&key) else { continue };
                let color = if d.activity { Color::hex(ACTIVITY_PINK) } else { self.protagonist_color(&d.protagonist) };
                if p.distance(Self::feet(sim)) < 150.0 {
                    draw.prims.push(ring(p, 1.6, color));
                }
                if let Some(hud) = &mut ui.hud
                    && let Some(m) = &mut hud.minimap
                {
                    m.blips.push(Blip {
                        pos: flat(p),
                        icon: d.blip.as_ref().map_or(BlipIcon::Letter('?'), |b| b.map_icon()),
                        color,
                        label: Some(d.title.clone()),
                        clamp: true,
                        height: 0,
                        pulse: false,
                    });
                }
            }
            return;
        }
        let Some(run) = &self.missions.run else { return };
        let Ok(def) = super::get(&run.key) else { return };
        let hint = |b: PadButton, l: &str| Prompt { button: b, label: l.into(), hold: None };
        match &run.phase {
            Phase::Intro { t } => {
                let lines: Vec<String> = def.medals.iter().map(|m| format!("★ {}", m.text)).collect();
                let who = crate::protagonists::def()
                    .ok()
                    .and_then(|c| c.index(&def.protagonist).map(|i| c.protagonist[i].name.clone()));
                ui.screen = Some(Screen {
                    id: "mission/intro".into(),
                    title: def.title.to_uppercase(),
                    tabs: Vec::new(),
                    tab: 0,
                    hints: vec![hint(PadButton::South, "Start")],
                    body: ScreenBody::Cards(CardRow {
                        cards: vec![Card {
                            id: def.key.clone(),
                            title: def.title.to_uppercase(),
                            subtitle: who,
                            lines: if lines.is_empty() { vec![def.text.clone()] } else { lines },
                            bars: Vec::new(),
                            accent: self.protagonist_color(&def.protagonist),
                            icon: Some("star".into()),
                            enabled: true,
                        }],
                        focus: 0,
                    }),
                    backdrop: Backdrop::Dim,
                });
                ui.overlay.fade = (1.0 - t / 0.5).clamp(0.0, 1.0) * 0.6;
                if let Some(h) = &mut ui.hud {
                    h.opacity = 0.0;
                }
                ui.help = None;
                ui.prompts.clear();
                return;
            }
            Phase::Failed { t } => {
                // The banner first; the card takes over as it goes, the reason under Retry.
                ui.banners.push(Banner {
                    kind: BannerKind::Failed,
                    title: "MISSION FAILED".into(),
                    subtitle: Some(run.reason.clone()),
                    age: *t,
                    duration: FAIL_BANNER,
                });
                ui.overlay.desaturate = (t / 1.0).clamp(0.0, 0.8);
                if *t > FAIL_BANNER {
                    let items = fail_items(run.fails)
                        .iter()
                        .map(|(id, label)| MenuItem {
                            id: (*id).into(),
                            label: (*label).into(),
                            value: ItemValue::Button,
                            enabled: true,
                            note: match *id {
                                "retry" => Some(run.reason.clone()),
                                "skip" => Some("Move on to the next part".into()),
                                _ => None,
                            },
                        })
                        .collect();
                    ui.screen = Some(Screen {
                        id: "mission/failed".into(),
                        title: "MISSION FAILED".into(),
                        tabs: Vec::new(),
                        tab: 0,
                        hints: vec![hint(PadButton::South, "Select")],
                        body: ScreenBody::Menu(MenuList { items, focus: run.menu }),
                        backdrop: Backdrop::Dim,
                    });
                }
                if let Some(h) = &mut ui.hud {
                    h.opacity = 0.0;
                }
                ui.help = None;
                ui.prompts.clear();
                return;
            }
            Phase::Passed { t } => {
                let medal = run.medal.map(|m| match m {
                    Medal::Gold => "Gold",
                    Medal::Silver => "Silver",
                    Medal::Bronze => "Bronze",
                });
                ui.banners.push(Banner {
                    kind: BannerKind::Passed,
                    title: "MISSION PASSED".into(),
                    subtitle: medal.map(|m| format!("{m} medal")),
                    age: *t,
                    duration: PASS_BANNER,
                });
                if *t > PASS_BANNER {
                    let mut rows = vec![("Time".to_string(), clock_text(run.elapsed))];
                    for (m, ok) in def.medals.iter().zip(&run.met) {
                        rows.push((m.text.clone(), if *ok { "Done".into() } else { "Missed".into() }));
                    }
                    rows.truncate(5);
                    ui.screen = Some(Screen {
                        id: "mission/results".into(),
                        title: "MISSION PASSED".into(),
                        tabs: Vec::new(),
                        tab: 0,
                        hints: vec![hint(PadButton::South, "Continue")],
                        body: ScreenBody::Results(Results {
                            heading: def.title.to_uppercase(),
                            medal: run.medal,
                            rows,
                            cash: (run.paid != 0).then_some(run.paid),
                        }),
                        backdrop: Backdrop::Dim,
                    });
                }
                if let Some(h) = &mut ui.hud {
                    h.opacity = 0.0;
                }
                ui.help = None;
                ui.prompts.clear();
                return;
            }
            Phase::Play => {}
        }
        let Some(step) = def.step.get(run.step) else { return };
        // The step the player acts on: a parallel step's sub-step for whoever is played.
        let (step, prog) = match step.kind {
            StepKind::Parallel => match run.subs.iter().position(|s| s.who == self.crew.current && !s.done) {
                Some(k) => (&step.steps[k], &run.subs[k].prog),
                None => (step, &run.prog),
            },
            _ => (step, &run.prog),
        };
        let objective = match def.step[run.step].kind {
            StepKind::Parallel if run.subs.iter().any(|s| s.who == self.crew.current && !s.done) => step.objective.clone(),
            StepKind::Parallel => Some("Switch to the others: ~btn:down~ opens the switch wheel.".into()),
            _ => step.objective.clone(),
        };
        if let Some(h) = &mut ui.hud {
            h.objective = objective;
        }
        if let Some((text, age)) = &run.help {
            ui.help = Some(Help { text: text.clone(), age: *age });
        }
        // Subtitles.
        if matches!(step.kind, StepKind::Cutscene | StepKind::Dialogue)
            && let Some(line) = step.lines.get(prog.line)
        {
            ui.subtitle = Some(Subtitle {
                speaker: Some(Self::speaker_name(&def, &line.who)),
                text: line.text.clone(),
                color: Some(self.protagonist_color(&line.who)),
            });
        }
        if step.kind == StepKind::Cutscene {
            ui.overlay.letterbox = (run.t / 0.4).clamp(0.0, 1.0);
            if let Some(h) = &mut ui.hud {
                h.opacity = 0.0;
            }
            ui.prompts = if step.skip { vec![hint(PadButton::South, "Skip")] } else { Vec::new() };
            ui.markers.clear();
            return;
        }
        // The timer of a time limit.
        let limit = step
            .fail
            .iter()
            .filter_map(|f| if let When::Timer(s) = f.when { Some((s - run.t, f.reason.clone())) } else { None })
            .chain(
                def.fail
                    .iter()
                    .filter_map(|f| if let When::Timer(s) = f.when { Some((s - run.elapsed, f.reason.clone())) } else { None }),
            )
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let (Some((left, _)), Some(h)) = (limit, &mut ui.hud) {
            h.timer = Some(TimerHud { label: "TIME".into(), seconds: left.max(0.0), warn: left < 10.0 });
        }
        // Meters.
        if let Some(h) = &mut ui.hud {
            match step.kind {
                StepKind::Stealth => {
                    h.meters.push(Meter { label: "DETECTION".into(), value: prog.meter.min(1.0), color: Color::hex("#ff4d4d") })
                }
                StepKind::Holdup => {
                    h.meters.push(Meter { label: "BAG".into(), value: prog.meter.min(1.0), color: Color::hex("#6fe28a") })
                }
                StepKind::Race => h.meters.push(Meter {
                    label: {
                        let (place, of) = self.race_place(sim, run, step, prog);
                        format!(
                            "LAP {}/{}  ·  POS {place}/{of}",
                            (prog.lap + 1).min(step.laps.unwrap_or(1)),
                            step.laps.unwrap_or(1)
                        )
                    },
                    value: prog.gate as f32 / step.gates.len().max(1) as f32,
                    color: Color::hex("#ffc145"),
                }),
                _ => {}
            }
        }
        if step.kind == StepKind::Interact {
            let close = step
                .target
                .as_deref()
                .and_then(|a| self.anchor_at(sim, run, a))
                .is_some_and(|(p, _)| flat(p).distance(flat(Self::feet(sim))) < step.radius.unwrap_or(2.2));
            if close {
                ui.prompts.push(Prompt {
                    button: PadButton::Right,
                    label: step.prompt.clone().unwrap_or_else(|| "Hold".into()),
                    hold: Some(prog.meter.min(1.0)),
                });
            }
        }
        // Where to go: the destination's ring, marker and blip.
        let dest: Option<(Vec3, Color, BlipIcon, Option<String>)> = match step.kind {
            StepKind::Goto | StepKind::DriveTo | StepKind::Interact | StepKind::Stealth | StepKind::Deliver => step
                .target
                .as_deref()
                .and_then(|a| self.anchor_at(sim, run, a))
                .map(|(p, _)| (p, Color::hex("#ffd23f"), BlipIcon::Objective, None)),
            StepKind::Race => step
                .gates
                .get(prog.gate)
                .and_then(|g| self.anchor_at(sim, run, g))
                .map(|(p, _)| (p, Color::hex("#ffc145"), BlipIcon::Race, Some("Gate".into()))),
            _ => None,
        };
        if let Some((p, color, icon, label)) = dest {
            let on_road = matches!(step.kind, StepKind::DriveTo | StepKind::Race | StepKind::Deliver);
            draw.prims.push(ring(p, if on_road { step.radius.unwrap_or(6.0).min(5.0) } else { 1.2 }, color));
            ui.markers.push(ScreenMarker { pos: p + Vec3::Y * 2.0, icon: BlipIcon::Objective, color, label, distance: true });
            if let Some(h) = &mut ui.hud
                && let Some(m) = &mut h.minimap
            {
                m.blips.push(Blip { pos: flat(p), icon, color, label: None, clamp: true, height: 0, pulse: true });
            }
        }
        // Actors with blips, the targets of the step.
        let targets: Vec<&String> = step.targets.iter().chain(&step.pursuers).chain(&step.watchers).collect();
        let target_actor = step.target.as_deref().and_then(|t| t.strip_prefix("actor:"));
        for (id, a) in &run.actors {
            // (Not the car the player sits in.)
            if down(sim, run, id) || (a.entity.is_some() && a.entity == self.drive.vehicle()) {
                continue;
            }
            let kind = if targets.contains(&id) || Some(id.as_str()) == target_actor {
                a.blip.clone().or(Some("target".into()))
            } else {
                a.blip.clone()
            };
            let Some(kind) = kind else { continue };
            let Some((p, _)) = actor_at(sim, run, id) else { continue };
            let (icon, hex) = match kind.as_str() {
                "enemy" => (BlipIcon::Enemy, "#e8423f"),
                "ally" => (BlipIcon::Ally, "#3dbd5d"),
                "vehicle" => (BlipIcon::Vehicle, "#3d8bff"),
                "pickup" => (BlipIcon::Collectible, "#6fe28a"),
                _ => (BlipIcon::Objective, "#ffd23f"),
            };
            let color = Color::hex(hex);
            if let Some(h) = &mut ui.hud
                && let Some(m) = &mut h.minimap
            {
                m.blips.push(Blip { pos: flat(p), icon, color, label: None, clamp: kind != "enemy", height: 0, pulse: false });
            }
            if a.entity.is_none() {
                // A pickup: a glowing box where it lies.
                draw.prims.push(PrimDraw {
                    shape: pav_core::shape::Shape::Box { half: Vec3::splat(0.25) },
                    pos: p + Vec3::Y * (0.5 + (self.ui_time * 2.0).sin() * 0.08),
                    rot: Quat::from_rotation_y(self.ui_time),
                    color,
                    emissive: 1.5,
                    look: pav_core::shape::Look::Unlit,
                    shadow: false,
                    glow: Default::default(),
                });
            } else if kind != "enemy" {
                let label = def.spawn.get(a.spawn).and_then(|s| s.name.clone());
                let car = a.entity.and_then(|e| sim.state.entities.get(e)).is_some_and(|e| e.vehicle.is_some());
                let lift = if car { 2.4 } else { 2.2 };
                ui.markers.push(ScreenMarker { pos: p + Vec3::Y * lift, icon, color, label, distance: true });
            }
        }
        // Watchers' cones on the radar.
        if step.kind == StepKind::Stealth
            && let Some(h) = &mut ui.hud
            && let Some(m) = &mut h.minimap
        {
            for w in &step.watchers {
                if let Some((p, facing)) = actor_at(sim, run, w).filter(|_| !down(sim, run, w)) {
                    m.cones.push(ConeBlip {
                        pos: flat(p),
                        dir: PI - facing,
                        angle: 100f32.to_radians(),
                        range: 18.0,
                        color: Color::hex("#ff4d4d"),
                    });
                }
            }
        }
        // The planning board.
        if step.kind == StepKind::Choose {
            ui.screen = Some(Screen {
                id: "mission/board".into(),
                title: def.title.to_uppercase(),
                tabs: Vec::new(),
                tab: 0,
                hints: vec![hint(PadButton::Left, "Look"), hint(PadButton::South, "Choose")],
                body: ScreenBody::Cards(CardRow {
                    cards: step
                        .options
                        .iter()
                        .map(|o| Card {
                            id: o.id.clone(),
                            title: o.title.to_uppercase(),
                            subtitle: (o.cost > 0).then(|| format!("${}", o.cost)),
                            lines: o.lines.clone(),
                            bars: Vec::new(),
                            accent: Color::hex("#ffc145"),
                            icon: Some("jobs".into()),
                            enabled: o.cost <= self.money,
                        })
                        .collect(),
                    focus: prog.focus,
                }),
                backdrop: Backdrop::Dim,
            });
        }
    }

    /// Where the GPS points during a mission: the step's destination or target (moving ones on a
    /// 10 m grid, so the route is not planned anew every tick).
    pub(crate) fn mission_waypoint(&self, sim: &Sim) -> Option<Vec2> {
        let run = self.missions.run.as_ref()?;
        if run.phase != Phase::Play {
            return None;
        }
        let def = super::get(&run.key).ok()?;
        let top = def.step.get(run.step)?;
        let (step, prog) = match top.kind {
            StepKind::Parallel => {
                let k = run.subs.iter().position(|s| s.who == self.crew.current && !s.done)?;
                (&top.steps[k], &run.subs[k].prog)
            }
            _ => (top, &run.prog),
        };
        use StepKind::*;
        let p = match step.kind {
            Goto | DriveTo | Interact | Stealth | Deliver | Follow | Chase | EnterVehicle | Holdup | Defend => {
                self.anchor_at(sim, run, step.target.as_deref()?)?.0
            }
            Race => self.anchor_at(sim, run, step.gates.get(prog.gate)?)?.0,
            Collect => {
                let me = self.player_point(sim);
                step.targets
                    .iter()
                    .filter(|t| run.actors.get(*t).is_some_and(|a| !a.taken))
                    .filter_map(|t| actor_at(sim, run, t).map(|x| x.0))
                    .min_by(|a, b| a.distance(me).total_cmp(&b.distance(me)))?
            }
            _ => return None,
        };
        let q = |v: f32| (v / 10.0).round() * 10.0;
        Some(Vec2::new(q(p.x), q(p.z)))
    }

    /// The cutscene's camera (None outside cutscenes).
    pub(crate) fn mission_camera(&self, sim: &Sim) -> Option<CameraIntent> {
        let run = self.missions.run.as_ref()?;
        if run.phase != Phase::Play {
            return None;
        }
        let def = super::get(&run.key).ok()?;
        let step = def.step.get(run.step)?;
        if step.kind != StepKind::Cutscene || step.shots.is_empty() {
            return None;
        }
        let k = run.prog.shot.min(step.shots.len() - 1);
        let shot = &step.shots[k];
        let (at, facing) = self.anchor_at(sim, run, &shot.anchor)?;
        let u = (run.prog.shot_t / shot.seconds.max(0.01)).clamp(0.0, 1.0);
        let off = Vec3::from(shot.offset).lerp(Vec3::from(shot.to.unwrap_or(shot.offset)), u * u * (3.0 - 2.0 * u));
        let rot = Quat::from_rotation_y(facing);
        let eye = at + rot * Vec3::new(-off.x, off.y, off.z);
        let look = match &shot.look {
            Some(l) => self.anchor_at(sim, run, l).map(|x| x.0).unwrap_or(at),
            None => at,
        } + Vec3::from(shot.look_offset)
            + Vec3::Y * 1.2;
        Some(CameraIntent {
            eye,
            target: look,
            up: Vec3::Y,
            fov: shot.fov,
            near: 0.2,
            far: 2000.0,
            cut: run.prog.shot_t < 0.05,
            shake: 0.0,
        })
    }

    /// A protagonist's colour (others: white).
    pub(crate) fn protagonist_color(&self, who: &str) -> Color {
        crate::protagonists::def()
            .ok()
            .and_then(|d| d.index(who).map(|i| d.protagonist[i].tint()))
            .unwrap_or(Color::hex("#ffffff"))
    }

    /// Controls are the mission's now (the title card, a cutscene, the cards).
    pub fn mission_holds_controls(&self) -> bool {
        let Some(run) = &self.missions.run else { return false };
        match run.phase {
            Phase::Play => super::get(&run.key)
                .ok()
                .and_then(|d| d.step.get(run.step).map(|s| matches!(s.kind, StepKind::Cutscene | StepKind::Choose)))
                .unwrap_or(false),
            _ => true,
        }
    }

    /// The world stands still for the mission's cards and the planning board.
    pub fn mission_pauses_world(&self) -> bool {
        let Some(run) = &self.missions.run else { return false };
        match run.phase {
            Phase::Intro { .. } => true,
            Phase::Failed { t } => t > FAIL_BANNER,
            Phase::Passed { t } => t > PASS_BANNER,
            Phase::Play => {
                super::get(&run.key).ok().and_then(|d| d.step.get(run.step).map(|s| s.kind == StepKind::Choose)).unwrap_or(false)
            }
        }
    }

    /// The vehicle the mission under way must not be left far behind (an `abandoned` fail
    /// condition, the mission's or the step's), if any: the evasion bot keeps to it.
    pub(crate) fn kept_car(&self, sim: &Sim) -> Option<EntityId> {
        let run = self.missions.run.as_ref()?;
        let def = super::get(&run.key).ok()?;
        let step = def.step.get(run.step);
        def.fail.iter().chain(step.map(|s| s.fail.iter()).into_iter().flatten()).find_map(|f| match &f.when {
            When::Abandoned { actor, .. } => {
                entity(sim, run, actor).filter(|e| sim.state.entities.get(*e).is_some_and(|x| x.vehicle.is_some()))
            }
            _ => None,
        })
    }

    /// The mission for `status` (tests, tools).
    pub fn mission_status(&self) -> serde_json::Value {
        let Some(run) = &self.missions.run else {
            return serde_json::json!({ "passed": self.missions.passed, "failed": self.missions.failed, "records": self.missions.records.len() });
        };
        let def = super::get(&run.key).ok();
        let step = def.as_ref().and_then(|d| d.step.get(run.step));
        serde_json::json!({
            "key": run.key,
            "phase": match run.phase { Phase::Intro { .. } => "intro", Phase::Play => "play", Phase::Failed { .. } => "failed", Phase::Passed { .. } => "passed" },
            "step": run.step,
            "kind": step.map(|s| s.kind.name()),
            "objective": step.and_then(|s| s.objective.clone()),
            "elapsed": (run.elapsed * 10.0).round() / 10.0,
            "fails": run.fails,
            "reason": run.reason,
            "flags": run.flags,
            "actors": run.actors.len(),
            "passed": self.missions.passed,
            "failed": self.missions.failed,
        })
    }
}

/// The fail card's items: Retry, Skip (after three fails), Quit.
fn fail_items(fails: u32) -> Vec<(&'static str, &'static str)> {
    let mut v = vec![("retry", "Retry")];
    if fails >= 3 {
        v.push(("skip", "Skip"));
    }
    v.push(("quit", "Quit"));
    v
}

/// m:ss.
fn clock_text(s: f32) -> String {
    let s = s.max(0.0) as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

/// A glowing ring on the ground (a destination, a mission's pickup point).
fn ring(p: Vec3, r: f32, color: Color) -> PrimDraw {
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

/// How many gates a racer is ahead of (or behind) the player before it eases off (or pushes).
const RUBBER: f32 = 0.75;

/// How far round the course someone is: gates passed (laps counted) and the way to the next
/// one as a share of the leg to it.
fn race_score(gates: &[Option<Vec3>], lap: u32, gate: usize, at: Vec3) -> f32 {
    let n = gates.len().max(1);
    let next = gates.get(gate).copied().flatten();
    let prev = gates.get((gate + n - 1) % n).copied().flatten();
    let part = match (prev, next) {
        (Some(a), Some(b)) => {
            let leg = flat(a).distance(flat(b)).max(1.0);
            (1.0 - flat(at).distance(flat(b)) / leg).clamp(0.0, 1.0)
        }
        _ => 0.0,
    };
    (lap as usize * n + gate) as f32 + part
}
