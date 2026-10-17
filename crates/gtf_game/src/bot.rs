//! The player bot (PLAN.md §8.4 `bot/`): what a player would press, for tests, soaks and the
//! attract mode. The game updates it each tick (`update`, with the world in hand); the hook's
//! `bot()` turns it into the controller frame (`frame`). Its one job so far, `escape`: the
//! autopilot drives away from the police; a car that is wrecked, burning or boxed in is left
//! behind (△), and the bot makes for the nearest car it can take — dragging out its driver — or,
//! with none near, runs from the police on foot. Getting into a car hands back to the autopilot.

use glam::{Vec2, Vec3};
use pav_core::InputFrame;
use pav_core::entity::EntityId;
use pav_core::input::{buttons, pad};
use pav_core::sim::Sim;
use pav_world::City;
use serde::{Deserialize, Serialize};

use crate::drive::{self, Car};

/// A car that has gone no farther than this (m) in [`STUCK_AFTER`] seconds is boxed in.
const STUCK_MOVE: f32 = 3.0;
const STUCK_AFTER: f32 = 8.0;
/// The bot looks for a car to take this far around it (m).
const LOOK: f32 = 40.0;

/// Where △ is in a tap: held one tick, then let go.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tap {
    #[default]
    Idle,
    Hold,
    Release,
}

/// The bot's state (part of the game's state).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Bot {
    /// Running from the police.
    pub escape: bool,
    /// △ in a car (get out).
    pub tap: Tap,
    /// On foot: the car it makes for, the way there (tile centres) and seconds to the next
    /// replan; the cars it left (not taken again); where it walks this tick, sprinting.
    pub target: Option<EntityId>,
    pub path: Vec<Vec2>,
    pub replan: f32,
    pub left: Vec<EntityId>,
    /// The nearest it has come to the target car, and seconds since it last came nearer (a
    /// target it cannot get to is given up).
    pub best: f32,
    pub since_best: f32,
    pub walk: Vec2,
    pub enter: bool,
    /// In a car: where it was a while ago and the seconds since (boxed in when it barely moved).
    pub anchor: Vec3,
    pub anchor_t: f32,
    /// Counts for tests and the attract mode: cars left behind and cars taken.
    pub bailed: u32,
    pub taken: u32,
}

/// What the bot needs to know this tick.
pub struct Moment<'a> {
    pub city: &'a City,
    /// The player's feet, the car they sit in, and whether they are getting in or out.
    pub feet: Vec3,
    pub car: Option<EntityId>,
    pub busy: bool,
    /// Where the police are (the nearest officer or unit), and their vehicles (not to be taken).
    pub threat: Option<Vec3>,
    pub police: &'a [EntityId],
    /// Wanted at all.
    pub wanted: bool,
    pub dt: f32,
}

impl Bot {
    /// One tick: decide the next presses.
    pub fn update(&mut self, sim: &Sim, m: &Moment) {
        self.walk = Vec2::ZERO;
        self.enter = false;
        self.tap = match self.tap {
            Tap::Hold => Tap::Release,
            _ => Tap::Idle,
        };
        if !self.escape || m.busy {
            return;
        }
        match m.car {
            Some(car) => self.in_car(sim, m, car),
            None => self.on_foot(sim, m),
        }
    }

    /// In a car: leave it when it burns, is nearly done for or has been boxed in.
    fn in_car(&mut self, sim: &Sim, m: &Moment, car: EntityId) {
        self.target = None;
        let Some(c) = Car::read(sim, car) else { return };
        if self.anchor_t <= 0.0 || self.anchor.distance(c.center) > STUCK_MOVE {
            self.anchor = c.center;
            self.anchor_t = 0.0;
        }
        self.anchor_t += m.dt;
        let st = sim.vehicle_status(car);
        let done = st.as_ref().is_some_and(|s| s.burning || s.flooded || s.health < 0.15);
        // Boxed in: out once the police are not right there (under their guns the car is the
        // better cover while the autopilot shoves to get free) — or when that is going nowhere.
        let close = m.threat.is_some_and(|t| t.distance(c.center) < 25.0);
        let boxed = m.wanted && self.anchor_t > if close { STUCK_AFTER * 2.0 } else { STUCK_AFTER };
        if (done || boxed) && self.tap == Tap::Idle {
            self.tap = Tap::Hold;
            self.left.push(car);
            self.anchor_t = 0.0;
            self.bailed += 1;
        }
    }

    /// On foot: to the best car to take (near, clear of the police, nothing parked on its nose),
    /// then △ at its door; with none, away from the police.
    fn on_foot(&mut self, sim: &Sim, m: &Moment) {
        self.anchor_t = 0.0;
        // Not getting any nearer (something in the way): another car.
        if let Some(c) = self.target.and_then(|id| Car::read(sim, id)) {
            let d = c.center.distance(m.feet);
            if d < self.best - 0.3 {
                self.best = d;
                self.since_best = 0.0;
            } else {
                self.since_best += m.dt;
            }
            if self.since_best > 1.5 {
                self.left.push(c.id);
                self.target = None;
                self.path.clear();
            }
        }
        let fit = |id: EntityId| -> bool {
            !self.left.contains(&id)
                && !m.police.contains(&id)
                && sim
                    .vehicle_status(id)
                    .is_some_and(|s| !s.wrecked && !s.burning && !s.in_water && !s.flooded && !s.upside_down && s.health > 0.4)
                && Car::read(sim, id).is_some_and(|c| !c.heli && c.center.distance(m.feet) < LOOK)
        };
        if self.target.is_some_and(|id| !fit(id)) {
            self.target = None;
            self.path.clear();
        }
        if self.target.is_none() {
            let cars: Vec<Car> =
                sim.state.entities.iter().filter(|e| e.vehicle.is_some()).filter_map(|e| Car::read(sim, e.id)).collect();
            let score = |c: &Car| -> f32 {
                let near_police = cars.iter().any(|o| m.police.contains(&o.id) && o.center.distance(c.center) < 15.0);
                let nosed = cars.iter().any(|o| {
                    let d = o.center - c.center;
                    o.id != c.id && d.length() < 8.0 && d.dot(c.forward()) > 0.0 && d.dot(c.left()).abs() < 2.2
                });
                c.center.distance(m.feet) + if near_police { 30.0 } else { 0.0 } + if nosed { 15.0 } else { 0.0 }
            };
            self.target = cars
                .iter()
                .filter(|c| fit(c.id))
                .map(|c| (c.id, score(c)))
                .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
                .map(|(id, _)| id);
            self.replan = 0.0;
            self.best = f32::MAX;
            self.since_best = 0.0;
        }
        // Within reach of a car worth taking (the drive walks to its door and gets in): △.
        if drive::Drive::nearest(sim, m.feet).is_some_and(|(id, _)| fit(id)) {
            self.enter = true;
            self.taken += 1;
            return;
        }
        let feet = Vec2::new(m.feet.x, m.feet.z);
        let goal = match self.target.and_then(|id| Car::read(sim, id)) {
            Some(c) => Vec2::new(c.center.x, c.center.z),
            None => {
                // No car near: run, away from the police.
                let from = m.threat.map(|t| Vec2::new(t.x, t.z)).unwrap_or(feet - Vec2::X);
                feet + (feet - from).normalize_or(Vec2::X) * 60.0
            }
        };
        // Over the sidewalks and crossings (from the nearest walkable tile, to the one nearest the
        // goal), then straight there.
        self.replan -= m.dt;
        if self.replan <= 0.0 || self.path.is_empty() {
            self.replan = 1.0;
            let walk = &m.city.walk;
            self.path = match (walk.nearest_walkable(feet, 6), walk.nearest_walkable(goal, 8)) {
                (Some(a), Some(b)) if a.distance(goal) > 3.0 => {
                    let mut p = walk.path(a, b, 6000);
                    p.push(goal);
                    p
                }
                _ => vec![goal],
            };
        }
        while self.path.len() > 1 && self.path[0].distance(feet) < 1.2 {
            self.path.remove(0);
        }
        let next = self.path.first().copied().unwrap_or(goal);
        self.walk = (next - feet).normalize_or(Vec2::ZERO);
    }

    /// The controller frame: △ taps, the walk (world space, sprinting), △ at a door. The
    /// autopilot's pedals are added by the game.
    pub fn frame(&self, f: &mut InputFrame) {
        if self.tap == Tap::Hold {
            f.pad.held |= pad::NORTH;
            f.pad.pressed |= pad::NORTH;
        }
        if self.enter {
            f.pad.held |= pad::NORTH;
            f.pad.pressed |= pad::NORTH;
        }
        if self.walk != Vec2::ZERO {
            f.move_dir = self.walk;
            f.held |= buttons::SPRINT;
        }
    }

    /// Stops the job (the `autopilot off` command, a respawn).
    pub fn stop(&mut self) {
        *self = Bot { bailed: self.bailed, taken: self.taken, ..Default::default() };
    }
}
