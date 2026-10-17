//! # pav_ui: the engine's 10-foot, controller-first UI kit
//!
//! Games describe their UI as plain data: [`pav_core::present::UiModel`], part of the
//! `PlayFrame` a game returns from `Game::present` (HUD, full-screen pages, wheel, phone,
//! banners, toasts, subtitles, prompts, help, world markers, overlays). This crate draws that
//! data with egui, in the window (native and browser) with [`Ui::draw`] and without one for
//! headless captures with [`Offscreen`] or [`render_offscreen`]. A game never touches egui;
//! tools read the same model as JSON.
//!
//! ## Using it from a host
//!
//! ```ignore
//! // Once, where the egui context is made (keeps the host's own Proportional/Monospace fonts):
//! let mut fonts = egui::FontDefinitions::default();
//! pav_ui::add_fonts(&mut fonts);
//! ctx.set_fonts(fonts);
//! let mut ui = pav_ui::Ui::new();
//!
//! // Each frame, inside the egui pass, when the frame has a PlayFrame:
//! let host = pav_ui::HostInfo { fps, frame_ms, device, keys: play.app.keys.as_deref(), view_proj: Some(vp), touch_gear };
//! let r = ui.draw(&ctx, &play.ui, &host, play.map.as_deref());
//! // The kit never moves focus: the game does, from recorded commands.
//! if r.hovered.is_some() && r.hovered != last_hovered { cmds.push(format!("ui.focus {}", r.hovered.unwrap())) }
//! if let Some(id) = r.clicked { cmds.push(format!("ui.accept {id}")) }
//! ```
//!
//! ## The 10-foot rules it enforces (PLAN.md §5.9)
//!
//! - Every size is a 1080p pixel scaled by the screen height times `UiModel::scale`; the layout
//!   keeps inside `UiModel::safe_margin` on every side.
//! - Type: body text at least 28 px, HUD numbers at least 40 px, titles 64 to 120 px
//!   ([`theme::size`]), in heavy weights: Anton for display, Barlow Semi Condensed SemiBold and
//!   Bold for the rest (OFL fonts from `assets/fonts`, embedded).
//! - Text sits on translucent slate plates, or carries a 2 px shadow (an outline for big
//!   numbers) over the world.
//! - Exactly one focused element, shown by the accent fill, a 1.05 scale and a glow.
//! - Every screen with hints gets the glyph-and-verb bar bottom right; lists show at most seven
//!   rows (with scroll marks); no tables or dense grids; numbers are bars or big digits.
//! - Glyphs are drawn from shapes for PlayStation, Xbox and keyboard, following the device
//!   used last (`GlyphStyle::Auto`), with keycaps labelled from the game's key map.
//! - Animation keys off `UiModel::time` and the ages in the data, never a clock, so captures of
//!   the same model are identical.
//!
//! ## Conventions
//!
//! - Map angles are compass bearings: 0 = north (world -z, the top of the map picture),
//!   clockwise, so pi/2 is east (+x). The radar turns so `Minimap::heading` points up.
//! - Wheel slot 0 is at the top; slots go clockwise.
//! - Pointer ids: menu items and cards by their `id`, phone tiles by theirs, phone rows
//!   `phone_row:<i>`, wheel slots `wheel:<i>`, tabs `tab:<i>`; the touch gear sets
//!   [`UiResponse::gear`].
//! - Text markup ([`markup`]): `~y~ ~b~ ~r~ ~g~ ~o~ ~p~` colours until `~s~`, `~n~` breaks the
//!   line, `~btn:NAME~` draws a glyph inline.

mod blips;
pub mod demo;
mod glyphs;
mod hud;
mod icons;
mod map;
pub mod markup;
mod moments;
mod offscreen;
mod paint;
mod phone;
mod screens;
pub mod theme;
mod wheel;

use std::cell::RefCell;

use egui::{Id, LayerId, Order, Pos2, Rect, vec2};
use pav_core::input::Device;
use pav_core::present::{KeyMap, MapImage, UiModel};

pub use blips::blip_name;
pub use glyphs::{GlyphSet, glyph_set, key_label, pretty_key};
pub use offscreen::{Offscreen, render_offscreen};
pub use theme::add_fonts;

use paint::{Cx, Families, Hit};

/// What the host knows that the game does not: frame timing, the device used last, the active
/// key map, the camera, whether to show the touch gear.
#[derive(Clone, Debug)]
pub struct HostInfo<'a> {
    /// Frames per second and frame time for the perf readout (0 = unknown: no readout).
    pub fps: f32,
    pub frame_ms: f32,
    /// The device used last: `GlyphStyle::Auto` follows it.
    pub device: Device,
    /// The game's active key map (`AppRequests::keys`) for keycap labels; None = the standard map.
    pub keys: Option<&'a KeyMap>,
    /// The camera's view-projection (world to clip space) for screen markers; None hides them.
    pub view_proj: Option<glam::Mat4>,
    /// Touch screens: draw the gear button top right and report taps on it.
    pub touch_gear: bool,
}

impl Default for HostInfo<'_> {
    fn default() -> Self {
        Self { fps: 0.0, frame_ms: 0.0, device: Device::PlayStation, keys: None, view_proj: None, touch_gear: false }
    }
}

/// What the pointer did this frame, as element ids: menu items and cards by their `id`, phone
/// tiles by theirs, phone rows `phone_row:<i>`, wheel slots `wheel:<i>`, tabs `tab:<i>`. The
/// host turns a change of `hovered` into a recorded `ui.focus <id>` command and `clicked` into
/// `ui.accept <id>`; the kit never moves focus itself.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UiResponse {
    /// The element under the pointer (None over nothing, or over the host's own windows).
    pub hovered: Option<String>,
    /// The element clicked or tapped this frame.
    pub clicked: Option<String>,
    /// The touch gear was tapped.
    pub gear: bool,
}

/// The UI kit: draws a [`UiModel`] each frame. Keeps the map texture between frames.
pub struct Ui {
    map: map::MapCache,
    standard_keys: KeyMap,
    /// The egui layer the UI paints on (background order, under the host's windows).
    pub layer: LayerId,
}

impl Default for Ui {
    fn default() -> Self {
        Self::new()
    }
}

/// Where the next element of each screen region goes while drawing a frame.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Layout {
    /// The top-right column (money, stars, weapon, toasts): right edge and next free top.
    pub right_x: f32,
    pub right_y: f32,
    /// The bottom-right stack (vehicle card, prompts, timer, meters): right edge and the next
    /// free bottom, going up.
    pub br_right: f32,
    pub br_bottom: f32,
    /// The subtitle's bottom line; the objective sits above the subtitle.
    pub bottom_center: f32,
    /// The top-left column (perf, help): next free top.
    pub top_left: f32,
}

impl Ui {
    pub fn new() -> Self {
        Self {
            map: Default::default(),
            standard_keys: KeyMap::standard(),
            layer: LayerId::new(Order::Background, Id::new("pav_ui")),
        }
    }

    /// Draws `model` over the whole of `ctx`'s screen and reports the pointer. `map` is the
    /// frame's map picture (`PlayFrame::map`) for the radar and the pause map.
    pub fn draw(&mut self, ctx: &egui::Context, model: &UiModel, host: &HostInfo, map: Option<&MapImage>) -> UiResponse {
        let ready = theme::fonts_ready(ctx);
        if !ready {
            // The host skipped `add_fonts`: install them for the next frame, draw with its fonts now.
            log::info!("pav_ui: installing the kit's fonts (call pav_ui::add_fonts where the egui context is made)");
            theme::install_fonts(ctx);
        }
        let screen = ctx.content_rect();
        if screen.width() < 2.0 || screen.height() < 2.0 {
            return UiResponse::default();
        }
        let scale = if model.scale.is_finite() { model.scale.clamp(0.5, 2.0) } else { 1.0 };
        let u = screen.height() / 1080.0 * scale;
        let m = model.safe_margin.clamp(0.0, 0.15);
        let inset = vec2(screen.width() * m, screen.height() * m);
        let safe = Rect::from_min_max(screen.min + inset, screen.max - inset);
        let hits = RefCell::new(Vec::new());
        let cx = Cx {
            ctx: ctx.clone(),
            p: ctx.layer_painter(self.layer),
            layer: self.layer,
            u,
            screen,
            safe,
            time: model.time,
            glyphs: glyph_set(model.glyphs, host.device),
            keys: host.keys.unwrap_or(&self.standard_keys),
            fam: Families::new(ready),
            hits: &hits,
        };
        let tex = self.map.texture(ctx, map);
        draw_frame(&cx, model, host, tex);
        let hits = hits.into_inner();
        pointer(ctx, &hits)
    }
}

/// Paints every element in its layer order.
fn draw_frame(cx: &Cx, model: &UiModel, host: &HostInfo, tex: Option<map::MapTex>) {
    let safe = cx.safe;
    let mut layout = Layout {
        right_x: safe.right(),
        right_y: safe.top(),
        br_right: safe.right(),
        br_bottom: safe.bottom(),
        bottom_center: safe.bottom() - cx.px(6.0),
        top_left: safe.top(),
    };
    // Elements that move others: the phone takes the bottom right, the gear the top-right
    // corner, the perf readout the top-left one.
    let phone_rect = model.phone.as_ref().map(|_| cx.phone_rect());
    if let Some(r) = phone_rect {
        layout.br_right = r.left() - cx.px(28.0);
    }
    if host.touch_gear {
        layout.right_y += cx.px(68.0 + 18.0);
    }
    let show_perf = model.perf && host.fps > 0.0;
    if show_perf {
        let (t, b) = cx.cap(&cx.bold(theme::size::BODY));
        layout.top_left += (b - t) + cx.px(18.0);
    }
    let subtitle = model.subtitle.as_ref().map(|s| cx.subtitle_layout(s));

    // World effects, then world-anchored markers.
    cx.overlay_under(&model.overlay);
    if let Some(vp) = host.view_proj {
        let from = model
            .hud
            .as_ref()
            .and_then(|h| h.minimap.as_ref())
            .map(|mm| glam::Vec3::new(mm.player.x, 0.0, mm.player.y))
            .or_else(|| moments::eye_of(vp));
        let flat = model.hud.as_ref().is_some_and(|h| h.minimap.is_some());
        cx.markers(&model.markers, vp, from, flat);
    }
    // The HUD and play prompts.
    if let Some(hud) = &model.hud {
        let hc = cx.faded(hud.opacity);
        hc.hud_fixed(hud, tex);
        if let Some(v) = &hud.vehicle {
            hc.vehicle_card(v, &mut layout);
        }
    }
    cx.prompts(&model.prompts, &mut layout);
    if let Some(hud) = &model.hud {
        let hc = cx.faded(hud.opacity);
        if let Some(t) = &hud.timer {
            hc.timer(t, &mut layout);
        }
        for m in &hud.meters {
            hc.meter(m, &mut layout);
        }
        hc.hud_right(hud, &mut layout);
        if let Some(o) = &hud.objective {
            let above = subtitle.as_ref().map_or(0.0, |s| s.size.y + cx.px(22.0));
            hc.objective(o, layout.bottom_center - above);
        }
    }
    if let Some(h) = &model.help {
        cx.help(h, &mut layout);
    }
    cx.letterbox(model.overlay.letterbox);
    if let Some(w) = &model.wheel {
        cx.wheel(w);
    }
    if let (Some(p), Some(r)) = (&model.phone, phone_rect) {
        cx.phone(p, r);
    }
    // Pages, then the moments that may sit over them.
    if let Some(s) = &model.screen {
        cx.screen(s, tex);
    }
    for (i, b) in model.banners.iter().enumerate() {
        cx.banner(b, i);
    }
    let mut toast_layout = layout;
    if let Some(r) = phone_rect {
        toast_layout.right_x = r.left() - cx.px(28.0);
    }
    cx.toasts(&model.toasts, &toast_layout);
    if let Some(s) = &subtitle {
        cx.paint_subtitle(s, layout.bottom_center);
    }
    cx.fade(model.overlay.fade);
    if show_perf {
        cx.perf(host.fps, host.frame_ms, safe.top());
    }
    if host.touch_gear {
        cx.gear();
    }
}

/// The topmost element under the pointer, and the one clicked, unless the host's own windows
/// are in the way.
fn pointer(ctx: &egui::Context, hits: &[Hit]) -> UiResponse {
    let mut out = UiResponse::default();
    let (hover, clicked, at) = ctx.input(|i| (i.pointer.hover_pos(), i.pointer.primary_clicked(), i.pointer.interact_pos()));
    let free = |p: Pos2| ctx.layer_id_at(p).is_none_or(|l| l.order == Order::Background);
    let find = |p: Pos2| hits.iter().rev().find(|h| h.area.contains(p)).map(|h| h.id.clone());
    if let Some(p) = hover.filter(|p| free(*p)) {
        out.hovered = find(p).filter(|id| id != "gear");
    }
    if clicked {
        match at.filter(|p| free(*p)).and_then(find) {
            Some(id) if id == "gear" => out.gear = true,
            Some(id) => out.clicked = Some(id),
            None => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::epaint::ClippedShape;
    use egui::{Event, PointerButton, RawInput, pos2};

    fn context() -> egui::Context {
        let ctx = egui::Context::default();
        let mut fonts = egui::FontDefinitions::default();
        add_fonts(&mut fonts);
        ctx.set_fonts(fonts);
        ctx
    }

    /// One 1080p frame: the response and the shapes it drew.
    fn frame(
        ctx: &egui::Context,
        ui: &mut Ui,
        model: &UiModel,
        host: &HostInfo,
        events: Vec<Event>,
        t: f64,
    ) -> (UiResponse, Vec<ClippedShape>) {
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1920.0, 1080.0))),
            time: Some(t),
            events,
            ..Default::default()
        };
        let mut resp = UiResponse::default();
        let mut out = ctx.run_ui(raw, |u| resp = ui.draw(u.ctx(), model, host, None));
        // No GPU here: drop the texture uploads (egui insists they are handled).
        out.textures_delta.clear();
        (resp, out.shapes)
    }

    #[test]
    fn the_pointer_reports_element_ids_and_never_moves_focus() {
        let ctx = context();
        let mut ui = Ui::new();
        let model = demo::weapon_wheel();
        let host = HostInfo { touch_gear: true, ..Default::default() };
        // Slot 0 is at the top of the wheel, slot 2 on the right (clockwise from the top).
        let (r, _) = frame(&ctx, &mut ui, &model, &host, vec![Event::PointerMoved(pos2(960.0, 540.0 - 245.0))], 0.0);
        assert_eq!(r.hovered.as_deref(), Some("wheel:0"));
        let (r, _) = frame(&ctx, &mut ui, &model, &host, vec![Event::PointerMoved(pos2(960.0 + 245.0, 540.0))], 0.1);
        assert_eq!(r.hovered.as_deref(), Some("wheel:2"));
        let (r, _) = frame(&ctx, &mut ui, &model, &host, vec![Event::PointerMoved(pos2(400.0, 500.0))], 0.2);
        assert_eq!(r, UiResponse::default(), "nothing under the pointer");
        // A tap on the gear (top right of the safe area) is the gear, not a click.
        let gear = pos2(1824.0 - 34.0, 54.0 + 34.0);
        let button =
            |pressed| Event::PointerButton { pos: gear, button: PointerButton::Primary, pressed, modifiers: Default::default() };
        frame(&ctx, &mut ui, &model, &host, vec![Event::PointerMoved(gear), button(true)], 0.3);
        let (r, _) = frame(&ctx, &mut ui, &model, &host, vec![button(false)], 0.35);
        assert!(r.gear && r.clicked.is_none(), "{r:?}");
        // A click on a slot reports it; the model's focus is untouched (the game owns it).
        let slot = pos2(960.0 + 245.0, 540.0);
        let button =
            |pressed| Event::PointerButton { pos: slot, button: PointerButton::Primary, pressed, modifiers: Default::default() };
        frame(&ctx, &mut ui, &model, &host, vec![Event::PointerMoved(slot), button(true)], 0.5);
        let (r, _) = frame(&ctx, &mut ui, &model, &host, vec![button(false)], 0.55);
        assert_eq!(r.clicked.as_deref(), Some("wheel:2"));
        assert_eq!(model.wheel.as_ref().and_then(|w| w.focus), Some(0));
    }

    #[test]
    fn the_same_model_draws_the_same_shapes_whatever_the_clock() {
        for (name, model) in demo::pages() {
            let host = HostInfo { fps: 60.0, frame_ms: 16.6, ..Default::default() };
            let (_, a) = frame(&context(), &mut Ui::new(), &model, &host, vec![], 0.0);
            let (_, b) = frame(&context(), &mut Ui::new(), &model, &host, vec![], 123.4);
            assert!(!a.is_empty(), "{name} drew nothing");
            assert!(a == b, "{name}: egui's clock leaked into the drawing");
        }
    }

    #[test]
    fn missing_fonts_install_themselves_without_panicking() {
        let ctx = egui::Context::default();
        let mut ui = Ui::new();
        let model = demo::hud();
        let host = HostInfo::default();
        frame(&ctx, &mut ui, &model, &host, vec![], 0.0);
        frame(&ctx, &mut ui, &model, &host, vec![], 0.1);
        assert!(theme::fonts_ready(&ctx));
    }
}
