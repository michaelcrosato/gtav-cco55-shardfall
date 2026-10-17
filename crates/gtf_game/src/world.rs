//! Paloma Bay: the district `pav_world` builds from `content/freeway/district.toml`, streamed
//! around the player and the camera, with its landmarks on the map.

use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use glam::{Vec2, Vec3};
use pav_core::Color;
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

/// The district a scene mode plays in.
pub fn city_for(mode: &str) -> Result<Arc<City>> {
    if mode == "dev" { dev_city() } else { city() }
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

/// Ground-plane position of a world point (the map's coordinates).
pub fn flat(p: Vec3) -> Vec2 {
    Vec2::new(p.x, p.z)
}
