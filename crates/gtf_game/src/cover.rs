//! Taking cover (PLAN.md §5.6.1). R1 within 3 m of a wall, a low wall, a planter or a container
//! (the district's cover points, §6.6.6) slides the player against it, facing the wall with the
//! threat beyond it — crouched behind low cover, standing at tall cover. The stick moves along
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

/// Where the player is in cover.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Spot {
    /// The point on the ground behind the wall, the direction away from the wall, and whether
    /// the wall is tall enough to stand behind.
    pub pos: Vec3,
    pub normal: Vec2,
    pub high: bool,
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

    /// R1: the nearest cover point within reach whose wall stands between the player and where
    /// the camera looks. False when there is none.
    pub fn enter(&mut self, city: &City, feet: Vec3, look: Vec3) -> bool {
        let flat = Vec3::new(look.x, 0.0, look.z).normalize_or(Vec3::NEG_Z);
        let threat = feet + flat * 25.0;
        match city.cover.near(feet, threat, REACH).first() {
            Some(p) => {
                self.at = Some(Spot { pos: p.pos, normal: p.normal, high: p.high });
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
        // Along the wall, while the cover lasts.
        let tangent = Vec2::new(spot.normal.y, -spot.normal.x);
        let along = dir.dot(tangent);
        if along.abs() > 0.25 && !self.peeking {
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
        if let Some(c) = sim.state.entities.get_mut(pid).and_then(|e| e.character.as_mut()) {
            c.face = None;
        }
    }
}
