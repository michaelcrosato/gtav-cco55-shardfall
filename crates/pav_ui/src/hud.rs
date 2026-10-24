//! The in-play HUD (PLAN.md §5.9 layout): radar and vitals bottom left, money, stars and weapon
//! top right, objective bottom centre, timer, meters and the vehicle card bottom right, the
//! crosshair in the middle.

use egui::{Align, Align2, Color32, Rect, Stroke, pos2, vec2};
use pav_core::present::{Crosshair, Hud, Money, TimerHud, VehicleHud, Wanted, WeaponHud};

use crate::Layout;
use crate::map::MapTex;
use crate::paint::{Cx, Fx, ease_out, grouped, money, ramp};
use crate::theme::{color, linear, size};

impl Cx<'_> {
    /// The radar, vitals and crosshair (things with fixed places).
    pub fn hud_fixed(&self, hud: &Hud, tex: Option<MapTex>) {
        let (mw, mh) = (self.px(380.0), self.px(236.0));
        let vitals_h = self.px(26.0);
        let gap = self.px(8.0);
        let mut bottom = self.safe.bottom();
        if let Some(v) = &hud.vitals {
            let r = Rect::from_min_size(pos2(self.safe.left(), bottom - vitals_h), vec2(mw, vitals_h));
            self.vitals(v, r);
            bottom -= vitals_h + gap;
        }
        if let Some(mm) = &hud.minimap {
            let r = Rect::from_min_size(pos2(self.safe.left(), bottom - mh), vec2(mw, mh));
            self.minimap(mm, r, tex);
        }
        if let Some(c) = &hud.crosshair {
            self.crosshair(c);
        }
    }

    /// Money, stars and weapon down the right column.
    pub fn hud_right(&self, hud: &Hud, layout: &mut Layout) {
        let x = layout.right_x;
        let mut y = layout.right_y;
        if let Some(m) = &hud.money {
            y = self.money_hud(m, x, y) + self.px(14.0);
        }
        if let Some(w) = &hud.wanted {
            y = self.wanted(w, x, y) + self.px(16.0);
        }
        if let Some(w) = &hud.weapon {
            y = self.weapon(w, x, y) + self.px(18.0);
        }
        layout.right_y = y;
    }

    /// "$12,480" with its last change under it. Returns the bottom.
    fn money_hud(&self, m: &Money, right: f32, top: f32) -> f32 {
        let r = self.text(
            pos2(right, top),
            Align2::RIGHT_TOP,
            money(m.amount),
            self.display(size::HUD_NUMBER),
            color::TEXT,
            Fx::Outline,
        );
        let mut bottom = r.bottom();
        if m.delta != 0 && m.delta_age < 4.0 {
            // Slides down into place, holds, then fades.
            let t_in = ease_out(ramp(m.delta_age, 0.0, 0.25));
            let alpha = t_in * (1.0 - ramp(m.delta_age, 3.2, 0.8));
            let (sign, col) = if m.delta > 0 { ("+", color::POSITIVE) } else { ("-", color::DANGER) };
            let text = format!("{sign}${}", grouped(m.delta.abs()));
            let y = bottom + self.px(12.0) - (1.0 - t_in) * self.px(10.0);
            let c = self.faded(alpha);
            let rr = c.text(pos2(right, y), Align2::RIGHT_TOP, text, self.display(size::HUD_SMALL_NUMBER), col, Fx::Outline);
            bottom = rr.bottom();
        }
        bottom
    }

    /// The wanted stars, right-aligned. Returns the bottom.
    fn wanted(&self, w: &Wanted, right: f32, top: f32) -> f32 {
        let max = w.max.clamp(1, 10) as usize;
        let s = self.px(42.0);
        let gap = self.px(6.0);
        let total = max as f32 * s + (max - 1) as f32 * gap;
        // Searching: the earned stars blink between full and dim, twice a second.
        let blink = if w.flashing { 0.35 + 0.65 * (((self.time * 2.0).fract() < 0.5) as i32 as f32) } else { 1.0 };
        for i in 0..max {
            let c = pos2(right - total + s * 0.5 + i as f32 * (s + gap), top + s * 0.5);
            let lit = i < w.stars as usize;
            self.star(c + vec2(self.px(2.0), self.px(2.0)), s * 0.56, Color32::from_black_alpha(150));
            self.star(c, s * 0.56, Color32::from_black_alpha(190));
            if lit {
                self.star(c, s * 0.47, color::TEXT.gamma_multiply(blink));
            } else {
                self.star(c, s * 0.47, Color32::from_rgba_unmultiplied(0x2a, 0x30, 0x3a, 200));
            }
        }
        top + s
    }

    /// Weapon icon and ammo on a plate (the name instead of numbers for melee). Returns the
    /// bottom.
    fn weapon(&self, w: &WeaponHud, right: f32, top: f32) -> f32 {
        let h = self.px(72.0);
        let icon_w = self.px(120.0);
        let pad = self.px(18.0);
        let ammo_font = self.display(size::HUD_SMALL_NUMBER);
        let res_font = self.display(32.0);
        let ammo = w.clip.map(|c| c.to_string());
        let reserve = w.reserve.map(|r| format!("/ {r}"));
        let name = w.name.to_uppercase();
        let name_font = self.bold(size::LABEL);
        let gap = self.px(8.0);
        let block = if ammo.is_some() || reserve.is_some() {
            ammo.as_ref().map_or(0.0, |a| self.text_width(a, &ammo_font))
                + reserve.as_ref().map_or(0.0, |r| self.text_width(r, &res_font) + gap)
        } else {
            self.text_width(&name, &name_font)
        };
        let width = pad + icon_w + self.px(20.0) + block + pad;
        let rect = Rect::from_min_size(pos2(right - width, top), vec2(width, h));
        self.backplate(rect, 12.0);
        let icon_rect = Rect::from_min_size(pos2(rect.left() + pad, rect.top() + self.px(12.0)), vec2(icon_w, h - self.px(24.0)));
        self.icon(&w.icon, icon_rect, color::TEXT);
        let x1 = rect.right() - pad;
        let cy = rect.center().y;
        if ammo.is_some() || reserve.is_some() {
            // Shared baseline: the big clip count, the reserve smaller and dimmer.
            let (at, ab) = self.cap(&ammo_font);
            let base = cy + (ab - at) * 0.5;
            let mut x = x1;
            if let Some(r) = &reserve {
                let b = self.text(pos2(x, base), Align2::RIGHT_BOTTOM, r, res_font.clone(), color::TEXT_DIM, Fx::None);
                x = b.left() - gap;
            }
            if let Some(a) = &ammo {
                let empty = w.clip == Some(0);
                self.text(
                    pos2(x, base),
                    Align2::RIGHT_BOTTOM,
                    a,
                    ammo_font,
                    if empty { color::DANGER } else { color::TEXT },
                    Fx::None,
                );
            }
        } else {
            self.text(pos2(x1, cy), Align2::RIGHT_CENTER, name, name_font, color::TEXT, Fx::None);
        }
        rect.bottom()
    }

    /// The objective line, bottom centre, its last row ending at `bottom`.
    pub fn objective(&self, text: &str, bottom: f32) {
        let font = self.bold(34.0);
        let wrap = self.px(1100.0);
        let rich = self.rich(text, font, color::TEXT, Some(wrap));
        let top = bottom - rich.size.y;
        self.paint_rich(&rich, pos2(self.screen.center().x - wrap * 0.5, top), wrap, Align::Center, Fx::Shadow);
    }

    /// Bottom-right stack, from the bottom up: vehicle card, then timer and meters.
    pub fn vehicle_card(&self, v: &VehicleHud, layout: &mut Layout) {
        let t = ease_out(ramp(v.age, 0.0, 0.3));
        let c = self.faded(t);
        let right = layout.br_right;
        let mut y = layout.br_bottom;
        c.moved(vec2((1.0 - t) * self.px(40.0), 0.0), |c| {
            if let Some(song) = &v.song {
                // A song, or a DJ's line: wrapped, so a long one stays in the corner.
                let wrap = self.px(620.0);
                let rich = c.rich(song, self.ui(size::BODY), color::TEXT_DIM, Some(wrap));
                let top = y - rich.size.y;
                c.paint_rich(&rich, pos2(right - wrap, top), wrap, Align::Max, Fx::Shadow);
                y = top - self.px(12.0);
            }
            if let Some(st) = &v.station {
                let r = c.text(
                    pos2(right, y),
                    Align2::RIGHT_BOTTOM,
                    st.to_uppercase(),
                    self.bold(size::LABEL),
                    color::ACCENT,
                    Fx::Shadow,
                );
                y = r.top() - self.px(12.0);
            }
            let r = c.text(
                pos2(right, y),
                Align2::RIGHT_BOTTOM,
                v.name.to_uppercase(),
                self.display(size::HEADING),
                color::TEXT,
                Fx::Outline,
            );
            y = r.top();
        });
        layout.br_bottom = y - self.px(26.0);
    }

    pub fn timer(&self, t: &TimerHud, layout: &mut Layout) {
        let w = self.px(380.0);
        let h = self.px(64.0);
        let rect = Rect::from_min_size(pos2(layout.br_right - w, layout.br_bottom - h), vec2(w, h));
        self.backplate(rect, 10.0);
        let pad = self.px(18.0);
        let warn = t.warn;
        let col = if warn { color::WARNING } else { color::TEXT };
        let pulse = if warn { 0.75 + 0.25 * (self.time * std::f32::consts::TAU).sin() } else { 1.0 };
        self.text(
            pos2(rect.left() + pad, rect.center().y),
            Align2::LEFT_CENTER,
            t.label.to_uppercase(),
            self.bold(size::BODY),
            color::TEXT_DIM,
            Fx::None,
        );
        self.faded(pulse).text(
            pos2(rect.right() - pad, rect.center().y),
            Align2::RIGHT_CENTER,
            clock(t.seconds),
            self.display(size::HUD_SMALL_NUMBER),
            col,
            Fx::None,
        );
        if warn {
            self.p.rect_filled(
                Rect::from_min_size(rect.min, vec2(self.px(6.0), h)),
                crate::paint::cr(self.px(3.0)),
                color::WARNING,
            );
        }
        layout.br_bottom = rect.top() - self.px(10.0);
    }

    pub fn meter(&self, m: &pav_core::present::Meter, layout: &mut Layout) {
        let w = self.px(380.0);
        let h = self.px(70.0);
        let rect = Rect::from_min_size(pos2(layout.br_right - w, layout.br_bottom - h), vec2(w, h));
        self.backplate(rect, 10.0);
        let pad = self.px(16.0);
        self.text(
            pos2(rect.left() + pad, rect.top() + self.px(14.0)),
            Align2::LEFT_TOP,
            m.label.to_uppercase(),
            self.bold(size::BODY),
            color::TEXT,
            Fx::None,
        );
        let bar =
            Rect::from_min_size(pos2(rect.left() + pad, rect.bottom() - pad - self.px(12.0)), vec2(w - pad * 2.0, self.px(12.0)));
        self.bar(bar, m.value, linear(m.color));
        layout.br_bottom = rect.top() - self.px(10.0);
    }

    /// The reticle: four ticks opening with the spread, a dot, hit and kill markers.
    fn crosshair(&self, c: &Crosshair) {
        let center = self.screen.center();
        let col = if c.target { color::DANGER } else { color::TEXT };
        let gap = self.px(7.0 + c.spread.max(0.0) * 3.2);
        let len = self.px(12.0);
        let w = self.px(3.0).max(1.5);
        let shadow = Color32::from_black_alpha(160);
        for i in 0..4 {
            let d = crate::paint::dir(i as f32 * std::f32::consts::FRAC_PI_2);
            let (a, b) = (center + d * gap, center + d * (gap + len));
            self.line(a, b, w + self.px(2.0), shadow);
            self.line(a, b, w, col);
        }
        self.p.circle(center, self.px(2.5).max(1.5), col, Stroke::new(self.px(1.0), shadow));
        let marker = |amount: f32, size: f32, mcol: Color32| {
            if amount <= 0.0 {
                return;
            }
            let k = self.faded(amount.min(1.0));
            let r0 = self.px(size);
            let r1 = r0 + self.px(10.0);
            for i in 0..4 {
                let d = crate::paint::dir(std::f32::consts::FRAC_PI_4 + i as f32 * std::f32::consts::FRAC_PI_2);
                k.line(center + d * r0, center + d * r1, w + self.px(2.0), shadow);
                k.line(center + d * r0, center + d * r1, w, mcol);
            }
        };
        marker(c.hit, 12.0, color::TEXT);
        marker(c.kill, 18.0, color::DANGER);
    }
}

/// Seconds as "1:05" (or "12.4" under ten seconds, when every tenth matters).
pub(crate) fn clock(seconds: f32) -> String {
    let s = seconds.max(0.0);
    if s < 10.0 {
        return format!("{s:.1}");
    }
    let total = s.floor() as u64;
    if total >= 3600 {
        format!("{}:{:02}:{:02}", total / 3600, total / 60 % 60, total % 60)
    } else {
        format!("{}:{:02}", total / 60, total % 60)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn timers_read_as_clocks() {
        assert_eq!(super::clock(47.0), "0:47");
        assert_eq!(super::clock(9.44), "9.4");
        assert_eq!(super::clock(3725.0), "1:02:05");
        assert_eq!(super::clock(-3.0), "0.0");
    }
}
