//! The game's own agent tools, registered into `gtf` beside every engine tool
//! (`pav_tools::tools::register`). They need the game's data, so they live here rather than in
//! `pav_tools` (PLAN.md §6.5 E12): `perf`, `citymap`, `lanes` and `vehicle_feel` now; `soak`,
//! `traffic` and `peds` arrive with the systems they measure.

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
            arg("split", "boolean", "also time the frame without the vehicles and without the people"),
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
    Tool {
        name: "lanes",
        help: "Paloma Bay's lane graph: totals by kind; at= the nearest lane (limit, links, junction, its light); at= and to= the route between two points.",
        args: &[
            arg("at", "array", "[x,z] (on the ground) or [x,y,z] world metres"),
            arg("to", "array", "[x,z] or [x,y,z]: with at, the route (lanes, length, minutes at the limits)"),
            arg("time", "number", "seconds on the signal clock for light states (default 0)"),
        ],
        run: t_lanes,
    },
    Tool {
        name: "traffic",
        help: "Traffic in the loaded freeway scene: counts by tier and mood, soak counters, the nearest cars (lane, speed, mood).",
        args: &[arg("limit", "integer", "cars to list, nearest first (default 8)")],
        run: t_traffic,
    },
    Tool {
        name: "peds",
        help: "People in the loaded freeway scene: counts by what they do and by archetype, soak counters, the nearest people.",
        args: &[arg("limit", "integer", "people to list, nearest first (default 8)")],
        run: t_peds,
    },
    Tool {
        name: "soak",
        help: "Runs Paloma Bay unattended (PLAN §7.5): mode traffic (autopilot cruising), peds (standing), roam (the bot drives, gets heat every minute of calm and evades); reports anomaly counters, heat episodes, ms/tick, entity counts.",
        args: &[
            arg("mode", "string", "traffic, peds or roam (default traffic)"),
            arg("seconds", "number", "game seconds to run (default 120)"),
            arg("seed", "integer", "scene seed (default 1)"),
            arg("time", "string", "time of day to start at, HH:MM (default 12:00)"),
            arg("wanted", "integer", "roam: stars to start with (default 0: the first after a minute of calm)"),
            arg("shots", "string", "a path prefix: also capture the game (HUD on) every shot_every seconds"),
            arg("shot_every", "number", "seconds between captures (default 10)"),
        ],
        run: t_soak,
    },
    Tool {
        name: "vehicle_feel",
        help: "Vehicle archetypes on the feel tracks against PLAN §5.6.3: top speed, 0-100 km/h, braking, skidpad g, handbrake slide, 15% climb, the kicker jump.",
        args: &[arg("key", "string", "an archetype key from content/freeway/vehicles.toml, or all (default all)")],
        run: t_vehicle_feel,
    },
];

fn freeway(s: &Session) -> Result<&gtf_game::Freeway> {
    s.sim
        .play()
        .and_then(|g| g.get::<gtf_game::Freeway>())
        .ok_or_else(|| anyhow!("load a freeway scene first (load scene=freeway/roam)"))
}

fn t_traffic(s: &mut Session, a: &Args) -> Result<Output> {
    let g = freeway(s)?;
    let limit = a.get("limit").and_then(Value::as_u64).unwrap_or(8) as usize;
    let me = s.sim.state.player.and_then(|id| s.sim.feet(id)).unwrap_or(Vec3::ZERO);
    let mut cars: Vec<_> =
        g.traffic.cars.iter().filter_map(|c| Some((s.sim.state.entities.get(c.car)?.pos.distance(me), c))).collect();
    cars.sort_by(|x, y| x.0.total_cmp(&y.0));
    let list: Vec<Value> = cars
        .iter()
        .take(limit)
        .map(|(d, c)| {
            json!({ "car": c.car.0, "key": c.key, "dist": r1(*d), "lane": c.lane, "speed": r1(c.speed),
                    "tier": format!("{:?}", c.tier).to_lowercase(), "mood": format!("{:?}", c.mood).to_lowercase(),
                    "held": c.held, "why": c.why, "route": c.route.len(), "goal": c.goal.map(|g| [r1(g.x), r1(g.y), r1(g.z)]), "end": c.end,
                    "bus": c.bus.is_some(), "autopilot": c.pilot })
        })
        .collect();
    Ok(Output::Json(json!({ "summary": g.traffic.summary(), "nearest": list })))
}

fn t_peds(s: &mut Session, a: &Args) -> Result<Output> {
    let g = freeway(s)?;
    let limit = a.get("limit").and_then(Value::as_u64).unwrap_or(8) as usize;
    let def = gtf_game::peds::def()?;
    let me = s.sim.state.player.and_then(|id| s.sim.feet(id)).unwrap_or(Vec3::ZERO);
    let mut doing = std::collections::BTreeMap::<String, usize>::new();
    let mut kinds = std::collections::BTreeMap::<String, usize>::new();
    let mut near = Vec::new();
    for p in &g.peds.list {
        *doing.entry(format!("{:?}", p.doing).to_lowercase()).or_default() += 1;
        let kind = def.archetype.get(p.arch as usize).map(|a| a.id.clone()).unwrap_or_default();
        *kinds.entry(kind.clone()).or_default() += 1;
        if let Some(f) = s.sim.feet(p.id) {
            near.push((f.distance(me), p.id.0, kind, format!("{:?}", p.doing).to_lowercase()));
        }
    }
    near.sort_by(|x, y| x.0.total_cmp(&y.0));
    let list: Vec<Value> =
        near.iter().take(limit).map(|(d, id, k, what)| json!({ "id": id, "kind": k, "dist": r1(*d), "doing": what })).collect();
    Ok(Output::Json(json!({
        "out": g.peds.list.len(), "doing": doing, "kinds": kinds,
        "spawned": g.peds.spawned, "despawned": g.peds.despawned, "rerouted": g.peds.stuck,
        "struck": g.struck.count, "nearest": list,
    })))
}

fn t_soak(s: &mut Session, a: &Args) -> Result<Output> {
    let mode = text(a, "mode").unwrap_or("traffic").to_string();
    if !matches!(mode.as_str(), "traffic" | "peds" | "roam") {
        bail!("soak: mode is traffic, peds or roam (attract arrives with the attract mode)");
    }
    let seconds = a.get("seconds").and_then(Value::as_f64).unwrap_or(120.0).clamp(5.0, 7200.0);
    let seed = a.get("seed").and_then(Value::as_u64).unwrap_or(1);
    let mut args = Args::new();
    args.insert("scene".into(), json!("freeway/roam"));
    args.insert("seed".into(), json!(seed));
    pav_tools::call(s, "load", &args)?;
    let cmd = |s: &mut Session, c: &str| s.sim.step(&pav_core::InputFrame { cmds: vec![c.into()], ..Default::default() });
    cmd(s, &format!("time {}", text(a, "time").unwrap_or("12:00")));
    let shots = text(a, "shots").map(str::to_string);
    let shot_every = (a.get("shot_every").and_then(Value::as_f64).unwrap_or(10.0).max(1.0) * 60.0) as u64;
    match mode.as_str() {
        "traffic" => {
            cmd(s, "drive fenwick_commuter");
            cmd(s, "autopilot on");
        }
        // The bot drives a fast car; a minute without heat and it gets some (1-3 stars, in
        // turn), then runs — leaving wrecks for other cars — until the police give up (or it is
        // wasted or busted, and walks to the next car).
        "roam" => {
            cmd(s, "drive okami_kaze");
            cmd(s, "autopilot escape");
            if let Some(n) = a.get("wanted").and_then(Value::as_u64).filter(|n| *n > 0) {
                cmd(s, &format!("wanted {}", n.min(5)));
            }
        }
        _ => {}
    }
    let (mut episodes, mut escapes, mut wasted, mut busted, mut calm) = (0u32, 0u32, 0u32, 0u32, 0.0f32);
    let mut was_alive = true;
    let mut was_wanted = false;
    let ticks = (seconds * 60.0) as u64;
    let (mut nan, mut fell, mut slowest, mut total_ms) = (0u32, 0u32, 0.0f64, 0.0f64);
    let mut fallen: Vec<(u32, String)> = Vec::new();
    let (mut fewest, mut most) = (usize::MAX, 0usize);
    let start = s.sim.state.player.and_then(|id| s.sim.feet(id)).unwrap_or(Vec3::ZERO);
    let mut travelled = 0.0f32;
    let mut last = start;
    for t in 0..ticks {
        let input = if mode == "peds" { pav_core::InputFrame::default() } else { s.sim.bot_input() };
        let clock = std::time::Instant::now();
        s.sim.step(&input);
        let ms = clock.elapsed().as_secs_f64() * 1000.0;
        total_ms += ms;
        slowest = slowest.max(ms);
        if t % 60 == 0 {
            let n = s.sim.state.entities.iter().count();
            fewest = fewest.min(n);
            most = most.max(n);
            for e in s.sim.state.entities.iter() {
                if !e.pos.is_finite() {
                    nan += 1;
                } else if e.pos.y < -40.0 {
                    fell += 1;
                    if fallen.len() < 12 && !fallen.iter().any(|(id, _)| *id == e.id.0) {
                        fallen.push((e.id.0, format!("{}#{} at {:.0},{:.0},{:.0}", e.name, e.id.0, e.pos.x, e.pos.y, e.pos.z)));
                    }
                }
            }
            let now = s.sim.state.player.and_then(|id| s.sim.feet(id)).unwrap_or(last);
            travelled += now.distance(last);
            last = now;
        }
        s.sim.drain_events();
        if let Some(prefix) = &shots
            && t > 0
            && t % shot_every == 0
        {
            let mut c = Args::new();
            c.insert("out".into(), json!(format!("{prefix}_{:03}s.png", t / 60)));
            c.insert("hud".into(), json!(true));
            c.insert("width".into(), json!(1280));
            c.insert("height".into(), json!(720));
            pav_tools::call(s, "capture", &c)?;
        }
        if mode == "roam" {
            let g = freeway(s)?;
            let (alive, stars, fate) = (g.fate.alive(), g.heat.stars, g.fate);
            if was_alive && !alive {
                match fate {
                    gtf_game::fate::Fate::Wasted { .. } => wasted += 1,
                    gtf_game::fate::Fate::Busted { .. } => busted += 1,
                    gtf_game::fate::Fate::Alive => {}
                }
            } else if alive && was_wanted && stars == 0 && was_alive {
                escapes += 1;
            }
            was_alive = alive;
            was_wanted = stars > 0;
            calm = if alive && stars == 0 { calm + 1.0 / 60.0 } else { 0.0 };
            if calm > 60.0 {
                calm = 0.0;
                episodes += 1;
                cmd(s, &format!("wanted {}", 1 + (episodes - 1) % 3));
            }
        }
    }
    let g = freeway(s)?;
    let traffic = g.traffic.summary();
    let peds = json!({ "out": g.peds.list.len(), "spawned": g.peds.spawned, "rerouted": g.peds.stuck, "struck": g.struck.count });
    let stuck_cars = traffic["stuck_cars"].as_u64().unwrap_or(0);
    let ai_crashes = traffic["ai_crashes"].as_u64().unwrap_or(0);
    let in_view = traffic["spawn_in_view"].as_u64().unwrap_or(0) + traffic["despawn_in_view"].as_u64().unwrap_or(0);
    let police_in_view = g.police.spawn_in_view;
    let ok = stuck_cars == 0 && ai_crashes <= 3 && in_view == 0 && police_in_view == 0 && nan == 0 && fell == 0;
    let heat = json!({
        "episodes": episodes, "escapes": escapes, "wasted": wasted, "busted": busted,
        "police_spawned": g.police.spawned, "police_spawn_in_view": police_in_view,
        "cars_left": g.bot.bailed, "cars_taken": g.bot.taken, "peak": g.heat.peak,
    });
    Ok(Output::Json(json!({
        "mode": mode, "seconds": seconds, "seed": seed, "ok": ok,
        "limits": "stuck_cars 0, ai_crashes <= 3, spawn/despawn in view 0 (traffic and police), NaN 0, fell through 0",
        "traffic": traffic, "peds": peds, "heat": heat,
        "nan_positions": nan, "fell_through_world": fell,
        "fallen": fallen.iter().map(|f| f.1.clone()).collect::<Vec<_>>(),
        "ms_per_tick": (total_ms / ticks.max(1) as f64 * 1000.0).round() / 1000.0,
        "slowest_tick_ms": (slowest * 100.0).round() / 100.0,
        "entities": [fewest, most],
        "player_travelled_m": travelled.round(),
    })))
}

fn t_vehicle_feel(_s: &mut Session, a: &Args) -> Result<Output> {
    use pav_core::vehicle::feel::{self, Test};
    let all = gtf_game::vehicles::all()?;
    let key = a.get("key").and_then(Value::as_str).unwrap_or("all");
    let chosen: Vec<_> = all.iter().filter(|v| (key == "all" && !v.helicopter()) || v.key == key).collect();
    if chosen.is_empty() {
        let keys: Vec<&str> = all.iter().map(|v| v.key.as_str()).collect();
        bail!("no vehicle '{key}' (vehicles: {}, all)", keys.join(", "));
    }
    let mut rows = Vec::new();
    for v in chosen {
        if v.helicopter() {
            rows.push(json!({ "key": v.key, "note": "helicopters fly (no road feel tracks); tests/vehicles.rs checks they climb and fly forward" }));
            continue;
        }
        let f = feel::run(&v.def, &Test::ALL);
        let misses = gtf_game::vehicles::feel_misses(v, &f);
        let j = f.jump.clone().unwrap_or_default();
        rows.push(json!({
            "key": v.key,
            "ok": misses.is_empty(),
            "top_speed": f.top_speed.map(r1),
            "accel_to_kmh": (f.accel_to * 3.6).round(),
            "accel_s": f.accel_time.map(|t| (t * 100.0).round() / 100.0),
            "brake_m": f.brake_distance.map(r1),
            "skidpad_g": f.grip_g.map(|g| (g * 100.0).round() / 100.0),
            "slide_deg": f.slide_deg.map(r1),
            "climb_share": f.climb_share.map(|c| (c * 100.0).round() / 100.0),
            "jump": { "takeoff": r1(j.takeoff_pitch), "landing": r1(j.landing_pitch), "air_s": (j.air_time * 100.0).round() / 100.0, "upright": j.upright },
            "misses": misses,
        }));
    }
    let failed = rows.iter().filter(|r| r["ok"] == json!(false)).count();
    Ok(Output::Json(json!({ "vehicles": rows, "failed": failed })))
}

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
        // Warm the view's caches, then measure frames (the median of five: one frame's time
        // swings with whatever else the machine is doing).
        s.render_timed(960, 540)?;
        let mut times = Vec::new();
        for _ in 0..5 {
            times.push(s.render_timed(960, 540)?.1);
        }
        times.sort_by(f64::total_cmp);
        let view_ms = times[2];
        // Where the frame's time goes: the median of five frames as it is, without the vehicles,
        // and without the people too (the scene is put back afterwards).
        let split = if a.get("split").and_then(Value::as_bool).unwrap_or(false) {
            let median = |s: &mut Session| -> Result<f64> {
                let mut t = Vec::new();
                for _ in 0..5 {
                    t.push(s.render_timed(960, 540)?.1);
                }
                t.sort_by(f64::total_cmp);
                Ok((t[2] * 100.0).round() / 100.0)
            };
            let all = median(s)?;
            let keep = s.sim.state.clone();
            let cars: Vec<_> = s.sim.state.entities.iter().filter(|e| e.vehicle.is_some()).map(|e| e.id).collect();
            for id in &cars {
                s.sim.despawn(*id);
            }
            let without_vehicles = median(s)?;
            let people: Vec<_> = s
                .sim
                .state
                .entities
                .iter()
                .filter(|e| e.character.is_some() && Some(e.id) != s.sim.state.player)
                .map(|e| e.id)
                .collect();
            for id in &people {
                s.sim.despawn(*id);
            }
            let without_people = median(s)?;
            s.sim.state = keep;
            Some(
                json!({ "median": all, "vehicles": cars.len(), "without_vehicles": without_vehicles, "people": people.len(), "without_people_too": without_people }),
            )
        } else {
            None
        };
        let mut stats = pav_tools::tools::render_stats(s)?;
        let t = std::time::Instant::now();
        s.step(ticks);
        let sim_ms = t.elapsed().as_secs_f64() * 1000.0 / ticks as f64;
        stats["view_ms"] = json!((view_ms * 100.0).round() / 100.0);
        stats["sim_ms"] = json!((sim_ms * 1000.0).round() / 1000.0);
        if let Some(split) = split {
            stats["view_ms_split"] = split;
        }
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
        "note": "view_ms: CPU time to build a frame (the median of five after a warm one; build gtf with --release for the budgets); sim_ms: per tick, traffic and people included",
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

/// A point argument: [x, z] on the ground, or [x, y, z].
fn point(a: &Args, key: &str, city: &pav_world::City) -> Result<Option<Vec3>> {
    match a.get(key) {
        None => Ok(None),
        Some(Value::Array(v)) if v.len() == 2 || v.len() == 3 => {
            let f: Vec<f32> = v.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect();
            Ok(Some(match f[..] {
                [x, z] => Vec3::new(x, city.ground_y(Vec2::new(x, z)), z),
                [x, y, z] => Vec3::new(x, y, z),
                _ => unreachable!(),
            }))
        }
        Some(_) => bail!("{key} is [x, z] or [x, y, z]"),
    }
}

fn r1(x: f32) -> f32 {
    (x * 10.0).round() / 10.0
}

fn xyz(p: Vec3) -> [f32; 3] {
    [r1(p.x), r1(p.y), r1(p.z)]
}

fn lower(x: impl std::fmt::Debug) -> String {
    format!("{x:?}").to_lowercase()
}

fn t_lanes(_s: &mut Session, a: &Args) -> Result<Output> {
    let city = gtf_game::world::city()?;
    let g = &city.lanes;
    let t = a.get("time").and_then(Value::as_f64).unwrap_or(0.0) as f32;
    let lane = |i: u32| {
        let l = &g.lanes[i as usize];
        json!({
            "lane": i,
            "kind": lower(l.kind),
            "limit_kmh": (l.speed * 3.6).round(),
            "width": r1(l.width),
            "length": r1(l.length),
            "from": xyz(l.points[0]),
            "to": xyz(*l.points.last().unwrap()),
            "next": l.next,
            "prev": l.prev,
            "left": l.left,
            "right": l.right,
            "junction": l.junction,
            "turn": lower(l.turn),
            "light": g.light(i, t).map(lower),
            "closed": l.closed,
        })
    };
    let (at, to) = (point(a, "at", &city)?, point(a, "to", &city)?);
    let out = match (at, to) {
        (Some(from), Some(to)) => {
            let route = g.route(from, to);
            if route.is_empty() {
                json!({ "route": null, "why": "no lane route between the points (closed lanes, or off the network)" })
            } else {
                let length: f32 = route.iter().map(|&i| g.lanes[i as usize].length).sum();
                let secs: f32 = route.iter().map(|&i| g.lanes[i as usize].length / g.lanes[i as usize].speed.max(1.0)).sum();
                // The route's legs: consecutive lanes of one kind, with their length.
                let mut legs: Vec<(String, f32)> = Vec::new();
                for &i in &route {
                    let l = &g.lanes[i as usize];
                    let kind = lower(l.kind);
                    match legs.last_mut() {
                        Some(leg) if leg.0 == kind => leg.1 += l.length,
                        _ => legs.push((kind, l.length)),
                    }
                }
                json!({
                    "lanes": route.len(),
                    "length": r1(length),
                    "minutes_at_limits": r1(secs / 60.0),
                    "legs": legs.iter().map(|(k, m)| format!("{k} {} m", m.round())).collect::<Vec<_>>(),
                    "first": lane(route[0]),
                    "last": lane(*route.last().unwrap()),
                })
            }
        }
        (Some(p), None) => match g.nearest(p, |_| true) {
            Some((i, along, off)) => json!({ "along": r1(along), "off": r1(off), "nearest": lane(i) }),
            None => json!({ "nearest": null }),
        },
        (None, Some(_)) => bail!("to needs at"),
        (None, None) => {
            let mut kinds: std::collections::BTreeMap<String, (usize, f32)> = Default::default();
            for l in &g.lanes {
                let e = kinds.entry(lower(l.kind)).or_default();
                e.0 += 1;
                e.1 += l.length;
            }
            json!({
                "lanes": g.lanes.len(),
                "km": r1(g.lanes.iter().map(|l| l.length).sum::<f32>() / 1000.0),
                "by_kind": kinds.iter().map(|(k, (n, m))| (k.clone(), json!({ "lanes": n, "km": r1(m / 1000.0) }))).collect::<serde_json::Map<_, _>>(),
                "closed": g.lanes.iter().filter(|l| l.closed).count(),
                "junctions": g.junctions.len(),
                "signals": g.junctions.iter().filter(|j| j.signal.is_some()).count(),
                "parking": g.parking.len(),
            })
        }
    };
    Ok(Output::Json(out))
}
