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
