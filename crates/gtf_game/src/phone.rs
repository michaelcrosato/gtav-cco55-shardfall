//! The phone (PLAN.md §5.6.9): D-pad up brings it up, bottom right, sized for the TV. Nine big
//! tiles — Contacts (calling starts a contact's mission), Messages (story texts, unlocks, spam),
//! Camera (a framed photo mode: a mission's photo targets count when centred, near and in
//! sight), Map (a waypoint to a place), Jobs (missions to pick up), Radio, Settings, Gallery and
//! the satirical LifeHakr. The D-pad moves, ✕ opens, ○ backs out.

use glam::{Vec2, Vec3};
use pav_core::InputFrame;
use pav_core::entity::EntityId;
use pav_core::frame::SimEvent;
use pav_core::input::pad;
use pav_core::present::{CameraIntent, Phone as PhoneUi, PhoneRow, PhoneTile};
use pav_core::sim::Sim;
use serde::{Deserialize, Serialize};

use crate::Freeway;
use crate::pad::Nav;

/// The home screen's tiles: id, label, icon.
pub const APPS: [(&str, &str, &str); 9] = [
    ("contacts", "Contacts", "contacts"),
    ("messages", "Messages", "messages"),
    ("camera", "Camera", "camera"),
    ("map", "Map", "map"),
    ("jobs", "Jobs", "jobs"),
    ("radio", "Radio", "music"),
    ("settings", "Settings", "settings"),
    ("gallery", "Gallery", "star"),
    ("lifehakr", "LifeHakr", "info"),
];

/// LifeHakr's tips.
const TIPS: [&str; 8] = [
    "Save on gas: drive downhill everywhere.",
    "Productivity hack: a two-hour commute is two hours of podcasts.",
    "Ten habits of people who own a freeway. Number four: tolls.",
    "Cold showers build character. So does a cold apartment.",
    "Why rent when you can sleep in your car? Equity!",
    "Manifest your dream car. Then repossess it.",
    "Networking tip: everyone at the diner is a contact.",
    "Mindfulness: be present. Especially when the police arrive.",
];

/// How far a photo reaches (m), and how near the middle of the frame its subject must be.
const PHOTO_RANGE: f32 = 60.0;
const PHOTO_CONE: f32 = 12.0;

/// A text message.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub from: String,
    pub body: String,
    pub read: bool,
}

/// A photo in the gallery (what it shows, not pixels).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Photo {
    pub clock: String,
    pub at: Vec2,
    pub subjects: u32,
}

/// The phone (part of the game's state).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Phone {
    pub open: bool,
    /// `home` or an app's id, the focus in it, and the message open in Messages.
    pub app: String,
    pub focus: usize,
    pub reading: Option<usize>,
    pub nav: Nav,
    pub messages: Vec<Message>,
    pub gallery: Vec<Photo>,
    /// Photos taken (missions count from it), what the last one caught, and seconds since the
    /// shutter.
    pub photos: u64,
    pub subjects: Vec<EntityId>,
    pub flash: f32,
    pub calls: u32,
}

impl Phone {
    /// A text arrives.
    pub fn receive(&mut self, from: &str, body: &str) {
        self.messages.insert(0, Message { from: from.into(), body: body.into(), read: false });
        self.messages.truncate(30);
    }

    pub fn unread(&self) -> u32 {
        self.messages.iter().filter(|m| !m.read).count() as u32
    }

    /// The camera app is up (the view becomes the phone's).
    pub fn camera(&self) -> bool {
        self.open && self.app == "camera"
    }

    fn close(&mut self) {
        self.open = false;
        self.app = "home".into();
        self.reading = None;
        self.focus = 0;
    }
}

/// The buttons the phone takes while it is up (the rest still move the player).
pub const TAKES: u32 = pad::UP | pad::DOWN | pad::LEFT | pad::RIGHT | pad::SOUTH | pad::EAST | pad::R2;

impl Freeway {
    /// A text message: into the phone, with a notification.
    pub(crate) fn text_message(&mut self, from: &str, body: &str) {
        self.phone.receive(from, body);
        self.note("messages", from, body);
    }

    /// The contacts: the other protagonists, then the people missions come from.
    fn contacts(&self) -> Vec<(String, String)> {
        let mut v: Vec<(String, String)> = Vec::new();
        if let Ok(d) = crate::protagonists::def() {
            for (i, p) in d.protagonist.iter().enumerate() {
                if i != self.crew.current {
                    v.push((p.key.clone(), p.name.clone()));
                }
            }
        }
        for d in crate::missions::all() {
            if let Some(c) = &d.contact
                && !v.iter().any(|x| &x.0 == c)
            {
                v.push((c.clone(), d.names.get(c).cloned().unwrap_or_else(|| c.to_uppercase())));
            }
        }
        v.push(("customs".into(), "Paloma Customs".into()));
        v
    }

    /// The places the map app lists: every landmark with a map icon, nearest first.
    fn places(&self, from: Vec3) -> Vec<(String, Vec2)> {
        let Some(city) = self.city.as_ref() else { return Vec::new() };
        let mut v: Vec<(String, Vec2)> = crate::world::blips(city).into_iter().filter_map(|b| Some((b.label?, b.pos))).collect();
        let me = Vec2::new(from.x, from.z);
        v.sort_by(|a, b| a.1.distance(me).total_cmp(&b.1.distance(me)));
        v
    }

    /// The missions the jobs app lists, nearest first.
    fn jobs(&self, from: Vec3) -> Vec<(String, String, Vec3)> {
        let mut v: Vec<(String, String, Vec3)> = self
            .open_missions()
            .into_iter()
            .filter_map(|(k, p)| crate::missions::get(&k).ok().map(|d| (k, d.title.clone(), p)))
            .collect();
        v.sort_by(|a, b| a.2.distance(from).total_cmp(&b.2.distance(from)));
        v
    }

    /// One tick of the phone: up and down, the apps, the camera's shutter.
    pub(crate) fn phone_tick(&mut self, sim: &mut Sim, input: &InputFrame, real: f32, events: &mut Vec<SimEvent>) {
        self.phone.flash += real;
        let pressed = |b: u32| input.pad.pressed & b != 0;
        if !self.phone.open {
            if pressed(pad::UP) && self.fate.alive() && !self.drive.busy() {
                self.phone.open = true;
                self.phone.app = "home".into();
                self.phone.focus = 0;
                events.push(SimEvent::Sound { cue: "phone.open".into(), pos: None, gain: 1.0, pitch: 1.0 });
            }
            return;
        }
        if !self.fate.alive() {
            self.phone.close();
            return;
        }
        let (dx, dy) = self.phone.nav.step(input, real);
        let feet = Self::feet(sim);
        let accept = pressed(pad::SOUTH);
        let back = pressed(pad::EAST);
        let app = self.phone.app.clone();
        let tick =
            |events: &mut Vec<SimEvent>| events.push(SimEvent::Sound { cue: "ui.move".into(), pos: None, gain: 0.6, pitch: 1.4 });
        match app.as_str() {
            "home" => {
                let f = self.phone.focus as i32;
                let moved = (f + dx + dy * 3).rem_euclid(APPS.len() as i32) as usize;
                if moved != self.phone.focus {
                    self.phone.focus = moved;
                    tick(events);
                }
                if back {
                    self.phone.close();
                    events.push(SimEvent::Sound { cue: "phone.close".into(), pos: None, gain: 1.0, pitch: 1.0 });
                } else if accept {
                    let id = APPS[self.phone.focus].0;
                    events.push(SimEvent::Sound { cue: "ui.accept".into(), pos: None, gain: 0.7, pitch: 1.3 });
                    if id == "settings" {
                        self.phone.close();
                        self.front.go(crate::frontend::Page::Settings, self.ui_time);
                    } else {
                        self.phone.app = id.into();
                        self.phone.focus = 0;
                        self.phone.reading = None;
                    }
                }
            }
            "camera" => {
                if back {
                    self.phone.app = "home".into();
                    self.phone.focus = 2;
                } else if accept || pressed(pad::R2) {
                    self.take_photo(sim, events);
                }
            }
            _ => {
                let n = self.app_rows(feet).len().max(1);
                if self.phone.reading.is_none() {
                    let moved = crate::pad::move_focus(self.phone.focus, n, dy, |_| true);
                    if moved != self.phone.focus {
                        self.phone.focus = moved;
                        tick(events);
                    }
                }
                if back {
                    if self.phone.reading.is_some() {
                        self.phone.reading = None;
                    } else {
                        let home = APPS.iter().position(|a| a.0 == app).unwrap_or(0);
                        self.phone.app = "home".into();
                        self.phone.focus = home;
                    }
                } else if accept {
                    self.app_accept(sim, &app, feet, events);
                }
            }
        }
    }

    /// ✕ on a row of an app.
    fn app_accept(&mut self, sim: &mut Sim, app: &str, feet: Vec3, events: &mut Vec<SimEvent>) {
        let i = self.phone.focus;
        match app {
            "messages" => {
                if i < self.phone.messages.len() {
                    self.phone.messages[i].read = true;
                    self.phone.reading = Some(i);
                }
            }
            "contacts" => {
                let Some((key, name)) = self.contacts().get(i).cloned() else { return };
                self.phone.calls += 1;
                events.push(SimEvent::Sound { cue: "phone.ring".into(), pos: None, gain: 1.0, pitch: 1.0 });
                // A contact with a mission ready starts it; the rest have a word.
                let job = crate::missions::all()
                    .into_iter()
                    .find(|d| d.contact.as_deref() == Some(key.as_str()) && self.missions.open(d));
                match job {
                    Some(d) if self.missions.run.is_none() => {
                        self.phone.close();
                        if let Err(e) = self.start_mission(sim, &d.key, false, d.blip.is_none(), events) {
                            log::warn!("{e:#}");
                        }
                    }
                    _ => self.note("contacts", &name, "Not now. I'll call you."),
                }
            }
            "map" => {
                if let Some((name, p)) = self.places(feet).get(i).cloned() {
                    self.front.waypoint = Some(p);
                    self.note("map", "Waypoint", &name);
                    self.phone.close();
                }
            }
            "jobs" => {
                if let Some((_, title, p)) = self.jobs(feet).get(i).cloned() {
                    self.front.waypoint = Some(Vec2::new(p.x, p.z));
                    self.note("jobs", "Waypoint", &title);
                    self.phone.close();
                }
            }
            "radio" => {
                if let Ok(def) = crate::radio::def() {
                    let pick = (i < def.station.len()).then_some(i);
                    self.radio.tune(pick);
                    if let Some(p) = pick {
                        self.radio.last = p;
                    }
                    self.drive.card = 0.0;
                }
            }
            _ => {}
        }
    }

    /// The shutter: what is in the middle of the frame, near and in sight, is in the photo.
    fn take_photo(&mut self, sim: &Sim, events: &mut Vec<SimEvent>) {
        let cam = self.phone_view(sim);
        let fwd = (cam.target - cam.eye).normalize_or(Vec3::NEG_Z);
        let me = sim.state.player;
        let subjects: Vec<EntityId> = sim
            .state
            .entities
            .iter()
            .filter(|e| Some(e.id) != me && (e.character.is_some() || e.vehicle.is_some()))
            .filter(|e| {
                let aim = if e.character.is_some() { e.pos + Vec3::Y * 0.4 } else { e.pos };
                let d = aim - cam.eye;
                let len = d.length();
                len < PHOTO_RANGE
                    && len > 0.5
                    && (d / len).dot(fwd) > PHOTO_CONE.to_radians().cos()
                    && sim.raycast_static(cam.eye, d / len, len - 0.5).is_none()
            })
            .map(|e| e.id)
            .collect();
        self.phone.photos += 1;
        self.phone.flash = 0.0;
        self.phone.gallery.insert(
            0,
            Photo {
                clock: crate::daytime::format_time(self.sky.hour),
                at: Vec2::new(cam.eye.x, cam.eye.z),
                subjects: subjects.len() as u32,
            },
        );
        self.phone.gallery.truncate(40);
        self.phone.subjects = subjects;
        events.push(SimEvent::Sound { cue: "camera.shutter".into(), pos: None, gain: 1.0, pitch: 1.0 });
    }

    /// The phone camera's view: from the player's eyes along the camera's heading.
    pub fn phone_view(&self, sim: &Sim) -> CameraIntent {
        let feet = Self::feet(sim);
        let (s, c) = self.cam.yaw.sin_cos();
        let pitch = (0.35 - self.cam.elevation).clamp(-0.9, 0.9) * 0.8;
        let dir = Vec3::new(s * pitch.cos(), pitch.sin(), -c * pitch.cos());
        let eye = feet + Vec3::Y * 1.62 + Vec3::new(s, 0.0, -c) * 0.25;
        CameraIntent { eye, target: eye + dir * 10.0, up: Vec3::Y, fov: 50.0, near: 0.1, far: 1500.0, cut: false, shake: 0.0 }
    }

    /// The rows of the open app.
    fn app_rows(&self, feet: Vec3) -> Vec<PhoneRow> {
        let row = |t: String, d: String| PhoneRow { title: t, detail: d };
        let metres = |p: Vec2| {
            let d = p.distance(Vec2::new(feet.x, feet.z));
            if d >= 1000.0 { format!("{:.1} km", d / 1000.0) } else { format!("{d:.0} m") }
        };
        match self.phone.app.as_str() {
            "contacts" => self.contacts().into_iter().map(|(_, n)| row(n, "Call".into())).collect(),
            "messages" => match self.phone.reading.and_then(|i| self.phone.messages.get(i)) {
                Some(m) => vec![row(m.from.clone(), m.body.clone())],
                None if self.phone.messages.is_empty() => vec![row("No messages".into(), String::new())],
                None => self
                    .phone
                    .messages
                    .iter()
                    .map(|m| row(format!("{}{}", if m.read { "" } else { "● " }, m.from), m.body.chars().take(40).collect()))
                    .collect(),
            },
            "map" => self.places(feet).into_iter().map(|(n, p)| row(n, metres(p))).collect(),
            "jobs" => {
                let v: Vec<PhoneRow> = self.jobs(feet).into_iter().map(|(_, t, p)| row(t, metres(Vec2::new(p.x, p.z)))).collect();
                if v.is_empty() { vec![row("Nothing on right now".into(), String::new())] } else { v }
            }
            "radio" => {
                let mut v: Vec<PhoneRow> = crate::radio::def()
                    .map(|d| d.station.iter().map(|s| row(s.name.clone(), s.freq.clone())).collect())
                    .unwrap_or_default();
                v.push(row("Off".into(), String::new()));
                v
            }
            "gallery" => {
                if self.phone.gallery.is_empty() {
                    vec![row("No photos yet".into(), String::new())]
                } else {
                    self.phone
                        .gallery
                        .iter()
                        .map(|p| row(p.clock.clone(), format!("{} in frame · {}", p.subjects, metres(p.at))))
                        .collect()
                }
            }
            "lifehakr" => TIPS.iter().map(|t| row((*t).into(), String::new())).collect(),
            _ => Vec::new(),
        }
    }

    /// The phone as the UI kit draws it.
    pub(crate) fn phone_ui(&self, sim: &Sim) -> Option<PhoneUi> {
        if !self.phone.open {
            return None;
        }
        let feet = Self::feet(sim);
        let clock = crate::daytime::format_time(self.sky.hour);
        let app = self.phone.app.as_str();
        let title = APPS.iter().find(|a| a.0 == app).map_or("Phone", |a| a.1).to_string();
        let tiles = if app == "home" {
            APPS.iter()
                .map(|(id, label, icon)| PhoneTile {
                    id: (*id).into(),
                    label: (*label).into(),
                    icon: (*icon).into(),
                    badge: if *id == "messages" { self.phone.unread() } else { 0 },
                })
                .collect()
        } else {
            Vec::new()
        };
        let rows = if app == "camera" {
            vec![PhoneRow { title: "~btn:south~ Take a photo".into(), detail: format!("{} taken", self.phone.photos) }]
        } else if app == "home" {
            Vec::new()
        } else {
            self.app_rows(feet)
        };
        Some(PhoneUi { app: app.into(), title, tiles, rows, focus: self.phone.focus, clock })
    }
}
