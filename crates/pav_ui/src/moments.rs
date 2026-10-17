//! Everything that comes and goes over play: banners, toasts, subtitles, context prompts, the
//! help box, world markers, whole-screen overlays, the perf readout and the touch gear.

use egui::{Align, Align2, Color32, Mesh, Pos2, Rect, Shape, Vec2, pos2, vec2};
use pav_core::present::{Banner, BannerKind, Help, Overlay, Prompt, ScreenMarker, Subtitle, Toast};

use crate::Layout;

/// How wide subtitles run before wrapping (1080p pixels).
const SUBTITLE_WRAP: f32 = 1240.0;
use crate::paint::{Cx, Fx, HitArea, ease_out, ramp};
use crate::theme::{color, linear, size};

impl Cx<'_> {
    /// Big centre text: scales in by `age`, fades out over the last half second.
    pub fn banner(&self, b: &Banner, index: usize) {
        let t_in = ease_out(ramp(b.age, 0.0, 0.3));
        let out = if b.duration > 0.0 { 1.0 - ramp(b.age, b.duration - 0.5, 0.5) } else { 1.0 };
        let alpha = (ramp(b.age, 0.0, 0.18) * out).clamp(0.0, 1.0);
        if alpha <= 0.0 {
            return;
        }
        let (top, bottom) = match b.kind {
            BannerKind::Passed => (Color32::from_rgb(0xff, 0xe4, 0x9a), Color32::from_rgb(0xf2, 0xa5, 0x1e)),
            BannerKind::Failed | BannerKind::Wasted => (Color32::from_rgb(0xff, 0x70, 0x6a), Color32::from_rgb(0xc8, 0x1e, 0x24)),
            BannerKind::Busted => (Color32::from_rgb(0x8c, 0xbc, 0xff), Color32::from_rgb(0x2a, 0x68, 0xe0)),
            BannerKind::Stunt => (color::ACCENT, color::ACCENT_2),
            BannerKind::Info => (color::TEXT, Color32::from_rgb(0xc9, 0xc4, 0xba)),
        };
        let c = self.faded(alpha);
        let font = self.display(size::BANNER);
        let galley = self.galley(b.title.to_uppercase(), font.clone(), top);
        let (ct, cb) = self.cap(&font);
        let cap_h = cb - ct;
        let cy = self.screen.top() + self.screen.height() * 0.36 + index as f32 * self.px(200.0);
        let sub = b.subtitle.as_ref().map(|s| self.rich(s, self.bold(size::ITEM), color::TEXT, Some(self.px(1200.0))));
        let sub_h = sub.as_ref().map_or(0.0, |s| s.size.y + self.px(18.0));
        let band = Rect::from_center_size(
            pos2(self.screen.center().x, cy + sub_h * 0.5),
            vec2(self.screen.width(), cap_h + sub_h + self.px(110.0)),
        );
        c.band(band, Color32::from_black_alpha(150), self.screen.width() * 0.3);
        let center = pos2(self.screen.center().x, cy);
        c.scaled(center, 1.22 - 0.22 * t_in, |c| {
            let pos = pos2(center.x - galley.size().x * 0.5, cy - cap_h * 0.5 - ct);
            c.gradient_galley(pos, &galley, top, bottom, Fx::Outline);
        });
        if let Some(s) = &sub {
            let w = self.px(1200.0);
            c.paint_rich(s, pos2(center.x - w * 0.5, cy + cap_h * 0.5 + self.px(26.0) - s.cap.0), w, Align::Center, Fx::Shadow);
        }
    }

    /// Notifications down the right column, newest at the top, sliding in from the right.
    pub fn toasts(&self, toasts: &[Toast], layout: &Layout) {
        let w = self.px(480.0);
        let mut y = layout.right_y;
        for t in toasts.iter().rev().take(3) {
            let slide = ease_out(ramp(t.age, 0.0, 0.3));
            let c = self.faded(ramp(t.age, 0.0, 0.15));
            let pad = self.px(18.0);
            let icon = self.px(60.0);
            let text_w = w - pad * 3.0 - icon;
            let body = self.rich(&t.body, self.ui(size::BODY), color::TEXT, Some(text_w));
            let title_font = self.bold(30.0);
            let (tt, tb) = self.cap(&title_font);
            let title_h = tb - tt;
            let h = (pad * 2.0 + title_h + self.px(14.0) + body.size.y).max(icon + pad * 2.0);
            let rect = Rect::from_min_size(pos2(layout.right_x - w, y), vec2(w, h));
            c.moved(vec2((1.0 - slide) * (w + self.px(60.0)), 0.0), |c| {
                c.backplate(rect, 14.0);
                let ir = Rect::from_min_size(rect.min + vec2(pad, pad), vec2(icon, icon));
                c.accent(ir, 12.0);
                c.icon(&t.icon, ir.shrink(self.px(12.0)), color::SLATE);
                let x = ir.right() + pad;
                c.text(
                    pos2(x, rect.top() + pad),
                    Align2::LEFT_TOP,
                    c.fit(&t.title.to_uppercase(), &title_font, text_w),
                    title_font.clone(),
                    color::TEXT,
                    Fx::None,
                );
                c.paint_rich(
                    &body,
                    pos2(x, rect.top() + pad + title_h + self.px(14.0) - body.cap.0),
                    text_w,
                    Align::Min,
                    Fx::None,
                );
            });
            y += h + self.px(12.0);
        }
    }

    /// The subtitle laid out: the speaker in their colour, then the words.
    pub fn subtitle_layout(&self, s: &Subtitle) -> crate::markup::Rich {
        let speaker_col = s.color.map(linear).unwrap_or(color::ACCENT);
        let lead = s.speaker.as_ref().map(|n| format!("{}: ", n.to_uppercase()));
        self.rich_prefixed(
            lead.as_deref().map(|l| (l, speaker_col)),
            &s.text,
            self.bold(size::LABEL),
            color::TEXT,
            Some(self.px(SUBTITLE_WRAP)),
        )
    }

    /// Paints a laid-out subtitle centred, its last row ending at `bottom`.
    pub fn paint_subtitle(&self, rich: &crate::markup::Rich, bottom: f32) {
        let wrap = self.px(SUBTITLE_WRAP);
        let top = bottom - rich.size.y;
        self.paint_rich(rich, pos2(self.screen.center().x - wrap * 0.5, top), wrap, Align::Center, Fx::Shadow);
    }

    /// Context prompts bottom right, stacked upward: glyph and verb on a pill.
    pub fn prompts(&self, prompts: &[Prompt], layout: &mut Layout) {
        for pr in prompts.iter().rev() {
            let r = self.prompt_pill(pr, pos2(layout.br_right, layout.br_bottom), Align2::RIGHT_BOTTOM, self.px(62.0));
            layout.br_bottom = r.top() - self.px(10.0);
        }
    }

    /// One glyph-and-verb pill anchored at `at`. Returns its rectangle.
    pub fn prompt_pill(&self, pr: &Prompt, at: Pos2, anchor: Align2, h: f32) -> Rect {
        let font = self.bold(30.0);
        let gh = h * 0.66;
        let gw = self.glyph_width(pr.button, gh);
        let pad = h * 0.22;
        let label = pr.label.to_uppercase();
        let lw = self.text_width(&label, &font);
        let w = pad + gw + pad * 1.1 + lw + pad * 1.6;
        let rect = anchor.anchor_size(at, vec2(w, h));
        self.backplate(rect, h * 0.5 / self.u);
        let gc = pos2(rect.left() + pad + gw * 0.5 + h * 0.06, rect.center().y);
        if let Some(p) = pr.hold {
            self.hold_ring(gc, gh, p);
        }
        self.paint_glyph(pr.button, gc, gh);
        self.text(pos2(gc.x + gw * 0.5 + pad * 1.1, rect.center().y), Align2::LEFT_CENTER, label, font, color::TEXT, Fx::None);
        rect
    }

    /// The tutorial tip, top left: a plate with an accent edge, at most a few lines.
    pub fn help(&self, h: &Help, layout: &mut Layout) {
        let t = ease_out(ramp(h.age, 0.0, 0.25));
        let c = self.faded(t);
        // At most two lines: the box widens before the text wraps a third time.
        let mut wrap = self.px(560.0);
        let mut rich = self.rich(&h.text, self.ui(size::BODY), color::TEXT, Some(wrap));
        if rich.lines.len() > 2 {
            wrap = self.px(780.0).min(self.safe.width() * 0.5);
            rich = self.rich(&h.text, self.ui(size::BODY), color::TEXT, Some(wrap));
        }
        let pad = self.px(20.0);
        let edge = self.px(6.0);
        let rect = Rect::from_min_size(
            pos2(self.safe.left(), layout.top_left),
            vec2(rich.size.x + pad * 2.0 + edge, rich.size.y + pad * 1.6),
        );
        c.moved(vec2(-(1.0 - t) * self.px(30.0), 0.0), |c| {
            c.backplate(rect, 10.0);
            let bar = Rect::from_min_size(rect.min, vec2(edge, rect.height()));
            c.p.rect_filled(bar, egui::CornerRadius { nw: (edge) as u8, sw: (edge) as u8, ..Default::default() }, color::ACCENT);
            c.paint_rich(&rich, pos2(rect.left() + edge + pad, rect.top() + pad * 0.8), wrap, Align::Min, Fx::None);
        });
        layout.top_left = rect.bottom() + self.px(14.0);
    }

    /// World points on screen; off-screen ones as arrows on the safe edge.
    /// `from` is where distances are measured from; `flat` measures them on the ground (from
    /// the radar's player position) rather than in 3D (from the camera).
    pub fn markers(&self, markers: &[ScreenMarker], vp: glam::Mat4, from: Option<glam::Vec3>, flat: bool) {
        let s = self.px(40.0);
        let edge = self.safe.shrink(s * 0.9);
        for m in markers {
            let clip = vp * m.pos.extend(1.0);
            let col = linear(m.color);
            let behind = clip.w <= 1e-4;
            // Behind the camera the projection mirrors: (x, y) of clip space still says which way
            // to turn; such markers sit on the lower half of the edge.
            let ndc = if behind {
                glam::Vec2::new(clip.x, clip.y.min(0.0) - clip.w.abs().max(1e-3)) * 1e4
            } else {
                glam::Vec2::new(clip.x, clip.y) / clip.w
            };
            let p = pos2(
                self.screen.left() + (ndc.x * 0.5 + 0.5) * self.screen.width(),
                self.screen.top() + (0.5 - ndc.y * 0.5) * self.screen.height(),
            );
            let dist = from.filter(|_| m.distance).map(|e| {
                let d = m.pos - e;
                if flat { glam::Vec2::new(d.x, d.z).length() } else { d.length() }
            });
            if !behind && edge.contains(p) {
                self.blip(m.icon, p, s, col);
                let mut y = p.y + s * 0.62;
                if let Some(l) = &m.label {
                    let r = self.text(
                        pos2(p.x, y),
                        Align2::CENTER_TOP,
                        l.to_uppercase(),
                        self.bold(size::BODY),
                        color::TEXT,
                        Fx::Shadow,
                    );
                    y = r.bottom() + self.px(8.0);
                }
                if let Some(d) = dist {
                    self.text(pos2(p.x, y), Align2::CENTER_TOP, metres(d), self.bold(size::BODY), color::TEXT_DIM, Fx::Shadow);
                }
            } else {
                // Clamp along the ray from the centre and point the way.
                let c = self.screen.center();
                let d = p - c;
                let d = if d.length_sq() < 1e-6 { vec2(0.0, 1.0) } else { d };
                let k = (d.x.abs() / (edge.width() * 0.5)).max(d.y.abs() / (edge.height() * 0.5)).max(1e-6);
                let q = c + d / k;
                let a = self.bearing_of(d);
                let f = crate::paint::dir(a);
                let side = Vec2::new(-f.y, f.x);
                let tip = q + f * s * 0.95;
                let base = q + f * s * 0.62;
                self.polygon_outlined(
                    vec![tip, base + side * s * 0.26, base - side * s * 0.26],
                    col,
                    self.px(2.0),
                    Color32::from_black_alpha(200),
                );
                self.blip(m.icon, q, s * 0.9, col);
                if let Some(d) = dist {
                    // Beside the icon on the screen's side of it.
                    let (anchor, at) = if f.x.abs() > f.y.abs() {
                        if f.x > 0.0 {
                            (Align2::RIGHT_CENTER, q - vec2(s * 0.8, 0.0))
                        } else {
                            (Align2::LEFT_CENTER, q + vec2(s * 0.8, 0.0))
                        }
                    } else if f.y > 0.0 {
                        (Align2::CENTER_BOTTOM, q - vec2(0.0, s * 0.8))
                    } else {
                        (Align2::CENTER_TOP, q + vec2(0.0, s * 0.8))
                    };
                    self.text(at, anchor, metres(d), self.bold(size::BODY), color::TEXT, Fx::Shadow);
                }
            }
        }
    }

    /// Tint and vignette: world effects, under the HUD.
    pub fn overlay_under(&self, o: &Overlay) {
        if let Some([r, g, b, k]) = o.tint {
            if k > 0.0 {
                let c = linear(pav_core::Color([r, g, b]));
                self.p.rect_filled(self.screen, 0.0, c.gamma_multiply(k.clamp(0.0, 1.0)));
            }
        }
        if o.vignette > 0.0 {
            self.vignette(o.vignette.clamp(0.0, 1.0));
        }
    }

    /// Darkened edges: rings from an inner ellipse (clear) to the screen's edge, eased so the
    /// falloff shows no bands.
    fn vignette(&self, k: f32) {
        let r = self.screen;
        let c = r.center();
        // Outer points walk the screen's edge (corners included), inner ones sit on an ellipse
        // in the same directions.
        let per_side = 16;
        let corners = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()];
        let mut edge = Vec::with_capacity(per_side * 4);
        for i in 0..4 {
            let (a, b) = (corners[i], corners[(i + 1) % 4]);
            for j in 0..per_side {
                edge.push(a + (b - a) * (j as f32 / per_side as f32));
            }
        }
        let (ex, ey) = (r.width() * 0.5 * 0.6, r.height() * 0.5 * 0.5);
        let rings = [0.0f32, 0.25, 0.5, 0.75, 1.0];
        let mut mesh = Mesh::default();
        for e in &edge {
            let d = (*e - c).normalized();
            let s = 1.0 / ((d.x / ex).powi(2) + (d.y / ey).powi(2)).sqrt();
            let inner = c + d * s;
            for t in rings {
                let alpha = k * 235.0 * t.powf(1.7);
                mesh.colored_vertex(inner + (*e - inner) * t, Color32::from_black_alpha(alpha as u8));
            }
        }
        let (n, m) = (edge.len() as u32, rings.len() as u32);
        for i in 0..n {
            let j = (i + 1) % n;
            for q in 0..m - 1 {
                let (a, b) = (i * m + q, j * m + q);
                mesh.add_triangle(a, a + 1, b);
                mesh.add_triangle(b, a + 1, b + 1);
            }
        }
        self.p.add(Shape::mesh(mesh));
    }

    pub fn letterbox(&self, amount: f32) {
        let h = self.screen.height() * 0.12 * amount.clamp(0.0, 1.0);
        if h > 0.0 {
            self.p.rect_filled(Rect::from_min_size(self.screen.min, vec2(self.screen.width(), h)), 0.0, Color32::BLACK);
            self.p.rect_filled(
                Rect::from_min_max(pos2(self.screen.left(), self.screen.bottom() - h), self.screen.max),
                0.0,
                Color32::BLACK,
            );
        }
    }

    pub fn fade(&self, amount: f32) {
        if amount > 0.0 {
            self.p.rect_filled(self.screen, 0.0, Color32::from_black_alpha((amount.clamp(0.0, 1.0) * 255.0) as u8));
        }
    }

    /// "60 FPS  16.6 ms", top left. Returns the bottom.
    pub fn perf(&self, fps: f32, frame_ms: f32, top: f32) -> f32 {
        let font = self.bold(size::BODY);
        let x = self.safe.left();
        let col = if fps >= 55.0 {
            color::TEXT
        } else if fps >= 28.0 {
            color::WARNING
        } else {
            color::DANGER
        };
        let r = self.text(pos2(x, top), Align2::LEFT_TOP, format!("{:.0} FPS", fps), font.clone(), col, Fx::Shadow);
        self.text(
            pos2(r.right() + self.px(14.0), top),
            Align2::LEFT_TOP,
            format!("{frame_ms:.1} ms"),
            font,
            color::TEXT_DIM,
            Fx::Shadow,
        );
        r.bottom() + self.px(18.0)
    }

    /// The touch gear, top right. Returns its rectangle.
    pub fn gear(&self) -> Rect {
        let s = self.px(68.0);
        let rect = Rect::from_min_size(pos2(self.safe.right() - s, self.safe.top()), vec2(s, s));
        self.p.circle_filled(rect.center(), s * 0.5, color::BACKPLATE);
        self.p.circle_stroke(rect.center(), s * 0.5 - 1.0, egui::Stroke::new(self.px(2.0), Color32::from_white_alpha(60)));
        self.icon("settings", rect.shrink(s * 0.24), color::TEXT);
        self.hit(HitArea::Rect(rect), "gear");
        rect
    }
}

/// "85 m" / "1.2 km".
pub(crate) fn metres(d: f32) -> String {
    if d >= 1000.0 { format!("{:.1} km", d / 1000.0) } else { format!("{:.0} m", d) }
}

/// The camera position of a perspective view-projection (where x, y and w of clip space are
/// zero), or None for an orthographic one.
pub(crate) fn eye_of(vp: glam::Mat4) -> Option<glam::Vec3> {
    let (r0, r1, r3) = (vp.row(0), vp.row(1), vp.row(3));
    let a = glam::Mat3::from_cols(r0.truncate(), r1.truncate(), r3.truncate()).transpose();
    if a.determinant().abs() < 1e-9 {
        return None;
    }
    Some(a.inverse() * -glam::Vec3::new(r0.w, r1.w, r3.w))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_eye_comes_back_out_of_a_view_projection() {
        let eye = glam::Vec3::new(3.0, 12.0, -7.0);
        let view = glam::camera::rh::view::look_at_mat4(eye, glam::Vec3::new(0.0, 1.0, 0.0), glam::Vec3::Y);
        let proj = glam::camera::rh::proj::directx::perspective(1.0, 16.0 / 9.0, 0.1, 500.0);
        let got = eye_of(proj * view).expect("perspective");
        assert!(got.distance(eye) < 1e-3, "{got}");
        assert_eq!(metres(85.4), "85 m");
        assert_eq!(metres(1234.0), "1.2 km");
    }
}
