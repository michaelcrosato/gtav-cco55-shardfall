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
pub mod activities;
pub mod ambience;
pub mod anchor;
pub mod audio;
pub mod bot;
pub mod camera;
pub mod combat;
pub mod content;
pub mod cover;
pub mod daytime;
pub mod drive;
pub mod enemies;
pub mod fate;
pub mod frontend;
pub mod gallery;
pub mod glass;
pub mod gps;
pub mod heat;
pub mod hud;
pub mod missions;
pub mod pad;
pub mod peds;
pub mod phone;
mod play;
pub mod player;
pub mod police;
pub mod progress;
pub mod protagonists;
pub mod radio;
pub mod settings;
pub mod shops;
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
    /// The saves the host keeps, by slot (`host.save`).
    pub saves: BTreeMap<String, String>,
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
    /// The car radio (stations are live on its clock), and its wheel (hold D-pad left).
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
    /// Cover the player has taken.
    #[serde(default)]
    pub cover: cover::Cover,
    /// The wanted level, the police, and the player's fate (Wasted, Busted).
    #[serde(default)]
    pub heat: heat::Heat,
    #[serde(default)]
    pub police: police::Police,
    #[serde(default)]
    pub fate: fate::Fate,
    /// Crimes counted lately (a burst of fire is one shooting): seconds until each counts again.
    #[serde(default)]
    pub crime_clock: Vec<(heat::Crime, f32)>,
    /// Seconds to the next scanner chatter, the line on air and how many have been said, and
    /// seconds of an officer's hand on the player's shoulder.
    #[serde(default)]
    pub scanner: f32,
    #[serde(default)]
    pub chatter: Option<radio::Chatter>,
    #[serde(default)]
    pub chatters: u32,
    #[serde(default)]
    pub cuffed: f32,
    /// Where the player's feet were a little while ago and the seconds they have stayed within
    /// a metre of it (pinned between officers, running on the spot is standing still).
    #[serde(default)]
    pub law_feet: Option<Vec3>,
    #[serde(default)]
    pub law_still: f32,
    /// The player bot (`autopilot escape`: the heat-evasion bot).
    #[serde(default)]
    pub bot: bot::Bot,
    /// Armed opponents (missions; `enemies <n> <side>`).
    #[serde(default)]
    pub enemies: enemies::Enemies,
    /// Resprays bought (stats), and Paloma Customs' door (found once).
    #[serde(default)]
    pub resprays: u32,
    #[serde(skip)]
    pub customs: Option<Option<Vec3>>,
    /// Told the price at the door without the money (once until out of the car).
    #[serde(default)]
    pub told_price: bool,
    /// Crimes typed as commands (`crime assault`: tests, bots), judged next tick.
    #[serde(default)]
    pub typed_crimes: Vec<heat::Crime>,
    /// This tick's inputs for traffic and the police (worked out in `pre`, handed over in
    /// `Game::vehicles`).
    #[serde(skip)]
    ai_inputs: BTreeMap<EntityId, pav_core::vehicle::VehicleInput>,
    /// The camera behind the vehicle being driven.
    #[serde(default)]
    pub chase: camera::ChaseCam,
    /// The protagonists: who is played, the others' things, switching, the ability.
    #[serde(default)]
    pub crew: protagonists::Crew,
    /// Missions played and the one under way, and the bot that plays them.
    #[serde(default)]
    pub missions: missions::Missions,
    /// Stunt jumps, the Lost Shields, vigilante, random events: what is done and under way.
    #[serde(default)]
    pub activities: activities::Activities,
    #[serde(default)]
    pub mbot: missions::bot::MissionBot,
    #[serde(default)]
    pub phone: phone::Phone,
    /// A shop's menu (or the save slots', the replay list's), and the counter the player stands at.
    #[serde(default)]
    pub shop: Option<shops::ShopState>,
    #[serde(default)]
    pub counter: Option<shops::Counter>,
    #[serde(default)]
    pub shop_bought: u32,
    /// Saving: the newest stamp, the request for the host, a save due after a mission.
    #[serde(default)]
    pub save_stamp: u64,
    #[serde(default)]
    pub save_serial: u64,
    #[serde(default)]
    pub save_req: Option<SaveRequest>,
    #[serde(default)]
    pub autosave: bool,
    /// The shops' doors (looked up once).
    #[serde(skip)]
    doors: Option<Vec<(shops::Counter, Option<Vec3>)>>,
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
    // `freeway/mission/<key>`: straight into a mission, in its district.
    let mission = match mode.strip_prefix("mission/") {
        Some(key) => Some(missions::get(key)?),
        None => None,
    };
    let dev = mode == "dev" || mission.as_ref().is_some_and(|m| m.district == "dev");
    let city = match &view {
        Some(v) => world::build_scene(sim, world::city()?, Vec2::new(v.target.x, v.target.z), &[v.eye, v.target])?,
        None if dev => world::build_scene(sim, world::dev_city()?, world::flat(world::DEV_SPAWN), &[])?,
        None => {
            // A Paloma Bay mission loads the ground around its start (the player is put there).
            let paloma = world::city()?;
            let start = mission
                .as_ref()
                .and_then(|m| m.start.as_deref().or(m.blip.as_ref().map(|b| b.at.as_str())))
                .and_then(|a| anchor::Anchor::parse(a).ok())
                .and_then(|a| anchor::point(&paloma, &a))
                .map_or(world::flat(world::SPAWN), world::flat);
            world::build_scene(sim, paloma, start, &[])?
        }
    };
    let mode_is_tour = mode == "tour";
    let mut front = Front::default();
    front.page = match mode.as_str() {
        "roam" | "dev" | "tour" => Page::Play,
        _ if view.is_some() || mission.is_some() => Page::Play,
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
    let mut game = Freeway {
        mode,
        time: 0.0,
        ui_time: 0.0,
        front,
        cam,
        settings: Settings::default(),
        settings_serial: 0,
        web: false,
        touch: false,
        saves: BTreeMap::new(),
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
        cover: cover::Cover::default(),
        heat: heat::Heat::default(),
        police: police::Police::default(),
        fate: fate::Fate::default(),
        crime_clock: Vec::new(),
        scanner: 0.0,
        chatter: None,
        chatters: 0,
        cuffed: 0.0,
        law_feet: None,
        law_still: 0.0,
        typed_crimes: Vec::new(),
        bot: bot::Bot::default(),
        enemies: enemies::Enemies::default(),
        resprays: 0,
        customs: None,
        told_price: false,
        ai_inputs: BTreeMap::new(),
        chase: camera::ChaseCam::default(),
        crew: protagonists::Crew::new(0),
        missions: missions::Missions::default(),
        activities: activities::Activities::default(),
        mbot: missions::bot::MissionBot::default(),
        phone: phone::Phone::default(),
        shop: None,
        counter: None,
        shop_bought: 0,
        save_stamp: 0,
        save_serial: 0,
        save_req: None,
        autosave: false,
        doors: None,
        peds: peds::Peds::default(),
        combat: combat::Combat::default(),
        tour: (mode_is_tour).then(|| Arc::new(tour::Tour::new(&city))),
        gps: gps::Gps::default(),
        city: Some(city),
        settings_json: None,
    };
    // Dez to begin with; switching is open (the story's intro missions will lock it until they
    // are done).
    game.crew.unlocked = true;
    game.load_member(0);
    game.dress_player(sim);
    if let Some(m) = &mission {
        game.start_mission(sim, &m.key, false, true, &mut Vec::new())?;
    }
    sim.state.play = Some(GameBox::new(game));
    Ok(())
}

impl Freeway {
    /// Money in (positive) or out, shown beside the cash for a while.
    pub(crate) fn pay(&mut self, delta: i64) {
        self.money += delta;
        self.money_delta = delta;
        self.money_age = 0.0;
    }

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
                if let Some((slot, json)) = rest.split_once(' ')
                    && !slot.is_empty()
                {
                    self.saves.insert(slot.to_string(), json.to_string());
                }
            }
        }
    }

    /// World commands (also typed by tests, tools and bots): `time HH:MM`, `clock <speed>`,
    /// `weather <name>` (held until changed), `weather auto`, `radio <station>|off|next|prev`,
    /// `radio seek <seconds>|end` (into the tuned station's slot, or to its song's end),
    /// `weapon <id or icon>`.
    fn world_cmds(&mut self, sim: &mut Sim, input: &InputFrame, events: &mut Vec<SimEvent>) {
        let Ok(world) = daytime::def() else { return };
        for c in &input.cmds {
            let mut w = c.split_whitespace();
            match (w.next(), w.next()) {
                // `mission start|replay <key>`, `mission quit`, `mission fail`, `mission choose <id>`.
                (Some("mission"), Some(verb)) => {
                    let arg = w.next().unwrap_or("");
                    match verb {
                        "start" | "replay" => {
                            if let Err(e) = self.start_mission(sim, arg, verb == "replay", true, events) {
                                log::warn!("{e:#}");
                            }
                        }
                        "quit" => self.quit_mission(sim),
                        "fail" => {
                            if let Some(mut run) = self.missions.run.take() {
                                if run.phase == missions::Phase::Play {
                                    run.phase = missions::Phase::Failed { t: 0.0 };
                                    run.reason = "Failed on purpose.".into();
                                    run.fails += 1;
                                    run.menu = 0;
                                    self.missions.failed += 1;
                                }
                                self.missions.run = Some(run);
                            }
                        }
                        "choose" => self.missions.bot_choice = (!arg.is_empty()).then(|| arg.to_string()),
                        // A choice kept from an earlier mission (the heist's approach), for tests.
                        "story" if !arg.is_empty() => {
                            self.missions.story.insert(arg.to_string());
                        }
                        _ => log::warn!("mission: start, replay, quit, fail, choose or story"),
                    }
                }
                // `switch <protagonist>`: the switch, wheel or not.
                (Some("switch"), Some(who)) => {
                    if let Some(i) = protagonists::def().ok().and_then(|d| d.index(who)) {
                        self.begin_switch(sim, i, events);
                    } else {
                        log::warn!("switch: no protagonist '{who}'");
                    }
                }
                (Some("meter"), Some(x)) => {
                    if let (Ok(x), Some(m)) = (x.parse::<f32>(), self.crew.members.get_mut(self.crew.current)) {
                        m.meter = x.clamp(0.0, 1.0);
                    }
                }
                (Some("give"), Some(id)) => self.give_weapon(id, w.next().and_then(|a| a.parse().ok()).unwrap_or(0)),
                (Some("cash"), Some(n)) => {
                    if let Ok(n) = n.parse::<i64>() {
                        self.pay(n);
                    }
                }
                (Some("save"), Some(slot)) => self.save_game(sim, slot),
                (Some("load"), Some(slot)) => {
                    if let Some(json) = self.saves.get(slot).cloned()
                        && let Err(e) = self.load_game(sim, &json)
                    {
                        log::warn!("load {slot}: {e:#}");
                        self.note("save", "Can't load this save", "It is damaged or from a newer version");
                    }
                }
                // `phone <app>|home|close` (captures, tools); `menu gun|mods|store|save|load|replay`.
                (Some("phone"), Some(app)) => {
                    if app == "close" {
                        self.phone.open = false;
                    } else if app == "home" || phone::APPS.iter().any(|a| a.0 == app) {
                        self.phone.open = true;
                        self.phone.app = app.into();
                        self.phone.focus = phone::APPS.iter().position(|a| a.0 == app).unwrap_or(0);
                        self.phone.reading = None;
                    } else {
                        log::warn!("phone: home, close or an app ({})", phone::APPS.map(|a| a.0).join(", "));
                    }
                }
                (Some("menu"), Some(which)) => {
                    let c = match which {
                        "gun" => Some(shops::Counter::Gun),
                        "mods" => Some(shops::Counter::Mods),
                        "store" => Some(shops::Counter::Store),
                        "save" => Some(shops::Counter::Save),
                        "load" => Some(shops::Counter::Load),
                        "replay" => Some(shops::Counter::Replay),
                        _ => None,
                    };
                    match c {
                        Some(c) => self.shop = Some(shops::ShopState::new(c, true)),
                        None => log::warn!("menu: gun, mods, store, save, load or replay"),
                    }
                }
                // An activity or collectible counted as done (`jump:<id>`, `shield:<id>`, ...): tests.
                (Some("activity_done"), Some(id)) => self.activity_done(id),
                // A run at a stunt jump for the bot, from its run-up in an Okami Kaze.
                (Some("stunt"), Some(id)) => self.stunt_cmd(id),
                // The autopilot's destination: `pilot_goal <x> <z> [desire] [y] [then <x> <y> <z>]`, `pilot_goal off`.
                (Some("pilot_goal"), Some("off")) => self.traffic.pilot_goal(None, 1.0),
                (Some("pilot_goal"), Some(x)) => {
                    let z = w.next().and_then(|z| z.parse::<f32>().ok());
                    let desire = w.next().and_then(|d| d.parse::<f32>().ok()).unwrap_or(1.0);
                    // (The height picks the level: a deck or a ramp over a street. The ground's
                    // when none. `then <x> <y> <z>`: on through it without stopping toward that
                    // point, a race's gate and the next.)
                    let y = w.next().and_then(|y| y.parse::<f32>().ok());
                    let then = match (w.next(), w.next(), w.next(), w.next()) {
                        (Some("then"), Some(x), Some(y), Some(z)) => match (x.parse(), y.parse(), z.parse()) {
                            (Ok(x), Ok(y), Ok(z)) => Some(Vec3::new(x, y, z)),
                            _ => None,
                        },
                        _ => None,
                    };
                    if let (Ok(x), Some(z), Some(city)) = (x.parse::<f32>(), z, self.city.clone()) {
                        let y = y.unwrap_or_else(|| city.ground_y(Vec2::new(x, z)));
                        self.traffic.pilot_goal(Some(Vec3::new(x, y, z)), desire);
                        self.traffic.drive_through(None, then);
                    }
                }
                (Some("time"), Some(t)) => match daytime::parse_time(t) {
                    Some(h) => self.sky.hour = h,
                    None => log::warn!("time: '{t}' is not HH:MM"),
                },
                (Some("clock"), Some(k)) => match k.parse::<f32>() {
                    Ok(k) if (0.0..=1000.0).contains(&k) => self.sky.speed = k,
                    _ => log::warn!("clock: '{k}' is not a speed (0-1000)"),
                },
                (Some("radio"), Some("seek")) => {
                    let to = w.next().unwrap_or("");
                    if let Err(e) = radio::def().and_then(|d| self.radio.seek(&d, to)) {
                        log::warn!("{e}");
                    }
                }
                (Some("radio"), Some(arg)) => self.radio_cmd(arg),
                (Some("crime"), Some(kind)) => {
                    use heat::Crime::*;
                    let c = match kind {
                        "assault" => Some(Assault),
                        "theft" => Some(CarTheft),
                        "shooting" => Some(Shooting),
                        "murder" => Some(KillCivilian),
                        "holdup" => Some(HoldUp),
                        "cop" => Some(AttackPolice),
                        _ => None,
                    };
                    match c {
                        Some(c) => self.typed_crimes.push(c),
                        None => log::warn!("crime: '{kind}' (assault, theft, shooting, murder, holdup, cop)"),
                    }
                }
                // `enemies <n> [side] [weapon]` around the player (18–26 m off, on walkable
                // ground), `enemies alert`, `enemies clear`.
                (Some("enemies"), Some(arg)) => {
                    if arg == "clear" {
                        self.enemies.clear(sim);
                    } else if arg == "alert" {
                        self.enemies.alert();
                    } else if let (Ok(n), Some(city)) = (arg.parse::<u32>(), self.city.clone()) {
                        let side = w.next().unwrap_or("gang");
                        let weapon = w.next();
                        let at = Self::feet(sim);
                        for i in 0..n.min(12) {
                            let a = i as f32 / n.max(1) as f32 * 1.6 - 0.8 + sim.state.rng.range(-0.2, 0.2);
                            let r = sim.state.rng.range(18.0, 26.0);
                            let want = Vec2::new(at.x + a.sin() * r, at.z + a.cos() * r);
                            let Some(p) = city.walk.nearest_walkable(want, 6) else { continue };
                            let feet = Vec3::new(p.x, city.ground_y(p).max(at.y - 2.0), p.y);
                            let face = (at.x - feet.x).atan2(at.z - feet.z);
                            if self.enemies.spawn(sim, side, feet, face, weapon).is_none() {
                                log::warn!("enemies: no side '{side}' (content/freeway/enemies.toml)");
                                break;
                            }
                        }
                    }
                }
                (Some("wanted"), Some(n)) => match n.parse::<u8>() {
                    Ok(0) => {
                        self.heat.clear();
                        self.police.stand_down();
                    }
                    Ok(n) => {
                        let at = Self::feet(sim);
                        self.heat.raise(n.min(5), at);
                    }
                    Err(_) => log::warn!("wanted: '{n}' is not 0-5"),
                },
                (Some("autopilot"), Some(arg)) => {
                    // `on`, `escape` (run from the police: the bot leaves wrecked or boxed-in
                    // cars and takes others), `off`.
                    let car = if arg == "off" { None } else { self.drive.vehicle() };
                    self.bot.stop();
                    self.bot.escape = arg == "escape";
                    if let Some(city) = self.city.clone() {
                        self.traffic.autopilot(sim, &city, car);
                    }
                }
                (Some(verb @ ("car" | "drive")), Some(key)) => {
                    let paint = w.next().and_then(|p| p.parse().ok());
                    self.car_cmd(sim, key, paint, verb == "drive");
                }
                (Some("target"), Some(m)) => self.target_cmd(sim, m.parse().unwrap_or(8.0)),
                // `weapon <id or icon>`: in hand (given first if not owned).
                (Some("weapon"), Some(arg)) => {
                    match combat::def().ok().and_then(|d| d.weapon.iter().position(|w| w.id == arg || w.icon == arg)) {
                        Some(i) => {
                            if !self.crew.owns(i) {
                                self.give_weapon(arg, 0);
                            }
                            self.weapon = i;
                        }
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
                if let Some((head, line)) = self.radio.card(&def, self.radio.clock) {
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

    /// The radio's words on screen: the scanner's line (after its squelch), else what a talk
    /// station is saying.
    fn radio_subtitle(&self, def: &Arc<radio::RadioDef>) -> Option<Subtitle> {
        if let Some(c) = &self.chatter
            && c.age >= radio::CHATTER_LEAD
        {
            let name = if def.scanner.name.is_empty() { "SCANNER".to_string() } else { def.scanner.name.clone() };
            return Some(Subtitle { speaker: Some(name), text: c.text.clone(), color: Some(pav_core::Color::hex("#8ab4ff")) });
        }
        let a = self.radio.on_air(def, self.radio.clock).filter(|a| !a.dj)?;
        let st = def.station.get(self.radio.on?)?;
        Some(Subtitle {
            speaker: Some(format!("{} · {}", a.speaker.to_uppercase(), st.name.to_uppercase())),
            text: a.text,
            color: Some(pav_core::Color::hex("#ffcf6e")),
        })
    }

    /// A traffic or weather report going on air: its words from the heat and the weather now.
    fn radio_reports(&mut self, sim: &Sim) {
        let Ok(def) = radio::def() else { return };
        let Some((key, words, h)) = self.radio.pending(&def, self.radio.clock) else { return };
        let news = self.news(sim, &def);
        let station = def.station.get(key.station).map_or("", |s| s.id.as_str());
        let text = radio::report(&def, station, &words, h, &news);
        self.radio.say(radio::Said { text, ..key });
    }

    /// What the police and the weather are doing, for the radio.
    pub fn news(&self, sim: &Sim, def: &radio::RadioDef) -> radio::News {
        let feet = Self::feet(sim);
        let (player, zone) = match self.city.as_deref() {
            Some(c) => (def.places.name(c, feet), def.places.name(c, self.heat.last_known)),
            None => (def.places.elsewhere.clone(), def.places.elsewhere.clone()),
        };
        radio::News { stars: self.heat.stars, searching: self.heat.searching, player, zone, weather: self.sky.weather.clone() }
    }

    /// What getting in and out did: the radio comes on in a car, the cameras hand over, people
    /// near a smashed window or a carjacking take fright.
    fn after_deeds(&mut self, sim: &mut Sim, deeds: &[drive::Deed]) {
        use drive::Deed;
        for d in deeds {
            match *d {
                Deed::Entered { car } => {
                    self.traffic.unpark(car);
                    // A car the police have not seen, while they search: a star off.
                    if self.heat.swapped_car(car) && self.heat.stars == 0 {
                        self.police.stand_down();
                    }
                    // The bot on the run drives off at once.
                    if self.bot.escape
                        && let Some(city) = self.city.clone()
                    {
                        self.traffic.autopilot(sim, &city, Some(car));
                    }
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
                    self.traffic.release(sim, car);
                    if let Some(city) = self.city.clone() {
                        self.peds.scare(sim, &city.walk, at, 20.0);
                        self.peds.adopt(sim, &city.walk, driver, at);
                    }
                }
            }
        }
    }

    /// Paloma Customs' door (the sidewalk in front of it), looked up once.
    pub(crate) fn customs_door(&mut self, city: &pav_world::City) -> Option<Vec3> {
        *self.customs.get_or_insert_with(|| world::door(city, "customs"))
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
                if let Some(p) = pick.filter(|p| *p != self.weapon && self.crew.owns(*p)) {
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
    pub fn camera(&self, sim: &Sim) -> CameraIntent {
        if let Some(v) = &self.view {
            return v.camera();
        }
        if let Some(t) = &self.tour {
            return t.camera(self.time * tour::SPEED);
        }
        if self.drone_view() {
            return camera::drone(world::DOWNTOWN, self.ui_time, 230.0, 125.0);
        }
        // A switch's sky-cam (or quick blend), a mission's cutscene, the phone's camera.
        if let Some(s) = &self.crew.switch {
            return s.camera(&OrbitCam::settled(s.dest, s.yaw));
        }
        if let Some(c) = self.mission_camera(sim) {
            return c;
        }
        if self.phone.camera() {
            return self.phone_view(sim);
        }
        if !self.fate.alive() {
            // A slow orbit around the player.
            let mut c = camera::drone(Self::feet(sim), self.fate.since() * 4.0, 6.5, 2.4);
            c.target = Self::feet(sim) + Vec3::Y * 0.6;
            c.fov = 50.0;
            c.near = 0.1;
            return c;
        }
        if let Some(c) = self.stunt_camera(sim) {
            return c;
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
        // The crew a mission has placed (and the cars they sit in) keep their ground loaded: a
        // switch lands on them.
        if let Some(run) = &self.missions.run
            && let Ok(def) = missions::get(&run.key)
        {
            let crew_id = |id: &str| def.spawn.iter().any(|x| x.id == id && x.protagonist.is_some());
            for a in run.actors.values() {
                let Some(s) = def.spawn.get(a.spawn) else { continue };
                if s.protagonist.is_none() && !s.driver.as_deref().is_some_and(crew_id) {
                    continue;
                }
                let p = a.entity.and_then(|e| sim.state.entities.get(e)).map_or(a.at, |e| e.pos);
                sim.state.world.interest.push(p);
            }
        }
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
        self.world_cmds(sim, input, events);
        if let Ok(world) = daytime::def() {
            self.sky.step(&world, dt, &mut sim.state.rng, events);
        }
        let feet = Self::feet(sim);
        let target = self.mission_waypoint(sim).or_else(|| self.street_waypoint(sim)).or(self.front.waypoint);
        self.gps.set(target);
        if let Some(c) = &self.city {
            self.gps.update(c, feet);
        }
        let mut ctx = Ctx {
            settings: &mut self.settings,
            has_save: !self.saves.is_empty(),
            web: self.web,
            player: Vec2::new(feet.x, feet.z),
            now: self.ui_time,
            on_mission: self.missions.run.is_some(),
        };
        let mut sounds = Vec::new();
        match self.front.step(input, &mut ctx, real, &mut sounds) {
            Some(Action::Play) => self.roam_age = 0.0,
            Some(Action::Continue) => {
                self.roam_age = 0.0;
                if let Some(json) = self.newest_save()
                    && let Err(e) = self.load_game(sim, &json)
                {
                    log::warn!("continue: {e:#}");
                }
            }
            Some(Action::NewGame) => {
                self.roam_age = 0.0;
                self.new_game(sim);
            }
            Some(Action::Replay) => self.shop = Some(shops::ShopState::new(shops::Counter::Replay, true)),
            Some(Action::Load) => self.shop = Some(shops::ShopState::new(shops::Counter::Load, true)),
            Some(Action::QuitMission) => self.quit_mission(sim),
            Some(Action::Settings) => self.settings_changed(),
            Some(Action::Quit) => self.quit = true,
            None => {}
        }
        // The radio runs on real time (slow motion and the pause menu neither stretch nor stop
        // it); a report going on air says what the police are doing now.
        self.radio.clock += real as f64;
        self.radio_reports(sim);
        for n in &mut self.notes {
            n.age += real;
        }
        self.notes.retain(|n| n.age < 6.0);
        self.money_age += real;
        // The others' clocks; the switch card's.
        let cur = self.crew.current;
        for (i, m) in self.crew.members.iter_mut().enumerate() {
            if i != cur {
                m.away += dt;
            }
        }
        if let Some(c) = &mut self.crew.card {
            c.1 += real;
            if c.1 > protagonists::CARD {
                self.crew.card = None;
            }
        }
        let out = if self.front.page == Page::Play {
            let mut out = BTreeMap::new();
            self.switch_tick(sim, real, events);
            // Who has the controls: a switch, a mission's card or cutscene, a menu — or the
            // player (less what the phone, the switch wheel and a counter's prompt take).
            let held = self.crew.switch.is_some() || self.mission_holds_controls() || self.shop.is_some();
            self.shops_tick(sim, input, real, events);
            if !held {
                self.phone_tick(sim, input, real, events);
                if !self.phone.open && self.wheel.is_none() {
                    self.switch_wheel(sim, input, real, events);
                }
            }
            self.ability_tick(input, real, events);
            self.mission_tick(sim, input, dt, real, events, &mut out);
            self.activities_tick(sim, input, dt, real, events, &mut out);
            let held = held || self.shop.is_some();
            let mut take = 0;
            if self.phone.open {
                take |= phone::TAKES;
            }
            // (A counter's prompt has D-pad right; so has vigilante's in a police car.)
            if self.counter.is_some() || self.vigilante_takes_right(sim) {
                take |= pav_core::input::pad::RIGHT;
            }
            let wheel = self.crew.wheel.is_some();
            let play_input = controls(input, held, take, wheel);
            let more = self.play(sim, &play_input, dt, real, feet, events, &mut sounds);
            out.extend(more);
            self.life_idle(sim);
            self.mission_bot_tick(sim, real);
            if self.autosave && self.missions.run.is_none() {
                self.autosave = false;
                self.save_game(sim, "auto");
            }
            out
        } else {
            self.wheel = None;
            self.crew.wheel = None;
            BTreeMap::new()
        };
        self.combat.update(sim, dt);
        // Glass a vehicle is about to go through breaks before it touches.
        if let Some(city) = self.city.clone() {
            glass::vehicles(sim, &city, events);
        }
        events.extend(sounds.into_iter().map(|cue| SimEvent::Sound { cue: cue.into(), pos: None, gain: 1.0, pitch: 1.0 }));
        out
    }

    fn post(&mut self, sim: &mut Sim, dt: f32, events: &mut Vec<SimEvent>) {
        self.time += dt;
        let mut hot: Vec<Vec3> =
            self.police.units.iter().filter_map(|u| sim.state.entities.get(u.vehicle).map(|e| e.pos)).collect();
        if self.heat.stars > 0 {
            hot.extend(sim.state.player.and_then(|p| sim.state.entities.get(p)).map(|e| e.pos));
        }
        self.traffic.crashes(sim, events, &hot);
        audio::crash_sounds(events);
        // Explosions hurt everyone near them; whoever sat in the vehicle that blew up dies.
        let blast = fate::blasts(sim, events, sim.state.player);
        let mut out = combat::Outcome::default();
        for (who, dmg, dir) in blast.hurt {
            let at = sim.state.entities.get(who).map(|e| e.pos).unwrap_or_default();
            self.combat.damage(sim, who, at, dir, dmg, dmg * 0.04, events, &mut out);
            sim.hit_character(who, at, dir, (dmg * 0.05).clamp(2.0, 12.0), false, events);
            self.peds.forget_if_dead(sim, who);
        }
        if self.fate.alive() && sim.state.player.is_some() {
            if blast.player_dies {
                self.combat.health = 0.0;
            } else if blast.player_hurt > 0.0 {
                self.combat.hurt_player(blast.player_hurt);
            }
        }
        // Ramming a police vehicle (driving into it, not being rammed) puts its wreck on the player.
        let mine = self.drive.vehicle().and_then(|c| drive::Car::read(sim, c));
        for ev in events.iter() {
            let SimEvent::Crash { vehicle, other: Some(other), impulse, .. } = *ev else { continue };
            let Some(me) = mine.as_ref() else { break };
            let them = if vehicle == me.id {
                other
            } else if other == me.id {
                vehicle
            } else {
                continue;
            };
            let toward = sim.state.entities.get(them).map(|e| (e.pos - me.center).normalize_or(Vec3::ZERO).dot(me.vel));
            if impulse >= 800.0 && toward.is_some_and(|v| v > 2.0) {
                self.police.harmed(them);
            }
        }
        // A police vehicle the player harmed blowing up is on the player.
        for ev in events.iter() {
            if let SimEvent::Explosion { pos, .. } = ev {
                let theirs = sim
                    .state
                    .entities
                    .iter()
                    .any(|e| e.vehicle.is_some() && e.pos.distance(*pos) < 1.5 && self.police.blamed(e.id));
                if theirs {
                    self.heat.crime(sim, heat::Crime::DestroyPolice, *pos, true, &[]);
                }
            }
        }
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
            "combat": json!({
                "aiming": self.combat.aiming,
                "shots": self.combat.shots,
                "hits": self.combat.hits,
                "kills": self.combat.kills,
                "rounds": [self.combat.mag.get(self.weapon).copied().unwrap_or(0), self.combat.reserve.get(self.weapon).copied().unwrap_or(0)],
            }),
            "wheel": self.wheel_open(),
            "attract_due": self.front.attract_due,
            "idle": round(self.front.idle),
            "camera_yaw": round(self.cam.yaw.to_degrees()),
            "clock": daytime::format_time(self.sky.hour),
            "weather": self.sky.weather,
            "peds": peds,
            "traffic": self.traffic.summary(),
            "evade": json!({ "on": self.bot.escape, "tap": format!("{:?}", self.bot.tap), "stuck": round(self.bot.anchor_t), "bailed": self.bot.bailed, "taken": self.bot.taken }),
            "heat": json!({ "stars": self.heat.stars, "searching": self.heat.searching, "reports": self.heat.reports.len(), "evade": round(self.heat.evade), "units": self.police.units.len(), "officers": self.police.officers.len(), "police_spawned": self.police.spawned, "spawn_in_view": self.police.spawn_in_view }),
            "fate": match self.fate { fate::Fate::Alive => "alive", fate::Fate::Wasted { .. } => "wasted", fate::Fate::Busted { .. } => "busted" },
            "health": round(self.combat.health),
            "money": self.money,
            "mission": self.mission_status(),
            "mbot": json!({ "walk": self.mbot.path.iter().take(3).map(|p| [p.x.round(), p.y.round()]).collect::<Vec<_>>(), "goal": self.mbot.goal.map(|g| [g.x.round(), g.y.round()]), "step": self.mbot.step, "t": round(self.mbot.t) }),
            "activities": json!({
                "done": self.activities.done.len(),
                "flight": self.activities.jumps.flight.as_ref().map(|f| if f.air { "air" } else { "gate" }),
                "stunt_run": self.activities.jumps.run.is_some(),
                "completion": round(self.completion() * 100.0),
            }),
            "activity": self.street_status(sim),
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
            "radio": radio::def().ok().and_then(|d| self.radio.card(&d, self.radio.clock)).map(|(head, line)| format!("{head}: {line}")),
            "on_air": radio::def().ok().and_then(|d| self.radio.on_air(&d, self.radio.clock)).map(|a| format!("{}: {}", a.speaker, a.text)),
            "scanner": self.chatter.as_ref().map(|c| c.text.clone()),
            "web": self.web,
            "touch": self.touch,
        })
    }

    fn bot(&self, _sim: &Sim) -> InputFrame {
        // In a mission: the mission bot's presses and commands. On autopilot: the traffic
        // driver's pedals and wheel, as a controller would give them.
        let mut f = if self.missions.run.is_some() { self.mbot.frame.clone() } else { self.street_frame() };
        if let (Some(vi), Some(_)) = (self.traffic.pilot_input, self.drive.vehicle()) {
            f.pad.r2 = vi.throttle;
            f.pad.l2 = vi.brake;
            // An open wheel has the stick.
            if self.crew.wheel.is_none() && self.wheel.is_none() {
                f.pad.stick_l.x = vi.steer;
            }
            if vi.handbrake {
                f.pad.held |= pav_core::input::pad::R1;
            }
        }
        self.bot.frame(&mut f);
        self.stunt_frame(_sim, &mut f);
        f
    }

    fn time_scale(&self) -> f32 {
        if !self.fate.alive() {
            fate::SLOW
        } else if (self.wheel_open() || self.switch_wheel_open()) && self.settings.wheel_slowmo {
            0.25
        } else if self.stunt_slowmo() {
            activities::jumps::SLOW
        } else {
            self.ability_time_scale()
        }
    }

    fn world_paused(&self) -> bool {
        let f = &self.front;
        f.page == Page::Pause
            || (matches!(f.page, Page::Settings | Page::Gallery | Page::About) && matches!(f.back, Page::Pause | Page::Play))
            || (f.page == Page::Play && (self.mission_pauses_world() || self.shop.is_some()))
    }

    fn outcome(&self) -> pav_core::games::Outcome {
        use pav_core::games::Outcome;
        // An activity played with `activity <id>`.
        if let Some(o) = self.street_outcome() {
            return o;
        }
        if !self.mode.starts_with("mission/") {
            return Outcome::Playing;
        }
        if self.missions.passed > 0 {
            return Outcome::Won;
        }
        match &self.missions.run {
            Some(r) if matches!(r.phase, missions::Phase::Failed { .. }) => Outcome::Lost,
            _ => Outcome::Playing,
        }
    }

    fn legacy_controls(&self) -> bool {
        false
    }

    fn vehicles(&mut self, _sim: &mut Sim, _dt: f32) -> BTreeMap<EntityId, pav_core::vehicle::VehicleInput> {
        let mut out = self.ai_inputs.clone();
        self.drive.inputs(&mut out);
        out
    }

    fn present(&self, sim: &Sim) -> Option<PlayFrame> {
        let feet = Self::feet(sim);
        // The radio's stations, once a frame (the HUD's card and subtitles, the host's music).
        let rdef = radio::def().ok();
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
            let brief = self.missions.run.as_ref().and_then(|r| {
                let d = missions::get(&r.key).ok()?;
                let objective = d.step.get(r.step).and_then(|s| s.objective.clone()).unwrap_or_default();
                Some(TextCard { heading: d.title.to_uppercase(), lines: vec![d.text.clone(), objective] })
            });
            ui.screen = self.front.screen(
                &self.settings,
                !self.saves.is_empty(),
                self.web,
                player,
                &blips,
                &self.gps.minimap(feet),
                {
                    let mut row = self.crew.stat_cards(self.front.stats, self.money);
                    row.cards.extend(self.progress_cards());
                    row.focus = self.front.stats.min(row.cards.len().saturating_sub(1));
                    row
                },
                brief,
                self.missions.run.is_some(),
            );
        }
        let mut shop_screen = None;
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
            let ability = (
                self.crew.me().map_or(0.0, |m| m.meter),
                protagonists::def()
                    .ok()
                    .and_then(|d| d.protagonist.get(self.crew.current).map(|p| p.tint()))
                    .unwrap_or(pav_core::Color::hex("#ffc145")),
            );
            let (hud, help, prompts) = hud::hud(&hud::HudInput {
                player,
                heading,
                camera_yaw: self.cam.yaw,
                money: (self.money, self.money_delta, self.money_age),
                weapon: self.weapon,
                ability,
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
            let mut markers = Vec::new();
            // The law on the HUD: stars (flashing while searching), police blips, and the search
            // zone with the officers' vision cones.
            if self.heat.stars > 0 {
                hud.wanted = Some(Wanted { stars: self.heat.stars, max: 5, flashing: self.heat.searching });
            }
            // Witnesses on the phone to the police: a marker over each.
            for r in self.heat.reports.iter().filter(|r| r.left < play::CALL_SHOWS) {
                if let Some(f) = sim.feet(r.reporter) {
                    markers.push(ScreenMarker {
                        pos: f + Vec3::Y * 2.2,
                        icon: BlipIcon::Police,
                        color: pav_core::Color::hex("#ffb020"),
                        label: Some("Calling the police".into()),
                        distance: false,
                    });
                }
            }
            if let Some(m) = &mut hud.minimap {
                m.blips.extend(self.police.blips(sim, self.ui_time));
                // Those fighting the player: red.
                for e in self.enemies.list.iter().filter(|e| e.aware) {
                    if let Some(f) = sim.feet(e.id) {
                        m.blips.push(Blip {
                            pos: Vec2::new(f.x, f.z),
                            icon: BlipIcon::Enemy,
                            color: pav_core::Color::hex("#e8423f"),
                            label: None,
                            clamp: false,
                            height: 0,
                            pulse: false,
                        });
                    }
                }
                if let Some((c, r)) = self.heat.zone() {
                    m.circles.push(CircleBlip {
                        pos: Vec2::new(c.x, c.z),
                        radius: r,
                        color: pav_core::Color::hex("#ff4d4d"),
                        flashing: true,
                    });
                    m.cones.extend(self.police.cones(sim));
                }
            }
            if !self.fate.alive() {
                hud.opacity = 0.0;
                prompts.clear();
            }
            match self.drive.vehicle().and_then(|car| drive::Car::read(sim, car)) {
                Some(c) => {
                    // The radar opens out with speed; the car's card shows for a while.
                    if let Some(m) = &mut hud.minimap {
                        m.radius = 60.0 + (c.vel.length() * 2.2).min(60.0);
                    }
                    hud.weapon = None;
                    hud.crosshair = None;
                    hud.objective = Some("Explore Paloma Bay.".into());
                    let def = rdef.as_ref();
                    let card = def.and_then(|d| self.radio.card(d, self.radio.clock));
                    // The car's card on getting in and tuning (4 s), again as a song begins; the
                    // DJ's card while they talk between songs.
                    let fresh = def
                        .and_then(|d| self.radio.now(d, self.radio.clock))
                        .and_then(|(st, _, off)| (st.genre != "talk" && off < radio::NEW_SONG_CARD).then_some(off));
                    let dj = def.and_then(|d| self.radio.on_air(d, self.radio.clock)).filter(|a| a.dj);
                    if self.drive.card < 4.0 || fresh.is_some() {
                        let name = vehicles::get(&c.key).map(|a| a.name).unwrap_or_else(|_| c.key.clone());
                        hud.vehicle = Some(VehicleHud {
                            name,
                            station: card.as_ref().map(|c| c.0.clone()).or_else(|| c.radio.then(|| "RADIO OFF".to_string())),
                            song: card.map(|c| c.1),
                            age: if self.drive.card < 4.0 { self.drive.card } else { fresh.unwrap_or(0.0) },
                        });
                    } else if let Some(a) = dj {
                        hud.vehicle = Some(VehicleHud {
                            name: a.speaker.to_uppercase(),
                            station: card.map(|c| c.0),
                            song: Some(a.text),
                            age: a.since,
                        });
                    }
                }
                None if self.cover.at.is_some() => {
                    prompts.push(Prompt { button: PadButton::L2, label: "Peek".into(), hold: None });
                    prompts.push(Prompt { button: PadButton::R1, label: "Leave cover".into(), hold: None });
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
            ui.markers.extend(markers);
            let (shop, prompt) = self.shop_ui(sim);
            if let Some(p) = prompt {
                ui.prompts.push(p);
            }
            shop_screen = shop;
            if self.wheel_open() {
                let mut w = self.wheel.as_ref().unwrap().ui(self.weapon);
                for (i, slot) in w.slots.iter_mut().enumerate() {
                    slot.enabled = self.crew.owns(i);
                }
                ui.wheel = Some(w);
                ui.overlay.vignette = 0.45;
            }
            if self.switch_wheel_open()
                && let Some(w) = &self.crew.wheel
            {
                ui.wheel = Some(self.crew.wheel_ui(w));
                ui.overlay.vignette = 0.45;
            }
            if let Some(t) = self.ability_tint() {
                // The ability's grade: its colour over a half-washed-out picture, the edges dark.
                ui.overlay.tint = Some(t);
                ui.overlay.desaturate = ui.overlay.desaturate.max(0.35);
                ui.overlay.vignette = ui.overlay.vignette.max(0.6);
            }
            if let Some((text, age)) = &self.crew.card {
                ui.help = Some(Help { text: text.clone(), age: *age });
            }
            ui.phone = self.phone_ui(sim);
            if self.phone.camera() {
                // The viewfinder: no HUD or tips over the picture, the shutter and back buttons.
                if let Some(h) = &mut ui.hud {
                    h.opacity = 0.0;
                }
                ui.help = None;
                ui.prompts = vec![
                    Prompt { button: PadButton::South, label: "Take photo".into(), hold: None },
                    Prompt { button: PadButton::East, label: "Back".into(), hold: None },
                ];
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
        // Wasted and Busted: the banner over a draining picture.
        if page == Page::Play {
            let t = self.fate.since();
            let banner = match self.fate {
                fate::Fate::Wasted { .. } => Some((BannerKind::Wasted, "WASTED", format!("Hospital bill ${}", fate::WASTED_FEE))),
                fate::Fate::Busted { .. } => {
                    Some((BannerKind::Busted, "BUSTED", format!("Fine ${} · weapons seized", fate::busted_fee(self.money))))
                }
                fate::Fate::Alive => None,
            };
            if let Some((kind, title, sub)) = banner {
                ui.banners.push(Banner { kind, title: title.into(), subtitle: Some(sub), age: t, duration: fate::LENGTH });
                ui.overlay.desaturate = (t / 1.2).clamp(0.0, 1.0) * if kind == BannerKind::Wasted { 1.0 } else { 0.6 };
                ui.overlay.vignette = 0.5;
            }
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
            Page::Play if !self.fate.alive() => ((self.fate.since() - (fate::LENGTH - 0.7)) / 0.6).clamp(0.0, 1.0),
            Page::Title | Page::Play => self.front.fade(self.ui_time, 0.6),
            _ => 0.0,
        };
        let menu = page != Page::Play;
        let mut draw = DrawList::default();
        let mut dialogue = false;
        if page == Page::Play {
            draw.ribbons.extend(self.gps.ribbon(feet, 160.0));
            self.mission_ui(sim, &mut ui, &mut draw);
            self.activities_ui(sim, &mut ui, &mut draw);
            self.street_ui(sim, &mut ui, &mut draw);
            // A mission's subtitled lines duck the radio; with none, the scanner's words, else
            // the talk station's.
            dialogue = ui.subtitle.is_some();
            if !dialogue && self.settings.subtitles {
                ui.subtitle = rdef.as_ref().and_then(|d| self.radio_subtitle(d));
            }
            if shop_screen.is_some() {
                ui.screen = shop_screen;
            }
            // A card or a menu over play (a mission's cards, a counter): the HUD, the tips and the
            // play prompts make way for it.
            if ui.screen.is_some() {
                if let Some(h) = &mut ui.hud {
                    h.opacity = 0.0;
                }
                ui.help = None;
                ui.prompts.clear();
            }
        }
        if let Some(c) = &self.city {
            world::signal_lamps(c, self.camera(sim).target, self.time, &mut draw.prims);
            world::wheels(c, self.camera(sim).eye, self.time, &mut draw.assets);
        }
        self.combat.draw(&mut draw);
        if let (Some((from, vel)), true) = (self.combat.arc, page == Page::Play && self.combat.aiming) {
            draw.ribbons.push(RibbonDraw {
                points: combat::arc(sim, from, vel),
                width: 0.06,
                color: pav_core::Color::hex("#ffe14d"),
                emissive: 1.5,
            });
        }
        self.police.draw(sim, !(6.3..19.5).contains(&self.sky.hour), &mut draw);
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
            if let (Some(c), Some(d)) = (&self.chatter, rdef.as_deref()) {
                loops.extend(audio::scanner_loop(c, d));
            }
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
                // Menus duck the radio and the city most, subtitled dialogue a little less.
                ducking: if menu {
                    audio::DUCK_MENU
                } else if dialogue {
                    audio::DUCK_DIALOGUE
                } else {
                    0.0
                },
                listener,
                loops,
                // The radio sits under the engines and the effects.
                music: rdef.as_ref().and_then(|d| self.radio.music(d, self.radio.clock, self.settings.music * audio::RADIO)),
                ..Default::default()
            },
            map: self.city.as_ref().map(|c| c.map.clone()),
            far: self.city.as_ref().map(|c| c.far.clone()),
            app: AppRequests {
                save: self.save_req.clone(),
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

/// The controls play gets: nothing while something else holds them (`held`; the commands still
/// pass), less the buttons in `take` (the phone's, a counter's prompt), and no walking while the
/// switch wheel's stick picks.
fn controls(input: &InputFrame, held: bool, take: u32, wheel: bool) -> InputFrame {
    let mut f = input.clone();
    if held {
        f.pad = Default::default();
        f.move_dir = Vec2::ZERO;
        f.held = 0;
        f.pressed = 0;
        f.mouse_delta = Vec2::ZERO;
        return f;
    }
    f.pad.held &= !take;
    f.pad.pressed &= !take;
    if take & pav_core::input::pad::R2 != 0 {
        f.pad.r2 = 0.0;
    }
    if wheel {
        f.pad.stick_l = Vec2::ZERO;
        f.move_dir = Vec2::ZERO;
        f.pad.held &= !pav_core::input::pad::DOWN;
    }
    f
}

impl Freeway {
    /// A new game: everyone as the profiles start them, Dez on Sunset Row, nothing played yet.
    fn new_game(&mut self, sim: &mut Sim) {
        if self.missions.run.is_some() {
            self.end_mission(sim);
        }
        self.crew = protagonists::Crew::new(0);
        self.crew.unlocked = true;
        self.missions = missions::Missions::default();
        self.activities = activities::Activities::default();
        self.phone = phone::Phone::default();
        self.load_member(0);
        self.dress_player(sim);
        self.heat.clear();
        self.police.clear(sim);
        self.put_player(sim, world::SPAWN, std::f32::consts::FRAC_PI_2);
    }

    /// A protagonist found at a life spot plays its idle until they move.
    fn life_idle(&mut self, sim: &mut Sim) {
        if !self.crew.life_idle {
            return;
        }
        let moving = sim.player().and_then(|p| p.character.as_ref()).is_some_and(|c| c.vel.length() > 0.5);
        if moving {
            self.crew.life_idle = false;
            self.dress_player(sim);
        }
    }
}
