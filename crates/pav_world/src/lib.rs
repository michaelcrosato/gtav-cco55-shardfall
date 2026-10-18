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

pub mod channel;
pub mod cover;
pub mod def;
pub mod dress;
mod edge;
mod extras;
pub mod freeway;
pub mod grid;
pub mod ground;
pub mod lanes;
mod lots;
pub mod raster;
pub mod terrain;
pub mod walk;

pub use def::{DistrictDef, RoadClass};
pub use grid::{Cell, Grid};
pub use terrain::Terrain;

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
    /// Floor height (above the ground on slopes) and height above it.
    pub base: f32,
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
    pub terrain: Terrain,
    pub buildings: Vec<Building>,
    /// Elevated freeways (deck centre lines and ramps).
    pub freeways: Vec<freeway::FreewayGeom>,
    /// Storm channels (The Ditch).
    pub channels: Vec<channel::ChannelGeom>,
    /// Every drivable lane, junctions with their signals, parking spots.
    pub lanes: lanes::LaneGraph,
    /// Traffic light heads (their lamps are drawn live from the signal plans).
    pub signal_heads: Vec<dress::SignalHead>,
    /// Where people walk (2 m tiles) and where they take cover.
    pub walk: walk::WalkMap,
    pub cover: cover::Cover,
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
        let terrain = Terrain::build(&def, &grid)?;
        let mut out = Out::default();
        ground::build(&def, &grid, &terrain, &mut out);
        ground::terrain_meshes(&def, &grid, &terrain, &mut out);
        let channels = channel::build(&def, &grid, &terrain, &mut out);
        extras::build(&def, &grid, &terrain, &mut out);
        lots::build(&def, &grid, &terrain, &mut out);
        dress::build(&def, &grid, &terrain, &mut out);
        dress::docks(&def, &grid, &mut out);
        dress::beach(&def, &grid, &mut out);
        let freeways = freeway::build(&def, &grid, &terrain, &mut out);
        edge::build(&def, &grid, &terrain, &mut out);
        let lanes = lanes::LaneGraph::build(&grid, &terrain, &freeways, &channels, def.seed);
        let signal_heads = dress::signals(&grid, &lanes, &mut out);
        let walk = walk::WalkMap::build(&def, &grid, &out.buildings, &lanes);
        let cover = cover::Cover::build(&out.buildings, &walk, &channels, &freeways);
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
        let map = Arc::new(raster::draw(&grid, &terrain, &out.buildings, &freeways, &def));
        let far = Arc::new(far_view(&grid, &terrain, &def, &out.buildings, &freeways));
        Ok(City {
            name: def.name.clone(),
            grid,
            def,
            terrain,
            buildings: out.buildings,
            freeways,
            channels,
            lanes,
            signal_heads,
            walk,
            cover,
            chunk_bounds,
            bins,
            map,
            far,
        })
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

    /// The ground's height at a world point: the asphalt on roads, the lot level elsewhere, the
    /// surface on water (terrain included; not decks or roofs).
    pub fn ground_y(&self, p: Vec2) -> f32 {
        ground_at(&self.def, &self.grid, &self.terrain, p)
    }

    /// Whether a world point lies inside a building's footprint (grown by `margin` metres).
    pub fn in_building(&self, p: Vec2, margin: f32) -> bool {
        self.buildings
            .iter()
            .any(|b| p.x > b.min.x - margin && p.x < b.max.x + margin && p.y > b.min.y - margin && p.y < b.max.y + margin)
    }

    /// The highest roof within `r` metres of a world point, or the ground there when no building
    /// stands that close (what a helicopter flies over).
    pub fn roof_y(&self, p: Vec2, r: f32) -> f32 {
        self.buildings
            .iter()
            .filter(|b| p.clamp(b.min, b.max).distance_squared(p) < r * r)
            .map(|b| b.base + b.height)
            .fold(self.ground_y(p), f32::max)
    }

    /// The middle of the map cell `c` (x, z).
    pub fn cell_centre(&self, c: IVec2) -> Vec2 {
        self.grid.corner(c) + Vec2::splat(self.grid.cell * 0.5)
    }

    /// A chunk's content (tools and tests).
    pub fn bin(&self, x: i32, z: i32) -> Option<&Bin> {
        self.bins.get(&(x, z))
    }

    /// The dressing props within `r` metres of `p` (a game's scenario points: benches, bus
    /// stops), nearest first.
    pub fn props_near(&self, p: Vec3, r: f32) -> Vec<&StaticProp> {
        let (x0, z0) = chunk_of(p - Vec3::splat(r));
        let (x1, z1) = chunk_of(p + Vec3::splat(r));
        let mut out = Vec::new();
        for z in z0..=z1 {
            for x in x0..=x1 {
                if let Some(b) = self.bins.get(&(x, z)) {
                    out.extend(b.props.iter().filter(|s| s.pos.distance(p) <= r));
                }
            }
        }
        out.sort_by(|a, b| a.pos.distance(p).total_cmp(&b.pos.distance(p)));
        out
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

/// The far view: per chunk, its cells' ground (zone colour, asphalt, water; terrain at 8 m
/// detail) and its buildings' masses as boxes; around the map, the backdrop hills and the ocean.
fn far_view(
    grid: &Grid,
    terrain: &Terrain,
    def: &DistrictDef,
    buildings: &[Building],
    freeways: &[freeway::FreewayGeom],
) -> FarView {
    let mut chunks: BTreeMap<(i32, i32), FarMesh> = BTreeMap::new();
    for z in 0..grid.h {
        for x in 0..grid.w {
            let c = IVec2::new(x, z);
            let lo = grid.corner(c);
            let hi = lo + Vec2::splat(grid.cell);
            let mid = Vec3::new((lo.x + hi.x) * 0.5, 0.0, (lo.y + hi.y) * 0.5);
            let mesh = chunks.entry(chunk_of(mid)).or_default();
            let cell = grid.at(c).unwrap();
            if terrain.is_terrain(c) {
                let (y0, kind) = match cell {
                    Cell::Road(_) => (ground::ROAD_Y, None),
                    Cell::Zone(zi) => (ground::LOT_Y, Some(def.zones[&grid.zones[zi as usize]].ground.as_str())),
                };
                let half = terrain::PER / 2;
                for j in 0..2 {
                    for i in 0..2 {
                        let s = [(0, 0), (0, 1), (1, 1), (1, 0)]
                            .map(|(a, b)| (x * terrain::PER + (i + a) * half, z * terrain::PER + (j + b) * half));
                        let v = s.map(|(sx, sz)| {
                            let q = terrain.sample_pos(sx, sz);
                            Vec3::new(q.x, y0 + terrain.sample(sx, sz), q.y)
                        });
                        let n = s.map(|(sx, sz)| terrain.normal(sx, sz));
                        let col = [0, 1, 2, 3].map(|k| match kind {
                            Some(kind) => ground::ground_tint(def.seed, kind, Vec2::new(v[k].x, v[k].z), n[k]),
                            None => ground::asphalt().0,
                        });
                        far_quad(mesh, v, n, col);
                    }
                }
                continue;
            }
            let (y, color) = match cell {
                Cell::Road(RoadClass::Path) => (ground::LOT_Y, pav_core::Color::hex("#c9b797")),
                Cell::Road(_) => (ground::ROAD_Y, ground::asphalt()),
                Cell::Zone(zi) => {
                    let g = &def.zones[&grid.zones[zi as usize]].ground;
                    if g == "water" {
                        (ground::WATER_Y, pav_core::Color::hex("#2a5a74"))
                    } else {
                        (ground::LOT_Y + def.zones[&grid.zones[zi as usize]].level, ground::ground_color(g))
                    }
                }
            };
            let quad = [Vec3::new(lo.x, y, lo.y), Vec3::new(lo.x, y, hi.y), Vec3::new(hi.x, y, hi.y), Vec3::new(hi.x, y, lo.y)];
            mesh.add_quad(quad, [0.0, 1.0, 0.0], color.0);
        }
    }
    for b in buildings {
        let mid = Vec3::new((b.min.x + b.max.x) * 0.5, 0.0, (b.min.y + b.max.y) * 0.5);
        chunks.entry(chunk_of(mid)).or_default().add_box(
            Vec3::new(b.min.x, b.base - 0.5, b.min.y),
            Vec3::new(b.max.x, b.base + b.height, b.max.y),
            b.color,
        );
    }
    for f in freeways {
        let deck = pav_core::Color::hex("#9c978c").0;
        let strip = |p: &freeway::Path, half: f32, chunks: &mut BTreeMap<(i32, i32), FarMesh>| {
            for i in 0..p.len().saturating_sub(1) {
                let m = (p.points[i] + p.points[i + 1]) * 0.5;
                let mesh = chunks.entry(chunk_of(m)).or_default();
                let top = [p.at(i, -half, 0.85), p.at(i + 1, -half, 0.85), p.at(i + 1, half, 0.85), p.at(i, half, 0.85)];
                far_quad(mesh, top, [Vec3::Y; 4], [deck; 4]);
                for s in [-half, half] {
                    let side = [p.at(i, s, 0.85), p.at(i + 1, s, 0.85), p.at(i + 1, s, -1.25), p.at(i, s, -1.25)];
                    let n = p.right[i] * s.signum();
                    far_side(mesh, side, n, deck);
                }
            }
        };
        strip(&f.deck, freeway::HALF, &mut chunks);
        for r in &f.ramps {
            strip(&r.path, freeway::RAMP_HALF, &mut chunks);
        }
    }
    backdrop(grid, terrain, def, &mut chunks);
    FarView { chunks: chunks.into_iter().map(|(k, m)| (k, Arc::new(m))).collect(), version: 1 }
}

/// Adds a quad with per-vertex normals and colours (wound to face up).
fn far_quad(mesh: &mut FarMesh, v: [Vec3; 4], n: [Vec3; 4], c: [[f32; 3]; 4]) {
    let base = mesh.positions.len() as u32;
    for k in 0..4 {
        mesh.positions.push(v[k].to_array());
        mesh.normals.push(n[k].to_array());
        mesh.colors.push(c[k]);
        mesh.min = if base == 0 && k == 0 { v[k] } else { mesh.min.min(v[k]) };
        mesh.max = if base == 0 && k == 0 { v[k] } else { mesh.max.max(v[k]) };
    }
    if (v[1] - v[0]).cross(v[2] - v[0]).y >= 0.0 {
        mesh.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    } else {
        mesh.indices.extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
    }
}

/// Adds a vertical quad facing `n`.
fn far_side(mesh: &mut FarMesh, v: [Vec3; 4], n: Vec3, c: [f32; 3]) {
    let base = mesh.positions.len() as u32;
    for p in v {
        mesh.positions.push(p.to_array());
        mesh.normals.push(n.to_array());
        mesh.colors.push(c);
        mesh.min = if base == 0 && mesh.positions.len() == 1 { p } else { mesh.min.min(p) };
        mesh.max = if base == 0 && mesh.positions.len() == 1 { p } else { mesh.max.max(p) };
    }
    if (v[1] - v[0]).cross(v[2] - v[0]).dot(n) >= 0.0 {
        mesh.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    } else {
        mesh.indices.extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
    }
}

/// Beyond the map: hills rising behind the north edge (a pass where the freeway corridor leaves),
/// low hills to the east, and the ocean to the south and west, out to the horizon. Each piece is
/// a mesh under a chunk key outside the streaming bounds, so it is always drawn from afar.
fn backdrop(grid: &Grid, terrain: &Terrain, def: &DistrictDef, chunks: &mut BTreeMap<(i32, i32), FarMesh>) {
    let lo = grid.origin;
    let hi = grid.origin + Vec2::new(grid.w as f32, grid.h as f32) * grid.cell;
    let corridor = def.freeway.first().and_then(|f| f.points.first()).map(|p| lo.x + p[0] * grid.cell).unwrap_or(hi.x - 40.0);
    // Height of the backdrop at a point outside the map.
    let height = |p: Vec2| -> f32 {
        let north = (lo.y - p.y).max(0.0);
        let east = (p.x - hi.x).max(0.0);
        let west = (lo.x - p.x).max(0.0);
        let edge = terrain.height_at(Vec2::new(p.x.clamp(lo.x, hi.x), lo.y));
        let pass = ((p.x - corridor).abs() / 140.0).clamp(0.0, 1.0);
        let pass = pass * pass * (3.0 - 2.0 * pass);
        let ridge = ground::noise(def.seed ^ 0xbac, p, 260.0) * 38.0 + ground::noise(def.seed ^ 0xb0b, p, 90.0) * 12.0;
        let n = if p.y <= lo.y {
            (edge + 170.0 * (1.0 - (-north / 420.0).exp()) + ridge * (north / 300.0).min(1.0)) * pass.max(0.15)
        } else {
            0.0
        };
        let e = if east > 0.0 && p.y < hi.y - 300.0 {
            (60.0 * (1.0 - (-east / 300.0).exp()) + ridge * 0.5).max(0.0) * pass.max(0.2)
        } else {
            0.0
        };
        // The hills run on west of the map along the coast, falling into the sea.
        let w = if west > 0.0 && p.y < lo.y + 240.0 {
            (edge * 0.8 + 40.0) * (1.0 - (west / 600.0).min(1.0)) * (1.0 - ((p.y - lo.y) / 240.0).clamp(0.0, 1.0))
        } else {
            0.0
        };
        n.max(e).max(w).max(0.0)
    };
    // Rows and columns land on the map's edges (16 and 18 steps across it).
    let step = 64.0;
    let ext = 1664.0;
    let mut land = FarMesh::default();
    let (x0, x1) = (lo.x - ext, hi.x + ext);
    let (z0, z1) = (lo.y - ext, hi.y);
    let nx = ((x1 - x0) / step) as i32;
    let nz = ((z1 - z0) / step) as i32;
    let pos = |i: i32, j: i32| Vec2::new(x0 + i as f32 * step, z0 + j as f32 * step);
    let inside = |p: Vec2| p.x > lo.x + 0.1 && p.x < hi.x - 0.1 && p.y > lo.y + 0.1 && p.y < hi.y - 0.1;
    for j in 0..nz {
        for i in 0..nx {
            let q = [pos(i, j), pos(i, j + 1), pos(i + 1, j + 1), pos(i + 1, j)];
            let centre = (q[0] + q[2]) * 0.5;
            if inside(centre) {
                continue;
            }
            let hs = q.map(height);
            if hs.iter().all(|h| *h < 0.5) {
                continue;
            }
            let v = [0, 1, 2, 3].map(|k| Vec3::new(q[k].x, ground::LOT_Y + hs[k] - 0.3, q[k].y));
            let nrm = |p: Vec2| {
                let e = 6.0;
                let dx = (height(p + Vec2::X * e) - height(p - Vec2::X * e)) / (2.0 * e);
                let dz = (height(p + Vec2::Y * e) - height(p - Vec2::Y * e)) / (2.0 * e);
                Vec3::new(-dx, 1.0, -dz).normalize()
            };
            let n = q.map(nrm);
            let col = [0, 1, 2, 3].map(|k| ground::ground_tint(def.seed, "brush", q[k], n[k]));
            far_quad(&mut land, v, n, col);
        }
    }
    chunks.insert((-10_000, -10_000), land);
    // The ocean: south of the map and west of it below the hills, to the horizon.
    let mut sea = FarMesh::default();
    let ocean = pav_core::Color::hex("#1f5f78").0;
    let y = ground::WATER_Y;
    let far = 6000.0;
    let rects = [
        (Vec2::new(lo.x - far, hi.y), Vec2::new(hi.x + far, hi.y + far)),
        (Vec2::new(lo.x - far, lo.y + 200.0), Vec2::new(lo.x, hi.y)),
        (Vec2::new(hi.x, hi.y - 160.0), Vec2::new(hi.x + far, hi.y)),
    ];
    for (a, b) in rects {
        let v = [Vec3::new(a.x, y, a.y), Vec3::new(a.x, y, b.y), Vec3::new(b.x, y, b.y), Vec3::new(b.x, y, a.y)];
        far_quad(&mut sea, v, [Vec3::Y; 4], [ocean; 4]);
    }
    chunks.insert((-10_001, -10_001), sea);
}

/// The ground's height at a world point (see `City::ground_y`).
pub(crate) fn ground_at(def: &DistrictDef, grid: &Grid, terrain: &Terrain, p: Vec2) -> f32 {
    let h = terrain.height_at(p);
    match grid.at(grid.cell_of(p)) {
        Some(Cell::Road(RoadClass::Path)) => ground::LOT_Y + h,
        Some(Cell::Road(_)) => ground::ROAD_Y + h,
        Some(Cell::Zone(z)) => {
            let rules = &def.zones[&grid.zones[z as usize]];
            if rules.ground == "water" { ground::WATER_Y } else { ground::LOT_Y + rules.level + h }
        }
        None => ground::LOT_Y + h,
    }
}

/// A small deterministic random stream per (seed, purpose, place).
pub(crate) fn rng(seed: u64, salt: u64, x: i32, z: i32) -> pav_core::rng::Rng {
    pav_core::rng::Rng::new(pav_core::rng::hash3(seed ^ salt.wrapping_mul(0x9e37_79b9_7f4a_7c15), x, 0, z))
}
