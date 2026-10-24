//! The district file (PLAN.md §6.8.1): a character grid of zones and roads, the rules each zone
//! builds by, landmarks that take over map cells, elevated freeways and hills.

use std::collections::BTreeMap;

use serde::Deserialize;

fn sixteen() -> f32 {
    16.0
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DistrictDef {
    pub name: String,
    /// Metres per map character.
    #[serde(default = "sixteen")]
    pub cell: f32,
    /// World position (x, z) of the map's top-left corner (x east, z south).
    pub origin: [f32; 2],
    #[serde(default)]
    pub seed: u64,
    /// One entry per character used in `map`.
    pub legend: BTreeMap<String, Legend>,
    #[serde(default)]
    pub zones: BTreeMap<String, ZoneRules>,
    #[serde(default)]
    pub landmark: Vec<Landmark>,
    #[serde(default)]
    pub freeway: Vec<Freeway>,
    #[serde(default)]
    pub channel_ramp: Vec<ChannelRamp>,
    /// Stunt ramps and signs placed by hand.
    #[serde(default)]
    pub jump: Vec<Jump>,
    #[serde(default)]
    pub sign: Vec<Sign>,
    /// Named points for games: where scripted scenes begin, safehouse doors, spots where people
    /// pass the time.
    #[serde(default)]
    pub spot: Vec<Spot>,
    /// Facade materials by building style (`MAT/...`; `podium` for towers' podiums): the
    /// block's colour stays its base colour, and windows light up at night. A style without
    /// one draws its plain colour.
    #[serde(default)]
    pub facades: BTreeMap<String, String>,
    /// Rows of characters, north first.
    pub map: String,
    /// Ground heights, one character per map cell (`0`-`9`, then `a`-`z`), in steps of
    /// `terrain_step` metres; none: flat.
    #[serde(default)]
    pub terrain: Option<String>,
    #[serde(default = "four")]
    pub terrain_step: f32,
}

fn four() -> f32 {
    4.0
}

/// What a map character means: a zone or a road class.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Legend {
    #[serde(default)]
    pub zone: Option<String>,
    #[serde(default)]
    pub road: Option<RoadClass>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RoadClass {
    /// Narrow, no sidewalks, no markings.
    Alley,
    /// Two lanes with parking (one cell wide).
    Street,
    /// Four lanes (two cells wide when drawn two characters wide).
    Avenue,
    /// The grand strip: like an avenue, with a planted median.
    Boulevard,
    /// The storm channel: a sunken concrete bed with sloped walls.
    Channel,
    /// Footpaths, the boardwalk and the pier deck.
    Path,
}

impl RoadClass {
    pub fn name(self) -> &'static str {
        match self {
            RoadClass::Alley => "alley",
            RoadClass::Street => "street",
            RoadClass::Avenue => "avenue",
            RoadClass::Boulevard => "boulevard",
            RoadClass::Channel => "channel",
            RoadClass::Path => "path",
        }
    }

    /// Cars drive on it.
    pub fn drivable(self) -> bool {
        !matches!(self, RoadClass::Path)
    }

    /// Lanes each way and the speed limit (m/s).
    pub fn lanes(self) -> (u8, f32) {
        match self {
            RoadClass::Alley => (1, 8.0),
            RoadClass::Street => (1, 13.0),
            RoadClass::Avenue => (2, 16.0),
            RoadClass::Boulevard => (2, 16.0),
            RoadClass::Channel => (1, 20.0),
            RoadClass::Path => (0, 0.0),
        }
    }
}

/// How a zone builds.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ZoneRules {
    /// What the ground is: `concrete`, `grass`, `sand`, `water`, `asphalt`, `dirt`.
    pub ground: String,
    /// Building lots: width range (m) along the street and the deepest lot before it splits.
    pub lot: [f32; 2],
    pub depth: f32,
    /// Building heights (m) and the gap kept free at the front.
    pub height: [f32; 2],
    pub setback: f32,
    /// Building styles picked at random per lot (none: open ground).
    pub styles: Vec<String>,
    /// Street dressing: items per 100 m of curb (`lamp`, `palm`, `tree`, `bench`, `bin`,
    /// `hydrant`, `billboard`, ...).
    pub props: BTreeMap<String, f32>,
    /// Population and other per-zone tables the game reads (traffic, peds); unparsed here.
    pub population: toml::Table,
    /// Raises or lowers the zone's ground from lot level (m): -0.15 puts it at road level.
    pub level: f32,
    /// Names on the sign bands of its shops (`stucco_shop`).
    pub signs: ShopSigns,
}

impl Default for ZoneRules {
    fn default() -> Self {
        Self {
            ground: "concrete".into(),
            lot: [14.0, 28.0],
            depth: 36.0,
            height: [5.0, 9.0],
            setback: 0.0,
            styles: Vec::new(),
            props: BTreeMap::new(),
            population: toml::Table::new(),
            level: 0.0,
            signs: ShopSigns::default(),
        }
    }
}

/// Shop names for sign bands: each shop whose front faces a road of one of the `roads` classes
/// gets a name from the list (none the last few shops took, so neighbours differ); a `neon` share
/// is outlined in a tube of one of `neon_colors` that glows after dark.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ShopSigns {
    pub names: Vec<String>,
    pub roads: Vec<RoadClass>,
    pub neon: f32,
    pub neon_colors: Vec<String>,
}

/// An authored place that takes over a rectangle of map cells.
///
/// It is built either by a placeholder `assembly` (masses and its name on a sign) or, when it
/// has parts, from them. Parts are placed in the landmark's own frame: metres from the
/// north-west corner of its cells (`[x, z]`, x east, z south; rectangles `[x0, z0, x1, z1]`)
/// and heights (`y`) above its ground. Parts may reach outside its cells (a pier's stalls on
/// the boardwalk).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Landmark {
    pub key: String,
    pub name: String,
    /// Map cell rectangle, inclusive: [[x0, z0], [x1, z1]].
    pub cells: [[i32; 2]; 2],
    /// The assembly that builds it (`tower_bank`, `police_station`, `pier`, `big_wheel`...).
    #[serde(default)]
    pub assembly: String,
    /// Height for assemblies that take one (m).
    #[serde(default)]
    pub height: f32,
    /// The map icon (`bank`, `police`, `hospital`, `gun`...).
    #[serde(default)]
    pub blip: String,
    /// Level the terrain under it (a pad on a slope).
    #[serde(default)]
    pub level: bool,
    /// Paving over its whole rectangle (a colour, or a `MAT/...` material on concrete), which
    /// people walk on.
    #[serde(default)]
    pub ground: Option<String>,
    /// Crowds spawn and wander on its open ground (false: a gated yard, where people can be put
    /// but passers-by keep out).
    #[serde(default = "yes")]
    pub crowds: bool,
    /// Buildings with an inside: walls with doors and glass, a floor, a roof, lights.
    #[serde(default)]
    pub room: Vec<Room>,
    /// Solid building masses (no inside).
    #[serde(default)]
    pub mass: Vec<Mass>,
    /// Straight walls, fences and railings.
    #[serde(default)]
    pub wall: Vec<Wall>,
    /// Fences or walls round a rectangle, with gates.
    #[serde(default)]
    pub fence: Vec<Fence>,
    /// Slabs: floors, decks, platforms, counters, canopies on columns.
    #[serde(default)]
    pub slab: Vec<Slab>,
    /// Ramps cars drive up (parking decks, gangways).
    #[serde(default)]
    pub ramp: Vec<RampPart>,
    /// Library props (furniture, fittings, dressing).
    #[serde(default)]
    pub prop: Vec<PropPart>,
    /// Places where a vehicle stands parked (the game fills them: `City::lots`).
    #[serde(default)]
    pub park: Vec<Park>,
    /// Words on walls and boards, neon at night.
    #[serde(default)]
    pub sign: Vec<SignPart>,
    /// Light fittings: a glowing panel with its light.
    #[serde(default)]
    pub lamp: Vec<Lamp>,
    /// Ferris wheels: legs and deck baked in, the turning rim drawn by the game (`City::wheels`).
    #[serde(default)]
    pub wheel: Vec<WheelPart>,
    /// Ships' hulls moored in the water.
    #[serde(default)]
    pub hull: Vec<Hull>,
    /// Ladders up walls (to a roof).
    #[serde(default)]
    pub ladder: Vec<LadderPart>,
}

impl Landmark {
    /// Whether it is built from parts (rather than a placeholder assembly).
    pub fn has_parts(&self) -> bool {
        self.ground.is_some()
            || !self.room.is_empty()
            || !self.mass.is_empty()
            || !self.wall.is_empty()
            || !self.fence.is_empty()
            || !self.slab.is_empty()
            || !self.ramp.is_empty()
            || !self.prop.is_empty()
            || !self.park.is_empty()
            || !self.sign.is_empty()
            || !self.lamp.is_empty()
            || !self.wheel.is_empty()
            || !self.hull.is_empty()
            || !self.ladder.is_empty()
    }
}

fn ladder_w() -> f32 {
    0.6
}

fn ladder_color() -> String {
    "#8e9399".into()
}

/// A ladder on the `side` face of a wall (`at`: its middle on that face), from `y` up `height`
/// metres; people climb it facing the wall.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LadderPart {
    pub at: [f32; 2],
    pub side: Side,
    #[serde(default)]
    pub y: f32,
    pub height: f32,
    #[serde(default = "ladder_w")]
    pub width: f32,
    #[serde(default = "ladder_color")]
    pub color: String,
}

/// A side of a rectangle, or the way something faces.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    North,
    East,
    South,
    West,
}

impl Side {
    /// The outward direction on the ground (x, z).
    pub fn dir(self) -> glam::Vec2 {
        match self {
            Side::North => glam::Vec2::new(0.0, -1.0),
            Side::East => glam::Vec2::new(1.0, 0.0),
            Side::South => glam::Vec2::new(0.0, 1.0),
            Side::West => glam::Vec2::new(-1.0, 0.0),
        }
    }

    /// A north or south side runs along x.
    pub fn along_x(self) -> bool {
        matches!(self, Side::North | Side::South)
    }

    pub fn facing(self) -> pav_core::statics::Facing {
        match self {
            Side::North => pav_core::statics::Facing::North,
            Side::East => pav_core::statics::Facing::East,
            Side::South => pav_core::statics::Facing::South,
            Side::West => pav_core::statics::Facing::West,
        }
    }
}

fn yes() -> bool {
    true
}

fn wall_thick() -> f32 {
    0.3
}

fn cream() -> String {
    "#e9dcc4".into()
}

fn one_f() -> f32 {
    1.0
}

fn one_u() -> u32 {
    1
}

/// A building with an inside.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Room {
    /// Its outline [x0, z0, x1, z1]: the outer faces of its walls.
    pub rect: [f32; 4],
    /// The top of its roof above its floor (m).
    pub height: f32,
    /// Its floor above the landmark's ground (upper storeys, raised floors).
    #[serde(default)]
    pub y: f32,
    /// Wall thickness (m).
    #[serde(default = "wall_thick")]
    pub wall: f32,
    /// The outer walls: a colour and a material (`MAT/...`, the colour its base).
    #[serde(default = "cream")]
    pub color: String,
    #[serde(default)]
    pub material: Option<String>,
    /// The walls' inner faces and the ceiling (a lining in this colour; none: the walls').
    #[serde(default)]
    pub inside: Option<String>,
    /// The floor: a colour or a `MAT/...` (none: the ground shows).
    #[serde(default)]
    pub floor: Option<String>,
    /// The roof's colour (or `MAT/...`).
    #[serde(default)]
    pub roof: Option<String>,
    /// A parapet round the roof (m high; 0: none), in the wall's colour.
    #[serde(default)]
    pub parapet: f32,
    #[serde(default)]
    pub door: Vec<Opening>,
    #[serde(default)]
    pub glass: Vec<Glass>,
    /// Ceiling lights spread down the middle of the room.
    #[serde(default)]
    pub lights: u32,
    /// The floor is on the walk map (people can be put inside).
    #[serde(default = "yes")]
    pub walk: bool,
    /// A building on the map, in cover and the far view.
    #[serde(default = "yes")]
    pub building: bool,
}

fn door_w() -> f32 {
    1.8
}

fn door_h() -> f32 {
    2.4
}

/// An opening in a wall from the floor up (a door, a garage door, a gate in a fence): `at` is
/// its middle along the wall (local x on a north or south wall, z on an east or west one).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Opening {
    pub side: Side,
    pub at: f32,
    #[serde(default = "door_w")]
    pub width: f32,
    #[serde(default = "door_h")]
    pub height: f32,
}

fn sill() -> f32 {
    0.4
}

fn frame() -> String {
    "#2d3236".into()
}

/// Glass in a wall, from `from` to `to` along it (local x or z), between `sill` and `top` above
/// the floor, in panes framed by mullions. The panes are invisible colliders in their frames (the
/// renderer draws no transparency), so the inside shows through; a `band` (m above the floor) adds
/// a frosted strip that reads as glass. `breakable` panes are listed in `City::panes` for a game
/// to shatter.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Glass {
    pub side: Side,
    pub from: f32,
    pub to: f32,
    #[serde(default = "sill")]
    pub sill: f32,
    /// Its top (none: half a metre under the ceiling).
    #[serde(default)]
    pub top: Option<f32>,
    /// Panes across (0: about 2.4 m each).
    #[serde(default)]
    pub panes: u32,
    #[serde(default = "frame")]
    pub frame: String,
    #[serde(default)]
    pub band: f32,
    #[serde(default)]
    pub breakable: bool,
}

/// A solid building mass (or a plinth, a tower's crown).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mass {
    pub rect: [f32; 4],
    pub height: f32,
    #[serde(default)]
    pub y: f32,
    #[serde(default = "cream")]
    pub color: String,
    #[serde(default)]
    pub material: Option<String>,
    /// A cornice cap of this colour on top (none: no cap).
    #[serde(default)]
    pub cap: Option<String>,
    #[serde(default = "yes")]
    pub building: bool,
    /// Collides (false: drawn only).
    #[serde(default = "yes")]
    pub solid: bool,
}

/// What a wall line is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WallKind {
    /// A solid wall (stucco, concrete, block).
    #[default]
    Wall,
    /// Chain-link on posts with a top rail.
    Fence,
    /// A railing: posts and two rails, solid to people.
    Rail,
}

/// A gap in a wall line: its middle along the line (local x or z) and its width.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gap {
    pub at: f32,
    pub width: f32,
}

/// A straight wall, fence or railing along x or along z.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Wall {
    pub from: [f32; 2],
    pub to: [f32; 2],
    #[serde(default = "one_f")]
    pub height: f32,
    #[serde(default)]
    pub y: f32,
    #[serde(default = "wall_thick")]
    pub thick: f32,
    #[serde(default)]
    pub kind: WallKind,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub material: Option<String>,
    #[serde(default)]
    pub gap: Vec<Gap>,
    /// People take cover behind it (low cover under 1.3 m).
    #[serde(default)]
    pub cover: bool,
}

fn fence_h() -> f32 {
    2.4
}

fn fence_kind() -> WallKind {
    WallKind::Fence
}

/// A fence (or a wall) round a rectangle, with gates (openings from the ground up).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fence {
    pub rect: [f32; 4],
    #[serde(default = "fence_h")]
    pub height: f32,
    #[serde(default = "fence_kind")]
    pub kind: WallKind,
    #[serde(default)]
    pub thick: Option<f32>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub material: Option<String>,
    #[serde(default)]
    pub gate: Vec<Opening>,
}

fn slab_thick() -> f32 {
    0.2
}

fn concrete() -> String {
    "#a7a39a".into()
}

/// A flat slab: a floor, a deck, a platform, a counter, a canopy roof (on `columns`).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Slab {
    pub rect: [f32; 4],
    /// Its bottom above the landmark's ground.
    #[serde(default)]
    pub y: f32,
    #[serde(default = "slab_thick")]
    pub thick: f32,
    #[serde(default = "concrete")]
    pub color: String,
    #[serde(default)]
    pub material: Option<String>,
    #[serde(default = "yes")]
    pub solid: bool,
    /// People walk on it (its tiles on the walk map).
    #[serde(default)]
    pub walk: bool,
    /// Columns down to the ground, about this far apart round its edge (0: none).
    #[serde(default)]
    pub columns: f32,
    /// The columns' colour (none: the slab's).
    #[serde(default)]
    pub column_color: Option<String>,
    /// Piles from under it down to the sea bed, about this far apart round its edge (a pier's
    /// deck; 0: none).
    #[serde(default)]
    pub piles: f32,
    /// Glows (an emissive canopy underside, signs; 0: none), after dark only when `night`.
    #[serde(default)]
    pub glow: f32,
    #[serde(default)]
    pub night: bool,
}

/// A ramp a car drives up: its rectangle climbs toward `rise` from `y0` to `y1` (heights of its
/// surface above the landmark's ground), with kerbs along its sides; a solid wedge standing on
/// its low end's level (a deck's ramp stands on that deck).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RampPart {
    pub rect: [f32; 4],
    pub rise: Side,
    pub y0: f32,
    pub y1: f32,
    #[serde(default = "concrete")]
    pub color: String,
    #[serde(default = "yes")]
    pub kerbs: bool,
}

/// A library prop: `yaw` is the way its front (+z) faces, degrees from north toward east; a row
/// of `count` copies `step` apart.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropPart {
    pub asset: String,
    pub at: [f32; 2],
    #[serde(default)]
    pub y: f32,
    #[serde(default)]
    pub yaw: f32,
    #[serde(default = "one_f")]
    pub scale: f32,
    #[serde(default)]
    pub seed: u32,
    #[serde(default = "yes")]
    pub solid: bool,
    #[serde(default = "one_u")]
    pub count: u32,
    #[serde(default)]
    pub step: [f32; 2],
    /// People take cover behind it.
    #[serde(default)]
    pub cover: bool,
}

/// Where a vehicle stands parked, facing `yaw` (degrees from north): `kind` names what the game
/// puts there (a vehicle archetype; empty: any), `always` keeps one there (else the game's share
/// of its lot places); a row of `count` places `step` apart.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Park {
    pub at: [f32; 2],
    #[serde(default)]
    pub y: f32,
    #[serde(default)]
    pub yaw: f32,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub always: bool,
    #[serde(default = "one_u")]
    pub count: u32,
    #[serde(default)]
    pub step: [f32; 2],
}

fn sign_color() -> String {
    "#f4f1ea".into()
}

/// Words on a wall facing `facing`, centred at `at` (on the wall's face) and `y` (the letters'
/// middle), on an optional `board`; `neon` outlines the board in a tube that glows after dark.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignPart {
    pub text: String,
    pub at: [f32; 2],
    pub y: f32,
    pub facing: Side,
    #[serde(default = "one_f")]
    pub size: f32,
    #[serde(default = "sign_color")]
    pub color: String,
    #[serde(default)]
    pub board: Option<String>,
    #[serde(default)]
    pub neon: Option<String>,
    /// A pole sign standing free (posts down to the ground under the board).
    #[serde(default)]
    pub pole: bool,
}

fn lamp_color() -> String {
    "#fff0d0".into()
}

fn lamp_size() -> [f32; 2] {
    [1.2, 0.5]
}

fn lamp_glow() -> f32 {
    1.0
}

/// A light fitting: a glowing panel facing down at `y` (its underside), `size` [x, z] metres; a
/// `glow` above 0.5 also lights the room round it (about 4 + 2 × glow metres). After dark only
/// when `night`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lamp {
    pub at: [f32; 2],
    pub y: f32,
    #[serde(default = "lamp_color")]
    pub color: String,
    #[serde(default = "lamp_size")]
    pub size: [f32; 2],
    #[serde(default = "lamp_glow")]
    pub glow: f32,
    #[serde(default)]
    pub night: bool,
}

/// A Ferris wheel: its hub `hub` metres up over `at`, its face looking along `yaw` (degrees from
/// north), `radius` to the cars' pivots, `cars` cars, a turn every `period` seconds. Its legs and
/// boarding deck are baked in; the rim (`rim`, a library asset with its hub at `rim_hub` metres
/// above its origin) and the cars (`car`, hanging from `car_pivot` metres above their origin)
/// are drawn by the game from `City::wheels`, turned by the clock.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WheelPart {
    pub at: [f32; 2],
    pub hub: f32,
    pub radius: f32,
    #[serde(default)]
    pub yaw: f32,
    pub cars: u32,
    pub period: f32,
    pub rim: String,
    pub rim_hub: f32,
    pub car: String,
    pub car_pivot: f32,
    /// Half the axle's length: where the legs meet it, each side of the wheel.
    #[serde(default = "wheel_half_axle")]
    pub axle: f32,
    #[serde(default = "wheel_legs")]
    pub color: String,
}

fn wheel_half_axle() -> f32 {
    2.4
}

fn wheel_legs() -> String {
    "#e8e4dc".into()
}

/// A ship's hull moored in the water: its outline, the end its bow points to, its deck's height
/// above the landmark's ground, its colours; a superstructure (`house`, a rectangle on the deck,
/// `house_height` tall) at the stern.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hull {
    pub rect: [f32; 4],
    pub bow: Side,
    pub deck: f32,
    #[serde(default = "hull_color")]
    pub color: String,
    #[serde(default = "hull_band")]
    pub band: String,
    #[serde(default = "hull_deck")]
    pub deck_color: String,
    #[serde(default)]
    pub house: Option<[f32; 4]>,
    #[serde(default)]
    pub house_height: f32,
}

fn hull_color() -> String {
    "#7a2a24".into()
}

fn hull_band() -> String {
    "#20242a".into()
}

fn hull_deck() -> String {
    "#4f6b52".into()
}

/// An elevated freeway alignment.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Freeway {
    pub key: String,
    #[serde(default)]
    pub name: String,
    /// Deck height above the ground (m).
    pub height: f32,
    /// Lanes each way.
    pub lanes: u8,
    /// Centre line in map cell coordinates (fractions allowed), smoothed.
    pub points: Vec<[f32; 2]>,
    #[serde(default)]
    pub ramps: Vec<Ramp>,
    /// A barrier closes the deck at its first point (where it leaves the map).
    #[serde(default)]
    pub closed_start: bool,
    /// The last stretch ends in the air (the broken construction ramp).
    #[serde(default)]
    pub broken_end: bool,
}

/// Which way traffic uses a ramp.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RampKind {
    /// From the deck down to the street.
    Off,
    /// From the street up to the deck.
    On,
}

/// A ramp between a freeway deck and a street.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ramp {
    /// Its end beside the deck and its end on the street (map cell coordinates).
    pub from: [f32; 2],
    pub to: [f32; 2],
    pub kind: RampKind,
}

/// A ramp down a storm channel's wall to its bed.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelRamp {
    /// A column of the channel (its cells run north-south).
    pub col: i32,
    /// The wall it hugs: `west` or `east`.
    pub side: String,
    /// Rows (map cell coordinates, fractions allowed) of its top end and its bottom end.
    pub rows: [f32; 2],
}

/// A kicker ramp for stunt jumps: a wedge rising along `yaw` (degrees, 0 = north, 90 = east).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Jump {
    /// Map cell coordinates of its middle (fractions allowed).
    pub at: [f32; 2],
    pub yaw: f32,
    #[serde(default = "jump_len")]
    pub length: f32,
    #[serde(default = "jump_width")]
    pub width: f32,
    #[serde(default = "jump_height")]
    pub height: f32,
    #[serde(default)]
    pub name: String,
}

fn jump_len() -> f32 {
    12.0
}

fn jump_width() -> f32 {
    5.0
}

fn jump_height() -> f32 {
    2.0
}

/// A named point (map cell coordinates, fractions allowed) and a facing (degrees, 0 = north,
/// 90 = east).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spot {
    pub name: String,
    pub at: [f32; 2],
    #[serde(default)]
    pub yaw: f32,
    /// Metres above the ground there: a place on a deck or a roof (people and vehicles stand
    /// exactly there).
    #[serde(default)]
    pub y: f32,
}

/// Words on a post (the developer district's station signs, street furniture text).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sign {
    pub at: [f32; 2],
    pub text: String,
    #[serde(default = "sign_height")]
    pub height: f32,
    #[serde(default = "sign_size")]
    pub size: f32,
}

fn sign_height() -> f32 {
    3.2
}

fn sign_size() -> f32 {
    0.9
}
