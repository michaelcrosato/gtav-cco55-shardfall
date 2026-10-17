//! The renderer's sky, time of day, weather, facade windows and night lights over a real city:
//! Paloma Bay (`content/freeway/district.toml`, built by `pav_world`), dressed with the
//! `MAT/gtf/*` facade materials and night-only lamps, seen from the street, at the skyline, from
//! the title screen's drone and into the low sun, plus the four facades side by side, under
//! every `view.atmosphere` preset. Opt-in (slow on software rendering):
//!
//! `cargo test -p pav_tools --test e7_city -- --ignored --nocapture` → `out/e7/city_*.png`
//! (`E7_ONLY=night,rain` limits the presets; the stats of each capture are printed).

use std::sync::Arc;

use anyhow::{Result, anyhow};
use glam::{IVec2, Vec3};
use pav_core::Sim;
use pav_core::games::{self, Entry, Game, GameBox, entry};
use pav_core::params::ChoiceParam;
use pav_core::present::{CameraIntent, FarView, PlayFrame, SurfaceStyle, ToneCurve, ViewIntent};
use pav_core::region::{RegionFill, RegionProvider, StreamSettings};
use pav_core::statics::{Block, Glow};
use pav_tools::{Output, Session};
use pav_view::atmosphere::Atmosphere;
use pav_world::City;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The district, dressed for the renderer: facades on the buildings, lamps that glow at night.
struct Dressed(Arc<City>);

/// A building block's facade, by its size and colour (a stand-in for the game's own choice).
fn facade(b: &Block) -> Option<&'static str> {
    let size = b.max - b.min;
    if size.y < 2.5 || size.x.min(size.z) < 3.0 {
        return None;
    }
    let [r, _, bl] = b.color.0;
    Some(if size.y > 20.0 && bl > r * 1.15 {
        "MAT/gtf/glass_curtain"
    } else if size.y > 16.0 {
        "MAT/gtf/office_concrete"
    } else if size.y > 6.5 && (r - bl).abs() < 0.08 {
        "MAT/gtf/brick_windows"
    } else {
        "MAT/gtf/stucco_windows"
    })
}

impl RegionProvider for Dressed {
    fn bounds(&self) -> (IVec2, IVec2) {
        self.0.bounds()
    }

    fn fill(&self, x: i32, z: i32, out: &mut RegionFill) {
        self.0.fill(x, z, out);
        for b in &mut out.blocks {
            b.material = facade(b).map(String::from);
        }
        for d in out.decor.iter_mut().filter(|d| d.emissive > 0.0) {
            d.glow = Glow::Night;
            d.emissive = 5.0;
        }
    }
}

/// The four facades side by side on a plaza: a 14 m, 3-storey block of each, a tower behind.
struct Facades;

const FACADES: [(&str, &str); 4] = [
    ("MAT/gtf/stucco_windows", "#f1caa1"),
    ("MAT/gtf/brick_windows", "#9a5a44"),
    ("MAT/gtf/office_concrete", "#b8b0a2"),
    ("MAT/gtf/glass_curtain", "#5f7d96"),
];

impl RegionProvider for Facades {
    fn bounds(&self) -> (IVec2, IVec2) {
        (IVec2::new(-1, -2), IVec2::new(1, 0))
    }

    fn fill(&self, x: i32, z: i32, out: &mut RegionFill) {
        if (x, z) != (0, -1) {
            return;
        }
        let plaza = pav_core::Color::hex("#cfc6b4");
        out.blocks.push(Block::new(Vec3::new(-6.0, -0.2, -30.0), Vec3::new(62.0, 0.0, 0.0), plaza));
        for (i, (material, color)) in FACADES.iter().enumerate() {
            let x0 = i as f32 * 14.5;
            let tall = if i == 3 { 34.0 } else { 10.5 };
            let mut b = Block::new(Vec3::new(x0, 0.0, -24.0), Vec3::new(x0 + 13.0, tall, -14.0), pav_core::Color::hex(color));
            b.material = Some(material.to_string());
            out.blocks.push(b);
        }
    }
}

/// A camera fixed on the city, under the golden hour with the art direction's look.
#[derive(Clone, Serialize, Deserialize)]
struct CityShot {
    camera: CameraIntent,
    #[serde(skip)]
    far: Option<Arc<FarView>>,
}

impl Game for CityShot {
    const KEY: &'static str = "e7_city";

    fn present(&self, _sim: &Sim) -> Option<PlayFrame> {
        let mut view: ViewIntent = Atmosphere::Golden.intent().unwrap();
        view.style = Some(SurfaceStyle::Lit);
        view.outlines = Some(false);
        view.tonemap = Some(ToneCurve::Aces);
        view.bloom_threshold = Some(1.0);
        view.haze_height = Some(10.0);
        view.shadow_radius = if self.camera.eye.y > 50.0 { 220.0 } else { 70.0 };
        Some(PlayFrame { camera: Some(self.camera), view: Some(view), far: self.far.clone(), ..Default::default() })
    }
}

fn shot(eye: Vec3, target: Vec3, fov: f32) -> CameraIntent {
    CameraIntent { eye, target, up: Vec3::Y, fov, near: 0.2, far: 2500.0, cut: true, shake: 0.0 }
}

/// `e7_city/street` (west along Sunset Row, into the sunset), `e7_city/skyline` (downtown's
/// towers from the boulevard), `e7_city/drone` (the title screen's shot), `e7_city/facades`
/// (the four facade materials side by side) or `e7_city/sun` (into the golden hour's sun).
fn build(sim: &mut Sim, rest: Option<&str>) -> Result<()> {
    let text = include_str!("../../../content/freeway/district.toml");
    let city = Arc::new(City::from_toml(text).map_err(|e| anyhow!("district: {e}"))?);
    let camera = match rest.unwrap_or("street") {
        "street" => shot(Vec3::new(-96.0, 1.7, -228.5), Vec3::new(-230.0, 7.0, -236.0), 55.0),
        "skyline" => shot(Vec3::new(-150.0, 1.8, -229.0), Vec3::new(-215.0, 34.0, -120.0), 60.0),
        "drone" => shot(Vec3::new(24.4, 125.6, -12.7), Vec3::new(-190.0, 37.5, -96.0), 42.0),
        "facades" => shot(Vec3::new(30.0, 8.0, 22.0), Vec3::new(28.0, 9.0, -19.0), 60.0),
        // Into the golden hour's sun (9° up, bearing 255°), from above the roofs north of downtown.
        "sun" => shot(Vec3::new(-96.0, 40.0, -300.0), Vec3::new(-191.4, 55.6, -274.4), 55.0),
        other => return Err(anyhow!("unknown shot {other}")),
    };
    sim.state.spawn = camera.target;
    sim.state.focus = camera.target;
    sim.state.world.interest = vec![Vec3::new(camera.target.x, 0.0, camera.target.z)];
    let stream = StreamSettings { radius: 160.0, unload: 230.0, lookahead: 1.6, budget: 3 };
    let swatch = rest == Some("facades");
    let provider: Arc<dyn RegionProvider> = if swatch { Arc::new(Facades) } else { Arc::new(Dressed(city.clone())) };
    sim.set_region_provider(provider, stream);
    sim.stream_now();
    let far = (!swatch).then(|| city.far.clone());
    sim.state.play = Some(GameBox::new(CityShot { camera, far }));
    Ok(())
}

static CITY_SHOT: Entry = entry::<CityShot>("Test: Paloma Bay dressed for the renderer, from a fixed camera.", build);

fn call(s: &mut Session, tool: &str, args: Value) -> Result<Output> {
    pav_tools::call(s, tool, args.as_object().unwrap())
}

#[test]
#[ignore = "slow on software rendering: writes out/e7/city_*.png"]
fn the_city_under_every_sky() {
    games::register(&CITY_SHOT);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/e7");
    std::fs::create_dir_all(&dir).unwrap();
    let only = std::env::var("E7_ONLY").ok();
    let wanted = |name: &str| only.as_deref().is_none_or(|o| o.split(',').any(|w| w == name));
    for view in ["street", "skyline", "drone", "facades", "sun"] {
        let mut s = Session::new(&format!("{}/{view}", CityShot::KEY), 1).unwrap();
        call(&mut s, "step", json!({"ticks": 2})).unwrap();
        for (i, name) in Atmosphere::NAMES.iter().enumerate() {
            if !wanted(name) {
                continue;
            }
            s.view.atmosphere = Atmosphere::from_index(i);
            let out = dir.join(format!("city_{view}_{name}.png"));
            let reply = call(&mut s, "capture", json!({"width": 960, "height": 540, "stats": true, "out": out})).unwrap();
            let Output::Image { meta, .. } = reply else { panic!("an image") };
            println!("{view} {name}: {}", meta["stats"]);
        }
    }
}
