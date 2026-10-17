//! Icons drawn from shapes for the keys games put in the model: weapons (`pistol`, `smg`...),
//! phone apps (`contacts`, `messages`...), notifications (`money`, `heat`...) and a few
//! general ones. An unknown key draws its initials in a ring, so every key shows something.

use egui::{Align2, Color32, Pos2, Rect, Stroke, pos2, vec2};

use crate::paint::{Cx, Fx, cr, dir};

/// Maps a unit box onto a rectangle of a given aspect (width / height), centred in `rect`.
struct Boxer {
    r: Rect,
}

impl Boxer {
    fn new(rect: Rect, aspect: f32) -> Self {
        let (w, h) = if rect.width() / rect.height().max(1e-3) > aspect {
            (rect.height() * aspect, rect.height())
        } else {
            (rect.width(), rect.width() / aspect)
        };
        Self { r: Rect::from_center_size(rect.center(), vec2(w, h)) }
    }
    fn p(&self, u: f32, v: f32) -> Pos2 {
        pos2(self.r.left() + u * self.r.width(), self.r.top() + v * self.r.height())
    }
    fn rect(&self, u0: f32, v0: f32, u1: f32, v1: f32) -> Rect {
        Rect::from_two_pos(self.p(u0, v0), self.p(u1, v1))
    }
    /// A length as a fraction of the box height.
    fn s(&self, f: f32) -> f32 {
        f * self.r.height()
    }
    fn quad(&self, pts: [(f32, f32); 4]) -> Vec<Pos2> {
        pts.iter().map(|(u, v)| self.p(*u, *v)).collect()
    }
}

/// Whether a key is a weapon (drawn wide, 2:1).
pub fn is_weapon(key: &str) -> bool {
    matches!(
        canonical(key),
        "pistol" | "smg" | "rifle" | "shotgun" | "sniper" | "fists" | "knife" | "bat" | "grenade" | "molotov" | "rpg"
    )
}

fn canonical(key: &str) -> &str {
    match key {
        "handgun" | "revolver" => "pistol",
        "uzi" | "machine_pistol" => "smg",
        "carbine" | "assault" | "assault_rifle" | "ar" | "carbine_rifle" => "rifle",
        "pump" | "pump_shotgun" => "shotgun",
        "sniper_rifle" => "sniper",
        "unarmed" | "fist" | "none" => "fists",
        "melee" | "blade" => "knife",
        "club" | "baseball_bat" => "bat",
        "explosive" | "grenades" => "grenade",
        "firebomb" => "molotov",
        "launcher" | "rocket" | "rocket_launcher" => "rpg",
        "person" | "people" | "contact" => "contacts",
        "texts" | "text" | "sms" | "chat" | "message" => "messages",
        "gps" | "waypoint" => "map",
        "gear" | "options" => "settings",
        "browser" | "web" | "globe" => "internet",
        "mail" | "inbox" => "email",
        "missions" | "mission_log" | "heist" | "briefcase" | "job" => "jobs",
        "radio" | "song" | "note" => "music",
        "cash" | "bank" | "stocks" | "$" | "dollar" => "money",
        "mission" | "stunt" | "favourite" | "favorite" => "star",
        "wasted" | "death" | "dead" => "skull",
        "police" | "badge" | "cop" | "wanted" => "heat",
        "vehicle" | "garage" => "car",
        "call" => "phone",
        "i" | "help" => "info",
        "alert" | "!" | "danger" => "warning",
        "medal" | "award" => "trophy",
        "locked" => "lock",
        "done" | "ok" | "complete" => "check",
        "time" | "timer" => "clock",
        k => k,
    }
}

impl Cx<'_> {
    /// Draws the icon for `key` inside `rect` in `col`.
    pub fn icon(&self, key: &str, rect: Rect, col: Color32) {
        let lower = key.to_ascii_lowercase();
        let k = canonical(lower.as_str());
        let dark = Color32::from_rgba_unmultiplied(0x0e, 0x11, 0x16, 230);
        match k {
            // ---- weapons (2:1, pointing right)
            "pistol" => {
                let b = Boxer::new(rect, 2.0);
                self.p.rect_filled(b.rect(0.22, 0.18, 0.86, 0.42), cr(b.s(0.06)), col);
                self.p.rect_filled(b.rect(0.82, 0.22, 0.9, 0.34), cr(0.0), col);
                self.polygon(b.quad([(0.3, 0.38), (0.48, 0.38), (0.42, 0.9), (0.24, 0.9)]), col);
                self.polyline(vec![b.p(0.48, 0.42), b.p(0.5, 0.6), b.p(0.58, 0.6), b.p(0.58, 0.42)], b.s(0.07), col);
            }
            "smg" => {
                let b = Boxer::new(rect, 2.0);
                self.p.rect_filled(b.rect(0.12, 0.2, 0.84, 0.46), cr(b.s(0.05)), col);
                self.p.rect_filled(b.rect(0.82, 0.27, 0.95, 0.37), cr(0.0), col);
                self.polygon(b.quad([(0.3, 0.44), (0.42, 0.44), (0.38, 0.86), (0.27, 0.86)]), col);
                self.polygon(b.quad([(0.52, 0.44), (0.62, 0.44), (0.66, 0.95), (0.56, 0.95)]), col);
                self.p.rect_filled(b.rect(0.02, 0.24, 0.14, 0.4), cr(0.0), col);
            }
            "rifle" => {
                let b = Boxer::new(rect, 2.0);
                self.p.rect_filled(b.rect(0.2, 0.3, 0.8, 0.5), cr(b.s(0.04)), col);
                self.p.rect_filled(b.rect(0.78, 0.35, 0.99, 0.43), cr(0.0), col);
                self.polygon(b.quad([(0.0, 0.32), (0.22, 0.32), (0.22, 0.52), (0.02, 0.66)]), col);
                self.polygon(b.quad([(0.36, 0.48), (0.46, 0.48), (0.42, 0.85), (0.32, 0.85)]), col);
                self.polygon(b.quad([(0.54, 0.48), (0.64, 0.48), (0.7, 0.92), (0.6, 0.92)]), col);
                self.p.rect_filled(b.rect(0.4, 0.2, 0.6, 0.3), cr(b.s(0.03)), col);
            }
            "shotgun" => {
                let b = Boxer::new(rect, 2.0);
                self.p.rect_filled(b.rect(0.24, 0.34, 0.98, 0.46), cr(b.s(0.04)), col);
                self.p.rect_filled(b.rect(0.5, 0.47, 0.86, 0.57), cr(b.s(0.04)), col);
                self.polygon(b.quad([(0.0, 0.36), (0.26, 0.34), (0.3, 0.5), (0.03, 0.68)]), col);
                self.polygon(b.quad([(0.3, 0.44), (0.4, 0.44), (0.37, 0.7), (0.28, 0.7)]), col);
            }
            "sniper" => {
                let b = Boxer::new(rect, 2.0);
                self.p.rect_filled(b.rect(0.2, 0.42, 0.99, 0.52), cr(b.s(0.03)), col);
                self.polygon(b.quad([(0.0, 0.44), (0.24, 0.42), (0.28, 0.56), (0.02, 0.72)]), col);
                self.p.rect_filled(b.rect(0.34, 0.2, 0.62, 0.32), cr(b.s(0.05)), col);
                self.p.rect_filled(b.rect(0.44, 0.31, 0.5, 0.42), cr(0.0), col);
                self.polygon(b.quad([(0.34, 0.5), (0.42, 0.5), (0.39, 0.78), (0.31, 0.78)]), col);
            }
            "fists" => {
                // A fist from the side: four knuckles over a palm, the thumb across.
                let b = Boxer::new(rect, 2.0);
                for i in 0..4 {
                    let x = 0.36 + i as f32 * 0.085;
                    self.p.rect_filled(b.rect(x, 0.12, x + 0.08, 0.5), cr(b.s(0.09)), col);
                }
                self.p.rect_filled(b.rect(0.34, 0.3, 0.7, 0.82), cr(b.s(0.14)), col);
                self.p.rect_filled(b.rect(0.3, 0.46, 0.56, 0.62), cr(b.s(0.08)), dark);
                self.p.rect_filled(b.rect(0.31, 0.47, 0.55, 0.61), cr(b.s(0.07)), col);
                self.p.rect_filled(b.rect(0.42, 0.8, 0.62, 0.95), cr(b.s(0.03)), col);
            }
            "knife" => {
                let b = Boxer::new(rect, 2.0);
                self.polygon(b.quad([(0.4, 0.38), (0.98, 0.42), (0.9, 0.58), (0.4, 0.58)]), col);
                self.p.rect_filled(b.rect(0.06, 0.4, 0.36, 0.58), cr(b.s(0.06)), col);
                self.p.rect_filled(b.rect(0.35, 0.28, 0.41, 0.68), cr(b.s(0.02)), col);
            }
            "bat" => {
                let b = Boxer::new(rect, 2.0);
                self.polygon(b.quad([(0.04, 0.46), (0.5, 0.4), (0.5, 0.6), (0.04, 0.54)]), col);
                self.p.rect_filled(b.rect(0.45, 0.36, 0.98, 0.64), cr(b.s(0.14)), col);
                self.circle(b.p(0.04, 0.5), b.s(0.07), col);
            }
            "grenade" => {
                let b = Boxer::new(rect, 1.0);
                self.circle(b.p(0.5, 0.6), b.s(0.3), col);
                self.p.rect_filled(b.rect(0.4, 0.2, 0.6, 0.34), cr(b.s(0.03)), col);
                self.ring(b.p(0.68, 0.22), b.s(0.08), b.s(0.04), col);
                self.line(b.p(0.4, 0.5), b.p(0.6, 0.5), b.s(0.04), dark);
                self.line(b.p(0.5, 0.36), b.p(0.5, 0.84), b.s(0.04), dark);
            }
            "molotov" => {
                let b = Boxer::new(rect, 1.0);
                self.p.rect_filled(b.rect(0.34, 0.42, 0.66, 0.95), cr(b.s(0.08)), col);
                self.p.rect_filled(b.rect(0.44, 0.24, 0.56, 0.44), cr(0.0), col);
                self.polygon(vec![b.p(0.5, 0.0), b.p(0.62, 0.16), b.p(0.5, 0.24), b.p(0.38, 0.16)], crate::theme::color::ACCENT);
            }
            "rpg" => {
                let b = Boxer::new(rect, 2.0);
                self.p.rect_filled(b.rect(0.02, 0.38, 0.8, 0.54), cr(b.s(0.05)), col);
                self.polygon(b.quad([(0.78, 0.3), (0.9, 0.36), (0.9, 0.56), (0.78, 0.62)]), col);
                self.polygon(vec![b.p(0.9, 0.36), b.p(1.0, 0.46), b.p(0.9, 0.56)], col);
                self.polygon(b.quad([(0.32, 0.52), (0.4, 0.52), (0.38, 0.8), (0.3, 0.8)]), col);
                self.p.rect_filled(b.rect(0.5, 0.28, 0.62, 0.38), cr(b.s(0.02)), col);
            }
            // ---- phone apps and general icons (square)
            "contacts" => {
                let b = Boxer::new(rect, 1.0);
                self.circle(b.p(0.5, 0.32), b.s(0.18), col);
                self.p.rect_filled(
                    b.rect(0.18, 0.56, 0.82, 0.92),
                    egui::CornerRadius { nw: b.s(0.3) as u8, ne: b.s(0.3) as u8, sw: b.s(0.06) as u8, se: b.s(0.06) as u8 },
                    col,
                );
            }
            "messages" => {
                let b = Boxer::new(rect, 1.0);
                self.p.rect_filled(b.rect(0.08, 0.14, 0.92, 0.7), cr(b.s(0.16)), col);
                self.polygon(vec![b.p(0.26, 0.66), b.p(0.46, 0.66), b.p(0.22, 0.9)], col);
                for i in 0..3 {
                    self.circle(b.p(0.3 + 0.2 * i as f32, 0.42), b.s(0.055), dark);
                }
            }
            "camera" => {
                let b = Boxer::new(rect, 1.0);
                self.p.rect_filled(b.rect(0.06, 0.28, 0.94, 0.84), cr(b.s(0.1)), col);
                self.p.rect_filled(b.rect(0.32, 0.16, 0.6, 0.3), cr(b.s(0.04)), col);
                self.circle(b.p(0.5, 0.56), b.s(0.2), dark);
                self.circle(b.p(0.5, 0.56), b.s(0.12), col);
            }
            "map" => {
                let b = Boxer::new(rect, 1.0);
                self.circle(b.p(0.5, 0.38), b.s(0.3), col);
                self.polygon(vec![b.p(0.24, 0.5), b.p(0.76, 0.5), b.p(0.5, 0.96)], col);
                self.circle(b.p(0.5, 0.38), b.s(0.12), dark);
            }
            "settings" => {
                let b = Boxer::new(rect, 1.0);
                let c = b.p(0.5, 0.5);
                for i in 0..8 {
                    let a = i as f32 * std::f32::consts::TAU / 8.0;
                    let d = dir(a);
                    let side = vec2(-d.y, d.x) * b.s(0.08);
                    self.polygon(
                        vec![
                            c + d * b.s(0.2) - side,
                            c + d * b.s(0.46) - side * 0.8,
                            c + d * b.s(0.46) + side * 0.8,
                            c + d * b.s(0.2) + side,
                        ],
                        col,
                    );
                }
                self.circle(c, b.s(0.33), col);
                self.circle(c, b.s(0.13), dark);
            }
            "internet" => {
                let b = Boxer::new(rect, 1.0);
                let c = b.p(0.5, 0.5);
                let r = b.s(0.4);
                let w = b.s(0.065);
                self.ring(c, r, w, col);
                self.line(c - vec2(r, 0.0), c + vec2(r, 0.0), w, col);
                self.line(c - vec2(0.0, r), c + vec2(0.0, r), w, col);
                let ell: Vec<Pos2> = (0..=24)
                    .map(|i| {
                        let a = i as f32 / 24.0 * std::f32::consts::TAU;
                        c + vec2(a.sin() * r * 0.45, -a.cos() * r)
                    })
                    .collect();
                self.polyline(ell, w, col);
            }
            "email" => {
                let b = Boxer::new(rect, 1.0);
                let r = b.rect(0.06, 0.2, 0.94, 0.8);
                self.p.rect_filled(r, cr(b.s(0.06)), col);
                self.polyline(
                    vec![
                        r.left_top() + vec2(b.s(0.04), b.s(0.04)),
                        r.center() + vec2(0.0, b.s(0.04)),
                        r.right_top() + vec2(-b.s(0.04), b.s(0.04)),
                    ],
                    b.s(0.07),
                    dark,
                );
            }
            "jobs" => {
                let b = Boxer::new(rect, 1.0);
                self.p.rect_filled(b.rect(0.06, 0.3, 0.94, 0.86), cr(b.s(0.08)), col);
                self.p.rect_stroke(
                    b.rect(0.34, 0.14, 0.66, 0.32),
                    cr(b.s(0.05)),
                    Stroke::new(b.s(0.07), col),
                    egui::StrokeKind::Middle,
                );
                self.p.rect_filled(b.rect(0.06, 0.52, 0.94, 0.58), cr(0.0), dark);
            }
            "music" => {
                let b = Boxer::new(rect, 1.0);
                self.circle(b.p(0.3, 0.76), b.s(0.14), col);
                self.circle(b.p(0.74, 0.66), b.s(0.14), col);
                self.p.rect_filled(b.rect(0.38, 0.16, 0.45, 0.76), cr(0.0), col);
                self.p.rect_filled(b.rect(0.81, 0.08, 0.88, 0.66), cr(0.0), col);
                self.polygon(b.quad([(0.38, 0.16), (0.88, 0.06), (0.88, 0.2), (0.38, 0.3)]), col);
            }
            "money" => {
                let b = Boxer::new(rect, 1.0);
                self.text(
                    b.p(0.5, 0.5),
                    Align2::CENTER_CENTER,
                    "$",
                    egui::FontId::new(b.s(0.92), self.fam.display.clone()),
                    col,
                    Fx::None,
                );
            }
            "star" => {
                let b = Boxer::new(rect, 1.0);
                self.star(b.p(0.5, 0.54), b.s(0.48), col);
            }
            "skull" => {
                let b = Boxer::new(rect, 1.0);
                self.circle(b.p(0.5, 0.42), b.s(0.34), col);
                self.p.rect_filled(b.rect(0.3, 0.5, 0.7, 0.88), cr(b.s(0.08)), col);
                self.circle(b.p(0.36, 0.46), b.s(0.1), dark);
                self.circle(b.p(0.64, 0.46), b.s(0.1), dark);
                self.polygon(vec![b.p(0.5, 0.58), b.p(0.45, 0.68), b.p(0.55, 0.68)], dark);
                for x in [0.42, 0.5, 0.58] {
                    self.line(b.p(x, 0.76), b.p(x, 0.88), b.s(0.035), dark);
                }
            }
            "heat" => {
                let b = Boxer::new(rect, 1.0);
                // A shield badge with a star.
                self.polygon(
                    vec![b.p(0.5, 0.04), b.p(0.9, 0.18), b.p(0.84, 0.6), b.p(0.5, 0.96), b.p(0.16, 0.6), b.p(0.1, 0.18)],
                    col,
                );
                self.star(b.p(0.5, 0.46), b.s(0.22), dark);
            }
            "car" => {
                let b = Boxer::new(rect, 1.6);
                self.polygon(b.quad([(0.24, 0.5), (0.34, 0.2), (0.7, 0.2), (0.82, 0.5)]), col);
                self.p.rect_filled(b.rect(0.04, 0.46, 0.96, 0.8), cr(b.s(0.12)), col);
                self.polygon(b.quad([(0.33, 0.48), (0.4, 0.28), (0.5, 0.28), (0.5, 0.48)]), dark);
                self.polygon(b.quad([(0.54, 0.48), (0.54, 0.28), (0.66, 0.28), (0.73, 0.48)]), dark);
                for x in [0.26, 0.74] {
                    self.circle(b.p(x, 0.8), b.s(0.14), dark);
                    self.circle(b.p(x, 0.8), b.s(0.08), col);
                }
            }
            "phone" => {
                let b = Boxer::new(rect, 1.0);
                self.p.rect_filled(b.rect(0.28, 0.04, 0.72, 0.96), cr(b.s(0.1)), col);
                self.p.rect_filled(b.rect(0.33, 0.14, 0.67, 0.8), cr(b.s(0.03)), dark);
                self.circle(b.p(0.5, 0.88), b.s(0.04), dark);
            }
            "info" | "warning" => {
                let b = Boxer::new(rect, 1.0);
                if k == "info" {
                    self.circle(b.p(0.5, 0.5), b.s(0.46), col);
                } else {
                    self.polygon(vec![b.p(0.5, 0.04), b.p(0.97, 0.9), b.p(0.03, 0.9)], col);
                }
                let (t, y) = if k == "info" { ("i", 0.52) } else { ("!", 0.62) };
                self.text(
                    b.p(0.5, y),
                    Align2::CENTER_CENTER,
                    t,
                    egui::FontId::new(b.s(0.62), self.fam.display.clone()),
                    dark,
                    Fx::None,
                );
            }
            "trophy" => {
                let b = Boxer::new(rect, 1.0);
                self.polygon(b.quad([(0.22, 0.08), (0.78, 0.08), (0.68, 0.5), (0.32, 0.5)]), col);
                self.circle(b.p(0.5, 0.42), b.s(0.18), col);
                self.ring(b.p(0.22, 0.24), b.s(0.12), b.s(0.06), col);
                self.ring(b.p(0.78, 0.24), b.s(0.12), b.s(0.06), col);
                self.p.rect_filled(b.rect(0.44, 0.56, 0.56, 0.8), cr(0.0), col);
                self.p.rect_filled(b.rect(0.28, 0.8, 0.72, 0.94), cr(b.s(0.03)), col);
            }
            "lock" => {
                let b = Boxer::new(rect, 1.0);
                self.p.rect_filled(b.rect(0.18, 0.44, 0.82, 0.94), cr(b.s(0.08)), col);
                self.arc(b.p(0.5, 0.44), b.s(0.2), -std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2, b.s(0.08), col);
                self.line(b.p(0.3, 0.44), b.p(0.3, 0.32), b.s(0.08), col);
                self.line(b.p(0.7, 0.44), b.p(0.7, 0.32), b.s(0.08), col);
                self.circle(b.p(0.5, 0.66), b.s(0.07), dark);
            }
            "check" => {
                let b = Boxer::new(rect, 1.0);
                self.polyline(vec![b.p(0.14, 0.52), b.p(0.4, 0.78), b.p(0.88, 0.22)], b.s(0.14), col);
            }
            "clock" => {
                let b = Boxer::new(rect, 1.0);
                let c = b.p(0.5, 0.5);
                self.ring(c, b.s(0.4), b.s(0.08), col);
                self.line(c, c + vec2(0.0, -b.s(0.26)), b.s(0.07), col);
                self.line(c, c + vec2(b.s(0.18), 0.0), b.s(0.07), col);
            }
            _ => self.initials(key, rect, col),
        }
    }

    /// An unknown key's initials in a ring (stations, characters): "freeway_fm" -> "FF".
    pub fn initials(&self, key: &str, rect: Rect, col: Color32) {
        let b = Boxer::new(rect, 1.0);
        let c = b.p(0.5, 0.5);
        self.ring(c, b.s(0.44), b.s(0.07), col);
        let letters: String =
            key.split(['_', ' ', '-', '/']).filter_map(|w| w.chars().next()).take(2).flat_map(char::to_uppercase).collect();
        let size = if letters.chars().count() > 1 { 0.42 } else { 0.5 };
        self.text(c, Align2::CENTER_CENTER, letters, egui::FontId::new(b.s(size), self.fam.bold.clone()), col, Fx::None);
    }
}
