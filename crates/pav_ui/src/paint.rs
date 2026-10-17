//! The drawing context every widget gets, and the visual language's primitives: backplates,
//! accent fills with glow, bars, arcs, text with shadows or outlines, and scaled groups (the
//! focused element's 1.05 and banners' scale-in). Everything is laid out in 1080p reference
//! pixels and scaled by [`Cx::px`].

use std::cell::RefCell;
use std::sync::Arc;

use egui::emath::TSTransform;
use egui::epaint::{CircleShape, PathShape, PathStroke, RectShape, TextShape};
use egui::{
    Align, Align2, Color32, CornerRadius, FontFamily, FontId, Galley, LayerId, Mesh, Painter, Pos2, Rect, Shape, Stroke,
    StrokeKind, Vec2, pos2, vec2,
};
use pav_core::present::KeyMap;

use crate::glyphs::GlyphSet;
use crate::theme::{self, color};

/// Something the pointer can land on: menu items, cards, tiles, wheel slots, tabs, the gear.
#[derive(Clone, Debug)]
pub(crate) struct Hit {
    pub area: HitArea,
    pub id: String,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum HitArea {
    Rect(Rect),
    /// A ring segment: centre, radii and compass angles (0 = up, clockwise).
    Sector {
        center: Pos2,
        r0: f32,
        r1: f32,
        a0: f32,
        a1: f32,
    },
}

impl HitArea {
    pub fn contains(&self, p: Pos2) -> bool {
        match *self {
            HitArea::Rect(r) => r.contains(p),
            HitArea::Sector { center, r0, r1, a0, a1 } => {
                let d = p - center;
                let r = d.length();
                if r < r0 || r > r1 {
                    return false;
                }
                let a = compass(d);
                let span = (a1 - a0).rem_euclid(std::f32::consts::TAU);
                (a - a0).rem_euclid(std::f32::consts::TAU) <= span
            }
        }
    }
}

/// The compass angle of a screen vector (0 = up, clockwise, 0..TAU).
pub(crate) fn compass(d: Vec2) -> f32 {
    d.x.atan2(-d.y).rem_euclid(std::f32::consts::TAU)
}

/// A screen direction from a compass angle (0 = up, clockwise).
pub(crate) fn dir(a: f32) -> Vec2 {
    vec2(a.sin(), -a.cos())
}

/// How text stands out over the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fx {
    /// On a backplate: plain.
    None,
    /// Over the world: a 2 px drop shadow.
    Shadow,
    /// Big numbers over busy backgrounds: a dark outline plus the shadow.
    Outline,
}

#[derive(Clone)]
pub(crate) struct Families {
    pub display: FontFamily,
    pub ui: FontFamily,
    pub bold: FontFamily,
}

impl Families {
    pub fn new(ready: bool) -> Self {
        if ready {
            Self {
                display: FontFamily::Name(theme::FAMILY_DISPLAY.into()),
                ui: FontFamily::Name(theme::FAMILY_UI.into()),
                bold: FontFamily::Name(theme::FAMILY_UI_BOLD.into()),
            }
        } else {
            // The host has not installed the fonts yet (they arrive next frame): draw with its own.
            Self { display: FontFamily::Proportional, ui: FontFamily::Proportional, bold: FontFamily::Proportional }
        }
    }
}

/// The drawing context: painter, scale, safe area, UI time, glyph set and pointer targets.
#[derive(Clone)]
pub(crate) struct Cx<'a> {
    pub ctx: egui::Context,
    pub p: Painter,
    pub layer: LayerId,
    /// Points per 1080p reference pixel (screen height / 1080 x the model's scale).
    pub u: f32,
    pub screen: Rect,
    /// The screen inset by the model's safe margin on each side.
    pub safe: Rect,
    /// UI seconds (`UiModel::time`): every animation keys off it.
    pub time: f32,
    pub glyphs: GlyphSet,
    /// The key map keycaps are labelled from.
    pub keys: &'a KeyMap,
    pub fam: Families,
    pub hits: &'a RefCell<Vec<Hit>>,
}

impl Cx<'_> {
    /// Reference pixels to points.
    pub fn px(&self, v: f32) -> f32 {
        v * self.u
    }

    pub fn display(&self, size: f32) -> FontId {
        FontId::new(self.px(size), self.fam.display.clone())
    }

    pub fn ui(&self, size: f32) -> FontId {
        FontId::new(self.px(size), self.fam.ui.clone())
    }

    pub fn bold(&self, size: f32) -> FontId {
        FontId::new(self.px(size), self.fam.bold.clone())
    }

    /// The same context with everything drawn at `opacity` (multiplied).
    pub fn faded(&self, opacity: f32) -> Self {
        let mut c = self.clone();
        c.p.multiply_opacity(opacity.clamp(0.0, 1.0));
        c
    }

    pub fn hit(&self, area: HitArea, id: impl Into<String>) {
        self.hits.borrow_mut().push(Hit { area, id: id.into() });
    }

    // ---- groups -----------------------------------------------------------------------------

    /// Draws `f`, then scales what it drew by `s` about `center` (focus pop, banner scale-in).
    pub fn scaled<R>(&self, center: Pos2, s: f32, f: impl FnOnce(&Self) -> R) -> R {
        let start = self.ctx.graphics_mut(|g| g.entry(self.layer).next_idx());
        let r = f(self);
        if (s - 1.0).abs() > 1e-4 {
            let end = self.ctx.graphics_mut(|g| g.entry(self.layer).next_idx());
            let t = TSTransform::new(center.to_vec2() * (1.0 - s), s);
            self.ctx.graphics_mut(|g| g.entry(self.layer).transform_range(start, end, t));
        }
        r
    }

    /// Draws `f` moved by `offset` (slide-ins).
    pub fn moved<R>(&self, offset: Vec2, f: impl FnOnce(&Self) -> R) -> R {
        let start = self.ctx.graphics_mut(|g| g.entry(self.layer).next_idx());
        let r = f(self);
        if offset != Vec2::ZERO {
            let end = self.ctx.graphics_mut(|g| g.entry(self.layer).next_idx());
            self.ctx.graphics_mut(|g| g.entry(self.layer).transform_range(start, end, TSTransform::from_translation(offset)));
        }
        r
    }

    // ---- plates and fills -------------------------------------------------------------------

    pub fn backplate(&self, rect: Rect, radius: f32) {
        self.p.rect_filled(rect, cr(self.px(radius)), color::BACKPLATE);
    }

    pub fn fill(&self, rect: Rect, radius: f32, color: Color32) {
        self.p.rect_filled(rect, cr(self.px(radius)), color);
    }

    pub fn outline(&self, rect: Rect, radius: f32, width: f32, color: Color32) {
        self.p.rect_stroke(rect, cr(self.px(radius)), Stroke::new(self.px(width).max(1.0), color), StrokeKind::Inside);
    }

    /// A soft glow (or shadow) around `rect`.
    pub fn glow(&self, rect: Rect, radius: f32, color: Color32, spread: f32) {
        let s = self.px(spread);
        let mut shape = RectShape::filled(rect.expand(s * 0.25), cr(self.px(radius) + s * 0.25), color);
        shape.blur_width = s * 1.5;
        self.p.add(shape);
    }

    /// The accent fill (sunset gradient, left to right) for focused elements.
    pub fn accent(&self, rect: Rect, radius: f32) {
        self.gradient_rect(rect, radius, color::ACCENT, color::ACCENT_2);
    }

    /// A rounded rectangle filled with a horizontal gradient, anti-aliased.
    pub fn gradient_rect(&self, rect: Rect, radius: f32, left: Color32, right: Color32) {
        let pts = rounded_rect_points(rect, self.px(radius).min(rect.height() * 0.5).min(rect.width() * 0.5));
        let w = rect.width().max(1.0);
        self.convex_fill(&pts, |p| left.lerp_to_gamma(right, ((p.x - rect.left()) / w).clamp(0.0, 1.0)));
    }

    /// A rounded rectangle filled with a vertical gradient, anti-aliased.
    pub fn vgradient_rect(&self, rect: Rect, radius: f32, top: Color32, bottom: Color32) {
        let pts = rounded_rect_points(rect, self.px(radius).min(rect.height() * 0.5).min(rect.width() * 0.5));
        let h = rect.height().max(1.0);
        self.convex_fill(&pts, |p| top.lerp_to_gamma(bottom, ((p.y - rect.top()) / h).clamp(0.0, 1.0)));
    }

    /// Fills a convex polygon (clockwise or not) with a per-vertex colour, feathered by one
    /// physical pixel so the edge is anti-aliased like egui's own shapes.
    pub fn convex_fill(&self, pts: &[Pos2], color: impl Fn(Pos2) -> Color32) {
        if pts.len() < 3 {
            return;
        }
        let feather = 1.0 / self.p.pixels_per_point();
        let n = pts.len();
        let c = pts.iter().fold(Vec2::ZERO, |a, p| a + p.to_vec2()) / n as f32;
        let mut mesh = Mesh::default();
        // Inner ring at half a feather inside the edge, outer ring half outside with alpha 0.
        for p in pts {
            let out = (p.to_vec2() - c).normalized();
            let inner = *p - out * feather * 0.5;
            let outer = *p + out * feather * 0.5;
            let col = color(*p);
            mesh.colored_vertex(inner, col);
            mesh.colored_vertex(outer, Color32::TRANSPARENT);
        }
        for i in 0..n as u32 {
            let j = (i + 1) % n as u32;
            if i >= 1 && i + 1 < n as u32 {
                mesh.add_triangle(0, 2 * i, 2 * j);
            }
            mesh.add_triangle(2 * i, 2 * i + 1, 2 * j);
            mesh.add_triangle(2 * j, 2 * i + 1, 2 * j + 1);
        }
        self.p.add(Shape::mesh(mesh));
    }

    /// A horizontal band fading out at both ends (banner backdrops).
    pub fn band(&self, rect: Rect, color: Color32, fade: f32) {
        let fade = fade.min(rect.width() * 0.5);
        let mut mesh = Mesh::default();
        let xs = [rect.left(), rect.left() + fade, rect.right() - fade, rect.right()];
        let cs = [Color32::TRANSPARENT, color, color, Color32::TRANSPARENT];
        for (x, c) in xs.iter().zip(cs) {
            mesh.colored_vertex(pos2(*x, rect.top()), c);
            mesh.colored_vertex(pos2(*x, rect.bottom()), c);
        }
        for i in 0..3u32 {
            mesh.add_triangle(2 * i, 2 * i + 1, 2 * i + 2);
            mesh.add_triangle(2 * i + 1, 2 * i + 3, 2 * i + 2);
        }
        self.p.add(Shape::mesh(mesh));
    }

    /// A vertical gradient over `rect` (backdrops, legibility scrims).
    pub fn scrim(&self, rect: Rect, top: Color32, bottom: Color32) {
        let mut mesh = Mesh::default();
        mesh.colored_vertex(rect.left_top(), top);
        mesh.colored_vertex(rect.right_top(), top);
        mesh.colored_vertex(rect.left_bottom(), bottom);
        mesh.colored_vertex(rect.right_bottom(), bottom);
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(1, 3, 2);
        self.p.add(Shape::mesh(mesh));
    }

    /// A horizontal bar: a dark track and a fill for `value` (0..1).
    pub fn bar(&self, rect: Rect, value: f32, fill: Color32) {
        let r = rect.height() * 0.5;
        self.p.rect_filled(rect, cr(r), color::SLATE_2.gamma_multiply(0.85));
        let v = value.clamp(0.0, 1.0);
        if v > 0.0 {
            let w = (rect.width() * v).max(rect.height());
            self.p.rect_filled(Rect::from_min_size(rect.min, vec2(w, rect.height())), cr(r), fill);
        }
    }

    /// The same bar with the accent gradient as its fill.
    pub fn accent_bar(&self, rect: Rect, value: f32) {
        let r = rect.height() * 0.5;
        self.p.rect_filled(rect, cr(r), color::SLATE_2.gamma_multiply(0.85));
        let v = value.clamp(0.0, 1.0);
        if v > 0.0 {
            let w = (rect.width() * v).max(rect.height());
            let fill = Rect::from_min_size(rect.min, vec2(w, rect.height()));
            let pts = rounded_rect_points(fill, r);
            self.convex_fill(&pts, |p| {
                color::ACCENT.lerp_to_gamma(color::ACCENT_2, ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0))
            });
        }
    }

    // ---- lines, circles, arcs ---------------------------------------------------------------

    pub fn circle(&self, center: Pos2, radius: f32, fill: Color32) {
        self.p.add(CircleShape::filled(center, radius, fill));
    }

    pub fn ring(&self, center: Pos2, radius: f32, width: f32, color: Color32) {
        self.p.add(CircleShape::stroke(center, radius, Stroke::new(width, color)));
    }

    pub fn line(&self, a: Pos2, b: Pos2, width: f32, color: Color32) {
        self.p.line_segment([a, b], Stroke::new(width, color));
    }

    pub fn polyline(&self, pts: Vec<Pos2>, width: f32, color: Color32) {
        if pts.len() >= 2 {
            self.p.add(PathShape::line(pts, PathStroke::new(width, color)));
        }
    }

    pub fn polygon(&self, pts: Vec<Pos2>, fill: Color32) {
        if pts.len() >= 3 {
            self.p.add(PathShape::convex_polygon(pts, fill, Stroke::NONE));
        }
    }

    pub fn polygon_outlined(&self, pts: Vec<Pos2>, fill: Color32, width: f32, outline: Color32) {
        if pts.len() >= 3 {
            self.p.add(PathShape::convex_polygon(pts, fill, Stroke::new(width, outline)));
        }
    }

    /// An arc from compass angle `a0` to `a1` (clockwise).
    pub fn arc(&self, center: Pos2, radius: f32, a0: f32, a1: f32, width: f32, color: Color32) {
        let n = (((a1 - a0).abs() * radius / 4.0).ceil() as usize).clamp(4, 128);
        let pts = (0..=n).map(|i| center + dir(a0 + (a1 - a0) * i as f32 / n as f32) * radius).collect();
        self.p.add(PathShape::line(pts, PathStroke::new(width, color)));
    }

    /// A ring segment between radii `r0` and `r1` and compass angles `a0..a1`, filled by a
    /// per-vertex colour and anti-aliased on every edge.
    pub fn sector(&self, center: Pos2, r0: f32, r1: f32, a0: f32, a1: f32, color: impl Fn(Pos2) -> Color32) {
        let f = 1.0 / self.p.pixels_per_point();
        let n = (((a1 - a0).abs() * r1 / 6.0).ceil() as usize).clamp(6, 96);
        let mut mesh = Mesh::default();
        // Columns of 4 vertices: inner feather, inner, outer, outer feather.
        let col = |m: &mut Mesh, a: f32, alpha: bool| {
            let d = dir(a);
            let (pi, po) = (center + d * r0, center + d * r1);
            let (ci, co) = if alpha { (color(pi), color(po)) } else { (Color32::TRANSPARENT, Color32::TRANSPARENT) };
            m.colored_vertex(center + d * (r0 - f * 0.5).max(0.0), Color32::TRANSPARENT);
            m.colored_vertex(center + d * (r0 + f * 0.5), ci);
            m.colored_vertex(center + d * (r1 - f * 0.5), co);
            m.colored_vertex(center + d * (r1 + f * 0.5), Color32::TRANSPARENT);
        };
        // Feathered ends: a column with alpha 0 just outside each end.
        let (ea0, ea1) = (a0 - f / r1.max(1.0), a1 + f / r1.max(1.0));
        col(&mut mesh, ea0, false);
        for i in 0..=n {
            col(&mut mesh, a0 + (a1 - a0) * i as f32 / n as f32, true);
        }
        col(&mut mesh, ea1, false);
        let cols = n as u32 + 3;
        for c in 0..cols - 1 {
            let (a, b) = (4 * c, 4 * (c + 1));
            for k in 0..3 {
                mesh.add_triangle(a + k, b + k, a + k + 1);
                mesh.add_triangle(a + k + 1, b + k, b + k + 1);
            }
        }
        self.p.add(Shape::mesh(mesh));
    }

    /// A star polygon outline with `points` tips (wanted stars, activity blips).
    pub fn star_points(&self, center: Pos2, r_out: f32, r_in: f32, points: usize) -> Vec<Pos2> {
        (0..points * 2)
            .map(|i| {
                let r = if i % 2 == 0 { r_out } else { r_in };
                center + dir(i as f32 * std::f32::consts::PI / points as f32) * r
            })
            .collect()
    }

    /// A filled star: one fan from its centre (stars are not convex, but every tip is seen
    /// from the centre), feathered on the rim like [`Cx::convex_fill`].
    pub fn star(&self, center: Pos2, r_out: f32, color: Color32) {
        let pts = self.star_points(center, r_out, r_out * 0.45, 5);
        let feather = 1.0 / self.p.pixels_per_point();
        let mut mesh = Mesh::default();
        mesh.colored_vertex(center, color);
        for p in &pts {
            let out = (*p - center).normalized();
            mesh.colored_vertex(*p - out * feather * 0.5, color);
            mesh.colored_vertex(*p + out * feather * 0.5, Color32::TRANSPARENT);
        }
        let n = pts.len() as u32;
        for i in 0..n {
            let j = (i + 1) % n;
            let (ai, ao, bi, bo) = (1 + 2 * i, 2 + 2 * i, 1 + 2 * j, 2 + 2 * j);
            mesh.add_triangle(0, ai, bi);
            mesh.add_triangle(ai, ao, bi);
            mesh.add_triangle(bi, ao, bo);
        }
        self.p.add(Shape::mesh(mesh));
    }

    // ---- text -------------------------------------------------------------------------------

    pub fn galley(&self, text: impl Into<String>, font: FontId, color: Color32) -> Arc<Galley> {
        self.p.layout_no_wrap(text.into(), font, color)
    }

    pub fn text_width(&self, text: &str, font: &FontId) -> f32 {
        self.galley(text, font.clone(), Color32::WHITE).size().x
    }

    /// Where a font's capitals sit in its row: top and bottom (the baseline) below the row top.
    pub fn cap(&self, font: &FontId) -> (f32, f32) {
        let g = self.galley("H", font.clone(), Color32::WHITE);
        match g.rows.first() {
            Some(row) if row.row.visuals.mesh_bounds.is_positive() => {
                let b = row.row.visuals.mesh_bounds;
                (row.pos.y + b.top(), row.pos.y + b.bottom())
            }
            _ => (font.size * 0.25, font.size * 0.95),
        }
    }

    /// Paints a galley with its text effect.
    pub fn paint_galley(&self, pos: Pos2, galley: &Arc<Galley>, color: Color32, fx: Fx) {
        let d = self.px(2.0).max(1.0);
        match fx {
            Fx::None => {}
            Fx::Shadow => {
                self.p
                    .add(TextShape::new(pos + vec2(d, d), galley.clone(), color::SHADOW).with_override_text_color(color::SHADOW));
            }
            Fx::Outline => {
                let o = self.px(1.6).max(1.0);
                let dark = Color32::from_rgba_unmultiplied(0, 0, 0, 170);
                self.p.add(TextShape::new(pos + vec2(d, d) * 1.2, galley.clone(), dark).with_override_text_color(dark));
                for (x, y) in
                    [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0), (-0.7, -0.7), (0.7, -0.7), (-0.7, 0.7), (0.7, 0.7)]
                {
                    self.p.add(TextShape::new(pos + vec2(x, y) * o, galley.clone(), dark).with_override_text_color(dark));
                }
            }
        }
        self.p.add(TextShape::new(pos, galley.clone(), color));
    }

    /// Text placed by its capitals: `align`'s vertical part puts the cap top (TOP), the cap
    /// middle (CENTER) or the baseline (BOTTOM) at `pos.y`. Returns the cap box.
    pub fn text(&self, pos: Pos2, align: Align2, text: impl Into<String>, font: FontId, color: Color32, fx: Fx) -> Rect {
        let galley = self.galley(text, font.clone(), color);
        self.place(pos, align, &galley, &font, color, fx)
    }

    /// Places a laid-out galley like [`Cx::text`].
    pub fn place(&self, pos: Pos2, align: Align2, galley: &Arc<Galley>, font: &FontId, color: Color32, fx: Fx) -> Rect {
        let (ct, cb) = self.cap(font);
        let w = galley.size().x;
        let x = match align.x() {
            Align::Min => pos.x,
            Align::Center => pos.x - w * 0.5,
            Align::Max => pos.x - w,
        };
        let cap_h = cb - ct;
        let top = match align.y() {
            Align::Min => pos.y,
            Align::Center => pos.y - cap_h * 0.5,
            Align::Max => pos.y - cap_h,
        };
        self.paint_galley(pos2(x, top - ct), galley, color, fx);
        Rect::from_min_size(pos2(x, top), vec2(w, cap_h))
    }

    /// Text filled with a vertical gradient (logo, banners). The glyph vertices of a copy of
    /// the galley are recoloured; it stays a text shape, so egui still maps its atlas UVs at
    /// tessellation. `pos` is the galley's top-left.
    pub fn gradient_galley(&self, pos: Pos2, galley: &Arc<Galley>, top: Color32, bottom: Color32, fx: Fx) {
        if fx != Fx::None {
            self.paint_galley_fx_only(pos, galley, fx);
        }
        let bounds = galley.mesh_bounds;
        let h = bounds.height().max(1.0);
        let mut g: Galley = (**galley).clone();
        for row in &mut g.rows {
            let y0 = row.pos.y;
            let r = Arc::make_mut(&mut row.row);
            for v in &mut r.visuals.mesh.vertices {
                let t = ((y0 + v.pos.y - bounds.top()) / h).clamp(0.0, 1.0);
                v.color = top.lerp_to_gamma(bottom, t);
            }
        }
        self.p.add(TextShape::new(pos, Arc::new(g), top));
    }

    /// Only the shadow or outline of a galley (under a gradient fill).
    fn paint_galley_fx_only(&self, pos: Pos2, galley: &Arc<Galley>, fx: Fx) {
        let d = self.px(2.0).max(1.0);
        let dark = Color32::from_rgba_unmultiplied(0, 0, 0, if fx == Fx::Outline { 170 } else { 200 });
        self.p.add(TextShape::new(pos + vec2(d, d) * 1.5, galley.clone(), dark).with_override_text_color(dark));
        if fx == Fx::Outline {
            let o = self.px(2.0).max(1.0);
            for (x, y) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0), (-0.7, -0.7), (0.7, -0.7), (-0.7, 0.7), (0.7, 0.7)] {
                self.p.add(TextShape::new(pos + vec2(x, y) * o, galley.clone(), dark).with_override_text_color(dark));
            }
        }
    }

    /// Shortens `text` with an ellipsis until it fits `width` in `font`.
    pub fn fit(&self, text: &str, font: &FontId, width: f32) -> String {
        if self.text_width(text, font) <= width {
            return text.to_string();
        }
        let chars: Vec<char> = text.chars().collect();
        let (mut lo, mut hi) = (0usize, chars.len());
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            let s: String = chars[..mid].iter().collect::<String>().trim_end().to_string() + "…";
            if self.text_width(&s, font) <= width { lo = mid } else { hi = mid - 1 }
        }
        chars[..lo].iter().collect::<String>().trim_end().to_string() + "…"
    }
}

/// A corner radius from points (egui stores whole points up to 255).
pub(crate) fn cr(r: f32) -> CornerRadius {
    CornerRadius::same(r.round().clamp(0.0, 255.0) as u8)
}

/// The outline of a rounded rectangle, clockwise from the top-left corner.
pub(crate) fn rounded_rect_points(rect: Rect, r: f32) -> Vec<Pos2> {
    let r = r.max(0.0);
    if r < 0.5 {
        return vec![rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()];
    }
    let seg = ((r * 0.6).ceil() as usize).clamp(3, 16);
    let mut pts = Vec::with_capacity(seg * 4 + 4);
    let corners = [
        (pos2(rect.left() + r, rect.top() + r), std::f32::consts::PI * 1.5),
        (pos2(rect.right() - r, rect.top() + r), 0.0),
        (pos2(rect.right() - r, rect.bottom() - r), std::f32::consts::PI * 0.5),
        (pos2(rect.left() + r, rect.bottom() - r), std::f32::consts::PI),
    ];
    for (c, a0) in corners {
        // Compass angles: the top-left corner sweeps from left (270°) to up (360°).
        for i in 0..=seg {
            let a = a0 + std::f32::consts::FRAC_PI_2 * i as f32 / seg as f32;
            pts.push(c + dir(a) * r);
        }
    }
    pts
}

/// Ease-out (cubic) on 0..1.
pub(crate) fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

/// 0..1 over `[start, start + len]`.
pub(crate) fn ramp(x: f32, start: f32, len: f32) -> f32 {
    ((x - start) / len.max(1e-4)).clamp(0.0, 1.0)
}

/// Thousands separators: 12480 -> "12,480".
pub(crate) fn grouped(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    if n < 0 { format!("-{out}") } else { out }
}

/// "$12,480" / "-$200".
pub(crate) fn money(n: i64) -> String {
    if n < 0 { format!("-${}", grouped(-n)) } else { format!("${}", grouped(n)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_reads_with_separators() {
        assert_eq!(money(12_480), "$12,480");
        assert_eq!(money(-250), "-$250");
        assert_eq!(money(1_000_000), "$1,000,000");
        assert_eq!(grouped(999), "999");
    }

    #[test]
    fn compass_bearings_go_clockwise_from_up() {
        assert!((compass(vec2(0.0, -1.0)) - 0.0).abs() < 1e-6);
        assert!((compass(vec2(1.0, 0.0)) - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        assert!((dir(std::f32::consts::PI) - vec2(0.0, 1.0)).length() < 1e-6);
        let ring = HitArea::Sector { center: Pos2::ZERO, r0: 1.0, r1: 2.0, a0: -0.4, a1: 0.4 };
        assert!(ring.contains(pos2(0.0, -1.5)), "straddles north");
        assert!(!ring.contains(pos2(0.0, 1.5)) && !ring.contains(pos2(0.0, -0.5)));
    }
}
