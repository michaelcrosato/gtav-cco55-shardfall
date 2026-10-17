//! The radial wheel (weapons, characters, stations): segments clockwise from the top, the
//! focused one in the accent fill, pushed out and glowing, its name in the middle.

use egui::{Align, Align2, Color32, Rect, pos2, vec2};
use pav_core::present::Wheel;

use crate::paint::{Cx, Fx, HitArea, dir};
use crate::theme::{color, size};

impl Cx<'_> {
    pub fn wheel(&self, w: &Wheel) {
        let n = w.slots.len().max(3);
        let c = self.screen.center();
        let (r0, r1) = (self.px(150.0), self.px(340.0));
        // A soft dark pool behind the ring keeps it legible over any scene.
        let pool = Rect::from_center_size(c, vec2(r1 * 2.3, r1 * 2.3));
        self.glow(pool, r1 * 1.15 / self.u, Color32::from_black_alpha(120), 70.0);
        let step = std::f32::consts::TAU / n as f32;
        let half_gap = self.px(4.0) / ((r0 + r1) * 0.5);
        let grad = |p: egui::Pos2| {
            let t = ((p.x - (c.x - r1)) / (2.0 * r1)).clamp(0.0, 1.0);
            color::ACCENT.lerp_to_gamma(color::ACCENT_2, t)
        };
        for (i, slot) in w.slots.iter().enumerate() {
            let mid = i as f32 * step;
            let (a0, a1) = (mid - step * 0.5 + half_gap, mid + step * 0.5 - half_gap);
            let focused = w.focus == Some(i);
            let rr1 = if focused { r1 + self.px(16.0) } else { r1 };
            let k = if slot.enabled { 1.0 } else { 0.45 };
            let cs = self.faded(k);
            if focused {
                // Glow: two wider, fainter copies of the segment.
                cs.sector(c, r0 - self.px(10.0), rr1 + self.px(14.0), a0 - half_gap, a1 + half_gap, |p| {
                    grad(p).gamma_multiply(0.18)
                });
                cs.sector(c, r0 - self.px(4.0), rr1 + self.px(6.0), a0, a1, |p| grad(p).gamma_multiply(0.3));
                cs.sector(c, r0, rr1, a0, a1, grad);
            } else {
                cs.sector(c, r0, rr1, a0, a1, |_| color::BACKPLATE);
            }
            self.hit(
                HitArea::Sector { center: c, r0, r1: rr1, a0: mid - step * 0.5, a1: mid + step * 0.5 },
                format!("wheel:{i}"),
            );
            let fg = if focused { color::SLATE } else { color::TEXT };
            let mid_r = (r0 + rr1) * 0.5;
            let at = c + dir(mid) * mid_r;
            let wide = crate::icons::is_weapon(&slot.icon);
            let icon = if wide { vec2(self.px(104.0), self.px(52.0)) } else { vec2(self.px(60.0), self.px(60.0)) };
            let icon_rect = Rect::from_center_size(at - vec2(0.0, self.px(18.0)), icon);
            cs.icon(&slot.icon, icon_rect, fg);
            let label_font = self.bold(size::BODY);
            let max_w = (step * mid_r).min(self.px(190.0)) - self.px(16.0);
            let label = cs.fit(&slot.label.to_uppercase(), &label_font, max_w);
            cs.text(pos2(at.x, icon_rect.bottom() + self.px(10.0)), Align2::CENTER_TOP, label, label_font, fg, Fx::None);
        }
        // The middle: name (first line) in the display face, the rest under it.
        let rc = r0 - self.px(14.0);
        self.p.circle(c, rc, color::SLATE.gamma_multiply(0.92), egui::Stroke::new(self.px(2.0), Color32::from_white_alpha(40)));
        let (head, rest) = split_first_line(&w.center);
        let head = crate::markup::plain(head);
        let head_font = self.display(40.0);
        let head = self.fit(&head.to_uppercase(), &head_font, rc * 1.7);
        let body = (!rest.is_empty()).then(|| self.rich(rest, self.ui(size::BODY), color::TEXT_DIM, Some(rc * 1.6)));
        let (ht, hb) = self.cap(&head_font);
        let body_h = body.as_ref().map_or(0.0, |b| b.size.y + self.px(10.0));
        let top = c.y - ((hb - ht) + body_h) * 0.5;
        self.text(pos2(c.x, top), Align2::CENTER_TOP, head, head_font, color::TEXT, Fx::None);
        if let Some(b) = body {
            self.paint_rich(
                &b,
                pos2(c.x - rc * 0.8, top + (hb - ht) + self.px(14.0) - b.cap.0),
                rc * 1.6,
                Align::Center,
                Fx::None,
            );
        }
    }
}

/// The text before the first line break (`~n~` or a newline) and the marked-up rest.
fn split_first_line(text: &str) -> (&str, &str) {
    let a = text.find("~n~").map(|i| (i, 3));
    let b = text.find('\n').map(|i| (i, 1));
    match [a, b].into_iter().flatten().min_by_key(|(i, _)| *i) {
        Some((i, len)) => (&text[..i], &text[i + len..]),
        None => (text, ""),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_first_line_splits_off() {
        assert_eq!(super::split_first_line("PISTOL~n~12 / 48"), ("PISTOL", "12 / 48"));
        assert_eq!(super::split_first_line("SMG\nAmmo ~y~low"), ("SMG", "Ammo ~y~low"));
        assert_eq!(super::split_first_line("FISTS"), ("FISTS", ""));
    }
}
