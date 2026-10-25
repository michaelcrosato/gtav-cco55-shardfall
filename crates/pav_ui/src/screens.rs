//! Full-screen pages: title and attract, menus, card rows, the pause map, text, loading and
//! results, with the title-and-tabs header and the button-hint bar every menu screen gets.

use egui::text::LayoutJob;
use egui::{Align, Align2, Color32, Pos2, Rect, Stroke, pos2, vec2};
use pav_core::present::{
    Backdrop, BlipIcon, CardRow, ItemValue, LoadingCard, MapView, Medal, MenuItem, MenuList, Results, Screen, ScreenBody,
    TextCard, TitleCard,
};

use crate::map::MapTex;
use crate::paint::{Cx, Fx, HitArea, cr, money};
use crate::theme::{color, linear, size};

impl Cx<'_> {
    pub fn screen(&self, s: &Screen, tex: Option<MapTex>) {
        self.backdrop(s.backdrop);
        match &s.body {
            ScreenBody::Title(t) => self.title_screen(s, t),
            ScreenBody::Menu(m) => {
                let top = self.header(s);
                let w = self.px(840.0);
                let bottom = self.menu_list(m, pos2(self.safe.left(), top), w, false);
                self.item_note(m, pos2(self.safe.left(), bottom + self.px(22.0)), w);
            }
            ScreenBody::Cards(c) => {
                let top = self.header(s);
                let area =
                    Rect::from_min_max(pos2(self.safe.left(), top), pos2(self.safe.right(), self.safe.bottom() - self.px(96.0)));
                self.cards(c, area);
            }
            ScreenBody::Map(mv) => {
                self.pause_map(mv, self.screen, tex);
                // A scrim at the top keeps the header legible over the map.
                let scrim = Rect::from_min_size(self.screen.min, vec2(self.screen.width(), self.px(360.0)));
                self.scrim(scrim, Color32::from_black_alpha(215), Color32::TRANSPARENT);
                let top = self.header(s);
                self.map_extras(mv, top);
            }
            ScreenBody::Text(t) => {
                let top = self.header(s);
                self.text_card(t, top);
            }
            ScreenBody::Loading(l) => self.loading(l, s.backdrop),
            ScreenBody::Results(r) => self.results(r),
        }
        if !s.hints.is_empty() {
            self.hints(&s.hints);
        }
    }

    fn backdrop(&self, b: Backdrop) {
        match b {
            Backdrop::Clear => {}
            Backdrop::Dim => {
                self.p.rect_filled(self.screen, 0.0, Color32::from_rgba_unmultiplied(0x0e, 0x11, 0x16, 165));
                let top = Rect::from_min_size(self.screen.min, vec2(self.screen.width(), self.screen.height() * 0.4));
                self.scrim(top, Color32::from_black_alpha(120), Color32::TRANSPARENT);
                let bottom = Rect::from_min_max(
                    pos2(self.screen.left(), self.screen.bottom() - self.screen.height() * 0.3),
                    self.screen.max,
                );
                self.scrim(bottom, Color32::TRANSPARENT, Color32::from_black_alpha(120));
            }
            Backdrop::Solid => {
                self.scrim(self.screen, Color32::from_rgb(0x16, 0x1a, 0x23), Color32::from_rgb(0x07, 0x08, 0x0b));
            }
        }
    }

    /// Title, accent bar and tabs, top left. Returns where the content starts.
    fn header(&self, s: &Screen) -> f32 {
        let x = self.safe.left();
        let title_font = self.display(size::TITLE);
        let r =
            self.text(pos2(x, self.safe.top()), Align2::LEFT_TOP, s.title.to_uppercase(), title_font, color::TEXT, Fx::Shadow);
        let bar = Rect::from_min_size(pos2(x, r.bottom() + self.px(18.0)), vec2(self.px(132.0), self.px(7.0)));
        self.accent(bar, 3.5);
        let mut y = bar.bottom() + self.px(26.0);
        if !s.tabs.is_empty() {
            y = self.tabs(&s.tabs, s.tab, pos2(x, y)) + self.px(34.0);
        } else {
            y += self.px(12.0);
        }
        y
    }

    /// The tab strip with the bumper glyphs that cycle it. Returns its bottom.
    fn tabs(&self, tabs: &[String], active: usize, at: Pos2) -> f32 {
        let h = self.px(58.0);
        let font = self.bold(30.0);
        let gh = h * 0.7;
        let mut x = at.x;
        let gw = self.glyph_width(pav_core::present::PadButton::L1, gh);
        self.paint_glyph(pav_core::present::PadButton::L1, pos2(x + gw * 0.5, at.y + h * 0.5), gh);
        x += gw + self.px(12.0);
        for (i, t) in tabs.iter().enumerate() {
            let label = t.to_uppercase();
            let w = self.text_width(&label, &font) + self.px(52.0);
            let r = Rect::from_min_size(pos2(x, at.y), vec2(w, h));
            let on = i == active;
            self.hit(HitArea::Rect(r), format!("tab:{i}"));
            if on {
                self.glow(r, 10.0, color::ACCENT_2.gamma_multiply(0.45), 16.0);
                self.accent(r, 10.0);
            } else {
                self.backplate(r, 10.0);
            }
            self.text(
                r.center(),
                Align2::CENTER_CENTER,
                label,
                font.clone(),
                if on { color::SLATE } else { color::TEXT_DIM },
                Fx::None,
            );
            x = r.right() + self.px(8.0);
        }
        x += self.px(4.0);
        let gw = self.glyph_width(pav_core::present::PadButton::R1, gh);
        self.paint_glyph(pav_core::present::PadButton::R1, pos2(x + gw * 0.5, at.y + h * 0.5), gh);
        at.y + h
    }

    /// The button-hint bar, bottom right: glyph and verb pairs on one plate.
    fn hints(&self, hints: &[pav_core::present::Prompt]) {
        let h = self.px(60.0);
        let gh = h * 0.64;
        let font = self.bold(size::BODY);
        let gap = self.px(30.0);
        let pad = self.px(20.0);
        let items: Vec<(f32, String, f32)> = hints
            .iter()
            .map(|p| {
                let label = p.label.to_uppercase();
                let lw = self.text_width(&label, &font);
                (self.glyph_width(p.button, gh), label, lw)
            })
            .collect();
        let inner: f32 =
            items.iter().map(|(gw, _, lw)| gw + self.px(12.0) + lw).sum::<f32>() + gap * (items.len().saturating_sub(1)) as f32;
        let rect =
            Rect::from_min_size(pos2(self.safe.right() - inner - pad * 2.0, self.safe.bottom() - h), vec2(inner + pad * 2.0, h));
        self.backplate(rect, 30.0);
        let mut x = rect.left() + pad;
        for (p, (gw, label, lw)) in hints.iter().zip(items) {
            self.paint_glyph(p.button, pos2(x + gw * 0.5, rect.center().y), gh);
            x += gw + self.px(12.0);
            self.text(pos2(x, rect.center().y), Align2::LEFT_CENTER, label, font.clone(), color::TEXT, Fx::None);
            x += lw + gap;
        }
    }

    // ---- menus ------------------------------------------------------------------------------

    /// A vertical list of at most seven rows from `at` (left, or centre when `centered`).
    /// Returns its bottom.
    pub fn menu_list(&self, m: &MenuList, at: Pos2, w: f32, centered: bool) -> f32 {
        let h = self.px(70.0);
        let gap = self.px(10.0);
        let max = 7;
        let n = m.items.len();
        let start = m.focus.saturating_sub(max / 2).min(n.saturating_sub(max));
        let left = if centered { at.x - w * 0.5 } else { at.x };
        let mut y = at.y;
        if start > 0 {
            self.chevron(pos2(left + w * 0.5, y + self.px(6.0)), true);
            y += self.px(22.0);
        }
        for (k, item) in m.items.iter().enumerate().skip(start).take(max) {
            let r = Rect::from_min_size(pos2(left, y), vec2(w, h));
            self.menu_item(item, r, k == m.focus, centered);
            y += h + gap;
        }
        if start + max < n {
            self.chevron(pos2(left + w * 0.5, y + self.px(4.0)), false);
            y += self.px(22.0);
        }
        y - gap
    }

    /// The height [`Cx::menu_list`] takes for `m`.
    pub fn menu_height(&self, m: &MenuList) -> f32 {
        let n = m.items.len().min(7) as f32;
        let more = if m.items.len() > 7 { self.px(44.0) } else { 0.0 };
        n * self.px(80.0) - self.px(10.0) + more
    }

    fn chevron(&self, c: Pos2, up: bool) {
        let s = self.px(10.0);
        let d = if up { -1.0 } else { 1.0 };
        self.polygon(
            vec![c + vec2(0.0, d * s), c + vec2(s * 1.4, -d * s * 0.4), c + vec2(-s * 1.4, -d * s * 0.4)],
            color::TEXT_DIM,
        );
    }

    fn menu_item(&self, item: &MenuItem, r: Rect, focused: bool, centered: bool) {
        self.hit(HitArea::Rect(r), item.id.clone());
        let c = self.faded(if item.enabled { 1.0 } else { 0.45 });
        c.scaled(r.center(), if focused { 1.05 } else { 1.0 }, |c| {
            if focused {
                c.glow(r, 12.0, color::ACCENT_2.gamma_multiply(0.55), 22.0);
                c.accent(r, 12.0);
            } else {
                c.backplate(r, 12.0);
            }
            let fg = if focused { color::SLATE } else { color::TEXT };
            let pad = self.px(28.0);
            let font = self.bold(size::ITEM);
            let has_value = !matches!(item.value, ItemValue::Button);
            // Long labels end in an ellipsis before the value's room (or the lock's).
            let room = if has_value {
                self.px(400.0)
            } else if item.enabled {
                0.0
            } else {
                self.px(56.0)
            };
            let label = c.fit(&item.label.to_uppercase(), &font, r.width() - pad * 2.0 - room);
            if centered && !has_value {
                c.text(r.center(), Align2::CENTER_CENTER, label, font, fg, Fx::None);
            } else {
                c.text(pos2(r.left() + pad, r.center().y), Align2::LEFT_CENTER, label, font, fg, Fx::None);
            }
            if !item.enabled {
                let lr = Rect::from_center_size(
                    pos2(r.right() - pad - self.px(14.0), r.center().y),
                    vec2(self.px(28.0), self.px(28.0)),
                );
                c.icon("lock", lr, fg);
                return;
            }
            c.item_value(&item.value, pos2(r.right() - pad, r.center().y), focused);
        });
    }

    /// A row's value, right-aligned at `right` (vertical centre `right.y`).
    fn item_value(&self, v: &ItemValue, right: Pos2, focused: bool) {
        let fg = if focused { color::SLATE } else { color::TEXT };
        let font = self.bold(30.0);
        match v {
            ItemValue::Button => {}
            ItemValue::Toggle { on } => {
                let (tw, th) = (self.px(72.0), self.px(36.0));
                let track = Rect::from_min_size(pos2(right.x - tw, right.y - th * 0.5), vec2(tw, th));
                let fill = match (on, focused) {
                    (true, false) => color::POSITIVE,
                    (true, true) => color::SLATE,
                    (false, false) => color::SLATE_2,
                    (false, true) => Color32::from_rgba_unmultiplied(0x0e, 0x11, 0x16, 90),
                };
                self.p.rect_filled(track, cr(th * 0.5), fill);
                let kx = if *on { track.right() - th * 0.5 } else { track.left() + th * 0.5 };
                let knob = if focused && *on { color::ACCENT } else { color::TEXT };
                self.p.circle_filled(pos2(kx, right.y), th * 0.36, knob);
                self.text(
                    pos2(track.left() - self.px(16.0), right.y),
                    Align2::RIGHT_CENTER,
                    if *on { "ON" } else { "OFF" },
                    font,
                    fg,
                    Fx::None,
                );
            }
            ItemValue::Slider { value, text } => {
                let r = self.text(right, Align2::RIGHT_CENTER, text, font, fg, Fx::None);
                let bw = self.px(240.0);
                let bar =
                    Rect::from_min_size(pos2(r.left() - self.px(22.0) - bw, right.y - self.px(7.0)), vec2(bw, self.px(14.0)));
                if focused {
                    self.p.rect_filled(bar, cr(bar.height() * 0.5), Color32::from_rgba_unmultiplied(0x0e, 0x11, 0x16, 80));
                    let f =
                        Rect::from_min_size(bar.min, vec2((bar.width() * value.clamp(0.0, 1.0)).max(bar.height()), bar.height()));
                    self.p.rect_filled(f, cr(bar.height() * 0.5), color::SLATE);
                } else {
                    self.accent_bar(bar, *value);
                }
            }
            ItemValue::Choice { options, index } => {
                let text = options.get(*index).map(|s| s.to_uppercase()).unwrap_or_default();
                let s = self.px(9.0);
                let arrow_col = if focused { color::SLATE } else { color::TEXT_DIM };
                let rx = right.x - s;
                self.polygon(
                    vec![pos2(rx + s, right.y), pos2(rx - s * 0.6, right.y - s * 1.2), pos2(rx - s * 0.6, right.y + s * 1.2)],
                    arrow_col,
                );
                let r = self.text(pos2(rx - s * 2.2, right.y), Align2::RIGHT_CENTER, text, font, fg, Fx::None);
                let lx = r.left() - s * 1.6;
                self.polygon(
                    vec![pos2(lx - s, right.y), pos2(lx + s * 0.6, right.y - s * 1.2), pos2(lx + s * 0.6, right.y + s * 1.2)],
                    arrow_col,
                );
            }
        }
    }

    /// The focused item's description under the list.
    fn item_note(&self, m: &MenuList, at: Pos2, w: f32) {
        let Some(note) = m.items.get(m.focus).and_then(|i| i.note.as_ref()) else { return };
        let pad = self.px(22.0);
        let rich = self.rich(note, self.ui(size::BODY), color::TEXT, Some(w - pad * 2.0));
        let rect = Rect::from_min_size(at, vec2(w, rich.size.y + pad * 1.6));
        self.backplate(rect, 12.0);
        self.paint_rich(&rich, pos2(at.x + pad, at.y + pad * 0.8), w - pad * 2.0, Align::Min, Fx::None);
    }

    // ---- cards ------------------------------------------------------------------------------

    fn cards(&self, row: &CardRow, area: Rect) {
        let n = row.cards.len().max(1);
        let gap = self.px(32.0);
        let w = ((area.width() - gap * (n - 1) as f32) / n as f32).min(self.px(440.0));
        let h = area.height().min(self.px(600.0));
        let total = w * n as f32 + gap * (n - 1) as f32;
        let x0 = area.center().x - total * 0.5;
        let top = area.top() + self.px(12.0);
        for (i, card) in row.cards.iter().enumerate() {
            let r = Rect::from_min_size(pos2(x0 + i as f32 * (w + gap), top), vec2(w, h));
            let focused = i == row.focus;
            self.hit(HitArea::Rect(r), card.id.clone());
            let accent = linear(card.accent);
            let c = self.faded(if card.enabled { 1.0 } else { 0.45 });
            c.scaled(r.center(), if focused { 1.05 } else { 1.0 }, |c| {
                if focused {
                    c.glow(r, 20.0, color::ACCENT_2.gamma_multiply(0.5), 26.0);
                }
                c.fill(r, 20.0, Color32::from_rgba_unmultiplied(0x0e, 0x11, 0x16, 242));
                // Header band: the accent fill when focused, the card's colour otherwise.
                let pad = self.px(28.0);
                let band_h = self.px(150.0);
                let band = Rect::from_min_size(r.min, vec2(r.width(), band_h));
                let rr = (self.px(20.0)).round() as u8;
                let band_round = egui::CornerRadius { nw: rr, ne: rr, sw: 0, se: 0 };
                if focused {
                    let pts = crate::paint::rounded_rect_points(band, self.px(20.0));
                    // Square off the band's bottom corners.
                    let pts: Vec<Pos2> = pts
                        .into_iter()
                        .map(|p| if p.y > band.center().y { pos2(p.x, p.y.max(band.bottom() - 0.01)) } else { p })
                        .collect();
                    c.convex_fill(&pts, |p| {
                        color::ACCENT.lerp_to_gamma(color::ACCENT_2, ((p.x - band.left()) / band.width()).clamp(0.0, 1.0))
                    });
                } else {
                    c.p.rect_filled(band, band_round, accent.gamma_multiply(0.28));
                    c.p.rect_filled(
                        Rect::from_min_size(band.min, vec2(band.width(), self.px(6.0))),
                        egui::CornerRadius { nw: rr, ne: rr, sw: 0, se: 0 },
                        accent,
                    );
                }
                let fg = if focused { color::SLATE } else { color::TEXT };
                let mut tx = r.left() + pad;
                if let Some(icon) = &card.icon {
                    let ir = Rect::from_min_size(pos2(tx, band.top() + self.px(34.0)), vec2(self.px(64.0), self.px(64.0)));
                    c.icon(icon, ir, if focused { color::SLATE } else { accent });
                    tx = ir.right() + self.px(20.0);
                }
                let title_font = self.display(size::HEADING);
                let title = c.fit(&card.title.to_uppercase(), &title_font, r.right() - pad - tx);
                let tr = c.text(pos2(tx, band.top() + self.px(40.0)), Align2::LEFT_TOP, title, title_font, fg, Fx::None);
                if let Some(sub) = &card.subtitle {
                    let sf = self.bold(size::BODY);
                    let st = c.fit(&sub.to_uppercase(), &sf, r.right() - pad - tx);
                    c.text(
                        pos2(tx, tr.bottom() + self.px(14.0)),
                        Align2::LEFT_TOP,
                        st,
                        sf,
                        if focused { color::SLATE } else { color::TEXT_DIM },
                        Fx::None,
                    );
                }
                let mut y = band.bottom() + self.px(26.0);
                let inner_w = r.width() - pad * 2.0;
                for line in card.lines.iter().take(4) {
                    let rich = self.rich(line, self.ui(30.0), color::TEXT, Some(inner_w));
                    c.paint_rich(&rich, pos2(r.left() + pad, y), inner_w, Align::Min, Fx::None);
                    y += rich.size.y + self.px(10.0);
                }
                if !card.lines.is_empty() {
                    y += self.px(12.0);
                }
                for (label, v) in card.bars.iter().take(5) {
                    let lf = self.bold(size::BODY);
                    let lr =
                        c.text(pos2(r.left() + pad, y), Align2::LEFT_TOP, label.to_uppercase(), lf, color::TEXT_DIM, Fx::None);
                    let bar =
                        Rect::from_min_size(pos2(r.left() + pad, lr.bottom() + self.px(12.0)), vec2(inner_w, self.px(14.0)));
                    c.bar(bar, *v, accent);
                    y = bar.bottom() + self.px(20.0);
                }
                if !card.enabled {
                    let lr = Rect::from_center_size(
                        pos2(r.center().x, r.bottom() - self.px(60.0)),
                        vec2(self.px(40.0), self.px(40.0)),
                    );
                    c.icon("lock", lr, color::TEXT);
                }
                c.outline(r, 20.0, 2.0, if focused { color::ACCENT } else { Color32::from_white_alpha(28) });
            });
        }
    }

    // ---- the pause map ----------------------------------------------------------------------

    /// The legend (right) and the description of the blip under the cursor (bottom left).
    fn map_extras(&self, mv: &MapView, top: f32) {
        if mv.legend {
            let mut kinds: Vec<(BlipIcon, Color32, String)> = Vec::new();
            for b in &mv.blips {
                let key = std::mem::discriminant(&b.icon);
                if !kinds.iter().any(|(k, _, _)| std::mem::discriminant(k) == key) {
                    kinds.push((b.icon, linear(b.color), crate::blips::blip_name(b.icon).to_string()));
                }
            }
            let w = self.px(400.0);
            let row = self.px(56.0);
            let pad = self.px(22.0);
            let shown = kinds.len().min(7);
            let title_font = self.bold(size::LABEL);
            let rect =
                Rect::from_min_size(pos2(self.safe.right() - w, top), vec2(w, pad * 2.0 + self.px(46.0) + row * shown as f32));
            self.backplate(rect, 14.0);
            self.text(
                pos2(rect.left() + pad, rect.top() + pad),
                Align2::LEFT_TOP,
                "LEGEND",
                title_font,
                color::TEXT_DIM,
                Fx::None,
            );
            for (i, (icon, col, name)) in kinds.iter().take(shown).enumerate() {
                let y = rect.top() + pad + self.px(46.0) + row * i as f32 + row * 0.5;
                self.blip(*icon, pos2(rect.left() + pad + self.px(18.0), y), self.px(34.0), *col);
                self.text(
                    pos2(rect.left() + pad + self.px(52.0), y),
                    Align2::LEFT_CENTER,
                    name.to_uppercase(),
                    self.bold(size::BODY),
                    color::TEXT,
                    Fx::None,
                );
            }
        }
        if let Some(sel) = &mv.selected {
            let w = self.px(620.0);
            let pad = self.px(24.0);
            let rich = self.rich(sel, self.ui(30.0), color::TEXT, Some(w - pad * 2.0));
            let h = rich.size.y + pad * 1.7;
            let rect = Rect::from_min_size(pos2(self.safe.left(), self.safe.bottom() - h), vec2(w, h));
            self.backplate(rect, 14.0);
            let bar = Rect::from_min_size(rect.min, vec2(self.px(6.0), h));
            let e = self.px(6.0) as u8;
            self.p.rect_filled(bar, egui::CornerRadius { nw: e, sw: e, ..Default::default() }, color::ACCENT);
            self.paint_rich(
                &rich,
                pos2(rect.left() + pad + self.px(4.0), rect.top() + pad * 0.85),
                w - pad * 2.0,
                Align::Min,
                Fx::None,
            );
        }
    }

    // ---- text, loading, results -------------------------------------------------------------

    fn text_card(&self, t: &TextCard, top: f32) {
        let max_w = self.px(1180.0).min(self.safe.width());
        let pad = self.px(40.0);
        let heading_font = self.display(size::HEADING);
        let heading = t.heading.to_uppercase();
        let lines: Vec<_> =
            t.lines.iter().take(6).map(|l| self.rich(l, self.ui(30.0), color::TEXT, Some(max_w - pad * 2.0))).collect();
        // As wide as the words need, within reason.
        let content = lines.iter().map(|l| l.size.x).fold(self.text_width(&heading, &heading_font), f32::max);
        let w = (content + pad * 2.0).clamp(self.px(720.0).min(max_w), max_w);
        let inner = w - pad * 2.0;
        let (ht, hb) = self.cap(&heading_font);
        let body_h: f32 = lines.iter().map(|l| l.size.y + self.px(16.0)).sum();
        let h = pad * 2.0 + (hb - ht) + self.px(34.0) + body_h;
        let rect = Rect::from_min_size(pos2(self.safe.left(), top), vec2(w, h));
        self.backplate(rect, 18.0);
        let r =
            self.text(pos2(rect.left() + pad, rect.top() + pad), Align2::LEFT_TOP, heading, heading_font, color::TEXT, Fx::None);
        let mut y = r.bottom() + self.px(34.0);
        for l in &lines {
            self.paint_rich(l, pos2(rect.left() + pad, y - l.cap.0), inner, Align::Min, Fx::None);
            y += l.size.y + self.px(16.0);
        }
    }

    fn loading(&self, l: &LoadingCard, backdrop: Backdrop) {
        if matches!(backdrop, Backdrop::Clear) {
            // Over the world (a picture the game made in the engine): only a scrim under the
            // words.
            let s = self.screen;
            let fade = Rect::from_min_max(pos2(s.left(), s.top() + s.height() * 0.5), s.max);
            self.scrim(fade, Color32::TRANSPARENT, Color32::from_black_alpha(215));
        } else {
            self.sunset_art();
        }
        let x = self.safe.left();
        let tip_w = self.px(980.0).min(self.safe.width() * 0.7);
        let pad = self.px(24.0);
        // The word before the tip ("TIP" unless the card names another; none when empty).
        let label = l.tag.as_deref().unwrap_or("TIP");
        let tag_w = if label.is_empty() { 0.0 } else { self.px(80.0) };
        let tip = (!l.tip.is_empty()).then(|| self.rich(&l.tip, self.ui(30.0), color::TEXT, Some(tip_w - pad * 2.0 - tag_w)));
        let tip_h = tip.as_ref().map_or(0.0, |t| t.size.y + pad * 1.7);
        let bar_y = self.safe.bottom() - self.px(10.0);
        let mut y = bar_y - self.px(40.0);
        if let Some(t) = &tip {
            let rect = Rect::from_min_size(pos2(x, y - tip_h), vec2(tip_w, tip_h));
            self.backplate(rect, 14.0);
            let at = pos2(rect.left() + pad, rect.top() + pad * 0.85);
            let tag = if label.is_empty() {
                Rect::from_min_size(at - vec2(self.px(22.0), 0.0), vec2(0.0, 0.0))
            } else {
                self.text(at, Align2::LEFT_TOP, label, self.bold(size::BODY), color::ACCENT, Fx::None)
            };
            self.paint_rich(
                t,
                pos2(tag.right() + self.px(22.0), rect.top() + pad * 0.85 - t.cap.0 + tag.top() - rect.top() - pad * 0.85),
                tip_w - pad * 2.0 - tag_w,
                Align::Min,
                Fx::None,
            );
            y = rect.top() - self.px(34.0);
        }
        self.text(pos2(x, y), Align2::LEFT_BOTTOM, l.heading.to_uppercase(), self.display(96.0), color::TEXT, Fx::Shadow);
        if l.progress < 0.0 {
            // (No bar: a card that is not waiting on anything, so no spinner either.)
            return;
        }
        let bar = Rect::from_min_size(pos2(x, bar_y - self.px(8.0)), vec2(self.safe.width(), self.px(8.0)));
        self.accent_bar(bar, l.progress);
        // Spinner, bottom right: an arc turning with UI time.
        let c = pos2(self.safe.right() - self.px(30.0), bar_y - self.px(70.0));
        let a = self.time * 4.0;
        self.ring(c, self.px(22.0), self.px(5.0), Color32::from_white_alpha(30));
        self.arc(c, self.px(22.0), a, a + 1.9, self.px(5.0), color::ACCENT);
        self.text(
            pos2(c.x - self.px(40.0), c.y),
            Align2::RIGHT_CENTER,
            "LOADING",
            self.bold(size::BODY),
            color::TEXT_DIM,
            Fx::None,
        );
    }

    /// The loading card's art: a striped sunset sun over a skyline, all from shapes.
    fn sunset_art(&self) {
        let s = self.screen;
        let horizon = s.top() + s.height() * 0.66;
        let sun_c = pos2(s.left() + s.width() * 0.68, horizon);
        let r = s.height() * 0.36;
        // Glow behind the sun.
        self.glow(
            Rect::from_center_size(sun_c, vec2(r * 2.4, r * 2.4)),
            r * 1.2 / self.u,
            color::ACCENT_2.gamma_multiply(0.16),
            140.0,
        );
        // The disc in bands: solid on top, cut below by gaps that widen toward the horizon.
        // Each band is one feathered polygon, so no seams show inside it.
        let gaps: Vec<(f32, f32)> =
            (0..6).map(|i| (0.5 + i as f32 * 0.085, 0.5 + i as f32 * 0.085 + 0.012 + 0.008 * i as f32)).collect();
        let mut t0 = 0.0;
        let y_at = |t: f32| sun_c.y - r + t * r;
        for (g0, g1) in gaps.iter().copied().chain([(1.0, 1.0)]) {
            let (ya, yb) = (y_at(t0), y_at(g0));
            if yb > ya {
                // Sample the rim evenly by angle so the top of the disc stays round.
                let steps = 24;
                let ang = |y: f32| ((sun_c.y - y) / r).clamp(-1.0, 1.0).acos();
                let (aa, ab) = (ang(ya), ang(yb));
                let rim = |k: usize| {
                    let a = aa + (ab - aa) * k as f32 / steps as f32;
                    (r * a.sin(), sun_c.y - r * a.cos())
                };
                let mut pts = Vec::with_capacity(steps * 2 + 2);
                for k in 0..=steps {
                    let (x, y) = rim(k);
                    pts.push(pos2(sun_c.x + x, y));
                }
                for k in (0..=steps).rev() {
                    let (x, y) = rim(k);
                    pts.push(pos2(sun_c.x - x, y));
                }
                let top = sun_c.y - r;
                self.convex_fill(&pts, |p| {
                    let t = ((p.y - top) / r).clamp(0.0, 1.0);
                    Color32::from_rgb(0xff, 0xc8, 0x6a).lerp_to_gamma(color::ACCENT_2, t).gamma_multiply(0.9)
                });
            }
            t0 = g1;
        }
        // Skyline: deterministic towers along the horizon.
        let mut x = s.left();
        let mut i = 0u32;
        while x < s.right() {
            let hsh = i.wrapping_mul(2654435761).rotate_left(13) ^ 0x9e37_79b9;
            let w = self.px(40.0 + (hsh % 70) as f32);
            let h = self.px(40.0 + ((hsh >> 8) % 220) as f32) * if (hsh >> 20).is_multiple_of(5) { 1.6 } else { 1.0 };
            let b = Rect::from_min_max(pos2(x, horizon - h), pos2(x + w, s.bottom()));
            self.p.rect_filled(b, 0.0, Color32::from_rgb(0x0b, 0x0d, 0x12));
            // A few lit windows.
            let cols = (w / self.px(14.0)) as i32;
            let rows = (h / self.px(18.0)) as i32;
            for cy in 0..rows.min(14) {
                for cx in 0..cols {
                    let k = hsh.wrapping_add((cy * 31 + cx * 7) as u32).wrapping_mul(747796405);
                    if k.is_multiple_of(9) {
                        let wr = Rect::from_min_size(
                            pos2(
                                b.left() + self.px(5.0) + cx as f32 * self.px(14.0),
                                b.top() + self.px(8.0) + cy as f32 * self.px(18.0),
                            ),
                            vec2(self.px(5.0), self.px(8.0)),
                        );
                        self.p.rect_filled(wr, 0.0, color::WARNING.gamma_multiply(0.5));
                    }
                }
            }
            x += w + self.px(2.0 + (hsh >> 12) as f32 % 10.0);
            i += 1;
        }
        // Ground fade so text at the bottom sits on dark.
        let fade = Rect::from_min_max(pos2(s.left(), horizon - s.height() * 0.1), s.max);
        self.scrim(fade, Color32::TRANSPARENT, Color32::from_black_alpha(200));
    }

    fn results(&self, res: &Results) {
        let w = self.px(980.0);
        let cx = self.screen.center().x;
        let mut y = self.safe.top() + self.px(20.0);
        if let Some(m) = res.medal {
            self.medal(m, pos2(cx, y + self.px(70.0)), self.px(66.0));
            y += self.px(196.0);
        }
        let font = self.display(104.0);
        let galley = self.galley(res.heading.to_uppercase(), font.clone(), color::TEXT);
        let (ct, cb) = self.cap(&font);
        let pos = pos2(cx - galley.size().x * 0.5, y - ct);
        self.gradient_galley(pos, &galley, Color32::from_rgb(0xff, 0xe4, 0x9a), Color32::from_rgb(0xf2, 0xa5, 0x1e), Fx::Outline);
        y += cb - ct + self.px(40.0);
        let row_h = self.px(66.0);
        let rows = res.rows.len().min(5);
        if rows > 0 {
            let rect = Rect::from_min_size(pos2(cx - w * 0.5, y), vec2(w, row_h * rows as f32 + self.px(20.0)));
            self.backplate(rect, 16.0);
            for (i, (label, value)) in res.rows.iter().take(5).enumerate() {
                let ry = rect.top() + self.px(10.0) + row_h * (i as f32 + 0.5);
                if i > 0 {
                    let ly = rect.top() + self.px(10.0) + row_h * i as f32;
                    self.line(
                        pos2(rect.left() + self.px(30.0), ly),
                        pos2(rect.right() - self.px(30.0), ly),
                        1.0,
                        Color32::from_white_alpha(22),
                    );
                }
                self.text(
                    pos2(rect.left() + self.px(36.0), ry),
                    Align2::LEFT_CENTER,
                    label.to_uppercase(),
                    self.bold(size::ITEM),
                    color::TEXT_DIM,
                    Fx::None,
                );
                self.text(
                    pos2(rect.right() - self.px(36.0), ry),
                    Align2::RIGHT_CENTER,
                    value,
                    self.display(size::HUD_SMALL_NUMBER),
                    color::TEXT,
                    Fx::None,
                );
            }
            y = rect.bottom() + self.px(30.0);
        }
        if let Some(cash) = res.cash {
            let (sign, col) = if cash >= 0 { ("+", color::POSITIVE) } else { ("", color::DANGER) };
            let label = self.text(
                pos2(cx, y + self.px(6.0)),
                Align2::CENTER_TOP,
                "CASH",
                self.bold(size::LABEL),
                color::TEXT_DIM,
                Fx::Shadow,
            );
            self.text(
                pos2(cx, label.bottom() + self.px(16.0)),
                Align2::CENTER_TOP,
                format!("{sign}{}", money(cash)),
                self.display(76.0),
                col,
                Fx::Outline,
            );
        }
    }

    /// A medal: a ribbon behind a metal disc with a star.
    fn medal(&self, m: Medal, c: Pos2, r: f32) {
        let metal = match m {
            Medal::Gold => color::GOLD,
            Medal::Silver => color::SILVER,
            Medal::Bronze => color::BRONZE,
        };
        // Two ribbon tails hanging behind the disc, each with a V notch (two convex halves).
        let tail = |x0: f32, x1: f32, slant: f32, col: Color32| {
            let (top, bottom) = (c.y + r * 0.2, c.y + r * 1.45);
            let xm = (x0 + x1) * 0.5;
            let notch = bottom - r * 0.28;
            self.polygon(vec![pos2(x0, top), pos2(xm, top), pos2(xm + slant, notch), pos2(x0 + slant, bottom)], col);
            self.polygon(vec![pos2(xm, top), pos2(x1, top), pos2(x1 + slant, bottom), pos2(xm + slant, notch)], col);
        };
        tail(c.x - r * 0.78, c.x - r * 0.12, -r * 0.3, color::ACCENT);
        tail(c.x + r * 0.12, c.x + r * 0.78, r * 0.3, color::ACCENT_2);
        self.glow(Rect::from_center_size(c, vec2(r * 2.0, r * 2.0)), r / self.u, metal.gamma_multiply(0.5), 30.0);
        self.p.circle(c, r, metal, Stroke::new(self.px(4.0), Color32::from_black_alpha(140)));
        self.p.circle_stroke(c, r * 0.78, Stroke::new(self.px(3.0), Color32::from_white_alpha(120)));
        self.star(c, r * 0.55, Color32::from_white_alpha(235));
    }

    // ---- the title --------------------------------------------------------------------------

    fn title_screen(&self, s: &Screen, t: &TitleCard) {
        let sc = self.screen;
        if t.small_logo {
            // Attract: logo small in the top-left corner, caption under it.
            let bottom = self.logo(&s.title, pos2(self.safe.left(), self.safe.top()), 0.45, Align::Min);
            if let Some(cap) = &t.caption {
                self.spaced_text(
                    pos2(self.safe.left(), bottom + self.px(18.0)),
                    Align2::LEFT_TOP,
                    &cap.to_uppercase(),
                    self.bold(size::LABEL),
                    color::TEXT,
                    0.12,
                );
            }
        } else {
            // Legibility: dark falloff behind the logo and at the bottom.
            let top = Rect::from_min_size(sc.min, vec2(sc.width(), sc.height() * 0.55));
            self.scrim(top, Color32::from_black_alpha(130), Color32::TRANSPARENT);
            let bottom = Rect::from_min_max(pos2(sc.left(), sc.top() + sc.height() * 0.5), sc.max);
            self.scrim(bottom, Color32::TRANSPARENT, Color32::from_black_alpha(170));
            let (logo_top, logo_scale) =
                if t.menu.is_some() { (sc.top() + sc.height() * 0.08, 0.78) } else { (sc.top() + sc.height() * 0.2, 1.0) };
            let mut y = self.logo(&s.title, pos2(sc.center().x, logo_top), logo_scale, Align::Center);
            if let Some(cap) = &t.caption {
                let r = self.spaced_text(
                    pos2(sc.center().x, y + self.px(26.0)),
                    Align2::CENTER_TOP,
                    &cap.to_uppercase(),
                    self.bold(size::LABEL),
                    color::TEXT,
                    0.18,
                );
                y = r.bottom();
            }
            if let Some(m) = &t.menu {
                // Under the logo, but always clear of the hint bar.
                let h = self.menu_height(m);
                let (lo, hi) = (y + self.px(48.0), self.safe.bottom() - self.px(92.0) - h);
                let top = if hi > lo { lo + (hi - lo) * 0.35 } else { hi };
                self.menu_list(m, pos2(sc.center().x, top), self.px(600.0), true);
            } else if let Some(p) = &t.prompt {
                // Breathes once a second (UI time).
                let a = 0.55 + 0.45 * (self.time * std::f32::consts::TAU * 0.5).cos().abs();
                self.faded(a).spaced_text(
                    pos2(sc.center().x, sc.top() + sc.height() * 0.78),
                    Align2::CENTER_CENTER,
                    &p.to_uppercase(),
                    self.bold(size::ITEM),
                    color::TEXT,
                    0.22,
                );
            }
        }
    }

    /// The game's logo from its title: the last word big in the sunset gradient, the words
    /// before it spaced out above. `scale` 1 is the title screen's size. Returns the bottom.
    fn logo(&self, title: &str, at: Pos2, scale: f32, align: Align) -> f32 {
        let title = title.trim().to_uppercase();
        let (lead, hero) = match title.rsplit_once(' ') {
            Some((a, b)) => (Some(a.to_string()), b.to_string()),
            None => (None, title.clone()),
        };
        let hero_font = self.display(size::LOGO * scale);
        let hero_g = self.galley(hero, hero_font.clone(), color::ACCENT);
        let (ht, hb) = self.cap(&hero_font);
        let mut y = at.y;
        let anchor_x = |w: f32| match align {
            Align::Min => at.x,
            Align::Center => at.x - w * 0.5,
            Align::Max => at.x - w,
        };
        if let Some(lead) = lead {
            // Spaced out toward the hero word's width, like a real lockup: the size that fits
            // at a fixed spacing, capped so the lead never competes with the hero.
            let em = 0.14;
            let n = lead.chars().count().max(2) as f32;
            let per_px = self.text_width(&lead, &egui::FontId::new(100.0, self.fam.display.clone())) / 100.0;
            let fs = (hero_g.size().x / (per_px + em * (n - 1.0))).min(hero_font.size * 0.46);
            let lf = egui::FontId::new(fs, self.fam.display.clone());
            let g = self.spaced_galley(&lead, lf.clone(), color::TEXT, fs * em);
            let (lt, lb) = self.cap(&lf);
            // Letter spacing trails the last letter too: centre on the letters.
            let x = anchor_x(g.size().x - fs * em);
            self.paint_galley(pos2(x, y - lt), &g, color::TEXT, Fx::Shadow);
            y += (lb - lt) + self.px(20.0 * scale);
        }
        let x = anchor_x(hero_g.size().x);
        self.gradient_galley(pos2(x, y - ht), &hero_g, Color32::from_rgb(0xff, 0x9a, 0x3d), color::ACCENT_2, Fx::Outline);
        y += hb - ht;
        // A thin accent rule under the lockup.
        let rule_w = hero_g.size().x * 0.6;
        let rule =
            Rect::from_min_size(pos2(anchor_x(rule_w), y + self.px(20.0 * scale)), vec2(rule_w, self.px(5.0 * scale).max(2.0)));
        self.accent(rule, 2.5 * scale);
        rule.bottom()
    }

    fn spaced_galley(&self, text: &str, font: egui::FontId, col: Color32, spacing: f32) -> std::sync::Arc<egui::Galley> {
        let mut job = LayoutJob::simple_singleline(text.to_string(), font, col);
        if let Some(s) = job.sections.first_mut() {
            s.format.extra_letter_spacing = spacing;
        }
        self.p.layout_job(job)
    }

    /// Letter-spaced text placed by its capitals like [`Cx::text`]; `em` is the extra spacing
    /// as a fraction of the font size.
    fn spaced_text(&self, pos: Pos2, align: Align2, text: &str, font: egui::FontId, col: Color32, em: f32) -> Rect {
        let g = self.spaced_galley(text, font.clone(), col, font.size * em);
        self.place(pos, align, &g, &font, col, Fx::Shadow)
    }
}
