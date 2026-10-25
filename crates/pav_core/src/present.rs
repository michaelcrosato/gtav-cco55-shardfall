//! What a game shows and plays beyond the world itself, as plain data: the UI model (HUD, menus,
//! wheels, phone, banners...), things to draw from game state, the camera, the atmosphere, the
//! audio scene, the map picture and requests to the host (save files, settings). A game builds a
//! [`PlayFrame`] each tick in `Game::present`; the engine's view, UI kit (`pav_ui`) and synth draw
//! and play it, and tools read it as JSON — so a game needs no window, GPU or audio code, and
//! everything it shows is checkable without a display (PLAN.md §6.5 E4).

use std::sync::Arc;

use glam::{Quat, Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::color::Color;
use crate::shape::{Look, Shape};
pub use crate::statics::Glow;

/// Everything a game hands the host for one tick besides the world.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayFrame {
    pub ui: UiModel,
    pub draw: DrawList,
    /// The game's camera; None leaves the camera to the view's own rig.
    pub camera: Option<CameraIntent>,
    /// The game's atmosphere (sun, sky, fog, grading...); None keeps the view's settings.
    pub view: Option<ViewIntent>,
    pub audio: AudioScene,
    /// The picture behind the minimap and the pause map (the same `Arc` every tick; a new
    /// `version` means a new picture).
    #[serde(skip)]
    pub map: Option<Arc<MapImage>>,
    /// The whole world in low detail, drawn wherever streaming has not loaded a region (the
    /// same `Arc` every tick).
    #[serde(skip)]
    pub far: Option<Arc<FarView>>,
    pub app: AppRequests,
}

/// A low-detail picture of a big world (a city's building masses, ground and water): one mesh
/// per 32 m streaming chunk, so the view can draw exactly the chunks that are not loaded.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FarView {
    pub chunks: Vec<((i32, i32), Arc<FarMesh>)>,
    /// Changes when the meshes do.
    pub version: u64,
}

/// A vertex-coloured triangle mesh in world space, and its bounds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FarMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub min: Vec3,
    pub max: Vec3,
}

impl FarMesh {
    /// Adds an axis-aligned box's top and four sides (bottoms are never seen from afar).
    pub fn add_box(&mut self, min: Vec3, max: Vec3, color: [f32; 3]) {
        let (a, b) = (min, max);
        let faces: [([f32; 3], [Vec3; 4]); 5] = [
            (
                [0.0, 1.0, 0.0],
                [Vec3::new(a.x, b.y, a.z), Vec3::new(a.x, b.y, b.z), Vec3::new(b.x, b.y, b.z), Vec3::new(b.x, b.y, a.z)],
            ),
            (
                [0.0, 0.0, -1.0],
                [Vec3::new(a.x, a.y, a.z), Vec3::new(a.x, b.y, a.z), Vec3::new(b.x, b.y, a.z), Vec3::new(b.x, a.y, a.z)],
            ),
            (
                [0.0, 0.0, 1.0],
                [Vec3::new(b.x, a.y, b.z), Vec3::new(b.x, b.y, b.z), Vec3::new(a.x, b.y, b.z), Vec3::new(a.x, a.y, b.z)],
            ),
            (
                [-1.0, 0.0, 0.0],
                [Vec3::new(a.x, a.y, b.z), Vec3::new(a.x, b.y, b.z), Vec3::new(a.x, b.y, a.z), Vec3::new(a.x, a.y, a.z)],
            ),
            (
                [1.0, 0.0, 0.0],
                [Vec3::new(b.x, a.y, a.z), Vec3::new(b.x, b.y, a.z), Vec3::new(b.x, b.y, b.z), Vec3::new(b.x, a.y, b.z)],
            ),
        ];
        for (n, quad) in faces {
            self.add_quad(quad, n, color);
        }
    }

    /// Adds a quad (corners in order) facing `n`.
    pub fn add_quad(&mut self, quad: [Vec3; 4], n: [f32; 3], color: [f32; 3]) {
        let i0 = self.positions.len() as u32;
        if self.positions.is_empty() {
            self.min = quad[0];
            self.max = quad[0];
        }
        for p in quad {
            self.positions.push(p.to_array());
            self.normals.push(n);
            self.colors.push(color);
            self.min = self.min.min(p);
            self.max = self.max.max(p);
        }
        self.indices.extend_from_slice(&[i0, i0 + 1, i0 + 2, i0, i0 + 2, i0 + 3]);
    }
}

// ---------------------------------------------------------------------------------------------
// UI model
// ---------------------------------------------------------------------------------------------

/// The whole UI for one tick. Text may carry light markup: `~y~ ~b~ ~r~ ~g~ ~o~ ~p~` colour the
/// words that follow (yellow, blue, red, green, orange, pink), `~s~` resets, and `~btn:NAME~`
/// draws a controller glyph (`~btn:south~`, `~btn:r2~`; names as [`PadButton`] in snake case).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiModel {
    pub hud: Option<Hud>,
    /// A full-screen page (title, menus, pause, map, results, loading).
    pub screen: Option<Screen>,
    pub wheel: Option<Wheel>,
    pub phone: Option<Phone>,
    pub banners: Vec<Banner>,
    pub toasts: Vec<Toast>,
    pub subtitle: Option<Subtitle>,
    /// Context prompts ("△ Enter vehicle"), bottom right.
    pub prompts: Vec<Prompt>,
    /// A tutorial tip, top left.
    pub help: Option<Help>,
    /// World-anchored markers drawn on screen (off-screen ones as edge arrows).
    pub markers: Vec<ScreenMarker>,
    pub overlay: Overlay,
    /// Show the FPS and frame-time readout.
    pub perf: bool,
    pub glyphs: GlyphStyle,
    /// HUD scale (1 = the reference sizes at 1080p) and safe-area margin (fraction of the screen
    /// on each side).
    pub scale: f32,
    pub safe_margin: f32,
    /// Subtitles' size against the reference (1), and a high-contrast HUD: darker, opaque
    /// backplates with an outline, a plate behind subtitles (accessibility settings).
    pub subtitle_scale: f32,
    pub high_contrast: bool,
    /// Seconds of UI time (game time, also while the world is paused): animations key off it so
    /// captures are reproducible.
    pub time: f32,
}

impl Default for UiModel {
    fn default() -> Self {
        Self {
            hud: None,
            screen: None,
            wheel: None,
            phone: None,
            banners: Vec::new(),
            toasts: Vec::new(),
            subtitle: None,
            prompts: Vec::new(),
            help: None,
            markers: Vec::new(),
            overlay: Overlay::default(),
            perf: true,
            glyphs: GlyphStyle::Auto,
            scale: 1.0,
            safe_margin: 0.05,
            subtitle_scale: 1.0,
            high_contrast: false,
            time: 0.0,
        }
    }
}

/// Which controller glyphs prompts show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GlyphStyle {
    /// The last device used (PlayStation for an unknown pad).
    #[default]
    Auto,
    PlayStation,
    Xbox,
    Keyboard,
}

/// A controller button, by position (south = ✕ on PlayStation, A on Xbox).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PadButton {
    South,
    East,
    West,
    North,
    L1,
    R1,
    L2,
    R2,
    L3,
    R3,
    Up,
    Down,
    Left,
    Right,
    Start,
    Select,
    Touchpad,
    LeftStick,
    RightStick,
}

/// The in-play HUD (§5.9). Elements a game leaves None are not drawn.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Hud {
    pub minimap: Option<Minimap>,
    pub vitals: Option<Vitals>,
    pub money: Option<Money>,
    pub wanted: Option<Wanted>,
    pub weapon: Option<WeaponHud>,
    /// The current objective, bottom centre.
    pub objective: Option<String>,
    pub timer: Option<TimerHud>,
    pub vehicle: Option<VehicleHud>,
    pub crosshair: Option<Crosshair>,
    pub meters: Vec<Meter>,
    /// 0..1: the whole HUD fades (cutscenes, idle).
    pub opacity: f32,
}

impl Default for Hud {
    fn default() -> Self {
        Self {
            minimap: None,
            vitals: None,
            money: None,
            wanted: None,
            weapon: None,
            objective: None,
            timer: None,
            vehicle: None,
            crosshair: None,
            meters: Vec::new(),
            opacity: 1.0,
        }
    }
}

/// The radar, bottom left: a window on the map picture turned with the camera.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Minimap {
    /// World position (x, z) at the centre, and the heading up the radar (radians; 0 = north).
    pub center: Vec2,
    pub heading: f32,
    /// World metres from the centre to the radar's edge.
    pub radius: f32,
    pub player: Vec2,
    pub player_heading: f32,
    pub blips: Vec<Blip>,
    /// The GPS route (world x, z).
    pub route: Vec<Vec2>,
    pub cones: Vec<ConeBlip>,
    pub circles: Vec<CircleBlip>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Blip {
    pub pos: Vec2,
    pub icon: BlipIcon,
    pub color: Color,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Pinned to the radar's edge when outside it.
    #[serde(default)]
    pub clamp: bool,
    /// Above (1) or below (-1) the player, for the up/down arrow.
    #[serde(default)]
    pub height: i8,
    #[serde(default)]
    pub pulse: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlipIcon {
    Dot,
    /// A mission giver's letter.
    Letter(char),
    Waypoint,
    Objective,
    Enemy,
    Ally,
    Police,
    Helicopter,
    Vehicle,
    Shop,
    Gun,
    Garage,
    Safehouse,
    Hospital,
    Activity,
    Race,
    Jump,
    Collectible,
    Heist,
}

/// A vision cone on the radar (police searching).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConeBlip {
    pub pos: Vec2,
    /// Facing (radians, 0 = north), full angle (radians) and reach (m).
    pub dir: f32,
    pub angle: f32,
    pub range: f32,
    pub color: Color,
}

/// A circle on the radar (the search zone).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CircleBlip {
    pub pos: Vec2,
    pub radius: f32,
    pub color: Color,
    #[serde(default)]
    pub flashing: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Vitals {
    /// Fractions 0..1.
    pub health: f32,
    pub armor: f32,
    pub ability: f32,
    pub ability_color: Color,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Money {
    pub amount: i64,
    /// The last change and seconds since it (shown as "+$1,500" for a while).
    #[serde(default)]
    pub delta: i64,
    #[serde(default)]
    pub delta_age: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Wanted {
    pub stars: u8,
    pub max: u8,
    /// Searching: the stars flash.
    pub flashing: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WeaponHud {
    pub name: String,
    /// An icon key (`pistol`, `smg`, `fists`...).
    pub icon: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reserve: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TimerHud {
    pub label: String,
    pub seconds: f32,
    /// Draw in the warning colour (running out).
    #[serde(default)]
    pub warn: bool,
}

/// Shown for a few seconds on entering a vehicle or changing station.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VehicleHud {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub station: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub song: Option<String>,
    /// Seconds since it appeared.
    #[serde(default)]
    pub age: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Crosshair {
    /// Spread in degrees (the reticle opens with it).
    pub spread: f32,
    /// 0..1 hit-marker and kill-marker flashes.
    #[serde(default)]
    pub hit: f32,
    #[serde(default)]
    pub kill: f32,
    /// Over a target (the reticle turns red).
    #[serde(default)]
    pub target: bool,
}

/// A labelled bar (detection, distance to a target, a hold-up's bag...).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Meter {
    pub label: String,
    pub value: f32,
    pub color: Color,
}

/// A full-screen page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Screen {
    /// A stable id (`title`, `pause/map`, `settings/audio`...): tools and the pointer use it.
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub tabs: Vec<String>,
    #[serde(default)]
    pub tab: usize,
    /// The button-hint bar (glyph + verb), bottom right.
    #[serde(default)]
    pub hints: Vec<Prompt>,
    pub body: ScreenBody,
    #[serde(default)]
    pub backdrop: Backdrop,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Backdrop {
    /// The world shows through (title, attract).
    #[default]
    Clear,
    /// The world dimmed and blurred-looking (pause).
    Dim,
    /// Opaque (loading).
    Solid,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ScreenBody {
    Title(TitleCard),
    Menu(MenuList),
    Cards(CardRow),
    Map(MapView),
    Text(TextCard),
    Loading(LoadingCard),
    Results(Results),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TitleCard {
    /// "PRESS ANY BUTTON", or None when a menu shows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub menu: Option<MenuList>,
    /// A caption under the logo (attract: "A CITY THAT REACTS").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    /// Draw the logo large (title) or small in a corner (attract).
    #[serde(default)]
    pub small_logo: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MenuList {
    pub items: Vec<MenuItem>,
    pub focus: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MenuItem {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub value: ItemValue,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ItemValue {
    #[default]
    Button,
    Toggle {
        on: bool,
    },
    /// 0..1 and the text shown beside the bar.
    Slider {
        value: f32,
        text: String,
    },
    Choice {
        options: Vec<String>,
        index: usize,
    },
}

/// Big cards side by side (the planning board, stats, shop categories).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CardRow {
    pub cards: Vec<Card>,
    pub focus: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Card {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default)]
    pub lines: Vec<String>,
    /// Labelled bars (0..1): stats, a weapon's damage and range.
    #[serde(default)]
    pub bars: Vec<(String, f32)>,
    pub accent: Color,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default = "yes")]
    pub enabled: bool,
}

/// The pause map.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MapView {
    /// World centre (x, z) and metres per screen height.
    pub center: Vec2,
    pub zoom: f32,
    pub cursor: Vec2,
    pub legend: bool,
    pub blips: Vec<Blip>,
    pub route: Vec<Vec2>,
    /// The blip under the cursor, described.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TextCard {
    pub heading: String,
    pub lines: Vec<String>,
}

/// A loading card: a big heading, a tip and a bar. On a solid backdrop the UI kit draws its own
/// art behind them; on a clear one the world shows through (a picture the game makes in the
/// engine, e.g. a character posed under `ViewIntent::stylize`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LoadingCard {
    pub heading: String,
    pub tip: String,
    /// 0..1, or negative for no bar (and no spinner: a card that waits on nothing).
    pub progress: f32,
    /// The word before the tip: None says "TIP", an empty one says nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Results {
    pub heading: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub medal: Option<Medal>,
    /// At most five big lines (label, value).
    pub rows: Vec<(String, String)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cash: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Medal {
    Bronze,
    Silver,
    Gold,
}

/// A radial menu: weapons, characters, radio stations.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Wheel {
    pub id: String,
    pub slots: Vec<WheelSlot>,
    pub focus: Option<usize>,
    /// Text in the middle (the focused slot's name and detail).
    pub center: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WheelSlot {
    pub label: String,
    pub icon: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default = "yes")]
    pub enabled: bool,
}

/// The phone (§5.6.9).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Phone {
    /// The open app (`home`, `contacts`, `messages`, `camera`...).
    pub app: String,
    pub title: String,
    pub tiles: Vec<PhoneTile>,
    pub rows: Vec<PhoneRow>,
    pub focus: usize,
    /// The clock on the phone.
    pub clock: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhoneTile {
    pub id: String,
    pub label: String,
    pub icon: String,
    #[serde(default)]
    pub badge: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhoneRow {
    pub title: String,
    #[serde(default)]
    pub detail: String,
}

/// Big centre text: MISSION PASSED, WASTED, BUSTED, UNIQUE STUNT BONUS.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Banner {
    pub kind: BannerKind,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    /// Seconds since it appeared, and how long it stays.
    pub age: f32,
    pub duration: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BannerKind {
    Passed,
    Failed,
    Wasted,
    Busted,
    Stunt,
    Info,
}

/// A notification (a text message, a heat report), sliding in at the right.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Toast {
    pub id: u64,
    pub icon: String,
    pub title: String,
    pub body: String,
    pub age: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Subtitle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Prompt {
    pub button: PadButton,
    pub label: String,
    /// A hold prompt's progress (0..1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hold: Option<f32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Help {
    pub text: String,
    pub age: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScreenMarker {
    pub pos: Vec3,
    pub icon: BlipIcon,
    pub color: Color,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Show the distance under it.
    #[serde(default)]
    pub distance: bool,
}

/// Whole-screen effects.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Overlay {
    /// 0 = clear, 1 = black.
    pub fade: f32,
    /// Cutscene bars (0..1).
    pub letterbox: f32,
    /// 0 = full colour, 1 = grey (wasted).
    pub desaturate: f32,
    /// A colour wash (linear RGB and strength): abilities.
    pub tint: Option<[f32; 4]>,
    pub vignette: f32,
}

// ---------------------------------------------------------------------------------------------
// Drawing from game state
// ---------------------------------------------------------------------------------------------

/// Things to draw this tick that are not entities: markers, held items, route ribbons, tracers,
/// decals, extra lights.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DrawList {
    pub assets: Vec<AssetDraw>,
    pub prims: Vec<PrimDraw>,
    pub lights: Vec<LightDraw>,
    pub spots: Vec<SpotDraw>,
    pub decals: Vec<DecalDraw>,
    pub ribbons: Vec<RibbonDraw>,
    pub tracers: Vec<TracerDraw>,
    pub texts: Vec<TextDraw>,
}

/// A library asset (`LIB/gtf_city/traffic_signal`) placed from game state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AssetDraw {
    pub asset: String,
    pub pos: Vec3,
    #[serde(default = "quat_id")]
    pub rot: Quat,
    #[serde(default = "one")]
    pub scale: f32,
    /// A stable id (outline group, material seeds).
    #[serde(default)]
    pub id: u32,
    #[serde(default)]
    pub seed: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrimDraw {
    pub shape: Shape,
    pub pos: Vec3,
    #[serde(default = "quat_id")]
    pub rot: Quat,
    pub color: Color,
    #[serde(default)]
    pub emissive: f32,
    #[serde(default)]
    pub look: Look,
    #[serde(default)]
    pub shadow: bool,
    /// When its emissive shows: always, or only after dark (`ViewIntent::street_lights`).
    #[serde(default, skip_serializing_if = "Glow::is_always")]
    pub glow: Glow,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LightDraw {
    pub pos: Vec3,
    pub color: Color,
    #[serde(default = "one")]
    pub intensity: f32,
    pub radius: f32,
    #[serde(default)]
    pub shadows: bool,
}

/// A spotlight, faked as a soft additive cone plus a light where it lands (searchlights,
/// headlight beams).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpotDraw {
    pub pos: Vec3,
    pub dir: Vec3,
    /// Half angle (radians) and reach (m).
    pub angle: f32,
    pub range: f32,
    pub color: Color,
    #[serde(default = "one")]
    pub intensity: f32,
}

/// A flat mark on a surface: skid marks, bullet holes, scorch, puddles.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DecalDraw {
    pub pos: Vec3,
    pub normal: Vec3,
    pub yaw: f32,
    pub size: Vec2,
    pub color: Color,
    #[serde(default = "one")]
    pub alpha: f32,
}

/// A strip along points (the GPS route on the road, a race line).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RibbonDraw {
    pub points: Vec<Vec3>,
    pub width: f32,
    pub color: Color,
    #[serde(default)]
    pub emissive: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TracerDraw {
    pub from: Vec3,
    pub to: Vec3,
    pub color: Color,
    pub width: f32,
}

/// Text in the world (signs, billboards, street names).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TextDraw {
    pub text: String,
    pub pos: Vec3,
    #[serde(default = "quat_id")]
    pub rot: Quat,
    /// Cap height (m).
    pub size: f32,
    pub color: Color,
    #[serde(default)]
    pub emissive: f32,
    /// Font slot (0 = the engine's default).
    #[serde(default)]
    pub font: u8,
    /// Turn to face the camera.
    #[serde(default)]
    pub billboard: bool,
}

fn quat_id() -> Quat {
    Quat::IDENTITY
}

fn one() -> f32 {
    1.0
}

// ---------------------------------------------------------------------------------------------
// Camera, atmosphere, audio, map, host requests
// ---------------------------------------------------------------------------------------------

/// Where the game's camera is this tick (the view interpolates between ticks unless `cut`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CameraIntent {
    pub eye: Vec3,
    pub target: Vec3,
    #[serde(default = "up")]
    pub up: Vec3,
    /// Vertical field of view (degrees).
    pub fov: f32,
    #[serde(default = "near")]
    pub near: f32,
    #[serde(default = "far")]
    pub far: f32,
    /// Jump here without interpolating from the last tick (scene cuts).
    #[serde(default)]
    pub cut: bool,
    /// Cosmetic shake strength (the view adds it; it never feeds back into the sim).
    #[serde(default)]
    pub shake: f32,
}

fn up() -> Vec3 {
    Vec3::Y
}

fn near() -> f32 {
    0.1
}

fn far() -> f32 {
    1500.0
}

/// The game's atmosphere: sun, sky, fog, light levels and grading (time of day and weather).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewIntent {
    /// Direction the sunlight travels (from the sun toward the ground), colour and strength.
    pub sun_dir: Vec3,
    pub sun_color: Color,
    pub sun_intensity: f32,
    pub ambient_sky: Color,
    pub ambient_ground: Color,
    pub ambient: f32,
    pub sky: Option<SkyIntent>,
    pub fog: Option<FogIntent>,
    pub exposure: f32,
    pub bloom: f32,
    pub haze: f32,
    pub shafts: f32,
    /// Grading: warm (+) / cool (-), green (+) / magenta (-), contrast and saturation (1 = none).
    pub temperature: f32,
    pub tint: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub vignette: f32,
    /// Wet surfaces (0..1) and rain near the camera (drops per m² per s, roughly).
    pub wetness: f32,
    pub rain: f32,
    /// Wind (x, z) for foliage, rain and smoke.
    pub wind: Vec2,
    /// 0..1: how lit street lamps, signs and neon are (`Glow::Night` decor and prims), and how
    /// many facade windows are lit (`windows` material layers). They come on at dusk.
    pub street_lights: f32,
    pub windows: f32,
    /// The sun's shadows cover a square this far (m) around a point ahead of the camera.
    pub shadow_radius: f32,
    /// The picture's look over the session's view settings (None keeps the session's own):
    /// one surface style for everything, or `per_object` with `style_objects` and
    /// `style_environment` for the characters-and-objects and the environment parts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<SurfaceStyle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style_objects: Option<SurfaceStyle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style_environment: Option<SurfaceStyle>,
    /// Screen-space outlines on or off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outlines: Option<bool>,
    /// How HDR light maps onto the screen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tonemap: Option<ToneCurve>,
    /// The HDR brightness where bloom starts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bloom_threshold: Option<f32>,
    /// The height (m) over which the haze thins out above the ground.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub haze_height: Option<f32>,
    /// Screen-space bounce light and ambient occlusion (0 = off).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gi: Option<f32>,
    /// A painterly or print look over the whole picture (a loading card, an intro).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stylize: Option<StylizeIntent>,
}

/// A painterly or print look for `ViewIntent::stylize`: which, the size of its strokes, dots
/// or hatching in pixels, how much of it shows (0..1) and how much of the scene's colour it
/// keeps (0 = ink only, 1 = full colour).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct StylizeIntent {
    pub kind: StylizeKind,
    pub size: f32,
    pub mix: f32,
    pub color: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StylizeKind {
    /// Oil paint (flat strokes that keep their edges).
    Paint,
    /// Print: dot screens.
    Halftone,
    /// Pencil hatching and contours.
    Sketch,
}

/// A surface style for `ViewIntent::style`: every surface flat, cel-shaded or lit, or each as
/// its look says (`per_object`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceStyle {
    PerObject,
    Flat,
    Cel,
    Lit,
}

/// How HDR light maps onto the screen (`ViewIntent::tonemap`): clipped, a soft knee that keeps
/// authored colours, or the filmic ACES curve.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToneCurve {
    Clamp,
    SoftKnee,
    Aces,
}

impl Default for ViewIntent {
    fn default() -> Self {
        Self {
            sun_dir: Vec3::new(-0.35, -1.0, -0.55).normalize(),
            sun_color: Color([1.0, 0.95, 0.85]),
            sun_intensity: 1.0,
            ambient_sky: Color([0.52, 0.56, 0.66]),
            ambient_ground: Color([0.36, 0.33, 0.30]),
            ambient: 1.0,
            sky: None,
            fog: None,
            exposure: 1.0,
            bloom: 0.35,
            haze: 0.0,
            shafts: 1.0,
            temperature: 0.0,
            tint: 0.0,
            contrast: 1.0,
            saturation: 1.0,
            vignette: 0.0,
            wetness: 0.0,
            rain: 0.0,
            wind: Vec2::ZERO,
            street_lights: 0.0,
            windows: 0.0,
            shadow_radius: 60.0,
            style: None,
            style_objects: None,
            style_environment: None,
            outlines: None,
            tonemap: None,
            bloom_threshold: None,
            haze_height: None,
            gi: None,
            stylize: None,
        }
    }
}

/// A sky dome: gradient, sun and moon, stars, clouds and lightning. With a sky, fog fades into
/// the sky's colour in each direction (keep `FogIntent::color` the horizon's).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkyIntent {
    /// Straight up, at the horizon, and well below it.
    pub zenith: Color,
    pub horizon: Color,
    pub ground: Color,
    /// The glow around the sun (sunsets) and the sun disc's angular diameter (radians; 0 =
    /// none). The sun sits opposite `ViewIntent::sun_dir`.
    pub glow: Color,
    pub sun_size: f32,
    /// Direction toward the moon (from the viewer up into the sky). When it is also where the
    /// light comes from (`sun_dir` = -`moon_dir`, moonlight), no sun disc is drawn.
    #[serde(default)]
    pub moon_dir: Option<Vec3>,
    /// 0..1.
    #[serde(default)]
    pub stars: f32,
    #[serde(default)]
    pub clouds: f32,
    pub cloud_color: Color,
    /// A lightning flash (0..1).
    #[serde(default)]
    pub lightning: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FogIntent {
    pub color: Color,
    pub start: f32,
    pub end: f32,
}

/// What should be sounding this tick besides one-shot events: engines, sirens, beds, music.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioScene {
    pub listener: Option<Listener>,
    pub loops: Vec<LoopSource>,
    pub music: Option<Music>,
    /// Decibels the radio and the ambience beds duck by (dialogue, menus): 0 for none.
    pub ducking: f32,
    /// 0..1: reverb amount (tunnels, interiors).
    pub reverb: f32,
    /// The player's volumes (0..1): everything, sound effects (one-shots and loops) and voices
    /// (the one-shots whose cue starts `voice.`: dialogue, the scanner's babble).
    pub master: f32,
    pub effects: f32,
    pub voice: f32,
}

impl Default for AudioScene {
    fn default() -> Self {
        Self { listener: None, loops: Vec::new(), music: None, ducking: 0.0, reverb: 0.0, master: 1.0, effects: 1.0, voice: 1.0 }
    }
}

impl AudioScene {
    /// The volume a one-shot with this cue plays at: `voice` for voices, else `effects`.
    pub fn gain_for(&self, cue: &str) -> f32 {
        if cue.starts_with("voice.") { self.voice } else { self.effects }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Listener {
    pub pos: Vec3,
    pub forward: Vec3,
}

/// A continuous sound, matched across ticks by `id` (the host fades it in and out and glides
/// its parameters).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LoopSource {
    pub id: u64,
    /// A sound-bank cue (`engine.sports`, `siren.wail`, `amb.surf`).
    pub cue: String,
    /// None = not positional (beds).
    #[serde(default)]
    pub pos: Option<Vec3>,
    pub gain: f32,
    #[serde(default = "one")]
    pub pitch: f32,
    /// A cue-specific parameter (engine load, tyre slip).
    #[serde(default)]
    pub param: f32,
}

/// The radio: which station, which song and where in it (stations are live: the sim keeps
/// their clocks), and the talk on its air.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Music {
    pub station: String,
    /// The song (`pav_core::radio::song`): what the host renders and the HUD names.
    pub song: crate::radio::SongMeta,
    /// Seconds into the song's slot (past its `seconds`: the ident between songs).
    pub offset: f32,
    pub volume: f32,
    /// 0..1: heard from outside the car (low-passed).
    #[serde(default)]
    pub muffle: f32,
    /// Talk on this slot's air (`pav_core::radio::Segment`): a talk station's news, ads and
    /// reports, a DJ between songs. The host renders their voices, sounder and jingles; the game
    /// shows the words.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub segments: Vec<crate::radio::Segment>,
}

/// The map picture: RGBA rows, top row north.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// World position (x, z) of the top-left pixel's corner, and metres per pixel.
    pub origin: Vec2,
    pub metres_per_px: f32,
    /// Changes when the picture does.
    pub version: u64,
}

/// What the game asks of its host.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppRequests {
    /// Write this save (a new `serial` means a new request).
    pub save: Option<SaveRequest>,
    /// The game's settings as JSON (audio, display, controls): apply and keep them.
    pub settings: Option<SettingsRequest>,
    /// Leave the app (native builds).
    pub quit: bool,
    /// Rumble the controller (a new `serial` means a new request).
    pub vibrate: Option<Rumble>,
    /// How the keyboard and mouse stand in for the controller now (None: [`KeyMap::standard`]).
    #[serde(skip)]
    pub keys: Option<Arc<KeyMap>>,
    /// Mouse look: hide and hold the pointer while playing (released in menus).
    pub mouse_look: bool,
    /// The player's display settings (a new `serial` means new ones): apply them.
    pub display: Option<DisplayRequest>,
}

/// Display settings a game asks its host for (PLAN §5.9's Display page). Full screen and v-sync
/// are the window's (a browser page leaves them to the browser); the quality is one of the host's
/// presets; the render scale multiplies what the preset draws; `dynamic` lets the resolution follow
/// the frame rate on any device.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DisplayRequest {
    pub serial: u64,
    pub fullscreen: bool,
    pub vsync: bool,
    pub quality: DisplayQuality,
    pub render_scale: f32,
    pub dynamic: bool,
}

/// The host's quality presets a game can ask for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayQuality {
    Low,
    Medium,
    #[default]
    High,
}

/// How the keyboard and mouse stand in for the controller while this map is up; a game swaps
/// maps by context (menus, on foot, driving) so one key can mean "run" on foot and "throttle"
/// in a car. Keys are physical positions by W3C code name (`KeyW`, `Space`, `ShiftLeft`,
/// `ArrowUp`, `Escape`, `Digit1`); mouse buttons are `MouseLeft`, `MouseRight`, `MouseMiddle`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct KeyMap {
    pub name: String,
    pub binds: Vec<KeyBind>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KeyBind {
    pub key: String,
    pub to: PadTarget,
}

/// What a key does on the controller.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PadTarget {
    /// A button (an `input::pad` bit).
    Button(u32),
    /// Pushes a stick this way (x right, y forward); keys add up, clamped to length 1.
    StickL(Vec2),
    StickR(Vec2),
    /// Pulls a trigger fully.
    L2,
    R2,
}

impl KeyMap {
    pub fn new(name: &str, binds: &[(&str, PadTarget)]) -> Self {
        Self { name: name.into(), binds: binds.iter().map(|(k, t)| KeyBind { key: (*k).into(), to: *t }).collect() }
    }

    /// A generic map: WASD the left stick, arrows the D-pad, Enter and Space ✕, Escape ○, Q
    /// and E the bumpers, the mouse buttons the triggers, Tab Options. (Backspace stays the
    /// developer's rewind key.)
    pub fn standard() -> Self {
        use crate::input::pad;
        use PadTarget::*;
        Self::new(
            "standard",
            &[
                ("KeyW", StickL(Vec2::Y)),
                ("KeyS", StickL(Vec2::NEG_Y)),
                ("KeyA", StickL(Vec2::NEG_X)),
                ("KeyD", StickL(Vec2::X)),
                ("ArrowUp", Button(pad::UP)),
                ("ArrowDown", Button(pad::DOWN)),
                ("ArrowLeft", Button(pad::LEFT)),
                ("ArrowRight", Button(pad::RIGHT)),
                ("Enter", Button(pad::SOUTH)),
                ("Space", Button(pad::SOUTH)),
                ("Escape", Button(pad::EAST)),
                ("KeyF", Button(pad::NORTH)),
                ("KeyR", Button(pad::WEST)),
                ("KeyQ", Button(pad::L1)),
                ("KeyE", Button(pad::R1)),
                ("MouseRight", L2),
                ("MouseLeft", R2),
                ("Tab", Button(pad::START)),
            ],
        )
    }
}

/// Controller rumble: the low (heavy) and high (light) motors' strength (0..1), for `secs`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rumble {
    pub serial: u64,
    pub low: f32,
    pub high: f32,
    pub secs: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SaveRequest {
    pub serial: u64,
    pub slot: String,
    pub json: Arc<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SettingsRequest {
    pub serial: u64,
    pub json: Arc<String>,
}
