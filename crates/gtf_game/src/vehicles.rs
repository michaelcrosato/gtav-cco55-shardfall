//! The vehicle archetypes (`content/freeway/vehicles.toml`, PLAN §5.6.3 and §6.8.2): each is
//! the engine's [`VehicleDef`] (handling, wheels, body prop, doors, seats, lamps) plus the
//! game's own fields: its name, paint colours, share of ambient traffic and engine sound.
//! [`spawn`] puts one in the world; drive it through `Game::vehicles` and read its state with
//! `Sim::vehicle_status` (speed, rpm, gear, slip, damage, lamps, siren).

use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, anyhow, bail};
use glam::{Quat, Vec3};
use pav_core::entity::EntityId;
use pav_core::sim::Sim;
use pav_core::vehicle::VehicleDef;
use pav_core::vehicle::feel::Feel;
use serde::Deserialize;

/// One archetype from `vehicles.toml`.
#[derive(Clone, Debug, PartialEq)]
pub struct Archetype {
    pub key: String,
    pub name: String,
    /// Paint colours (`#rrggbb`) a spawned copy picks from (the body's `$accent` parts).
    pub colors: Vec<String>,
    /// Its share of ambient traffic (0 = never spawned as traffic).
    pub traffic_weight: f32,
    /// The engine sound cue family (`engine.compact`).
    pub sound: String,
    pub def: VehicleDef,
}

impl Archetype {
    /// Heavy vehicles (2.5 t and up: vans, trucks, the bus) get the looser feel ranges.
    pub fn heavy(&self) -> bool {
        self.def.mass >= 2500.0
    }

    pub fn helicopter(&self) -> bool {
        self.def.kind == pav_core::vehicle::VehicleKind::Helicopter
    }
}

/// The game's fields in an archetype's table; the rest is the engine's `VehicleDef`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GameFields {
    name: String,
    #[serde(default)]
    colors: Vec<String>,
    #[serde(default)]
    traffic_weight: f32,
    #[serde(default)]
    sound: String,
}

const GAME_KEYS: [&str; 4] = ["name", "colors", "traffic_weight", "sound"];

/// Reads a vehicles file: every top-level table is an archetype (sorted by key).
pub fn parse(text: &str) -> Result<Vec<Archetype>> {
    let doc: toml::Table = text.parse().map_err(|e| anyhow!("{e}"))?;
    let mut out = Vec::new();
    for (key, value) in doc {
        let toml::Value::Table(mut table) = value else { bail!("{key}: an archetype is a table ([{key}])") };
        let mut game = toml::Table::new();
        for k in GAME_KEYS {
            if let Some(v) = table.remove(k) {
                game.insert(k.into(), v);
            }
        }
        let fields: GameFields = game.try_into().with_context(|| format!("[{key}]"))?;
        let mut def: VehicleDef = toml::Value::Table(table).try_into().with_context(|| format!("[{key}]"))?;
        if let Some(c) = fields.colors.first() {
            def.color = c.clone();
        }
        for c in &fields.colors {
            if pav_core::Color::try_hex(c).is_none() {
                bail!("[{key}]: colour '{c}' is not #rrggbb");
            }
        }
        out.push(Archetype {
            key,
            name: fields.name,
            colors: fields.colors,
            traffic_weight: fields.traffic_weight,
            sound: fields.sound,
            def,
        });
    }
    Ok(out)
}

/// Every archetype (`content/freeway/vehicles.toml`, read again when its text changes).
pub fn all() -> Result<Arc<Vec<Archetype>>> {
    static CACHE: Mutex<Option<(String, Arc<Vec<Archetype>>)>> = Mutex::new(None);
    let text = crate::content::text("vehicles.toml", include_str!("../../../content/freeway/vehicles.toml"));
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((t, list)) = cache.as_ref()
        && *t == *text
    {
        return Ok(list.clone());
    }
    let list = Arc::new(parse(&text).context("content/freeway/vehicles.toml")?);
    *cache = Some((text.into_owned(), list.clone()));
    Ok(list)
}

/// The archetype `key`.
pub fn get(key: &str) -> Result<Archetype> {
    let list = all()?;
    list.iter().find(|a| a.key == key).cloned().ok_or_else(|| {
        let keys: Vec<&str> = list.iter().map(|a| a.key.as_str()).collect();
        anyhow!("no vehicle '{key}' (vehicles: {})", keys.join(", "))
    })
}

/// Spawns archetype `key` standing at `feet` facing `yaw` (radians, 0 = +z), painted with its
/// colour `paint` (wrapping; None: the first).
pub fn spawn(sim: &mut Sim, key: &str, feet: Vec3, yaw: f32, paint: Option<usize>) -> Result<EntityId> {
    let a = get(key)?;
    let mut def = a.def.clone();
    if let (Some(i), false) = (paint, a.colors.is_empty()) {
        def.color = a.colors[i % a.colors.len()].clone();
    }
    Ok(sim.spawn_vehicle(key, def, feet, Quat::from_rotation_y(yaw), None))
}

/// PLAN §5.6.3: (key, top speed m/s, 0–100 km/h seconds).
pub const FEEL_TARGETS: &[(&str, f32, f32)] = &[
    ("fenwick_commuter", 30.0, 9.0),
    ("zephyr_wagon", 31.0, 8.5),
    ("okami_kaze", 42.0, 3.8),
    ("bravura_stallion", 38.0, 4.6),
    ("daimon_titan", 31.0, 7.0),
    ("marlin_bay", 30.0, 7.5),
    ("fenwick_courier", 26.0, 11.0),
    ("paloma_cab", 32.0, 8.0),
    ("sppd_interceptor", 40.0, 5.0),
    ("metro_bus", 20.0, 18.0),
    ("ironclad_armored", 25.0, 14.0),
    ("city_works", 24.0, 13.0),
    ("sppd_tactical", 30.0, 9.0),
];

fn within(x: f32, target: f32, tol: f32) -> bool {
    (x - target).abs() <= target * tol
}

/// What `a` misses of its PLAN §5.6.3 targets on the feel tracks (empty: all met; a helicopter
/// or an archetype without targets has none).
pub fn feel_misses(a: &Archetype, f: &Feel) -> Vec<String> {
    let Some(&(_, top, accel)) = FEEL_TARGETS.iter().find(|(k, _, _)| *k == a.key) else { return Vec::new() };
    let mut out = Vec::new();
    let mut check = |ok: bool, what: String| {
        if !ok {
            out.push(what);
        }
    };
    let ts = f.top_speed.unwrap_or(0.0);
    check(within(ts, top, 0.1), format!("top speed {ts:.1} m/s (target {top})"));
    match f.accel_time {
        Some(t) => check(within(t, accel, 0.1), format!("0-{:.0} km/h in {t:.2} s (target {accel})", f.accel_to * 3.6)),
        None => check(false, format!("never reached {:.0} km/h", f.accel_to * 3.6)),
    }
    let d = f.brake_distance.unwrap_or(f32::MAX);
    match a.def.class.as_str() {
        "compact" => check((38.0..=50.0).contains(&d), format!("braking {d:.1} m (38-50)")),
        "sports" => check((32.0..=40.0).contains(&d), format!("braking {d:.1} m (32-40)")),
        _ => {
            let g = f.brake_from * f.brake_from / (2.0 * d) / 9.81;
            check((0.45..=1.15).contains(&g), format!("braking {d:.1} m from {:.1} m/s = {g:.2} g", f.brake_from))
        }
    }
    let g = f.grip_g.unwrap_or(0.0);
    let s = f.slide_deg.unwrap_or(0.0);
    let climb = f.climb_share.unwrap_or(0.0);
    if a.heavy() {
        check((0.6..=1.1).contains(&g), format!("skidpad {g:.2} g (0.6-1.1)"));
        check(s < 90.0, format!("handbrake slide {s:.0} deg spun it round"));
        check(climb >= 0.15, format!("15% grade at {:.0}% of top speed (heavy: climbs it)", climb * 100.0));
    } else {
        check((0.9..=1.1).contains(&g), format!("skidpad {g:.2} g (0.9-1.1)"));
        check((25.0..=45.0).contains(&s), format!("handbrake slide {s:.0} deg (25-45)"));
        check(f.slide_recovered == Some(true), "did not straighten out after the slide".into());
        check(climb >= 0.6, format!("15% grade at {:.0}% of top speed (>= 60%)", climb * 100.0));
    }
    check(!f.rolled_over, "rolled over on flat ground".into());
    match &f.jump {
        Some(j) => {
            check(j.upright, format!("not upright after the jump ({j:?})"));
            check(j.landing_pitch.abs() <= 20.0, format!("landed at {:.0} deg pitch", j.landing_pitch));
            check(j.max_roll <= 15.0, format!("rolled {:.0} deg in the air", j.max_roll));
        }
        None => check(false, "no jump".into()),
    }
    out
}

/// One line of feel numbers for `a`.
pub fn feel_row(a: &Archetype, f: &Feel) -> String {
    let j = f.jump.clone().unwrap_or_default();
    format!(
        "{:18} top {:5.1}  0-{:.0} {:5.2}s  brake {:5.1}m  {:4.2}g  slide {:4.1}{}  climb {:4.0}%  jump {:+5.1}/{:+5.1}/{:+5.1} deg air {:.2}s {}",
        a.key,
        f.top_speed.unwrap_or(0.0),
        f.accel_to * 3.6,
        f.accel_time.unwrap_or(f32::NAN),
        f.brake_distance.unwrap_or(f32::NAN),
        f.grip_g.unwrap_or(f32::NAN),
        f.slide_deg.unwrap_or(f32::NAN),
        if f.slide_recovered == Some(true) { "" } else { "!" },
        f.climb_share.unwrap_or(0.0) * 100.0,
        j.takeoff_pitch,
        j.max_pitch,
        j.landing_pitch,
        j.air_time,
        if j.upright { "upright" } else { "NOT UPRIGHT" },
    )
}
