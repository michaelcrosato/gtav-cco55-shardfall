//! Grand Theft Freeway-BX — the game's rules and content runtime on the Pavilion engine's game
//! hook (`pav_core::games`). Simulation only: no graphics, window or audio code (PLAN.md §6.2);
//! everything the game shows and plays leaves as data in its `PlayFrame` (`present`). The
//! binaries in `gtf_app` make it known to the engine with [`register`].

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use glam::{Vec2, Vec3};
use pav_core::entity::EntityId;
use pav_core::frame::SimEvent;
use pav_core::games::{Entry, Game, GameBox, entry};
use pav_core::present::*;
use pav_core::room::RoomDef;
use pav_core::sim::Sim;
use pav_core::{Color, InputFrame};
use serde::{Deserialize, Serialize};
use serde_json::json;

pub mod about;
pub mod camera;
pub mod content;
pub mod frontend;
pub mod gallery;
pub mod hud;
pub mod map;
pub mod pad;
pub mod player;
pub mod protagonists;
pub mod settings;

use camera::{Look, OrbitCam};
use frontend::{Action, Ctx, Front, Page};
use hud::{Note, WEAPONS, WheelState};
use settings::Settings;

/// The game's scene key: `load scene=freeway` (the frontend), `freeway/roam` (straight into
/// free roam), `freeway/dev` (the Proving Grounds), `freeway/attract`, `freeway/gallery`.
pub const KEY: &str = "freeway";

/// The game's line in the engine's game list.
pub static ENTRY: Entry = entry::<Freeway>(
    "Grand Theft Freeway-BX: an open-world crime caper in Paloma Bay (freeway/roam, freeway/dev, freeway/attract).",
    build,
);

/// Makes the game known to this process: call it before building a scene or loading a snapshot.
pub fn register() {
    pav_core::games::register(&ENTRY);
}

/// Seconds the weapon wheel button must be held before the wheel opens (a tap swaps to the last
/// weapon).
const WHEEL_DELAY: f32 = 0.18;

/// The game's state (everything that changes during play lives here or in entities).
#[derive(Clone, Serialize, Deserialize)]
pub struct Freeway {
    /// The sub-scene this run was built for (`roam`, `dev`, `attract`, `gallery`; empty = the
    /// frontend).
    pub mode: String,
    /// Seconds of game time (slowed by slow motion, stopped by the pause menu).
    pub time: f32,
    /// Seconds of real time since the scene was built (the UI's clock).
    pub ui_time: f32,
    pub front: Front,
    pub cam: OrbitCam,
    pub settings: Settings,
    /// Bumped on every settings change: the host keeps the newest.
    pub settings_serial: u64,
    /// Where the host runs (`host.platform`).
    pub web: bool,
    pub touch: bool,
    /// Save slots the host has (`host.save`).
    pub saves: Vec<String>,
    pub wheel: Option<WheelState>,
    pub weapon: usize,
    pub last_weapon: usize,
    pub money: i64,
    pub money_delta: i64,
    pub money_age: f32,
    pub notes: Vec<Note>,
    pub next_note: u64,
    /// Seconds since free roam began (the first tip shows for a while).
    pub roam_age: f32,
    pub quit: bool,
    /// The map picture (rebuilt from the level when missing, e.g. after loading a snapshot; a
    /// cache, so not part of the state).
    #[serde(skip)]
    map: Option<Arc<MapImage>>,
    #[serde(skip)]
    settings_json: Option<Arc<String>>,
}

/// Builds a scene of the game. M1: the frontend over a placeholder lot; the generated district
/// arrives in M2.
pub fn build(sim: &mut Sim, rest: Option<&str>) -> Result<()> {
    let mode = rest.unwrap_or("").to_string();
    let text = content::text("m0_lot.toml", include_str!("../../../content/freeway/m0_lot.toml"));
    let def = RoomDef::parse(&text).map_err(|e| anyhow!("content/freeway/m0_lot.toml: {e}"))?;
    pav_core::scenes::build_standalone_room(sim, KEY, def);
    let mut front = Front::default();
    front.page = match mode.as_str() {
        "roam" | "dev" => Page::Play,
        "gallery" => {
            front.back = Page::Title;
            Page::Gallery
        }
        "attract" => Page::Title,
        _ => Page::Boot,
    };
    let mut cam = OrbitCam::default();
    // The player starts facing south: look over their shoulder.
    cam.face(std::f32::consts::PI);
    let game = Freeway {
        mode,
        time: 0.0,
        ui_time: 0.0,
        front,
        cam,
        settings: Settings::default(),
        settings_serial: 0,
        web: false,
        touch: false,
        saves: Vec::new(),
        wheel: None,
        weapon: 0,
        last_weapon: 1,
        money: 12_480,
        money_delta: 0,
        money_age: 99.0,
        notes: Vec::new(),
        next_note: 1,
        roam_age: 0.0,
        quit: false,
        map: None,
        settings_json: None,
    };
    sim.state.play = Some(GameBox::new(game));
    Ok(())
}

impl Freeway {
    fn note(&mut self, icon: &str, title: &str, body: &str) {
        self.notes.push(Note { id: self.next_note, icon: icon.into(), title: title.into(), body: body.into(), age: 0.0 });
        self.next_note += 1;
        if self.notes.len() > 3 {
            self.notes.remove(0);
        }
    }

    /// What the host tells the game: where it runs, kept settings and saves.
    fn host_cmds(&mut self, input: &InputFrame) {
        for c in &input.cmds {
            if let Some(p) = c.strip_prefix("host.platform ") {
                let words: Vec<&str> = p.split_whitespace().collect();
                self.web = words.contains(&"web");
                self.touch = words.contains(&"touch");
                if self.touch {
                    // Phones and tablets show the frame-rate readout only on request.
                    self.settings.fps = false;
                }
            } else if let Some(json) = c.strip_prefix("host.settings ") {
                match serde_json::from_str::<Settings>(json) {
                    Ok(s) => self.settings = s,
                    Err(e) => log::warn!("kept settings unreadable ({e}); using defaults"),
                }
            } else if let Some(rest) = c.strip_prefix("host.save ") {
                let slot = rest.split_whitespace().next().unwrap_or("").to_string();
                if !slot.is_empty() && !self.saves.contains(&slot) {
                    self.saves.push(slot);
                }
            }
        }
    }

    fn settings_changed(&mut self) {
        self.settings_serial += 1;
        self.settings_json = serde_json::to_string(&self.settings).ok().map(Arc::new);
    }

    /// Hold L1: after a moment the wheel opens (slow motion); the left stick picks, releasing
    /// takes it. A tap swaps to the last weapon.
    fn wheel_tick(&mut self, input: &InputFrame, real: f32) {
        let down = pad::held(input, pav_core::input::pad::L1);
        match (&mut self.wheel, down) {
            (None, true) => self.wheel = Some(WheelState::default()),
            (Some(w), true) => {
                w.open_for += real;
                if w.open_for >= WHEEL_DELAY {
                    w.aim(input.pad.stick_l);
                }
            }
            (Some(w), false) => {
                let pick = if w.open_for < WHEEL_DELAY { Some(self.last_weapon) } else { w.focus };
                self.wheel = None;
                if let Some(p) = pick.filter(|p| *p != self.weapon) {
                    self.last_weapon = self.weapon;
                    self.weapon = p;
                    let (name, icon, detail) = WEAPONS[p];
                    self.note(icon, name, detail);
                }
            }
            (None, false) => {}
        }
    }

    /// The wheel is open (past the tap delay).
    pub fn wheel_open(&self) -> bool {
        self.wheel.as_ref().is_some_and(|w| w.open_for >= WHEEL_DELAY)
    }

    fn feet(sim: &Sim) -> Vec3 {
        sim.state.player.and_then(|id| sim.feet(id)).unwrap_or(sim.state.focus)
    }

    fn look(&self) -> Look {
        Look { sensitivity: self.settings.sensitivity, invert_y: self.settings.invert_y }
    }

    /// The camera for this page: a drone over the lot in the menus, the orbit camera in play.
    fn camera(&self, feet: Vec3) -> CameraIntent {
        match self.front.page {
            Page::Boot | Page::Title | Page::Main | Page::About => {
                camera::drone(Vec3::new(0.0, 0.0, -1.0), self.ui_time, 22.0, 9.0)
            }
            Page::Settings if self.front.back != Page::Pause => camera::drone(Vec3::new(0.0, 0.0, -1.0), self.ui_time, 22.0, 9.0),
            _ => {
                let _ = feet;
                self.cam.intent()
            }
        }
    }

    /// Golden hour over the bay (time of day and weather arrive in M2).
    fn view() -> ViewIntent {
        let (el, az) = (22f32.to_radians(), 250f32.to_radians());
        let to_sun = Vec3::new(az.sin() * el.cos(), el.sin(), -az.cos() * el.cos());
        let horizon = Color::hex("#b9cde0");
        ViewIntent {
            sun_dir: -to_sun,
            sun_color: Color::hex("#ffd9a8"),
            sun_intensity: 1.15,
            ambient_sky: Color::hex("#8fa6c8"),
            ambient_ground: Color::hex("#7a6352"),
            ambient: 0.85,
            sky: Some(SkyIntent {
                zenith: Color::hex("#4f7fbf"),
                horizon,
                ground: Color::hex("#6b5a4a"),
                glow: Color::hex("#ffcf8a"),
                sun_size: 0.03,
                moon_dir: None,
                stars: 0.0,
                clouds: 0.2,
                cloud_color: Color::hex("#fff1e0"),
                lightning: 0.0,
            }),
            fog: Some(FogIntent { color: horizon, start: 140.0, end: 520.0 }),
            exposure: 1.0,
            bloom: 0.35,
            temperature: 0.12,
            saturation: 1.08,
            shadow_radius: 60.0,
            ..Default::default()
        }
    }
}

impl Game for Freeway {
    const KEY: &'static str = KEY;

    fn pre(&mut self, sim: &mut Sim, input: &InputFrame, _dt: f32, events: &mut Vec<SimEvent>) -> BTreeMap<EntityId, InputFrame> {
        let real = sim.dt();
        self.ui_time += real;
        if self.map.is_none() {
            // The level never changes shape yet: one picture, version 1.
            self.map = Some(Arc::new(map::raster(sim, 1)));
        }
        self.host_cmds(input);
        let feet = Self::feet(sim);
        let mut ctx = Ctx {
            settings: &mut self.settings,
            has_save: !self.saves.is_empty(),
            web: self.web,
            player: Vec2::new(feet.x, feet.z),
            now: self.ui_time,
        };
        let mut sounds = Vec::new();
        match self.front.step(input, &mut ctx, real, &mut sounds) {
            Some(Action::Play) => self.roam_age = 0.0,
            Some(Action::Settings) => self.settings_changed(),
            Some(Action::Quit) => self.quit = true,
            None => {}
        }
        for n in &mut self.notes {
            n.age += real;
        }
        self.notes.retain(|n| n.age < 6.0);
        self.money_age += real;
        let mut out = BTreeMap::new();
        if self.front.page == Page::Play {
            self.roam_age += real;
            let (open, focus, notes) = (self.wheel_open(), self.wheel.as_ref().and_then(|w| w.focus), self.next_note);
            self.wheel_tick(input, real);
            match (open, self.wheel_open()) {
                (false, true) => sounds.push("ui.slowmo_in"),
                (true, false) => sounds.push(if self.next_note != notes { "wheel.pick" } else { "ui.slowmo_out" }),
                _ => {}
            }
            if self.wheel_open() && self.wheel.as_ref().and_then(|w| w.focus) != focus {
                sounds.push("wheel.slot");
            }
            if self.next_note != notes && !open {
                sounds.push("ui.toast");
            }
            let look = self.look();
            if self.wheel_open() {
                self.cam.update(&InputFrame::default(), feet, look, real);
            } else {
                self.cam.update(input, feet, look, real);
                if let Some(id) = sim.state.player {
                    out.insert(id, player::on_foot(input, &self.cam));
                }
            }
        } else {
            self.wheel = None;
        }
        events.extend(sounds.into_iter().map(|cue| SimEvent::Sound { cue: cue.into(), pos: None, gain: 1.0, pitch: 1.0 }));
        out
    }

    fn post(&mut self, _sim: &mut Sim, dt: f32, _events: &mut Vec<SimEvent>) {
        self.time += dt;
    }

    fn hud(&self, _sim: &Sim) -> Vec<String> {
        vec!["Grand Theft Freeway-BX".into(), format!("page: {}", self.front.page.name())]
    }

    fn status(&self, sim: &Sim) -> serde_json::Value {
        let player = sim.state.player.and_then(|id| sim.feet(id)).map(|p| [round(p.x), round(p.z)]);
        json!({
            "mode": self.mode,
            "page": self.front.page.name(),
            "time": round(self.time),
            "ui_time": round(self.ui_time),
            "player": player,
            "weapon": WEAPONS[self.weapon].0,
            "wheel": self.wheel_open(),
            "attract_due": self.front.attract_due,
            "idle": round(self.front.idle),
            "camera_yaw": round(self.cam.yaw.to_degrees()),
            "web": self.web,
            "touch": self.touch,
        })
    }

    fn time_scale(&self) -> f32 {
        if self.wheel_open() && self.settings.wheel_slowmo { 0.25 } else { 1.0 }
    }

    fn world_paused(&self) -> bool {
        let f = &self.front;
        f.page == Page::Pause || (matches!(f.page, Page::Settings | Page::Gallery | Page::About) && f.back == Page::Pause)
    }

    fn legacy_controls(&self) -> bool {
        false
    }

    fn present(&self, sim: &Sim) -> Option<PlayFrame> {
        let feet = Self::feet(sim);
        let player = Vec2::new(feet.x, feet.z);
        let blips = hud::lot_blips();
        let page = self.front.page;
        let mut ui =
            if page == Page::Gallery { gallery::page(self.front.gallery, self.front.since, feet) } else { UiModel::default() };
        ui.perf = self.settings.fps && !self.touch;
        ui.glyphs = self.settings.glyphs;
        ui.scale = self.settings.hud_scale;
        ui.safe_margin = self.settings.safe_area;
        ui.time = self.ui_time;
        if page != Page::Gallery {
            ui.screen = self.front.screen(&self.settings, !self.saves.is_empty(), self.web, player, &blips);
        }
        if page == Page::Play {
            let heading = sim.player().and_then(|p| p.character.as_ref()).map(|c| c.facing).unwrap_or(0.0);
            let help = (self.roam_age < 12.0).then_some((
                "Move with ~btn:left_stick~ and look with ~btn:right_stick~. ~btn:west~ jumps, ~btn:start~ pauses.",
                self.roam_age,
            ));
            let (hud, help, prompts) = hud::hud(&hud::HudInput {
                player,
                heading,
                camera_yaw: self.cam.yaw,
                money: (self.money, self.money_delta, self.money_age),
                weapon: self.weapon,
                help,
                map_blips: blips.clone(),
            });
            ui.hud = Some(hud);
            ui.help = help;
            ui.prompts = prompts;
            if self.wheel_open() {
                ui.wheel = Some(self.wheel.as_ref().unwrap().ui(self.weapon));
                ui.overlay.vignette = 0.45;
            }
        }
        if page != Page::Gallery {
            ui.toasts = self.notes.iter().map(|n| n.toast()).collect();
        }
        // Fades: into the title from the boot splash, and into play.
        ui.overlay.fade = match page {
            Page::Boot => {
                let t = self.front.since;
                // Fade between the engine card and the game card.
                if t < 0.4 {
                    1.0 - t / 0.4
                } else if (1.1..1.5).contains(&t) {
                    1.0 - (t - 1.3).abs() / 0.2
                } else {
                    0.0
                }
            }
            Page::Title | Page::Play => self.front.fade(self.ui_time, 0.6),
            _ => 0.0,
        };
        let menu = page != Page::Play;
        Some(PlayFrame {
            ui,
            draw: DrawList::default(),
            camera: Some(self.camera(feet)),
            view: Some(Self::view()),
            audio: AudioScene {
                master: self.settings.master,
                effects: self.settings.sfx,
                // The menus duck the city.
                ducking: if menu { 0.6 } else { 0.0 },
                ..Default::default()
            },
            map: self.map.clone(),
            app: AppRequests {
                save: None,
                settings: self
                    .settings_json
                    .as_ref()
                    .map(|json| SettingsRequest { serial: self.settings_serial, json: json.clone() }),
                quit: self.quit,
                vibrate: None,
                keys: Some(if menu { pad::MENU_KEYS.clone() } else { pad::FOOT_KEYS.clone() }),
                mouse_look: !menu && !self.wheel_open(),
            },
        })
    }
}

fn round(x: f32) -> f64 {
    (x as f64 * 100.0).round() / 100.0
}
