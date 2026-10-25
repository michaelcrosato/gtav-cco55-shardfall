//! Paloma Bay: the district `pav_world` builds from `content/freeway/district.toml`, streamed
//! around the player and the camera, with its landmarks on the map.

use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use glam::{Vec2, Vec3};
use pav_core::Color;
use pav_core::entity::EntityId;
use pav_core::present::{Blip, BlipIcon};
use pav_core::region::StreamSettings;
use pav_core::sim::Sim;
use pav_world::City;

/// Where a new game starts: the south sidewalk of Sunset Row, by Bang Bang Bros.
pub const SPAWN: Vec3 = Vec3::new(-148.0, 0.15, -225.0);

/// The middle of downtown (the title screen's drone circles it).
pub const DOWNTOWN: Vec3 = Vec3::new(-190.0, 0.0, -96.0);

/// Streaming for a city at street level and freeway speed (fog hides the edge until the far
/// view arrives).
pub const STREAM: StreamSettings = StreamSettings { radius: 160.0, unload: 230.0, lookahead: 1.6, budget: 3 };

/// A district file under `content/freeway/`, built once per process for each version of its text.
fn district(file: &'static str, built_in: &'static str) -> Result<Arc<City>> {
    static CACHE: Mutex<Vec<(&'static str, String, Arc<City>)>> = Mutex::new(Vec::new());
    let text = crate::content::text(file, built_in);
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, _, c)) = cache.iter().find(|(f, t, _)| *f == file && *t == *text) {
        return Ok(c.clone());
    }
    let city = Arc::new(City::from_toml(&text).map_err(|e| anyhow!("content/freeway/{file}: {e}"))?);
    cache.retain(|(f, _, _)| *f != file);
    cache.push((file, text.into_owned(), city.clone()));
    Ok(city)
}

/// Paloma Bay.
pub fn city() -> Result<Arc<City>> {
    district("district.toml", include_str!("../../../content/freeway/district.toml"))
}

/// The Proving Grounds, the developer district (`freeway/dev`).
pub fn dev_city() -> Result<Arc<City>> {
    district("dev_district.toml", include_str!("../../../content/freeway/dev_district.toml"))
}

/// The district a scene mode plays in (a mission scene: its mission's).
pub fn city_for(mode: &str) -> Result<Arc<City>> {
    let mission = mode.strip_prefix("mission/").and_then(|k| crate::missions::get(k).ok());
    if mode == "dev" || mission.is_some_and(|m| m.district == "dev") { dev_city() } else { city() }
}

/// Where the Proving Grounds start: by the main crossroads.
pub const DEV_SPAWN: Vec3 = Vec3::new(-22.0, 0.15, -22.0);

/// Builds the city scene: hands the simulation the district, loads the area around `at` (the
/// start, or a view's target and eye) and puts the player on the ground at `at`.
pub fn build_scene(sim: &mut Sim, city: Arc<City>, at: Vec2, also: &[Vec3]) -> Result<Arc<City>> {
    let spawn = Vec3::new(at.x, city.ground_y(at) + 0.05, at.y);
    sim.state.spawn = spawn;
    sim.state.focus = spawn;
    sim.set_region_provider(city.clone(), STREAM);
    sim.state.world.interest.extend_from_slice(also);
    sim.stream_now();
    sim.state.world.interest.clear();
    sim.spawn_player();
    Ok(city)
}

fn icon(name: &str) -> Option<(BlipIcon, &'static str)> {
    Some(match name {
        "bank" => (BlipIcon::Heist, "#6fe28a"),
        "police" => (BlipIcon::Police, "#3d8bff"),
        "hospital" => (BlipIcon::Hospital, "#ff4d4d"),
        "gun" => (BlipIcon::Gun, "#ff7a3d"),
        "shop" => (BlipIcon::Shop, "#ffc145"),
        "vehicle" => (BlipIcon::Vehicle, "#c9d6df"),
        "heist" => (BlipIcon::Heist, "#ffc145"),
        "activity" => (BlipIcon::Activity, "#ff3e8a"),
        "garage" => (BlipIcon::Garage, "#8fd3ff"),
        _ => return None,
    })
}

/// The landmarks with map icons.
pub fn blips(city: &City) -> Vec<Blip> {
    city.def
        .landmark
        .iter()
        .filter_map(|l| {
            let (icon, hex) = icon(&l.blip)?;
            let [[x0, z0], [x1, z1]] = l.cells;
            let a = city.cell_centre(glam::IVec2::new(x0, z0));
            let b = city.cell_centre(glam::IVec2::new(x1, z1));
            Some(Blip {
                pos: (a + b) * 0.5,
                icon,
                color: Color::hex(hex),
                label: Some(l.name.clone()),
                clamp: false,
                height: 0,
                pulse: false,
            })
        })
        .collect()
}

/// The sidewalk outside landmark `key` (its door, near enough): the walkable tile nearest the
/// landmark's middle, on the ground.
pub fn door(city: &City, key: &str) -> Option<Vec3> {
    let l = city.def.landmark.iter().find(|l| l.key == key)?;
    let [[x0, z0], [x1, z1]] = l.cells;
    let a = city.cell_centre(glam::IVec2::new(x0, z0));
    let b = city.cell_centre(glam::IVec2::new(x1, z1));
    let mid = (a + b) * 0.5;
    let half = ((b - a).abs() * 0.5 + Vec2::splat(12.0)).max_element();
    // Spiral out from the middle until a sidewalk or path tile turns up.
    for r in 0..40 {
        let rr = r as f32 * 2.0;
        for k in 0..16 {
            let ang = k as f32 / 16.0 * std::f32::consts::TAU;
            let p = mid + Vec2::new(ang.cos(), ang.sin()) * (half * 0.5 + rr);
            let t = city.walk.tile_of(p);
            if matches!(city.walk.get(t), pav_world::walk::Walk::Sidewalk | pav_world::walk::Walk::Path) {
                let c = city.walk.centre(t);
                return Some(Vec3::new(c.x, city.ground_y(c), c.y));
            }
        }
    }
    None
}

/// The landmarks' Ferris wheels, turned by the game clock `t` (seconds): each rim within 700 m
/// of the eye, its cars within 450 m (closer still they are the only moving part of it).
pub fn wheels(city: &City, eye: Vec3, t: f32, out: &mut Vec<pav_core::present::AssetDraw>) {
    for (i, w) in city.wheels.iter().enumerate() {
        let d = w.hub.distance(eye);
        if d > 700.0 {
            continue;
        }
        let id = 0x5157_0000 + i as u32 * 256;
        let (pos, rot) = w.rim_at(t as f64);
        out.push(pav_core::present::AssetDraw { asset: w.rim.clone(), pos, rot, scale: 1.0, id, seed: 0 });
        if d < 450.0 {
            for (k, (pos, rot)) in w.cars_at(t as f64).into_iter().enumerate() {
                let k = k as u32;
                out.push(pav_core::present::AssetDraw { asset: w.car.clone(), pos, rot, scale: 1.0, id: id + 1 + k, seed: k });
            }
        }
    }
}

/// Ground-plane position of a world point (the map's coordinates).
pub fn flat(p: Vec3) -> Vec2 {
    Vec2::new(p.x, p.z)
}

/// The lit lamp of every traffic light near `at` (signal heads are drawn live: their state is a
/// function of the game clock).
pub fn signal_lamps(city: &City, at: Vec3, t: f32, out: &mut Vec<pav_core::present::PrimDraw>) {
    use pav_world::lanes::Light;
    for h in &city.signal_heads {
        if h.red.distance_squared(at) > 160.0 * 160.0 {
            continue;
        }
        let Some(plan) = city.lanes.junctions[h.junction as usize].signal else { continue };
        let (pos, hex) = match plan.light(h.ns, t) {
            Light::Green => (h.green, "#3dff8a"),
            Light::Amber => (h.amber, "#ffb52e"),
            Light::Red => (h.red, "#ff3b30"),
        };
        out.push(pav_core::present::PrimDraw {
            shape: pav_core::shape::Shape::Cylinder { half_height: 0.012, radius: 0.12 },
            pos: pos + h.facing * 0.02,
            rot: glam::Quat::from_rotation_arc(Vec3::Y, h.facing),
            color: Color::hex(hex),
            emissive: 3.5,
            look: pav_core::shape::Look::Unlit,
            shadow: false,
            glow: Default::default(),
        });
    }
}

/// Marks `id` as population the game makes again (traffic, parked cars, people, the police):
/// when its ground unloads it goes with it, rather than staying behind with nothing under it.
pub fn ambient(sim: &mut Sim, id: EntityId) {
    if let Some(e) = sim.state.entities.get_mut(id) {
        e.unload = pav_core::entity::Unload::Despawn;
    }
}

/// People of the population on foot where the ground is not loaded (they walked off the edge of
/// the streamed area, or were left there) go, as the engine removes those standing on ground
/// that unloads: otherwise they would fall through. So do any who fell through all the same
/// (below the lowest ground there is).
pub fn sweep_unloaded(sim: &mut Sim) {
    let gone: Vec<EntityId> = sim
        .state
        .entities
        .iter()
        .filter(|e| e.unload == pav_core::entity::Unload::Despawn && Some(e.id) != sim.state.player)
        .filter(|e| e.character.as_ref().is_some_and(|c| c.riding.is_none()))
        .filter(|e| e.pos.y < FELL_THROUGH || !ground_loaded(sim, e.pos))
        .map(|e| e.id)
        .collect();
    for id in gone {
        sim.despawn(id);
    }
}

/// Below this height (m) a person has fallen through the world (the Ditch's bed and the harbor's
/// floor are a few metres down).
const FELL_THROUGH: f32 = -30.0;

/// Whether the ground around `p` is loaded (every streaming chunk within a car's length of it is
/// active; none lies beyond the map's edge): only there do bodies with physics stand on it
/// (elsewhere they fall through, or tip over the edge of what is loaded).
pub fn ground_loaded(sim: &Sim, p: Vec3) -> bool {
    use pav_core::statics::{CHUNK_SIZE, RegionKey};
    const REACH: f32 = 3.0;
    let (lo, hi) = sim.region_provider().map(|r| r.bounds()).unwrap_or((glam::IVec2::MIN, glam::IVec2::MAX));
    let span = |a: f32, min: i32, max: i32| {
        (((a - REACH) / CHUNK_SIZE).floor() as i32).max(min)..=(((a + REACH) / CHUNK_SIZE).floor() as i32).min(max)
    };
    span(p.x, lo.x, hi.x).all(|x| span(p.z, lo.y, hi.y).all(|z| sim.state.statics.is_active(RegionKey::Chunk(x, z))))
}
