//! The cards between the menus and play (PLAN.md §5.9): the new game's intro — a card for each
//! protagonist, then the three together — and the loading card on the way into play. Each card
//! is a picture made in the engine: the protagonist posed where play begins, framed close under
//! the painterly or print filter, with a heading and a line under it. The world holds still
//! while they show; ✕ moves on a card, ○ skips the rest, and play begins after the last. The
//! words are `content/freeway/frontend.toml`.

use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use glam::Vec3;
use pav_core::InputFrame;
use pav_core::entity::EntityId;
use pav_core::frame::SimEvent;
use pav_core::input::pad;
use pav_core::present::*;
use pav_core::sim::Sim;
use serde::{Deserialize, Serialize};

use crate::Freeway;
use crate::frontend::Page;

/// The filter over a card's picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Style {
    Paint,
    Halftone,
}

impl Style {
    fn intent(self) -> StylizeIntent {
        match self {
            Style::Paint => StylizeIntent { kind: StylizeKind::Paint, size: 6.0, mix: 1.0, color: 1.0 },
            Style::Halftone => StylizeIntent { kind: StylizeKind::Halftone, size: 5.0, mix: 1.0, color: 0.85 },
        }
    }
}

/// One of the intro's cards.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardDef {
    /// A protagonist's key, or `crew` (the three side by side).
    pub who: String,
    pub heading: String,
    pub line: String,
    /// Their pose (a clip, held still).
    pub clip: String,
    pub style: Style,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoadingDef {
    pub style: Style,
    pub tips: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardsDef {
    pub intro: Vec<CardDef>,
    pub loading: LoadingDef,
}

impl CardsDef {
    /// Every card names a protagonist (or the crew), and there is a tip to show.
    pub fn check(&self) -> Result<()> {
        let crew = crate::protagonists::def()?;
        for c in &self.intro {
            if c.who != "crew" && crew.index(&c.who).is_none() {
                return Err(anyhow!("frontend.toml: intro card '{}': no protagonist '{}'", c.heading, c.who));
            }
        }
        if self.loading.tips.is_empty() {
            return Err(anyhow!("frontend.toml: [loading] has no tips"));
        }
        Ok(())
    }

    /// Every clip a card poses with (the content check looks each up).
    pub fn clips(&self) -> impl Iterator<Item = &str> {
        self.intro.iter().map(|c| c.clip.as_str())
    }
}

pub fn def() -> Result<Arc<CardsDef>> {
    static CACHE: Mutex<Option<(String, Arc<CardsDef>)>> = Mutex::new(None);
    let text = crate::content::text("frontend.toml", include_str!("../../../content/freeway/frontend.toml"));
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((t, d)) = cache.as_ref()
        && *t == *text
    {
        return Ok(d.clone());
    }
    let d: CardsDef = toml::from_str(&text).map_err(|e| anyhow!("content/freeway/frontend.toml: {e}"))?;
    let d = Arc::new(d);
    *cache = Some((text.into_owned(), d.clone()));
    Ok(d)
}

/// Seconds an intro card holds, and the loading card (✕ moves on sooner).
pub const CARD_SECS: f32 = 4.5;
pub const LOADING_SECS: f32 = 2.5;

/// The cards up (part of the game's state).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Cards {
    /// The new game's intro (else one loading card), the card shown and real seconds on it.
    pub intro: bool,
    pub at: usize,
    pub t: f32,
    /// Loading cards shown so far (each takes the next tip).
    pub shown: usize,
    /// The protagonists not played, standing in for the intro (by protagonist), and where the
    /// crew stands and faces.
    pub extras: Vec<(usize, EntityId)>,
    pub spot: Vec3,
    pub facing: f32,
}

/// Which way the crew faces for the intro: into the golden-hour sun (west), so it lights them.
const INTRO_FACING: f32 = -std::f32::consts::FRAC_PI_2;
/// How far apart they stand.
const SPACING: f32 = 1.3;

fn forward(facing: f32) -> Vec3 {
    Vec3::new(facing.sin(), 0.0, facing.cos())
}

impl Freeway {
    /// The new game's intro (after `new_game`): the crew side by side where Dez starts.
    pub(crate) fn cards_intro(&mut self, sim: &mut Sim) {
        let Some(feet) = sim.state.player.and_then(|p| sim.feet(p)) else { return };
        self.put_player(sim, feet, INTRO_FACING);
        let across = forward(INTRO_FACING).cross(Vec3::Y);
        self.cards.extras.clear();
        if let Ok(crew) = crate::protagonists::def() {
            let mut side = 1.0;
            for (i, p) in crew.protagonist.iter().enumerate() {
                if i == self.crew.current {
                    continue;
                }
                let at = feet + across * SPACING * side;
                let at = Vec3::new(at.x, self.city.as_ref().map_or(feet.y, |c| c.ground_y(glam::Vec2::new(at.x, at.z))), at.z);
                let id = sim.spawn_npc("intro", at, INTRO_FACING, p.puppet(), None, None);
                self.cards.extras.push((i, id));
                side = -side;
            }
        }
        self.cards.intro = true;
        self.cards.at = 0;
        self.cards.t = 0.0;
        self.cards.spot = feet;
        self.cards.facing = INTRO_FACING;
        self.cards_pose(sim);
    }

    /// The loading card on the way into play: whoever is played, as they stand.
    pub(crate) fn cards_loading(&mut self, sim: &mut Sim) {
        self.cards.intro = false;
        self.cards.at = 0;
        self.cards.t = 0.0;
        self.cards.extras.clear();
        let p = sim.player();
        self.cards.spot = p.map_or(Vec3::ZERO, |e| e.pos);
        self.cards.facing = p.and_then(|e| e.character.as_ref()).map_or(0.0, |c| c.facing);
        sim.stream_now();
    }

    /// The intro card shown (None: the loading card).
    fn intro_card(&self) -> Option<CardDef> {
        if !self.cards.intro {
            return None;
        }
        def().ok()?.intro.get(self.cards.at).cloned()
    }

    /// Everyone on the card in its pose (the clip's first frame: the world holds still).
    fn cards_pose(&self, sim: &mut Sim) {
        let Some(card) = self.intro_card() else { return };
        let mut set = |id: EntityId| {
            if let Some(c) = sim.state.entities.get_mut(id).and_then(|e| e.character.as_mut())
                && let Some(p) = &c.puppet
            {
                let mut p = (**p).clone();
                p.idle_clip = card.clip.clone();
                c.puppet = Some(Arc::new(p));
            }
        };
        if let Some(pid) = sim.state.player {
            set(pid);
        }
        for (_, id) in &self.cards.extras {
            set(*id);
        }
    }

    /// One tick on the cards: ✕ the next card, ○ (or Options) the rest; play after the last.
    pub(crate) fn cards_tick(&mut self, sim: &mut Sim, input: &InputFrame, real: f32, events: &mut Vec<SimEvent>) {
        if self.front.page != Page::Loading {
            return;
        }
        self.cards.t += real;
        let pressed = |b: u32| input.pad.pressed & b != 0;
        let next = (pressed(pad::SOUTH) || input.cmds.iter().any(|c| c.starts_with("ui.accept"))) && self.cards.t > 0.3;
        let skip = pressed(pad::EAST) || pressed(pad::START);
        let n = if self.cards.intro { def().map(|d| d.intro.len()).unwrap_or(0) } else { 1 };
        let secs = if self.cards.intro { CARD_SECS } else { LOADING_SECS };
        if next || self.cards.t >= secs {
            self.cards.at += 1;
            self.cards.t = 0.0;
            if self.cards.at < n {
                events.push(SimEvent::Sound { cue: "ui.move".into(), pos: None, gain: 1.0, pitch: 1.0 });
                self.cards_pose(sim);
                return;
            }
        }
        if skip || self.cards.at >= n {
            self.cards_end(sim);
        }
    }

    /// Off the cards and into play: the stand-ins go, the player is themselves again.
    fn cards_end(&mut self, sim: &mut Sim) {
        for (_, id) in std::mem::take(&mut self.cards.extras) {
            sim.despawn(id);
        }
        if self.cards.intro {
            // (Back as the new game starts: facing east along Sunset Row.)
            let spot = self.cards.spot;
            self.put_player(sim, spot, std::f32::consts::FRAC_PI_2);
            self.cam.face(std::f32::consts::FRAC_PI_2);
        }
        if !self.cards.intro {
            self.cards.shown += 1;
        }
        self.cards.intro = false;
        self.dress_player(sim);
        self.roam_age = 0.0;
        self.front.go(Page::Play, self.ui_time);
    }

    /// Who the card frames: their feet and the way they face.
    fn card_subject(&self, sim: &Sim) -> (Vec3, f32, bool) {
        let me = sim.state.player.and_then(|p| sim.feet(p)).unwrap_or(self.cards.spot);
        let Some(card) = self.intro_card() else { return (me, self.cards.facing, false) };
        if card.who == "crew" {
            return (me, self.cards.facing, true);
        }
        let i = crate::protagonists::def().ok().and_then(|d| d.index(&card.who));
        let at = self.cards.extras.iter().find(|(k, _)| Some(*k) == i).and_then(|(_, id)| sim.feet(*id)).unwrap_or(me);
        (at, self.cards.facing, false)
    }

    /// The card's picture: close on the face and shoulders, pushing in slowly (all three
    /// together for the crew's card).
    pub(crate) fn cards_camera(&self, sim: &Sim) -> Option<CameraIntent> {
        if self.front.page != Page::Loading {
            return None;
        }
        let (feet, facing, crew) = self.card_subject(sim);
        let f = forward(facing);
        let side = f.cross(Vec3::Y);
        let k = (self.cards.t / CARD_SECS).clamp(0.0, 1.0);
        let (dist, height, look, fov) = if crew { (5.6 - k * 0.4, 1.55, 1.05, 40.0) } else { (2.5 - k * 0.25, 1.5, 1.32, 30.0) };
        let target = feet + Vec3::Y * look;
        let mut eye = feet + f * dist + side * (dist * 0.18) + Vec3::Y * height;
        // (Not through a wall behind the camera.)
        let d = eye - target;
        if let Some(h) = sim.raycast_static(target, d.normalize_or(Vec3::Z), d.length()) {
            eye = target + d.normalize_or(Vec3::Z) * (h.point.distance(target) - 0.3).max(0.8);
        }
        Some(CameraIntent { eye, target, up: Vec3::Y, fov, near: 0.1, far: 1500.0, cut: self.cards.t < 0.05, shake: 0.0 })
    }

    /// The filter over the picture.
    pub(crate) fn cards_style(&self) -> Option<StylizeIntent> {
        if self.front.page != Page::Loading {
            return None;
        }
        let style = match self.intro_card() {
            Some(c) => c.style,
            None => def().ok()?.loading.style,
        };
        Some(style.intent())
    }

    /// The card's words over the picture.
    pub(crate) fn cards_screen(&self) -> Option<Screen> {
        if self.front.page != Page::Loading {
            return None;
        }
        let d = def().ok()?;
        let (id, card, hints) = match self.intro_card() {
            Some(c) => (
                format!("intro/{}", c.who),
                LoadingCard { heading: c.heading.clone(), tip: c.line.clone(), progress: -1.0, tag: Some(String::new()) },
                vec![
                    Prompt { button: PadButton::South, label: "Next".into(), hold: None },
                    Prompt { button: PadButton::East, label: "Skip".into(), hold: None },
                ],
            ),
            None => {
                let tip = d.loading.tips.get(self.cards.shown % d.loading.tips.len().max(1)).cloned().unwrap_or_default();
                let progress = (self.cards.t / LOADING_SECS).clamp(0.0, 1.0);
                ("loading".to_string(), LoadingCard { heading: "PALOMA BAY".into(), tip, progress, tag: None }, Vec::new())
            }
        };
        Some(Screen {
            id,
            title: String::new(),
            tabs: Vec::new(),
            tab: 0,
            hints,
            body: ScreenBody::Loading(card),
            backdrop: Backdrop::Clear,
        })
    }
}
