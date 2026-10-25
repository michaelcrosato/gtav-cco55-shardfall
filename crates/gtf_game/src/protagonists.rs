//! The three protagonists (PLAN.md §5.4) — Dez, Mack and Roxy — from
//! `content/freeway/protagonists.toml`. Each keeps their own cash, weapons and ammunition,
//! health, stats, ability meter and whereabouts. Holding D-pad down opens the switch wheel; the
//! switch flies the sky-cam up from one and down to the other (the ground there loads on the way),
//! who is found where they were left or, after a while, at one of their life spots, busy. Their
//! special abilities (L3 + R3) last up to ten seconds of a full meter, which play style refills:
//! Slipstream slows the world and grips the road at the wheel, Deadeye slows it for the aim,
//! Overdrive doubles the damage dealt and halves the damage taken.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use glam::Vec3;
use pav_core::Color;
use pav_core::InputFrame;
use pav_core::entity::EntityId;
use pav_core::frame::SimEvent;
use pav_core::input::pad;
use pav_core::present::{CameraIntent, Card, CardRow, Wheel, WheelSlot};
use pav_core::puppet::{BagKind, BeardStyle, HairStyle, HelmKind, PuppetDef};
use pav_core::sim::Sim;
use serde::{Deserialize, Serialize};

use crate::Freeway;
use crate::hud::WheelState;

fn one() -> f32 {
    1.0
}

fn all_day() -> [f32; 2] {
    [0.0, 24.0]
}

/// Stats, 0-100.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stats {
    pub driving: f32,
    pub shooting: f32,
    pub stamina: f32,
    pub strength: f32,
    pub stealth: f32,
}

/// A protagonist's look: the clothes' colours, a baseball cap's colour (`cap`), hair and beard
/// (styles as `pav_core::puppet::HairStyle` and `BeardStyle` name them), sunglasses' and a
/// bag's colours (`bag` is `backpack` or `shoulder`), height (`scale`) and build.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookDef {
    pub skin: String,
    pub shirt: String,
    pub pants: String,
    pub shoes: String,
    #[serde(default)]
    pub cap: Option<String>,
    #[serde(default)]
    pub eyes: Option<String>,
    #[serde(default)]
    pub hair: Option<String>,
    #[serde(default)]
    pub hair_color: Option<String>,
    #[serde(default)]
    pub beard: Option<String>,
    #[serde(default)]
    pub glasses: Option<String>,
    #[serde(default)]
    pub bag: Option<String>,
    #[serde(default)]
    pub bag_color: Option<String>,
    #[serde(default = "one")]
    pub scale: f32,
    #[serde(default = "one")]
    pub build: f32,
}

impl LookDef {
    /// What is wrong with it: unknown styles or bags, colours that are not `#rrggbb`.
    pub fn check(&self) -> Result<(), String> {
        crate::looks::named::<HairStyle>("hair", self.hair.as_deref())?;
        crate::looks::named::<BeardStyle>("beard", self.beard.as_deref())?;
        crate::looks::named::<BagKind>("bag", self.bag.as_deref())?;
        let colours = [&self.cap, &self.eyes, &self.hair_color, &self.glasses, &self.bag_color];
        if let Some(c) = colours.into_iter().flatten().find(|c| pav_core::Color::try_hex(c).is_none()) {
            return Err(format!("colour '{c}' is not #rrggbb"));
        }
        Ok(())
    }
}

/// Somewhere a protagonist passes the time when not played.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LifeDef {
    pub at: String,
    pub clip: String,
    pub text: String,
    #[serde(default = "all_day")]
    pub hours: [f32; 2],
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtagonistDef {
    pub key: String,
    pub name: String,
    pub short: String,
    pub letter: char,
    pub color: String,
    pub ability: String,
    pub cash: i64,
    pub weapons: Vec<String>,
    pub stats: Stats,
    pub vehicle: String,
    pub voice: f32,
    pub home: String,
    /// Where they save (an anchor; none: `home`).
    #[serde(default)]
    pub bed: Option<String>,
    pub look: LookDef,
    #[serde(default)]
    pub life: Vec<LifeDef>,
}

impl ProtagonistDef {
    /// Their look as a puppet (the game's biped, `crate::looks`).
    pub fn puppet(&self) -> PuppetDef {
        let l = &self.look;
        let body = crate::looks::body();
        let mut p = body.biped();
        p.skin = l.skin.clone();
        p.shirt = l.shirt.clone();
        p.pants = l.pants.clone();
        p.shoes = l.shoes.clone();
        crate::peds::shape(&mut p, &body, body.height() * l.scale, l.build);
        if let Some(cap) = &l.cap {
            p.gear.helm = HelmKind::Ballcap;
            p.gear.helm_color = cap.clone();
        }
        if let Some(eyes) = &l.eyes {
            p.eyes = eyes.clone();
        }
        // (Checked when the file is read.)
        p.hair = crate::looks::named("hair", l.hair.as_deref()).ok().flatten().unwrap_or_default();
        p.beard = crate::looks::named("beard", l.beard.as_deref()).ok().flatten().unwrap_or_default();
        p.hair_color = l.hair_color.clone().unwrap_or_default();
        p.gear.glasses = l.glasses.clone().unwrap_or_default();
        p.gear.bag = crate::looks::named("bag", l.bag.as_deref()).ok().flatten().unwrap_or_default();
        p.gear.bag_color = l.bag_color.clone().unwrap_or_default();
        p.idle_clip = "MESH2MOTION/Idle_A".into();
        p
    }

    pub fn tint(&self) -> Color {
        Color::hex(&self.color)
    }
}

/// A special ability's tuning.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbilityDef {
    pub name: String,
    pub seconds: f32,
    #[serde(default = "one")]
    pub time_scale: f32,
    #[serde(default)]
    pub grip: f32,
    #[serde(default = "one")]
    pub assist: f32,
    #[serde(default = "one")]
    pub damage: f32,
    #[serde(default = "one")]
    pub taken: f32,
    #[serde(default = "one")]
    pub knockback: f32,
    #[serde(default)]
    pub on_foot: bool,
    #[serde(default)]
    pub in_car: bool,
    pub tint: [f32; 4],
    #[serde(default)]
    pub fill: BTreeMap<String, f32>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrewDef {
    pub protagonist: Vec<ProtagonistDef>,
    pub ability: BTreeMap<String, AbilityDef>,
}

impl CrewDef {
    pub fn index(&self, key: &str) -> Option<usize> {
        self.protagonist.iter().position(|p| p.key == key)
    }

    pub fn ability_of(&self, i: usize) -> Option<&AbilityDef> {
        self.ability.get(&self.protagonist.get(i)?.ability)
    }
}

/// The protagonists (cached; re-read when the file changes during native hot reload).
pub fn def() -> Result<Arc<CrewDef>> {
    static CACHE: Mutex<Option<(String, Arc<CrewDef>)>> = Mutex::new(None);
    let text = crate::content::text("protagonists.toml", include_str!("../../../content/freeway/protagonists.toml"));
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((t, d)) = cache.as_ref()
        && *t == *text
    {
        return Ok(d.clone());
    }
    let d: CrewDef = toml::from_str(&text).map_err(|e| anyhow!("content/freeway/protagonists.toml: {e}"))?;
    if d.protagonist.is_empty() {
        return Err(anyhow!("content/freeway/protagonists.toml: nobody"));
    }
    for p in &d.protagonist {
        if !d.ability.contains_key(&p.ability) {
            return Err(anyhow!("content/freeway/protagonists.toml: {}: no ability '{}'", p.key, p.ability));
        }
        p.look.check().map_err(|e| anyhow!("content/freeway/protagonists.toml: {}: look: {e}", p.key))?;
    }
    let d = Arc::new(d);
    *cache = Some((text.into_owned(), d.clone()));
    Ok(d)
}

/// Holding D-pad down this long opens the switch wheel (s, real time).
const WHEEL_DELAY: f32 = 0.2;
/// The sky-cam: rising (s), crossing at least this long (s; longer while the ground there
/// loads), dropping (s), and its height (m).
const RISE: f32 = 1.1;
const TRAVEL: f32 = 1.3;
const DROP: f32 = 1.2;
const HIGH: f32 = 170.0;
/// A protagonist this near (m) is switched to with a quick blend instead of the sky-cam (s).
const NEAR: f32 = 120.0;
const BLEND: f32 = 0.8;
/// Switched back within this long (game seconds), a protagonist is where they were left.
const RETURN: f32 = 120.0;
/// Seconds the switch card (what they were doing) shows.
pub const CARD: f32 = 5.0;

/// One protagonist's own things (part of the game's state). The one being played keeps cash,
/// ammunition, the weapon in hand and health in the game's live fields; these are written back
/// on switching and saving.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Member {
    pub key: String,
    pub cash: i64,
    pub stats: Stats,
    /// Weapons owned (by weapon index), rounds in each magazine and in reserve, the weapon in
    /// hand and the one before.
    pub owned: Vec<bool>,
    pub mag: Vec<u32>,
    pub reserve: Vec<u32>,
    pub weapon: usize,
    pub last_weapon: usize,
    pub health: f32,
    pub armor: f32,
    /// The ability meter (0..1).
    pub meter: f32,
    /// Where they were left (feet, facing), and game seconds since.
    pub at: Option<(Vec3, f32)>,
    pub away: f32,
}

impl Member {
    /// A protagonist as a new game starts them.
    pub fn new(p: &ProtagonistDef, weapons: &crate::combat::WeaponsDef) -> Member {
        let owned: Vec<bool> =
            weapons.weapon.iter().map(|w| w.id == "fists" || w.id == "holster" || p.weapons.contains(&w.id)).collect();
        let mag = weapons.weapon.iter().zip(&owned).map(|(w, &o)| if o { w.magazine } else { 0 }).collect();
        let reserve = weapons.weapon.iter().zip(&owned).map(|(w, &o)| if o { w.reserve } else { 0 }).collect();
        Member {
            key: p.key.clone(),
            cash: p.cash,
            stats: p.stats,
            owned,
            mag,
            reserve,
            weapon: 0,
            last_weapon: 1,
            health: crate::combat::MAX_HEALTH,
            armor: 0.0,
            meter: 0.5,
            at: None,
            away: RETURN * 10.0,
        }
    }
}

/// A switch under way.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Switch {
    pub to: usize,
    /// Where the switch began (the old player's feet) and the camera then, where it lands
    /// (feet, facing), seconds in, whether the bodies have changed over, and whether it is the
    /// sky-cam (far) or a quick blend (near).
    pub from: Vec3,
    pub eye0: Vec3,
    pub target0: Vec3,
    pub dest: Vec3,
    pub yaw: f32,
    pub t: f32,
    pub swapped: bool,
    pub long: bool,
    /// The life spot they are found at (index into their `life`), if any.
    pub life: Option<usize>,
}

/// Who is being played, the others' things, switching and the ability (part of the game's
/// state).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Crew {
    pub members: Vec<Member>,
    pub current: usize,
    /// Switching is open (story: after the intro missions; free roam: from the start).
    pub unlocked: bool,
    pub wheel: Option<WheelState>,
    pub switch: Option<Switch>,
    /// The ability is on.
    pub ability: bool,
    /// Other protagonists' bodies in the world (a mission's), by member index.
    pub bodies: BTreeMap<usize, EntityId>,
    /// Missions: whom the wheel may pick (None: anyone, once unlocked).
    pub allowed: Option<Vec<usize>>,
    /// What the protagonist switched to was doing, and seconds since the card appeared.
    pub card: Option<(String, f32)>,
    /// The player still plays a life spot's idle (until they move).
    pub life_idle: bool,
    /// Traffic passed close at speed lately: seconds before each counts again.
    pub near: Vec<(EntityId, f32)>,
    /// Counts for tests and stats.
    pub switches: u32,
    pub activations: u32,
}

impl Crew {
    /// A new game's crew: everyone as the profiles start them, playing `current`.
    pub fn new(current: usize) -> Crew {
        let (Ok(d), Ok(w)) = (def(), crate::combat::def()) else { return Crew::default() };
        Crew { members: d.protagonist.iter().map(|p| Member::new(p, &w)).collect(), current, ..Default::default() }
    }

    /// The protagonist being played.
    pub fn me(&self) -> Option<&Member> {
        self.members.get(self.current)
    }

    pub fn key(&self) -> &str {
        self.me().map_or("", |m| m.key.as_str())
    }

    /// Whether the wheel may pick `i` now.
    pub fn may_switch_to(&self, i: usize) -> bool {
        i != self.current && i < self.members.len() && self.allowed.as_ref().map_or(self.unlocked, |a| a.contains(&i))
    }

    /// Whether the player owns weapon `i`.
    pub fn owns(&self, i: usize) -> bool {
        self.me().is_none_or(|m| m.owned.get(i).copied().unwrap_or(false))
    }

    /// The meter fills by `kind` (an ability's `fill` key) times `amount`, unless the ability is
    /// on.
    pub fn fill(&mut self, kind: &str, amount: f32) {
        if self.ability || amount <= 0.0 {
            return;
        }
        let Ok(d) = def() else { return };
        let Some(rate) = d.ability_of(self.current).and_then(|a| a.fill.get(kind)).copied() else { return };
        if let Some(m) = self.members.get_mut(self.current) {
            m.meter = (m.meter + rate * amount).min(1.0);
        }
    }

    /// The switch wheel: everyone, those who cannot be picked now greyed.
    pub fn wheel_ui(&self, w: &WheelState) -> Wheel {
        let d = def().ok();
        let slots: Vec<WheelSlot> = self
            .members
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let p = d.as_ref().and_then(|d| d.protagonist.get(i));
                let ability = d.as_ref().and_then(|d| d.ability_of(i)).map(|a| a.name.clone()).unwrap_or_default();
                WheelSlot {
                    label: p.map_or(m.key.to_uppercase(), |p| p.short.clone()),
                    icon: p.map_or_else(|| m.key.clone(), |p| p.letter.to_string()),
                    detail: Some(if i == self.current {
                        "Playing".into()
                    } else if self.may_switch_to(i) {
                        ability
                    } else {
                        "Not now".into()
                    }),
                    enabled: i == self.current || self.may_switch_to(i),
                }
            })
            .collect();
        let focus = w.focus.unwrap_or(self.current).min(slots.len().saturating_sub(1));
        let center =
            slots.get(focus).map(|s| format!("{}\n{}", s.label, s.detail.clone().unwrap_or_default())).unwrap_or_default();
        Wheel { id: "switch".into(), slots, focus: Some(focus), center }
    }

    /// The pause menu's stats page: one big card per protagonist.
    pub fn stat_cards(&self, focus: usize, live_cash: i64) -> CardRow {
        let d = def().ok();
        CardRow {
            cards: self
                .members
                .iter()
                .enumerate()
                .map(|(i, m)| {
                    let p = d.as_ref().and_then(|d| d.protagonist.get(i));
                    let cash = if i == self.current { live_cash } else { m.cash };
                    let s = m.stats;
                    Card {
                        id: m.key.clone(),
                        title: p.map_or(m.key.to_uppercase(), |p| p.short.clone()),
                        subtitle: d.as_ref().and_then(|d| d.ability_of(i)).map(|a| a.name.clone()),
                        lines: vec![money_text(cash)],
                        bars: [
                            ("Driving", s.driving),
                            ("Shooting", s.shooting),
                            ("Stamina", s.stamina),
                            ("Strength", s.strength),
                            ("Stealth", s.stealth),
                        ]
                        .iter()
                        .map(|(l, v)| (l.to_string(), v / 100.0))
                        .collect(),
                        accent: p.map_or(Color::hex("#ffffff"), |p| p.tint()),
                        icon: None,
                        enabled: true,
                    }
                })
                .collect(),
            focus: focus.min(self.members.len().saturating_sub(1)),
        }
    }
}

fn cash(v: i64) -> String {
    let s = v.abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    format!("{}${out}", if v < 0 { "-" } else { "" })
}

/// Money as the HUD writes it.
pub fn money_text(v: i64) -> String {
    cash(v)
}

fn ease(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

impl Switch {
    /// Seconds the whole switch takes.
    pub fn length(&self) -> f32 {
        if self.long { RISE + TRAVEL + DROP } else { BLEND }
    }

    /// The camera during the switch; `landing` is where the new player's camera settles.
    pub fn camera(&self, landing: &CameraIntent) -> CameraIntent {
        let mut c = CameraIntent { eye: self.eye0, target: self.target0, fov: 55.0, near: 0.5, far: 2500.0, ..*landing };
        if !self.long {
            let k = ease(self.t / BLEND);
            c.eye = self.eye0.lerp(landing.eye, k);
            c.target = self.target0.lerp(landing.target, k);
            c.fov = landing.fov;
            return c;
        }
        let above = |p: Vec3| p + Vec3::new(0.0, HIGH, 6.0);
        let t = self.t;
        if t < RISE {
            let k = ease(t / RISE);
            c.eye = self.eye0.lerp(above(self.from), k);
            c.target = self.target0.lerp(self.from, k);
        } else if t < RISE + TRAVEL {
            let k = ease((t - RISE) / TRAVEL);
            let p = self.from.lerp(self.dest, k);
            c.eye = above(p);
            c.target = p;
        } else {
            let k = ease((t - RISE - TRAVEL) / DROP);
            c.eye = above(self.dest).lerp(landing.eye, k);
            c.target = self.dest.lerp(landing.target, k);
            c.fov = 55.0 + (landing.fov - 55.0) * k;
        }
        c
    }
}

impl Freeway {
    /// Writes the live cash, ammunition, weapon and health back into the played protagonist's
    /// record (before a switch, a save).
    pub(crate) fn stash_member(&mut self, sim: &Sim) {
        let feet = sim.state.player.and_then(|id| sim.feet(id));
        let facing = sim.player().and_then(|p| p.character.as_ref()).map_or(0.0, |c| c.facing);
        let i = self.crew.current;
        let Some(m) = self.crew.members.get_mut(i) else { return };
        m.cash = self.money;
        m.mag = self.combat.mag.clone();
        m.reserve = self.combat.reserve.clone();
        m.weapon = self.weapon;
        m.last_weapon = self.last_weapon;
        m.health = self.combat.health;
        m.armor = self.combat.armor;
        if let Some(f) = feet {
            m.at = Some((f, facing));
            m.away = 0.0;
        }
    }

    /// Takes protagonist `i`'s record into the live fields.
    pub(crate) fn load_member(&mut self, i: usize) {
        let Some(m) = self.crew.members.get(i).cloned() else { return };
        self.crew.current = i;
        self.money = m.cash;
        self.money_delta = 0;
        self.money_age = 99.0;
        if !m.mag.is_empty() {
            self.combat.mag = m.mag;
            self.combat.reserve = m.reserve;
        }
        self.weapon = if m.owned.get(m.weapon).copied().unwrap_or(false) { m.weapon } else { 0 };
        self.last_weapon = m.last_weapon;
        self.combat.health = m.health.max(1.0);
        self.combat.armor = m.armor;
    }

    /// Dresses the player's body as the protagonist being played.
    pub(crate) fn dress_player(&self, sim: &mut Sim) {
        let (Some(pid), Ok(d)) = (sim.state.player, def()) else { return };
        let Some(p) = d.protagonist.get(self.crew.current) else { return };
        let puppet = Arc::new(p.puppet());
        if let Some(c) = sim.state.entities.get_mut(pid).and_then(|e| e.character.as_mut()) {
            c.puppet = Some(puppet);
        }
    }

    /// Where protagonist `i` is found if switched to now: their body in the world, where they
    /// were left a short while ago, or a life spot for the hour (else their home). Returns
    /// (feet, facing, life spot).
    fn whereabouts(&self, sim: &mut Sim, i: usize) -> Option<(Vec3, f32, Option<usize>)> {
        if let Some(&body) = self.crew.bodies.get(&i)
            && let Some(f) = sim.feet(body)
        {
            let facing = sim.state.entities.get(body).and_then(|e| e.character.as_ref()).map_or(0.0, |c| c.facing);
            return Some((f, facing, None));
        }
        let m = self.crew.members.get(i)?;
        if let Some((at, yaw)) = m.at
            && m.away < RETURN
        {
            return Some((at, yaw, None));
        }
        let city = self.city.clone()?;
        let d = def().ok()?;
        let p = d.protagonist.get(i)?;
        let hour = self.sky.hour;
        let open: Vec<usize> = p
            .life
            .iter()
            .enumerate()
            .filter(|(_, l)| (l.hours[0]..l.hours[1]).contains(&hour))
            .filter(|(_, l)| crate::anchor::Anchor::parse(&l.at).is_ok_and(|a| a.exists(&city)))
            .map(|(k, _)| k)
            .collect();
        if !open.is_empty() {
            let k = open[sim.state.rng.below(open.len() as u32) as usize];
            let a = crate::anchor::Anchor::parse(&p.life[k].at).ok()?;
            if let Some((feet, yaw)) = crate::anchor::foot(&city, &a) {
                return Some((feet, yaw, Some(k)));
            }
        }
        let home = crate::anchor::Anchor::parse(&p.home).ok().and_then(|a| crate::anchor::foot(&city, &a));
        home.map(|(f, y)| (f, y, None))
    }

    /// Starts switching to protagonist `to` (the wheel, a mission). False when it cannot.
    pub fn begin_switch(&mut self, sim: &mut Sim, to: usize, events: &mut Vec<SimEvent>) -> bool {
        if self.crew.switch.is_some() || to == self.crew.current || to >= self.crew.members.len() || !self.fate.alive() {
            return false;
        }
        let Some(from) = sim.state.player.and_then(|id| sim.feet(id)) else { return false };
        let Some((dest, yaw, life)) = self.whereabouts(sim, to) else { return false };
        let cam = self.camera(sim);
        let long = !self.crew.bodies.contains_key(&to) || dest.distance(from) > NEAR;
        self.crew.switch =
            Some(Switch { to, from, eye0: cam.eye, target0: cam.target, dest, yaw, t: 0.0, swapped: false, long, life });
        self.crew.wheel = None;
        if self.crew.ability {
            self.crew.ability = false;
        }
        events.push(SimEvent::Sound { cue: "switch.whoosh".into(), pos: None, gain: 1.0, pitch: if long { 0.9 } else { 1.2 } });
        true
    }

    /// The switch under way: the camera's time runs on (in real seconds), the ground at the far
    /// end loads, and the bodies change over once it has.
    pub(crate) fn switch_tick(&mut self, sim: &mut Sim, real: f32, events: &mut Vec<SimEvent>) {
        let Some(mut s) = self.crew.switch.take() else { return };
        sim.state.world.interest.push(s.dest);
        s.t += real;
        let at = if s.long { RISE + TRAVEL } else { 0.0 };
        if !s.swapped && s.t >= at {
            if s.long && !crate::world::ground_loaded(sim, s.dest) {
                // Hold over the far end until it is there to stand on.
                s.t = at;
            } else {
                self.swap(sim, &s, events);
                s.swapped = true;
            }
        }
        if s.t < s.length() {
            self.crew.switch = Some(s);
        }
    }

    /// The bodies change over: protagonist `s.to` is played from here.
    fn swap(&mut self, sim: &mut Sim, s: &Switch, events: &mut Vec<SimEvent>) {
        let Ok(d) = def() else { return };
        let Some(p) = d.protagonist.get(s.to) else { return };
        let old = sim.state.player;
        let was = self.crew.current;
        self.stash_member(sim);
        // The one left behind: a far switch takes them out of the world (they are remembered
        // where they were); a near one leaves them standing (or sitting) there.
        let car = self.drive.vehicle();
        if let Some(old) = old {
            if s.long {
                if let Some(car) = car {
                    sim.exit_vehicle(old, car);
                }
                sim.despawn(old);
            } else {
                self.crew.bodies.insert(was, old);
                if let Some(e) = sim.state.entities.get_mut(old) {
                    e.unload = pav_core::entity::Unload::Keep;
                }
            }
        }
        let body = match self.crew.bodies.remove(&s.to).filter(|b| sim.state.entities.get(*b).is_some()) {
            Some(b) => b,
            None => {
                let mut puppet = p.puppet();
                if let Some(l) = s.life.and_then(|k| p.life.get(k)) {
                    puppet.idle_clip = l.clip.clone();
                    self.crew.life_idle = true;
                }
                sim.spawn_npc(&p.key, s.dest, s.yaw, puppet, None, None)
            }
        };
        if let Some(e) = sim.state.entities.get_mut(body) {
            e.unload = pav_core::entity::Unload::Keep;
        }
        sim.state.player = Some(body);
        if let Some(f) = sim.feet(body) {
            sim.state.focus = f;
        }
        self.load_member(s.to);
        // Whatever the old player was doing with their hands and feet stops.
        self.wheel = None;
        self.radio_wheel = None;
        self.cover = Default::default();
        self.foot = Default::default();
        self.combat.aiming = false;
        self.drive.siren = false;
        let riding = sim.state.entities.get(body).and_then(|e| e.character.as_ref()).and_then(|c| c.riding);
        self.drive.phase = match riding {
            Some(car) => crate::drive::Phase::In { car },
            None => crate::drive::Phase::Foot,
        };
        match riding.and_then(|car| crate::drive::Car::read(sim, car)) {
            Some(c) => {
                self.chase.place_behind(c.yaw);
                self.radio.tune(c.radio.then_some(self.radio.last));
            }
            None => {
                self.radio.tune(None);
                self.cam.face(std::f32::consts::PI - s.yaw);
            }
        }
        if let Some(city) = self.city.clone() {
            self.traffic.autopilot(sim, &city, None);
        }
        self.bot.stop();
        self.crew.switches += 1;
        let text = s.life.and_then(|k| p.life.get(k)).map(|l| l.text.clone());
        self.crew.card = Some((text.unwrap_or_else(|| p.name.clone()), 0.0));
        events.push(SimEvent::Sound { cue: "switch.land".into(), pos: None, gain: 1.0, pitch: 1.0 });
    }

    /// Plays protagonist `i` at once (a mission's start, a checkpoint, a skip): their body in
    /// the world if they have one there, else they take the player's place. No sky-cam.
    pub(crate) fn force_protagonist(&mut self, sim: &mut Sim, i: usize) {
        if i == self.crew.current || i >= self.crew.members.len() {
            return;
        }
        let Some(pid) = sim.state.player else { return };
        let feet = sim.feet(pid).unwrap_or(sim.state.focus);
        let facing = sim.state.entities.get(pid).and_then(|e| e.character.as_ref()).map_or(0.0, |c| c.facing);
        let body = self.crew.bodies.get(&i).copied().filter(|b| sim.state.entities.get(*b).is_some());
        let (dest, yaw) = match body.and_then(|b| sim.feet(b)) {
            Some(f) => (f, sim.state.entities.get(body.unwrap()).and_then(|e| e.character.as_ref()).map_or(0.0, |c| c.facing)),
            None => (feet, facing),
        };
        let cam = self.camera(sim);
        let s = Switch {
            to: i,
            from: feet,
            eye0: cam.eye,
            target0: cam.target,
            dest,
            yaw,
            t: 0.0,
            swapped: false,
            long: body.is_none(),
            life: None,
        };
        let mut quiet = Vec::new();
        self.crew.ability = false;
        self.swap(sim, &s, &mut quiet);
        self.crew.card = None;
    }

    /// Hold D-pad down: the switch wheel opens (slow motion, as the weapon wheel); the stick
    /// picks, letting go switches.
    pub(crate) fn switch_wheel(&mut self, sim: &mut Sim, input: &InputFrame, real: f32, events: &mut Vec<SimEvent>) {
        let down = input.pad.held & pad::DOWN != 0;
        let n = self.crew.members.len();
        match (&mut self.crew.wheel, down) {
            (None, true) if input.pad.pressed & pad::DOWN != 0 => self.crew.wheel = Some(WheelState::default()),
            (Some(w), true) => {
                w.open_for += real;
                if w.open_for >= WHEEL_DELAY {
                    w.aim_n(input.pad.stick_l, n);
                }
            }
            (Some(w), false) => {
                let (open_for, focus) = (w.open_for, w.focus);
                self.crew.wheel = None;
                if open_for >= WHEEL_DELAY
                    && let Some(to) = focus
                {
                    if self.heat.stars > 0 && self.missions.run.is_none() {
                        self.note("heat", "Not now", "Lose the police before switching.");
                    } else if self.crew.may_switch_to(to) {
                        self.begin_switch(sim, to, events);
                    }
                }
            }
            _ => {}
        }
    }

    /// The switch wheel is open (past the hold).
    pub fn switch_wheel_open(&self) -> bool {
        self.crew.wheel.as_ref().is_some_and(|w| w.open_for >= WHEEL_DELAY)
    }

    /// L3 + R3: the ability on or off; on, it drains the meter and works on the world, the
    /// weapons and the car.
    pub(crate) fn ability_tick(&mut self, input: &InputFrame, real: f32, events: &mut Vec<SimEvent>) {
        let Ok(d) = def() else { return };
        let Some(a) = d.ability_of(self.crew.current).cloned() else { return };
        let in_car = self.drive.vehicle().is_some();
        let fits = if in_car { a.in_car } else { a.on_foot };
        let chord =
            input.pad.held & pad::L3 != 0 && input.pad.held & pad::R3 != 0 && input.pad.pressed & (pad::L3 | pad::R3) != 0;
        let meter = self.crew.me().map_or(0.0, |m| m.meter);
        if chord {
            if self.crew.ability {
                self.crew.ability = false;
                events.push(SimEvent::Sound { cue: "ability.off".into(), pos: None, gain: 1.0, pitch: 1.0 });
            } else if fits && meter > 0.05 && self.fate.alive() {
                self.crew.ability = true;
                self.crew.activations += 1;
                events.push(SimEvent::Sound { cue: "ability.on".into(), pos: None, gain: 1.0, pitch: 1.0 });
            }
        }
        if self.crew.ability {
            let i = self.crew.current;
            if let Some(m) = self.crew.members.get_mut(i) {
                m.meter = (m.meter - real / a.seconds.max(0.1)).max(0.0);
            }
            if self.crew.me().is_none_or(|m| m.meter <= 0.0) || !fits || !self.fate.alive() {
                self.crew.ability = false;
                events.push(SimEvent::Sound { cue: "ability.off".into(), pos: None, gain: 1.0, pitch: 1.0 });
            }
        }
        let on = self.crew.ability;
        self.combat.power = if on { a.damage } else { 1.0 };
        self.combat.tough = if on { a.taken } else { 1.0 };
        self.combat.assist_cone = if on { a.assist } else { 1.0 };
        self.combat.shove = if on { a.knockback } else { 1.0 };
        self.drive.grip = if on { a.grip } else { 0.0 };
    }

    /// The world's speed the ability asks for (1: none).
    pub fn ability_time_scale(&self) -> f32 {
        if !self.crew.ability {
            return 1.0;
        }
        def().ok().and_then(|d| d.ability_of(self.crew.current).map(|a| a.time_scale)).unwrap_or(1.0)
    }

    /// The screen wash while the ability is on.
    pub fn ability_tint(&self) -> Option<[f32; 4]> {
        if !self.crew.ability {
            return None;
        }
        def().ok().and_then(|d| d.ability_of(self.crew.current).map(|a| a.tint))
    }

    /// The meter fills by how the protagonist plays this tick: at the wheel near misses, drifts
    /// and air; on foot head shots, kills and takedowns; and damage dealt, taken and explosions.
    pub(crate) fn fill_meter(&mut self, sim: &Sim, res: &crate::combat::Outcome, events: &[SimEvent], real: f32) {
        let taken = std::mem::take(&mut self.combat.taken);
        self.crew.fill("taken", taken);
        self.crew.fill("dealt", res.dealt);
        self.crew.fill("headshot", res.headshots as f32);
        self.crew.fill("kill", res.killed.len() as f32);
        if self.cover.at.is_some() {
            self.crew.fill("takedown", res.killed.len() as f32);
        }
        let me = sim.state.player.and_then(|id| sim.feet(id));
        for ev in events {
            if let (SimEvent::Explosion { pos, .. }, Some(f)) = (ev, me)
                && pos.distance(f) < 20.0
            {
                self.crew.fill("explosion", 1.0);
            }
        }
        for n in &mut self.crew.near {
            n.1 -= real;
        }
        self.crew.near.retain(|n| n.1 > 0.0);
        let Some(car) = self.drive.vehicle() else { return };
        let (Some(c), Some(st)) = (crate::drive::Car::read(sim, car), sim.vehicle_status(car)) else { return };
        if st.airborne {
            self.crew.fill("air", real);
        }
        if st.drift > 2.0 && c.speed() > 8.0 {
            self.crew.fill("drift", real);
        }
        // A near miss: another vehicle passing within an arm's length at speed.
        let close: Vec<EntityId> = sim
            .state
            .entities
            .iter()
            .filter(|e| e.id != car && e.vehicle.is_some() && e.pos.distance(c.center) < 6.0)
            .filter(|e| !self.crew.near.iter().any(|n| n.0 == e.id))
            .filter_map(|e| {
                let o = crate::drive::Car::read(sim, e.id)?;
                let side = (o.center - c.center).dot(c.left()).abs() - o.half.x - c.half.x;
                let rel = (o.vel - c.vel).length();
                (side < 1.0 && side > -0.2 && rel > 12.0).then_some(e.id)
            })
            .collect();
        for id in close {
            self.crew.near.push((id, 3.0));
            self.crew.fill("near_miss", 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_has_thousands_separators() {
        assert_eq!(money_text(12_480), "$12,480");
        assert_eq!(money_text(-1_000_000), "-$1,000,000");
        assert_eq!(money_text(999), "$999");
    }

    #[test]
    fn the_profiles_read_and_start_as_the_plan_says() {
        let d = def().expect("protagonists.toml");
        let keys: Vec<&str> = d.protagonist.iter().map(|p| p.key.as_str()).collect();
        assert_eq!(keys, ["dez", "mack", "roxy"]);
        let cash: Vec<i64> = d.protagonist.iter().map(|p| p.cash).collect();
        assert_eq!(cash, [350, 2500, 120], "PLAN §5.6.10's starting cash");
        let crew = Crew::new(0);
        assert_eq!(crew.members.len(), 3);
        assert!(crew.owns(0), "fists are always owned");
        assert_eq!(crew.key(), "dez");
    }

    #[test]
    fn the_meter_fills_only_by_the_protagonists_own_play_style() {
        let mut crew = Crew::new(0);
        crew.members[0].meter = 0.0;
        crew.fill("headshot", 3.0);
        assert_eq!(crew.members[0].meter, 0.0, "Dez's meter ignores head shots");
        crew.fill("drift", 1.0);
        assert!(crew.members[0].meter > 0.0, "Dez's meter fills on drifts");
        crew.ability = true;
        let before = crew.members[0].meter;
        crew.fill("drift", 1.0);
        assert_eq!(crew.members[0].meter, before, "no filling while the ability is on");
    }
}
