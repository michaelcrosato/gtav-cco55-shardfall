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
pub mod ambience;
pub mod audio;
pub mod camera;
pub mod combat;
pub mod content;
pub mod daytime;
pub mod drive;
pub mod frontend;
pub mod gallery;
pub mod gps;
pub mod hud;
pub mod pad;
pub mod peds;
pub mod player;
pub mod protagonists;
pub mod radio;
pub mod settings;
pub mod struck;
pub mod tour;
pub mod traffic;
pub mod vehicles;
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
/// Seconds D-pad left must be held in a car before the radio wheel opens (a tap steps back).
const RADIO_WHEEL_DELAY: f32 = 0.3;

/// The radio wheel: every station, then "Off".
fn radio_wheel_ui(def: &radio::RadioDef, w: &WheelState, on: Option<usize>) -> Wheel {
    let mut slots: Vec<WheelSlot> = def
        .station
        .iter()
        .map(|s| WheelSlot { label: s.name.clone(), icon: "radio".into(), detail: Some(s.freq.clone()), enabled: true })
        .collect();
    slots.push(WheelSlot { label: "Off".into(), icon: "radio_off".into(), detail: None, enabled: true });
    let focus = w.focus.or(on).unwrap_or(def.station.len());
    let center = slots.get(focus).map(|s| format!("{}\n{}", s.label, s.detail.clone().unwrap_or_default())).unwrap_or_default();
    Wheel { id: "radio".into(), slots, focus: Some(focus), center }
}

/// What the chase camera needs of the vehicle `car`.
fn ride(sim: &Sim, car: EntityId) -> Option<camera::Ride> {
    let c = drive::Car::read(sim, car)?;
    let drift = sim.vehicle_status(car).map(|s| s.drift).unwrap_or(0.0);
    Some(camera::Ride { center: c.center, yaw: c.yaw, vel: c.vel, half: c.half, drift, heli: c.heli })
}

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
    /// The player's weapons and vitals, everyone's health.
    #[serde(default)]
    pub combat: combat::Combat,
    /// People on foot around the player.
    #[serde(default)]
    pub peds: peds::Peds,
    /// The car radio (stations are live on the game clock), and its wheel (hold D-pad left).
    #[serde(default)]
    pub radio: radio::Radio,
    #[serde(default)]
    pub radio_wheel: Option<WheelState>,
    /// Getting in and out of vehicles, driving.
    #[serde(default)]
    pub drive: drive::Drive,
    /// Cars on the roads, parked cars, buses.
    #[serde(default)]
    pub traffic: traffic::Traffic,
    /// People recently struck by vehicles.
    #[serde(default)]
    pub struck: struck::Struck,
    /// This tick's inputs for traffic (worked out in `pre`, handed over in `Game::vehicles`).
    #[serde(skip)]
    traffic_inputs: BTreeMap<EntityId, pav_core::vehicle::VehicleInput>,
    /// The camera behind the vehicle being driven.
    #[serde(default)]
    pub chase: camera::ChaseCam,
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
        radio: radio::Radio::default(),
        radio_wheel: None,
        drive: drive::Drive::default(),
        traffic: traffic::Traffic::default(),
        struck: struck::Struck::default(),
        traffic_inputs: BTreeMap::new(),
        chase: camera::ChaseCam::default(),
        peds: peds::Peds::default(),
        combat: combat::Combat::default(),
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

    /// World commands (also typed by tests, tools and bots): `time HH:MM`, `clock <speed>`,
    /// `weather <name>` (held until changed), `weather auto`, `radio <station>|off|next|prev`,
    /// `weapon <id or icon>`.
    fn world_cmds(&mut self, sim: &mut Sim, input: &InputFrame) {
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
                (Some("radio"), Some(arg)) => self.radio_cmd(arg),
                (Some("autopilot"), Some(arg)) => {
                    let car = if arg == "off" { None } else { self.drive.vehicle() };
                    if let Some(city) = self.city.clone() {
                        self.traffic.autopilot(sim, &city, car);
                    }
                }
                (Some(verb @ ("car" | "drive")), Some(key)) => {
                    let paint = w.next().and_then(|p| p.parse().ok());
                    self.car_cmd(sim, key, paint, verb == "drive");
                }
                (Some("target"), Some(m)) => self.target_cmd(sim, m.parse().unwrap_or(8.0)),
                (Some("weapon"), Some(arg)) => {
                    match combat::def().ok().and_then(|d| d.weapon.iter().position(|w| w.id == arg || w.icon == arg)) {
                        Some(i) => self.weapon = i,
                        None => log::warn!("weapon: no weapon '{arg}'"),
                    }
                }
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

    /// `target <metres>`: a stranger standing that far along the camera's view (the Proving
    /// Grounds' range, bots and checks).
    fn target_cmd(&mut self, sim: &mut Sim, ahead: f32) {
        let (Some(id), cam) = (sim.state.player, self.cam.intent()) else { return };
        let Some(feet) = sim.feet(id) else { return };
        let f = (cam.target - cam.eye).normalize_or(Vec3::NEG_Z);
        let flat = Vec3::new(f.x, 0.0, f.z).normalize_or(Vec3::NEG_Z);
        let at = feet + flat * ahead.clamp(1.0, 60.0);
        let look = pav_core::puppet::PuppetDef { shirt: "#3a5a40".into(), ..Default::default() };
        sim.spawn_npc("target", at, (-flat.x).atan2(-flat.z), look, None, None);
    }

    /// `radio <station id>|off|next|prev`: tunes, and shows the dial card.
    fn radio_cmd(&mut self, arg: &str) {
        let Ok(def) = radio::def() else { return };
        match self.radio.command(&def, arg) {
            Ok(()) => {
                if let Some((head, line)) = self.radio.card(&def, self.time as f64) {
                    self.note("radio", &head, &line);
                }
            }
            Err(e) => log::warn!("{e}"),
        }
    }

    /// In a car: D-pad ←/→ step the stations (the card shows again); holding ← opens the radio
    /// wheel, the left stick picks and letting go tunes (`real` seconds).
    fn radio_controls(&mut self, input: &InputFrame, real: f32, sounds: &mut Vec<&'static str>) {
        use pav_core::input::pad::{LEFT, RIGHT};
        let Ok(def) = radio::def() else { return };
        let n = def.station.len();
        match (&mut self.radio_wheel, pad::held(input, LEFT)) {
            (None, true) if pad::pressed(input, LEFT) => self.radio_wheel = Some(WheelState::default()),
            (Some(w), true) => {
                w.open_for += real;
                if w.open_for >= RADIO_WHEEL_DELAY {
                    let before = w.focus;
                    w.aim_n(input.pad.stick_l, n + 1);
                    if w.focus != before {
                        sounds.push("wheel.slot");
                    }
                }
            }
            (Some(w), false) => {
                let (open_for, focus) = (w.open_for, w.focus);
                self.radio_wheel = None;
                if open_for < RADIO_WHEEL_DELAY {
                    // A tap steps back a station.
                    self.radio.step(&def, -1);
                } else if let Some(f) = focus {
                    // The last slot is "Off".
                    self.radio.tune((f < n).then_some(f));
                    sounds.push("wheel.pick");
                }
                self.drive.card = 0.0;
            }
            _ => {}
        }
        if pad::pressed(input, RIGHT) && self.radio_wheel.is_none() {
            self.radio.step(&def, 1);
            self.drive.card = 0.0;
        }
    }

    /// The radio wheel is open (past the tap delay).
    pub fn radio_wheel_open(&self) -> bool {
        self.radio_wheel.as_ref().is_some_and(|w| w.open_for >= RADIO_WHEEL_DELAY)
    }

    /// What getting in and out did: the radio comes on in a car, the cameras hand over, people
    /// near a smashed window or a carjacking take fright.
    fn after_deeds(&mut self, sim: &mut Sim, deeds: &[drive::Deed]) {
        use drive::Deed;
        for d in deeds {
            match *d {
                Deed::Entered { car } => {
                    self.traffic.unpark(car);
                    if let Some(c) = drive::Car::read(sim, car) {
                        self.chase.place_behind(c.yaw);
                        if c.radio {
                            let last = self.radio.last;
                            self.radio.tune(Some(last));
                        }
                    }
                }
                Deed::Exited { car } | Deed::Bailed { car, .. } => {
                    if let Some(city) = self.city.clone() {
                        self.traffic.autopilot(sim, &city, None);
                    }
                    self.radio.tune(None);
                    self.radio.last = self.radio.last.min(radio::def().map(|d| d.station.len().saturating_sub(1)).unwrap_or(0));
                    self.radio_wheel = None;
                    // The on-foot camera carries on looking the way the chase camera did.
                    let yaw = self.chase.yaw + self.chase.look;
                    self.cam.face(std::f32::consts::PI - yaw);
                    let _ = car;
                }
                Deed::Smashed { at, .. } => {
                    if let Some(city) = self.city.clone() {
                        self.peds.scare(sim, &city.walk, at, 12.0);
                    }
                }
                Deed::Jacked { at, driver, car } => {
                    self.traffic.release(car);
                    if let Some(city) = self.city.clone() {
                        self.peds.scare(sim, &city.walk, at, 20.0);
                        self.peds.adopt(sim, &city.walk, driver, at);
                    }
                }
            }
        }
    }

    /// Someone struck by a vehicle: the player is hurt; a person dies above 12 m/s or is knocked
    /// down and runs; people near film it or flee.
    fn strike(&mut self, sim: &mut Sim, city: &pav_world::City, hit: struck::Strike, events: &mut Vec<SimEvent>) {
        if Some(hit.who) == sim.state.player {
            self.combat.hurt_player(hit.speed * 6.0);
        } else {
            let lethal = hit.speed >= struck::LETHAL;
            let amount = if lethal { 500.0 } else { hit.speed * 5.0 };
            let mut out = combat::Outcome::default();
            self.combat.damage(sim, hit.who, hit.at, hit.dir, amount, 0.0, events, &mut out);
            if out.killed.iter().any(|k| k.0 == hit.who) || lethal {
                self.peds.forget(hit.who);
                let clip = pav_core::clips::find_cached("QUATERNIUS/Hit_Knockback");
                if let Some(c) = sim.state.entities.get_mut(hit.who).and_then(|e| e.character.as_mut()) {
                    // Full detail while thrown, so the knock-back carries the body.
                    c.lod = false;
                    if clip != 0 {
                        c.anim.replay_clip(clip, pav_core::clips::CLAMP, 1.0);
                    }
                }
            } else {
                self.peds.knocked_down(sim, hit.who, hit.at);
            }
        }
        self.peds.scare(sim, &city.walk, hit.at, 12.0);
        self.peds.incident(sim, hit.at, 35.0);
    }

    /// `car <archetype> [paint]`: a vehicle standing 5 m ahead of the player, side on (the
    /// Proving Grounds, bots and checks). `drive <archetype>`: and the player in its seat.
    fn car_cmd(&mut self, sim: &mut Sim, key: &str, paint: Option<usize>, seat: bool) {
        let Some(pid) = sim.state.player else { return };
        let Some(feet) = sim.feet(pid) else { return };
        let (f, _) = self.cam.axes();
        let want = if seat { feet } else { feet + f * 5.0 };
        // On the nearest street lane, along it; else where asked, side on to the camera (the
        // driver's door toward the player), or along the view when driven.
        let on_lane = self.city.as_ref().and_then(|c| {
            use pav_world::lanes::LaneKind::*;
            let keep = |l: &pav_world::lanes::Lane| !l.closed && matches!(l.kind, Street | Avenue | Boulevard | Alley);
            let (lane, along, d) = c.lanes.nearest(want, keep)?;
            (d < 25.0).then(|| {
                let (p, dir) = c.lanes.lanes[lane as usize].at(along);
                (p, dir.x.atan2(dir.z))
            })
        });
        let (at, yaw) = on_lane.unwrap_or((want, f.x.atan2(f.z) - if seat { 0.0 } else { std::f32::consts::FRAC_PI_2 }));
        match vehicles::spawn(sim, key, at + Vec3::Y * 0.05, yaw, paint) {
            Ok(id) => {
                if seat && self.drive.seat(sim, pid, id) {
                    self.after_deeds(sim, &[drive::Deed::Entered { car: id }]);
                }
            }
            Err(e) => log::warn!("{e:#}"),
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
        Look { sensitivity: self.settings.sensitivity, invert_y: self.settings.invert_y, recentre: self.settings.recentre }
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
    fn camera(&self, sim: &Sim) -> CameraIntent {
        if let Some(v) = &self.view {
            return v.camera();
        }
        if let Some(t) = &self.tour {
            return t.camera(self.time * tour::SPEED);
        }
        if self.drone_view() {
            return camera::drone(world::DOWNTOWN, self.ui_time, 230.0, 125.0);
        }
        match self.drive.vehicle().and_then(|car| ride(sim, car)) {
            Some(r) => self.chase.intent(&r),
            None => self.cam.intent(),
        }
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
        self.world_cmds(sim, input);
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
            let (notes, was_open) = (self.next_note, self.wheel_open());
            if self.drive.vehicle().is_none() {
                let (open, focus) = (self.wheel_open(), self.wheel.as_ref().and_then(|w| w.focus));
                self.wheel_tick(input, real);
                match (open, self.wheel_open()) {
                    (false, true) => sounds.push("ui.slowmo_in"),
                    (true, false) => sounds.push(if self.next_note != notes { "wheel.pick" } else { "ui.slowmo_out" }),
                    _ => {}
                }
                if self.wheel_open() && self.wheel.as_ref().and_then(|w| w.focus) != focus {
                    sounds.push("wheel.slot");
                }
            } else {
                self.wheel = None;
            }
            let look = self.look();
            // The player (not in a reference view's still or on the streaming tour's flight).
            let pid = sim.state.player.filter(|_| self.view.is_none() && self.mode != "tour");
            let mut deeds = Vec::new();
            let mut gunfire = None;
            if let Some(id) = pid {
                // Getting in or out, and driving.
                if let Some(walk) = self.drive.tick(sim, id, input, dt, &mut deeds, events) {
                    out.insert(id, walk);
                }
                self.drive.steer(sim, id, input, dt, &mut deeds, events);
            }
            let riding = self.drive.vehicle().and_then(|car| Some((car, ride(sim, car)?)));
            if let Some((car, r)) = riding {
                // Walls pull the camera in: the distance to the first surface along a ray.
                // (The fixed world only: people and props walking past do not shove the camera.)
                let hit = |o: Vec3, d: Vec3, max: f32| sim.raycast_static(o, d, max).map(|h| h.point.distance(o));
                self.chase.update(input, &r, look, real, &hit);
                self.radio_controls(input, real, &mut sounds);
                // Headlights after dark and in bad weather.
                let dark = !(6.3..19.5).contains(&self.sky.hour) || matches!(self.sky.weather.as_str(), "rain" | "storm" | "fog");
                sim.set_vehicle_lights(car, dark);
            } else if self.wheel_open() {
                let hit = |o: Vec3, d: Vec3, max: f32| sim.raycast_static(o, d, max).map(|h| h.point.distance(o));
                self.cam.update(&InputFrame::default(), feet, look, camera::Moving::default(), real, &hit);
            } else {
                self.radio_wheel = None;
                let (stick, world) = crate::pad::stick_or_move(input);
                let moving = camera::Moving {
                    dir: if world { stick } else { self.cam.world_move(stick) }.clamp_length_max(1.0),
                    sprinting: self.foot.sprinting,
                    aiming: input.pad.l2 > 0.5 || input.pad.held & pav_core::input::pad::L2 != 0,
                };
                let hit = |o: Vec3, d: Vec3, max: f32| sim.raycast_static(o, d, max).map(|h| h.point.distance(o));
                self.cam.update(input, feet, look, moving, real, &hit);
                // △ near a car starts getting in (the walk to its door begins next tick).
                let entering = pid.is_some()
                    && !self.drive.busy()
                    && pad::pressed(input, pav_core::input::pad::NORTH)
                    && self.drive.begin_enter(sim, feet);
                if let (Some(id), false, false) = (pid, entering, self.drive.busy()) {
                    let (frame, haste) = self.foot.input(input, &self.cam, 50.0, dt);
                    let wdef = combat::def().ok();
                    let gun = wdef.as_ref().and_then(|d| d.weapon.get(self.weapon)).is_some_and(|w| w.kind == combat::Kind::Gun);
                    let cam = self.cam.intent();
                    let look = (cam.target - cam.eye).normalize_or(Vec3::NEG_Z);
                    if let Some(c) = sim.state.entities.get_mut(id).and_then(|e| e.character.as_mut()) {
                        c.haste = haste;
                        // Aiming a gun turns the body to the aim (strafing).
                        c.face = (moving.aiming && gun).then(|| look.x.atan2(look.z));
                    }
                    out.insert(id, frame);
                    // The weapon in hand: fire, reload, melee; the people around react.
                    if let Some(wdef) = wdef {
                        let facing = sim.state.entities.get(id).and_then(|e| e.character.as_ref()).map_or(0.0, |c| c.facing);
                        let shooter = combat::Shooter { id, feet, facing, camera: &cam, moving: moving.dir.length() > 0.3 };
                        let res = self.combat.tick(sim, &wdef, self.weapon, input, &shooter, &mut self.cam, dt, events);
                        gunfire = res.gunfire;
                        if let Some(city) = self.city.clone() {
                            if let Some(at) = res.gunfire {
                                self.peds.scare(sim, &city.walk, at, 40.0);
                            }
                            let melee = wdef.weapon.get(self.weapon).is_some_and(|w| w.kind == combat::Kind::Melee);
                            for &(who, at) in &res.hurt {
                                if melee {
                                    // Tough people fight back; the rest run.
                                    self.peds.assaulted(sim, &city.walk, who, at);
                                    self.peds.scare(sim, &city.walk, at, 8.0);
                                } else {
                                    self.peds.scare(sim, &city.walk, at, 15.0);
                                }
                                self.peds.incident(sim, at, 35.0);
                            }
                            for &(who, at) in &res.killed {
                                self.peds.forget(who);
                                self.peds.scare(sim, &city.walk, at, 25.0);
                            }
                        }
                    }
                }
            }
            if self.next_note != notes && !was_open {
                sounds.push("ui.toast");
            }
            self.after_deeds(sim, &deeds);
            // The people around the player (not on the streaming tour's flight).
            if let (Some(city), false) = (self.city.clone(), self.mode == "tour") {
                // A reference view's crowd stands around its camera, not its target.
                let player = match &self.view {
                    Some(v) => Vec3::new(v.eye.x, 0.0, v.eye.z),
                    None => sim.state.player.and_then(|id| sim.feet(id)).unwrap_or(feet),
                };
                // Vehicles about to run people over throw them first (before the physics step).
                let on_foot = sim.state.player.filter(|_| self.drive.vehicle().is_none() && self.view.is_none());
                // Everyone on foot near the player (the crowd, drivers who got out, the player).
                let people: Vec<EntityId> = sim
                    .state
                    .entities
                    .iter()
                    .filter(|e| e.character.as_ref().is_some_and(|c| c.riding.is_none()) && !e.name.starts_with("dead:"))
                    .filter(|e| e.pos.distance(player) < 120.0)
                    .filter(|e| Some(e.id) != sim.state.player || on_foot.is_some())
                    .map(|e| e.id)
                    .collect();
                for hit in self.struck.tick(sim, &people, dt, events) {
                    self.strike(sim, &city, hit, events);
                }
                let gun_out = self.combat.aiming
                    && combat::def()
                        .ok()
                        .and_then(|d| d.weapon.get(self.weapon).map(|w| w.kind == combat::Kind::Gun))
                        .unwrap_or(false);
                let m = peds::Moment {
                    city: &city,
                    player,
                    camera: self.camera(sim),
                    time: self.time,
                    hour: self.sky.hour,
                    dt,
                    player_on_foot: on_foot.and_then(|id| sim.feet(id)),
                    gun_out,
                };
                let done = self.peds.tick(sim, &m, &mut out);
                for dmg in done.punches {
                    self.combat.hurt_player(dmg);
                }
                // Traffic around the player (a reference view's around its camera).
                self.traffic_inputs.clear();
                {
                    let sirens = sim
                        .state
                        .entities
                        .iter()
                        .filter_map(|e| {
                            let v = e.vehicle.as_ref()?;
                            v.state.siren_on.then(|| (e.pos, Vec3::new(v.yaw.sin(), 0.0, v.yaw.cos())))
                        })
                        .collect();
                    let people = self
                        .peds
                        .list
                        .iter()
                        .filter_map(|p| sim.feet(p.id))
                        .chain(sim.state.player.and_then(|id| sim.feet(id)).filter(|_| self.drive.vehicle().is_none()))
                        .filter(|f| f.distance(player) < 160.0)
                        .collect();
                    let tm = traffic::Moment {
                        city: &city,
                        player,
                        camera: self.camera(sim),
                        time: self.time,
                        hour: self.sky.hour,
                        dt,
                        player_car: self.drive.vehicle(),
                        sirens,
                        people,
                        gunfire,
                    };
                    let mut inputs = BTreeMap::new();
                    self.traffic.tick(sim, &tm, &mut inputs, &mut self.drive.locked);
                    self.traffic_inputs = inputs;
                }
            }
        } else {
            self.wheel = None;
        }
        self.combat.update(sim, dt);
        events.extend(sounds.into_iter().map(|cue| SimEvent::Sound { cue: cue.into(), pos: None, gain: 1.0, pitch: 1.0 }));
        out
    }

    fn post(&mut self, sim: &mut Sim, dt: f32, events: &mut Vec<SimEvent>) {
        self.time += dt;
        self.traffic.crashes(sim, events);
        audio::crash_sounds(events);
    }

    fn hud(&self, _sim: &Sim) -> Vec<String> {
        vec!["Grand Theft Freeway-BX".into(), format!("page: {}", self.front.page.name())]
    }

    fn status(&self, sim: &Sim) -> serde_json::Value {
        let (walking, waiting, lingering) = self.peds.counts();
        let peds = json!({ "out": self.peds.list.len(), "walking": walking, "waiting": waiting, "lingering": lingering, "spawned": self.peds.spawned, "stuck": self.peds.stuck });
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
            "peds": peds,
            "traffic": self.traffic.summary(),
            "drive": match self.drive.phase {
                drive::Phase::Foot => "foot",
                drive::Phase::Enter { .. } => "entering",
                drive::Phase::In { .. } => "driving",
                drive::Phase::Stopping { .. } => "stopping",
                drive::Phase::Exit { .. } => "exiting",
            },
            "vehicle": self.drive.vehicle().and_then(|car| {
                let st = sim.vehicle_status(car)?;
                let name = sim.state.entities.get(car).map(|e| e.name.clone())?;
                Some(json!({ "key": name, "kmh": round(st.kmh), "gear": st.gear, "health": round(st.health), "siren": st.siren_on, "airborne": st.airborne }))
            }),
            "radio": radio::def().ok().and_then(|d| self.radio.card(&d, self.time as f64)).map(|(head, line)| format!("{head}: {line}")),
            "web": self.web,
            "touch": self.touch,
        })
    }

    fn bot(&self, _sim: &Sim) -> InputFrame {
        // On autopilot: the traffic driver's pedals and wheel, as a controller would give them.
        let mut f = InputFrame::default();
        if let (Some(vi), Some(_)) = (self.traffic.pilot_input, self.drive.vehicle()) {
            f.pad.r2 = vi.throttle;
            f.pad.l2 = vi.brake;
            f.pad.stick_l.x = vi.steer;
            if vi.handbrake {
                f.pad.held |= pav_core::input::pad::R1;
            }
        }
        f
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

    fn vehicles(&mut self, _sim: &mut Sim, _dt: f32) -> BTreeMap<EntityId, pav_core::vehicle::VehicleInput> {
        let mut out = self.traffic_inputs.clone();
        self.drive.inputs(&mut out);
        out
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
            let help = if self.drive.vehicle().is_some() {
                (self.drive.card < 8.0).then_some((
                    "~btn:r2~ accelerates, ~btn:l2~ brakes and reverses, ~btn:r1~ is the handbrake. ~btn:left~ ~btn:right~ change the station, ~btn:north~ gets out.",
                    self.drive.card,
                ))
            } else {
                (self.roam_age < 12.0).then_some((
                    "Move with ~btn:left_stick~ and look with ~btn:right_stick~. ~btn:west~ jumps, ~btn:north~ enters vehicles, ~btn:start~ pauses.",
                    self.roam_age,
                ))
            };
            let (hud, help, prompts) = hud::hud(&hud::HudInput {
                player,
                heading,
                camera_yaw: self.cam.yaw,
                money: (self.money, self.money_delta, self.money_age),
                weapon: self.weapon,
                help,
                map_blips: blips.clone(),
                route: self.gps.minimap(feet),
                vitals: (self.combat.health / combat::MAX_HEALTH, self.combat.armor / combat::MAX_ARMOR),
                ammo: self.combat.mag.get(self.weapon).map(|&m| (m, self.combat.reserve.get(self.weapon).copied().unwrap_or(0))),
                crosshair: combat::def()
                    .ok()
                    .and_then(|d| d.weapon.get(self.weapon).cloned())
                    .and_then(|w| self.combat.crosshair(&w, self.combat.aiming, false)),
            });
            let mut hud = hud;
            let mut prompts = prompts;
            match self.drive.vehicle().and_then(|car| drive::Car::read(sim, car)) {
                Some(c) => {
                    // The radar opens out with speed; the car's card shows for a while.
                    if let Some(m) = &mut hud.minimap {
                        m.radius = 60.0 + (c.vel.length() * 2.2).min(60.0);
                    }
                    hud.weapon = None;
                    hud.crosshair = None;
                    hud.objective = Some("Explore Paloma Bay.".into());
                    if self.drive.card < 4.0 {
                        let name = vehicles::get(&c.key).map(|a| a.name).unwrap_or_else(|_| c.key.clone());
                        let card = radio::def().ok().and_then(|d| self.radio.card(&d, self.time as f64));
                        hud.vehicle = Some(VehicleHud {
                            name,
                            station: card.as_ref().map(|c| c.0.clone()).or_else(|| c.radio.then(|| "RADIO OFF".to_string())),
                            song: card.map(|c| c.1),
                            age: self.drive.card,
                        });
                    }
                }
                None if !self.drive.busy() => {
                    if drive::Drive::nearest(sim, feet).is_some() {
                        prompts.push(Prompt { button: PadButton::North, label: "Enter vehicle".into(), hold: None });
                    }
                }
                None => {}
            }
            ui.hud = Some(hud);
            ui.help = help;
            ui.prompts = prompts;
            if self.wheel_open() {
                ui.wheel = Some(self.wheel.as_ref().unwrap().ui(self.weapon));
                ui.overlay.vignette = 0.45;
            }
            if self.radio_wheel_open() {
                if let (Some(w), Ok(def)) = (&self.radio_wheel, radio::def()) {
                    ui.wheel = Some(radio_wheel_ui(&def, w, self.radio.on));
                }
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
        if let Some(c) = &self.city {
            world::signal_lamps(c, self.camera(sim).target, self.time, &mut draw.prims);
        }
        self.combat.draw(&mut draw);
        let cam = self.camera(sim);
        let view = self.look_now();
        // The beds: where the ears are, who is near, the weather and the hour.
        let crowd = self.peds.list.iter().filter_map(|p| sim.feet(p.id)).filter(|f| f.distance(cam.eye) < 25.0).count();
        let loops = match self.city.as_deref() {
            Some(city) => ambience::beds(&ambience::Ears {
                city,
                at: cam.eye,
                crowd,
                rain: view.rain.min(1.0),
                wind: view.wind.length(),
                night: 0.5 + 0.5 * (self.sky.hour / 24.0 * std::f32::consts::TAU).cos(),
            }),
            None => Vec::new(),
        };
        let mut loops = loops;
        if !menu {
            audio::vehicle_loops(sim, cam.eye, self.drive.vehicle(), &self.drive.alarms, &mut loops);
        }
        let listener =
            Some(pav_core::present::Listener { pos: cam.eye, forward: (cam.target - cam.eye).normalize_or(Vec3::NEG_Z) });
        Some(PlayFrame {
            ui,
            draw,
            camera: Some(cam),
            view: Some(view),
            audio: AudioScene {
                master: self.settings.master,
                effects: self.settings.sfx,
                // The menus duck the city.
                ducking: if menu { 0.6 } else { 0.0 },
                listener,
                loops,
                music: radio::def().ok().and_then(|d| self.radio.music(&d, self.time as f64, self.settings.music)),
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
                keys: Some(if menu {
                    pad::MENU_KEYS.clone()
                } else if self.drive.vehicle().is_some() {
                    pad::DRIVE_KEYS.clone()
                } else {
                    pad::FOOT_KEYS.clone()
                }),
                mouse_look: !menu && !self.wheel_open() && !self.radio_wheel_open(),
            },
        })
    }
}

fn round(x: f32) -> f64 {
    (x as f64 * 100.0).round() / 100.0
}
