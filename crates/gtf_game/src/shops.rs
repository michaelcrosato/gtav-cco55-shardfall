//! Shops and the safehouse (PLAN.md §5.6.10, §5.6.14) as 10-foot menus: Bang Bang Bros. sells
//! guns, ammunition and armour; Paloma Customs resprays and repairs the car the player drives
//! up in; Gas-N-Grab's snacks heal. At the protagonist's own safehouse the same D-pad right
//! saves the game. The world holds still while a menu is up.

use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use glam::Vec3;
use pav_core::InputFrame;
use pav_core::frame::SimEvent;
use pav_core::input::pad;
use pav_core::present::{Backdrop, ItemValue, MenuItem, MenuList, PadButton, Prompt, Screen, ScreenBody};
use pav_core::sim::Sim;
use serde::{Deserialize, Serialize};

use crate::Freeway;
use crate::pad::Nav;
use crate::protagonists::money_text;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GunItem {
    pub weapon: String,
    pub price: i64,
    pub ammo: u32,
    pub ammo_price: i64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GunShop {
    pub landmark: String,
    /// Where its counter is (an anchor; none: the landmark's door).
    #[serde(default)]
    pub at: Option<String>,
    pub name: String,
    pub armor: i64,
    pub item: Vec<GunItem>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModShop {
    pub landmark: String,
    #[serde(default)]
    pub at: Option<String>,
    pub name: String,
    pub respray: i64,
    pub repair: [i64; 2],
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snack {
    pub name: String,
    pub price: i64,
    pub heal: f32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Store {
    pub landmark: String,
    #[serde(default)]
    pub at: Option<String>,
    pub name: String,
    pub item: Vec<Snack>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShopsDef {
    pub gun: GunShop,
    pub mods: ModShop,
    pub store: Store,
}

/// The shops (cached; re-read when the file changes during native hot reload).
pub fn def() -> Result<Arc<ShopsDef>> {
    static CACHE: Mutex<Option<(String, Arc<ShopsDef>)>> = Mutex::new(None);
    let text = crate::content::text("shops.toml", include_str!("../../../content/freeway/shops.toml"));
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((t, d)) = cache.as_ref()
        && *t == *text
    {
        return Ok(d.clone());
    }
    let d: ShopsDef = toml::from_str(&text).map_err(|e| anyhow!("content/freeway/shops.toml: {e}"))?;
    let d = Arc::new(d);
    *cache = Some((text.into_owned(), d.clone()));
    Ok(d)
}

/// Which counter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Counter {
    Gun,
    Mods,
    Store,
    /// The safehouse bed: saving.
    Save,
    /// The pause menu's Load Game.
    Load,
    /// The pause menu's Replay Mission: missions passed.
    Replay,
}

/// A menu that is up (part of the game's state).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShopState {
    pub counter: Counter,
    pub focus: usize,
    pub nav: Nav,
    /// Opened this tick from another menu (the press that opened it is not for this one).
    #[serde(default)]
    pub fresh: bool,
}

impl ShopState {
    pub fn new(counter: Counter, fresh: bool) -> ShopState {
        ShopState { counter, focus: 0, nav: Nav::default(), fresh }
    }
}

/// How near a counter the player must stand on foot (m), and Paloma Customs' bay in a car.
const REACH: f32 = 3.0;
const CAR_REACH: f32 = 11.0;

/// Where a shop's counter is: its anchor (`at`) when the district has it, else the landmark's
/// door.
fn counter_point(city: &pav_world::City, landmark: &str, at: Option<&str>) -> Option<Vec3> {
    at.and_then(|a| crate::anchor::Anchor::parse(a).ok())
        .filter(|a| a.exists(city))
        .and_then(|a| crate::anchor::foot(city, &a))
        .map(|(p, _)| p)
        .or_else(|| crate::world::door(city, landmark))
}

/// Where a protagonist saves: their bed (`bed`, an anchor) when the district has it, else home.
pub fn save_point(city: &pav_world::City, p: &crate::protagonists::ProtagonistDef) -> Option<Vec3> {
    [p.bed.as_deref(), Some(p.home.as_str())]
        .into_iter()
        .flatten()
        .filter_map(|a| crate::anchor::Anchor::parse(a).ok())
        .find(|a| a.exists(city))
        .and_then(|a| crate::anchor::foot(city, &a))
        .map(|(p, _)| p)
}
/// The save slots.
pub const SLOTS: [&str; 3] = ["slot1", "slot2", "slot3"];

/// A menu line: its id, what it says, its price (or state) beside it, whether it can be had.
struct Line {
    id: String,
    label: String,
    note: Option<String>,
    enabled: bool,
}

impl Freeway {
    /// The counter the player stands at (or drives up to), if any.
    pub(crate) fn counter_here(&mut self, sim: &Sim) -> Option<Counter> {
        let city = self.city.clone()?;
        let d = def().ok()?;
        let feet = Self::feet(sim);
        if self.doors.is_none() {
            self.doors = Some(vec![
                (Counter::Gun, counter_point(&city, &d.gun.landmark, d.gun.at.as_deref())),
                (Counter::Mods, counter_point(&city, &d.mods.landmark, d.mods.at.as_deref())),
                (Counter::Store, counter_point(&city, &d.store.landmark, d.store.at.as_deref())),
            ]);
        }
        let in_car = self.drive.vehicle().is_some();
        for (c, at) in self.doors.clone().unwrap_or_default() {
            let Some(at) = at else { continue };
            let near = match c {
                Counter::Mods => in_car && self.player_point(sim).distance(at) < CAR_REACH,
                _ => !in_car && feet.distance(at) < REACH,
            };
            if near {
                return Some(c);
            }
        }
        // The protagonist's own safehouse: the bed.
        let def = crate::protagonists::def().ok()?;
        let at = save_point(&city, def.protagonist.get(self.crew.current)?)?;
        (!in_car && feet.distance(at) < REACH).then_some(Counter::Save)
    }

    /// One tick of the counters: the prompt at a door, the menu while it is up.
    pub(crate) fn shops_tick(&mut self, sim: &mut Sim, input: &InputFrame, real: f32, events: &mut Vec<SimEvent>) {
        let pressed = |b: u32| input.pad.pressed & b != 0;
        let Some(mut shop) = self.shop.take() else {
            // In a mission only Paloma Customs serves (a respray loses the police: a getaway's
            // trick), and not in its cutscenes.
            let mission = self.missions.run.as_ref();
            let respray_only = mission.is_some();
            let busy_mission = mission.is_some_and(|r| {
                r.phase != crate::missions::Phase::Play
                    || crate::missions::get(&r.key)
                        .ok()
                        .and_then(|d| {
                            d.step.get(r.step).map(|s| {
                                matches!(s.kind, crate::missions::StepKind::Cutscene | crate::missions::StepKind::Choose)
                            })
                        })
                        .unwrap_or(true)
            });
            if busy_mission || !self.fate.alive() || self.phone.open || self.drive.busy() {
                self.counter = None;
                return;
            }
            self.counter = self.counter_here(sim).filter(|c| !respray_only || *c == Counter::Mods);
            if let Some(c) = self.counter
                && pressed(pad::RIGHT)
            {
                if c == Counter::Save && self.heat.stars > 0 {
                    self.note("heat", "Not now", "You can't save while the police are after you.");
                } else {
                    self.shop = Some(ShopState::new(c, false));
                    events.push(SimEvent::Sound { cue: "ui.accept".into(), pos: None, gain: 1.0, pitch: 1.0 });
                }
            }
            return;
        };
        if shop.fresh {
            shop.fresh = false;
            self.shop = Some(shop);
            return;
        }
        let lines = self.counter_lines(sim, shop.counter);
        let (_, dy) = shop.nav.step(input, real);
        let before = shop.focus;
        shop.focus = crate::pad::move_focus(shop.focus, lines.len(), dy, |_| true);
        if shop.focus != before {
            events.push(SimEvent::Sound { cue: "ui.move".into(), pos: None, gain: 1.0, pitch: 1.0 });
        }
        for c in &input.cmds {
            if let Some(id) = c.strip_prefix("ui.focus ").or(c.strip_prefix("ui.accept "))
                && let Some(i) = lines.iter().position(|l| l.id == id)
            {
                shop.focus = i;
            }
        }
        let accept = pressed(pad::SOUTH) || input.cmds.iter().any(|c| c.starts_with("ui.accept "));
        if pressed(pad::EAST) {
            events.push(SimEvent::Sound { cue: "ui.back".into(), pos: None, gain: 1.0, pitch: 1.0 });
            return;
        }
        if accept && let Some(line) = lines.get(shop.focus) {
            if !line.enabled {
                events.push(SimEvent::Sound { cue: "ui.error".into(), pos: None, gain: 1.0, pitch: 1.0 });
            } else {
                let close = self.buy(sim, shop.counter, &line.id, events);
                if close {
                    return;
                }
            }
        }
        self.shop = Some(shop);
    }

    /// What a counter offers now.
    fn counter_lines(&self, sim: &Sim, c: Counter) -> Vec<Line> {
        let Ok(d) = def() else { return Vec::new() };
        let money = self.money;
        let price = |p: i64| format!("{} · you have {}", money_text(p), money_text(money));
        match c {
            Counter::Gun => {
                let wd = crate::combat::def().ok();
                let mut v: Vec<Line> = d
                    .gun
                    .item
                    .iter()
                    .filter_map(|it| {
                        let wd = wd.as_ref()?;
                        let i = wd.weapon.iter().position(|w| w.id == it.weapon)?;
                        let name = wd.weapon[i].name.clone();
                        Some(if self.crew.owns(i) {
                            Line {
                                id: format!("ammo:{}", it.weapon),
                                label: format!("{name} rounds ×{}", it.ammo),
                                note: Some(price(it.ammo_price)),
                                enabled: money >= it.ammo_price,
                            }
                        } else {
                            Line {
                                id: format!("gun:{}", it.weapon),
                                label: name,
                                note: Some(price(it.price)),
                                enabled: money >= it.price,
                            }
                        })
                    })
                    .collect();
                v.push(Line {
                    id: "armor".into(),
                    label: "Body armour".into(),
                    note: Some(price(d.gun.armor)),
                    enabled: money >= d.gun.armor && self.combat.armor < crate::combat::MAX_ARMOR,
                });
                v
            }
            Counter::Mods => {
                let health = self.drive.vehicle().and_then(|c| sim.vehicle_status(c)).map_or(1.0, |s| s.health);
                let repair = self.repair_price(health);
                vec![
                    Line {
                        id: "respray".into(),
                        label: "Respray".into(),
                        note: Some(price(d.mods.respray)),
                        enabled: money >= d.mods.respray,
                    },
                    Line {
                        id: "repair".into(),
                        label: "Repair".into(),
                        note: Some(if health >= 0.999 { "Like new".into() } else { price(repair) }),
                        enabled: health < 0.999 && money >= repair,
                    },
                ]
            }
            Counter::Store => d
                .store
                .item
                .iter()
                .enumerate()
                .map(|(i, s)| Line {
                    id: format!("snack:{i}"),
                    label: s.name.clone(),
                    note: Some(price(s.price)),
                    enabled: money >= s.price,
                })
                .collect(),
            Counter::Save | Counter::Load => SLOTS
                .iter()
                .enumerate()
                .map(|(i, slot)| {
                    let info = self.saves.get(*slot).and_then(|j| crate::progress::summary(j));
                    Line {
                        id: (*slot).into(),
                        label: format!("Slot {}", i + 1),
                        note: Some(info.clone().unwrap_or_else(|| "Empty".into())),
                        enabled: c == Counter::Save || info.is_some(),
                    }
                })
                .chain((c == Counter::Load).then(|| {
                    let info = self.saves.get("auto").and_then(|j| crate::progress::summary(j));
                    Line {
                        id: "auto".into(),
                        label: "Autosave".into(),
                        enabled: info.is_some(),
                        note: Some(info.unwrap_or_else(|| "Empty".into())),
                    }
                }))
                .collect(),
            Counter::Replay => {
                let v: Vec<Line> = crate::missions::all()
                    .iter()
                    .filter(|d| self.missions.passed(&d.key))
                    .map(|d| {
                        let medal = self.missions.records.get(&d.key).and_then(|r| r.medal).map(|m| match m {
                            pav_core::present::Medal::Gold => "Gold",
                            pav_core::present::Medal::Silver => "Silver",
                            pav_core::present::Medal::Bronze => "Bronze",
                        });
                        Line { id: d.key.clone(), label: d.title.clone(), note: medal.map(String::from), enabled: true }
                    })
                    .collect();
                if v.is_empty() {
                    vec![Line { id: "none".into(), label: "No missions passed yet".into(), note: None, enabled: false }]
                } else {
                    v
                }
            }
        }
    }

    fn repair_price(&self, health: f32) -> i64 {
        let Ok(d) = def() else { return 0 };
        let [lo, hi] = d.mods.repair;
        lo + ((hi - lo) as f32 * (1.0 - health).clamp(0.0, 1.0)).round() as i64
    }

    /// Buys `id` at counter `c`. Returns true when the menu closes.
    fn buy(&mut self, sim: &mut Sim, c: Counter, id: &str, events: &mut Vec<SimEvent>) -> bool {
        let Ok(d) = def() else { return true };
        let cash = |me: &mut Freeway, p: i64, events: &mut Vec<SimEvent>| -> bool {
            if me.money < p {
                events.push(SimEvent::Sound { cue: "ui.error".into(), pos: None, gain: 1.0, pitch: 1.0 });
                return false;
            }
            me.pay(-p);
            events.push(SimEvent::Sound { cue: "ui.cash".into(), pos: None, gain: 1.0, pitch: 1.0 });
            true
        };
        match c {
            Counter::Gun => {
                if id == "armor" {
                    if cash(self, d.gun.armor, events) {
                        self.combat.armor = crate::combat::MAX_ARMOR;
                    }
                } else if let Some(w) = id.strip_prefix("gun:") {
                    let it = d.gun.item.iter().find(|i| i.weapon == w);
                    if let Some(it) = it
                        && cash(self, it.price, events)
                    {
                        self.give_weapon(w, it.ammo);
                        self.shop_bought += 1;
                    }
                } else if let Some(w) = id.strip_prefix("ammo:") {
                    let it = d.gun.item.iter().find(|i| i.weapon == w);
                    if let (Some(it), Ok(wd)) = (it, crate::combat::def())
                        && let Some(i) = wd.weapon.iter().position(|x| x.id == w)
                        && cash(self, it.ammo_price, events)
                    {
                        self.combat.arm(&wd);
                        self.combat.reserve[i] += it.ammo;
                        self.shop_bought += 1;
                    }
                }
                false
            }
            Counter::Mods => {
                let Some(car) = self.drive.vehicle() else { return true };
                match id {
                    "respray" => {
                        if cash(self, d.mods.respray, events) {
                            crate::vehicles::repaint(sim, car);
                            self.resprays += 1;
                            // The police were looking for another colour.
                            if self.heat.stars > 0 {
                                self.heat.clear();
                                self.police.stand_down();
                            }
                            self.note("garage", &d.mods.name, "New paint, no questions.");
                        }
                    }
                    "repair" => {
                        let health = sim.vehicle_status(car).map_or(1.0, |s| s.health);
                        if cash(self, self.repair_price(health), events) {
                            sim.repair_vehicle(car);
                        }
                    }
                    _ => {}
                }
                false
            }
            Counter::Store => {
                if let Some(s) = id.strip_prefix("snack:").and_then(|i| i.parse::<usize>().ok()).and_then(|i| d.store.item.get(i))
                    && cash(self, s.price, events)
                {
                    self.combat.health = (self.combat.health + s.heal).min(crate::combat::MAX_HEALTH);
                    self.shop_bought += 1;
                }
                false
            }
            Counter::Save => {
                self.save_game(sim, id);
                self.note("save", "Game saved", &format!("Slot {}", SLOTS.iter().position(|s| *s == id).map_or(0, |i| i + 1)));
                true
            }
            Counter::Load => {
                if let Some(json) = self.saves.get(id).cloned() {
                    match self.load_game(sim, &json) {
                        Ok(()) => self.note("save", "Game loaded", ""),
                        Err(e) => log::warn!("load {id}: {e:#}"),
                    }
                }
                true
            }
            Counter::Replay => {
                if let Err(e) = self.start_mission(sim, id, true, true, events) {
                    log::warn!("{e:#}");
                }
                true
            }
        }
    }

    /// The prompt at a door, and the menu while it is up.
    pub(crate) fn shop_ui(&self, sim: &Sim) -> (Option<Screen>, Option<Prompt>) {
        let Ok(d) = def() else { return (None, None) };
        let name = |c: Counter| match c {
            Counter::Gun => d.gun.name.clone(),
            Counter::Mods => d.mods.name.clone(),
            Counter::Store => d.store.name.clone(),
            Counter::Save => "Save game".into(),
            Counter::Load => "Load game".into(),
            Counter::Replay => "Replay mission".into(),
        };
        let Some(shop) = &self.shop else {
            let prompt = self.counter.map(|c| Prompt { button: PadButton::Right, label: name(c), hold: None });
            return (None, prompt);
        };
        let lines = self.counter_lines(sim, shop.counter);
        let items = lines
            .into_iter()
            .map(|l| MenuItem { id: l.id, label: l.label, value: ItemValue::Button, enabled: l.enabled, note: l.note })
            .collect();
        let verb = match shop.counter {
            Counter::Save => "Save",
            Counter::Load => "Load",
            Counter::Replay => "Play",
            _ => "Buy",
        };
        let screen = Screen {
            id: format!(
                "shop/{}",
                match shop.counter {
                    Counter::Gun => "gun",
                    Counter::Mods => "mods",
                    Counter::Store => "store",
                    Counter::Save => "save",
                    Counter::Load => "load",
                    Counter::Replay => "replay",
                }
            ),
            title: name(shop.counter).to_uppercase(),
            tabs: Vec::new(),
            tab: 0,
            hints: vec![
                Prompt { button: PadButton::South, label: verb.into(), hold: None },
                Prompt { button: PadButton::East, label: "Leave".into(), hold: None },
            ],
            body: ScreenBody::Menu(MenuList { items, focus: shop.focus }),
            backdrop: Backdrop::Dim,
        };
        (Some(screen), None)
    }

    /// The door of a counter (tests, bots).
    pub fn counter_door(&self, c: Counter) -> Option<Vec3> {
        let city = self.city.as_ref()?;
        let d = def().ok()?;
        let (key, at) = match c {
            Counter::Gun => (&d.gun.landmark, &d.gun.at),
            Counter::Mods => (&d.mods.landmark, &d.mods.at),
            Counter::Store => (&d.store.landmark, &d.store.at),
            _ => return None,
        };
        counter_point(city, key, at.as_deref())
    }
}
