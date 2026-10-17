//! Grand Theft Freeway-BX — the game's rules and content runtime on the Pavilion engine's game
//! hook (`pav_core::games`). Simulation only: no graphics, window or audio code (PLAN.md §6.2);
//! everything the game shows and plays leaves as data in its `PlayFrame` (`present`). The
//! binaries in `gtf_app` make it known to the engine with [`register`].

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::Result;
use glam::{Vec2, Vec3};
use pav_core::InputFrame;
use pav_core::entity::EntityId;
use pav_core::frame::SimEvent;
use pav_core::games::{Entry, Game, GameBox, entry};
use pav_core::present::*;
use pav_core::sim::Sim;
use serde::{Deserialize, Serialize};
use serde_json::json;

pub mod about;
pub mod camera;
pub mod content;
pub mod daytime;
pub mod frontend;
pub mod gallery;
pub mod gps;
pub mod hud;
pub mod pad;
pub mod player;
pub mod protagonists;
pub mod settings;
pub mod tour;
pub mod views;
pub mod world;

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
    /// A reference view's still camera (`freeway/view/<name>`).
    pub view: Option<views::View>,
    /// The clock and the weather.
    pub sky: daytime::Sky,
    /// Sprint stamina and crouch.
    pub foot: player::Foot,
    /// The district (rebuilt from its file when missing, e.g. after loading a snapshot; a
    /// cache, so not part of the state).
    #[serde(skip)]
    city: Option<Arc<pav_world::City>>,
    /// The streaming tour's route (`freeway/tour`; rebuilt from the city when missing).
    #[serde(skip)]
    tour: Option<Arc<tour::Tour>>,
    /// The route to the waypoint (derived; replanned when missing).
    #[serde(skip)]
    gps: gps::Gps,
    #[serde(skip)]
    settings_json: Option<Arc<String>>,
}

/// Builds a scene of the game: Paloma Bay with the frontend (or straight into play).
pub fn build(sim: &mut Sim, rest: Option<&str>) -> Result<()> {
    let mode = rest.unwrap_or("").to_string();
    let view = match mode.strip_prefix("view/") {
        Some(name) => Some(views::find(name)?),
        None => None,
    };
    let city = match &view {
        Some(v) => world::build_scene(sim, world::city()?, Vec2::new(v.target.x, v.target.z), &[v.eye, v.target])?,
        None if mode == "dev" => world::build_scene(sim, world::dev_city()?, world::flat(world::DEV_SPAWN), &[])?,
        None => world::build_scene(sim, world::city()?, world::flat(world::SPAWN), &[])?,
    };
    let mode_is_tour = mode == "tour";
    let mut front = Front::default();
    front.page = match mode.as_str() {
        "roam" | "dev" | "tour" => Page::Play,
        _ if view.is_some() => Page::Play,
        "gallery" => {
            front.back = Page::Title;
            Page::Gallery
        }
        "attract" => Page::Title,
        _ => Page::Boot,
    };
    let mut cam = OrbitCam::default();
    // Look east along Sunset Row from the start.
    cam.face(std::f32::consts::FRAC_PI_2);
    player::tune(&mut sim.config.movement);
    let world = daytime::def()?;
    let mut sky = daytime::Sky::new(&world);
    if let Some(v) = &view {
        // A view is a still: its hour holds, its weather stays.
        if let Some(t) = v.time.as_deref() {
            sky.hour = daytime::parse_time(t).ok_or_else(|| anyhow::anyhow!("view {}: bad time '{t}'", v.name))?;
        }
        sky.speed = 0.0;
        sky.set_weather(&world, v.weather.as_deref().unwrap_or("clear"), true)?;
    }
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
        view,
        sky,
        foot: player::Foot::default(),
        tour: (mode_is_tour).then(|| Arc::new(tour::Tour::new(&city))),
        gps: gps::Gps::default(),
        city: Some(city),
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

    /// World commands (also typed by tests and tools): `time HH:MM`, `clock <speed>`,
    /// `weather <name>` (held until changed), `weather auto`.
    fn world_cmds(&mut self, input: &InputFrame) {
        let Ok(world) = daytime::def() else { return };
        for c in &input.cmds {
            let mut w = c.split_whitespace();
            match (w.next(), w.next()) {
                (Some("time"), Some(t)) => match daytime::parse_time(t) {
                    Some(h) => self.sky.hour = h,
                    None => log::warn!("time: '{t}' is not HH:MM"),
                },
                (Some("clock"), Some(k)) => match k.parse::<f32>() {
                    Ok(k) if (0.0..=1000.0).contains(&k) => self.sky.speed = k,
                    _ => log::warn!("clock: '{k}' is not a speed (0-1000)"),
                },
                (Some("weather"), Some("auto")) => self.sky.held = false,
                (Some("weather"), Some(name)) => {
                    if let Err(e) = self.sky.set_weather(&world, name, true) {
                        log::warn!("{e}");
                    }
                }
                _ => {}
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

    /// The menus show the city from a drone; play uses the orbit camera.
    fn drone_view(&self) -> bool {
        if self.view.is_some() {
            return false;
        }
        match self.front.page {
            Page::Boot | Page::Title | Page::Main | Page::About => true,
            Page::Settings => self.front.back != Page::Pause,
            _ => false,
        }
    }

    /// The camera for this page.
    fn camera(&self) -> CameraIntent {
        if let Some(v) = &self.view {
            return v.camera();
        }
        if let Some(t) = &self.tour {
            return t.camera(self.time * tour::SPEED);
        }
        if self.drone_view() { camera::drone(world::DOWNTOWN, self.ui_time, 230.0, 125.0) } else { self.cam.intent() }
    }

    /// The look of the moment: the clock and the weather.
    fn look_now(&self) -> ViewIntent {
        match daytime::def() {
            Ok(world) => self.sky.view(&world),
            Err(e) => {
                log::error!("{e:#}");
                ViewIntent::default()
            }
        }
    }
}

impl Game for Freeway {
    const KEY: &'static str = KEY;

    fn pre(&mut self, sim: &mut Sim, input: &InputFrame, dt: f32, events: &mut Vec<SimEvent>) -> BTreeMap<EntityId, InputFrame> {
        let real = sim.dt();
        self.ui_time += real;
        if self.city.is_none() {
            match world::city_for(&self.mode) {
                Ok(c) => self.city = Some(c),
                Err(e) => log::error!("{e:#}"),
            }
        }
        // Streaming follows the camera when it is away from the player (the menus' drone).
        sim.state.world.interest.clear();
        if self.drone_view() {
            let c = camera::drone(world::DOWNTOWN, self.ui_time, 230.0, 125.0);
            sim.state.world.interest.push(c.target);
        }
        if let Some(v) = &self.view {
            sim.state.world.interest.extend_from_slice(&[v.eye, v.target]);
        }
        if self.mode == "tour" {
            if self.tour.is_none() {
                self.tour = self.city.as_ref().map(|c| Arc::new(tour::Tour::new(c)));
            }
            if let Some(t) = &self.tour {
                let s = self.time * tour::SPEED;
                sim.state.world.interest.extend_from_slice(&[t.at(s), t.at(s + 40.0)]);
                // The player rides along (one focus for streaming, as in play).
                if let Some(id) = sim.state.player {
                    sim.set_position(id, t.at(s) + Vec3::Y * 0.05);
                }
            }
        }
        self.host_cmds(input);
        self.world_cmds(input);
        if let Ok(world) = daytime::def() {
            self.sky.step(&world, dt, &mut sim.state.rng, events);
        }
        let feet = Self::feet(sim);
        self.gps.set(self.front.waypoint);
        if let Some(c) = &self.city {
            self.gps.update(c, feet);
        }
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
                if let (Some(id), None, false) = (sim.state.player, &self.view, self.mode == "tour") {
                    let (frame, haste) = self.foot.input(input, &self.cam, 50.0, dt);
                    if let Some(c) = sim.state.entities.get_mut(id).and_then(|e| e.character.as_mut()) {
                        c.haste = haste;
                    }
                    out.insert(id, frame);
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
            "clock": daytime::format_time(self.sky.hour),
            "weather": self.sky.weather,
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
        let blips = self.city.as_deref().map(world::blips).unwrap_or_default();
        let page = self.front.page;
        let mut ui =
            if page == Page::Gallery { gallery::page(self.front.gallery, self.front.since, feet) } else { UiModel::default() };
        ui.perf = self.settings.fps && !self.touch;
        ui.glyphs = self.settings.glyphs;
        ui.scale = self.settings.hud_scale;
        ui.safe_margin = self.settings.safe_area;
        ui.time = self.ui_time;
        if page != Page::Gallery {
            ui.screen =
                self.front.screen(&self.settings, !self.saves.is_empty(), self.web, player, &blips, &self.gps.minimap(feet));
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
                route: self.gps.minimap(feet),
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
        let mut draw = DrawList::default();
        if page == Page::Play {
            draw.ribbons.extend(self.gps.ribbon(feet, 160.0));
        }
        Some(PlayFrame {
            ui,
            draw,
            camera: Some(self.camera()),
            view: Some(self.look_now()),
            audio: AudioScene {
                master: self.settings.master,
                effects: self.settings.sfx,
                // The menus duck the city.
                ducking: if menu { 0.6 } else { 0.0 },
                ..Default::default()
            },
            map: self.city.as_ref().map(|c| c.map.clone()),
            far: self.city.as_ref().map(|c| c.far.clone()),
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
