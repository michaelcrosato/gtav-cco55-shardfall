//! The in-play HUD and the weapon wheel (PLAN.md §5.9 layout): built from the game's state each
//! tick as a `UiModel` the engine's UI kit draws.

use glam::Vec2;
use pav_core::Color;
use pav_core::present::{Blip, BlipIcon, Help, Hud, Minimap, Money, Prompt, Toast, Vitals, WeaponHud, Wheel, WheelSlot};
use serde::{Deserialize, Serialize};

/// The weapon wheel's eight slots (§5.6.4), clockwise from the top.
pub const WEAPONS: [(&str, &str, &str); 8] = [
    ("Fists", "fists", "Unarmed"),
    ("Pistol", "pistol", "Semi-automatic · 12 rounds"),
    ("SMG", "smg", "Compact · fast fire"),
    ("Shotgun", "shotgun", "Pump action · close range"),
    ("Carbine", "rifle", "Assault rifle · 30 rounds"),
    ("Marksman", "sniper", "Scoped rifle · long range"),
    ("Grenades", "grenade", "Thrown · 4 carried"),
    ("Phone", "phone", "Put the weapon away"),
];

/// A notification sliding in at the right.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Note {
    pub id: u64,
    pub icon: String,
    pub title: String,
    pub body: String,
    pub age: f32,
}

impl Note {
    pub fn toast(&self) -> Toast {
        Toast { id: self.id, icon: self.icon.clone(), title: self.title.clone(), body: self.body.clone(), age: self.age }
    }
}

/// The open weapon wheel: since when, and the slot the stick points at.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct WheelState {
    pub open_for: f32,
    pub focus: Option<usize>,
}

impl WheelState {
    /// Points the stick: past half way picks the slot in that direction (slot 0 at the top,
    /// clockwise); closer to the middle keeps the last pick.
    pub fn aim(&mut self, stick: Vec2) {
        if stick.length() < 0.5 {
            return;
        }
        // Angle clockwise from up.
        let a = stick.x.atan2(stick.y).rem_euclid(std::f32::consts::TAU);
        self.focus = Some(((a / std::f32::consts::TAU * 8.0 + 0.5).floor() as usize) % 8);
    }

    pub fn ui(&self, current: usize) -> Wheel {
        let focus = self.focus.unwrap_or(current);
        let (name, _, detail) = WEAPONS[focus];
        Wheel {
            id: "weapons".into(),
            slots: WEAPONS
                .iter()
                .map(|(label, icon, detail)| WheelSlot {
                    label: (*label).into(),
                    icon: (*icon).into(),
                    detail: Some((*detail).into()),
                    enabled: true,
                })
                .collect(),
            focus: Some(focus),
            center: format!("{name}\n{detail}"),
        }
    }
}

/// What the HUD shows this tick.
pub struct HudInput<'a> {
    pub player: Vec2,
    pub heading: f32,
    pub camera_yaw: f32,
    pub money: (i64, i64, f32),
    pub weapon: usize,
    pub help: Option<(&'a str, f32)>,
    pub map_blips: Vec<Blip>,
}

pub fn hud(h: &HudInput) -> (Hud, Option<Help>, Vec<Prompt>) {
    let armed = h.weapon != 0 && h.weapon != 7;
    let hud = Hud {
        minimap: Some(Minimap {
            center: h.player,
            heading: h.camera_yaw,
            radius: 60.0,
            player: h.player,
            player_heading: h.heading,
            blips: h.map_blips.clone(),
            route: Vec::new(),
            cones: Vec::new(),
            circles: Vec::new(),
        }),
        vitals: Some(Vitals { health: 1.0, armor: 0.5, ability: 0.8, ability_color: Color::hex("#ffc145") }),
        money: Some(Money { amount: h.money.0, delta: h.money.1, delta_age: h.money.2 }),
        weapon: armed.then(|| {
            let (name, icon, _) = WEAPONS[h.weapon];
            WeaponHud { name: name.into(), icon: icon.into(), clip: Some(12), reserve: Some(48) }
        }),
        objective: Some("Explore the lot. Hold ~btn:l1~ for the weapon wheel.".into()),
        ..Default::default()
    };
    let help = h.help.map(|(text, age)| Help { text: text.into(), age });
    (hud, help, Vec::new())
}

/// Example blips around the lot (the city's real ones arrive with the district).
pub fn lot_blips() -> Vec<Blip> {
    let blip = |x: f32, z: f32, icon: BlipIcon, hex: &str, label: &str| Blip {
        pos: Vec2::new(x, z),
        icon,
        color: Color::hex(hex),
        label: Some(label.into()),
        clamp: false,
        height: 0,
        pulse: false,
    };
    vec![
        blip(-12.0, -6.0, BlipIcon::Safehouse, "#6fe28a", "Dez's place"),
        blip(14.0, 5.0, BlipIcon::Gun, "#ff7a3d", "Bayside Arms"),
        blip(60.0, -40.0, BlipIcon::Letter('M'), "#3d8bff", "Mack"),
    ]
}
