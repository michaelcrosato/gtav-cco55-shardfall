//! The controller as the game reads it (`InputFrame.pad`), menu navigation with hold-to-repeat,
//! and the keyboard maps per context (PLAN.md §5.10): the host turns keys into this controller
//! through whichever map the game names, so the rules only ever see a pad.

use std::sync::{Arc, LazyLock};

use glam::Vec2;
use pav_core::InputFrame;
use pav_core::input::pad;
use pav_core::present::{KeyMap, PadTarget};
use serde::{Deserialize, Serialize};

/// A button went down this tick.
pub fn pressed(input: &InputFrame, bit: u32) -> bool {
    input.pad.pressed & bit != 0
}

/// A button is held this tick.
pub fn held(input: &InputFrame, bit: u32) -> bool {
    input.pad.held & bit != 0
}

/// Any button went down this tick ("PRESS ANY BUTTON").
pub fn any_pressed(input: &InputFrame) -> bool {
    input.pad.pressed != 0
}

/// One step through a menu per press and, while held, again after 0.35 s and then every
/// 0.11 s: from the D-pad or the left stick pushed past half way.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Nav {
    dir: (i8, i8),
    held_for: f32,
    next: f32,
}

impl Nav {
    /// This tick's step: x = -1 left / 1 right, y = -1 up / 1 down. `dt` is real time.
    pub fn step(&mut self, input: &InputFrame, dt: f32) -> (i32, i32) {
        let p = &input.pad;
        let d = |neg: u32, pos: u32| (p.held & pos != 0) as i8 - (p.held & neg != 0) as i8;
        let mut dir = (d(pad::LEFT, pad::RIGHT), d(pad::UP, pad::DOWN));
        let s = p.stick_l;
        if dir == (0, 0) && s.length() > 0.5 {
            dir = if s.x.abs() > s.y.abs() { (s.x.signum() as i8, 0) } else { (0, -s.y.signum() as i8) };
        }
        let fresh = |bit: u32| p.pressed & bit != 0;
        if dir == (0, 0) {
            *self = Nav::default();
            // A tap shorter than a tick still moves once.
            let tap = (fresh(pad::RIGHT) as i32 - fresh(pad::LEFT) as i32, fresh(pad::DOWN) as i32 - fresh(pad::UP) as i32);
            return tap;
        }
        if dir != self.dir {
            *self = Nav { dir, held_for: 0.0, next: 0.35 };
            return (dir.0 as i32, dir.1 as i32);
        }
        self.held_for += dt;
        if self.held_for >= self.next {
            self.next += 0.11;
            return (dir.0 as i32, dir.1 as i32);
        }
        (0, 0)
    }
}

/// Moves a focus index through `n` items by `dy`, wrapping around, skipping disabled ones.
pub fn move_focus(focus: usize, n: usize, dy: i32, enabled: impl Fn(usize) -> bool) -> usize {
    if n == 0 || dy == 0 {
        return focus.min(n.saturating_sub(1));
    }
    let mut f = focus as i32;
    for _ in 0..n {
        f = (f + dy.signum()).rem_euclid(n as i32);
        if enabled(f as usize) {
            return f as usize;
        }
    }
    focus
}

/// The left stick, or the engine tools' world-space `move_dir` when the pad is idle (`input
/// move=`, the feel meter). The bool says the result is already in world space.
pub fn stick_or_move(input: &InputFrame) -> (Vec2, bool) {
    if input.pad.stick_l != Vec2::ZERO || input.move_dir == Vec2::ZERO {
        (input.pad.stick_l, false)
    } else {
        (input.move_dir, true)
    }
}

/// Menus: arrows and WASD move the focus, Enter and Space accept, Esc backs out, Q and E switch
/// tabs, Z and C page or zoom, Tab is Options.
pub static MENU_KEYS: LazyLock<Arc<KeyMap>> = LazyLock::new(|| {
    use PadTarget::*;
    Arc::new(KeyMap::new(
        "menu",
        &[
            ("ArrowUp", Button(pad::UP)),
            ("ArrowDown", Button(pad::DOWN)),
            ("ArrowLeft", Button(pad::LEFT)),
            ("ArrowRight", Button(pad::RIGHT)),
            ("KeyW", Button(pad::UP)),
            ("KeyS", Button(pad::DOWN)),
            ("KeyA", Button(pad::LEFT)),
            ("KeyD", Button(pad::RIGHT)),
            ("Enter", Button(pad::SOUTH)),
            ("Space", Button(pad::SOUTH)),
            ("Escape", Button(pad::EAST)),
            ("KeyQ", Button(pad::L1)),
            ("KeyE", Button(pad::R1)),
            ("KeyZ", Button(pad::L2)),
            ("KeyC", Button(pad::R2)),
            ("KeyF", Button(pad::NORTH)),
            ("KeyR", Button(pad::WEST)),
            ("Tab", Button(pad::START)),
        ],
    ))
});

/// On foot (§5.10): WASD walk, the mouse looks, Shift sprints, Space jumps, F enters vehicles,
/// R reloads, Q takes cover, the right mouse button aims and the left fires, Tab opens the
/// weapon wheel, C crouches, V looks behind, Caps Lock is the special ability, the up arrow the
/// phone, Alt the character wheel, E interacts, Esc pauses, M opens the map.
pub static FOOT_KEYS: LazyLock<Arc<KeyMap>> = LazyLock::new(|| {
    use PadTarget::*;
    Arc::new(KeyMap::new(
        "foot",
        &[
            ("KeyW", StickL(Vec2::Y)),
            ("KeyS", StickL(Vec2::NEG_Y)),
            ("KeyA", StickL(Vec2::NEG_X)),
            ("KeyD", StickL(Vec2::X)),
            ("ShiftLeft", Button(pad::SOUTH)),
            ("Space", Button(pad::WEST)),
            ("KeyF", Button(pad::NORTH)),
            ("KeyR", Button(pad::EAST)),
            ("KeyQ", Button(pad::R1)),
            ("MouseRight", L2),
            ("MouseLeft", R2),
            ("Tab", Button(pad::L1)),
            ("KeyC", Button(pad::L3)),
            ("KeyV", Button(pad::R3)),
            ("CapsLock", Button(pad::L3 | pad::R3)),
            ("ArrowUp", Button(pad::UP)),
            ("AltLeft", Button(pad::DOWN)),
            ("KeyE", Button(pad::RIGHT)),
            ("Escape", Button(pad::START)),
            ("KeyM", Button(pad::TOUCHPAD)),
            ("Enter", Button(pad::SOUTH)),
        ],
    ))
});

/// In a vehicle (§5.10): W throttle, S brake and reverse, A and D steer, I and K pitch (a
/// helicopter flies forward and back), Space the handbrake, F gets out (held: bails out), H the
/// horn (tapped in a police car: the siren's wail, yelp, off), V looks behind, the middle mouse
/// button holds the cinematic camera, comma and period step the radio (comma held: the radio
/// wheel), E is D-pad right (vigilante in a police car), the up arrow the phone, Alt the
/// character wheel, Esc pauses, M opens the map.
pub static DRIVE_KEYS: LazyLock<Arc<KeyMap>> = LazyLock::new(|| {
    use PadTarget::*;
    Arc::new(KeyMap::new(
        "drive",
        &[
            ("KeyW", R2),
            ("KeyS", L2),
            ("KeyA", StickL(Vec2::NEG_X)),
            ("KeyD", StickL(Vec2::X)),
            ("KeyI", StickL(Vec2::Y)),
            ("KeyK", StickL(Vec2::NEG_Y)),
            ("Space", Button(pad::R1)),
            ("MouseRight", Button(pad::L1)),
            ("MouseLeft", Button(pad::R1)),
            ("KeyF", Button(pad::NORTH)),
            ("KeyH", Button(pad::L3)),
            ("KeyV", Button(pad::R3)),
            ("MouseMiddle", Button(pad::EAST)),
            ("Comma", Button(pad::LEFT)),
            ("Period", Button(pad::RIGHT)),
            ("KeyE", Button(pad::RIGHT)),
            ("CapsLock", Button(pad::L3 | pad::R3)),
            ("ArrowUp", Button(pad::UP)),
            ("AltLeft", Button(pad::DOWN)),
            ("Escape", Button(pad::START)),
            ("KeyM", Button(pad::TOUCHPAD)),
        ],
    ))
});

#[cfg(test)]
mod tests {
    use super::*;
    use pav_core::input::PadState;

    fn frame(held: u32, pressed: u32, stick: Vec2) -> InputFrame {
        InputFrame { pad: PadState { held, pressed, stick_l: stick, ..Default::default() }, ..Default::default() }
    }

    #[test]
    fn holding_a_direction_repeats() {
        let mut nav = Nav::default();
        let dt = 1.0 / 60.0;
        let mut steps = 0;
        for i in 0..60 {
            let pressed = if i == 0 { pad::DOWN } else { 0 };
            steps += nav.step(&frame(pad::DOWN, pressed, Vec2::ZERO), dt).1;
        }
        // One at once, then after 0.35 s every 0.11 s: 1 + 6 in a second.
        assert_eq!(steps, 7);
        assert_eq!(nav.step(&frame(0, 0, Vec2::ZERO), dt), (0, 0));
        assert_eq!(nav.step(&frame(0, 0, Vec2::new(0.0, 0.9)), dt), (0, -1), "stick up = focus up");
        assert_eq!(nav.step(&frame(0, pad::UP, Vec2::ZERO), dt), (0, -1), "a tap shorter than a tick counts");
    }

    #[test]
    fn focus_wraps_and_skips_disabled_items() {
        let enabled = |i: usize| i != 1;
        assert_eq!(move_focus(0, 4, 1, enabled), 2);
        assert_eq!(move_focus(3, 4, 1, enabled), 0);
        assert_eq!(move_focus(0, 4, -1, enabled), 3);
    }
}
