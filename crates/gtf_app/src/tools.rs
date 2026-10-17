//! The game's own agent tools, registered into `gtf` beside every engine tool
//! (`pav_tools::tools::register`). They need the game's data, so they live here rather than in
//! `pav_tools` (PLAN.md §6.5 E12): `citymap` now; `perf`, `soak`, `traffic`, `peds`,
//! `vehicle_feel` arrive with the systems they measure.

use std::path::PathBuf;

use anyhow::{Result, anyhow, bail};
use glam::{Vec2, Vec3};
use pav_tools::Tool;
use pav_tools::session::Session;
use pav_tools::tools::{Args, Output, arg};
use pav_world::lanes::LaneKind;
use pav_world::walk::Walk;
use serde_json::{Value, json};

pub static TOOLS: &[Tool] = &[
    Tool {
        name: "perf",
        help: "Performance budgets (PLAN §6.9) at reference views: render stats, view build ms, sim ms/tick.",
        args: &[
            arg("view", "string", "a view name or all (default all)"),
            arg("ticks", "integer", "sim ticks to time per view (default 300)"),
        ],
        run: t_perf,
    },
    Tool {
        name: "citymap",
        help: "Paloma Bay's map as a PNG with overlays (lanes and their directions, signals, parking, walk tiles, cover).",
        args: &[
            arg("layers", "string", "comma list: lanes,signals,parking,walk,cover,all (default lanes,signals)"),
            arg("crop", "array", "[x0,z0,x1,z1] world metres (default: the whole district)"),
            arg("scale", "integer", "pixels per metre, 1-8 (default 1, or 3 with crop)"),
            arg("out", "string", "PNG path (default out/citymap.png)"),
        ],
        run: t_citymap,
    },
];

/// One budget row: (metric, budget, relaxed budget for `hills_vista`).
const BUDGETS: &[(&str, f64, f64)] = &[
    ("draw_calls", 2000.0, 2500.0),
    ("mesh_instances", 40_000.0, 55_000.0),
    ("triangles", 2_500_000.0, 3_500_000.0),
    ("sdf", 3000.0, 3000.0),
    ("lights", 64.0, 64.0),
    ("view_ms", 4.0, 6.0),
    ("sim_ms", 3.0, 3.0),
];

fn t_perf(s: &mut Session, a: &Args) -> Result<Output> {
    let views = gtf_game::views::all()?;
    let which = text(a, "view").unwrap_or("all");
    let ticks = a.get("ticks").and_then(Value::as_u64).unwrap_or(300).clamp(10, 6000);
    let chosen: Vec<_> = views.iter().filter(|v| which == "all" || v.name == which).collect();
    if chosen.is_empty() {
        let names: Vec<&str> = views.iter().map(|v| v.name.as_str()).collect();
        bail!("no view '{which}' (views: {})", names.join(", "));
    }
    let mut rows = Vec::new();
    let mut failed = Vec::new();
    for v in chosen {
        let mut args = Args::new();
        args.insert("scene".into(), json!(format!("freeway/view/{}", v.name)));
        pav_tools::call(s, "load", &args)?;
        s.step(5);
        // Warm the view's caches, then measure a frame.
        s.render_timed(960, 540)?;
        let (_, view_ms) = s.render_timed(960, 540)?;
        let mut stats = pav_tools::tools::render_stats(s)?;
        let t = std::time::Instant::now();
        s.step(ticks);
        let sim_ms = t.elapsed().as_secs_f64() * 1000.0 / ticks as f64;
        stats["view_ms"] = json!((view_ms * 100.0).round() / 100.0);
        stats["sim_ms"] = json!((sim_ms * 1000.0).round() / 1000.0);
        let relaxed = v.name == "hills_vista";
        let mut over = Vec::new();
        for (k, budget, wide) in BUDGETS {
            let limit = if relaxed { *wide } else { *budget };
            if let Some(x) = stats.get(*k).and_then(Value::as_f64) {
                if x > limit {
                    over.push(format!("{k} {x} > {limit}"));
                }
            }
        }
        if !over.is_empty() {
            failed.push(format!("{}: {}", v.name, over.join(", ")));
        }
        rows.push(json!({ "view": v.name, "ok": over.is_empty(), "stats": stats, "over": over }));
    }
    Ok(Output::Json(json!({
        "ok": failed.is_empty(),
        "views": rows,
        "failed": failed,
        "note": "debug build; view_ms is CPU time to build one frame; sim_ms covers the city without population yet",
    })))
}

fn text<'a>(a: &'a Args, k: &str) -> Option<&'a str> {
    a.get(k).and_then(Value::as_str)
}

/// A picture to draw on, in world metres.
struct Canvas {
    w: u32,
    h: u32,
    px: Vec<u8>,
    origin: Vec2,
    scale: f32,
}

impl Canvas {
    fn put(&mut self, x: i32, y: i32, c: [u8; 4]) {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return;
        }
        let i = ((y as u32 * self.w + x as u32) * 4) as usize;
        let a = c[3] as f32 / 255.0;
        for (dst, src) in self.px[i..i + 3].iter_mut().zip(c) {
            *dst = (*dst as f32 * (1.0 - a) + src as f32 * a) as u8;
        }
    }

    fn to_px(&self, p: Vec2) -> Vec2 {
        (p - self.origin) * self.scale
    }

    fn dot(&mut self, p: Vec2, r: f32, c: [u8; 4]) {
        let q = self.to_px(p);
        let rr = (r * self.scale).max(1.0);
        for y in (q.y - rr).floor() as i32..=(q.y + rr).ceil() as i32 {
            for x in (q.x - rr).floor() as i32..=(q.x + rr).ceil() as i32 {
                if Vec2::new(x as f32 + 0.5, y as f32 + 0.5).distance(q) <= rr {
                    self.put(x, y, c);
                }
            }
        }
    }

    fn line(&mut self, a: Vec2, b: Vec2, width: f32, c: [u8; 4]) {
        let (p, q) = (self.to_px(a), self.to_px(b));
        let n = (p.distance(q).ceil() as i32).max(1);
        let r = (width * self.scale * 0.5).max(0.5);
        for i in 0..=n {
            let m = p.lerp(q, i as f32 / n as f32);
            for y in (m.y - r).floor() as i32..=(m.y + r).ceil() as i32 {
                for x in (m.x - r).floor() as i32..=(m.x + r).ceil() as i32 {
                    if Vec2::new(x as f32 + 0.5, y as f32 + 0.5).distance(m) <= r + 0.01 {
                        self.put(x, y, c);
                    }
                }
            }
        }
    }
}

fn flat(p: Vec3) -> Vec2 {
    Vec2::new(p.x, p.z)
}

fn t_citymap(_s: &mut Session, a: &Args) -> Result<Output> {
    let city = gtf_game::world::city()?;
    let layers: Vec<&str> = text(a, "layers").unwrap_or("lanes,signals").split(',').map(str::trim).collect();
    let all = layers.contains(&"all");
    let has = |k: &str| all || layers.contains(&k);
    for l in &layers {
        if !["lanes", "signals", "parking", "walk", "cover", "all", ""].contains(l) {
            bail!("unknown layer '{l}' (lanes, signals, parking, walk, cover, all)");
        }
    }
    let map = &city.map;
    let full = (map.origin, map.origin + Vec2::new(map.width as f32, map.height as f32) * map.metres_per_px);
    let crop = match a.get("crop") {
        Some(Value::Array(v)) if v.len() == 4 => {
            let f: Vec<f32> = v.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect();
            (Vec2::new(f[0].min(f[2]), f[1].min(f[3])), Vec2::new(f[0].max(f[2]), f[1].max(f[3])))
        }
        Some(_) => bail!("crop is [x0, z0, x1, z1]"),
        None => full,
    };
    let scale = match a.get("scale").and_then(Value::as_u64) {
        Some(k) if (1..=8).contains(&k) => k as f32,
        Some(_) => bail!("scale is 1-8"),
        None if a.contains_key("crop") => 3.0,
        None => 1.0,
    };
    let size = (crop.1 - crop.0) * scale;
    if size.x < 1.0 || size.y < 1.0 || size.x * size.y > 16e6 {
        bail!("the picture would be {}x{} pixels", size.x as u32, size.y as u32);
    }
    let (w, h) = (size.x as u32, size.y as u32);
    let mut c = Canvas { w, h, px: vec![0; (w * h * 4) as usize], origin: crop.0, scale };
    // The base map, sampled from the 1 px/m picture.
    for y in 0..h {
        for x in 0..w {
            let p = crop.0 + Vec2::new(x as f32 + 0.5, y as f32 + 0.5) / scale;
            let q = ((p - map.origin) / map.metres_per_px).floor();
            let i = ((y * w + x) * 4) as usize;
            if q.x >= 0.0 && q.y >= 0.0 && (q.x as u32) < map.width && (q.y as u32) < map.height {
                let j = ((q.y as u32 * map.width + q.x as u32) * 4) as usize;
                c.px[i..i + 4].copy_from_slice(&map.rgba[j..j + 4]);
            } else {
                c.px[i..i + 4].copy_from_slice(&[10, 14, 18, 255]);
            }
        }
    }
    let inside = |p: Vec2| p.x >= crop.0.x - 20.0 && p.y >= crop.0.y - 20.0 && p.x <= crop.1.x + 20.0 && p.y <= crop.1.y + 20.0;
    if has("walk") {
        let wm = &city.walk;
        let t0 = wm.tile_of(crop.0);
        let t1 = wm.tile_of(crop.1);
        for ty in t0.y.max(0)..=t1.y.min(wm.h - 1) {
            for tx in t0.x.max(0)..=t1.x.min(wm.w - 1) {
                let t = glam::IVec2::new(tx, ty);
                let col = match wm.get(t) {
                    Walk::Sidewalk => [220, 220, 220, 70],
                    Walk::Crossing { .. } => [255, 150, 40, 170],
                    Walk::Path => [210, 180, 120, 90],
                    Walk::Open => [120, 200, 120, 50],
                    Walk::Blocked => continue,
                };
                let ctr = wm.centre(t);
                let half = Vec2::splat(wm.tile * 0.5);
                let (p, q) = (c.to_px(ctr - half), c.to_px(ctr + half));
                for y in p.y as i32..q.y as i32 {
                    for x in p.x as i32..q.x as i32 {
                        c.put(x, y, col);
                    }
                }
            }
        }
    }
    let g = &city.lanes;
    if has("lanes") {
        for l in &g.lanes {
            if !l.points.iter().any(|p| inside(flat(*p))) {
                continue;
            }
            let col = match (l.kind, l.closed) {
                (_, true) => [230, 70, 70, 200],
                (LaneKind::Freeway, _) => [90, 170, 255, 255],
                (LaneKind::Ramp, _) => [80, 230, 230, 255],
                (LaneKind::Connector, _) => [240, 210, 80, 150],
                (LaneKind::Channel, _) => [80, 200, 170, 255],
                (LaneKind::Boulevard | LaneKind::Avenue, _) => [245, 245, 245, 230],
                _ => [200, 200, 205, 200],
            };
            let width = if l.kind == LaneKind::Connector { 0.4 } else { 0.7 };
            for w2 in l.points.windows(2) {
                c.line(flat(w2[0]), flat(w2[1]), width, col);
            }
            // An arrowhead every 24 m shows the direction of travel.
            if l.kind != LaneKind::Connector {
                let mut d = 12.0;
                while d < l.length {
                    let (p, dir) = l.at(d);
                    let (p, dir) = (flat(p), Vec2::new(dir.x, dir.z).normalize_or(Vec2::X));
                    let side = Vec2::new(-dir.y, dir.x);
                    c.line(p, p - dir * 2.2 + side * 1.2, 0.5, col);
                    c.line(p, p - dir * 2.2 - side * 1.2, 0.5, col);
                    d += 24.0;
                }
            }
        }
    }
    if has("parking") {
        for spot in &g.parking {
            if inside(flat(spot.pos)) {
                c.dot(flat(spot.pos), 0.9, [230, 90, 230, 220]);
            }
        }
    }
    if has("signals") {
        for j in &g.junctions {
            if inside(j.centre()) {
                let col = if j.signal.is_some() { [80, 230, 110, 255] } else { [150, 150, 150, 200] };
                c.dot(j.centre(), 2.4, col);
            }
        }
    }
    if has("cover") {
        for p in &city.cover.points {
            let q = flat(p.pos);
            if inside(q) {
                c.dot(q, 0.35, if p.high { [120, 255, 120, 255] } else { [255, 255, 120, 255] });
                c.line(q, q + p.normal * 0.9, 0.2, [120, 255, 120, 160]);
            }
        }
    }
    let path = PathBuf::from(text(a, "out").unwrap_or("out/citymap.png"));
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    let mut png_bytes = Vec::new();
    {
        let mut e = png::Encoder::new(&mut png_bytes, w, h);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().map_err(|e| anyhow!("png: {e}"))?.write_image_data(&c.px).map_err(|e| anyhow!("png: {e}"))?;
    }
    std::fs::write(&path, &png_bytes)?;
    let (side, cross, paths, open) = city.walk.counts();
    let meta = json!({
        "path": path.display().to_string(),
        "width": w,
        "height": h,
        "lanes": g.lanes.len(),
        "open_lanes": g.lanes.iter().filter(|l| !l.closed).count(),
        "junctions": g.junctions.len(),
        "signals": g.junctions.iter().filter(|j| j.signal.is_some()).count(),
        "parking": g.parking.len(),
        "walk_tiles": { "sidewalk": side, "crossing": cross, "path": paths, "open": open },
        "cover": city.cover.points.len(),
    });
    Ok(Output::Image { png: png_bytes, path: Some(path), meta })
}
