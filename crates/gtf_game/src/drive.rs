//! The player and vehicles (PLAN.md §5.6.3). △ near a car walks to the driver's door (at most
//! a second), the door swings open (a locked car's window is smashed first, a driver is pulled
//! out: carjacking), the player slides in and the door shuts — about 1.2 s in all, cancelled by
//! walking away. Driving reads the pad: R2 throttle and L2 brake / reverse (ramped in over
//! 0.15 s, as a keyboard's keys jump), R1 handbrake, the left stick steers (and pitches the car
//! in the air), L3 sounds the horn and, tapped in a police car, toggles the siren. △ stops the
//! car and gets out; held above 15 m/s it bails out with a roll. A helicopter climbs and
//! descends on R2 and L2, flies with the left stick and turns with either stick.

use std::collections::{BTreeMap, BTreeSet};

use glam::{Quat, Vec3};
use pav_core::InputFrame;
use pav_core::entity::EntityId;
use pav_core::frame::SimEvent;
use pav_core::input::pad;
use pav_core::sim::Sim;
use pav_core::vehicle::{Side, VehicleInput, VehicleKind};
use serde::{Deserialize, Serialize};

/// How near the driver's door must be for △ to start getting in (m).
pub const REACH: f32 = 4.0;
/// Bailing out needs this speed (m/s) and △ held this long (s).
pub const BAIL_SPEED: f32 = 15.0;
const BAIL_HOLD: f32 = 0.25;
/// The walk to the door gives up and steps there after this long (s).
const WALK_MAX: f32 = 1.0;
/// Stage lengths (s): the door swinging open, elbowing a locked window, pulling a driver out,
/// sliding in, the door shutting.
const OPEN: f32 = 0.3;
const SMASH: f32 = 0.55;
const JACK: f32 = 0.6;
const SIT: f32 = 0.3;
const SHUT: f32 = 0.3;
/// L3 let go sooner than this toggles a police car's siren; held longer, the horn (s).
const TAP: f32 = 0.22;
/// A car alarm sounds this long after its window breaks (s).
pub const ALARM: f32 = 12.0;

/// A vehicle as getting in and driving need it.
#[derive(Clone, Debug)]
pub struct Car {
    pub id: EntityId,
    /// Its archetype key (the entity's name).
    pub key: String,
    /// The body's centre and turn, its heading (0 faces +z, π/2 faces +x), velocity (m/s),
    /// half size and the centre's height above the ground at rest.
    pub center: Vec3,
    pub rot: Quat,
    pub yaw: f32,
    pub vel: Vec3,
    pub half: Vec3,
    pub base: f32,
    pub heli: bool,
    /// It has a siren (police) and a radio.
    pub siren: bool,
    pub radio: bool,
    pub driver: Option<EntityId>,
    pub wrecked: bool,
    seat: Vec3,
    door: Option<usize>,
}

impl Car {
    pub fn read(sim: &Sim, id: EntityId) -> Option<Car> {
        let e = sim.state.entities.get(id)?;
        let v = e.vehicle.as_ref()?;
        let vel = e
            .body
            .and_then(|h| sim.state.physics.bodies.get(h))
            .map(|b| {
                let l = b.linvel();
                Vec3::new(l.x, l.y, l.z)
            })
            .unwrap_or(Vec3::ZERO);
        // The heading from the body (a car's own `yaw` is only kept for kinematic moves and
        // helicopters).
        let f = e.rot * Vec3::Z;
        let yaw = if v.def.kind == VehicleKind::Helicopter || f.x.abs() + f.z.abs() < 1e-3 { v.yaw } else { f.x.atan2(f.z) };
        let seat = v.def.seats.first().copied().unwrap_or(Vec3::new(0.36, 0.55, 0.0));
        let left = seat.x >= 0.0;
        let door = v.def.doors.iter().position(|d| (d.side == Side::Left) == left);
        Some(Car {
            id,
            key: e.name.clone(),
            center: e.pos,
            rot: e.rot,
            yaw,
            vel,
            half: v.def.size,
            base: v.base,
            heli: v.def.kind == VehicleKind::Helicopter,
            siren: v.def.siren,
            radio: v.def.radio,
            driver: v.driver,
            wrecked: v.state.wrecked,
            seat,
            door,
        })
    }

    /// Facing on the ground (unit, no pitch).
    pub fn forward(&self) -> Vec3 {
        Vec3::new(self.yaw.sin(), 0.0, self.yaw.cos())
    }

    /// The car's own left on the ground (its driver's side).
    pub fn left(&self) -> Vec3 {
        Vec3::new(self.yaw.cos(), 0.0, -self.yaw.sin())
    }

    /// The ground under its centre.
    pub fn feet(&self) -> Vec3 {
        self.center - self.rot * Vec3::Y * self.base
    }

    /// Metres per second along its heading (negative backing up).
    pub fn speed(&self) -> f32 {
        self.vel.dot(self.forward())
    }

    /// Where the driver stands to get in: outside the driver's door, on the ground.
    pub fn door_point(&self) -> Vec3 {
        let side = if self.seat.x >= 0.0 { 1.0 } else { -1.0 };
        self.feet() + self.left() * side * (self.half.x + 0.55) + self.forward() * self.seat.z
    }

    /// The driver's door's index in the definition (None: no doors, as a helicopter).
    pub fn door(&self) -> Option<usize> {
        self.door
    }
}

/// A step of getting in or out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stage {
    #[default]
    Walk,
    Smash,
    Open,
    Jack,
    Sit,
    Shut,
}

/// Where the player is with vehicles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Phase {
    #[default]
    Foot,
    /// Getting in: the stage and seconds into it.
    Enter {
        car: EntityId,
        stage: Stage,
        t: f32,
    },
    In {
        car: EntityId,
    },
    /// Braking to a stop before getting out.
    Stopping {
        car: EntityId,
    },
    /// Getting out (Open, then Shut once standing beside the car).
    Exit {
        car: EntityId,
        stage: Stage,
        t: f32,
    },
}

/// What getting in and out did, for the rest of the game (heat, the people around, sounds).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Deed {
    Smashed { car: EntityId, at: Vec3 },
    Jacked { car: EntityId, driver: EntityId, at: Vec3 },
    Entered { car: EntityId },
    Exited { car: EntityId },
    Bailed { car: EntityId, speed: f32 },
}

/// The player's dealings with vehicles (part of the game's state).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Drive {
    pub phase: Phase,
    /// Cars that are locked (a share of the parked ones).
    pub locked: BTreeSet<EntityId>,
    /// Car alarms sounding: seconds left.
    pub alarms: BTreeMap<EntityId, f32>,
    /// Cars rolling on with nobody in them after a bail-out: seconds left.
    pub coasting: BTreeMap<EntityId, f32>,
    /// The siren is on (a police car's), seconds L3 and △ have been held.
    pub siren: bool,
    l3: f32,
    hold: f32,
    /// This tick's input for the player's vehicle (worked out in `pre`, handed to the engine
    /// in `Game::vehicles`).
    pub input: Option<(EntityId, VehicleInput)>,
    /// The smoothed controls.
    smooth: VehicleInput,
    /// The car the player was last in, and seconds since its card (name, station) appeared.
    pub last: Option<EntityId>,
    pub card: f32,
    /// Extra tyre grip from the driver's ability (Slipstream), as a share of the tyres' own.
    pub grip: f32,
}

impl Drive {
    /// The vehicle the player is in (or getting out of).
    pub fn vehicle(&self) -> Option<EntityId> {
        match self.phase {
            Phase::In { car } | Phase::Stopping { car } => Some(car),
            Phase::Exit { car, stage: Stage::Open, .. } => Some(car),
            _ => None,
        }
    }

    /// Getting in or out right now (the on-foot controls and weapons wait).
    pub fn busy(&self) -> bool {
        matches!(self.phase, Phase::Enter { .. } | Phase::Exit { .. })
    }

    /// The nearest vehicle the player could get into from `feet`, and the distance to its
    /// driver's door.
    pub fn nearest(sim: &Sim, feet: Vec3) -> Option<(EntityId, f32)> {
        sim.state
            .entities
            .iter()
            .filter(|e| e.vehicle.as_ref().is_some_and(|v| !v.state.wrecked))
            .filter(|e| e.pos.distance(feet) < REACH + 8.0)
            .filter_map(|e| {
                let c = Car::read(sim, e.id)?;
                // On the same level (not a car on the freeway deck overhead).
                if (c.center.y - c.half.y - feet.y).abs() > 2.5 {
                    return None;
                }
                let d = c.door_point();
                let flat = Vec3::new(d.x - feet.x, 0.0, d.z - feet.z).length();
                // The body's middle counts too, so standing at the bonnet works.
                let mid = Vec3::new(c.center.x - feet.x, 0.0, c.center.z - feet.z).length() - c.half.x.max(c.half.z) * 0.7;
                Some((e.id, flat.min(mid.max(0.0))))
            })
            .filter(|(_, d)| *d < REACH)
            .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
    }

    /// On foot, △ pressed: start getting into the nearest vehicle. False when none is near.
    pub fn begin_enter(&mut self, sim: &Sim, feet: Vec3) -> bool {
        match Self::nearest(sim, feet) {
            Some((car, _)) => {
                self.phase = Phase::Enter { car, stage: Stage::Walk, t: 0.0 };
                true
            }
            None => false,
        }
    }

    /// Puts the player straight into a vehicle (scripts, the `drive` command, respawns).
    pub fn seat(&mut self, sim: &mut Sim, pid: EntityId, car: EntityId) -> bool {
        if !sim.enter_vehicle_seat(pid, car, 0) {
            return false;
        }
        self.phase = Phase::In { car };
        self.last = Some(car);
        self.card = 0.0;
        true
    }

    /// One tick of getting in or out: the player's walk to the door (an input for the
    /// player's character, while walking) and what happened. `dt` is game time.
    pub fn tick(
        &mut self,
        sim: &mut Sim,
        pid: EntityId,
        input: &InputFrame,
        dt: f32,
        deeds: &mut Vec<Deed>,
        events: &mut Vec<SimEvent>,
    ) -> Option<InputFrame> {
        for t in self.alarms.values_mut() {
            *t -= dt;
        }
        self.alarms.retain(|id, t| *t > 0.0 && sim.state.entities.get(*id).is_some());
        for t in self.coasting.values_mut() {
            *t -= dt;
        }
        self.coasting.retain(|id, t| *t > 0.0 && sim.state.entities.get(*id).is_some());
        self.card += dt;
        let sound = |events: &mut Vec<SimEvent>, cue: &str, at: Vec3| {
            events.push(SimEvent::Sound { cue: cue.into(), pos: Some(at), gain: 1.0, pitch: 1.0 })
        };
        match self.phase {
            Phase::Foot | Phase::In { .. } | Phase::Stopping { .. } => None,
            Phase::Enter { car, stage, t } => {
                let Some(c) = Car::read(sim, car).filter(|c| !c.wrecked) else {
                    self.phase = Phase::Foot;
                    return None;
                };
                let t = t + dt;
                let feet = sim.feet(pid).unwrap_or(c.door_point());
                let door = c.door_point();
                let next = |stage: Stage| Phase::Enter { car, stage, t: 0.0 };
                let mut walk = None;
                self.phase = match stage {
                    Stage::Walk => {
                        let to = Vec3::new(door.x - feet.x, 0.0, door.z - feet.z);
                        // Pushing the stick away from the car calls it off.
                        let stick = input.pad.stick_l;
                        if stick.length() > 0.7 && t > 0.15 {
                            self.phase = Phase::Foot;
                            return None;
                        }
                        if to.length() < 0.45 || t >= WALK_MAX {
                            sim.set_position(pid, Vec3::new(door.x, feet.y.max(door.y), door.z));
                            if self.locked.contains(&car) { next(Stage::Smash) } else { next(Stage::Open) }
                        } else {
                            let dir = to.normalize_or_zero();
                            walk = Some(InputFrame { move_dir: glam::Vec2::new(dir.x, dir.z), ..Default::default() });
                            Phase::Enter { car, stage, t }
                        }
                    }
                    Stage::Smash if t >= SMASH => {
                        self.locked.remove(&car);
                        self.alarms.insert(car, ALARM);
                        sound(events, "glass", door + Vec3::Y * 1.1);
                        deeds.push(Deed::Smashed { car, at: door });
                        next(Stage::Open)
                    }
                    Stage::Open if t < dt * 1.5 => {
                        if let Some(d) = c.door() {
                            sim.set_vehicle_door(car, d, 1.0);
                        }
                        sound(events, "car.door_open", door + Vec3::Y);
                        Phase::Enter { car, stage, t }
                    }
                    Stage::Open if t >= OPEN => next(if c.driver.is_some_and(|d| d != pid) { Stage::Jack } else { Stage::Sit }),
                    Stage::Jack if t >= JACK * 0.5 && c.driver.is_some_and(|d| d != pid) => {
                        let driver = c.driver.unwrap();
                        sim.exit_vehicle(driver, car);
                        // Out on the ground, shoved away from the door.
                        sim.hit_character(driver, door, c.left() * if c.seat.x >= 0.0 { 1.0 } else { -1.0 }, 5.0, false, events);
                        deeds.push(Deed::Jacked { car, driver, at: door });
                        Phase::Enter { car, stage, t }
                    }
                    Stage::Jack if t >= JACK => next(Stage::Sit),
                    Stage::Sit if t >= SIT => {
                        if !sim.enter_vehicle_seat(pid, car, 0) {
                            if let Some(d) = c.door() {
                                sim.set_vehicle_door(car, d, 0.0);
                            }
                            self.phase = Phase::Foot;
                            return None;
                        }
                        next(Stage::Shut)
                    }
                    Stage::Shut if t < dt * 1.5 => {
                        if let Some(d) = c.door() {
                            sim.set_vehicle_door(car, d, 0.0);
                        }
                        sound(events, "car.door_close", door + Vec3::Y);
                        Phase::Enter { car, stage, t }
                    }
                    Stage::Shut if t >= SHUT => {
                        deeds.push(Deed::Entered { car });
                        self.last = Some(car);
                        self.card = 0.0;
                        self.hold = 0.0;
                        Phase::In { car }
                    }
                    _ => Phase::Enter { car, stage, t },
                };
                // Face the car while at its door.
                if walk.is_none() {
                    let to = c.center - feet;
                    if let Some(ch) = sim.state.entities.get_mut(pid).and_then(|e| e.character.as_mut()) {
                        ch.face = Some(to.x.atan2(to.z));
                    }
                }
                walk.or(Some(InputFrame::default()))
            }
            Phase::Exit { car, stage, t } => {
                let t = t + dt;
                let c = Car::read(sim, car);
                let door = c.as_ref().map(|c| c.door());
                match stage {
                    Stage::Open if t < dt * 1.5 => {
                        if let Some(Some(d)) = door {
                            sim.set_vehicle_door(car, d, 1.0);
                        }
                        if let Some(c) = &c {
                            sound(events, "car.door_open", c.door_point() + Vec3::Y);
                        }
                        self.phase = Phase::Exit { car, stage, t };
                    }
                    Stage::Open if t >= OPEN => {
                        sim.exit_vehicle(pid, car);
                        self.siren = false;
                        self.phase = Phase::Exit { car, stage: Stage::Shut, t: 0.0 };
                    }
                    Stage::Shut if t >= SHUT => {
                        if let Some(Some(d)) = door {
                            sim.set_vehicle_door(car, d, 0.0);
                        }
                        if let Some(c) = &c {
                            sound(events, "car.door_close", c.door_point() + Vec3::Y);
                        }
                        deeds.push(Deed::Exited { car });
                        self.phase = Phase::Foot;
                    }
                    _ => self.phase = Phase::Exit { car, stage, t },
                }
                Some(InputFrame::default())
            }
        }
    }

    /// In a vehicle: this tick's input for it from the pad, and △ (stop and get out; held at
    /// speed, bail out). Sets `self.input`. `dt` is game time.
    pub fn steer(
        &mut self,
        sim: &mut Sim,
        pid: EntityId,
        input: &InputFrame,
        dt: f32,
        deeds: &mut Vec<Deed>,
        events: &mut Vec<SimEvent>,
    ) {
        self.input = None;
        let car = match self.phase {
            Phase::In { car } | Phase::Stopping { car } => car,
            _ => return,
        };
        let Some(c) = Car::read(sim, car) else {
            self.phase = Phase::Foot;
            return;
        };
        // Thrown clear when it blows up, or put out when the engine let them go (water).
        let riding = sim.state.entities.get(pid).and_then(|e| e.character.as_ref()).and_then(|ch| ch.riding);
        if riding != Some(car) {
            self.phase = Phase::Foot;
            self.siren = false;
            return;
        }
        let p = &input.pad;
        let speed = c.speed();
        let north = p.held & pad::NORTH != 0;
        if let Phase::In { .. } = self.phase {
            if north {
                self.hold += dt;
                if speed.abs() >= BAIL_SPEED && self.hold >= BAIL_HOLD && !c.heli {
                    self.bail(sim, pid, &c, deeds, events);
                    return;
                }
            } else if self.hold > 0.0 {
                // Let go (a tap, or too slow to bail): stop, then get out.
                self.hold = 0.0;
                self.phase = if speed.abs() < 3.0 || c.heli && c.vel.length() < 3.0 {
                    Phase::Exit { car, stage: Stage::Open, t: 0.0 }
                } else {
                    Phase::Stopping { car }
                };
            }
        }
        if let Phase::Stopping { .. } = self.phase {
            if speed.abs() < 1.0 {
                self.phase = Phase::Exit { car, stage: Stage::Open, t: 0.0 };
            }
            self.input = Some((car, VehicleInput { brake: 1.0, ..Default::default() }));
            return;
        }
        let ramp = |v: &mut f32, to: f32| {
            let rate = if to > *v { 1.0 / 0.15 } else { 12.0 };
            *v += (to - *v).clamp(-rate * dt, rate * dt);
        };
        ramp(&mut self.smooth.throttle, p.r2.clamp(0.0, 1.0));
        ramp(&mut self.smooth.brake, p.l2.clamp(0.0, 1.0));
        let steer_to = if c.heli { (p.stick_l.x + p.stick_r.x).clamp(-1.0, 1.0) } else { p.stick_l.x.clamp(-1.0, 1.0) };
        let rate = 1.0 / 0.12;
        self.smooth.steer += (steer_to - self.smooth.steer).clamp(-rate * dt, rate * dt);
        // L3: the horn while held; tapped in a police car, the siren.
        let l3 = p.held & pad::L3 != 0 && p.held & pad::R3 == 0;
        if l3 {
            self.l3 += dt;
        } else {
            if self.l3 > 0.0 && self.l3 < TAP && c.siren {
                self.siren = !self.siren;
            }
            self.l3 = 0.0;
        }
        let horn = l3 && (!c.siren || self.l3 >= TAP);
        self.input = Some((
            car,
            VehicleInput {
                throttle: self.smooth.throttle,
                brake: self.smooth.brake,
                steer: self.smooth.steer,
                pitch: p.stick_l.y.clamp(-1.0, 1.0),
                // (Aiming a drive-by, R1 fires instead.)
                handbrake: p.held & pad::R1 != 0 && p.held & pad::L1 == 0 && !c.heli,
                horn,
                siren: self.siren && c.siren,
                grip: self.grip,
            },
        ));
    }

    /// Out of a moving car: thrown clear with a roll, hurt by the speed; the car rolls on.
    fn bail(&mut self, sim: &mut Sim, pid: EntityId, c: &Car, deeds: &mut Vec<Deed>, events: &mut Vec<SimEvent>) {
        let speed = c.speed().abs();
        sim.exit_vehicle(pid, c.id);
        let side = c.left() * if c.seat.x >= 0.0 { 1.0 } else { -1.0 };
        let feet = c.door_point() + side * 0.6;
        sim.set_position(pid, feet + Vec3::Y * 0.1);
        if let Some(ch) = sim.state.entities.get_mut(pid).and_then(|e| e.character.as_mut()) {
            ch.vel = c.forward() * speed * 0.35 + side * 2.5;
        }
        sim.hit_character(pid, feet, c.forward() * 0.6 + side, 4.0 + speed * 0.25, false, events);
        self.coasting.insert(c.id, 6.0);
        self.siren = false;
        self.hold = 0.0;
        self.phase = Phase::Foot;
        deeds.push(Deed::Bailed { car: c.id, speed });
    }

    /// The inputs this tick for vehicles the player deals with: the one being driven, and
    /// any rolling on after a bail-out (no throttle, no brake: they coast).
    pub fn inputs(&self, out: &mut BTreeMap<EntityId, VehicleInput>) {
        for &id in self.coasting.keys() {
            out.insert(id, VehicleInput::default());
        }
        if let Some((id, vi)) = self.input {
            out.insert(id, vi);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phases_name_the_vehicle_while_inside() {
        let car = EntityId(7);
        let mut d = Drive::default();
        assert_eq!(d.vehicle(), None);
        d.phase = Phase::In { car };
        assert_eq!(d.vehicle(), Some(car));
        d.phase = Phase::Exit { car, stage: Stage::Shut, t: 0.0 };
        assert_eq!(d.vehicle(), None, "standing beside it once out");
        assert!(d.busy());
    }
}
