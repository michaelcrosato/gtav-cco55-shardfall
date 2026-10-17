//! Controller glyphs drawn with shapes, no images: PlayStation (✕ ○ □ △ in their classic
//! colours on dark discs, bumper and trigger pills, Options, Create, touchpad, sticks), Xbox
//! (coloured A B X Y letters, LB/RB/LT/RT, Menu, View, sticks) and keyboard keycaps or a mouse,
//! labelled from the game's active key map.

use egui::{Align2, Color32, CornerRadius, Pos2, Rect, Stroke, StrokeKind, pos2, vec2};
use pav_core::input::{Device, pad};
use pav_core::present::{GlyphStyle, KeyMap, PadButton, PadTarget};

use crate::paint::{Cx, Fx, cr};

/// Which drawings the glyphs use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlyphSet {
    PlayStation,
    Xbox,
    Keyboard,
}

/// The glyph set for a style: `Auto` follows the device used last (PlayStation for unknown
/// pads and touch screens, the game's primary controller).
pub fn glyph_set(style: GlyphStyle, device: Device) -> GlyphSet {
    match style {
        GlyphStyle::PlayStation => GlyphSet::PlayStation,
        GlyphStyle::Xbox => GlyphSet::Xbox,
        GlyphStyle::Keyboard => GlyphSet::Keyboard,
        GlyphStyle::Auto => match device {
            Device::Keyboard => GlyphSet::Keyboard,
            Device::Xbox => GlyphSet::Xbox,
            Device::PlayStation | Device::Gamepad | Device::Touch => GlyphSet::PlayStation,
        },
    }
}

/// The `input::pad` bit of a button (sticks have none).
pub fn pad_bit(b: PadButton) -> Option<u32> {
    Some(match b {
        PadButton::South => pad::SOUTH,
        PadButton::East => pad::EAST,
        PadButton::West => pad::WEST,
        PadButton::North => pad::NORTH,
        PadButton::L1 => pad::L1,
        PadButton::R1 => pad::R1,
        PadButton::L2 => pad::L2,
        PadButton::R2 => pad::R2,
        PadButton::L3 => pad::L3,
        PadButton::R3 => pad::R3,
        PadButton::Up => pad::UP,
        PadButton::Down => pad::DOWN,
        PadButton::Left => pad::LEFT,
        PadButton::Right => pad::RIGHT,
        PadButton::Start => pad::START,
        PadButton::Select => pad::SELECT,
        PadButton::Touchpad => pad::TOUCHPAD,
        PadButton::LeftStick | PadButton::RightStick => return None,
    })
}

/// The keys a map binds to `button`, in map order.
pub fn key_codes(keys: &KeyMap, button: PadButton) -> Vec<&str> {
    let bit = pad_bit(button);
    keys.binds
        .iter()
        .filter(|k| match (button, k.to) {
            (_, PadTarget::Button(b)) => Some(b) == bit,
            (PadButton::L2, PadTarget::L2) | (PadButton::R2, PadTarget::R2) => true,
            (PadButton::LeftStick, PadTarget::StickL(_)) | (PadButton::RightStick, PadTarget::StickR(_)) => true,
            _ => false,
        })
        .map(|k| k.key.as_str())
        .collect()
}

/// What a keycap shows for `button` under `keys` ("W", "SPACE", "LMB", "WASD"), or None when
/// the map leaves it unbound.
pub fn key_label(keys: &KeyMap, button: PadButton) -> Option<String> {
    let codes = key_codes(keys, button);
    if matches!(button, PadButton::LeftStick | PadButton::RightStick) {
        let has = |c: &str| codes.contains(&c);
        if ["KeyW", "KeyA", "KeyS", "KeyD"].iter().all(|c| has(c)) {
            return Some("WASD".into());
        }
        if ["ArrowUp", "ArrowLeft", "ArrowDown", "ArrowRight"].iter().all(|c| has(c)) {
            return Some("ARROWS".into());
        }
        if codes.is_empty() && button == PadButton::RightStick {
            return Some("MOUSE".into());
        }
    }
    codes.first().map(|c| pretty_key(c))
}

/// A W3C key code as a keycap label: `KeyW` -> `W`, `Space` -> `SPACE`, `MouseLeft` -> `LMB`.
pub fn pretty_key(code: &str) -> String {
    if let Some(k) = code.strip_prefix("Key") {
        return k.to_ascii_uppercase();
    }
    if let Some(d) = code.strip_prefix("Digit") {
        return d.to_string();
    }
    if let Some(n) = code.strip_prefix("Numpad") {
        return format!("NUM {}", n.to_ascii_uppercase());
    }
    let s = match code {
        "Space" => "SPACE",
        "Escape" => "ESC",
        "Enter" | "NumpadEnter" => "ENTER",
        "Tab" => "TAB",
        "Backspace" => "BKSP",
        "ShiftLeft" | "ShiftRight" => "SHIFT",
        "ControlLeft" | "ControlRight" => "CTRL",
        "AltLeft" | "AltRight" => "ALT",
        "MetaLeft" | "MetaRight" => "META",
        "CapsLock" => "CAPS",
        "ArrowUp" => "UP",
        "ArrowDown" => "DOWN",
        "ArrowLeft" => "LEFT",
        "ArrowRight" => "RIGHT",
        "MouseLeft" => "LMB",
        "MouseRight" => "RMB",
        "MouseMiddle" => "MMB",
        "Comma" => ",",
        "Period" => ".",
        "Slash" => "/",
        "Backslash" => "\\",
        "Semicolon" => ";",
        "Quote" => "'",
        "BracketLeft" => "[",
        "BracketRight" => "]",
        "Minus" => "-",
        "Equal" => "=",
        "Backquote" => "`",
        "Delete" => "DEL",
        "Insert" => "INS",
        "PageUp" => "PG UP",
        "PageDown" => "PG DN",
        "Home" => "HOME",
        "End" => "END",
        other => return other.to_ascii_uppercase(),
    };
    s.to_string()
}

// Classic face-button colours.
const PS_CROSS: Color32 = Color32::from_rgb(0x8c, 0xb3, 0xff);
const PS_CIRCLE: Color32 = Color32::from_rgb(0xff, 0x6b, 0x6b);
const PS_SQUARE: Color32 = Color32::from_rgb(0xf0, 0x94, 0xdc);
const PS_TRIANGLE: Color32 = Color32::from_rgb(0x45, 0xd6, 0xaa);
const XB_A: Color32 = Color32::from_rgb(0x7c, 0xd3, 0x3c);
const XB_B: Color32 = Color32::from_rgb(0xf0, 0x55, 0x48);
const XB_X: Color32 = Color32::from_rgb(0x3d, 0x98, 0xf5);
const XB_Y: Color32 = Color32::from_rgb(0xf7, 0xc8, 0x2a);
const DISC: Color32 = Color32::from_rgb(0x14, 0x17, 0x1d);
const RIM: Color32 = Color32::from_rgba_unmultiplied_const(0xe8, 0xe4, 0xdc, 120);
const LABEL: Color32 = Color32::from_rgb(0xf4, 0xf1, 0xea);
const KEY_TOP: Color32 = Color32::from_rgb(0xec, 0xe8, 0xe0);
const KEY_LIP: Color32 = Color32::from_rgb(0x9e, 0x98, 0x8d);
const KEY_TEXT: Color32 = Color32::from_rgb(0x16, 0x19, 0x1f);

/// How a button is drawn in a set.
enum Look {
    Face(Face),
    Letter(&'static str, Color32),
    Pill(&'static str),
    Trigger(&'static str),
    Stick(&'static str, bool),
    Dpad(PadButton),
    Options,
    Create,
    Menu,
    View,
    Touchpad,
    Key(String, Option<PadButton>),
    Mouse(u8),
}

#[derive(Clone, Copy)]
enum Face {
    Cross,
    Circle,
    Square,
    Triangle,
}

impl Cx<'_> {
    fn look(&self, b: PadButton) -> Look {
        use PadButton::*;
        match self.glyphs {
            GlyphSet::PlayStation => match b {
                South => Look::Face(Face::Cross),
                East => Look::Face(Face::Circle),
                West => Look::Face(Face::Square),
                North => Look::Face(Face::Triangle),
                L1 => Look::Pill("L1"),
                R1 => Look::Pill("R1"),
                L2 => Look::Trigger("L2"),
                R2 => Look::Trigger("R2"),
                L3 => Look::Stick("L3", false),
                R3 => Look::Stick("R3", false),
                LeftStick => Look::Stick("L", true),
                RightStick => Look::Stick("R", true),
                Up | Down | Left | Right => Look::Dpad(b),
                Start => Look::Options,
                Select => Look::Create,
                Touchpad => Look::Touchpad,
            },
            GlyphSet::Xbox => match b {
                South => Look::Letter("A", XB_A),
                East => Look::Letter("B", XB_B),
                West => Look::Letter("X", XB_X),
                North => Look::Letter("Y", XB_Y),
                L1 => Look::Pill("LB"),
                R1 => Look::Pill("RB"),
                L2 => Look::Trigger("LT"),
                R2 => Look::Trigger("RT"),
                L3 => Look::Stick("LS", false),
                R3 => Look::Stick("RS", false),
                LeftStick => Look::Stick("L", true),
                RightStick => Look::Stick("R", true),
                Up | Down | Left | Right => Look::Dpad(b),
                Start => Look::Menu,
                Select | Touchpad => Look::View,
            },
            GlyphSet::Keyboard => {
                let codes = key_codes(self.keys, b);
                let mouse = codes.first().and_then(|c| match *c {
                    "MouseLeft" => Some(0),
                    "MouseRight" => Some(1),
                    "MouseMiddle" => Some(2),
                    _ => None,
                });
                if let Some(m) = mouse {
                    return Look::Mouse(m);
                }
                let arrow = codes.first().and_then(|c| match *c {
                    "ArrowUp" => Some(Up),
                    "ArrowDown" => Some(Down),
                    "ArrowLeft" => Some(Left),
                    "ArrowRight" => Some(Right),
                    _ => None,
                });
                let label = key_label(self.keys, b).unwrap_or_else(|| crate::markup::button_name(b).to_ascii_uppercase());
                Look::Key(label, arrow.filter(|_| !matches!(b, LeftStick | RightStick)))
            }
        }
    }

    /// The width a glyph of height `h` takes.
    pub fn glyph_width(&self, b: PadButton, h: f32) -> f32 {
        match self.look(b) {
            Look::Face(_) | Look::Letter(..) | Look::Stick(..) | Look::Dpad(_) | Look::Menu | Look::View => h,
            Look::Pill(_) => h * 1.45,
            Look::Trigger(_) => h * 1.3,
            Look::Options | Look::Create => h * 0.95,
            Look::Touchpad => h * 1.6,
            Look::Mouse(_) => h * 0.74,
            Look::Key(label, arrow) => {
                if arrow.is_some() {
                    h
                } else {
                    (self.text_width(&label, &egui::FontId::new(h * 0.5, self.fam.bold.clone())) + h * 0.6).max(h)
                }
            }
        }
    }

    /// Draws `b` centred at `center`, `h` tall. Returns its rectangle.
    pub fn paint_glyph(&self, b: PadButton, center: Pos2, h: f32) -> Rect {
        let w = self.glyph_width(b, h);
        let rect = Rect::from_center_size(center, vec2(w, h));
        let r = h * 0.5;
        let line = (h * 0.095).max(1.2);
        let rim = Stroke::new((h * 0.05).max(1.0), RIM);
        let label_font = |s: f32| egui::FontId::new(h * s, self.fam.bold.clone());
        match self.look(b) {
            Look::Face(face) => {
                self.p.circle(center, r, DISC, rim);
                let s = r * 0.42;
                match face {
                    Face::Cross => {
                        let st = Stroke::new(line, PS_CROSS);
                        self.p.line_segment([center + vec2(-s, -s), center + vec2(s, s)], st);
                        self.p.line_segment([center + vec2(-s, s), center + vec2(s, -s)], st);
                    }
                    Face::Circle => {
                        self.p.circle_stroke(center, s * 1.02, Stroke::new(line, PS_CIRCLE));
                    }
                    Face::Square => {
                        let q = Rect::from_center_size(center, vec2(s * 1.7, s * 1.7));
                        self.p.rect_stroke(q, CornerRadius::ZERO, Stroke::new(line, PS_SQUARE), StrokeKind::Middle);
                    }
                    Face::Triangle => {
                        let c = center + vec2(0.0, s * 0.12);
                        let k = s * 1.15;
                        let pts = vec![
                            c + crate::paint::dir(0.0) * k,
                            c + crate::paint::dir(std::f32::consts::TAU / 3.0) * k,
                            c + crate::paint::dir(2.0 * std::f32::consts::TAU / 3.0) * k,
                        ];
                        self.p.add(egui::epaint::PathShape::closed_line(pts, Stroke::new(line, PS_TRIANGLE)));
                    }
                }
            }
            Look::Letter(t, col) => {
                self.p.circle(center, r, DISC, Stroke::new((h * 0.05).max(1.0), col.gamma_multiply(0.7)));
                self.glyph_text(center, t, label_font(0.62), col);
            }
            Look::Pill(t) => {
                self.p.rect(rect.shrink2(vec2(0.0, h * 0.09)), cr(h * 0.32), DISC, rim, StrokeKind::Inside);
                self.glyph_text(center, t, label_font(0.52), LABEL);
            }
            Look::Trigger(t) => {
                let body = rect.shrink2(vec2(h * 0.04, h * 0.04));
                let round =
                    CornerRadius { nw: (h * 0.42) as u8, ne: (h * 0.42) as u8, sw: (h * 0.14) as u8, se: (h * 0.14) as u8 };
                self.p.rect(body, round, DISC, rim, StrokeKind::Inside);
                self.glyph_text(center + vec2(0.0, h * 0.04), t, label_font(0.52), LABEL);
            }
            Look::Stick(t, moving) => {
                self.p.circle(center, r, DISC, rim);
                self.p.circle_stroke(center, r * 0.72, Stroke::new((h * 0.05).max(1.0), RIM));
                if moving {
                    // Four ticks: the stick moves this way.
                    for i in 0..4 {
                        let d = crate::paint::dir(i as f32 * std::f32::consts::FRAC_PI_2);
                        let tip = center + d * r * 0.93;
                        let side = egui::Vec2::new(-d.y, d.x) * r * 0.13;
                        let base = center + d * r * 0.76;
                        self.p.add(egui::epaint::PathShape::convex_polygon(
                            vec![tip, base + side, base - side],
                            LABEL,
                            Stroke::NONE,
                        ));
                    }
                }
                self.glyph_text(center, t, label_font(if t.len() > 1 { 0.42 } else { 0.5 }), LABEL);
            }
            Look::Dpad(dirb) => {
                // A dark cross with the pressed arm lit end to end.
                let arm = h * 0.34;
                let len = h * 0.5;
                let v = Rect::from_center_size(center, vec2(arm, len * 2.0));
                let hz = Rect::from_center_size(center, vec2(len * 2.0, arm));
                self.p.rect_filled(v.expand(h * 0.05), cr(h * 0.09), RIM);
                self.p.rect_filled(hz.expand(h * 0.05), cr(h * 0.09), RIM);
                self.p.rect_filled(v, cr(h * 0.07), DISC);
                self.p.rect_filled(hz, cr(h * 0.07), DISC);
                // From just past the centre to the arm's end.
                let k = arm * 0.12;
                let lit = match dirb {
                    PadButton::Up => Rect::from_min_max(pos2(v.left(), v.top()), pos2(v.right(), center.y - k)),
                    PadButton::Down => Rect::from_min_max(pos2(v.left(), center.y + k), pos2(v.right(), v.bottom())),
                    PadButton::Left => Rect::from_min_max(pos2(hz.left(), hz.top()), pos2(center.x - k, hz.bottom())),
                    _ => Rect::from_min_max(pos2(center.x + k, hz.top()), pos2(hz.right(), hz.bottom())),
                };
                self.p.rect_filled(lit.shrink(h * 0.045), cr(h * 0.05), LABEL);
            }
            Look::Options => {
                let body = Rect::from_center_size(center, vec2(w * 0.92, h * 0.62));
                self.p.rect(body, cr(h * 0.31), DISC, rim, StrokeKind::Inside);
                for k in [-1.0, 0.0, 1.0] {
                    let y = center.y + k * h * 0.11;
                    self.p.line_segment(
                        [pos2(center.x - h * 0.17, y), pos2(center.x + h * 0.17, y)],
                        Stroke::new((h * 0.055).max(1.0), LABEL),
                    );
                }
            }
            Look::Create => {
                let body = Rect::from_center_size(center, vec2(w * 0.92, h * 0.62));
                self.p.rect(body, cr(h * 0.31), DISC, rim, StrokeKind::Inside);
                // Three rays: the PS5 Create mark.
                for k in [-1.0f32, 0.0, 1.0] {
                    let base = pos2(center.x + k * h * 0.11, center.y + h * 0.13);
                    let tip = pos2(center.x + k * h * 0.17, center.y - h * 0.13);
                    self.p.line_segment([base, tip], Stroke::new((h * 0.055).max(1.0), LABEL));
                }
            }
            Look::Menu => {
                self.p.circle(center, r, DISC, rim);
                for k in [-1.0, 0.0, 1.0] {
                    let y = center.y + k * h * 0.12;
                    self.p.line_segment(
                        [pos2(center.x - h * 0.2, y), pos2(center.x + h * 0.2, y)],
                        Stroke::new((h * 0.06).max(1.0), LABEL),
                    );
                }
            }
            Look::View => {
                self.p.circle(center, r, DISC, rim);
                let s = h * 0.24;
                let a = Rect::from_min_size(center + vec2(-s * 0.8, -s * 0.8), vec2(s, s));
                let b2 = Rect::from_min_size(center + vec2(-s * 0.2, -s * 0.2), vec2(s, s));
                let st = Stroke::new((h * 0.055).max(1.0), LABEL);
                self.p.rect_stroke(a, cr(h * 0.03), st, StrokeKind::Middle);
                self.p.rect_filled(b2, cr(h * 0.03), DISC);
                self.p.rect_stroke(b2, cr(h * 0.03), st, StrokeKind::Middle);
            }
            Look::Touchpad => {
                let body = rect.shrink2(vec2(h * 0.02, h * 0.14));
                self.p.rect(body, cr(h * 0.16), DISC, rim, StrokeKind::Inside);
                self.p.line_segment(
                    [
                        pos2(body.left() + h * 0.2, body.bottom() - h * 0.16),
                        pos2(body.right() - h * 0.2, body.bottom() - h * 0.16),
                    ],
                    Stroke::new((h * 0.04).max(1.0), RIM),
                );
            }
            Look::Key(label, arrow) => {
                let lip = h * 0.08;
                let body = rect.shrink2(vec2(h * 0.02, h * 0.04));
                self.p.rect_filled(body, cr(h * 0.18), KEY_LIP);
                let top = Rect::from_min_max(body.min, pos2(body.right(), body.bottom() - lip));
                self.p.rect_filled(top, cr(h * 0.17), KEY_TOP);
                let c = top.center();
                if let Some(a) = arrow {
                    let d = match a {
                        PadButton::Up => vec2(0.0, -1.0),
                        PadButton::Down => vec2(0.0, 1.0),
                        PadButton::Left => vec2(-1.0, 0.0),
                        _ => vec2(1.0, 0.0),
                    };
                    let side = vec2(-d.y, d.x);
                    let s = h * 0.2;
                    self.p.add(egui::epaint::PathShape::convex_polygon(
                        vec![c + d * s, c - d * s * 0.7 + side * s, c - d * s * 0.7 - side * s],
                        KEY_TEXT,
                        Stroke::NONE,
                    ));
                } else {
                    self.glyph_text(c, &label, label_font(0.5), KEY_TEXT);
                }
            }
            Look::Mouse(button) => {
                let body = Rect::from_center_size(center, vec2(w * 0.94, h * 0.96));
                self.p.rect_filled(body, cr(w * 0.47), KEY_TOP);
                let split_y = body.top() + body.height() * 0.42;
                let lit = crate::theme::color::ACCENT;
                match button {
                    0 | 1 => {
                        let half = if button == 0 {
                            Rect::from_min_max(body.min, pos2(body.center().x, split_y))
                        } else {
                            Rect::from_min_max(pos2(body.center().x, body.top()), pos2(body.right(), split_y))
                        };
                        let round = if button == 0 {
                            CornerRadius { nw: (w * 0.47) as u8, ..CornerRadius::ZERO }
                        } else {
                            CornerRadius { ne: (w * 0.47) as u8, ..CornerRadius::ZERO }
                        };
                        self.p.rect_filled(half.shrink(h * 0.035), round, lit);
                    }
                    _ => {
                        self.p.rect_filled(
                            Rect::from_center_size(
                                pos2(body.center().x, body.top() + body.height() * 0.24),
                                vec2(w * 0.2, h * 0.24),
                            ),
                            cr(w * 0.1),
                            lit,
                        );
                    }
                }
                let st = Stroke::new((h * 0.045).max(1.0), KEY_LIP);
                self.p.line_segment([pos2(body.left(), split_y), pos2(body.right(), split_y)], st);
                self.p.line_segment([pos2(body.center().x, body.top()), pos2(body.center().x, split_y)], st);
            }
        }
        rect
    }

    /// Centred label text inside a glyph (capitals centred optically).
    fn glyph_text(&self, center: Pos2, t: &str, font: egui::FontId, col: Color32) {
        self.text(center, Align2::CENTER_CENTER, t, font, col, Fx::None);
    }

    /// A hold prompt's progress ring around a glyph.
    pub fn hold_ring(&self, center: Pos2, h: f32, progress: f32) {
        let r = h * 0.64;
        let w = (h * 0.1).max(1.5);
        self.ring(center, r, w, Color32::from_rgba_unmultiplied(0, 0, 0, 150));
        let p = progress.clamp(0.0, 1.0);
        if p > 0.0 {
            self.arc(center, r, 0.0, p * std::f32::consts::TAU, w, crate::theme::color::ACCENT);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_follows_the_device() {
        assert_eq!(glyph_set(GlyphStyle::Auto, Device::Keyboard), GlyphSet::Keyboard);
        assert_eq!(glyph_set(GlyphStyle::Auto, Device::Xbox), GlyphSet::Xbox);
        assert_eq!(glyph_set(GlyphStyle::Auto, Device::Gamepad), GlyphSet::PlayStation);
        assert_eq!(glyph_set(GlyphStyle::Auto, Device::Touch), GlyphSet::PlayStation);
        assert_eq!(glyph_set(GlyphStyle::Xbox, Device::Keyboard), GlyphSet::Xbox);
    }

    #[test]
    fn keycaps_read_the_key_map() {
        let keys = KeyMap::standard();
        assert_eq!(key_label(&keys, PadButton::South).as_deref(), Some("ENTER"));
        assert_eq!(key_label(&keys, PadButton::East).as_deref(), Some("ESC"));
        assert_eq!(key_label(&keys, PadButton::North).as_deref(), Some("F"));
        assert_eq!(key_label(&keys, PadButton::R2).as_deref(), Some("LMB"));
        assert_eq!(key_label(&keys, PadButton::L2).as_deref(), Some("RMB"));
        assert_eq!(key_label(&keys, PadButton::Start).as_deref(), Some("TAB"));
        assert_eq!(key_label(&keys, PadButton::LeftStick).as_deref(), Some("WASD"));
        assert_eq!(key_label(&keys, PadButton::RightStick).as_deref(), Some("MOUSE"));
        assert_eq!(key_label(&keys, PadButton::Touchpad), None);
    }

    #[test]
    fn key_names_are_prettified() {
        assert_eq!(pretty_key("KeyW"), "W");
        assert_eq!(pretty_key("Space"), "SPACE");
        assert_eq!(pretty_key("MouseLeft"), "LMB");
        assert_eq!(pretty_key("Escape"), "ESC");
        assert_eq!(pretty_key("Digit1"), "1");
        assert_eq!(pretty_key("ShiftLeft"), "SHIFT");
        assert_eq!(pretty_key("F5"), "F5");
    }
}
