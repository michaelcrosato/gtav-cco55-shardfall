//! Ambience beds (PLAN.md §5.11 audio): the city's hum, the crowd's murmur near people, the surf
//! at the beach, the harbour by the docks, wind on the hills and the rain, mixed from where the
//! ears are (the zones around the listener, its height) and who is about (people close by), as
//! non-positional loops the host fades in and out and glides between.

use glam::{Vec2, Vec3};
use pav_core::present::LoopSource;
use pav_world::City;

/// The beds' loop ids (stable, so the host glides rather than restarts them).
const BED_ID: u64 = 0xbed0_0000;

/// What the beds hear: where the ears are, how many people are near, the weather.
pub struct Ears<'a> {
    pub city: &'a City,
    pub at: Vec3,
    /// People within 25 m.
    pub crowd: usize,
    /// 0..1: rain, wind (m/s), and night (1 at midnight) for a quieter city.
    pub rain: f32,
    pub wind: f32,
    pub night: f32,
}

/// The share of samples around `at` (a 5 × 5 grid `spacing` apart) whose zone is one of `zones`.
fn share(city: &City, at: Vec2, spacing: f32, zones: &[&str]) -> f32 {
    let mut n = 0;
    for z in -2..=2 {
        for x in -2..=2 {
            if city.zone_at(at + Vec2::new(x as f32, z as f32) * spacing).is_some_and(|zone| zones.contains(&zone)) {
                n += 1;
            }
        }
    }
    n as f32 / 25.0
}

/// The beds to play now, with their volumes (0..1).
pub fn beds(e: &Ears) -> Vec<LoopSource> {
    let at = Vec2::new(e.at.x, e.at.z);
    let city = e.city;
    let busy = share(city, at, 30.0, &["downtown", "commercial", "shops", "corridor"]);
    let homes = share(city, at, 30.0, &["residential", "industrial", "park", "missions", "lot", "yard"]);
    // Traffic far off is always there; downtown it is a roar, at night a murmur.
    let city_hum = (0.18 + 0.5 * busy + 0.2 * homes) * (1.0 - 0.45 * e.night);
    let surf = share(city, at, 45.0, &["beach", "water"]).powf(0.7);
    let harbor = share(city, at, 40.0, &["docks"]) * (1.0 - 0.3 * e.night);
    let high = ((e.at.y - 15.0) / 25.0).clamp(0.0, 1.0);
    let wind = (share(city, at, 40.0, &["hills", "wild"]) * 0.5 + high * 0.6 + (e.wind / 12.0).min(1.0) * 0.4).min(1.0);
    let crowd = (e.crowd as f32 / 10.0).min(1.0) * 0.7;
    let rain = e.rain.clamp(0.0, 1.0) * 0.8;
    [
        ("amb.city", city_hum),
        ("amb.surf", surf * 0.8),
        ("amb.harbor", harbor * 0.7),
        ("amb.wind", wind * 0.6),
        ("amb.crowd", crowd),
        ("amb.rain", rain),
    ]
    .into_iter()
    .enumerate()
    .filter(|(_, (_, g))| *g > 0.02)
    .map(|(i, (cue, gain))| LoopSource { id: BED_ID + i as u64, cue: cue.into(), pos: None, gain, pitch: 1.0, param: 0.0 })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gain(beds: &[LoopSource], cue: &str) -> f32 {
        beds.iter().find(|b| b.cue == cue).map_or(0.0, |b| b.gain)
    }

    #[test]
    fn the_beds_follow_the_place_and_the_crowd() {
        let city = crate::world::city().unwrap();
        let ears = |at: Vec3, crowd: usize| Ears { city: &city, at, crowd, rain: 0.0, wind: 2.0, night: 0.0 };
        let downtown = beds(&ears(Vec3::new(-150.0, 1.7, -104.0), 12));
        let beach = beds(&ears(Vec3::new(-400.0, 1.9, 296.0), 0));
        let hills = beds(&ears(Vec3::new(-262.0, 29.6, -570.0), 0));
        assert!(gain(&downtown, "amb.city") > gain(&beach, "amb.city"), "{downtown:?} {beach:?}");
        assert!(gain(&downtown, "amb.crowd") > 0.5);
        assert!(gain(&beach, "amb.surf") > 0.3, "{beach:?}");
        assert!(gain(&hills, "amb.wind") > gain(&downtown, "amb.wind"), "{hills:?}");
        assert_eq!(gain(&downtown, "amb.rain"), 0.0);
        let wet = beds(&Ears { rain: 1.0, ..ears(Vec3::new(-150.0, 1.7, -104.0), 0) });
        assert!(gain(&wet, "amb.rain") > 0.5);
    }
}
