//! The map picture as a texture (cached by `MapImage::version`), the radar and the pause map.
//!
//! Angles are compass bearings: 0 = north (world -z, the picture's top), clockwise, so pi/2 is
//! east (+x). The radar turns with `Minimap::heading` (up the radar).

use egui::epaint::Vertex;
use egui::{Align2, Color32, Mesh, Pos2, Rect, Shape, Stroke, TextureHandle, TextureId, TextureOptions, Vec2, pos2, vec2};
use pav_core::present::{BlipIcon, MapImage, MapView, Minimap};

use crate::paint::{Cx, Fx, compass, dir, rounded_rect_points};
use crate::theme::{color, linear};

/// The map texture and how world positions land on it.
#[derive(Clone, Copy)]
pub(crate) struct MapTex {
    pub id: TextureId,
    /// World (x, z) of the picture's top-left corner and metres per picture pixel.
    origin: glam::Vec2,
    mpp: f32,
    /// The texture's size: the picture plus one transparent pixel of border each side, so
    /// clamped sampling outside the picture is transparent.
    tex: Vec2,
}

impl MapTex {
    /// Texture coordinates of a world point (x, z).
    pub fn uv(&self, x: f32, z: f32) -> Pos2 {
        let px = (x - self.origin.x) / self.mpp;
        let py = (z - self.origin.y) / self.mpp;
        pos2((px + 1.0) / self.tex.x, (py + 1.0) / self.tex.y)
    }
}

/// Keeps the map texture while `MapImage::version` (and the size) stays the same.
#[derive(Default)]
pub(crate) struct MapCache {
    tex: Option<(u64, [u32; 2], TextureHandle)>,
}

impl MapCache {
    pub fn texture(&mut self, ctx: &egui::Context, map: Option<&MapImage>) -> Option<MapTex> {
        let m = map.filter(|m| m.width > 0 && m.height > 0 && m.rgba.len() >= (m.width * m.height * 4) as usize)?;
        let key = (m.version, [m.width, m.height]);
        if self.tex.as_ref().is_none_or(|(v, s, _)| (*v, *s) != key) {
            let max = ctx.input(|i| i.max_texture_side).max(256);
            let image = map_image(m, max);
            let handle = ctx.load_texture("pav_ui.map", image, TextureOptions::LINEAR);
            self.tex = Some((m.version, [m.width, m.height], handle));
        }
        let (_, _, handle) = self.tex.as_ref()?;
        let [tw, th] = handle.size();
        // A picture shrunk to fit the GPU keeps its world size: scale metres per pixel.
        let k = m.width as f32 / (tw - 2).max(1) as f32;
        Some(MapTex { id: handle.id(), origin: m.origin, mpp: m.metres_per_px * k, tex: vec2(tw as f32, th as f32) })
    }
}

/// The picture with a transparent pixel of border, shrunk (nearest) to fit `max` texels.
fn map_image(m: &MapImage, max: usize) -> egui::ColorImage {
    let (w, h) = (m.width as usize, m.height as usize);
    let step = w.max(h).div_ceil(max.saturating_sub(2).max(1)).max(1);
    let (dw, dh) = (w.div_ceil(step), h.div_ceil(step));
    let (tw, th) = (dw + 2, dh + 2);
    let mut pixels = vec![Color32::TRANSPARENT; tw * th];
    for y in 0..dh {
        for x in 0..dw {
            let i = ((y * step).min(h - 1) * w + (x * step).min(w - 1)) * 4;
            let p = &m.rgba[i..i + 4];
            pixels[(y + 1) * tw + x + 1] = Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]);
        }
    }
    egui::ColorImage::new([tw, th], pixels)
}

/// Screen placement of the radar: world <-> screen.
struct Radar {
    center_screen: Pos2,
    center_world: glam::Vec2,
    heading: f32,
    /// Metres per point.
    mpp: f32,
}

impl Radar {
    fn to_screen(&self, w: glam::Vec2) -> Pos2 {
        let d = w - self.center_world;
        let (s, c) = self.heading.sin_cos();
        // Forward (up the radar) and right in world (x, z).
        let fwd = d.x * s - d.y * c;
        let right = d.x * c + d.y * s;
        self.center_screen + vec2(right, -fwd) / self.mpp
    }

    fn to_world(&self, p: Pos2) -> glam::Vec2 {
        let v = (p - self.center_screen) * self.mpp;
        let (fwd, right) = (-v.y, v.x);
        let (s, c) = self.heading.sin_cos();
        self.center_world + glam::Vec2::new(fwd * s + right * c, -fwd * c + right * s)
    }

    /// A world bearing as a screen bearing on the radar.
    fn bearing(&self, a: f32) -> f32 {
        a - self.heading
    }
}

/// Pulls `p` inside `r` along the ray from its centre.
fn clamp_to(r: Rect, p: Pos2) -> (Pos2, bool) {
    if r.contains(p) {
        return (p, false);
    }
    let c = r.center();
    let d = p - c;
    let k = (d.x.abs() / (r.width() * 0.5).max(1e-3)).max(d.y.abs() / (r.height() * 0.5).max(1e-3));
    (c + d / k.max(1.0), true)
}

impl Cx<'_> {
    /// A textured convex fan: each outline point samples the map at `uv(point)`; feathered.
    fn map_fan(&self, pts: &[Pos2], tex: TextureId, tint: Color32, uv: impl Fn(Pos2) -> Pos2) {
        let n = pts.len();
        if n < 3 {
            return;
        }
        let feather = 1.0 / self.p.pixels_per_point();
        let c = pts.iter().fold(Vec2::ZERO, |a, p| a + p.to_vec2()) / n as f32;
        let mut mesh = Mesh::with_texture(tex);
        for p in pts {
            let out = (p.to_vec2() - c).normalized();
            let inner = *p - out * feather * 0.5;
            let outer = *p + out * feather * 0.5;
            mesh.vertices.push(Vertex { pos: inner, uv: uv(inner), color: tint });
            mesh.vertices.push(Vertex { pos: outer, uv: uv(outer), color: Color32::TRANSPARENT });
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

    /// The radar in `rect`: map, route, cones, circles, blips, the player and north.
    pub fn minimap(&self, mm: &Minimap, rect: Rect, tex: Option<MapTex>) {
        let radius = self.px(18.0);
        let pts = rounded_rect_points(rect, radius);
        // Base: deep slate (water and the unknown beyond the picture).
        self.glow(rect, 18.0, Color32::from_black_alpha(110), 14.0);
        self.fill(rect, 18.0, Color32::from_rgba_unmultiplied(0x0b, 0x0e, 0x13, 225));
        let radar = Radar {
            center_screen: rect.center(),
            center_world: glam::Vec2::new(mm.center.x, mm.center.y),
            heading: mm.heading,
            mpp: mm.radius.max(1.0) / (rect.height() * 0.5),
        };
        if let Some(t) = tex {
            self.map_fan(&pts, t.id, Color32::from_white_alpha(235), |p| {
                let w = radar.to_world(p);
                t.uv(w.x, w.y)
            });
        } else {
            // No picture yet: a faint grid so it still reads as a radar.
            let step = self.px(40.0);
            let mut x = rect.left() + step;
            while x < rect.right() {
                self.line(pos2(x, rect.top()), pos2(x, rect.bottom()), 1.0, Color32::from_white_alpha(12));
                x += step;
            }
            let mut y = rect.top() + step;
            while y < rect.bottom() {
                self.line(pos2(rect.left(), y), pos2(rect.right(), y), 1.0, Color32::from_white_alpha(12));
                y += step;
            }
        }
        let clip = self.clone_clipped(rect.shrink(1.0));
        // Search circles and vision cones under the route and blips.
        for circle in &mm.circles {
            let c = radar.to_screen(glam::Vec2::new(circle.pos.x, circle.pos.y));
            let r = circle.radius / radar.mpp;
            let mut a = 1.0;
            if circle.flashing {
                a = 0.55 + 0.45 * (self.time * std::f32::consts::TAU * 1.5).sin();
            }
            let col = linear(circle.color);
            clip.circle(c, r, col.gamma_multiply(0.2 * a));
            clip.ring(c, r, self.px(2.0), col.gamma_multiply(0.85 * a));
        }
        for cone in &mm.cones {
            let c = radar.to_screen(glam::Vec2::new(cone.pos.x, cone.pos.y));
            let a = radar.bearing(cone.dir);
            let half = cone.angle.clamp(0.05, std::f32::consts::PI) * 0.5;
            let r = cone.range / radar.mpp;
            let col = linear(cone.color);
            let n = 16;
            let mut fan = vec![c];
            fan.extend((0..=n).map(|i| c + dir(a - half + 2.0 * half * i as f32 / n as f32) * r));
            clip.polygon(fan.clone(), col.gamma_multiply(0.3));
            fan.push(c);
            clip.polyline(fan, self.px(1.5), col.gamma_multiply(0.7));
        }
        // The GPS route: a dark casing under the accent line.
        if mm.route.len() >= 2 {
            let line: Vec<Pos2> = mm.route.iter().map(|p| radar.to_screen(glam::Vec2::new(p.x, p.y))).collect();
            clip.polyline(line.clone(), self.px(8.0), Color32::from_black_alpha(150));
            clip.polyline(line, self.px(5.0), color::ACCENT_2);
        }
        // Blips: inside, or pinned to the edge when they ask for it.
        let s = self.px(26.0);
        let inner = rect.shrink(s * 0.62);
        for b in &mm.blips {
            let p = radar.to_screen(glam::Vec2::new(b.pos.x, b.pos.y));
            let (q, outside) = clamp_to(inner, p);
            if outside && !b.clamp {
                continue;
            }
            let size =
                if matches!(b.icon, BlipIcon::Objective | BlipIcon::Waypoint | BlipIcon::Letter(_)) { s * 1.15 } else { s };
            let col = linear(b.color);
            if b.pulse {
                self.blip_pulse(q, size, col);
            }
            self.blip(b.icon, q, if outside { size * 0.85 } else { size }, col);
            self.blip_height(q, size, b.height);
        }
        // North, on the rim.
        let north = rect.center() + dir(radar.bearing(0.0)) * rect.width();
        let (n, _) = clamp_to(rect.shrink(self.px(16.0)), north);
        self.p.circle(n, self.px(13.0), color::SLATE, Stroke::new(self.px(1.5), Color32::from_white_alpha(90)));
        self.text(n, Align2::CENTER_CENTER, "N", self.bold(18.0), color::TEXT, Fx::None);
        // The player: a white arrow with a dark rim.
        let (pp, _) = clamp_to(inner, radar.to_screen(glam::Vec2::new(mm.player.x, mm.player.y)));
        self.player_arrow(pp, radar.bearing(mm.player_heading), self.px(30.0));
        // Rim.
        self.outline(rect, 18.0, 2.0, Color32::from_white_alpha(46));
    }

    fn clone_clipped(&self, rect: Rect) -> Self {
        let mut c = self.clone();
        c.p = c.p.with_clip_rect(rect);
        c
    }

    /// The player's arrow pointing along compass bearing `a`.
    pub fn player_arrow(&self, c: Pos2, a: f32, s: f32) {
        let f = dir(a);
        let r = vec2(-f.y, f.x);
        let tip = c + f * s * 0.55;
        let back_l = c - f * s * 0.38 - r * s * 0.36;
        let back_r = c - f * s * 0.38 + r * s * 0.36;
        let notch = c - f * s * 0.16;
        let rim = self.px(2.0).max(1.0);
        // Two convex halves (the arrow has a notch).
        for side in [back_l, back_r] {
            self.polygon_outlined(vec![tip, side, notch], color::TEXT, rim, Color32::from_black_alpha(200));
        }
        self.polygon(vec![tip, back_l, notch], color::TEXT);
        self.polygon(vec![tip, notch, back_r], color::TEXT);
    }

    /// Health, armour and the ability bar under the radar.
    pub fn vitals(&self, v: &pav_core::present::Vitals, rect: Rect) {
        self.backplate(rect, 8.0);
        let pad = self.px(6.0);
        let inner = rect.shrink(pad);
        let gap = self.px(6.0);
        let w = inner.width() - gap * 2.0;
        let widths = [w * 0.5, w * 0.3, w * 0.2];
        let low = v.health < 0.25;
        let pulse = if low { 0.6 + 0.4 * (self.time * 7.0).sin().abs() } else { 1.0 };
        let fills = [
            if low { color::DANGER.gamma_multiply(pulse) } else { color::POSITIVE },
            Color32::from_rgb(0x5c, 0xa0, 0xff),
            linear(v.ability_color),
        ];
        let values = [v.health, v.armor, v.ability];
        let mut x = inner.left();
        for i in 0..3 {
            let r = Rect::from_min_size(pos2(x, inner.top()), vec2(widths[i], inner.height()));
            self.bar(r, values[i], fills[i]);
            x += widths[i] + gap;
        }
    }

    /// The pause map filling `area`: picture, route, blips, cursor; legend and description are
    /// drawn by the screen.
    pub fn pause_map(&self, mv: &MapView, area: Rect, tex: Option<MapTex>) {
        self.p.rect_filled(area, 0.0, Color32::from_rgb(0x0a, 0x0d, 0x12));
        let mpp = mv.zoom.max(1.0) / area.height();
        let to_screen = |x: f32, z: f32| area.center() + vec2(x - mv.center.x, z - mv.center.y) / mpp;
        if let Some(t) = tex {
            let half = area.size() * 0.5 * mpp;
            let uv0 = t.uv(mv.center.x - half.x, mv.center.y - half.y);
            let uv1 = t.uv(mv.center.x + half.x, mv.center.y + half.y);
            self.p.image(t.id, area, Rect::from_min_max(uv0, uv1), Color32::WHITE);
        }
        let clip = self.clone_clipped(area);
        if mv.route.len() >= 2 {
            let line: Vec<Pos2> = mv.route.iter().map(|p| to_screen(p.x, p.y)).collect();
            clip.polyline(line.clone(), self.px(10.0), Color32::from_black_alpha(150));
            clip.polyline(line, self.px(6.0), color::ACCENT_2);
        }
        let s = self.px(34.0);
        for b in &mv.blips {
            let p = to_screen(b.pos.x, b.pos.y);
            if !area.shrink(s * 0.5).contains(p) {
                continue;
            }
            let col = linear(b.color);
            if b.pulse {
                clip.blip_pulse(p, s, col);
            }
            clip.blip(b.icon, p, s, col);
            clip.blip_height(p, s, b.height);
        }
        // The cursor: a ring with ticks, snapping is the game's business.
        let c = to_screen(mv.cursor.x, mv.cursor.y);
        let r = self.px(26.0);
        let w = self.px(3.0);
        clip.ring(c, r, w + self.px(2.0), Color32::from_black_alpha(140));
        clip.ring(c, r, w, color::TEXT);
        for i in 0..4 {
            let d = dir(i as f32 * std::f32::consts::FRAC_PI_2);
            clip.line(c + d * (r + self.px(4.0)), c + d * (r + self.px(14.0)), w, color::TEXT);
        }
        clip.circle(c, self.px(4.0), color::ACCENT);
    }

    /// The direction of a screen vector as a compass bearing (re-exported for markers).
    pub fn bearing_of(&self, d: Vec2) -> f32 {
        compass(d)
    }
}
