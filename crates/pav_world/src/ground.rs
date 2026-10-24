//! The ground: road surfaces with sidewalks on their edges (a 16 m street is sidewalk 2 m,
//! parking 2.4 m, two 3.6 m lanes, parking, sidewalk), lane markings, crosswalks and stop lines,
//! the ground of every zone at lot level (15 cm above the asphalt: the kerb), water with a sea bed
//! and a swimming zone. Cells with terrain height are triangle meshes instead (`terrain_meshes`).

use std::collections::BTreeMap;
use std::sync::Arc;

use glam::{IVec2, Vec2, Vec3};
use pav_core::Color;
use pav_core::statics::{Block, CHUNK_SIZE, block_flags};
use pav_core::terrain::TerrainPatch;
use pav_core::zones::{Zone, ZoneKind};

use crate::Out;
use crate::def::{DistrictDef, RoadClass};
use crate::grid::{Cell, DIRS, Grid};
use crate::terrain::{PER, Terrain};

/// The top of the asphalt and of sidewalks and lots (the kerb is the step between them).
pub const ROAD_Y: f32 = 0.0;
pub const LOT_Y: f32 = 0.15;
/// Sidewalk width on a road's edge.
pub const SIDEWALK: f32 = 2.0;
/// The water's surface and the sea bed.
pub const WATER_Y: f32 = -0.6;
pub const SEABED_Y: f32 = -4.0;

pub fn asphalt() -> Color {
    Color::hex("#2c2e33")
}

fn sidewalk() -> Color {
    Color::hex("#b4afa5")
}

/// The colour of a zone's ground kind.
pub fn ground_color(kind: &str) -> Color {
    Color::hex(match kind {
        "grass" => "#5e7c40",
        "sand" => "#dbc79c",
        "dirt" => "#8c775a",
        "asphalt" => "#34363b",
        "plaza" => "#c4baa8",
        "water" => "#23404a",
        "brush" => "#8b8a5e",
        "dry_grass" => "#a59a63",
        _ => "#a39e94",
    })
}

fn slab(min: Vec2, max: Vec2, y0: f32, y1: f32, color: Color) -> Block {
    Block::new(Vec3::new(min.x, y0, min.y), Vec3::new(max.x, y1, max.y), color)
}

/// A flat painted mark lying on the asphalt (no collider).
fn paint(min: Vec2, max: Vec2, color: Color) -> Block {
    slab(min, max, ROAD_Y, ROAD_Y + 0.012, color).with_flags(block_flags::GHOST)
}

/// Storm channels are their own network below the streets: a road meets only roads of its kind.
fn same_level(a: RoadClass, b: RoadClass) -> bool {
    (a == RoadClass::Channel) == (b == RoadClass::Channel)
}

/// How many drivable road cells run through `c` along `d` (both ways, itself included).
fn run(grid: &Grid, c: IVec2, d: IVec2) -> i32 {
    let Some(me) = grid.road(c) else { return 1 };
    let mut n = 1;
    for s in [d, -d] {
        let mut p = c + s;
        while grid.road(p).is_some_and(|r| r.drivable() && same_level(r, me)) {
            n += 1;
            p += s;
        }
    }
    n
}

/// A road cell's role: a straight band (horizontal or vertical) of some thickness, or a junction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Along x (east-west); `k` cells thick, this one `i`-th from the north.
    Ew {
        k: i32,
        i: i32,
    },
    /// Along z (north-south); `k` cells thick, `i`-th from the west.
    Ns {
        k: i32,
        i: i32,
    },
    Junction,
    /// A dead end or a lone cell.
    Stub,
}

pub fn role(grid: &Grid, c: IVec2) -> Role {
    let me = grid.road(c).unwrap_or(RoadClass::Street);
    let ew = run(grid, c, IVec2::X);
    let ns = run(grid, c, IVec2::Y);
    // A band at most 2 cells thick; longer runs both ways make a junction. A two-cell band
    // needs its other cell to run the same way: a single cell sticking out sideways (a stub or a
    // driveway meeting a street) makes a junction instead.
    let along_x = ew >= 3 || (ew == 2 && ns == 1);
    let along_z = ns >= 3 || (ns == 2 && ew == 1);
    let beside = |d: IVec2, axis: IVec2| {
        grid.road(c + d).is_some_and(|r| r.drivable() && same_level(r, me)) && run(grid, c + d, axis) >= 2
    };
    let thick_x = ns == 1 || beside(IVec2::Y, IVec2::X) || beside(-IVec2::Y, IVec2::X);
    let thick_z = ew == 1 || beside(IVec2::X, IVec2::Y) || beside(-IVec2::X, IVec2::Y);
    match (along_x && ns <= 2 && thick_x, along_z && ew <= 2 && thick_z) {
        (true, false) => {
            let i = (1..=2)
                .take_while(|s| grid.road(c - IVec2::Y * *s).is_some_and(|r| r.drivable() && same_level(r, me)))
                .count() as i32;
            Role::Ew { k: ns, i }
        }
        (false, true) => {
            let i = (1..=2)
                .take_while(|s| grid.road(c - IVec2::X * *s).is_some_and(|r| r.drivable() && same_level(r, me)))
                .count() as i32;
            Role::Ns { k: ew, i }
        }
        (true, true) if ns <= 2 && ew <= 2 => Role::Stub,
        _ if ew >= 2 && ns >= 2 => Role::Junction,
        _ => Role::Stub,
    }
}

pub(crate) fn build(def: &DistrictDef, grid: &Grid, terrain: &Terrain, out: &mut Out) {
    let cs = grid.cell;
    let white = Color::hex("#e8e5da");
    let yellow = Color::hex("#e2b53c");
    for z in 0..grid.h {
        for x in 0..grid.w {
            let c = IVec2::new(x, z);
            if terrain.is_terrain(c) {
                continue;
            }
            let lo = grid.corner(c);
            let hi = lo + Vec2::splat(cs);
            match grid.at(c).unwrap() {
                Cell::Zone(zi) => {
                    let name = &grid.zones[zi as usize];
                    let rules = &def.zones[name];
                    if rules.ground == "water" {
                        out.blocks.push(slab(lo, hi, SEABED_Y - 1.0, SEABED_Y, ground_color("water")));
                        out.zones.push(Zone::new(
                            Vec3::new(lo.x, SEABED_Y, lo.y),
                            Vec3::new(hi.x, WATER_Y, hi.y),
                            ZoneKind::Water,
                        ));
                        // A sea wall where the water meets land.
                        for (i, d) in DIRS.iter().enumerate() {
                            if grid.zone(c + *d).is_some_and(|n| def.zones[n].ground != "water") || grid.road(c + *d).is_some() {
                                let (a, b) = edge(lo, hi, i, 0.6);
                                out.blocks.push(slab(a, b, SEABED_Y, LOT_Y, Color::hex("#8f8a80")));
                            }
                        }
                    } else {
                        out.blocks.push(slab(lo, hi, -0.6, LOT_Y + rules.level, ground_color(&rules.ground)));
                    }
                }
                Cell::Road(RoadClass::Channel) => {}
                Cell::Road(class) => road_cell(grid, c, class, lo, hi, white, yellow, out),
            }
        }
    }
}

/// The strip along side `i` (N, E, S, W) of a cell, `w` wide.
fn edge(lo: Vec2, hi: Vec2, i: usize, w: f32) -> (Vec2, Vec2) {
    match i {
        0 => (lo, Vec2::new(hi.x, lo.y + w)),
        1 => (Vec2::new(hi.x - w, lo.y), hi),
        2 => (Vec2::new(lo.x, hi.y - w), hi),
        _ => (lo, Vec2::new(lo.x + w, hi.y)),
    }
}

/// The sidewalks of road cell `c` (rectangles, x/z min and max): strips on edges facing anything
/// that is not a road, corner squares where two roads meet around a block corner. A storm channel
/// beside a road's length (a bridge's sides) gets a sidewalk; at a road's end (a driveway to a
/// channel ramp) it does not. Paths, alleys and channels have none.
pub(crate) fn sidewalk_rects(grid: &Grid, c: IVec2) -> Vec<(Vec2, Vec2)> {
    let Some(class) = grid.road(c) else { return Vec::new() };
    if matches!(class, RoadClass::Path | RoadClass::Alley | RoadClass::Channel) {
        return Vec::new();
    }
    let lo = grid.corner(c);
    let hi = lo + Vec2::splat(grid.cell);
    let r = role(grid, c);
    let is_road = |i: usize, d: IVec2| match grid.road(c + d) {
        Some(RoadClass::Path) | None => false,
        Some(RoadClass::Channel) => match r {
            Role::Ew { .. } => i == 1 || i == 3,
            Role::Ns { .. } => i == 0 || i == 2,
            _ => false,
        },
        Some(_) => true,
    };
    let mut rects = Vec::new();
    let mut walled = [false; 4];
    for (i, d) in DIRS.iter().enumerate() {
        if !is_road(i, *d) {
            walled[i] = true;
            rects.push(edge(lo, hi, i, SIDEWALK));
        }
    }
    for (i, (d1, d2)) in [(0, 1), (1, 2), (2, 3), (3, 0)].iter().enumerate() {
        if !walled[*d1] && !walled[*d2] && grid.road(c + DIRS[*d1] + DIRS[*d2]).is_none_or(|r| r == RoadClass::Path) {
            let corner = [Vec2::new(hi.x, lo.y), hi, Vec2::new(lo.x, hi.y), lo][i];
            let dir = Vec2::new(if corner.x == hi.x { -1.0 } else { 1.0 }, if corner.y == hi.y { -1.0 } else { 1.0 });
            let other = corner + dir * SIDEWALK;
            rects.push((corner.min(other), corner.max(other)));
        }
    }
    rects
}

#[allow(clippy::too_many_arguments)]
fn road_cell(grid: &Grid, c: IVec2, class: RoadClass, lo: Vec2, hi: Vec2, white: Color, yellow: Color, out: &mut Out) {
    let surface = match class {
        RoadClass::Path => Color::hex("#c9b797"),
        RoadClass::Channel => Color::hex("#9a968c"),
        _ => asphalt(),
    };
    let top = if class == RoadClass::Path { LOT_Y } else { ROAD_Y };
    out.blocks.push(slab(lo, hi, -0.6, top, surface));
    if class == RoadClass::Path || class == RoadClass::Alley {
        return;
    }
    for (a, b) in sidewalk_rects(grid, c) {
        out.blocks.push(slab(a, b, ROAD_Y, LOT_Y, sidewalk()));
    }
    if class == RoadClass::Channel {
        return;
    }
    // Markings.
    let dash = |out: &mut Out, a: Vec2, b: Vec2, w: f32, color: Color, dashed: bool| {
        let len = a.distance(b);
        let dir = (b - a) / len.max(1e-3);
        let side = Vec2::new(-dir.y, dir.x) * w * 0.5;
        let (on, off) = if dashed { (3.0, 5.0) } else { (len, 0.0) };
        let mut t = if dashed { 2.0 } else { 0.0 };
        while t < len - 0.1 {
            let e = (t + on).min(len);
            let (p, q) = (a + dir * t, a + dir * e);
            out.blocks.push(paint(
                (p - side).min(q + side).min(p + side).min(q - side),
                (p - side).max(q + side).max(p + side).max(q - side),
                color,
            ));
            t = e + off;
        }
    };
    let m = grid.cell * 0.5;
    match role(grid, c) {
        Role::Ew { k, i } => {
            let zc = lo.y + m;
            if k == 1 {
                dash(out, Vec2::new(lo.x, zc), Vec2::new(hi.x, zc), 0.15, yellow, true);
            } else if i == 0 {
                // The north half of a two-cell road: the double yellow on its south edge, lane
                // lines in each half.
                for off in [-0.12, 0.12] {
                    dash(out, Vec2::new(lo.x, hi.y + off), Vec2::new(hi.x, hi.y + off), 0.12, yellow, false);
                }
                for off in [-4.4, -8.0, 4.4, 8.0] {
                    dash(out, Vec2::new(lo.x, hi.y + off), Vec2::new(hi.x, hi.y + off), 0.13, white, true);
                }
            }
            crossings(grid, c, lo, hi, IVec2::X, k, white, out);
        }
        Role::Ns { k, i } => {
            let xc = lo.x + m;
            if k == 1 {
                dash(out, Vec2::new(xc, lo.y), Vec2::new(xc, hi.y), 0.15, yellow, true);
            } else if i == 0 {
                for off in [-0.12, 0.12] {
                    dash(out, Vec2::new(hi.x + off, lo.y), Vec2::new(hi.x + off, hi.y), 0.12, yellow, false);
                }
                for off in [-4.4, -8.0, 4.4, 8.0] {
                    dash(out, Vec2::new(hi.x + off, lo.y), Vec2::new(hi.x + off, hi.y), 0.13, white, true);
                }
            }
            crossings(grid, c, lo, hi, IVec2::Y, k, white, out);
        }
        Role::Junction | Role::Stub => {}
    }
}

/// Zebra crossings and stop lines where a band meets a junction along its axis `d`.
#[allow(clippy::too_many_arguments)]
fn crossings(grid: &Grid, c: IVec2, lo: Vec2, hi: Vec2, d: IVec2, k: i32, white: Color, out: &mut Out) {
    for s in [d, -d] {
        if role(grid, c + s) != Role::Junction {
            continue;
        }
        // The crossing runs across the road, 1.5 m in from the junction edge, 3 m wide.
        let along = |t: f32| -> f32 {
            if s.x + s.y > 0 {
                if d.x != 0 { hi.x - t } else { hi.y - t }
            } else if d.x != 0 {
                lo.x + t
            } else {
                lo.y + t
            }
        };
        let (a0, a1) = (along(1.0), along(4.0));
        let across = if d.x != 0 { (lo.y, hi.y) } else { (lo.x, hi.x) };
        let mut t = across.0 + if k == 1 { SIDEWALK } else { 0.5 } + 0.3;
        while t < across.1 - 0.9 {
            let (p, q) = if d.x != 0 {
                (Vec2::new(a0.min(a1), t), Vec2::new(a0.max(a1), t + 0.55))
            } else {
                (Vec2::new(t, a0.min(a1)), Vec2::new(t + 0.55, a0.max(a1)))
            };
            out.blocks.push(paint(p, q, white));
            t += 1.1;
        }
        // The stop line before it.
        let (b0, b1) = (along(4.6), along(5.0));
        let (p, q) = if d.x != 0 {
            (Vec2::new(b0.min(b1), across.0 + 1.0), Vec2::new(b0.max(b1), across.1 - 1.0))
        } else {
            (Vec2::new(across.0 + 1.0, b0.min(b1)), Vec2::new(across.1 - 1.0, b0.max(b1)))
        };
        out.blocks.push(paint(p, q, white));
    }
}

/// Triangles collected for one chunk.
#[derive(Default)]
pub(crate) struct MeshBuf {
    p: Vec<[f32; 3]>,
    n: Vec<[f32; 3]>,
    c: Vec<[f32; 3]>,
    i: Vec<u32>,
}

/// The chunk a world point (x, z) falls in.
pub(crate) fn chunk_key(p: Vec2) -> (i32, i32) {
    ((p.x / CHUNK_SIZE).floor() as i32, (p.y / CHUNK_SIZE).floor() as i32)
}

/// Turns per-chunk buffers into mesh patches (with their exact colliders).
pub(crate) fn patches(bufs: BTreeMap<(i32, i32), MeshBuf>, out: &mut Out) {
    for (key, b) in bufs {
        if !b.i.is_empty() {
            out.meshes.push(Arc::new(TerrainPatch::from_mesh(key, b.p, b.n, b.c, b.i)));
        }
    }
}

impl MeshBuf {
    /// A quad (corners in order around it), wound so it faces along `facing`.
    pub(crate) fn quad(&mut self, v: [Vec3; 4], n: [Vec3; 4], c: [[f32; 3]; 4], facing: Vec3) {
        let base = self.p.len() as u32;
        for k in 0..4 {
            self.p.push(v[k].to_array());
            self.n.push(n[k].to_array());
            self.c.push(c[k]);
        }
        let front = (v[1] - v[0]).cross(v[2] - v[0]).dot(facing) >= 0.0;
        if front {
            self.i.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        } else {
            self.i.extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
        }
    }

    /// A triangle of one colour facing along `n`.
    pub(crate) fn tri(&mut self, v: [Vec3; 3], n: Vec3, c: [f32; 3]) {
        let base = self.p.len() as u32;
        for p in v {
            self.p.push(p.to_array());
            self.n.push(n.to_array());
            self.c.push(c);
        }
        if (v[1] - v[0]).cross(v[2] - v[0]).dot(n) >= 0.0 {
            self.i.extend_from_slice(&[base, base + 1, base + 2]);
        } else {
            self.i.extend_from_slice(&[base, base + 2, base + 1]);
        }
    }
}

/// Smooth value noise in about -1..1 (cells `scale` metres wide).
pub fn noise(seed: u64, p: Vec2, scale: f32) -> f32 {
    let q = p / scale;
    let (fx, fz) = (q.x.floor(), q.y.floor());
    let (u, v) = (q.x - fx, q.y - fz);
    let (u, v) = (u * u * (3.0 - 2.0 * u), v * v * (3.0 - 2.0 * v));
    let h = |x: i32, z: i32| (pav_core::rng::hash3(seed, x, 0, z) >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0;
    let (x, z) = (fx as i32, fz as i32);
    let a = h(x, z) + (h(x + 1, z) - h(x, z)) * u;
    let b = h(x, z + 1) + (h(x + 1, z + 1) - h(x, z + 1)) * u;
    a + (b - a) * v
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn scale(a: [f32; 3], k: f32) -> [f32; 3] {
    [a[0] * k, a[1] * k, a[2] * k]
}

/// The colour of natural ground at a point: the zone's ground, darker brush in patches, rock on
/// steep slopes.
pub fn ground_tint(seed: u64, kind: &str, p: Vec2, normal: Vec3) -> [f32; 3] {
    let base = ground_color(kind).0;
    let patches = noise(seed ^ 0x9a7c, p, 23.0) * 0.6 + noise(seed ^ 0x51e, p, 7.0) * 0.4;
    let mut c = mix(base, Color::hex("#5c6a3c").0, (patches * 0.9).clamp(0.0, 0.7));
    let slope = 1.0 - normal.y;
    let rock = ((slope - 0.12) / 0.25).clamp(0.0, 1.0);
    c = mix(c, Color::hex("#8d7f6c").0, rock * 0.8);
    scale(c, 1.0 + noise(seed ^ 0x77, p, 3.0) * 0.06)
}

/// The meshes of every cell with terrain height, one patch per chunk: zone ground at lot level,
/// roads (flat across, a double yellow line on single-cell streets) at asphalt level, kerbs
/// between them, skirts where the map ends.
pub(crate) fn terrain_meshes(def: &DistrictDef, grid: &Grid, terrain: &Terrain, out: &mut Out) {
    let mut bufs: BTreeMap<(i32, i32), MeshBuf> = BTreeMap::new();
    let up = Vec3::Y;
    let asphalt_c = asphalt().0;
    let yellow = Color::hex("#e2b53c").0;
    let kerb_c = Color::hex("#a9a49a").0;
    for cz in 0..grid.h {
        for cx in 0..grid.w {
            let c = IVec2::new(cx, cz);
            if !terrain.is_terrain(c) {
                continue;
            }
            let lo = grid.corner(c);
            let buf = bufs.entry(chunk_key(lo + Vec2::splat(grid.cell * 0.5))).or_default();
            let at = |sx: i32, sz: i32, y0: f32| {
                let q = terrain.sample_pos(sx, sz);
                Vec3::new(q.x, y0 + terrain.sample(sx, sz), q.y)
            };
            let (s0x, s0z) = (cx * PER, cz * PER);
            match grid.at(c).unwrap() {
                Cell::Zone(zi) => {
                    let kind = def.zones[&grid.zones[zi as usize]].ground.as_str();
                    for j in 0..PER {
                        for i in 0..PER {
                            let s =
                                [(s0x + i, s0z + j), (s0x + i, s0z + j + 1), (s0x + i + 1, s0z + j + 1), (s0x + i + 1, s0z + j)];
                            let v = s.map(|(x, z)| at(x, z, LOT_Y));
                            let n = s.map(|(x, z)| terrain.normal(x, z));
                            let col = [0, 1, 2, 3].map(|k| ground_tint(def.seed, kind, Vec2::new(v[k].x, v[k].z), n[k]));
                            buf.quad(v, n, col, up);
                        }
                    }
                }
                Cell::Road(_) => {
                    let r = role(grid, c);
                    // A single-cell band gets its centre line as its own strip.
                    let (along_x, line) = match r {
                        Role::Ew { k: 1, .. } => (true, true),
                        Role::Ns { k: 1, .. } => (false, true),
                        _ => (true, false),
                    };
                    let cuts: Vec<f32> = if line { vec![0.0, 7.85, 8.15, 16.0] } else { vec![0.0, 16.0] };
                    let cuts: Vec<f32> = cuts.iter().map(|t| t / 16.0 * grid.cell).collect();
                    for k in 0..PER {
                        for strip in 0..cuts.len() - 1 {
                            let (a, b) = (cuts[strip], cuts[strip + 1]);
                            let colour = if line && strip == 1 { yellow } else { asphalt_c };
                            // Sample index along the road and the height there (flat across).
                            let corners: [Vec3; 4] = if along_x {
                                let h0 = terrain.sample(s0x + k, s0z + PER / 2);
                                let h1 = terrain.sample(s0x + k + 1, s0z + PER / 2);
                                let (x0, x1) = (lo.x + k as f32 * terrain.step, lo.x + (k + 1) as f32 * terrain.step);
                                if line {
                                    [
                                        Vec3::new(x0, ROAD_Y + h0, lo.y + a),
                                        Vec3::new(x0, ROAD_Y + h0, lo.y + b),
                                        Vec3::new(x1, ROAD_Y + h1, lo.y + b),
                                        Vec3::new(x1, ROAD_Y + h1, lo.y + a),
                                    ]
                                } else {
                                    // Junctions and wider bands: plain quads on the samples.
                                    let row = |j: i32| {
                                        [
                                            at(s0x + k, s0z + j, ROAD_Y),
                                            at(s0x + k, s0z + j + 1, ROAD_Y),
                                            at(s0x + k + 1, s0z + j + 1, ROAD_Y),
                                            at(s0x + k + 1, s0z + j, ROAD_Y),
                                        ]
                                    };
                                    for j in 0..PER {
                                        let v = row(j);
                                        buf.quad(v, [up; 4], [asphalt_c; 4], up);
                                    }
                                    continue;
                                }
                            } else {
                                let h0 = terrain.sample(s0x + PER / 2, s0z + k);
                                let h1 = terrain.sample(s0x + PER / 2, s0z + k + 1);
                                let (z0, z1) = (lo.y + k as f32 * terrain.step, lo.y + (k + 1) as f32 * terrain.step);
                                [
                                    Vec3::new(lo.x + a, ROAD_Y + h0, z0),
                                    Vec3::new(lo.x + a, ROAD_Y + h1, z1),
                                    Vec3::new(lo.x + b, ROAD_Y + h1, z1),
                                    Vec3::new(lo.x + b, ROAD_Y + h0, z0),
                                ]
                            };
                            let n = (corners[1] - corners[0]).cross(corners[2] - corners[0]).normalize_or(up);
                            let n = if n.y < 0.0 { -n } else { n };
                            buf.quad(corners, [n; 4], [colour; 4], up);
                        }
                    }
                    // Kerbs where the road meets terrain ground.
                    for (side, d) in DIRS.iter().enumerate() {
                        let nb = c + *d;
                        if grid.road(nb).is_some() || !terrain.is_terrain(nb) || !grid.inside(nb) {
                            continue;
                        }
                        let edge: Vec<(i32, i32)> = (0..=PER)
                            .map(|t| match side {
                                0 => (s0x + t, s0z),
                                1 => (s0x + PER, s0z + t),
                                2 => (s0x + t, s0z + PER),
                                _ => (s0x, s0z + t),
                            })
                            .collect();
                        let facing = -Vec3::new(d.x as f32, 0.0, d.y as f32);
                        for w in edge.windows(2) {
                            let (a, b) = (w[0], w[1]);
                            let v = [at(a.0, a.1, ROAD_Y), at(a.0, a.1, LOT_Y), at(b.0, b.1, LOT_Y), at(b.0, b.1, ROAD_Y)];
                            buf.quad(v, [facing; 4], [kerb_c; 4], facing);
                        }
                    }
                }
            }
            // Skirts down from the map's edge.
            for (side, d) in DIRS.iter().enumerate() {
                if grid.inside(c + *d) {
                    continue;
                }
                let y0 = if grid.road(c).is_some() { ROAD_Y } else { LOT_Y };
                let facing = Vec3::new(d.x as f32, 0.0, d.y as f32);
                for t in 0..PER {
                    let (a, b) = match side {
                        0 => ((s0x + t, s0z), (s0x + t + 1, s0z)),
                        1 => ((s0x + PER, s0z + t), (s0x + PER, s0z + t + 1)),
                        2 => ((s0x + t, s0z + PER), (s0x + t + 1, s0z + PER)),
                        _ => ((s0x, s0z + t), (s0x, s0z + t + 1)),
                    };
                    let (pa, pb) = (at(a.0, a.1, y0), at(b.0, b.1, y0));
                    let v = [pa, Vec3::new(pa.x, -6.0, pa.z), Vec3::new(pb.x, -6.0, pb.z), pb];
                    buf.quad(v, [facing; 4], [Color::hex("#7a6e5c").0; 4], facing);
                }
            }
        }
    }
    patches(bufs, out);
}
