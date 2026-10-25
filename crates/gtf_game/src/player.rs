//! On foot (PLAN.md §5.6.1): the controller turned into the player character's input, relative
//! to the camera. A light tilt walks (1.6 m/s), full tilt runs (4.6 m/s), ✕ held or tapped
//! sprints (7 m/s) on stamina; □ jumps, and the engine vaults low walls and climbs ledges from
//! it; L3 toggles a stealthy crouch. Movement tuning for the city is set once per scene.

use pav_core::InputFrame;
use pav_core::character::{MovementModel, MovementParams};
use pav_core::input::{buttons, pad};
use serde::{Deserialize, Serialize};

use crate::camera::OrbitCam;

/// Run and sprint speeds (m/s).
pub const RUN: f32 = 4.6;
pub const SPRINT: f32 = 7.0;
pub const WALK: f32 = 1.6;
/// The stick tilted less than this walks.
pub const WALK_TILT: f32 = 0.55;

/// Sprint taps closer together than this keep the sprint going (s).
const TAP_WINDOW: f32 = 0.4;

/// How a person moves in Paloma Bay (applied to the scene's movement parameters).
pub fn tune(m: &mut MovementParams) {
    m.model = MovementModel::Instant;
    m.speed = RUN;
    m.focus_speed = WALK;
    m.gravity = 22.0;
    m.jump_height = 1.1;
    m.air_control = 0.3;
    m.step_height = 0.38;
    m.ledge_grab = true;
    m.ledge_climb_time = 0.55;
    m.climb_speed = 2.4;
    m.swim_speed = 2.0;
    m.max_fall = 40.0;
    m.crouch_mult = 0.5;
}

/// The player's on-foot state (part of the game's state).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Foot {
    /// 0..1 of the sprint the stamina stat allows.
    pub stamina: f32,
    pub sprinting: bool,
    pub crouched: bool,
    /// Seconds since sprinting stopped (stamina refills after a rest) and since the last tap.
    pub rest: f32,
    pub since_tap: f32,
}

impl Default for Foot {
    fn default() -> Self {
        Foot { stamina: 1.0, sprinting: false, crouched: false, rest: 9.0, since_tap: 9.0 }
    }
}

impl Foot {
    /// Seconds of sprint a full bar holds: 10 s at stamina stat 0, +50% at 100.
    pub fn capacity(stat: f32) -> f32 {
        10.0 * (1.0 + 0.5 * (stat / 100.0).clamp(0.0, 1.0))
    }

    /// The player character's input this tick, and the speed boost to give the character
    /// (`Character::haste`). `dt` is game time.
    pub fn input(&mut self, input: &InputFrame, cam: &OrbitCam, stat: f32, dt: f32) -> (InputFrame, f32) {
        let (stick, world) = crate::pad::stick_or_move(input);
        let move_dir = if world { stick } else { cam.world_move(stick) }.clamp_length_max(1.0);
        let moving = stick.length() > 0.3;
        // ✕: held, or tapped again within the window.
        let held_x = input.pad.held & pad::SOUTH != 0 || input.held & buttons::SPRINT != 0;
        let tapped = input.pad.pressed & pad::SOUTH != 0;
        self.since_tap = if tapped { 0.0 } else { self.since_tap + dt };
        let wants = moving && (held_x || self.since_tap < TAP_WINDOW) && !self.crouched;
        let cap = Self::capacity(stat);
        if wants && self.stamina > 0.0 {
            self.sprinting = true;
            self.stamina = (self.stamina - dt / cap).max(0.0);
            self.rest = 0.0;
        } else {
            self.sprinting = false;
            self.rest += dt;
            if self.rest > 1.0 {
                // Two seconds of sprint back per second of rest.
                self.stamina = (self.stamina + 2.0 * dt / cap).min(1.0);
            }
        }
        // L3 toggles the crouch (on release, so the ability chord never crouches).
        if input.pad.pressed & pad::L3 != 0 && input.pad.held & pad::R3 == 0 {
            self.crouched = !self.crouched;
        }
        let mut held = 0;
        let mut pressed = 0;
        if input.pad.held & pad::WEST != 0 {
            held |= buttons::JUMP;
        }
        if input.pad.pressed & pad::WEST != 0 {
            pressed |= buttons::JUMP;
            // Jumping stands you up.
            self.crouched = false;
        }
        if self.crouched {
            held |= buttons::CROUCH;
        }
        // Barely tilting the stick walks: the walk band's edge is the full walk (the controller
        // goes as fast as the stick is pushed), a lighter tilt a slower one.
        let mut move_dir = move_dir;
        if stick.length() > 0.05 && stick.length() < WALK_TILT && !self.sprinting {
            held |= buttons::FOCUS;
            move_dir = (move_dir / WALK_TILT).clamp_length_max(1.0);
        }
        let haste = if self.sprinting { SPRINT / RUN - 1.0 } else { 0.0 };
        (InputFrame { move_dir, held: held | pressed, pressed, ..Default::default() }, haste)
    }
}
