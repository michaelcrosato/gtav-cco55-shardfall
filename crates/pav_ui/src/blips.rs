//! Map blips drawn from shapes, one look per `BlipIcon`, shared by the radar, the pause map and
//! screen markers: coloured shapes with a dark rim so they read on any part of the map.

use egui::{Align2, Color32, Pos2, Rect, Stroke, pos2, vec2};
use pav_core::present::BlipIcon;

use crate::paint::{Cx, Fx, cr};
use crate::theme::color;

const RIM: Color32 = Color32::from_rgba_unmultiplied_const(0x08, 0x0a, 0x0d, 235);

/// The legend's name for a blip kind.
pub fn blip_name(icon: BlipIcon) -> &'static str {
    match icon {
        BlipIcon::Dot => "Point of interest",
        BlipIcon::Letter(_) => "Mission",
        BlipIcon::Waypoint => "Waypoint",
        BlipIcon::Objective => "Objective",
        BlipIcon::Enemy => "Enemy",
        BlipIcon::Ally => "Ally",
        BlipIcon::Police => "Police",
        BlipIcon::Helicopter => "Helicopter",
        BlipIcon::Vehicle => "Vehicle",
        BlipIcon::Shop => "Shop",
        BlipIcon::Gun => "Gun store",
        BlipIcon::Garage => "Garage",
        BlipIcon::Safehouse => "Safehouse",
        BlipIcon::Hospital => "Hospital",
        BlipIcon::Activity => "Activity",
        BlipIcon::Race => "Race",
        BlipIcon::Jump => "Stunt jump",
        BlipIcon::Collectible => "Collectible",
        BlipIcon::Heist => "Heist",
    }
}

impl Cx<'_> {
    /// Draws a blip `s` across centred at `c`.
    pub fn blip(&self, icon: BlipIcon, c: Pos2, s: f32, col: Color32) {
        let r = s * 0.5;
        let rim = (s * 0.09).max(1.2);
        match icon {
            BlipIcon::Dot | BlipIcon::Enemy | BlipIcon::Ally => {
                let k = if icon == BlipIcon::Dot { 0.62 } else { 0.56 };
                self.p.circle(c, r * k, col, Stroke::new(rim, RIM));
            }
            BlipIcon::Objective => {
                self.p.circle(c, r * 0.78, col, Stroke::new(rim, RIM));
                self.p.circle_stroke(c, r * 0.4, Stroke::new(rim * 0.8, RIM.gamma_multiply(0.6)));
            }
            BlipIcon::Police => {
                // Red and blue, swapping twice a second like a light bar.
                let flip = (self.time * 4.0) as i64 % 2 == 0;
                let (a, b) = if flip { (color::POLICE_RED, color::POLICE_BLUE) } else { (color::POLICE_BLUE, color::POLICE_RED) };
                self.p.circle(c, r * 0.66, RIM, Stroke::NONE);
                self.sector(c, 0.0, r * 0.56, std::f32::consts::PI, std::f32::consts::TAU, |_| a);
                self.sector(c, 0.0, r * 0.56, 0.0, std::f32::consts::PI, |_| b);
            }
            BlipIcon::Letter(ch) => {
                self.p.circle(c, r * 0.86, col, Stroke::new(rim, RIM));
                let t: String = ch.to_uppercase().collect();
                self.text(
                    c,
                    Align2::CENTER_CENTER,
                    t,
                    egui::FontId::new(s * 0.6, self.fam.display.clone()),
                    color::SLATE,
                    Fx::None,
                );
            }
            BlipIcon::Waypoint => {
                let head = c + vec2(0.0, -r * 0.22);
                self.p.circle(head, r * 0.62, RIM, Stroke::NONE);
                self.polygon(vec![head + vec2(-r * 0.6, r * 0.18), head + vec2(r * 0.6, r * 0.18), c + vec2(0.0, r * 1.0)], RIM);
                self.p.circle(head, r * 0.48, col, Stroke::NONE);
                self.polygon(vec![head + vec2(-r * 0.44, r * 0.2), head + vec2(r * 0.44, r * 0.2), c + vec2(0.0, r * 0.84)], col);
                self.p.circle(head, r * 0.18, RIM, Stroke::NONE);
            }
            BlipIcon::Hospital => {
                let q = Rect::from_center_size(c, vec2(s * 0.84, s * 0.84));
                self.p.rect_filled(q.expand(rim * 0.6), cr(s * 0.2), RIM);
                self.p.rect_filled(q, cr(s * 0.18), col);
                let (a, b) = (s * 0.5, s * 0.16);
                self.p.rect_filled(Rect::from_center_size(c, vec2(a, b)), cr(0.0), color::TEXT);
                self.p.rect_filled(Rect::from_center_size(c, vec2(b, a)), cr(0.0), color::TEXT);
            }
            BlipIcon::Activity => {
                self.star(c, r * 1.04, RIM);
                self.star(c, r * 0.82, col);
            }
            BlipIcon::Collectible => {
                let d = |k: f32| {
                    vec![c + vec2(0.0, -r * k), c + vec2(r * k * 0.8, 0.0), c + vec2(0.0, r * k), c + vec2(-r * k * 0.8, 0.0)]
                };
                self.polygon(d(1.05), RIM);
                self.polygon(d(0.85), col);
                self.polygon(vec![c + vec2(0.0, -r * 0.85), c + vec2(r * 0.68, 0.0), c], Color32::from_white_alpha(70));
            }
            BlipIcon::Heist => {
                self.p.circle(c, r * 0.86, col, Stroke::new(rim, RIM));
                self.text(
                    c,
                    Align2::CENTER_CENTER,
                    "$",
                    egui::FontId::new(s * 0.64, self.fam.display.clone()),
                    color::SLATE,
                    Fx::None,
                );
            }
            _ => {
                // Silhouettes on a dark disc.
                self.p.circle(c, r, RIM, Stroke::NONE);
                let b = Rect::from_center_size(c, vec2(s * 0.66, s * 0.66));
                let p = |u: f32, v: f32| pos2(b.left() + u * b.width(), b.top() + v * b.height());
                let w = (s * 0.08).max(1.2);
                match icon {
                    BlipIcon::Helicopter => {
                        self.p.circle(p(0.42, 0.56), b.width() * 0.2, col, Stroke::NONE);
                        self.line(p(0.5, 0.56), p(0.98, 0.5), w, col);
                        self.line(p(0.0, 0.22), p(0.84, 0.22), w, col);
                        self.line(p(0.42, 0.22), p(0.42, 0.38), w, col);
                        self.line(p(0.2, 0.92), p(0.66, 0.92), w * 0.8, col);
                    }
                    BlipIcon::Vehicle => {
                        self.p.rect_filled(Rect::from_min_max(p(0.26, 0.0), p(0.74, 1.0)), cr(b.width() * 0.16), col);
                        self.p.rect_filled(Rect::from_min_max(p(0.32, 0.22), p(0.68, 0.38)), cr(1.0), RIM);
                        self.p.rect_filled(Rect::from_min_max(p(0.32, 0.72), p(0.68, 0.84)), cr(1.0), RIM);
                    }
                    BlipIcon::Shop => {
                        self.p.rect_filled(Rect::from_min_max(p(0.1, 0.34), p(0.9, 1.0)), cr(b.width() * 0.08), col);
                        self.arc(
                            p(0.5, 0.36),
                            b.width() * 0.22,
                            -std::f32::consts::FRAC_PI_2,
                            std::f32::consts::FRAC_PI_2,
                            w,
                            col,
                        );
                    }
                    BlipIcon::Gun => {
                        self.p.rect_filled(Rect::from_min_max(p(0.04, 0.2), p(0.96, 0.46)), cr(1.0), col);
                        self.polygon(vec![p(0.12, 0.42), p(0.42, 0.42), p(0.34, 0.92), p(0.06, 0.92)], col);
                    }
                    BlipIcon::Garage => {
                        self.polygon(vec![p(0.5, 0.0), p(1.0, 0.34), p(1.0, 1.0), p(0.0, 1.0), p(0.0, 0.34)], col);
                        for v in [0.52, 0.68, 0.84] {
                            self.line(p(0.2, v), p(0.8, v), w * 0.7, RIM);
                        }
                    }
                    BlipIcon::Safehouse => {
                        self.polygon(vec![p(0.5, 0.02), p(1.0, 0.46), p(0.0, 0.46)], col);
                        self.p.rect_filled(Rect::from_min_max(p(0.14, 0.44), p(0.86, 1.0)), cr(0.0), col);
                        self.p.rect_filled(Rect::from_min_max(p(0.4, 0.62), p(0.6, 1.0)), cr(0.0), RIM);
                    }
                    BlipIcon::Race => {
                        self.line(p(0.12, 0.0), p(0.12, 1.0), w, col);
                        let cell = b.width() * 0.25;
                        for row in 0..2 {
                            for colm in 0..3 {
                                let q = Rect::from_min_size(
                                    p(0.16, 0.04) + vec2(colm as f32 * cell, row as f32 * cell),
                                    vec2(cell, cell),
                                );
                                let fill = if (row + colm) % 2 == 0 { col } else { col.gamma_multiply(0.35) };
                                self.p.rect_filled(q, cr(0.0), fill);
                            }
                        }
                    }
                    BlipIcon::Jump => {
                        self.polygon(vec![p(0.0, 1.0), p(0.62, 0.62), p(0.62, 1.0)], col);
                        self.arc(
                            p(0.62, 0.62),
                            b.width() * 0.36,
                            -std::f32::consts::FRAC_PI_2 * 0.9,
                            std::f32::consts::FRAC_PI_2 * 0.6,
                            w,
                            col,
                        );
                    }
                    _ => {
                        self.p.circle(c, r * 0.5, col, Stroke::NONE);
                    }
                }
            }
        }
    }

    /// The small up or down arrow next to a blip above or below the player.
    pub fn blip_height(&self, c: Pos2, s: f32, height: i8) {
        if height == 0 {
            return;
        }
        let up = height > 0;
        let tip = c + vec2(s * 0.62, if up { -s * 0.52 } else { s * 0.52 });
        let base_y = tip.y + if up { s * 0.28 } else { -s * 0.28 };
        let pts = vec![tip, pos2(tip.x - s * 0.2, base_y), pos2(tip.x + s * 0.2, base_y)];
        self.polygon_outlined(pts, color::TEXT, (s * 0.06).max(1.0), RIM);
    }

    /// A pulsing ring around a blip (keyed to UI time).
    pub fn blip_pulse(&self, c: Pos2, s: f32, col: Color32) {
        let t = (self.time * 1.2).fract();
        let r = s * (0.5 + t * 0.7);
        self.ring(c, r, (s * 0.08).max(1.0), col.gamma_multiply(1.0 - t));
    }
}
