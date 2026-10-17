//! Damage. Contact forces on a vehicle's body (rapier's contact force events, on for every
//! vehicle collider) become crashes: one [`SimEvent::Crash`] per hit for sounds and effects,
//! and past a few metres per second of change in speed a dent in the crumple zone it was hit in
//! (front, back, left, right), broken lamps behind a deep front or back dent, and lost health.
//! Below [`SMOKE_BELOW`] of its health a vehicle smokes, below [`FIRE_BELOW`] it burns, and
//! after [`EXPLODE_AFTER`] seconds of burning (at once if it lies on its roof while burning) it
//! explodes: a blast that pushes everything near, a wreck, and nearby vehicles set burning half
//! a second from going up themselves. Games add damage of their own (bullets, rams) with
//! [`Sim::damage_vehicle`], burst tyres and break lamps, repair, or blow a vehicle up.

use std::collections::BTreeMap;

use glam::Vec3;

use super::Vehicle;
use crate::entity::EntityId;
use crate::frame::SimEvent;
use crate::physics::{ContactForce, entity_from_tag};
use crate::sim::Sim;

/// A vehicle smokes below this share of its health, and burns below the next.
pub const SMOKE_BELOW: f32 = 0.4;
pub const FIRE_BELOW: f32 = 0.1;
/// Seconds a burning vehicle lasts before it explodes, and the blast's radius (m).
pub const EXPLODE_AFTER: f32 = 5.0;
pub const EXPLOSION_RADIUS: f32 = 6.0;
/// A crash's change in speed (m/s) that starts to cost health, and health per m/s beyond it
/// (a car hitting a wall at 50 km/h loses about half of 1000).
const HURT_FROM: f32 = 2.5;
const HURT_PER: f32 = 45.0;
/// Dent per m/s beyond that, and the dent that breaks the lamps on that end.
const DENT_PER: f32 = 0.04;
const LAMPS_BREAK: f32 = 0.45;
/// The smallest change in speed that makes a crash event, and the gap between a vehicle's
/// events (bigger hits always get one).
const EVENT_FROM: f32 = 0.6;
const EVENT_GAP: f32 = 0.15;

/// Takes `amount` of health; at the fire line it catches fire.
fn hurt(v: &mut Vehicle, amount: f32) {
    if v.state.wrecked || amount <= 0.0 {
        return;
    }
    v.state.health = (v.state.health - amount).max(0.0);
    if v.health_share() < FIRE_BELOW {
        v.state.burning = true;
    }
}

impl Sim {
    /// Crashes from this tick's contact forces (after the physics step).
    pub(crate) fn vehicle_contacts(&mut self, contacts: &[ContactForce], dt: f32, events: &mut Vec<SimEvent>) {
        // Each vehicle's impulse this tick, with the strongest contact's point and partner.
        let mut hits: BTreeMap<EntityId, (f32, f32, Vec3, Option<EntityId>)> = BTreeMap::new();
        let owner = |c| self.state.physics.colliders.get(c).and_then(|x| entity_from_tag(x.user_data)).map(EntityId);
        for c in contacts {
            let (e1, e2) = (owner(c.c1), owner(c.c2));
            for (me, other, point) in [(e1, e2, c.point1), (e2, e1, c.point2)] {
                let Some(me) = me.filter(|id| self.state.entities.get(*id).is_some_and(|e| e.vehicle.is_some())) else {
                    continue;
                };
                let j = c.force * dt;
                let h = hits.entry(me).or_insert((0.0, 0.0, point, other));
                h.0 += j;
                if j > h.1 {
                    (h.1, h.2, h.3) = (j, point, other);
                }
            }
        }
        for (id, (impulse, _, point, other)) in hits {
            self.crash(id, impulse, point, other, events);
        }
    }

    fn crash(&mut self, id: EntityId, impulse: f32, point: Vec3, other: Option<EntityId>, events: &mut Vec<SimEvent>) {
        let Some(e) = self.state.entities.get_mut(id) else { return };
        let (pos, rot) = (e.pos, e.rot);
        let Some(v) = e.vehicle.as_deref_mut() else { return };
        let dv = impulse / v.def.mass.max(1.0);
        if dv >= EVENT_FROM && (v.state.since_crash >= EVENT_GAP || dv > 3.0) {
            v.state.since_crash = 0.0;
            events.push(SimEvent::Crash { vehicle: id, other, impulse, pos: point });
        }
        let excess = dv - HURT_FROM;
        if excess <= 0.0 {
            return;
        }
        // The side it was hit on, measured against the body's size.
        let local = rot.inverse() * (point - pos);
        let h = v.def.size.max(Vec3::splat(0.1));
        let zone = if (local.z / h.z).abs() >= (local.x / h.x).abs() {
            if local.z > 0.0 { 0 } else { 1 }
        } else if local.x > 0.0 {
            2
        } else {
            3
        };
        let dent = &mut v.state.crumple[zone];
        *dent = (*dent + excess * DENT_PER).min(1.0);
        if v.state.crumple[0] > LAMPS_BREAK {
            v.state.lights_broken[0] = true;
            v.state.lights_broken[1] = true;
        }
        if v.state.crumple[1] > LAMPS_BREAK {
            v.state.lights_broken[2] = true;
            v.state.lights_broken[3] = true;
        }
        hurt(v, excess * HURT_PER);
    }

    /// Burning vehicles count down to their explosion (after the physics step).
    pub(crate) fn update_vehicle_damage(&mut self, dt: f32, events: &mut Vec<SimEvent>) {
        let mut blow = Vec::new();
        for e in self.state.entities.map.values_mut() {
            let Some(v) = e.vehicle.as_deref_mut() else { continue };
            if !v.state.burning || v.state.wrecked {
                continue;
            }
            v.state.burn_time += dt;
            if v.state.burn_time >= EXPLODE_AFTER || v.state.upside >= super::car::RIGHT_AFTER {
                blow.push(e.id);
            }
        }
        for id in blow {
            self.explode_vehicle(id, events);
        }
    }

    /// Takes `amount` of a vehicle's health (bullets, rams): at the fire line it burns.
    pub fn damage_vehicle(&mut self, vid: EntityId, amount: f32) -> bool {
        self.state.entities.get_mut(vid).and_then(|e| e.vehicle.as_deref_mut()).map(|v| hurt(v, amount)).is_some()
    }

    /// Bursts tyre `wheel` (0 front left, 1 front right, 2 rear left, 3 rear right): it runs on
    /// its rim with less grip.
    pub fn burst_tyre(&mut self, vid: EntityId, wheel: usize) -> bool {
        match self.state.entities.get_mut(vid).and_then(|e| e.vehicle.as_deref_mut()).and_then(|v| v.state.burst.get_mut(wheel)) {
            Some(b) => {
                *b = true;
                true
            }
            None => false,
        }
    }

    /// Breaks lamp `lamp` (0 head left, 1 head right, 2 tail left, 3 tail right).
    pub fn break_vehicle_light(&mut self, vid: EntityId, lamp: usize) -> bool {
        match self
            .state
            .entities
            .get_mut(vid)
            .and_then(|e| e.vehicle.as_deref_mut())
            .and_then(|v| v.state.lights_broken.get_mut(lamp))
        {
            Some(b) => {
                *b = true;
                true
            }
            None => false,
        }
    }

    /// Puts a vehicle right (health, dents, tyres, lamps, fire, a drowned engine); a wreck stays
    /// a wreck.
    pub fn repair_vehicle(&mut self, vid: EntityId) -> bool {
        let Some(v) = self.state.entities.get_mut(vid).and_then(|e| e.vehicle.as_deref_mut()) else { return false };
        if v.state.wrecked {
            return false;
        }
        let s = &mut v.state;
        s.health = v.def.health;
        s.crumple = [0.0; 4];
        s.burst = [false; 4];
        s.lights_broken = [false; 4];
        s.burning = false;
        s.burn_time = 0.0;
        s.flooded = false;
        s.flood_time = 0.0;
        true
    }

    /// Blows a vehicle up: a blast of [`EXPLOSION_RADIUS`] that pushes everything near
    /// (`SimEvent::Explosion`), a burnt-out wreck thrown up, and vehicles in the blast hurt
    /// (those it sets burning go up half a second later). False if it is already a wreck.
    pub fn explode_vehicle(&mut self, vid: EntityId, events: &mut Vec<SimEvent>) -> bool {
        let Some(e) = self.state.entities.get_mut(vid) else { return false };
        let (pos, body) = (e.pos, e.body);
        let Some(v) = e.vehicle.as_deref_mut() else { return false };
        if v.state.wrecked {
            return false;
        }
        let s = &mut v.state;
        s.wrecked = true;
        s.burning = false;
        s.health = 0.0;
        s.engine_on = false;
        s.siren_on = false;
        s.lights_on = false;
        s.lights_broken = [true; 4];
        for d in &mut s.crumple {
            *d = d.max(0.6);
        }
        let mass = v.def.mass;
        if let Some(b) = body.and_then(|h| self.state.physics.bodies.get_mut(h)) {
            if b.is_dynamic() {
                b.apply_impulse(Vec3::Y * (mass * 5.0), true);
                b.apply_torque_impulse(Vec3::new(0.4, 0.0, 0.25) * (mass * 1.5), true);
            }
        }
        let near: Vec<(EntityId, f32)> = self
            .state
            .entities
            .iter()
            .filter(|o| o.id != vid && o.vehicle.is_some())
            .map(|o| (o.id, o.pos.distance(pos)))
            .filter(|(_, d)| *d < EXPLOSION_RADIUS)
            .collect();
        for (id, d) in near {
            if let Some(o) = self.state.entities.get_mut(id).and_then(|e| e.vehicle.as_deref_mut()) {
                hurt(o, o.def.health * 1.2 * (1.0 - d / EXPLOSION_RADIUS));
                if o.state.burning {
                    o.state.burn_time = o.state.burn_time.max(EXPLODE_AFTER - 0.5);
                }
            }
        }
        self.explode(pos, EXPLOSION_RADIUS, events);
        true
    }
}
