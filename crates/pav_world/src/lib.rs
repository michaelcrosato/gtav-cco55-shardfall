//! `pav_world`: the urban open-world kit (PLAN.md §6.6). A district file (a character grid of
//! zones and roads, zone rules, landmarks, freeways, hills) becomes a city: ground and roads
//! with sidewalks and markings, blocks split into lots, buildings by zone style, street
//! dressing, water, hills and elevated freeways, binned into the engine's 32 m streaming chunks
//! and served through `pav_core::region::RegionProvider`, plus the map picture.
//!
//! The city is a pure function of the district file and its seed: built once per scene (a few
//! hundred milliseconds), immutable, shared as `Arc<City>`; nothing of it lives in the
//! simulation's state except the chunks streaming has loaded.

use std::collections::BTreeMap;
use std::sync::Arc;

use glam::{IVec2, Vec2, Vec3};
use pav_core::present::{FarMesh, FarView, MapImage};
use pav_core::region::{RegionFill, RegionProvider};
use pav_core::statics::{Block, CHUNK_SIZE, Decor, StaticProp};
use pav_core::terrain::TerrainPatch;
use pav_core::zones::{Label, Zone};

pub mod def;
mod dress;
pub mod grid;
mod ground;
mod lots;
pub mod raster;

pub use def::{DistrictDef, RoadClass};
pub use grid::{Cell, Grid};

/// What one chunk holds.
#[derive(Clone, Debug, Default)]
pub struct Bin {
    pub blocks: Vec<Block>,
    pub decor: Vec<Decor>,
    pub meshes: Vec<Arc<TerrainPatch>>,
    pub props: Vec<StaticProp>,
    pub labels: Vec<Label>,
    pub zones: Vec<Zone>,
}

impl Bin {
    fn len(&self) -> usize {
        self.blocks.len() + self.decor.len() + self.meshes.len() + self.props.len() + self.labels.len() + self.zones.len()
    }
}

/// A building's footprint and height (the map, queries, the far view).
#[derive(Clone, Debug)]
pub struct Building {
    pub min: Vec2,
    pub max: Vec2,
    pub height: f32,
    pub zone: u16,
    /// Its main colour (linear RGB), for the far view.
    pub color: [f32; 3],
}

/// The generated city.
pub struct City {
    pub name: String,
    pub grid: Grid,
    pub def: DistrictDef,
    pub buildings: Vec<Building>,
    /// Chunk coordinate bounds, inclusive.
    pub chunk_bounds: (IVec2, IVec2),
    bins: BTreeMap<(i32, i32), Bin>,
    /// The map picture (1 px per metre).
    pub map: Arc<MapImage>,
    /// The whole city in low detail (building masses, ground, water), per chunk.
    pub far: Arc<FarView>,
}

/// Everything generated, before binning.
#[derive(Default)]
pub(crate) struct Out {
    pub blocks: Vec<Block>,
    pub decor: Vec<Decor>,
    pub meshes: Vec<Arc<TerrainPatch>>,
    pub props: Vec<StaticProp>,
    pub labels: Vec<Label>,
    pub zones: Vec<Zone>,
    pub buildings: Vec<Building>,
}

fn chunk_of(p: Vec3) -> (i32, i32) {
    ((p.x / CHUNK_SIZE).floor() as i32, (p.z / CHUNK_SIZE).floor() as i32)
}

impl City {
    /// Builds the city a district file describes.
    pub fn build(def: DistrictDef) -> Result<City, String> {
        let grid = Grid::parse(&def)?;
        let mut out = Out::default();
        ground::build(&def, &grid, &mut out);
        lots::build(&def, &grid, &mut out);
        dress::build(&def, &grid, &mut out);
        let lo = grid.origin;
        let hi = grid.origin + Vec2::new(grid.w as f32, grid.h as f32) * grid.cell;
        let chunk_bounds = (
            IVec2::new((lo.x / CHUNK_SIZE).floor() as i32, (lo.y / CHUNK_SIZE).floor() as i32),
            IVec2::new(((hi.x - 0.01) / CHUNK_SIZE).floor() as i32, ((hi.y - 0.01) / CHUNK_SIZE).floor() as i32),
        );
        let mut bins: BTreeMap<(i32, i32), Bin> = BTreeMap::new();
        for b in out.blocks {
            bins.entry(chunk_of(b.center())).or_default().blocks.push(b);
        }
        for d in out.decor {
            bins.entry(chunk_of(d.pos)).or_default().decor.push(d);
        }
        for m in out.meshes {
            let c = (m.min + m.max) * 0.5;
            bins.entry(chunk_of(c)).or_default().meshes.push(m);
        }
        for p in out.props {
            bins.entry(chunk_of(p.pos)).or_default().props.push(p);
        }
        for l in out.labels {
            bins.entry(chunk_of(l.pos)).or_default().labels.push(l);
        }
        for z in out.zones {
            bins.entry(chunk_of((z.min + z.max) * 0.5)).or_default().zones.push(z);
        }
        let map = Arc::new(raster::draw(&grid, &out.buildings, &def));
        let far = Arc::new(far_view(&grid, &def, &out.buildings));
        Ok(City { name: def.name.clone(), grid, def, buildings: out.buildings, chunk_bounds, bins, map, far })
    }

    /// Parses a district file and builds it.
    pub fn from_toml(text: &str) -> Result<City, String> {
        let def: DistrictDef = toml::from_str(text).map_err(|e| e.to_string())?;
        City::build(def)
    }

    /// How much there is: (chunks with content, items in all of them).
    pub fn stats(&self) -> (usize, usize) {
        (self.bins.len(), self.bins.values().map(|b| b.len()).sum())
    }

    /// The zone name at a world point (None on roads and outside the map).
    pub fn zone_at(&self, p: Vec2) -> Option<&str> {
        self.grid.zone(self.grid.cell_of(p))
    }

    /// The road class at a world point.
    pub fn road_at(&self, p: Vec2) -> Option<RoadClass> {
        self.grid.road(self.grid.cell_of(p))
    }

    /// The middle of the map cell `c` (x, z).
    pub fn cell_centre(&self, c: IVec2) -> Vec2 {
        self.grid.corner(c) + Vec2::splat(self.grid.cell * 0.5)
    }

    /// A chunk's content (tools and tests).
    pub fn bin(&self, x: i32, z: i32) -> Option<&Bin> {
        self.bins.get(&(x, z))
    }
}

impl RegionProvider for City {
    fn bounds(&self) -> (IVec2, IVec2) {
        self.chunk_bounds
    }

    fn fill(&self, x: i32, z: i32, out: &mut RegionFill) {
        let Some(b) = self.bins.get(&(x, z)) else { return };
        out.blocks.extend(b.blocks.iter().cloned());
        out.decor.extend(b.decor.iter().cloned());
        out.meshes.extend(b.meshes.iter().cloned());
        out.props.extend(b.props.iter().cloned());
        out.labels.extend(b.labels.iter().cloned());
        out.zones.extend(b.zones.iter().cloned());
    }
}

/// The far view: per chunk, its cells' ground (zone colour, asphalt, water) and its buildings'
/// masses as boxes.
fn far_view(grid: &Grid, def: &DistrictDef, buildings: &[Building]) -> FarView {
    let mut chunks: BTreeMap<(i32, i32), FarMesh> = BTreeMap::new();
    for z in 0..grid.h {
        for x in 0..grid.w {
            let c = IVec2::new(x, z);
            let lo = grid.corner(c);
            let hi = lo + Vec2::splat(grid.cell);
            let (y, color) = match grid.at(c).unwrap() {
                Cell::Road(RoadClass::Path) => (ground::LOT_Y, pav_core::Color::hex("#c9b797")),
                Cell::Road(_) => (ground::ROAD_Y, ground::asphalt()),
                Cell::Zone(zi) => {
                    let g = &def.zones[&grid.zones[zi as usize]].ground;
                    if g == "water" {
                        (ground::WATER_Y, pav_core::Color::hex("#2a5a74"))
                    } else {
                        (ground::LOT_Y, ground::ground_color(g))
                    }
                }
            };
            let mid = Vec3::new((lo.x + hi.x) * 0.5, 0.0, (lo.y + hi.y) * 0.5);
            let quad = [Vec3::new(lo.x, y, lo.y), Vec3::new(lo.x, y, hi.y), Vec3::new(hi.x, y, hi.y), Vec3::new(hi.x, y, lo.y)];
            chunks.entry(chunk_of(mid)).or_default().add_quad(quad, [0.0, 1.0, 0.0], color.0);
        }
    }
    for b in buildings {
        let mid = Vec3::new((b.min.x + b.max.x) * 0.5, 0.0, (b.min.y + b.max.y) * 0.5);
        let y0 = ground::LOT_Y;
        chunks.entry(chunk_of(mid)).or_default().add_box(
            Vec3::new(b.min.x, y0, b.min.y),
            Vec3::new(b.max.x, y0 + b.height, b.max.y),
            b.color,
        );
    }
    FarView { chunks: chunks.into_iter().map(|(k, m)| (k, Arc::new(m))).collect(), version: 1 }
}

/// A small deterministic random stream per (seed, purpose, place).
pub(crate) fn rng(seed: u64, salt: u64, x: i32, z: i32) -> pav_core::rng::Rng {
    pav_core::rng::Rng::new(pav_core::rng::hash3(seed ^ salt.wrapping_mul(0x9e37_79b9_7f4a_7c15), x, 0, z))
}
