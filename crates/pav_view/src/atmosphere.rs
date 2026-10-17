//! Time-of-day and weather presets for looking at a scene (`view.atmosphere`): the scene's own
//! atmosphere (a hook game's `ViewIntent`, or the view settings) is replaced by one of these,
//! so tools can show any scene at dawn, noon, golden hour, dusk, night, in rain, a storm or fog.
//! The palette is Southern California's (sunset gradient #ffb35c → #ff5e7e → #6a4c93, sodium
//! lamps, a blue night). A game keeps its own time of day; these are a reference and a preview.

use glam::{Vec2, Vec3};
use pav_core::Color;
use pav_core::present::{FogIntent, SkyIntent, ViewIntent};

pav_core::choice_enum! {
    /// `view.atmosphere`: the scene's own atmosphere, or a preset in its place.
    #[derive(Default)]
    pub enum Atmosphere {
        #[default]
        Scene => "scene",
        Dawn => "dawn",
        Noon => "noon",
        Golden => "golden",
        Dusk => "dusk",
        Night => "night",
        Rain => "rain",
        Storm => "storm",
        Lightning => "lightning",
        Fog => "fog",
    }
}

/// Direction toward a sky body `elevation` degrees above the horizon, at compass `azimuth`
/// degrees (0 north = -z, 90 east = +x), as the view's light settings measure them.
pub fn toward(elevation: f32, azimuth: f32) -> Vec3 {
    let (el, az) = (elevation.to_radians(), azimuth.to_radians());
    Vec3::new(az.sin() * el.cos(), el.sin(), -az.cos() * el.cos())
}

fn hex(s: &str) -> Color {
    Color::hex(s)
}

/// The fields every preset shares: a sky and fog in the horizon's colour, no weather.
#[allow(clippy::too_many_arguments)]
fn base(to_sun: Vec3, sun: &str, intensity: f32, zenith: &str, horizon: &str, ground: &str, glow: &str) -> ViewIntent {
    ViewIntent {
        sun_dir: -to_sun,
        sun_color: hex(sun),
        sun_intensity: intensity,
        sky: Some(SkyIntent {
            zenith: hex(zenith),
            horizon: hex(horizon),
            ground: hex(ground),
            glow: hex(glow),
            sun_size: 0.03,
            moon_dir: None,
            stars: 0.0,
            clouds: 0.2,
            cloud_color: hex("#ffffff"),
            lightning: 0.0,
        }),
        fog: Some(FogIntent { color: hex(horizon), start: 220.0, end: 1100.0 }),
        exposure: 1.0,
        bloom: 0.4,
        shafts: 1.0,
        wind: Vec2::new(1.5, 0.6),
        ..Default::default()
    }
}

impl Atmosphere {
    /// The preset's atmosphere, or None for the scene's own.
    pub fn intent(self) -> Option<ViewIntent> {
        let mut v = match self {
            Atmosphere::Scene => return None,
            Atmosphere::Dawn => {
                let mut v = base(toward(5.0, 95.0), "#ffc29a", 0.85, "#6a86b8", "#f4c9b0", "#6e6766", "#ffb48a");
                v.ambient_sky = hex("#93a3c6");
                v.ambient_ground = hex("#7d6c66");
                v.ambient = 0.75;
                let s = v.sky.as_mut().unwrap();
                s.clouds = 0.28;
                s.cloud_color = hex("#ffd8c8");
                s.stars = 0.05;
                v.fog = Some(FogIntent { color: hex("#f4c9b0"), start: 160.0, end: 950.0 });
                v.exposure = 1.1;
                v.temperature = 0.06;
                v.street_lights = 0.4;
                v.windows = 0.3;
                v
            }
            Atmosphere::Noon => {
                let mut v = base(toward(66.0, 200.0), "#fff3e0", 1.3, "#3d72c2", "#c6daec", "#8a8378", "#fff4dc");
                v.ambient_sky = hex("#a8bfdc");
                v.ambient_ground = hex("#8f8070");
                v.ambient = 0.95;
                let s = v.sky.as_mut().unwrap();
                s.clouds = 0.14;
                s.cloud_color = hex("#ffffff");
                v.fog = Some(FogIntent { color: hex("#c6daec"), start: 260.0, end: 1300.0 });
                v.saturation = 1.0;
                v
            }
            Atmosphere::Golden => {
                let mut v = base(toward(9.0, 255.0), "#ffc27a", 1.25, "#4b78b6", "#ecc89c", "#6b5a4a", "#ffb35c");
                v.ambient_sky = hex("#93a6c6");
                v.ambient_ground = hex("#8a6a52");
                v.ambient = 0.8;
                let s = v.sky.as_mut().unwrap();
                s.clouds = 0.24;
                s.cloud_color = hex("#ffe0bc");
                v.fog = Some(FogIntent { color: hex("#ecc89c"), start: 220.0, end: 1100.0 });
                v.temperature = 0.16;
                v.saturation = 1.1;
                v.windows = 0.05;
                v
            }
            Atmosphere::Dusk => {
                // Magic hour: the sun just under the horizon, a pink band under a violet sky.
                let mut v = base(toward(1.5, 262.0), "#ff9f86", 0.4, "#4a3f86", "#ff8a7e", "#4a3f4f", "#ffb35c");
                v.ambient_sky = hex("#7d6cae");
                v.ambient_ground = hex("#5c4a52");
                v.ambient = 0.62;
                let s = v.sky.as_mut().unwrap();
                s.clouds = 0.3;
                s.cloud_color = hex("#ff9f8f");
                s.stars = 0.2;
                v.fog = Some(FogIntent { color: hex("#ff8a7e"), start: 180.0, end: 1000.0 });
                v.exposure = 1.15;
                v.bloom = 0.5;
                v.saturation = 1.12;
                v.street_lights = 0.85;
                v.windows = 0.55;
                v
            }
            Atmosphere::Night => {
                // The moon lights the city; sodium lamps, neon and windows do the rest.
                let moon = toward(38.0, 140.0);
                // The horizon keeps a little of the city's sodium glow.
                let mut v = base(moon, "#9fb6e6", 0.12, "#060a1a", "#272842", "#0c0e15", "#141c34");
                v.ambient_sky = hex("#2c3a60");
                v.ambient_ground = hex("#1a1a22");
                v.ambient = 0.4;
                let s = v.sky.as_mut().unwrap();
                s.moon_dir = Some(moon);
                s.stars = 0.7;
                s.clouds = 0.16;
                s.cloud_color = hex("#262c44");
                v.fog = Some(FogIntent { color: hex("#272842"), start: 160.0, end: 900.0 });
                v.exposure = 1.15;
                v.bloom = 0.55;
                v.temperature = -0.08;
                v.street_lights = 1.0;
                v.windows = 0.8;
                v
            }
            Atmosphere::Rain | Atmosphere::Storm | Atmosphere::Lightning => {
                let storm = self != Atmosphere::Rain;
                let (zenith, horizon, clouds) =
                    if storm { ("#2c3138", "#4c535b", "#3b4249") } else { ("#5c656f", "#8b939b", "#6f767e") };
                let mut v =
                    base(toward(40.0, 210.0), "#c9cdd4", if storm { 0.12 } else { 0.3 }, zenith, horizon, "#3e4044", "#8a9098");
                v.ambient_sky = hex(if storm { "#56606c" } else { "#8c949e" });
                v.ambient_ground = hex("#55575a");
                v.ambient = if storm { 0.75 } else { 1.0 };
                let s = v.sky.as_mut().unwrap();
                s.sun_size = 0.0;
                s.clouds = if storm { 1.0 } else { 0.92 };
                s.cloud_color = hex(clouds);
                s.lightning = if self == Atmosphere::Lightning { 1.0 } else { 0.0 };
                let (start, end) = if storm { (50.0, 420.0) } else { (80.0, 520.0) };
                v.fog = Some(FogIntent { color: hex(horizon), start, end });
                v.exposure = 1.1;
                v.saturation = 0.85;
                v.wetness = if storm { 1.0 } else { 0.8 };
                v.rain = if storm { 12.0 } else { 6.0 };
                v.wind = if storm { Vec2::new(6.0, 2.0) } else { Vec2::new(2.5, 1.0) };
                v.street_lights = if storm { 0.7 } else { 0.3 };
                v.windows = if storm { 0.5 } else { 0.3 };
                v
            }
            Atmosphere::Fog => {
                let mut v = base(toward(24.0, 150.0), "#e9e4da", 0.45, "#a4adb4", "#bcc2c6", "#8c9092", "#f4efe4");
                v.ambient_sky = hex("#b4bcc2");
                v.ambient_ground = hex("#8a8a88");
                v.ambient = 0.95;
                let s = v.sky.as_mut().unwrap();
                s.clouds = 0.6;
                s.cloud_color = hex("#c6cacc");
                v.fog = Some(FogIntent { color: hex("#bcc2c6"), start: 18.0, end: 220.0 });
                v.saturation = 0.8;
                v.wetness = 0.25;
                v.street_lights = 0.25;
                v.windows = 0.15;
                v
            }
        };
        v.shadow_radius = 70.0;
        Some(v)
    }
}
