//! The phone (PLAN.md §5.6.9): a handset bottom right with a clock, the open app's title, and
//! big tiles (home) or rows (contacts, messages). Tiles report their `id` to the pointer, rows
//! `phone_row:<index>`.

use egui::{Align2, Color32, Rect, Stroke, StrokeKind, pos2, vec2};
use pav_core::present::Phone;

use crate::paint::{Cx, Fx, HitArea, cr};
use crate::theme::{color, size};

/// The phone's size at 1080p.
pub(crate) const PHONE: egui::Vec2 = vec2(440.0, 800.0);

impl Cx<'_> {
    /// Where the phone sits: bottom right of the safe area.
    pub fn phone_rect(&self) -> Rect {
        let s = vec2(self.px(PHONE.x), self.px(PHONE.y));
        Rect::from_min_size(pos2(self.safe.right() - s.x, self.safe.bottom() - s.y), s)
    }

    pub fn phone(&self, ph: &Phone, rect: Rect) {
        // Body, side buttons, screen.
        self.glow(rect, 56.0, Color32::from_black_alpha(140), 26.0);
        for (y, h) in [(0.2, 0.08), (0.31, 0.11)] {
            let b = Rect::from_min_size(
                pos2(rect.right() - self.px(2.0), rect.top() + rect.height() * y),
                vec2(self.px(6.0), rect.height() * h),
            );
            self.p.rect_filled(b, cr(self.px(3.0)), Color32::from_rgb(0x2a, 0x2e, 0x37));
        }
        self.vgradient_rect(rect, 56.0, Color32::from_rgb(0x2a, 0x2e, 0x37), Color32::from_rgb(0x15, 0x17, 0x1c));
        self.outline(rect, 56.0, 2.0, Color32::from_white_alpha(40));
        let screen = rect.shrink(self.px(14.0));
        self.vgradient_rect(screen, 44.0, Color32::from_rgb(0x3a, 0x17, 0x33), Color32::from_rgb(0x0c, 0x0e, 0x13));
        // A low sunset glow at the top of the wallpaper.
        let glow = Rect::from_min_size(screen.min, vec2(screen.width(), screen.height() * 0.45));
        self.vgradient_rect(glow, 44.0, color::ACCENT_2.gamma_multiply(0.32), Color32::TRANSPARENT);
        let pad = self.px(26.0);
        let inner = screen.shrink2(vec2(pad, 0.0));
        // Status bar: signal, clock, battery.
        let sy = screen.top() + self.px(32.0);
        if ph.app != "home" {
            // The home screen shows the time big instead.
            self.text(
                pos2(screen.center().x, sy),
                Align2::CENTER_CENTER,
                &ph.clock,
                self.bold(size::BODY),
                color::TEXT,
                Fx::None,
            );
        }
        for i in 0..4 {
            let h = self.px(6.0 + i as f32 * 4.0);
            let x = inner.left() + i as f32 * self.px(9.0);
            self.p.rect_filled(
                Rect::from_min_max(pos2(x, sy + self.px(10.0) - h), pos2(x + self.px(6.0), sy + self.px(10.0))),
                cr(1.0),
                color::TEXT,
            );
        }
        let bat = Rect::from_min_size(pos2(inner.right() - self.px(40.0), sy - self.px(9.0)), vec2(self.px(36.0), self.px(18.0)));
        self.p.rect_stroke(bat, cr(self.px(4.0)), Stroke::new(self.px(2.0), color::TEXT), StrokeKind::Inside);
        self.p.rect_filled(bat.shrink(self.px(4.0)), cr(self.px(2.0)), color::POSITIVE);
        let mut y = sy + self.px(40.0);
        // Title: the home screen shows the time big instead.
        if ph.app == "home" {
            let r =
                self.text(pos2(screen.center().x, y), Align2::CENTER_TOP, &ph.clock, self.display(84.0), color::TEXT, Fx::None);
            y = r.bottom() + self.px(16.0);
            if !ph.title.is_empty() {
                let r = self.text(
                    pos2(screen.center().x, y),
                    Align2::CENTER_TOP,
                    ph.title.to_uppercase(),
                    self.bold(size::BODY),
                    color::TEXT_DIM,
                    Fx::None,
                );
                y = r.bottom();
            }
            y += self.px(36.0);
        } else {
            let r = self.text(
                pos2(inner.left(), y + self.px(4.0)),
                Align2::LEFT_TOP,
                ph.title.to_uppercase(),
                self.display(46.0),
                color::TEXT,
                Fx::None,
            );
            y = r.bottom() + self.px(30.0);
        }
        let content = Rect::from_min_max(pos2(inner.left(), y), pos2(inner.right(), screen.bottom() - self.px(40.0)));
        if !ph.tiles.is_empty() {
            self.phone_tiles(ph, content);
        } else {
            self.phone_rows(ph, content);
        }
        // Home indicator.
        let hb =
            Rect::from_center_size(pos2(screen.center().x, screen.bottom() - self.px(18.0)), vec2(self.px(120.0), self.px(6.0)));
        self.p.rect_filled(hb, cr(self.px(3.0)), Color32::from_white_alpha(150));
    }

    fn phone_tiles(&self, ph: &Phone, area: Rect) {
        let cols = 3;
        let gap = self.px(14.0);
        let tw = (area.width() - gap * (cols - 1) as f32) / cols as f32;
        let plate = (tw * 0.78).min(self.px(100.0));
        let th = plate + self.px(54.0);
        let label_font = self.bold(size::BODY);
        for (i, t) in ph.tiles.iter().enumerate().take(9) {
            let (r, c) = (i / cols, i % cols);
            let cell =
                Rect::from_min_size(pos2(area.left() + c as f32 * (tw + gap), area.top() + r as f32 * (th + gap)), vec2(tw, th));
            if cell.bottom() > area.bottom() + 1.0 {
                break;
            }
            let focused = i == ph.focus;
            let pr = Rect::from_center_size(pos2(cell.center().x, cell.top() + plate * 0.5), vec2(plate, plate));
            self.hit(HitArea::Rect(cell), t.id.clone());
            self.scaled(pr.center(), if focused { 1.05 } else { 1.0 }, |c| {
                if focused {
                    c.glow(pr, 24.0, color::ACCENT_2.gamma_multiply(0.55), 18.0);
                    c.accent(pr, 24.0);
                } else {
                    c.fill(pr, 24.0, Color32::from_white_alpha(26));
                    c.outline(pr, 24.0, 1.5, Color32::from_white_alpha(30));
                }
                c.icon(&t.icon, pr.shrink(plate * 0.24), if focused { color::SLATE } else { color::TEXT });
                if t.badge > 0 {
                    let bc = pr.right_top() + vec2(-self.px(4.0), self.px(4.0));
                    let br = self.px(16.0);
                    c.p.circle(bc, br, color::DANGER, Stroke::new(self.px(2.0), color::SLATE));
                    c.text(bc, Align2::CENTER_CENTER, t.badge.min(99).to_string(), self.bold(22.0), color::TEXT, Fx::None);
                }
                let label = c.fit(&t.label, &label_font, tw + gap * 0.8);
                c.text(
                    pos2(cell.center().x, pr.bottom() + self.px(14.0)),
                    Align2::CENTER_TOP,
                    label,
                    label_font.clone(),
                    color::TEXT,
                    Fx::Shadow,
                );
            });
        }
    }

    fn phone_rows(&self, ph: &Phone, area: Rect) {
        let h = self.px(92.0);
        let gap = self.px(8.0);
        let fits = (((area.height() + gap) / (h + gap)).floor() as usize).max(1);
        let n = ph.rows.len();
        let start = ph.focus.saturating_sub(fits / 2).min(n.saturating_sub(fits));
        let title_font = self.bold(30.0);
        let detail_font = self.ui(size::BODY);
        let pad = self.px(18.0);
        for (k, row) in ph.rows.iter().enumerate().skip(start).take(fits) {
            let i = k - start;
            let r = Rect::from_min_size(pos2(area.left(), area.top() + i as f32 * (h + gap)), vec2(area.width(), h));
            let focused = k == ph.focus;
            self.hit(HitArea::Rect(r), format!("phone_row:{k}"));
            self.scaled(r.center(), if focused { 1.05 } else { 1.0 }, |c| {
                if focused {
                    c.glow(r, 16.0, color::ACCENT_2.gamma_multiply(0.5), 16.0);
                    c.accent(r, 16.0);
                } else {
                    c.fill(r, 16.0, Color32::from_white_alpha(18));
                }
                let (fg, dim) =
                    if focused { (color::SLATE, color::SLATE.gamma_multiply(0.8)) } else { (color::TEXT, color::TEXT_DIM) };
                let w = r.width() - pad * 2.0;
                let has_detail = !row.detail.is_empty();
                let ty = if has_detail { r.top() + self.px(20.0) } else { r.center().y };
                let align = if has_detail { Align2::LEFT_TOP } else { Align2::LEFT_CENTER };
                c.text(pos2(r.left() + pad, ty), align, c.fit(&row.title, &title_font, w), title_font.clone(), fg, Fx::None);
                if has_detail {
                    let d = c.fit(&crate::markup::plain(&row.detail), &detail_font, w);
                    c.text(
                        pos2(r.left() + pad, r.bottom() - self.px(18.0)),
                        Align2::LEFT_BOTTOM,
                        d,
                        detail_font.clone(),
                        dim,
                        Fx::None,
                    );
                }
            });
        }
        if n > fits {
            // Where in the list we are: a thin scroll track on the right.
            let track = Rect::from_min_max(
                pos2(area.right() + self.px(8.0), area.top()),
                pos2(area.right() + self.px(12.0), area.top() + fits as f32 * (h + gap) - gap),
            );
            self.p.rect_filled(track, cr(self.px(2.0)), Color32::from_white_alpha(30));
            let frac = fits as f32 / n as f32;
            let pos = start as f32 / n as f32;
            let thumb = Rect::from_min_size(
                pos2(track.left(), track.top() + track.height() * pos),
                vec2(track.width(), track.height() * frac),
            );
            self.p.rect_filled(thumb, cr(self.px(2.0)), color::TEXT);
        }
    }
}
