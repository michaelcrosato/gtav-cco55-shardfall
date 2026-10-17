//! Device-independent input for one simulation tick. The host (window, agent, replay) fills
//! it; the simulation never sees devices. Directions are already in world space.
//!
//! Two layers: the engine demo's own fields (`move_dir`, `aim`, the [`buttons`] bits), and
//! [`PadState`], where every device (keyboard and mouse included) is described as a controller —
//! games with their own camera and controls read that (`docs/MAKE_A_GAME.md`).

use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

pub mod buttons {
    pub const JUMP: u32 = 1 << 0;
    pub const CROUCH: u32 = 1 << 1;
    pub const INTERACT: u32 = 1 << 2;
    pub const USE: u32 = 1 << 3;
    pub const FOCUS: u32 = 1 << 4;
    pub const SPRINT: u32 = 1 << 5;
    pub const PRIMARY: u32 = 1 << 6;
    pub const SECONDARY: u32 = 1 << 7;
    pub const CRAWL: u32 = 1 << 8;
    /// Shardfall: skill slots 3-6 (slots 1-2 are PRIMARY / SECONDARY), dodge and potion.
    pub const SKILL3: u32 = 1 << 9;
    pub const SKILL4: u32 = 1 << 10;
    pub const SKILL5: u32 = 1 << 11;
    pub const SKILL6: u32 = 1 << 12;
    pub const DODGE: u32 = 1 << 13;
    pub const POTION: u32 = 1 << 14;

    pub const NAMES: &[(&str, u32)] = &[
        ("jump", JUMP),
        ("crouch", CROUCH),
        ("interact", INTERACT),
        ("use", USE),
        ("focus", FOCUS),
        ("sprint", SPRINT),
        ("primary", PRIMARY),
        ("secondary", SECONDARY),
        ("crawl", CRAWL),
        ("skill3", SKILL3),
        ("skill4", SKILL4),
        ("skill5", SKILL5),
        ("skill6", SKILL6),
        ("dodge", DODGE),
        ("potion", POTION),
    ];

    pub fn from_name(n: &str) -> Option<u32> {
        NAMES.iter().find(|(k, _)| k.eq_ignore_ascii_case(n)).map(|(_, v)| *v)
    }
}

/// Controller buttons ([`PadState`] bits), by position: `SOUTH` is Cross on a PlayStation pad and
/// A on an Xbox pad. The keyboard and mouse map onto these too.
pub mod pad {
    pub const SOUTH: u32 = 1 << 0;
    pub const EAST: u32 = 1 << 1;
    pub const WEST: u32 = 1 << 2;
    pub const NORTH: u32 = 1 << 3;
    pub const L1: u32 = 1 << 4;
    pub const R1: u32 = 1 << 5;
    /// The triggers as buttons (pulled past a third); [`super::PadState::l2`] has the analog value.
    pub const L2: u32 = 1 << 6;
    pub const R2: u32 = 1 << 7;
    pub const L3: u32 = 1 << 8;
    pub const R3: u32 = 1 << 9;
    pub const UP: u32 = 1 << 10;
    pub const DOWN: u32 = 1 << 11;
    pub const LEFT: u32 = 1 << 12;
    pub const RIGHT: u32 = 1 << 13;
    /// Options (PlayStation), Menu (Xbox).
    pub const START: u32 = 1 << 14;
    /// Create (PlayStation), View (Xbox).
    pub const SELECT: u32 = 1 << 15;
    pub const TOUCHPAD: u32 = 1 << 16;

    /// Names for tools and scripts: position names first, then PlayStation and Xbox aliases.
    pub const NAMES: &[(&str, u32)] = &[
        ("south", SOUTH),
        ("east", EAST),
        ("west", WEST),
        ("north", NORTH),
        ("l1", L1),
        ("r1", R1),
        ("l2", L2),
        ("r2", R2),
        ("l3", L3),
        ("r3", R3),
        ("up", UP),
        ("down", DOWN),
        ("left", LEFT),
        ("right", RIGHT),
        ("start", START),
        ("select", SELECT),
        ("touchpad", TOUCHPAD),
        ("cross", SOUTH),
        ("circle", EAST),
        ("square", WEST),
        ("triangle", NORTH),
        ("options", START),
        ("create", SELECT),
        ("a", SOUTH),
        ("b", EAST),
        ("x", WEST),
        ("y", NORTH),
        ("lb", L1),
        ("rb", R1),
        ("lt", L2),
        ("rt", R2),
        ("menu", START),
        ("view", SELECT),
    ];

    pub fn from_name(n: &str) -> Option<u32> {
        NAMES.iter().find(|(k, _)| k.eq_ignore_ascii_case(n)).map(|(_, v)| *v)
    }

    /// The position name of a single bit (`south`, `l1`...).
    pub fn name(bit: u32) -> Option<&'static str> {
        NAMES[..17].iter().find(|(_, v)| *v == bit).map(|(k, _)| *k)
    }
}

/// One controller's state this tick. Sticks are device-relative (x right, y forward/up, length
/// <= 1); the game turns them into world directions with its own camera.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PadState {
    /// [`pad`] buttons held this tick.
    #[serde(default)]
    pub held: u32,
    /// [`pad`] buttons that went down since the previous tick (never lost, even for short taps).
    #[serde(default)]
    pub pressed: u32,
    #[serde(default)]
    pub stick_l: Vec2,
    #[serde(default)]
    pub stick_r: Vec2,
    /// Analog triggers, 0..1.
    #[serde(default)]
    pub l2: f32,
    #[serde(default)]
    pub r2: f32,
}

impl PadState {
    pub fn is_idle(&self) -> bool {
        *self == Self::default()
    }
    pub fn down(&self, b: u32) -> bool {
        self.held & b != 0
    }
    pub fn just(&self, b: u32) -> bool {
        self.pressed & b != 0
    }
}

/// The kind of device the player last used: picks button glyphs and hints.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Device {
    #[default]
    Keyboard,
    PlayStation,
    Xbox,
    /// A controller of another or unknown make (Xbox-style positions).
    Gamepad,
    Touch,
}

impl Device {
    pub fn is_pad(self) -> bool {
        matches!(self, Device::PlayStation | Device::Xbox | Device::Gamepad)
    }

    /// The family of a controller from its name as the OS reports it.
    pub fn from_pad_name(name: &str) -> Device {
        let n = name.to_ascii_lowercase();
        if ["dualsense", "dualshock", "wireless controller", "ps5", "ps4", "playstation", "sony"].iter().any(|k| n.contains(k)) {
            Device::PlayStation
        } else if ["xbox", "xinput", "microsoft"].iter().any(|k| n.contains(k)) {
            Device::Xbox
        } else {
            Device::Gamepad
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Device::Keyboard => "keyboard",
            Device::PlayStation => "playstation",
            Device::Xbox => "xbox",
            Device::Gamepad => "gamepad",
            Device::Touch => "touch",
        }
    }

    pub fn from_name(n: &str) -> Option<Device> {
        [Device::Keyboard, Device::PlayStation, Device::Xbox, Device::Gamepad, Device::Touch]
            .into_iter()
            .find(|d| d.name().eq_ignore_ascii_case(n))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct InputFrame {
    /// Desired movement on the ground plane: x = world +X, y = world +Z. Length <= 1.
    pub move_dir: Vec2,
    /// Vertical intent for climbing/flying/swimming (-1..1).
    #[serde(default)]
    pub vertical: f32,
    /// World-space aim point (mouse on the ground plane, or stick projection).
    #[serde(default)]
    pub aim: Option<Vec3>,
    /// Buttons held this tick.
    #[serde(default)]
    pub held: u32,
    /// Buttons that went down since the previous tick (never lost, even for short taps).
    #[serde(default)]
    pub pressed: u32,
    /// A menu action this tick (Shardfall: equip, sell, travel...), queued one per tick.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cmd: Option<crate::arpg::GameCmd>,
    /// Every device as a controller (games with their own controls read this).
    #[serde(default, skip_serializing_if = "PadState::is_idle")]
    pub pad: PadState,
    /// Mouse movement since the previous tick in pixels (x right, y down): camera look.
    #[serde(default, skip_serializing_if = "is_zero2")]
    pub mouse_delta: Vec2,
    /// Mouse wheel notches since the previous tick (positive = away from the player).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub wheel: f32,
    /// Any device did something this tick (a key, a mouse move, a stick, a touch): idle timers.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub activity: bool,
    /// The device the player used last.
    #[serde(default, skip_serializing_if = "is_keyboard")]
    pub device: Device,
    /// Game commands this tick, in order: menu picks from the pointer or the tools, platform
    /// facts (`platform touch`), loaded saves and settings. Recorded and replayed like keys.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cmds: Vec<String>,
}

fn is_zero(v: &f32) -> bool {
    *v == 0.0
}

fn is_zero2(v: &Vec2) -> bool {
    *v == Vec2::ZERO
}

fn is_keyboard(d: &Device) -> bool {
    *d == Device::Keyboard
}

impl InputFrame {
    pub fn down(&self, b: u32) -> bool {
        self.held & b != 0
    }
    pub fn just(&self, b: u32) -> bool {
        self.pressed & b != 0
    }
}
