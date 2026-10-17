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
    pub hill: Vec<Hill>,
    /// Rows of characters, north first.
    pub map: String,
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
        }
    }
}

/// An authored place that takes over a rectangle of map cells.
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
}

/// An elevated freeway alignment.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Freeway {
    pub key: String,
    /// Deck height above the ground (m).
    pub height: f32,
    /// Lanes each way.
    pub lanes: u8,
    /// Centre line in map cell coordinates (fractions allowed), smoothed.
    pub points: Vec<[f32; 2]>,
    #[serde(default)]
    pub ramps: Vec<Ramp>,
    /// The last stretch ends in the air (the broken construction ramp).
    #[serde(default)]
    pub broken_end: bool,
}

/// A ramp from a freeway down to a street.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ramp {
    /// Where it leaves the freeway and where it lands (map cell coordinates).
    pub from: [f32; 2],
    pub to: [f32; 2],
}

/// Raised ground.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hill {
    /// Map cell rectangle, inclusive.
    pub cells: [[i32; 2]; 2],
    /// Height at the top (m) and how many cells it takes to fall back to 0.
    pub height: f32,
    #[serde(default = "three")]
    pub falloff: f32,
}

fn three() -> f32 {
    3.0
}
