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

/// The bat in the fists' slot (once owned).
pub const BAT_SLOT: (&str, &str, &str) = ("Bat", "fists", "Melee · knocks people down");

/// Wheel slot `i` as shown (`bat`: the bat owned, in the fists' slot).
pub fn slot(i: usize, bat: bool) -> (&'static str, &'static str, &'static str) {
    if i == 0 && bat { BAT_SLOT } else { WEAPONS[i.min(WEAPONS.len() - 1)] }
}

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
        self.aim_n(stick, 8);
    }

    /// The same for a wheel of `n` slots.
    pub fn aim_n(&mut self, stick: Vec2, n: usize) {
        if stick.length() < 0.5 || n == 0 {
            return;
        }
        // Angle clockwise from up.
        let a = stick.x.atan2(stick.y).rem_euclid(std::f32::consts::TAU);
        self.focus = Some(((a / std::f32::consts::TAU * n as f32 + 0.5).floor() as usize) % n);
    }

    /// The wheel as drawn (`bat`: the bat in the fists' slot).
    pub fn ui(&self, current: usize, bat: bool) -> Wheel {
        let focus = self.focus.unwrap_or(current);
        let (name, _, detail) = slot(focus, bat);
        Wheel {
            id: "weapons".into(),
            slots: (0..WEAPONS.len())
                .map(|i| slot(i, bat))
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
    /// The wheel slot in hand, and whether the bat fills the fists' slot.
    pub weapon: usize,
    pub bat: bool,
    /// The ability meter (0..1) and the protagonist's colour.
    pub ability: (f32, Color),
    pub help: Option<(&'a str, f32)>,
    pub map_blips: Vec<Blip>,
    /// The GPS route on the radar (map points from the player on).
    pub route: Vec<Vec2>,
    /// Health and armour (0..1), the weapon's magazine and reserve, the reticle while aiming.
    pub vitals: (f32, f32),
    pub ammo: Option<(u32, u32)>,
    pub crosshair: Option<pav_core::present::Crosshair>,
}

pub fn hud(h: &HudInput) -> (Hud, Option<Help>, Vec<Prompt>) {
    let armed = (h.weapon != 0 || h.bat) && h.weapon != 7;
    let hud = Hud {
        minimap: Some(Minimap {
            center: h.player,
            heading: h.camera_yaw,
            radius: 60.0,
            player: h.player,
            player_heading: h.heading,
            blips: h.map_blips.clone(),
            route: h.route.clone(),
            cones: Vec::new(),
            circles: Vec::new(),
        }),
        vitals: Some(Vitals { health: h.vitals.0, armor: h.vitals.1, ability: h.ability.0, ability_color: h.ability.1 }),
        money: Some(Money { amount: h.money.0, delta: h.money.1, delta_age: h.money.2 }),
        weapon: armed.then(|| {
            let (name, icon, _) = slot(h.weapon, h.bat);
            WeaponHud { name: name.into(), icon: icon.into(), clip: h.ammo.map(|a| a.0), reserve: h.ammo.map(|a| a.1) }
        }),
        crosshair: h.crosshair.clone(),
        objective: Some("Explore Paloma Bay. Hold ~btn:l1~ for the weapon wheel.".into()),
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

/// "Colourblind-safe colours" (PLAN §5.9): the radar's, the map's and the markers' reds turn
/// orange and their greens sky blue, so nothing hangs on telling red from green.
pub fn colorblind(ui: &mut pav_core::present::UiModel) {
    use pav_core::present::ScreenBody;
    fn safe(c: Color) -> Color {
        let [r, g, b] = c.0;
        let (hi, lo) = (r.max(g).max(b), r.min(g).min(b));
        if hi - lo < 0.04 {
            return c;
        }
        let d = hi - lo;
        let hue = if hi == r {
            60.0 * ((g - b) / d).rem_euclid(6.0)
        } else if hi == g {
            60.0 * ((b - r) / d + 2.0)
        } else {
            60.0 * ((r - g) / d + 4.0)
        };
        if !(25.0..335.0).contains(&hue) {
            Color([hi, hi * 0.32, lo * 0.2])
        } else if (75.0..165.0).contains(&hue) {
            Color([lo * 0.5, hi * 0.5, hi])
        } else {
            c
        }
    }
    if let Some(m) = ui.hud.as_mut().and_then(|h| h.minimap.as_mut()) {
        m.blips.iter_mut().for_each(|b| b.color = safe(b.color));
        m.cones.iter_mut().for_each(|c| c.color = safe(c.color));
        m.circles.iter_mut().for_each(|c| c.color = safe(c.color));
    }
    ui.markers.iter_mut().for_each(|m| m.color = safe(m.color));
    if let Some(ScreenBody::Map(m)) = ui.screen.as_mut().map(|s| &mut s.body) {
        m.blips.iter_mut().for_each(|b| b.color = safe(b.color));
    }
}
