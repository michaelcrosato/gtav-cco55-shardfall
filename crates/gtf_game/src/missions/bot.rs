//! The mission bot (PLAN.md §6.8.6): a policy for every step kind, so `gtf play bot=true`
//! finishes any mission. Each tick it plans from what it sees (the step, the actors, where the
//! player is) and leaves its presses — and the commands a player's hands stand in for (the
//! autopilot's destination, the weapon picked) — in its frame, which the game hands the engine
//! as the bot's input. It changes nothing itself: a person playing is never driven by it.

use glam::{Vec2, Vec3};
use pav_core::InputFrame;
use pav_core::entity::EntityId;
use pav_core::input::{buttons, pad};
use pav_core::sim::Sim;
use serde::{Deserialize, Serialize};

use super::run::{actor_at, down, entity};
use super::{Phase, Run, StepKind};
use crate::Freeway;

fn flat(v: Vec3) -> Vec2 {
    Vec2::new(v.x, v.z)
}

fn wrap(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

/// The bot's memory between ticks (part of the game's state).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MissionBot {
    /// The frame for the next tick.
    #[serde(skip)]
    pub frame: InputFrame,
    /// The walk: the way (walk-map tiles), where it leads, seconds to the next replan.
    pub path: Vec<Vec2>,
    pub goal: Option<Vec2>,
    pub replan: f32,
    /// The step being played (index, parallel sub-step) and seconds in it.
    pub step: (usize, usize),
    pub t: f32,
    /// The destination the autopilot was given last, and seconds to the next.
    pub pilot: Option<Vec3>,
    pub pilot_t: f32,
    /// A press held over ticks (the switch wheel), ticks for pulsed presses.
    pub hold: f32,
    pub tick: u32,
    /// The nearest it has come to where it walks, and seconds since it came nearer (no
    /// headway: a detour sideways, the other way each time).
    pub best: f32,
    pub best_t: f32,
    pub detour: f32,
    pub side: f32,
}

impl MissionBot {
    /// A button pressed every other tick (menus, the phone, getting in).
    fn tap(&mut self, bit: u32) {
        if self.tick.is_multiple_of(2) {
            self.frame.pad.pressed |= bit;
            self.frame.pad.held |= bit;
        }
    }

    fn hold_button(&mut self, bit: u32) {
        self.frame.pad.held |= bit;
    }

    fn cmd(&mut self, c: String) {
        self.frame.cmds.push(c);
    }
}

impl Freeway {
    /// Plans the mission bot's next frame (each tick, after the mission and play).
    pub(crate) fn mission_bot_tick(&mut self, sim: &Sim, dt: f32) {
        let mut b = std::mem::take(&mut self.mbot);
        b.frame = InputFrame::default();
        b.tick = b.tick.wrapping_add(1);
        if let Some(run) = self.missions.run.clone() {
            self.bot_plan(&mut b, sim, &run, dt);
        }
        self.mbot = b;
    }

    fn bot_plan(&self, b: &mut MissionBot, sim: &Sim, run: &Run, dt: f32) {
        let Ok(def) = super::get(&run.key) else { return };
        match run.phase {
            Phase::Intro { t } if t > 1.0 => b.tap(pad::SOUTH),
            Phase::Passed { t } if t > 3.9 => b.tap(pad::SOUTH),
            Phase::Play => {}
            _ => return,
        }
        if run.phase != Phase::Play {
            return;
        }
        let Some(top) = def.step.get(run.step) else { return };
        // A parallel step: the sub-step of whoever is played, or a switch to someone with one left.
        let (step, sub) = if top.kind == StepKind::Parallel {
            match run.subs.iter().position(|s| s.who == self.crew.current && !s.done) {
                Some(k) => (&top.steps[k], k + 1),
                None => {
                    if let Some(s) = run.subs.iter().find(|s| !s.done) {
                        self.bot_switch(b, s.who, dt);
                    }
                    return;
                }
            }
        } else {
            (top, 0)
        };
        if b.step != (run.step, sub) {
            b.step = (run.step, sub);
            b.t = 0.0;
            b.path.clear();
            b.goal = None;
            b.hold = 0.0;
        }
        b.t += dt;
        // Leftovers from the last step: the phone away, the escape autopilot off.
        if self.phone.open && step.kind != StepKind::Photograph {
            b.tap(pad::EAST);
            return;
        }
        if self.bot.escape && !matches!(step.kind, StepKind::LoseHeat) {
            b.cmd("autopilot off".into());
        }
        if self.foot.crouched && step.kind != StepKind::Stealth && self.drive.vehicle().is_none() && b.tick.is_multiple_of(30) {
            b.tap(pad::L3);
        }
        let target = step.target.as_deref().and_then(|a| self.anchor_at(sim, run, a));
        let target_id = step.target.as_deref().and_then(|a| a.strip_prefix("actor:"));
        use StepKind::*;
        match step.kind {
            Wait | Dialogue | Cutscene | Set | Branch | Parallel => {}
            Goto => {
                let car = target_id
                    .and_then(|id| entity(sim, run, id))
                    .filter(|e| sim.state.entities.get(*e).is_some_and(|x| x.vehicle.is_some()));
                if let Some(car) = car.filter(|_| self.drive.vehicle().is_none()) {
                    self.bot_enter(b, sim, car, dt);
                    // (Not in yet: the step ends near it.)
                    b.frame.pad.pressed &= !pad::NORTH;
                    b.frame.pad.held &= !pad::NORTH;
                } else if let Some((p, _)) = target {
                    if self.drive.vehicle().is_some() {
                        self.bot_drive(b, p, 0.9);
                    } else {
                        self.bot_walk(b, sim, p, true, dt);
                    }
                }
            }
            EnterVehicle => {
                if let Some(car) = target_id.and_then(|id| entity(sim, run, id)) {
                    self.bot_enter(b, sim, car, dt);
                }
            }
            DriveTo | Race | Deliver => {
                let car = step.vehicle.as_deref().and_then(|v| entity(sim, run, v));
                let dest = match step.kind {
                    Race => {
                        let prog = if sub > 0 { &run.subs[sub - 1].prog } else { &run.prog };
                        step.gates.get(prog.gate).and_then(|g| self.anchor_at(sim, run, g)).map(|x| x.0)
                    }
                    _ => target.map(|x| x.0),
                };
                let Some(dest) = dest else { return };
                match (self.drive.vehicle(), car) {
                    (Some(mine), Some(want)) if mine != want => b.tap(pad::NORTH),
                    (None, Some(want)) => self.bot_enter(b, sim, want, dt),
                    (None, None) => {
                        // Any car near.
                        if let Some((c, _)) = self.nearest_car(sim) {
                            self.bot_enter(b, sim, c, dt);
                        } else {
                            self.bot_walk(b, sim, dest, true, dt);
                        }
                    }
                    _ => {
                        let me = self.player_point(sim);
                        let close = flat(dest).distance(flat(me)) < step.radius.unwrap_or(7.0);
                        let speed =
                            self.drive.vehicle().and_then(|c| crate::drive::Car::read(sim, c)).map_or(0.0, |c| c.vel.length());
                        if step.kind == Deliver && close && speed < 1.0 {
                            b.tap(pad::NORTH);
                        } else {
                            self.bot_drive(b, dest, if step.kind == Race { 1.25 } else { 0.95 });
                        }
                    }
                }
            }
            Follow => {
                let Some(id) = target_id else { return };
                let Some((p, _)) = actor_at(sim, run, id) else { return };
                if self.drive.vehicle().is_none() {
                    if let Some((c, _)) = self.nearest_car(sim) {
                        self.bot_enter(b, sim, c, dt);
                    }
                    return;
                }
                let d = flat(p).distance(flat(self.player_point(sim)));
                let min = step.min.unwrap_or(0.0);
                let desire = if d < min + 12.0 {
                    0.35
                } else if d > step.distance.unwrap_or(70.0) * 0.6 {
                    1.15
                } else {
                    0.85
                };
                self.bot_drive_moving(b, p, desire, dt);
            }
            Chase => {
                let Some(id) = target_id else { return };
                let Some((p, _)) = actor_at(sim, run, id) else { return };
                let car = entity(sim, run, id).and_then(|e| sim.state.entities.get(e)).is_some_and(|e| e.vehicle.is_some());
                if step.destroy {
                    if let Some(e) = entity(sim, run, id) {
                        self.bot_shoot(b, sim, e, p, dt);
                    }
                } else if car || self.drive.vehicle().is_some() {
                    self.bot_drive_moving(b, p, 1.2, dt);
                } else {
                    self.bot_walk(b, sim, p, true, dt);
                }
            }
            Escape => {
                let me = self.player_point(sim);
                let near = step
                    .pursuers
                    .iter()
                    .filter(|p| !down(sim, run, p))
                    .filter_map(|p| actor_at(sim, run, p).map(|x| x.0))
                    .min_by(|a, c| a.distance(me).total_cmp(&c.distance(me)));
                let Some(from) = near else { return };
                match self.drive.vehicle() {
                    Some(_) => {
                        if b.pilot_t <= 0.0 || b.pilot.is_none_or(|g| g.distance(from) < 50.0) {
                            if let Some(g) = self.far_from(from, me) {
                                b.pilot = None;
                                self.bot_drive(b, g, 1.2);
                            }
                            b.pilot_t = 3.0;
                        } else if let Some(g) = b.pilot {
                            self.bot_drive(b, g, 1.2);
                        }
                        b.pilot_t -= dt;
                    }
                    None => {
                        let away = (flat(me) - flat(from)).normalize_or(Vec2::X);
                        let goal = me + Vec3::new(away.x, 0.0, away.y) * 30.0;
                        self.bot_walk(b, sim, goal, true, dt);
                    }
                }
            }
            LoseHeat => {
                if !self.bot.escape {
                    b.cmd("autopilot escape".into());
                }
            }
            Kill | Survive | Defend => {
                let me = Self::feet(sim);
                let foes: Vec<(EntityId, Vec3)> = if step.kind == Kill {
                    step.targets
                        .iter()
                        .filter(|t| !down(sim, run, t))
                        .filter_map(|t| Some((entity(sim, run, t)?, actor_at(sim, run, t)?.0)))
                        .collect()
                } else {
                    self.enemies
                        .list
                        .iter()
                        .filter_map(|e| Some((e.id, sim.feet(e.id)?)))
                        .filter(|(id, _)| sim.state.entities.get(*id).is_some_and(|x| !x.name.starts_with("dead:")))
                        .collect()
                };
                match foes.into_iter().min_by(|a, c| a.1.distance(me).total_cmp(&c.1.distance(me))) {
                    Some((e, p)) => self.bot_shoot(b, sim, e, p, dt),
                    None => {
                        // Nobody yet (a defend's next wave): wait by what is defended.
                        if let Some((p, _)) = target {
                            self.bot_walk(b, sim, p, false, dt);
                        }
                    }
                }
            }
            Collect => {
                let me = self.player_point(sim);
                let next = step
                    .targets
                    .iter()
                    .filter(|t| run.actors.get(*t).is_some_and(|a| !a.taken))
                    .filter_map(|t| actor_at(sim, run, t).map(|x| x.0))
                    .min_by(|a, c| a.distance(me).total_cmp(&c.distance(me)));
                if let Some(p) = next {
                    if self.drive.vehicle().is_some() {
                        self.bot_drive(b, p, 0.8);
                    } else {
                        self.bot_walk(b, sim, p, true, dt);
                    }
                }
            }
            Photograph => {
                let prog = if sub > 0 { &run.subs[sub - 1].prog } else { &run.prog };
                let next = step
                    .targets
                    .iter()
                    .enumerate()
                    .find(|(i, t)| !prog.done.get(*i).copied().unwrap_or(false) && !down(sim, run, t));
                if let Some((_, t)) = next
                    && let Some((p, _)) = actor_at(sim, run, t)
                {
                    self.bot_photo(b, sim, p, dt);
                }
            }
            Interact => {
                if let Some((p, _)) = target {
                    if flat(p).distance(flat(Self::feet(sim))) < step.radius.unwrap_or(2.2) * 0.6 {
                        b.hold_button(pad::RIGHT);
                    } else {
                        self.bot_walk(b, sim, p, true, dt);
                    }
                }
            }
            Switch => {
                if let Some(i) = step.to.as_deref().and_then(|k| crate::protagonists::def().ok()?.index(k)) {
                    self.bot_switch(b, i, dt);
                }
            }
            Stealth => {
                if !self.foot.crouched && b.tick.is_multiple_of(20) {
                    b.tap(pad::L3);
                }
                if let Some((p, _)) = target {
                    self.bot_walk(b, sim, p, false, dt);
                }
            }
            Holdup => {
                let Some(id) = target_id else { return };
                let Some((p, _)) = actor_at(sim, run, id) else { return };
                let me = Self::feet(sim);
                self.bot_arm(b);
                if flat(p).distance(flat(me)) > 3.5 {
                    self.bot_walk(b, sim, p, false, dt);
                } else {
                    let aimed = self.bot_look(b, me + glam::Vec3::Y * 1.6, p + Vec3::Y * 1.2);
                    if aimed < 0.25 || self.combat.aiming {
                        b.frame.pad.l2 = 1.0;
                        b.hold_button(pad::L2);
                    }
                }
            }
            Choose => {
                let want =
                    self.missions.bot_choice.as_ref().and_then(|c| step.options.iter().position(|o| &o.id == c)).unwrap_or(0);
                let prog = &run.prog;
                if b.t > 0.6 {
                    if prog.focus < want {
                        b.tap(pad::RIGHT);
                    } else if prog.focus > want {
                        b.tap(pad::LEFT);
                    } else {
                        b.tap(pad::SOUTH);
                    }
                }
            }
        }
    }

    /// The nearest car the player could take (not a wreck).
    fn nearest_car(&self, sim: &Sim) -> Option<(EntityId, f32)> {
        let me = Self::feet(sim);
        sim.state
            .entities
            .iter()
            .filter(|e| {
                e.vehicle.as_ref().is_some_and(|v| !v.state.wrecked && v.def.kind != pav_core::vehicle::VehicleKind::Helicopter)
            })
            .filter(|e| !self.police.is_unit(e.id))
            .map(|e| (e.id, e.pos.distance(me)))
            .filter(|x| x.1 < 60.0)
            .min_by(|a, c| a.1.total_cmp(&c.1).then(a.0.cmp(&c.0)))
    }

    /// A road point well away from `from` (escaping a pursuer): of eight around the player, the
    /// one farthest from it.
    fn far_from(&self, from: Vec3, me: Vec3) -> Option<Vec3> {
        let city = self.city.as_ref()?;
        (0..8)
            .map(|k| {
                let a = k as f32 / 8.0 * std::f32::consts::TAU;
                let p = Vec2::new(me.x + a.cos() * 110.0, me.z + a.sin() * 110.0);
                crate::anchor::car_at(city, p, 0.0).0
            })
            .max_by(|a, c| flat(*a).distance(flat(from)).total_cmp(&flat(*c).distance(flat(from))))
    }

    /// Walks (sprinting or not) to `to`: straight there when nothing fixed stands between,
    /// else over the walk map.
    fn bot_walk(&self, b: &mut MissionBot, sim: &Sim, to: Vec3, sprint: bool, dt: f32) {
        let Some(city) = self.city.as_ref() else { return };
        let feet = Self::feet(sim);
        let here = flat(feet);
        let goal = flat(to);
        let dist = here.distance(goal);
        if dist < 0.6 {
            return;
        }
        let new_goal = b.goal.is_none_or(|g| g.distance(goal) > 2.0);
        // No headway for a while: a step aside (the other way each time), then on.
        if new_goal || dist < b.best - 0.5 {
            b.best = dist;
            b.best_t = 0.0;
        }
        b.best_t += dt;
        if b.best_t > 2.0 && b.detour <= 0.0 {
            b.best_t = 0.0;
            b.best = dist;
            b.detour = 0.9;
            b.side = if b.side > 0.0 { -1.0 } else { 1.0 };
            b.path.clear();
        }
        b.replan -= dt;
        if b.replan <= 0.0 || b.path.is_empty() || new_goal {
            b.replan = 1.0;
            b.goal = Some(goal);
            let street = (feet.y - city.ground_y(here)).abs() < 2.0;
            let eye = feet + Vec3::Y * 1.0;
            let ray = Vec3::new(goal.x, to.y + 1.0, goal.y) - eye;
            let clear = dist < 45.0 && sim.raycast_static(eye, ray.normalize_or(Vec3::X), ray.length()).is_none();
            b.path = match (city.walk.nearest_walkable(here, 6), city.walk.nearest_walkable(goal, 8)) {
                (Some(s), Some(t)) if street && !clear && dist > 4.0 => {
                    let mut p = city.walk.path(s, t, 6000);
                    p.push(goal);
                    p
                }
                _ => vec![goal],
            };
        }
        while b.path.len() > 1 && b.path[0].distance(here) < 1.2 {
            b.path.remove(0);
        }
        let next = b.path.first().copied().unwrap_or(goal);
        let mut dir = (next - here).normalize_or_zero();
        if b.detour > 0.0 {
            b.detour -= dt;
            dir = Vec2::new(-dir.y, dir.x) * b.side;
        }
        b.frame.move_dir = dir;
        if sprint && here.distance(goal) > 3.0 {
            b.frame.held |= buttons::SPRINT;
        }
    }

    /// Walks round to `car`'s door (past a bumper when on its far side) and presses △ once
    /// it is the car the player would get into.
    fn bot_enter(&self, b: &mut MissionBot, sim: &Sim, car: EntityId, dt: f32) {
        if self.drive.vehicle() == Some(car) || self.drive.busy() {
            return;
        }
        if self.drive.vehicle().is_some() {
            b.tap(pad::NORTH);
            return;
        }
        let feet = Self::feet(sim);
        let Some(c) = crate::drive::Car::read(sim, car) else { return };
        let door = c.door_point();
        let near_door = flat(door).distance(flat(feet)) < 1.6;
        if crate::drive::Drive::nearest(sim, feet).is_some_and(|(n, _)| n == car)
            && (near_door || flat(c.center).distance(flat(feet)) < c.half.z + 1.0)
        {
            b.tap(pad::NORTH);
            return;
        }
        // On the far side: round the nearer bumper first.
        let side = flat(door - c.feet()).normalize_or_zero();
        let rel = flat(feet - c.feet());
        let to = if rel.dot(side) < c.half.x * 0.5 && !near_door {
            let fwd = flat(c.forward());
            let reach = c.half.z + 1.6;
            let front = flat(c.feet()) + fwd * reach + side * (c.half.x + 0.9);
            let back = flat(c.feet()) - fwd * reach + side * (c.half.x + 0.9);
            let corner = if front.distance(flat(feet)) < back.distance(flat(feet)) { front } else { back };
            if corner.distance(flat(feet)) < 1.2 { door } else { Vec3::new(corner.x, door.y, corner.y) }
        } else {
            door
        };
        self.bot_walk(b, sim, to, true, dt);
    }

    /// The autopilot to `dest` (switched on first).
    fn bot_drive(&self, b: &mut MissionBot, dest: Vec3, desire: f32) {
        if self.drive.vehicle().is_none() {
            return;
        }
        let piloted = self.traffic.cars.iter().any(|a| a.pilot);
        if !piloted {
            b.cmd("autopilot on".into());
            b.pilot = None;
        }
        if !piloted || b.pilot.is_none_or(|g| g.distance(dest) > 4.0) {
            b.cmd(format!("pilot_goal {:.1} {:.1} {:.2}", dest.x, dest.z, desire));
            b.pilot = Some(dest);
        }
    }

    /// The autopilot after something that moves (a target car, a runner): its destination
    /// renewed every second and a half.
    fn bot_drive_moving(&self, b: &mut MissionBot, dest: Vec3, desire: f32, dt: f32) {
        b.pilot_t -= dt;
        if b.pilot_t <= 0.0 || b.pilot.is_none() {
            b.pilot_t = 1.5;
            b.pilot = None;
        }
        self.bot_drive(b, dest, desire);
    }

    /// Turns the camera toward `to` from `eye` with the right stick. Returns the heading still
    /// to turn (radians).
    fn bot_look(&self, b: &mut MissionBot, eye: Vec3, to: Vec3) -> f32 {
        let d = to - eye;
        let want = d.x.atan2(-d.z);
        let err = wrap(want - self.cam.yaw);
        b.frame.pad.stick_r.x = if err.abs() < 0.01 { 0.0 } else { (err * 4.0).clamp(-1.0, 1.0).abs().max(0.2) * err.signum() };
        err.abs()
    }

    /// Picks a gun with rounds left (a player's hand on the weapon wheel).
    fn bot_arm(&self, b: &mut MissionBot) {
        let Ok(wd) = crate::combat::def() else { return };
        let ok = |i: usize| {
            wd.weapon.get(i).is_some_and(|w| w.kind == crate::combat::Kind::Gun)
                && self.crew.owns(i)
                && self.combat.mag.get(i).copied().unwrap_or(0) + self.combat.reserve.get(i).copied().unwrap_or(0) > 0
        };
        if !ok(self.weapon)
            && let Some(i) = (0..wd.weapon.len()).find(|&i| ok(i))
        {
            b.cmd(format!("weapon {}", wd.weapon[i].id));
        }
    }

    /// Fights `who` at `at`: near enough and in sight, aim (the assist snaps on) and fire;
    /// otherwise closer.
    fn bot_shoot(&self, b: &mut MissionBot, sim: &Sim, who: EntityId, at: Vec3, dt: f32) {
        if self.drive.vehicle().is_some() {
            b.tap(pad::NORTH);
            return;
        }
        self.bot_arm(b);
        let me = Self::feet(sim);
        let eye = me + Vec3::Y * 1.6;
        let chest = at + Vec3::Y * 1.2;
        let d = flat(at).distance(flat(me));
        let seen = crate::heat::sight(sim, eye, chest);
        if d > 28.0 || !seen {
            self.bot_walk(b, sim, at, d > 10.0, dt);
            return;
        }
        let err = self.bot_look(b, eye, chest);
        if err < 0.2 || self.combat.aiming {
            b.frame.pad.l2 = 1.0;
            b.hold_button(pad::L2);
            if self.combat.aiming && b.tick.is_multiple_of(8) {
                b.frame.pad.pressed |= pad::R2;
                b.frame.pad.held |= pad::R2;
                b.frame.pad.r2 = 1.0;
            }
        }
        let _ = who;
    }

    /// The phone's camera on `at`: up, to the camera tile, aimed, the shutter.
    fn bot_photo(&self, b: &mut MissionBot, sim: &Sim, at: Vec3, dt: f32) {
        let me = Self::feet(sim);
        if self.drive.vehicle().is_some() {
            b.tap(pad::NORTH);
            return;
        }
        if flat(at).distance(flat(me)) > 35.0 || !crate::heat::sight(sim, me + Vec3::Y * 1.6, at + Vec3::Y * 1.0) {
            if self.phone.open {
                b.tap(pad::EAST);
            } else {
                self.bot_walk(b, sim, at, true, dt);
            }
            return;
        }
        if !self.phone.open {
            b.tap(pad::UP);
            return;
        }
        match self.phone.app.as_str() {
            "home" => {
                let (f, want) = (self.phone.focus as i32, 2);
                let (fr, fc, wr, wc) = (f / 3, f % 3, want / 3, want % 3);
                if fr > wr {
                    b.tap(pad::UP);
                } else if fr < wr {
                    b.tap(pad::DOWN);
                } else if fc < wc {
                    b.tap(pad::RIGHT);
                } else if fc > wc {
                    b.tap(pad::LEFT);
                } else {
                    b.tap(pad::SOUTH);
                }
            }
            "camera" => {
                let view = self.phone_view(sim);
                let aim = at + Vec3::Y * 0.4;
                let err = self.bot_look(b, view.eye, aim);
                // Up or down to it: the camera's pitch follows the orbit camera's elevation.
                let d = aim - view.eye;
                let want_pitch = d.y.atan2(Vec2::new(d.x, d.z).length());
                let want_elev = 0.35 - want_pitch / 0.8;
                let e = want_elev - self.cam.elevation;
                b.frame.pad.stick_r.y = if e.abs() < 0.02 { 0.0 } else { (-e * 4.0).clamp(-1.0, 1.0) };
                if err < 0.08 && e.abs() < 0.1 && self.phone.flash > 0.5 {
                    b.tap(pad::SOUTH);
                }
            }
            _ => b.tap(pad::EAST),
        }
    }

    /// Holds D-pad down on the switch wheel toward protagonist `i`, then lets go.
    fn bot_switch(&self, b: &mut MissionBot, i: usize, dt: f32) {
        if self.crew.switch.is_some() || self.crew.current == i {
            b.hold = 0.0;
            return;
        }
        let n = self.crew.members.len().max(1);
        let a = i as f32 / n as f32 * std::f32::consts::TAU;
        b.hold += dt;
        if b.hold < 0.45 {
            if b.hold <= dt * 1.5 {
                b.frame.pad.pressed |= pad::DOWN;
            }
            b.frame.pad.held |= pad::DOWN;
            b.frame.pad.stick_l = Vec2::new(a.sin(), a.cos());
        } else {
            // Let go (a tick without the button), then again from the top.
            b.hold = 0.0;
        }
    }
}
