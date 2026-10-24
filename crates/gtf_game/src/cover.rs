//! Taking cover (PLAN.md §5.6.1). R1 within 3 m of a wall, a low wall, a planter or a container
//! (the district's cover points, §6.6.6) or a stopped car slides the player against it, facing
//! the wall with the threat beyond it — crouched behind low cover and cars, standing at tall
//! cover; a car that drives off takes the cover with it. The stick moves along
//! the wall (stopping at its end); L2 rises to peek and aim; R2 without L2 fires blind over or
//! round it (three times the spread); R1 again, or pushing away from the wall, leaves. Shots
//! hit far less often while in cover and not peeking.

use glam::{Vec2, Vec3};
use pav_core::InputFrame;
use pav_core::entity::EntityId;
use pav_core::input::{buttons, pad};
use pav_core::sim::Sim;
use pav_world::City;
use serde::{Deserialize, Serialize};

use crate::camera::OrbitCam;

/// How near a cover point must be to take it (m), and the speed along a wall (m/s).
pub const REACH: f32 = 3.0;
const ALONG: f32 = 1.8;
/// The poses held behind low and tall cover (facing the wall).
pub const POSE_LOW: &str = "WORKSHOP/Cover_Low";
pub const POSE_HIGH: &str = "WORKSHOP/Cover_High";

/// Where the player is in cover.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Spot {
    /// The point on the ground behind the wall, the direction away from the wall, and whether
    /// the wall is tall enough to stand behind.
    pub pos: Vec3,
    pub normal: Vec2,
    pub high: bool,
    /// The car it is behind (cars are cover too).
    #[serde(default)]
    pub car: Option<EntityId>,
}

/// The spot `gap` metres out from a stopped car on the side facing `from` (and that side's
/// direction): where to crouch behind it against a threat on the far side.
pub fn behind_car(sim: &Sim, car: EntityId, from: Vec3, gap: f32) -> Option<(Vec3, Vec2)> {
    behind_car_moving(sim, car, from, gap, 1.0)
}

/// [`behind_car`] for a car going no faster than `speed` (m/s): in cover already, a bump does
/// not end it; being driven off does.
fn behind_car_moving(sim: &Sim, car: EntityId, from: Vec3, gap: f32, speed: f32) -> Option<(Vec3, Vec2)> {
    let c = crate::drive::Car::read(sim, car)?;
    if c.heli || c.vel.length() > speed {
        return None;
    }
    let out = Vec2::new(from.x - c.center.x, from.z - c.center.z).normalize_or(Vec2::X);
    let fwd = Vec2::new(c.forward().x, c.forward().z);
    let left = Vec2::new(c.left().x, c.left().z);
    let ext = out.dot(fwd).abs() * c.half.z + out.dot(left).abs() * c.half.x;
    let feet = c.feet();
    Some((Vec3::new(c.center.x + out.x * (ext + gap), feet.y, c.center.z + out.y * (ext + gap)), out))
}

/// The player's cover (part of the game's state).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Cover {
    pub at: Option<Spot>,
    /// Up and aiming over or round it this tick.
    pub peeking: bool,
    /// Taken this tick (the R1 that took it does not also leave it).
    #[serde(default)]
    pub fresh: bool,
}

impl Cover {
    /// In cover and not exposed (peeking shows the player).
    pub fn covered(&self) -> bool {
        self.at.is_some() && !self.peeking
    }

    /// R1: the nearest cover within reach — a cover point, or a stopped car — that stands between
    /// the player and where the camera looks. False when there is none.
    pub fn enter(&mut self, city: &City, sim: &Sim, feet: Vec3, look: Vec3) -> bool {
        let flat = Vec3::new(look.x, 0.0, look.z).normalize_or(Vec3::NEG_Z);
        let threat = feet + flat * 25.0;
        let point =
            city.cover.near(feet, threat, REACH).first().map(|p| Spot { pos: p.pos, normal: p.normal, high: p.high, car: None });
        // A stopped car close by, between the player and the look.
        let car = sim
            .state
            .entities
            .iter()
            .filter(|e| e.vehicle.is_some() && e.pos.distance(feet) < REACH + 3.0)
            .filter_map(|e| {
                let (pos, normal) = behind_car(sim, e.id, feet, 0.55)?;
                let toward = Vec2::new(flat.x, flat.z).dot(-normal) > 0.4;
                (toward && pos.distance(feet) < REACH).then_some(Spot { pos, normal, high: false, car: Some(e.id) })
            })
            .min_by(|a, b| a.pos.distance(feet).total_cmp(&b.pos.distance(feet)));
        let best = match (point, car) {
            (Some(p), Some(c)) => Some(if c.pos.distance(feet) < p.pos.distance(feet) { c } else { p }),
            (p, c) => p.or(c),
        };
        match best {
            Some(spot) => {
                self.at = Some(spot);
                self.peeking = false;
                self.fresh = true;
                true
            }
            None => false,
        }
    }

    /// One tick in cover: the character's input (stepping onto the cover line, moving along the
    /// wall), or None when the player left it this tick.
    pub fn step(
        &mut self,
        city: &City,
        sim: &mut Sim,
        pid: EntityId,
        input: &InputFrame,
        cam: &OrbitCam,
        dt: f32,
    ) -> Option<InputFrame> {
        let mut spot = self.at?;
        let feet = sim.feet(pid)?;
        let (stick, world) = crate::pad::stick_or_move(input);
        let dir = if world { stick } else { cam.world_move(stick) };
        // Leaving: R1 again, or pushing away from the wall.
        let fresh = std::mem::take(&mut self.fresh);
        if (input.pad.pressed & pad::R1 != 0 && !fresh) || dir.dot(spot.normal) > 0.8 {
            self.leave(sim, pid);
            return None;
        }
        self.peeking = input.pad.l2 > 0.5 || input.pad.held & pad::L2 != 0;
        // Behind a car: it keeps the spot beside it, or drives off with the cover.
        if let Some(car) = spot.car {
            let threat = spot.pos - Vec3::new(spot.normal.x, 0.0, spot.normal.y) * 6.0;
            match behind_car_moving(sim, car, spot.pos + Vec3::new(spot.normal.x, 0.0, spot.normal.y) * 2.0, 0.55, 3.0) {
                Some((pos, normal)) if (threat - pos).dot(Vec3::new(normal.x, 0.0, normal.y)) < 0.0 => {
                    spot.pos = pos;
                    spot.normal = normal;
                }
                _ => {
                    self.leave(sim, pid);
                    return None;
                }
            }
        }
        // Along the wall, while the cover lasts.
        let tangent = Vec2::new(spot.normal.y, -spot.normal.x);
        let along = dir.dot(tangent);
        if along.abs() > 0.25 && !self.peeking && spot.car.is_none() {
            let step = tangent * along.signum() * ALONG * dt;
            let next = spot.pos + Vec3::new(step.x, 0.0, step.y);
            let ahead = next + Vec3::new(tangent.x, 0.0, tangent.y) * along.signum() * 0.4;
            let still = city.cover.near(ahead, ahead - Vec3::new(spot.normal.x, 0.0, spot.normal.y) * 10.0, 1.0);
            if let Some(p) = still.iter().find(|p| p.normal.dot(spot.normal) > 0.7) {
                // Slide along, keeping to the wall's line (and its height class).
                let line = Vec2::new(p.pos.x, p.pos.z);
                let off = (Vec2::new(next.x, next.z) - line).dot(spot.normal);
                spot.pos = Vec3::new(next.x - spot.normal.x * off, p.pos.y, next.z - spot.normal.y * off);
                spot.high = p.high;
                spot.car = None;
            }
        }
        self.at = Some(spot);
        // Face the wall unless peeking (aiming turns the body itself).
        if let Some(c) = sim.state.entities.get_mut(pid).and_then(|e| e.character.as_mut())
            && !self.peeking
        {
            c.face = Some((-spot.normal.x).atan2(-spot.normal.y));
        }
        // Slide onto the cover line (quickly: 5 m/s), then hold there.
        let to = Vec2::new(spot.pos.x - feet.x, spot.pos.z - feet.z);
        if to.length() > 0.05 {
            let step = to.clamp_length_max(5.0 * dt);
            sim.set_position(pid, Vec3::new(feet.x + step.x, feet.y.max(spot.pos.y), feet.z + step.y));
        }
        // The pose, held still behind it (crouched at low cover, flat to tall cover); moving
        // along the wall or peeking, the body's own animation.
        let pose = pav_core::clips::find_cached(if spot.high { POSE_HIGH } else { POSE_LOW });
        let still = along.abs() <= 0.25 && to.length() < 0.3 && !self.peeking;
        if let Some(c) = sim.state.entities.get_mut(pid).and_then(|e| e.character.as_mut())
            && pose != 0
        {
            if still {
                c.anim.play_clip(pose, 0, 1.0);
            } else if c.anim.clip == pose {
                c.anim.stop_clip();
            }
        }
        let mut held = buttons::FOCUS;
        if !spot.high && !self.peeking {
            held |= buttons::CROUCH;
        }
        Some(InputFrame { held, ..Default::default() })
    }

    /// Out of cover.
    pub fn leave(&mut self, sim: &mut Sim, pid: EntityId) {
        self.at = None;
        self.peeking = false;
        let poses = [pav_core::clips::find_cached(POSE_LOW), pav_core::clips::find_cached(POSE_HIGH)];
        if let Some(c) = sim.state.entities.get_mut(pid).and_then(|e| e.character.as_mut()) {
            c.face = None;
            if c.anim.clip != 0 && poses.contains(&c.anim.clip) {
                c.anim.stop_clip();
            }
        }
    }
}
