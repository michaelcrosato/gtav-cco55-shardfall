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
use super::{Phase, Run, StepKind, When};
use crate::Freeway;

/// Nothing (a wall, a car, someone else) in front of the chest of `who` (feet at `at`), where
/// the aim settles: seen from the camera (`eye`, where shots are aimed from) and from the gun of
/// a player standing at `me`.
fn clear_shot(sim: &Sim, eye: Vec3, me: Vec3, at: Vec3, who: EntityId) -> bool {
    let chest = at + Vec3::Y * 1.25;
    [eye, me + Vec3::Y * 1.35].into_iter().all(|from| {
        let d = chest - from;
        let len = d.length();
        len < 0.5 || sim.raycast(from, d / len, len - 0.3, Some(who)).is_none()
    })
}

/// A way round the vehicles between `from` and `to` (at most 14 m off): A* over half-metre
/// cells, with vehicles (grown by a body's width) and buildings blocked and roads open. The
/// points along it, a metre apart, ending at `to` or as near it as there is a way; None when
/// there is none.
fn round_cars(sim: &Sim, city: &pav_world::City, from: Vec2, to: Vec2) -> Option<Vec<Vec2>> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;
    const CELL: f32 = 0.5;
    const HALF: i32 = 28;
    const N: i32 = HALF * 2 + 1;
    let cars: Vec<crate::drive::Car> = sim
        .state
        .entities
        .iter()
        .filter(|e| e.vehicle.is_some() && flat(e.pos).distance(from) < 32.0)
        .filter_map(|e| crate::drive::Car::read(sim, e.id))
        .collect();
    let at = |c: glam::IVec2| from + Vec2::new(c.x as f32, c.y as f32) * CELL;
    let open = |p: Vec2| {
        let car = cars.iter().any(|c| {
            let d = p - flat(c.center);
            d.dot(flat(c.forward())).abs() < c.half.z + 0.45 && d.dot(flat(c.left())).abs() < c.half.x + 0.45
        });
        let wall = city.walk.at(p) == pav_world::walk::Walk::Blocked && city.grid.road(city.grid.cell_of(p)).is_none();
        !car && !wall
    };
    let index = |c: glam::IVec2| ((c.y + HALF) * N + c.x + HALF) as usize;
    let rel = (to - from) / CELL;
    let goal = glam::IVec2::new((rel.x.round() as i32).clamp(-HALF, HALF), (rel.y.round() as i32).clamp(-HALF, HALF));
    let h = |c: glam::IVec2| (c - goal).as_vec2().length();
    let mut cost = vec![f32::MAX; (N * N) as usize];
    let mut prev = vec![usize::MAX; (N * N) as usize];
    let mut heap = BinaryHeap::new();
    let start = glam::IVec2::ZERO;
    cost[index(start)] = 0.0;
    heap.push((Reverse((h(start) * 100.0) as u32), start.x, start.y));
    let mut best = (h(start), start);
    while let Some((_, x, y)) = heap.pop() {
        let c = glam::IVec2::new(x, y);
        if c == goal {
            best = (0.0, c);
            break;
        }
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
            let n = c + glam::IVec2::new(dx, dy);
            if n.x.abs() > HALF || n.y.abs() > HALF || !open(at(n)) {
                continue;
            }
            let step = if dx != 0 && dy != 0 { std::f32::consts::SQRT_2 } else { 1.0 };
            let g = cost[index(c)] + step;
            if g < cost[index(n)] {
                cost[index(n)] = g;
                prev[index(n)] = index(c);
                if h(n) < best.0 {
                    best = (h(n), n);
                }
                heap.push((Reverse(((g + h(n)) * 100.0) as u32), n.x, n.y));
            }
        }
    }
    let end = best.1;
    if end == start {
        return None;
    }
    let mut cells = vec![end];
    let mut i = index(end);
    while prev[i] != usize::MAX && prev[i] != index(start) {
        i = prev[i];
        cells.push(glam::IVec2::new(i as i32 % N - HALF, i as i32 / N - HALF));
    }
    cells.reverse();
    let mut way: Vec<Vec2> = cells.iter().skip(1).step_by(2).map(|c| at(*c)).collect();
    way.push(at(end));
    Some(way)
}

fn flat(v: Vec3) -> Vec2 {
    Vec2::new(v.x, v.z)
}

/// Wanted, a destination farther than this (m) is driven to once the police are lost.
const FAR_WANTED: f32 = 220.0;

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
    /// A fight from cover: seconds into the peek-and-duck cycle, seconds before cover is tried
    /// again, seconds in this cover and the hits landed when it was taken.
    pub peek: f32,
    pub cover_t: f32,
    pub cover_for: f32,
    pub cover_hits: u32,
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
            // A step done on foot (or with the hands): the last step's autopilot is off (it
            // would keep the wheel, and the stick).
            let afoot = matches!(
                step.kind,
                StepKind::EnterVehicle
                    | StepKind::Kill
                    | StepKind::Survive
                    | StepKind::Defend
                    | StepKind::Collect
                    | StepKind::Photograph
                    | StepKind::Interact
                    | StepKind::Switch
                    | StepKind::Stealth
                    | StepKind::Holdup
                    | StepKind::Choose
            );
            if afoot && self.traffic.cars.iter().any(|a| a.pilot) && !self.bot.escape {
                b.cmd("autopilot off".into());
            }
        }
        b.t += dt;
        // Leftovers from the last step: the phone away, the escape autopilot off.
        if self.phone.open && step.kind != StepKind::Photograph {
            b.tap(pad::EAST);
            return;
        }
        // Wanted with a long drive ahead: lose the police first (a player who drives across
        // town under the sirens meets a roadblock), then go.
        let target_far = step
            .target
            .as_deref()
            .and_then(|a| self.anchor_at(sim, run, a))
            .is_some_and(|(p, _)| flat(p).distance(flat(self.player_point(sim))) > FAR_WANTED);
        let shake_first =
            self.heat.stars > 0 && matches!(step.kind, StepKind::DriveTo | StepKind::Deliver | StepKind::Goto) && target_far;
        if shake_first {
            if !self.bot.escape {
                b.cmd("autopilot escape".into());
            }
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
                    let near = flat(p).distance(flat(self.player_point(sim))) < 30.0;
                    let stopped = b.t > 3.0
                        && self
                            .drive
                            .vehicle()
                            .and_then(|c| crate::drive::Car::read(sim, c))
                            .is_some_and(|c| c.vel.length() < 0.5);
                    if self.drive.vehicle().is_some() && near && (self.traffic.pilot_arrived() || stopped) {
                        // The car has stopped as near as the road goes: on foot the rest of the way.
                        b.tap(pad::NORTH);
                    } else if self.drive.vehicle().is_some() {
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
                        } else if close && step.kind != Race {
                            // There: stop here (the autopilot's own stop may lie round the block).
                            self.bot_drive(b, me, 0.6);
                        } else {
                            // A step that fails on more heat: an easy drive, nothing to see here.
                            let quiet = step.fail.iter().any(|f| matches!(f.when, When::HeatOver(_)));
                            let desire = match step.kind {
                                Race => 1.25,
                                _ if quiet => 0.85,
                                _ => 0.95,
                            };
                            self.bot_drive(b, dest, desire);
                        }
                    }
                }
            }
            Follow => {
                let Some(id) = target_id else { return };
                let Some((p, yaw)) = actor_at(sim, run, id) else { return };
                if self.drive.vehicle().is_none() {
                    if let Some((c, _)) = self.nearest_car(sim) {
                        self.bot_enter(b, sim, c, dt);
                    }
                    return;
                }
                let me = self.player_point(sim);
                let d = flat(p).distance(flat(me));
                let min = step.min.unwrap_or(0.0);
                let (vel, fwd) = entity(sim, run, id)
                    .and_then(|e| crate::drive::Car::read(sim, e))
                    .map(|c| (c.vel, c.forward()))
                    .unwrap_or((Vec3::ZERO, Vec3::new(yaw.sin(), 0.0, yaw.cos())));
                if min > 0.0 && d < min + 7.0 && vel.length() < 3.0 {
                    // It has stopped (a light, a junction): stop here, well back.
                    self.bot_drive(b, me, 0.3);
                    return;
                }
                // Close: a crawl; a few lengths back: easy; falling behind: hurrying (through the
                // light the target just made).
                let desire = if d < min + 6.0 {
                    0.4
                } else if d < min + 16.0 {
                    0.8
                } else if d < min + 36.0 {
                    1.0
                } else {
                    1.15
                };
                // The way it goes: a point a lane or two along its own route (the same roads),
                // else ahead of it — never the target itself (the autopilot stops at its goal).
                let car = entity(sim, run, id);
                let on_route = self.traffic.cars.iter().find(|a| Some(a.car) == car).and_then(|a| {
                    let lane = a.route.get(1).or(a.route.first()).copied().unwrap_or(a.lane);
                    self.city.as_ref()?.lanes.lanes.get(lane as usize)?.points.last().copied()
                });
                self.bot_drive_moving(b, on_route.unwrap_or(p + fwd * 30.0), desire, dt);
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
                // The nearest one in the clear (a car between is cover, for them as for us),
                // else the nearest: closer in, round what is in the way.
                let near = |a: &&(EntityId, Vec3), c: &&(EntityId, Vec3)| a.1.distance(me).total_cmp(&c.1.distance(me));
                let eye = self.camera(sim).eye;
                let pick = foes
                    .iter()
                    .filter(|(e, p)| clear_shot(sim, eye, me, *p, *e))
                    .min_by(near)
                    .or_else(|| foes.iter().min_by(near))
                    .copied();
                // The ability that fights on foot (Deadeye, Overdrive), once the meter allows.
                let fights = crate::protagonists::def().ok().and_then(|d| d.ability_of(self.crew.current).map(|a| a.on_foot));
                let meter = self.crew.me().map_or(0.0, |m| m.meter);
                if fights == Some(true) && !self.crew.ability && meter > 0.3 && pick.is_some() && b.tick.is_multiple_of(30) {
                    b.frame.pad.held |= pad::L3 | pad::R3;
                    b.frame.pad.pressed |= pad::L3;
                }
                match pick {
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
                    // Well inside the radius (a truck's body keeps its middle out of reach).
                    if flat(p).distance(flat(Self::feet(sim))) < step.radius.unwrap_or(2.2) * 0.85 {
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
            Jump => {
                let car = step.vehicle.as_deref().and_then(|v| entity(sim, run, v));
                match (self.drive.vehicle(), car) {
                    (Some(mine), Some(want)) if mine != want => return b.tap(pad::NORTH),
                    (None, Some(want)) => return self.bot_enter(b, sim, want, dt),
                    (None, None) => {
                        if let Some((c, _)) = self.nearest_car(sim) {
                            self.bot_enter(b, sim, c, dt);
                        }
                        return;
                    }
                    _ => {}
                }
                let gate = |k: usize| step.gates.get(k).and_then(|g| self.anchor_at(sim, run, g)).map(|x| x.0);
                let (Some(run_up), Some(lip), Some((land, _))) = (gate(0), gate(1), target) else { return };
                let Some(c) = self.drive.vehicle().and_then(|v| crate::drive::Car::read(sim, v)) else { return };
                let me = c.feet();
                let prog = if sub > 0 { &run.subs[sub - 1].prog } else { &run.prog };
                let launched = prog.meter >= 1.0;
                // `hold` is the phase: 0 to the run-up on the autopilot, 1 the run itself.
                if b.hold < 0.5 && !launched {
                    if flat(me).distance(flat(run_up)) < 12.0 {
                        b.hold = 1.0;
                    } else {
                        self.bot_drive(b, run_up, 0.9);
                        return;
                    }
                }
                if self.traffic.cars.iter().any(|a| a.pilot) {
                    b.cmd("autopilot off".into());
                }
                // Straight at the lip, then at the landing; flat out until airborne, then let the
                // car fly; down again, roll on to the landing.
                let past_lip = launched || (flat(lip) - flat(me)).dot(flat(lip) - flat(run_up)) < 0.0;
                let aim = if past_lip { land } else { lip };
                let want = (flat(aim) - flat(me)).normalize_or_zero();
                let fwd = flat(c.forward()).normalize_or_zero();
                let cross = fwd.x * want.y - fwd.y * want.x;
                let airborne = self.drive.vehicle().and_then(|v| sim.vehicle_status(v)).is_some_and(|s| s.airborne);
                if !airborne {
                    b.frame.pad.stick_l.x = (cross * 2.5).clamp(-1.0, 1.0);
                }
                b.frame.pad.r2 = if launched && !airborne {
                    0.4
                } else if airborne {
                    0.0
                } else {
                    1.0
                };
                if b.frame.pad.r2 > 0.0 {
                    b.frame.pad.held |= pad::R2;
                }
                // A run that went nowhere (stuck, missed): round again.
                if b.t > 25.0 && !launched {
                    b.hold = 0.0;
                    b.t = 0.0;
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
            // In the clear at the waist and the knee (a low wall stops the legs, not the eye).
            let clear = dist < 45.0
                && [1.0, 0.4].into_iter().all(|h| {
                    let eye = feet + Vec3::Y * h;
                    let ray = Vec3::new(goal.x, to.y + h, goal.y) - eye;
                    sim.raycast_static(eye, ray.normalize_or(Vec3::X), ray.length()).is_none()
                });
            b.path = match (city.walk.nearest_walkable(here, 6), city.walk.nearest_walkable(goal, 8)) {
                (Some(s), Some(t)) if street && !clear && dist > 4.0 => {
                    let mut p = city.walk.path(s, t, 6000);
                    p.push(goal);
                    p
                }
                _ => vec![goal],
            };
            // A vehicle across the first leg (cars at the kerb, a car between it and a door):
            // the way round it, then on.
            if let Some(first) = b.path.first().copied() {
                let leg = first - here;
                let len = leg.length().min(14.0);
                let dir = leg.normalize_or_zero();
                let hit =
                    (len > 1.0).then(|| sim.raycast(feet + Vec3::Y * 0.6, Vec3::new(dir.x, 0.0, dir.y), len, None)).flatten();
                let car =
                    hit.and_then(|h| h.entity).is_some_and(|e| sim.state.entities.get(e).is_some_and(|x| x.vehicle.is_some()));
                if car && let Some(mut way) = round_cars(sim, city, here, here + dir * len) {
                    way.append(&mut b.path);
                    b.path = way;
                }
            }
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
        // The hardest-hitting one (damage a minute, every pellet) with rounds left.
        let dps = |i: usize| wd.weapon[i].damage * wd.weapon[i].pellets.max(1) as f32 * wd.weapon[i].rpm;
        if let Some(i) = (0..wd.weapon.len()).filter(|&i| ok(i)).max_by(|&x, &y| dps(x).total_cmp(&dps(y)))
            && i != self.weapon
            && b.tick.is_multiple_of(30)
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
        let chest = at + Vec3::Y * 1.2;
        let d = flat(at).distance(flat(me));
        b.cover_t -= dt;
        if self.cover.at.is_some() {
            // In cover: up and firing a while, down a while (cover takes most of what comes
            // back); out again when peeking lands nothing (a target round the corner).
            b.cover_for += dt;
            b.peek = (b.peek + dt) % 2.4;
            let err = self.bot_look(b, self.camera(sim).eye, chest);
            if self.combat.hits > b.cover_hits {
                b.cover_hits = self.combat.hits;
                b.cover_for = 0.0;
            }
            if b.cover_for > 6.0 || d > 40.0 {
                b.tap(pad::R1);
                b.cover_t = 6.0;
            } else if b.peek < 1.5 {
                b.hold_button(pad::L2);
                b.frame.pad.l2 = 1.0;
                if self.combat.aiming && err < 0.15 && b.tick.is_multiple_of(8) {
                    b.frame.pad.pressed |= pad::R2;
                    b.frame.pad.held |= pad::R2;
                    b.frame.pad.r2 = 1.0;
                }
            }
            return;
        }
        if d > 28.0 || !clear_shot(sim, self.camera(sim).eye, me, at, who) {
            self.bot_walk(b, sim, at, d > 10.0, dt);
            return;
        }
        // (Turned from where the camera is: close in, the shoulder view sees round the body.)
        let err = self.bot_look(b, self.camera(sim).eye, chest);
        // A fight at close range: into cover if any is in reach (a car, a wall; R1 does nothing
        // without), facing them.
        if d < 30.0 && b.cover_t <= 0.0 && err < 0.3 {
            b.cover_t = 2.0;
            b.cover_for = 0.0;
            b.cover_hits = self.combat.hits;
            b.peek = 0.0;
            b.frame.pad.pressed |= pad::R1;
            b.frame.pad.held |= pad::R1;
            return;
        }
        if err < 0.2 || self.combat.aiming {
            b.frame.pad.l2 = 1.0;
            b.hold_button(pad::L2);
            if self.combat.aiming && b.tick.is_multiple_of(8) {
                b.frame.pad.pressed |= pad::R2;
                b.frame.pad.held |= pad::R2;
                b.frame.pad.r2 = 1.0;
            }
        }
    }

    /// The phone's camera on `at`: up, to the camera tile, aimed, the shutter.
    fn bot_photo(&self, b: &mut MissionBot, sim: &Sim, at: Vec3, dt: f32) {
        let me = Self::feet(sim);
        if self.drive.vehicle().is_some() {
            b.tap(pad::NORTH);
            return;
        }
        // (The phone reaches 60 m: from 50 m off a watcher seldom sees who takes the picture.)
        if flat(at).distance(flat(me)) > 50.0 || !crate::heat::sight(sim, me + Vec3::Y * 1.6, at + Vec3::Y * 1.0) {
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
