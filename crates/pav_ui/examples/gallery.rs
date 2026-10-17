//! Renders every demo page (`pav_ui::demo::pages`) over a stand-in city backdrop on a headless
//! GPU, at 1920x1080 and 960x540, to `out/ui/<page>_<w>x<h>.png`.
//!
//!   cargo run -p pav_ui --example gallery                 every page, both sizes
//!   cargo run -p pav_ui --example gallery -- hud phone    some pages
//!   cargo run -p pav_ui --example gallery -- fonts        the display-face comparison (Bebas Neue
//!                                                         and Passion One only if downloaded again)
//!
//! The glyph sheet renders once per glyph set (`glyphs_ps`, `glyphs_xbox`, `glyphs_keys`).

use anyhow::Result;
use egui::{Color32, Mesh, Rect, Shape, pos2, vec2};
use pav_core::input::Device;
use pav_ui::{HostInfo, Offscreen, demo};

const SIZES: [(u32, u32); 2] = [(1920, 1080), (960, 540)];

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let gpu = pav_render::gpu::Headless::new()?;
    let (device, queue) = (&gpu.device, &gpu.queue);
    std::fs::create_dir_all("out/ui")?;
    if args.iter().any(|a| a == "fonts") {
        return fonts(device, queue);
    }
    let map = demo::map();
    let mut pages = demo::pages();
    // The glyph sheet once per glyph set.
    let sheet = pages.iter().position(|(n, _)| *n == "glyphs").map(|i| pages.remove(i).1);
    let mut jobs: Vec<(String, pav_core::present::UiModel, Device)> =
        pages.into_iter().map(|(n, m)| (n.to_string(), m, Device::PlayStation)).collect();
    if let Some(m) = sheet {
        for (suffix, d) in [("ps", Device::PlayStation), ("xbox", Device::Xbox), ("keys", Device::Keyboard)] {
            jobs.push((format!("glyphs_{suffix}"), m.clone(), d));
        }
    }
    let wanted = |n: &str| args.is_empty() || args.iter().any(|a| n == a || n.starts_with(&format!("{a}_")));
    let mut ui = Offscreen::new(device, wgpu::TextureFormat::Rgba8Unorm);
    for (name, model, dev) in jobs.iter().filter(|(n, _, _)| wanted(n)) {
        for (w, h) in SIZES {
            let (tex, view) = target(device, w, h);
            // One egui pass per submit (see `Offscreen`): the backdrop, then the UI.
            let mut enc = device.create_command_encoder(&Default::default());
            ui.paint(device, queue, &mut enc, &view, (w, h), backdrop);
            queue.submit([enc.finish()]);
            let mut enc = device.create_command_encoder(&Default::default());
            let host = HostInfo {
                fps: 60.0,
                frame_ms: 16.6,
                device: *dev,
                keys: None,
                view_proj: Some(demo::camera(w as f32 / h as f32)),
                touch_gear: false,
            };
            ui.render(device, queue, &mut enc, &view, (w, h), model, &host, Some(&map));
            let px = pav_render::capture::read_texture(device, queue, enc, &tex, w, h)?;
            let path = format!("out/ui/{name}_{w}x{h}.png");
            pav_render::capture::save_png(std::path::Path::new(&path), w, h, &px)?;
            println!("{path}");
        }
    }
    Ok(())
}

/// An sRGB texture viewed as `Rgba8Unorm`, the way the window's swapchain is (egui blends in
/// gamma space there).
fn target(device: &wgpu::Device, w: u32, h: u32) -> (wgpu::Texture, wgpu::TextureView) {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("gallery"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[wgpu::TextureFormat::Rgba8Unorm],
    });
    let view =
        tex.create_view(&wgpu::TextureViewDescriptor { format: Some(wgpu::TextureFormat::Rgba8Unorm), ..Default::default() });
    (tex, view)
}

/// A stand-in for the game world: dusk sky, a skyline with lit windows, a bright billboard and
/// a road in perspective, so legibility is judged over busy mid tones, not flat colour.
fn backdrop(ctx: &egui::Context) {
    let p = ctx.layer_painter(egui::LayerId::background());
    let r = ctx.content_rect();
    let (w, h) = (r.width(), r.height());
    let grad = |rect: Rect, top: Color32, bottom: Color32| {
        let mut m = Mesh::default();
        m.colored_vertex(rect.left_top(), top);
        m.colored_vertex(rect.right_top(), top);
        m.colored_vertex(rect.left_bottom(), bottom);
        m.colored_vertex(rect.right_bottom(), bottom);
        m.add_triangle(0, 1, 2);
        m.add_triangle(1, 3, 2);
        Shape::mesh(m)
    };
    let horizon = r.top() + h * 0.58;
    p.add(grad(
        Rect::from_min_max(r.min, pos2(r.right(), horizon)),
        Color32::from_rgb(0x4a, 0x6c, 0x8e),
        Color32::from_rgb(0xf0, 0xb0, 0x7c),
    ));
    p.add(grad(
        Rect::from_min_max(pos2(r.left(), horizon), r.max),
        Color32::from_rgb(0x55, 0x58, 0x5e),
        Color32::from_rgb(0x2e, 0x31, 0x36),
    ));
    p.circle_filled(pos2(r.left() + w * 0.72, horizon - h * 0.08), h * 0.07, Color32::from_rgb(0xff, 0xe2, 0xb0));
    // Skyline.
    let mut x = r.left();
    let mut i = 0u32;
    while x < r.right() {
        let k = i.wrapping_mul(2_654_435_761).rotate_left(7);
        let bw = w * (0.03 + (k % 40) as f32 / 1000.0);
        let bh = h * (0.08 + ((k >> 8) % 300) as f32 / 1000.0);
        let shade = 0x55 + (k >> 16) as u8 % 0x30;
        let b = Rect::from_min_max(pos2(x, horizon - bh), pos2(x + bw, horizon + 2.0));
        p.rect_filled(b, 0.0, Color32::from_rgb(shade - 10, shade, shade + 14));
        let (cols, rows) = ((bw / (w * 0.008)) as i32, (bh / (h * 0.025)) as i32);
        for cy in 0..rows {
            for cx in 0..cols {
                if (k.wrapping_add((cy * 13 + cx * 7) as u32).wrapping_mul(747_796_405) >> 7).is_multiple_of(4) {
                    let win = Rect::from_min_size(
                        pos2(b.left() + w * 0.003 + cx as f32 * w * 0.008, b.top() + h * 0.008 + cy as f32 * h * 0.025),
                        vec2(w * 0.004, h * 0.012),
                    );
                    p.rect_filled(win, 0.0, Color32::from_rgb(0xff, 0xe0, 0xa0));
                }
            }
        }
        x += bw + w * 0.004;
        i += 1;
    }
    // A bright billboard top right and one bottom left (worst cases for white text).
    p.rect_filled(
        Rect::from_min_size(pos2(r.left() + w * 0.62, r.top() + h * 0.12), vec2(w * 0.22, h * 0.16)),
        4.0,
        Color32::from_rgb(0xf2, 0xee, 0xe6),
    );
    p.rect_filled(
        Rect::from_min_size(pos2(r.left() + w * 0.02, r.top() + h * 0.62), vec2(w * 0.18, h * 0.14)),
        4.0,
        Color32::from_rgb(0xe8, 0xd8, 0x6a),
    );
    // The road.
    let c = pos2(r.center().x, horizon);
    let road = vec![
        c + vec2(-w * 0.02, 0.0),
        c + vec2(w * 0.02, 0.0),
        pos2(r.center().x + w * 0.45, r.bottom()),
        pos2(r.center().x - w * 0.45, r.bottom()),
    ];
    p.add(egui::epaint::PathShape::convex_polygon(road, Color32::from_rgb(0x3e, 0x41, 0x46), egui::Stroke::NONE));
    for k in 0..8 {
        let t0 = k as f32 / 8.0;
        let t1 = t0 + 0.05;
        let y0 = horizon + (r.bottom() - horizon) * t0 * t0;
        let y1 = horizon + (r.bottom() - horizon) * t1 * t1;
        let wd = |t: f32| 2.0 + t * w * 0.006;
        p.add(egui::epaint::PathShape::convex_polygon(
            vec![pos2(c.x - wd(t0), y0), pos2(c.x + wd(t0), y0), pos2(c.x + wd(t1), y1), pos2(c.x - wd(t1), y1)],
            Color32::from_rgb(0xf0, 0xc0, 0x40),
            egui::Stroke::NONE,
        ));
    }
}

/// The display-face comparison: the same lines in each candidate face found on disk.
fn fonts(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<()> {
    let candidates = [
        ("Anton", "assets/fonts/anton/Anton-Regular.ttf"),
        ("Bebas Neue", "assets/fonts/bebasneue/BebasNeue-Regular.ttf"),
        ("Passion One", "assets/fonts/passionone/PassionOne-Regular.ttf"),
        ("Passion One Bold", "assets/fonts/passionone/PassionOne-Bold.ttf"),
    ];
    let mut ui = Offscreen::new(device, wgpu::TextureFormat::Rgba8Unorm);
    let mut defs = egui::FontDefinitions::default();
    pav_ui::add_fonts(&mut defs);
    let mut found = Vec::new();
    for (name, path) in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            defs.font_data.insert(name.into(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
            defs.families.insert(egui::FontFamily::Name(name.into()), vec![name.into()]);
            found.push(name);
        }
    }
    ui.context().set_fonts(defs);
    let (w, h) = (1920, 1080);
    let (tex, view) = target(device, w, h);
    let mut enc = device.create_command_encoder(&Default::default());
    ui.paint(device, queue, &mut enc, &view, (w, h), backdrop);
    queue.submit([enc.finish()]);
    let mut enc = device.create_command_encoder(&Default::default());
    ui.paint(device, queue, &mut enc, &view, (w, h), |ctx| {
        let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("fonts")));
        let row_h = h as f32 / found.len().max(1) as f32;
        for (i, name) in found.iter().enumerate() {
            let y = i as f32 * row_h;
            p.rect_filled(Rect::from_min_size(pos2(0.0, y), vec2(w as f32, row_h - 6.0)), 0.0, Color32::from_black_alpha(120));
            let fam = egui::FontFamily::Name((*name).into());
            let label = egui::FontId::new(26.0, egui::FontFamily::Name(pav_ui::theme::FAMILY_UI_BOLD.into()));
            p.text(pos2(30.0, y + 14.0), egui::Align2::LEFT_TOP, *name, label, Color32::from_rgb(0xff, 0x9a, 0x3d));
            p.text(
                pos2(30.0, y + 50.0),
                egui::Align2::LEFT_TOP,
                "GRAND THEFT FREEWAY-BX",
                egui::FontId::new(110.0, fam.clone()),
                Color32::WHITE,
            );
            p.text(
                pos2(30.0, y + 190.0),
                egui::Align2::LEFT_TOP,
                "MISSION PASSED   WASTED   $12,480",
                egui::FontId::new(56.0, fam.clone()),
                Color32::from_rgb(0xff, 0xc1, 0x45),
            );
            p.text(pos2(1350.0, y + 70.0), egui::Align2::LEFT_TOP, "12 / 48", egui::FontId::new(44.0, fam), Color32::WHITE);
        }
    });
    let px = pav_render::capture::read_texture(device, queue, enc, &tex, w, h)?;
    pav_render::capture::save_png(std::path::Path::new("out/ui/fonts.png"), w, h, &px)?;
    println!("out/ui/fonts.png");
    Ok(())
}
