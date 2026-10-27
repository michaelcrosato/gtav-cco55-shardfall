//! People struck by vehicles (PLAN.md §5.6.7, D4). A vehicle moving faster than a brisk walk
//! that is about to hit someone throws them along its path by the impact (the character's
//! knock-back), then the fall clip plays: up again and away, or dead above 12 m/s. Their capsule
//! stops colliding for a moment, so the car carries on through rather than stopping dead as it
//! would against a post. Slower, the player's car driven into someone shoves them aside (boxed
//! in by officers on foot, a crowd at the bumper): unhurt, and no strike.

use glam::Vec3;
use pav_core::entity::EntityId;
use pav_core::frame::SimEvent;
use pav_core::sim::Sim;
use serde::{Deserialize, Serialize};

/// Struck at this speed or more, a person dies (m/s).
pub const LETHAL: f32 = 12.0;
/// Below this a car only nudges people (m/s).
const NUDGE: f32 = 3.0;
/// How long a struck person's capsule does not collide (s).
const GHOST: f32 = 1.5;
/// The knock-back of a shove aside (m/s).
const SHOVE: f32 = 3.5;

/// Who was hit, by what, where and how fast.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Strike {
    pub who: EntityId,
    pub car: EntityId,
    pub at: Vec3,
    pub dir: Vec3,
    pub speed: f32,
}

/// Recently struck people (part of the game's state).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Struck {
    /// People whose capsules do not collide, and for how much longer.
    pub ghosts: Vec<(EntityId, f32)>,
    pub count: u32,
}

/// Turns a character's colliders into sensors (passing through) or back.
fn ghost(sim: &mut Sim, id: EntityId, on: bool) {
    let Some(body) = sim.state.entities.get(id).and_then(|e| e.body) else { return };
    let Some(b) = sim.state.physics.bodies.get(body) else { return };
    for c in b.colliders().to_vec() {
        if let Some(col) = sim.state.physics.colliders.get_mut(c) {
            col.set_sensor(on);
        }
    }
}

impl Struck {
    /// Before the physics step: vehicles about to run into any of `people` strike them; `shove`
    /// is the player's car and the way it is driven (1 ahead, -1 backing up) when a pedal is down.
    pub fn tick(
        &mut self,
        sim: &mut Sim,
        people: &[EntityId],
        shove: Option<(EntityId, f32)>,
        dt: f32,
        events: &mut Vec<SimEvent>,
    ) -> Vec<Strike> {
        for g in &mut self.ghosts {
            g.1 -= dt;
        }
        let done: Vec<EntityId> = self.ghosts.iter().filter(|g| g.1 <= 0.0).map(|g| g.0).collect();
        self.ghosts.retain(|g| g.1 > 0.0);
        for id in done {
            let riding = sim.state.entities.get(id).and_then(|e| e.character.as_ref()).is_some_and(|c| c.riding.is_some());
            if !riding {
                ghost(sim, id, false);
            }
        }
        let mut hits = Vec::new();
        let cars: Vec<crate::drive::Car> = sim
            .state
            .entities
            .iter()
            .filter(|e| e.vehicle.is_some())
            .filter_map(|e| crate::drive::Car::read(sim, e.id))
            .filter(|c| !c.heli && c.speed().abs() > NUDGE)
            .collect();
        for c in &cars {
            let speed = c.speed();
            let (fwd, left) = (c.forward() * speed.signum(), c.left());
            let feet = c.feet();
            for &who in people {
                if self.ghosts.iter().any(|g| g.0 == who) {
                    continue;
                }
                let Some(p) = sim.feet(who) else { continue };
                let to = p - feet;
                if to.length() > 12.0 || to.y.abs() > 2.0 {
                    continue;
                }
                // In front (or behind, backing up), within the car's width.
                let z = to.dot(fwd);
                let x = to.dot(left);
                if z > c.half.z - 0.6 && z < c.half.z + 0.6 + speed.abs() * 0.08 && x.abs() < c.half.x + 0.35 {
                    let dir = (c.vel.normalize_or(fwd) + Vec3::Y * 0.15).normalize();
                    let at = p + Vec3::Y;
                    ghost(sim, who, true);
                    self.ghosts.push((who, GHOST));
                    let knock = (speed.abs() * 0.9).clamp(5.0, 20.0);
                    sim.hit_character(who, at, dir, knock, false, events);
                    events.push(SimEvent::Sound { cue: "impact.light".into(), pos: Some(at), gain: 1.0, pitch: 1.0 });
                    hits.push(Strike { who, car: c.id, at, dir, speed: speed.abs() });
                    self.count += 1;
                }
            }
        }
        if let Some((car, way)) = shove
            && let Some(c) = crate::drive::Car::read(sim, car)
            && c.speed().abs() <= NUDGE
        {
            let (fwd, left) = (c.forward() * way, c.left());
            for &who in people {
                let Some(p) = sim.feet(who) else { continue };
                let to = p - c.feet();
                let (z, x) = (to.dot(fwd), to.dot(left));
                if z > c.half.z - 0.5 && z < c.half.z + 1.0 && x.abs() < c.half.x + 0.6 && to.y.abs() < 2.0 {
                    // Aside, the nearer way, and a little ahead of the bumper.
                    let side = left * if x >= 0.0 { 1.0 } else { -1.0 };
                    sim.hit_character(who, p + Vec3::Y, (side + fwd * 0.4).normalize(), SHOVE, false, events);
                }
            }
        }
        hits
    }
}
