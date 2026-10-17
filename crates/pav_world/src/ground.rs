//! The ground: road surfaces with sidewalks on their edges (a 16 m street is sidewalk 2 m,
//! parking 2.4 m, two 3.6 m lanes, parking, sidewalk), lane markings, crosswalks and stop lines,
//! the ground of every zone at lot level (15 cm above the asphalt: the kerb), water with a sea bed
//! and a swimming zone.

use glam::{IVec2, Vec2, Vec3};
use pav_core::Color;
use pav_core::statics::{Block, block_flags};
use pav_core::zones::{Zone, ZoneKind};

use crate::Out;
use crate::def::{DistrictDef, RoadClass};
use crate::grid::{Cell, DIRS, Grid};

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

/// How many drivable road cells run through `c` along `d` (both ways, itself included).
fn run(grid: &Grid, c: IVec2, d: IVec2) -> i32 {
    let mut n = 1;
    for s in [d, -d] {
        let mut p = c + s;
        while grid.road(p).is_some_and(|r| r.drivable()) {
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
    let ew = run(grid, c, IVec2::X);
    let ns = run(grid, c, IVec2::Y);
    // A band at most 2 cells thick; longer runs both ways make a junction.
    let along_x = ew >= 3 || (ew == 2 && ns == 1);
    let along_z = ns >= 3 || (ns == 2 && ew == 1);
    match (along_x && ns <= 2, along_z && ew <= 2) {
        (true, false) => {
            let i = (1..=2).take_while(|s| grid.road(c - IVec2::Y * *s).is_some_and(|r| r.drivable())).count() as i32;
            Role::Ew { k: ns, i }
        }
        (false, true) => {
            let i = (1..=2).take_while(|s| grid.road(c - IVec2::X * *s).is_some_and(|r| r.drivable())).count() as i32;
            Role::Ns { k: ew, i }
        }
        (true, true) if ns <= 2 && ew <= 2 => Role::Stub,
        _ if ew >= 2 && ns >= 2 => Role::Junction,
        _ => Role::Stub,
    }
}

pub fn build(def: &DistrictDef, grid: &Grid, out: &mut Out) {
    let cs = grid.cell;
    let white = Color::hex("#e8e5da");
    let yellow = Color::hex("#e2b53c");
    for z in 0..grid.h {
        for x in 0..grid.w {
            let c = IVec2::new(x, z);
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
                        out.blocks.push(slab(lo, hi, -0.6, LOT_Y, ground_color(&rules.ground)));
                    }
                }
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
    // Sidewalks on edges facing anything that is not a road; corner squares where two roads meet
    // around a block corner.
    let is_road = |d: IVec2| grid.road(c + d).is_some_and(|r| r != RoadClass::Path);
    let mut walled = [false; 4];
    for (i, d) in DIRS.iter().enumerate() {
        if !is_road(*d) {
            walled[i] = true;
            let (a, b) = edge(lo, hi, i, SIDEWALK);
            out.blocks.push(slab(a, b, ROAD_Y, LOT_Y, sidewalk()));
        }
    }
    for (i, (d1, d2)) in [(0, 1), (1, 2), (2, 3), (3, 0)].iter().enumerate() {
        if !walled[*d1] && !walled[*d2] && !is_road(DIRS[*d1] + DIRS[*d2]) {
            let corner = [Vec2::new(hi.x, lo.y), hi, Vec2::new(lo.x, hi.y), lo][i];
            let dir = Vec2::new(if corner.x == hi.x { -1.0 } else { 1.0 }, if corner.y == hi.y { -1.0 } else { 1.0 });
            let other = corner + dir * SIDEWALK;
            out.blocks.push(slab(corner.min(other), corner.max(other), ROAD_Y, LOT_Y, sidewalk()));
        }
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
