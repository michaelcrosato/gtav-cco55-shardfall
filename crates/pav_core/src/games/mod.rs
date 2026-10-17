//! Games built on the engine: the hook a new game implements, and the list of games.
//!
//! A game is a struct holding its rules' state (`Clone` and serde, like everything in
//! `SimState`) that implements [`Game`], plus a `build` function that makes its scene. One
//! line in [`GAMES`] and `load scene=<key>` starts it in the tools, the window
//! (`shardfall --scene <key>`) and the browser (`?scene=<key>`). `starter.rs` is the template
//! to copy; `docs/MAKE_A_GAME.md` is the guide. Shardfall (`crate::arpg`) predates this hook and
//! keeps its own.

use std::any::Any;
use std::collections::BTreeMap;

use anyhow::Result;
use serde::de::{DeserializeOwned, Error as _};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::entity::EntityId;
use crate::frame::SimEvent;
use crate::input::InputFrame;
use crate::present::PlayFrame;
use crate::sim::Sim;
use crate::vehicle::VehicleInput;

pub mod starter;

/// Every game the engine knows. `load scene=<key>` (or `<key>/<rest>`) builds it.
pub static GAMES: &[Entry] =
    &[entry::<starter::Starter>("Starter: collect every shard while slimes chase you (the template to copy).", starter::build)];

/// Still playing, won or lost.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    #[default]
    Playing,
    Won,
    Lost,
}

/// A game's rules. Every method has a default, so a game implements only what it uses.
///
/// The engine calls `pre` and `post` once per tick (60 a second) from `Sim::step`; everything
/// else is read-only. While `pre` or `post` runs, the game is out of `sim.state.play`, so use
/// `self` for the game's own state and `sim` for the world.
pub trait Game: Clone + Serialize + DeserializeOwned + Send + Sync + 'static {
    /// The scene key that starts the game: `load scene=<KEY>`, `--scene <KEY>`, `?scene=<KEY>`.
    const KEY: &'static str;

    /// Before characters move: read the player's input and decide what the characters the
    /// game drives do this tick. A returned input replaces that character's own (its `ai`
    /// brain, or the player's keys for the player).
    fn pre(
        &mut self,
        _sim: &mut Sim,
        _input: &InputFrame,
        _dt: f32,
        _events: &mut Vec<SimEvent>,
    ) -> BTreeMap<EntityId, InputFrame> {
        BTreeMap::new()
    }

    /// After physics: the rules (pickups, damage, score, win and loss). Push `SimEvent`s for
    /// sounds and effects (`Coin`, `Pickup`, `Hit`, `Explosion`...).
    fn post(&mut self, _sim: &mut Sim, _dt: f32, _events: &mut Vec<SimEvent>) {}

    /// HUD lines: drawn at the top left of the window, returned by the `play` tool.
    fn hud(&self, _sim: &Sim) -> Vec<String> {
        Vec::new()
    }

    /// The game's state for tools and tests (`play`), as JSON.
    fn status(&self, _sim: &Sim) -> serde_json::Value {
        serde_json::Value::Null
    }

    fn outcome(&self) -> Outcome {
        Outcome::Playing
    }

    /// The input a good player would give this tick. `play bot=true` and the tests drive the
    /// player with it; teach it every new rule, and keep a test that it wins.
    fn bot(&self, _sim: &Sim) -> InputFrame {
        InputFrame::default()
    }

    /// How fast the world runs (1 = normal; a weapon wheel's slow motion is 0.25). Physics,
    /// characters, behaviours and the `dt` given to `pre` and `post` all scale; `sim.dt()` stays
    /// the real tick.
    fn time_scale(&self) -> f32 {
        1.0
    }

    /// The world stands still (title, pause menu, loading): only `pre` and `post` run, with
    /// `dt` = 0, and ticks still count.
    fn world_paused(&self) -> bool {
        false
    }

    /// Driving for vehicles this tick (after characters move, before physics): the player's car
    /// from the controller, traffic from its drivers. A vehicle left out idles, unless
    /// `legacy_controls` lets the player drive it the engine demo's way.
    fn vehicles(&mut self, _sim: &mut Sim, _dt: f32) -> BTreeMap<EntityId, VehicleInput> {
        BTreeMap::new()
    }

    /// The game's camera, UI, drawing, atmosphere, audio and host requests for this tick
    /// (`crate::present`), or None to leave all of it to the engine. Called once per published
    /// frame, after the tick.
    fn present(&self, _sim: &Sim) -> Option<PlayFrame> {
        None
    }

    /// The rewind window in seconds (0 = off), or None for the engine's setting.
    fn history_seconds(&self) -> Option<f32> {
        None
    }

    /// The engine demo's own player controls: interact gets in and out of vehicles, use and
    /// primary throw bombs, crawl toggles, and the raw input moves the player and steers its car.
    /// A game with its own controls returns false: then only `pre` (characters) and `vehicles`
    /// decide, and the host stops filling the demo's button bits.
    fn legacy_controls(&self) -> bool {
        true
    }
}

/// One line of [`GAMES`]: made with [`entry`].
pub struct Entry {
    pub key: &'static str,
    pub about: &'static str,
    /// Builds the scene and sets `sim.state.play`. The `&str` is the rest of the scene name
    /// after `<key>/` (a level number, say), if any.
    pub build: fn(&mut Sim, Option<&str>) -> Result<()>,
    load: fn(ciborium::Value) -> Result<GameBox, String>,
}

/// A [`GAMES`] line for the game type `G`.
pub const fn entry<G: Game>(about: &'static str, build: fn(&mut Sim, Option<&str>) -> Result<()>) -> Entry {
    Entry { key: G::KEY, about, build, load: load_as::<G> }
}

fn load_as<G: Game>(v: ciborium::Value) -> Result<GameBox, String> {
    v.deserialized::<G>().map(GameBox::new).map_err(|e| e.to_string())
}

/// A game's level text: `crates/pav_core/src/games/<key>.toml` read from disk when the program
/// runs from the repository's root (`pav`, `cargo run`), so an edit shows at the next `load`
/// with no rebuild; else `built_in`, the copy embedded with `include_str!` (tests, the browser,
/// a shipped executable).
pub fn level_text(key: &str, built_in: &'static str) -> std::borrow::Cow<'static, str> {
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(text) = std::fs::read_to_string(format!("crates/pav_core/src/games/{key}.toml")) {
        return text.into();
    }
    #[cfg(target_arch = "wasm32")]
    let _ = key; // the browser has no files
    built_in.into()
}

/// Games defined in other crates (a game's own crate calls [`register`] from its binaries and
/// tests), after the built-in [`GAMES`].
static REGISTERED: std::sync::RwLock<Vec<&'static Entry>> = std::sync::RwLock::new(Vec::new());

/// Adds a game defined in another crate, so `load scene=<key>` (and snapshot files naming it)
/// work in this process. Binaries call it before building a scene or loading a snapshot; tests
/// call it in their setup. Registering the same key again does nothing.
pub fn register(entry: &'static Entry) {
    let mut list = REGISTERED.write().unwrap_or_else(|e| e.into_inner());
    if GAMES.iter().chain(list.iter().copied()).all(|g| g.key != entry.key) {
        list.push(entry);
    }
}

/// Every game: the built-in [`GAMES`], then the registered ones.
pub fn all() -> Vec<&'static Entry> {
    let list = REGISTERED.read().unwrap_or_else(|e| e.into_inner());
    GAMES.iter().chain(list.iter().copied()).collect()
}

/// The game registered under `key` (built in or registered).
pub fn find(key: &str) -> Option<&'static Entry> {
    all().into_iter().find(|g| g.key == key)
}

/// A running game in `SimState::play`: clones (rewind, snapshots) and serializes (snapshot
/// files, the state hash) through the game's own derives.
pub struct GameBox(Box<dyn DynGame>);

impl GameBox {
    pub fn new<G: Game>(game: G) -> Self {
        Self(Box::new(game))
    }

    pub fn key(&self) -> &'static str {
        self.0.key()
    }

    /// The game as its own type (None for another game).
    pub fn get<G: Game>(&self) -> Option<&G> {
        self.0.as_any().downcast_ref()
    }

    pub fn get_mut<G: Game>(&mut self) -> Option<&mut G> {
        self.0.as_any_mut().downcast_mut()
    }

    pub fn hud(&self, sim: &Sim) -> Vec<String> {
        self.0.hud(sim)
    }

    pub fn status(&self, sim: &Sim) -> serde_json::Value {
        self.0.status(sim)
    }

    pub fn outcome(&self) -> Outcome {
        self.0.outcome()
    }

    pub fn bot(&self, sim: &Sim) -> InputFrame {
        self.0.bot(sim)
    }

    pub fn time_scale(&self) -> f32 {
        self.0.time_scale()
    }

    pub fn world_paused(&self) -> bool {
        self.0.world_paused()
    }

    pub fn present(&self, sim: &Sim) -> Option<PlayFrame> {
        self.0.present(sim)
    }

    pub fn history_seconds(&self) -> Option<f32> {
        self.0.history_seconds()
    }

    pub fn legacy_controls(&self) -> bool {
        self.0.legacy_controls()
    }
}

impl Clone for GameBox {
    fn clone(&self) -> Self {
        Self(self.0.clone_box())
    }
}

#[derive(Serialize, Deserialize)]
struct Saved {
    key: String,
    state: ciborium::Value,
}

impl Serialize for GameBox {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let state = self.0.save().map_err(serde::ser::Error::custom)?;
        Saved { key: self.key().to_string(), state }.serialize(s)
    }
}

impl<'de> Deserialize<'de> for GameBox {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let saved = Saved::deserialize(d)?;
        let Some(entry) = find(&saved.key) else {
            let known: Vec<&str> = all().into_iter().map(|g| g.key).collect();
            return Err(D::Error::custom(format!("unknown game '{}' (known: {})", saved.key, known.join(", "))));
        };
        (entry.load)(saved.state).map_err(D::Error::custom)
    }
}

/// [`Game`] as an object (the trait itself isn't one: `Clone` and serde).
trait DynGame: Send + Sync {
    fn key(&self) -> &'static str;
    fn clone_box(&self) -> Box<dyn DynGame>;
    fn save(&self) -> Result<ciborium::Value, String>;
    fn pre(&mut self, sim: &mut Sim, input: &InputFrame, dt: f32, events: &mut Vec<SimEvent>) -> BTreeMap<EntityId, InputFrame>;
    fn post(&mut self, sim: &mut Sim, dt: f32, events: &mut Vec<SimEvent>);
    fn hud(&self, sim: &Sim) -> Vec<String>;
    fn status(&self, sim: &Sim) -> serde_json::Value;
    fn outcome(&self) -> Outcome;
    fn bot(&self, sim: &Sim) -> InputFrame;
    fn time_scale(&self) -> f32;
    fn world_paused(&self) -> bool;
    fn vehicles(&mut self, sim: &mut Sim, dt: f32) -> BTreeMap<EntityId, VehicleInput>;
    fn present(&self, sim: &Sim) -> Option<PlayFrame>;
    fn history_seconds(&self) -> Option<f32>;
    fn legacy_controls(&self) -> bool;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<G: Game> DynGame for G {
    fn key(&self) -> &'static str {
        G::KEY
    }
    fn clone_box(&self) -> Box<dyn DynGame> {
        Box::new(self.clone())
    }
    fn save(&self) -> Result<ciborium::Value, String> {
        ciborium::Value::serialized(self).map_err(|e| e.to_string())
    }
    fn pre(&mut self, sim: &mut Sim, input: &InputFrame, dt: f32, events: &mut Vec<SimEvent>) -> BTreeMap<EntityId, InputFrame> {
        Game::pre(self, sim, input, dt, events)
    }
    fn post(&mut self, sim: &mut Sim, dt: f32, events: &mut Vec<SimEvent>) {
        Game::post(self, sim, dt, events)
    }
    fn hud(&self, sim: &Sim) -> Vec<String> {
        Game::hud(self, sim)
    }
    fn status(&self, sim: &Sim) -> serde_json::Value {
        Game::status(self, sim)
    }
    fn outcome(&self) -> Outcome {
        Game::outcome(self)
    }
    fn bot(&self, sim: &Sim) -> InputFrame {
        Game::bot(self, sim)
    }
    fn time_scale(&self) -> f32 {
        Game::time_scale(self)
    }
    fn world_paused(&self) -> bool {
        Game::world_paused(self)
    }
    fn vehicles(&mut self, sim: &mut Sim, dt: f32) -> BTreeMap<EntityId, VehicleInput> {
        Game::vehicles(self, sim, dt)
    }
    fn present(&self, sim: &Sim) -> Option<PlayFrame> {
        Game::present(self, sim)
    }
    fn history_seconds(&self) -> Option<f32> {
        Game::history_seconds(self)
    }
    fn legacy_controls(&self) -> bool {
        Game::legacy_controls(self)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl Sim {
    /// The running game from [`GAMES`], if this scene is one.
    pub fn play(&self) -> Option<&GameBox> {
        self.state.play.as_ref()
    }

    /// The running game's outcome (`Playing` when there is none).
    pub fn outcome(&self) -> Outcome {
        self.play().map(|g| g.outcome()).unwrap_or_default()
    }

    /// The running game's bot input (none without a game).
    pub fn bot_input(&self) -> InputFrame {
        self.play().map(|g| g.bot(self)).unwrap_or_default()
    }

    pub(crate) fn play_pre(&mut self, input: &InputFrame, dt: f32, events: &mut Vec<SimEvent>) -> BTreeMap<EntityId, InputFrame> {
        let Some(mut g) = self.state.play.take() else { return BTreeMap::new() };
        let out = g.0.pre(self, input, dt, events);
        self.state.play = Some(g);
        out
    }

    pub(crate) fn play_post(&mut self, dt: f32, events: &mut Vec<SimEvent>) {
        let Some(mut g) = self.state.play.take() else { return };
        g.0.post(self, dt, events);
        self.state.play = Some(g);
    }

    pub(crate) fn play_hud(&self) -> Vec<String> {
        self.play().map(|g| g.hud(self)).unwrap_or_default()
    }

    pub(crate) fn play_vehicles(&mut self, dt: f32) -> BTreeMap<EntityId, VehicleInput> {
        let Some(mut g) = self.state.play.take() else { return BTreeMap::new() };
        let out = g.0.vehicles(self, dt);
        self.state.play = Some(g);
        out
    }

    /// The running game's time scale (1 without a game).
    pub fn play_time_scale(&self) -> f32 {
        self.play().map(|g| g.time_scale()).unwrap_or(1.0).max(0.0)
    }

    /// True while the running game holds the world still.
    pub fn world_paused(&self) -> bool {
        self.play().is_some_and(|g| g.world_paused())
    }

    /// False when the running game has its own controls (`Game::legacy_controls`).
    pub fn legacy_controls(&self) -> bool {
        self.play().is_none_or(|g| g.legacy_controls())
    }

    /// The running game's presentation for this tick (`Game::present`).
    pub fn present(&self) -> Option<PlayFrame> {
        self.play().and_then(|g| g.present(self))
    }

    /// Plays the running game with its bot until it is won or lost, or `ticks` pass.
    /// Returns the outcome and the ticks played.
    pub fn run_bot(&mut self, ticks: u64) -> (Outcome, u64) {
        for t in 0..ticks {
            let o = self.outcome();
            if o != Outcome::Playing {
                return (o, t);
            }
            let input = self.bot_input();
            self.step(&input);
        }
        (self.outcome(), ticks)
    }
}
