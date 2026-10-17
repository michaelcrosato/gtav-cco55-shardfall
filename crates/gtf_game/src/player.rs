//! On foot (PLAN.md §5.6.1): the controller turned into the player character's input, relative
//! to the camera. Tilt walks, full tilt runs; □ jumps. Sprint, cover, aim and melee arrive in M3.

use pav_core::InputFrame;
use pav_core::input::{buttons, pad};

use crate::camera::OrbitCam;

/// The player character's input this tick.
pub fn on_foot(input: &InputFrame, cam: &OrbitCam) -> InputFrame {
    let (stick, world) = crate::pad::stick_or_move(input);
    let move_dir = if world { stick } else { cam.world_move(stick) }.clamp_length_max(1.0);
    let mut held = 0;
    let mut pressed = 0;
    if input.pad.held & pad::WEST != 0 {
        held |= buttons::JUMP;
    }
    if input.pad.pressed & pad::WEST != 0 {
        pressed |= buttons::JUMP;
    }
    // Barely tilting the stick walks.
    if stick.length() > 0.05 && stick.length() < 0.55 {
        held |= buttons::FOCUS;
    }
    InputFrame { move_dir, held: held | pressed, pressed, ..Default::default() }
}
