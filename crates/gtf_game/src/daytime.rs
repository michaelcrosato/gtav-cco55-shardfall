//! Time of day and weather (PLAN.md §5.6.12, §5.11): the game clock, a weather state machine and
//! the look they make together — the `ViewIntent` the view draws: sun and moon from the hour,
//! sky, fog, light and grade blended between the presets of `content/freeway/world.toml`, then
//! changed by the weather (clouds, fog, rain, wet roads, lightning). State is in the game struct
//! and randomness comes from the simulation's generator, so it replays exactly.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use glam::{Vec2, Vec3};
use pav_core::Color;
use pav_core::frame::SimEvent;
use pav_core::present::{FogIntent, SkyIntent, SurfaceStyle, ToneCurve, ViewIntent};
use pav_core::rng::Rng;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldDef {
    pub day_length: f32,
    pub start: String,
    #[serde(default = "lat")]
    pub latitude: f32,
    pub preset: Vec<Preset>,
    pub weather: BTreeMap<String, WeatherDef>,
    #[serde(default)]
    pub look: LookDef,
}

/// The picture's look for the whole game, over the session's view settings (unset keeps them).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookDef {
    pub style: Option<SurfaceStyle>,
    pub outlines: Option<bool>,
    pub tonemap: Option<ToneCurve>,
    pub bloom_threshold: Option<f32>,
    pub haze_height: Option<f32>,
    pub gi: Option<f32>,
}

fn lat() -> f32 {
    34.0
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LightDef {
    pub color: String,
    pub intensity: f32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkyDef {
    pub zenith: String,
    pub horizon: String,
    pub ground: String,
    pub glow: String,
    #[serde(default)]
    pub stars: f32,
    #[serde(default)]
    pub clouds: f32,
    pub cloud: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AmbientDef {
    pub sky: String,
    pub ground: String,
    pub level: f32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FogDef {
    pub color: String,
    pub start: f32,
    pub end: f32,
}

fn one() -> f32 {
    1.0
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preset {
    pub name: String,
    pub hour: f32,
    pub sun: LightDef,
    #[serde(default)]
    pub moon: Option<LightDef>,
    pub sky: SkyDef,
    pub ambient: AmbientDef,
    pub fog: FogDef,
    #[serde(default = "one")]
    pub exposure: f32,
    #[serde(default)]
    pub bloom: f32,
    #[serde(default)]
    pub haze: f32,
    #[serde(default)]
    pub temperature: f32,
    #[serde(default)]
    pub tint: f32,
    #[serde(default = "one")]
    pub saturation: f32,
    #[serde(default = "one")]
    pub contrast: f32,
    #[serde(default)]
    pub vignette: f32,
    #[serde(default)]
    pub street_lights: f32,
    #[serde(default)]
    pub windows: f32,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct WeatherDef {
    /// Cloud cover (replaces the hour's when higher) and their colour.
    pub clouds: Option<f32>,
    pub cloud: Option<String>,
    /// Multipliers: fog distances, sunlight, saturation; additions: haze.
    pub fog: Option<f32>,
    pub light: Option<f32>,
    pub saturation: Option<f32>,
    pub haze: f32,
    /// Rain near the camera, wind (m/s) and lightning flashes per second.
    pub rain: f32,
    pub wind: f32,
    pub lightning: f32,
    /// How long it lasts (game hours) and what may follow (weights).
    pub hours: [f32; 2],
    pub next: BTreeMap<String, f32>,
}

/// The world file, parsed once per version of its text.
pub fn def() -> Result<Arc<WorldDef>> {
    static CACHE: Mutex<Option<(String, Arc<WorldDef>)>> = Mutex::new(None);
    let text = crate::content::text("world.toml", include_str!("../../../content/freeway/world.toml"));
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((t, d)) = cache.as_ref() {
        if *t == *text {
            return Ok(d.clone());
        }
    }
    let mut d: WorldDef = toml::from_str(&text).map_err(|e| anyhow!("content/freeway/world.toml: {e}"))?;
    if d.preset.is_empty() {
        return Err(anyhow!("content/freeway/world.toml: no presets"));
    }
    d.preset.sort_by(|a, b| a.hour.total_cmp(&b.hour));
    if !d.weather.contains_key("clear") {
        return Err(anyhow!("content/freeway/world.toml: no [weather.clear]"));
    }
    for (name, w) in &d.weather {
        for n in w.next.keys() {
            if !d.weather.contains_key(n) {
                return Err(anyhow!("content/freeway/world.toml: weather.{name} leads to unknown '{n}'"));
            }
        }
    }
    let d = Arc::new(d);
    *cache = Some((text.into_owned(), d.clone()));
    Ok(d)
}

/// "HH:MM" as hours.
pub fn parse_time(s: &str) -> Option<f32> {
    let (h, m) = s.trim().split_once(':')?;
    let (h, m): (f32, f32) = (h.parse().ok()?, m.parse().ok()?);
    ((0.0..24.0).contains(&h) && (0.0..60.0).contains(&m)).then_some(h + m / 60.0)
}

/// Hours as "HH:MM".
pub fn format_time(hour: f32) -> String {
    let m = (hour.rem_euclid(24.0) * 60.0).round() as i32 % (24 * 60);
    format!("{:02}:{:02}", m / 60, m % 60)
}

/// The clock and the weather (part of the game's state).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sky {
    /// Game hours, 0..24.
    pub hour: f32,
    /// Clock speed (1: the world file's day length; 0: stopped).
    pub speed: f32,
    pub weather: String,
    /// Blending from `previous` into `weather` (0..1).
    pub previous: String,
    pub blend: f32,
    /// Game hours until the weather may change; held by a command or a view.
    pub left: f32,
    pub held: bool,
    /// How wet the ground is (0..1) and the current lightning flash (0..1).
    pub wet: f32,
    pub flash: f32,
    /// Thunder waiting to be heard (seconds) and how loud.
    pub thunder: Vec<(f32, f32)>,
}

impl Sky {
    pub fn new(def: &WorldDef) -> Sky {
        Sky {
            hour: parse_time(&def.start).unwrap_or(17.5),
            speed: 1.0,
            weather: "clear".into(),
            previous: "clear".into(),
            blend: 1.0,
            left: 8.0,
            held: false,
            wet: 0.0,
            flash: 0.0,
            thunder: Vec::new(),
        }
    }

    /// Sets the weather now (`held`: it stays until changed again).
    pub fn set_weather(&mut self, def: &WorldDef, name: &str, held: bool) -> Result<()> {
        if !def.weather.contains_key(name) {
            let names: Vec<&str> = def.weather.keys().map(|s| s.as_str()).collect();
            return Err(anyhow!("no weather '{name}' (weathers: {})", names.join(", ")));
        }
        self.previous = name.into();
        self.weather = name.into();
        self.blend = 1.0;
        self.held = held;
        let w = &def.weather[name];
        self.wet = if w.rain > 0.0 { 0.8 } else { self.wet.min(0.3) };
        Ok(())
    }

    /// Advances the clock and the weather by `dt` real seconds.
    pub fn step(&mut self, def: &WorldDef, dt: f32, rng: &mut Rng, events: &mut Vec<SimEvent>) {
        let hours = dt * 24.0 / def.day_length.max(1.0) * self.speed;
        self.hour = (self.hour + hours).rem_euclid(24.0);
        if self.blend < 1.0 {
            self.blend = (self.blend + dt / 45.0).min(1.0);
        }
        if !self.held && self.speed > 0.0 {
            self.left -= hours;
            if self.left <= 0.0 {
                let cur = def.weather.get(&self.weather).cloned().unwrap_or_default();
                let total: f32 = cur.next.values().sum();
                let mut roll = rng.f32() * total;
                let mut pick = self.weather.clone();
                for (name, w) in &cur.next {
                    roll -= w;
                    if roll <= 0.0 {
                        pick = name.clone();
                        break;
                    }
                }
                self.previous = std::mem::replace(&mut self.weather, pick);
                self.blend = 0.0;
                let w = def.weather.get(&self.weather).cloned().unwrap_or_default();
                let [lo, hi] = w.hours;
                self.left = if hi > lo { rng.range(lo, hi) } else { lo.max(1.0) };
            }
        }
        // Rain wets the ground in half a minute; it dries in a few.
        let rain = self.mix(def, |w| w.rain);
        self.wet = if rain > 0.05 { (self.wet + dt * 0.035 * rain.min(1.5)).min(1.0) } else { (self.wet - dt * 0.004).max(0.0) };
        // Lightning, then its thunder a few seconds later.
        self.flash = (self.flash - dt * 6.0).max(0.0);
        let rate = self.mix(def, |w| w.lightning);
        if rate > 0.0 && rng.f32() < rate * dt {
            self.flash = 1.0;
            let distance = rng.range(0.4, 4.0);
            self.thunder.push((distance, (1.2 - distance * 0.2).clamp(0.3, 1.0)));
        }
        self.thunder.retain_mut(|(t, gain)| {
            *t -= dt;
            if *t <= 0.0 {
                events.push(SimEvent::Sound { cue: "weather.thunder".into(), pos: None, gain: *gain, pitch: 1.0 });
                false
            } else {
                true
            }
        });
    }

    /// How hard it is raining now (the weather's `rain`, blended between weathers).
    pub fn rain(&self, def: &WorldDef) -> f32 {
        self.mix(def, |w| w.rain)
    }

    /// A weather value blended between the previous weather and this one.
    fn mix(&self, def: &WorldDef, f: impl Fn(&WeatherDef) -> f32) -> f32 {
        let a = def.weather.get(&self.previous).map(&f).unwrap_or(0.0);
        let b = def.weather.get(&self.weather).map(&f).unwrap_or(0.0);
        a + (b - a) * self.blend
    }

    /// The direction toward the sun at this hour (sunrise in the east, noon to the south).
    pub fn to_sun(&self, def: &WorldDef) -> Vec3 {
        let lat = def.latitude.to_radians();
        let decl = 20f32.to_radians();
        // Solar noon at 13:00 (summer time).
        let ha = ((self.hour - 13.0) / 24.0) * std::f32::consts::TAU;
        let sin_el = lat.sin() * decl.sin() + lat.cos() * decl.cos() * ha.cos();
        let el = sin_el.clamp(-1.0, 1.0).asin();
        let cos_az = (decl.sin() - el.sin() * lat.sin()) / (el.cos() * lat.cos()).max(1e-4);
        let mut az = cos_az.clamp(-1.0, 1.0).acos();
        if ha > 0.0 {
            az = std::f32::consts::TAU - az;
        }
        // Azimuth from north, clockwise: x east, z south (north is -z).
        Vec3::new(az.sin() * el.cos(), el.sin(), -az.cos() * el.cos()).normalize()
    }

    /// The look of this moment.
    pub fn view(&self, def: &WorldDef) -> ViewIntent {
        let (a, b, t) = around(def, self.hour);
        let c = |x: &str, y: &str| lerp(col(x), col(y), t);
        let f = |x: f32, y: f32| x + (y - x) * t;
        let w = |g: &dyn Fn(&WeatherDef) -> Option<f32>, default: f32| self.mix(def, |wd| g(wd).unwrap_or(default));
        let light = w(&|wd| wd.light, 1.0);
        let fog_k = w(&|wd| wd.fog, 1.0);
        let sat_k = w(&|wd| wd.saturation, 1.0);
        let wclouds = w(&|wd| wd.clouds, 0.0);
        let gloom = (1.0 - light).clamp(0.0, 1.0);
        // The weather's cloud colour, blended in by how cloudy it is.
        let cloud_w = {
            let prev = def.weather.get(&self.previous).and_then(|x| x.cloud.clone());
            let cur = def.weather.get(&self.weather).and_then(|x| x.cloud.clone());
            match (prev, cur) {
                (Some(p), Some(q)) => Some(lerp(col(&p), col(&q), self.blend)),
                (None, Some(q)) => Some(col(&q)),
                (Some(p), None) => Some(col(&p)),
                (None, None) => None,
            }
        };
        let grey = |x: Color| match cloud_w {
            Some(g) => lerp(x, lerp(g, x, 0.35), (gloom * 1.4).min(0.85)),
            None => x,
        };
        let to_sun = self.to_sun(def);
        let sun_up = to_sun.y > -0.04;
        let sun_i = f(a.sun.intensity, b.sun.intensity) * light * ((to_sun.y + 0.04) * 8.0).clamp(0.0, 1.0);
        let moon_light = |p: &Preset| p.moon.as_ref().map(|m| (col(&m.color), m.intensity)).unwrap_or((col("#8fa3c8"), 0.0));
        let ((ma, mia), (mb, mib)) = (moon_light(a), moon_light(b));
        let moon_i = (mia + (mib - mia) * t) * light.max(0.5);
        let to_moon = Vec3::new(-to_sun.x, (-to_sun.y).max(0.35), -to_sun.z).normalize();
        // The stronger of the two lights the scene.
        let (dir, color, intensity) = if sun_up && sun_i >= moon_i {
            (to_sun, c(&a.sun.color, &b.sun.color), sun_i)
        } else {
            (to_moon, lerp(ma, mb, t), moon_i)
        };
        let horizon = grey(c(&a.sky.horizon, &b.sky.horizon));
        let zenith = grey(c(&a.sky.zenith, &b.sky.zenith));
        let clouds = f(a.sky.clouds, b.sky.clouds).max(wclouds);
        let rain = self.mix(def, |wd| wd.rain);
        let wind_speed = self.mix(def, |wd| wd.wind) + 1.0;
        // Wind off the ocean (from the south-west), veering a little through the day.
        let wa = 0.8 + (self.hour / 24.0 * std::f32::consts::TAU).sin() * 0.3;
        let flash = self.flash;
        let fog_color = horizon;
        ViewIntent {
            sun_dir: -dir,
            sun_color: color,
            sun_intensity: intensity * (1.0 + flash * 0.6),
            ambient_sky: lerp(grey(c(&a.ambient.sky, &b.ambient.sky)), col("#d8e2ff"), flash * 0.6),
            ambient_ground: c(&a.ambient.ground, &b.ambient.ground),
            ambient: f(a.ambient.level, b.ambient.level) * (0.8 + 0.2 * light) + flash * 0.5,
            sky: Some(SkyIntent {
                zenith,
                horizon,
                ground: c(&a.sky.ground, &b.sky.ground),
                glow: grey(c(&a.sky.glow, &b.sky.glow)),
                sun_size: 0.028,
                moon_dir: (!sun_up || to_sun.y < 0.15).then_some(to_moon),
                stars: f(a.sky.stars, b.sky.stars) * (1.0 - clouds * 0.9),
                clouds,
                cloud_color: match cloud_w {
                    Some(g) => lerp(c(&a.sky.cloud, &b.sky.cloud), g, (wclouds * 1.2).min(1.0)),
                    None => c(&a.sky.cloud, &b.sky.cloud),
                },
                lightning: flash,
            }),
            fog: Some(FogIntent {
                color: fog_color,
                start: f(a.fog.start, b.fog.start) * fog_k,
                end: f(a.fog.end, b.fog.end) * fog_k.max(0.3),
            }),
            exposure: f(a.exposure, b.exposure) * (1.0 + gloom * 0.25),
            bloom: f(a.bloom, b.bloom),
            haze: f(a.haze, b.haze) + self.mix(def, |wd| wd.haze),
            shafts: 1.0,
            temperature: f(a.temperature, b.temperature) * (1.0 - gloom * 0.7),
            tint: f(a.tint, b.tint),
            contrast: f(a.contrast, b.contrast),
            saturation: f(a.saturation, b.saturation) * sat_k,
            vignette: f(a.vignette, b.vignette),
            wetness: self.wet,
            rain,
            wind: Vec2::new(wa.sin(), wa.cos()) * wind_speed,
            street_lights: f(a.street_lights, b.street_lights).max(((gloom - 0.45) * 2.5).clamp(0.0, 1.0)),
            windows: f(a.windows, b.windows).max(((gloom - 0.5) * 1.5).clamp(0.0, 0.6)),
            shadow_radius: 70.0,
            style: def.look.style,
            outlines: def.look.outlines,
            tonemap: def.look.tonemap,
            bloom_threshold: def.look.bloom_threshold,
            haze_height: def.look.haze_height,
            gi: def.look.gi,
            ..Default::default()
        }
    }
}

fn col(hex: &str) -> Color {
    Color::hex(hex)
}

fn lerp(a: Color, b: Color, t: f32) -> Color {
    Color([a.0[0] + (b.0[0] - a.0[0]) * t, a.0[1] + (b.0[1] - a.0[1]) * t, a.0[2] + (b.0[2] - a.0[2]) * t])
}

/// The presets on either side of `hour` (around midnight too) and how far between them.
fn around(def: &WorldDef, hour: f32) -> (&Preset, &Preset, f32) {
    let p = &def.preset;
    let n = p.len();
    let i = p.iter().rposition(|x| x.hour <= hour).unwrap_or(n - 1);
    let (a, b) = (&p[i], &p[(i + 1) % n]);
    let span = (b.hour - a.hour).rem_euclid(24.0).max(1e-3);
    let into = (hour - a.hour).rem_euclid(24.0);
    let t = (into / span).clamp(0.0, 1.0);
    // Ease, so each preset holds a little around its hour.
    (a, b, t * t * (3.0 - 2.0 * t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_parse_and_print() {
        assert_eq!(parse_time("21:30"), Some(21.5));
        assert_eq!(parse_time("24:00"), None);
        assert_eq!(format_time(6.25), "06:15");
        assert_eq!(format_time(23.999), "00:00");
    }

    #[test]
    fn the_sun_rises_east_and_sets_west() {
        let d = def().unwrap();
        let mut s = Sky::new(&d);
        s.hour = 7.0;
        let morning = s.to_sun(&d);
        assert!(morning.x > 0.3 && morning.y > 0.0, "{morning:?}");
        s.hour = 13.0;
        let noon = s.to_sun(&d);
        assert!(noon.y > 0.9 && noon.z > 0.0, "high and to the south: {noon:?}");
        s.hour = 19.5;
        assert!(s.to_sun(&d).x < -0.3);
        s.hour = 1.0;
        assert!(s.to_sun(&d).y < 0.0, "below the horizon at night");
    }

    #[test]
    fn night_lights_the_city_and_rain_wets_it() {
        let d = def().unwrap();
        let mut s = Sky::new(&d);
        s.hour = 22.0;
        let v = s.view(&d);
        assert!(v.street_lights > 0.9 && v.windows > 0.9);
        assert!(v.sky.as_ref().unwrap().stars > 0.5);
        s.hour = 13.0;
        assert!(s.view(&d).street_lights < 0.1);
        s.set_weather(&d, "rain", true).unwrap();
        let mut rng = Rng::new(1);
        let mut ev = Vec::new();
        for _ in 0..600 {
            s.step(&d, 1.0 / 60.0, &mut rng, &mut ev);
        }
        let v = s.view(&d);
        assert!(v.rain > 0.5 && v.wetness > 0.8, "rain {} wet {}", v.rain, v.wetness);
        assert!(s.set_weather(&d, "snow", true).is_err());
    }

    #[test]
    fn weather_changes_by_itself_and_replays() {
        let d = def().unwrap();
        let run = || {
            let mut s = Sky::new(&d);
            s.speed = 200.0;
            let mut rng = Rng::new(7);
            let mut seen = std::collections::BTreeSet::new();
            let mut ev = Vec::new();
            for _ in 0..20_000 {
                s.step(&d, 1.0 / 60.0, &mut rng, &mut ev);
                seen.insert(s.weather.clone());
            }
            (seen, format!("{:.4} {}", s.hour, s.weather))
        };
        let (seen, end) = run();
        assert!(seen.len() >= 3, "{seen:?}");
        assert_eq!(run().1, end, "same seed, same weather");
    }
}
