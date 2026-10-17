//! Drawing the UI without a window: headless captures (`capture hud=true`) and the gallery.
//! One egui context and one egui-wgpu renderer, kept between frames so the font atlas and the
//! map texture are uploaded once.

use egui::{Pos2, RawInput, Rect, ViewportId};
use pav_core::present::{MapImage, UiModel};

use crate::{HostInfo, Ui};

/// A reusable offscreen UI renderer. Draws over whatever the target holds (`LoadOp::Load`) at
/// pixels-per-point 1, so a 1920x1080 target is laid out like a 1080p screen.
///
/// Submit the encoder between two calls: egui-wgpu fills its vertex buffers with
/// `Queue::write_buffer`, which lands at the next submit, so two passes recorded into one
/// encoder would both draw the second one's geometry.
pub struct Offscreen {
    ctx: egui::Context,
    renderer: egui_wgpu::Renderer,
    ui: Ui,
    format: wgpu::TextureFormat,
    /// Textures egui let go of last frame: freed on the next call, once the caller has
    /// submitted the encoder that still draws with them.
    to_free: Vec<egui::TextureId>,
}

impl Offscreen {
    /// `format` is the target view's format. egui blends in gamma space on a non-sRGB view (what
    /// the window does); on an sRGB view it blends in linear space, which makes translucent
    /// plates look lighter than in the window. Prefer `Rgba8Unorm` (an sRGB texture can be
    /// viewed that way when created with it in `view_formats`).
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let ctx = egui::Context::default();
        let mut fonts = egui::FontDefinitions::default();
        crate::add_fonts(&mut fonts);
        ctx.set_fonts(fonts);
        let renderer = egui_wgpu::Renderer::new(device, format, egui_wgpu::RendererOptions::PREDICTABLE);
        Self { ctx, renderer, ui: Ui::new(), format, to_free: Vec::new() }
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    /// The egui context it draws with (fonts, style).
    pub fn context(&self) -> &egui::Context {
        &self.ctx
    }

    /// Records the UI for `model` into `encoder`, drawn over `target` (`size` in pixels).
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: (u32, u32),
        model: &UiModel,
        host: &HostInfo,
        map: Option<&MapImage>,
    ) {
        let ui = &mut self.ui;
        run(&self.ctx, &mut self.renderer, &mut self.to_free, device, queue, encoder, target, size, |ctx| {
            ui.draw(ctx, model, host, map);
        });
    }

    /// Records any egui painting into `encoder` over `target`: the gallery's own pages, marks.
    pub fn paint(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: (u32, u32),
        draw: impl FnMut(&egui::Context),
    ) {
        run(&self.ctx, &mut self.renderer, &mut self.to_free, device, queue, encoder, target, size, draw);
    }
}

/// One egui pass at pixels-per-point 1 over `target`, recorded into `encoder`.
fn run(
    ctx: &egui::Context,
    renderer: &mut egui_wgpu::Renderer,
    to_free: &mut Vec<egui::TextureId>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    encoder: &mut wgpu::CommandEncoder,
    target: &wgpu::TextureView,
    size: (u32, u32),
    mut draw: impl FnMut(&egui::Context),
) {
    for id in to_free.drain(..) {
        renderer.free_texture(&id);
    }
    let (w, h) = (size.0.max(1), size.1.max(1));
    let mut raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(w as f32, h as f32))),
        max_texture_side: Some(device.limits().max_texture_dimension_2d as usize),
        ..Default::default()
    };
    if let Some(v) = raw.viewports.get_mut(&ViewportId::ROOT) {
        v.native_pixels_per_point = Some(1.0);
    }
    let out = ctx.run_ui(raw, |ui| draw(ui.ctx()));
    let prims = ctx.tessellate(out.shapes, out.pixels_per_point);
    let mut textures = out.textures_delta;
    for (id, deltas) in &textures.set {
        for delta in deltas {
            renderer.update_texture(device, queue, *id, delta);
        }
    }
    let sd = egui_wgpu::ScreenDescriptor { size_in_pixels: [w, h], pixels_per_point: out.pixels_per_point };
    let extra = renderer.update_buffers(device, queue, encoder, &prims, &sd);
    if !extra.is_empty() {
        queue.submit(extra);
    }
    {
        let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("pav_ui"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        renderer.render(&mut pass.forget_lifetime(), &prims, &sd);
    }
    to_free.extend(textures.free.iter().copied());
    textures.clear();
}

/// Draws the UI for `model` over `target` once (a fresh [`Offscreen`]; keep one for repeated
/// frames). `format` is the target view's format (see [`Offscreen::new`]).
pub fn render_offscreen(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    encoder: &mut wgpu::CommandEncoder,
    target: &wgpu::TextureView,
    format: wgpu::TextureFormat,
    size: (u32, u32),
    model: &UiModel,
    host: &HostInfo,
    map: Option<&MapImage>,
) {
    Offscreen::new(device, format).render(device, queue, encoder, target, size, model, host, map);
}
