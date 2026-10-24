//! The frontend (PLAN.md §5.9): boot splash → title → main menu → free roam ⇄ pause, with the
//! settings, About and the UI gallery. A small state machine over the controller and the
//! pointer's commands (`ui.focus <id>`, `ui.accept <id>`); it builds each page's `Screen`.
//! Every list wraps, exactly one element has focus, ✕ accepts, ○ backs out, L1/R1 switch tabs.

use glam::Vec2;
use pav_core::InputFrame;
use pav_core::input::pad;
use pav_core::present::*;
use serde::{Deserialize, Serialize};

use crate::pad::{Nav, move_focus, pressed};
use crate::settings::{PAGES as SETTINGS_PAGES, Settings};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Page {
    Boot,
    Title,
    Main,
    Settings,
    About,
    Play,
    Pause,
    Gallery,
}

impl Page {
    pub fn name(self) -> &'static str {
        match self {
            Page::Boot => "boot",
            Page::Title => "title",
            Page::Main => "main",
            Page::Settings => "settings",
            Page::About => "about",
            Page::Play => "play",
            Page::Pause => "pause",
            Page::Gallery => "gallery",
        }
    }

    /// Pages where nobody is playing: the idle timer runs (attract mode after 10 s).
    pub fn idles(self) -> bool {
        matches!(self, Page::Title | Page::Main | Page::Settings | Page::About)
    }
}

/// The pause menu's tabs.
pub const PAUSE_TABS: [&str; 5] = ["MAP", "BRIEF", "STATS", "SETTINGS", "GAME"];

/// Seconds without input on an idle page before the attract loop starts (§5.8).
pub const IDLE_SECONDS: f32 = 10.0;

/// What a frontend step asks the game to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Start playing (free roam).
    Play,
    /// Load the newest save and play.
    Continue,
    /// A fresh game.
    NewGame,
    /// The pause menu's replay list, Load Game, Quit Mission.
    Replay,
    Load,
    QuitMission,
    /// The settings changed (keep them).
    Settings,
    Quit,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Front {
    pub page: Page,
    /// Where Settings, About and the gallery go back to.
    pub back: Page,
    /// Real seconds on this page (animations, the boot splash).
    pub since: f32,
    pub main: usize,
    pub settings_tab: usize,
    pub settings: usize,
    pub pause_tab: usize,
    pub pause: usize,
    pub stats: usize,
    pub gallery: usize,
    pub nav: Nav,
    /// Seconds without input on an idle page.
    pub idle: f32,
    pub attract_due: bool,
    /// The pause map: cursor, zoom (metres per screen height), legend, waypoint.
    pub map_cursor: Vec2,
    pub map_zoom: f32,
    pub legend: bool,
    pub waypoint: Option<Vec2>,
    /// When the last page change happened (real seconds; fades key off it).
    pub changed_at: f32,
}

impl Default for Front {
    fn default() -> Self {
        Self {
            page: Page::Boot,
            back: Page::Main,
            since: 0.0,
            main: 0,
            settings_tab: 0,
            settings: 0,
            pause_tab: 4,
            pause: 0,
            stats: 0,
            gallery: 0,
            nav: Nav::default(),
            idle: 0.0,
            attract_due: false,
            map_cursor: Vec2::ZERO,
            map_zoom: 160.0,
            legend: false,
            waypoint: None,
            changed_at: 0.0,
        }
    }
}

/// What the frontend needs to know about the rest of the game.
pub struct Ctx<'a> {
    pub settings: &'a mut Settings,
    pub has_save: bool,
    pub web: bool,
    pub player: Vec2,
    /// Real seconds since the game started (the UI clock).
    pub now: f32,
    /// A mission is under way (the pause menu offers to quit it).
    pub on_mission: bool,
}

fn item(id: &str, label: &str, enabled: bool, note: Option<&str>) -> MenuItem {
    MenuItem { id: id.into(), label: label.into(), value: ItemValue::Button, enabled, note: note.map(String::from) }
}

fn hint(button: PadButton, label: &str) -> Prompt {
    Prompt { button, label: label.into(), hold: None }
}

impl Front {
    pub fn go(&mut self, page: Page, now: f32) {
        if page != self.page {
            if matches!(page, Page::Settings | Page::About | Page::Gallery) {
                self.back = self.page;
            }
            self.page = page;
            self.since = 0.0;
            self.idle = 0.0;
            self.nav = Nav::default();
            self.changed_at = now;
        }
    }

    pub fn main_items(&self, has_save: bool, web: bool) -> Vec<MenuItem> {
        let mut items = vec![
            item("continue", "Continue", has_save, (!has_save).then_some("No saved game yet")),
            item("new_game", "New Game", true, None),
            item("free_roam", "Free Roam", true, None),
            item("settings", "Settings", true, None),
            item("about", "About", true, None),
            item("gallery", "UI Gallery", true, Some("Every piece of the interface")),
        ];
        if !web {
            items.push(item("quit", "Quit", true, None));
        }
        items
    }

    fn game_items(on_mission: bool) -> Vec<MenuItem> {
        let mut v = vec![item("resume", "Resume", true, None)];
        if on_mission {
            v.push(item("quit_mission", "Quit Mission", true, None));
        }
        v.extend([
            item("replay", "Replay Mission", !on_mission, Some("Missions you have passed")),
            item("load", "Load Game", !on_mission, None),
            item("gallery", "UI Gallery", true, None),
            item("title", "Quit to Title", true, None),
        ]);
        v
    }

    fn settings_cards() -> Vec<Card> {
        let blurb = [
            "Volume of radio, effects and voices",
            "Frame rate readout and HUD size",
            "Camera, prompts and vibration",
            "Subtitles, shake and contrast",
        ];
        SETTINGS_PAGES
            .iter()
            .zip(blurb)
            .map(|(name, line)| Card {
                id: name.to_ascii_lowercase(),
                title: (*name).into(),
                subtitle: None,
                lines: vec![line.into()],
                bars: Vec::new(),
                accent: pav_core::Color::hex("#ff7a3d"),
                icon: Some("settings".into()),
                enabled: true,
            })
            .collect()
    }

    /// One tick of the frontend from the controller and the host's commands. `dt` is real time.
    /// The menu sounds it makes go to `sounds` (bank cues).
    pub fn step(&mut self, input: &InputFrame, ctx: &mut Ctx, dt: f32, sounds: &mut Vec<&'static str>) -> Option<Action> {
        let before =
            (self.page, self.main, self.settings, self.settings_tab, self.pause_tab, self.pause, self.stats, self.gallery);
        let action = self.update(input, ctx, dt);
        let after =
            (self.page, self.main, self.settings, self.settings_tab, self.pause_tab, self.pause, self.stats, self.gallery);
        if after.0 != before.0 {
            let back = pressed(input, pad::EAST) || (before.0 == Page::Pause && after.0 == Page::Play);
            sounds.push(if back { "ui.back" } else { "ui.accept" });
        } else if (after.3, after.4) != (before.3, before.4) {
            sounds.push("ui.tab");
        } else if after != before || action == Some(Action::Settings) {
            sounds.push("ui.move");
        } else if self.page == Page::Pause && self.pause_tab == 0 && pressed(input, pad::SOUTH) {
            sounds.push("ui.accept");
        }
        action
    }

    fn update(&mut self, input: &InputFrame, ctx: &mut Ctx, dt: f32) -> Option<Action> {
        self.since += dt;
        if self.page.idles() {
            if input.activity {
                self.idle = 0.0;
            } else {
                self.idle += dt;
            }
            if self.idle >= IDLE_SECONDS && !self.attract_due {
                self.attract_due = true;
                log::info!("attract would start: {IDLE_SECONDS} s without input on the {} page", self.page.name());
            }
        }
        // The pointer: focus what it is over, accept what it clicks.
        let mut accept_id = None;
        for c in &input.cmds {
            if let Some(id) = c.strip_prefix("ui.focus ") {
                self.focus_id(id, ctx);
            } else if let Some(id) = c.strip_prefix("ui.accept ") {
                self.focus_id(id, ctx);
                accept_id = Some(id.to_string());
            }
        }
        let (dx, dy) = self.nav.step(input, dt);
        let accept = pressed(input, pad::SOUTH) || accept_id.is_some();
        let back = pressed(input, pad::EAST);
        let now = ctx.now;
        match self.page {
            Page::Boot => {
                if self.since > 2.6 || crate::pad::any_pressed(input) || accept_id.is_some() {
                    self.go(Page::Title, now);
                }
            }
            Page::Title => {
                if self.since > 0.3 && (crate::pad::any_pressed(input) || accept_id.is_some()) {
                    self.go(Page::Main, now);
                }
            }
            Page::Main => {
                let items = self.main_items(ctx.has_save, ctx.web);
                self.main = move_focus(self.main, items.len(), dy, |i| items[i].enabled);
                if back {
                    self.go(Page::Title, now);
                } else if accept && items[self.main].enabled {
                    match items[self.main].id.as_str() {
                        "continue" => {
                            self.go(Page::Play, now);
                            return Some(Action::Continue);
                        }
                        "new_game" => {
                            self.go(Page::Play, now);
                            return Some(Action::NewGame);
                        }
                        "free_roam" => {
                            self.go(Page::Play, now);
                            return Some(Action::Play);
                        }
                        "settings" => self.go(Page::Settings, now),
                        "about" => self.go(Page::About, now),
                        "gallery" => self.go(Page::Gallery, now),
                        "quit" => return Some(Action::Quit),
                        _ => {}
                    }
                }
            }
            Page::Settings => {
                if pressed(input, pad::L1) || pressed(input, pad::R1) {
                    let d = if pressed(input, pad::R1) { 1 } else { -1 };
                    self.settings_tab = (self.settings_tab as i32 + d).rem_euclid(SETTINGS_PAGES.len() as i32) as usize;
                    self.settings = 0;
                }
                let items = ctx.settings.items(self.settings_tab);
                self.settings = move_focus(self.settings, items.len(), dy, |_| true);
                let id = items[self.settings.min(items.len() - 1)].id.clone();
                let change = if dx != 0 {
                    ctx.settings.adjust(&id, dx)
                } else if accept {
                    ctx.settings.adjust(&id, 0)
                } else {
                    false
                };
                if back {
                    let to = self.back;
                    self.go(to, now);
                }
                if change {
                    return Some(Action::Settings);
                }
            }
            Page::About => {
                if back || accept {
                    let to = self.back;
                    self.go(to, now);
                }
            }
            Page::Gallery => {
                let n = crate::gallery::PAGES.len() as i32;
                let d = (pressed(input, pad::R1) as i32 - pressed(input, pad::L1) as i32) + dx;
                self.gallery = (self.gallery as i32 + d).rem_euclid(n) as usize;
                if back {
                    let to = self.back;
                    self.go(to, now);
                }
            }
            Page::Play => {
                if pressed(input, pad::START) {
                    self.go(Page::Pause, now);
                    self.map_cursor = ctx.player;
                } else if pressed(input, pad::TOUCHPAD) || pressed(input, pad::SELECT) {
                    self.go(Page::Pause, now);
                    self.pause_tab = 0;
                    self.map_cursor = ctx.player;
                }
            }
            Page::Pause => {
                if pressed(input, pad::L1) || pressed(input, pad::R1) {
                    let d = if pressed(input, pad::R1) { 1 } else { -1 };
                    self.pause_tab = (self.pause_tab as i32 + d).rem_euclid(PAUSE_TABS.len() as i32) as usize;
                    self.pause = 0;
                }
                if pressed(input, pad::START) || (back && self.pause_tab != 0) {
                    self.go(Page::Play, now);
                    return None;
                }
                match self.pause_tab {
                    0 => {
                        // The map: the stick pans the cursor, L2/R2 zoom, ✕ sets a waypoint.
                        let s = input.pad.stick_l + Vec2::new(dx as f32, -dy as f32);
                        self.map_cursor += Vec2::new(s.x, -s.y) * self.map_zoom * 0.6 * dt;
                        let zoom = input.pad.r2 - input.pad.l2;
                        self.map_zoom = (self.map_zoom * (1.0 - zoom * 1.5 * dt)).clamp(40.0, 600.0);
                        if accept {
                            self.waypoint = if self.waypoint.is_some_and(|w| w.distance(self.map_cursor) < 4.0) {
                                None
                            } else {
                                Some(self.map_cursor)
                            };
                        }
                        if pressed(input, pad::WEST) {
                            self.legend = !self.legend;
                        }
                        if back {
                            self.go(Page::Play, now);
                        }
                    }
                    // (The protagonists, then completion and the activities.)
                    2 => self.stats = move_focus(self.stats, 5, dx, |_| true),
                    3 => {
                        self.pause = move_focus(self.pause, SETTINGS_PAGES.len(), dx + dy, |_| true);
                        if accept {
                            self.settings_tab = self.pause;
                            self.settings = 0;
                            self.go(Page::Settings, now);
                        }
                    }
                    4 => {
                        let items = Self::game_items(ctx.on_mission);
                        self.pause = move_focus(self.pause, items.len(), dy, |i| items[i].enabled);
                        if accept && items[self.pause.min(items.len() - 1)].enabled {
                            match items[self.pause.min(items.len() - 1)].id.as_str() {
                                "resume" => self.go(Page::Play, now),
                                "quit_mission" => {
                                    self.go(Page::Play, now);
                                    return Some(Action::QuitMission);
                                }
                                "replay" => {
                                    self.go(Page::Play, now);
                                    return Some(Action::Replay);
                                }
                                "load" => {
                                    self.go(Page::Play, now);
                                    return Some(Action::Load);
                                }
                                "gallery" => self.go(Page::Gallery, now),
                                "title" => self.go(Page::Title, now),
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        None
    }

    /// Focuses the element `id` on this page (the pointer is over it); `tab:<n>` switches tabs.
    fn focus_id(&mut self, id: &str, ctx: &Ctx) {
        if let Some(n) = id.strip_prefix("tab:").and_then(|n| n.parse::<usize>().ok()) {
            match self.page {
                Page::Pause if n < PAUSE_TABS.len() => {
                    self.pause_tab = n;
                    self.pause = 0;
                }
                Page::Settings if n < SETTINGS_PAGES.len() => {
                    self.settings_tab = n;
                    self.settings = 0;
                }
                _ => {}
            }
            return;
        }
        let find = |items: &[MenuItem]| items.iter().position(|i| i.id == id && i.enabled);
        match self.page {
            Page::Main => {
                if let Some(i) = find(&self.main_items(ctx.has_save, ctx.web)) {
                    self.main = i;
                }
            }
            Page::Settings => {
                if let Some(i) = find(&ctx.settings.items(self.settings_tab)) {
                    self.settings = i;
                }
            }
            Page::Pause => match self.pause_tab {
                3 => {
                    if let Some(i) = Self::settings_cards().iter().position(|c| c.id == id) {
                        self.pause = i;
                    }
                }
                4 => {
                    if let Some(i) = find(&Self::game_items(ctx.on_mission)) {
                        self.pause = i;
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    /// The page's full-screen `Screen` (None while playing). `stats` is the stats page's cards,
    /// `brief` the BRIEF tab (the mission under way; none: free roam).
    #[allow(clippy::too_many_arguments)]
    pub fn screen(
        &self,
        settings: &Settings,
        has_save: bool,
        web: bool,
        player: Vec2,
        blips: &[Blip],
        route: &[Vec2],
        stats: CardRow,
        brief: Option<TextCard>,
        on_mission: bool,
    ) -> Option<Screen> {
        let s = |id: &str, title: &str, body: ScreenBody, backdrop: Backdrop, hints: Vec<Prompt>| Screen {
            id: id.into(),
            title: title.into(),
            tabs: Vec::new(),
            tab: 0,
            hints,
            body,
            backdrop,
        };
        let select_back = || vec![hint(PadButton::South, "Select"), hint(PadButton::East, "Back")];
        Some(match self.page {
            Page::Play | Page::Gallery => return None,
            Page::Boot => {
                let (title, caption) =
                    if self.since < 1.3 { ("PAVILION", "ENGINE") } else { ("GRAND THEFT FREEWAY", "PALOMA BAY · BX") };
                let card = TitleCard { prompt: None, menu: None, caption: Some(caption.into()), small_logo: false };
                s("boot", title, ScreenBody::Title(card), Backdrop::Solid, Vec::new())
            }
            Page::Title => {
                let card = TitleCard {
                    prompt: Some("PRESS ANY BUTTON".into()),
                    menu: None,
                    caption: Some("PALOMA BAY · BX".into()),
                    small_logo: false,
                };
                s("title", "GRAND THEFT FREEWAY", ScreenBody::Title(card), Backdrop::Clear, Vec::new())
            }
            Page::Main => {
                let menu = MenuList { items: self.main_items(has_save, web), focus: self.main };
                let card =
                    TitleCard { prompt: None, menu: Some(menu), caption: Some("PALOMA BAY · BX".into()), small_logo: true };
                s("main", "GRAND THEFT FREEWAY", ScreenBody::Title(card), Backdrop::Clear, select_back())
            }
            Page::Settings => {
                let menu = MenuList { items: settings.items(self.settings_tab), focus: self.settings };
                let mut screen = s(
                    &format!("settings/{}", SETTINGS_PAGES[self.settings_tab].to_ascii_lowercase()),
                    "SETTINGS",
                    ScreenBody::Menu(menu),
                    Backdrop::Dim,
                    vec![hint(PadButton::Left, "Adjust"), hint(PadButton::South, "Change"), hint(PadButton::East, "Back")],
                );
                screen.tabs = SETTINGS_PAGES.iter().map(|t| t.to_string()).collect();
                screen.tab = self.settings_tab;
                screen
            }
            Page::About => {
                let text = TextCard { heading: "ABOUT".into(), lines: crate::about::lines() };
                s("about", "ABOUT", ScreenBody::Text(text), Backdrop::Dim, vec![hint(PadButton::East, "Back")])
            }
            Page::Pause => {
                let (body, backdrop, hints) = match self.pause_tab {
                    0 => (
                        ScreenBody::Map(MapView {
                            center: self.map_cursor,
                            zoom: self.map_zoom,
                            cursor: self.map_cursor,
                            legend: self.legend,
                            blips: blips
                                .iter()
                                .cloned()
                                .chain(self.waypoint.map(|w| Blip {
                                    pos: w,
                                    icon: BlipIcon::Waypoint,
                                    color: pav_core::Color::hex("#ff3e8a"),
                                    label: Some("Waypoint".into()),
                                    clamp: true,
                                    height: 0,
                                    pulse: false,
                                }))
                                .collect(),
                            route: if route.len() >= 2 {
                                route.to_vec()
                            } else {
                                self.waypoint.map(|w| vec![player, w]).unwrap_or_default()
                            },
                            selected: blips
                                .iter()
                                .find(|b| b.pos.distance(self.map_cursor) < self.map_zoom * 0.04)
                                .and_then(|b| b.label.clone()),
                        }),
                        Backdrop::Solid,
                        vec![
                            hint(PadButton::South, "Waypoint"),
                            hint(PadButton::West, "Legend"),
                            hint(PadButton::R2, "Zoom"),
                            hint(PadButton::East, "Back"),
                        ],
                    ),
                    1 => (
                        ScreenBody::Text(brief.unwrap_or_else(|| TextCard {
                            heading: "FREE ROAM".into(),
                            lines: vec![
                                "Paloma Bay is yours to explore: downtown, the docks, the hills.".into(),
                                "Hold ~btn:l1~ to open the weapon wheel, ~btn:down~ to switch, ~btn:up~ for the phone.".into(),
                                "~btn:start~ pauses; ~btn:l1~ ~btn:r1~ switch tabs.".into(),
                            ],
                        })),
                        Backdrop::Dim,
                        vec![hint(PadButton::East, "Resume")],
                    ),
                    2 => (ScreenBody::Cards(stats), Backdrop::Dim, vec![hint(PadButton::East, "Resume")]),
                    3 => (
                        ScreenBody::Cards(CardRow { cards: Self::settings_cards(), focus: self.pause }),
                        Backdrop::Dim,
                        vec![hint(PadButton::South, "Open"), hint(PadButton::East, "Resume")],
                    ),
                    _ => (
                        ScreenBody::Menu(MenuList { items: Self::game_items(on_mission), focus: self.pause }),
                        Backdrop::Dim,
                        vec![hint(PadButton::South, "Select"), hint(PadButton::East, "Resume")],
                    ),
                };
                let mut screen =
                    s(&format!("pause/{}", PAUSE_TABS[self.pause_tab].to_ascii_lowercase()), "PAUSED", body, backdrop, hints);
                screen.tabs = PAUSE_TABS.iter().map(|t| t.to_string()).collect();
                screen.tab = self.pause_tab;
                screen
            }
        })
    }

    /// The fade over a page change: black at the change, clear after `secs`.
    pub fn fade(&self, now: f32, secs: f32) -> f32 {
        (1.0 - (now - self.changed_at) / secs).clamp(0.0, 1.0)
    }

    /// Fades the picture in from black again (a respawn).
    pub fn fade_in(&mut self, now: f32) {
        self.changed_at = now;
    }
}
