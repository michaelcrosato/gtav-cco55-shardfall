//! Storm channels (The Ditch, PLAN.md §5.3): road-class `channel` cells running north-south
//! become a sunken concrete bed with sloped walls and a low-flow trickle down its middle; roads
//! that cross keep their own cells and become bridges with parapets; the channel ends in a
//! culvert headwall upstream and a weir against the sea downstream; ramps run down its walls from
//! driveways. Triangle meshes with exact colliders per chunk; the bed's centre line is kept for
//! the lane graph.

use std::collections::BTreeMap;

use glam::{IVec2, Quat, Vec2, Vec3};
use pav_core::Color;
use pav_core::shape::{Look, Shape};
use pav_core::statics::{Block, Decor};

use crate::def::{DistrictDef, RoadClass};
use crate::grid::{Cell, Grid};
use crate::ground::{LOT_Y, MeshBuf, ROAD_Y, chunk_key, patches};
use crate::terrain::Terrain;
use crate::{Out, ground_at};

/// The bed's height (below the streets) and the trickle's.
pub const BED_Y: f32 = -6.0;
const TRICKLE_Y: f32 = -6.3;
/// Wall run (horizontal) on each side, as a share of the channel's width.
const WALL: f32 = 0.25;

/// A channel's geometry: the columns it spans, its rows (north to south), the bed's centre line.
#[derive(Clone, Debug)]
pub struct ChannelGeom {
    pub x0: f32,
    pub x1: f32,
    pub z0: f32,
    pub z1: f32,
    /// Ramps: (top end, bottom end) on the road surface.
    pub ramps: Vec<(Vec3, Vec3)>,
}

impl ChannelGeom {
    pub fn centre_x(&self) -> f32 {
        (self.x0 + self.x1) * 0.5
    }
}

/// The cross-section from the west edge (u = 0) to the east (u = w): points (u, v) with the
/// colour of the face to the next point. `west`/`east` are the heights of the ground beside.
fn profile(w: f32, west: f32, east: f32) -> Vec<(Vec2, [f32; 3])> {
    let wall = Color::hex("#a7a296").0;
    let bed = Color::hex("#8e8a80").0;
    let water = Color::hex("#3d6b6e").0;
    let coping = Color::hex("#c4bfb3").0;
    let ww = w * WALL;
    let mid = w * 0.5;
    vec![
        (Vec2::new(0.0, west), coping),
        (Vec2::new(0.5, west), wall),
        (Vec2::new(ww, BED_Y), bed),
        (Vec2::new(mid - 1.4, BED_Y), wall),
        (Vec2::new(mid - 1.0, TRICKLE_Y), water),
        (Vec2::new(mid + 1.0, TRICKLE_Y), wall),
        (Vec2::new(mid + 1.4, BED_Y), bed),
        (Vec2::new(w - ww, BED_Y), wall),
        (Vec2::new(w - 0.5, east), coping),
        (Vec2::new(w, east), coping),
    ]
}

/// The height of a channel's surface at `u` across it (for ramps and tests).
pub fn surface(w: f32, west: f32, east: f32, u: f32) -> f32 {
    let p = profile(w, west, east);
    for k in 0..p.len() - 1 {
        let (a, b) = (p[k].0, p[k + 1].0);
        if u >= a.x && u <= b.x {
            let t = if b.x > a.x { (u - a.x) / (b.x - a.x) } else { 0.0 };
            return a.y + (b.y - a.y) * t;
        }
    }
    BED_Y
}

fn decor(shape: Shape, pos: Vec3, color: &str) -> Decor {
    Decor {
        shape,
        pos,
        rot: Quat::IDENTITY,
        color: Color::hex(color),
        look: Look::Cel,
        emissive: 0.0,
        solid: false,
        collider: None,
        sway: Default::default(),
        material: None,
        glow: Default::default(),
    }
}

/// Is `c` a channel cell or a road crossing one (a bridge cell inside the channel's columns)?
fn in_channel(grid: &Grid, c: IVec2, cols: (i32, i32)) -> bool {
    if c.x < cols.0 || c.x > cols.1 {
        return false;
    }
    matches!(grid.at(c), Some(Cell::Road(_)))
}

pub(crate) fn build(def: &DistrictDef, grid: &Grid, terrain: &Terrain, out: &mut Out) -> Vec<ChannelGeom> {
    let ground = |p: Vec2| ground_at(def, grid, terrain, p);
    let is_channel = |c: IVec2| grid.road(c) == Some(RoadClass::Channel);
    // Find each channel: a run of channel columns starting at its north-west cell.
    let mut seen = vec![false; (grid.w * grid.h) as usize];
    let mut geoms = Vec::new();
    let mut bufs: BTreeMap<(i32, i32), MeshBuf> = BTreeMap::new();
    for z in 0..grid.h {
        for x in 0..grid.w {
            let c = IVec2::new(x, z);
            if !is_channel(c) || seen[(z * grid.w + x) as usize] {
                continue;
            }
            // Its columns: channel cells to the east on this row.
            let mut x1 = x;
            while is_channel(IVec2::new(x1 + 1, z)) {
                x1 += 1;
            }
            let cols = (x, x1);
            // Its rows: down while the row is channel or a bridge across it.
            let mut z1 = z;
            while (cols.0..=cols.1).all(|cx| in_channel(grid, IVec2::new(cx, z1 + 1), cols)) {
                z1 += 1;
            }
            for zz in z..=z1 {
                for xx in cols.0..=cols.1 {
                    seen[(zz * grid.w + xx) as usize] = true;
                }
            }
            let lo = grid.corner(IVec2::new(cols.0, z));
            let hi = grid.corner(IVec2::new(cols.1 + 1, z1 + 1));
            let w = hi.x - lo.x;
            for row in z..=z1 {
                let z0 = grid.corner(IVec2::new(cols.0, row)).y;
                let zb = z0 + grid.cell;
                let bridge = !is_channel(IVec2::new(cols.0, row));
                // Under a bridge the walls stop below its slab.
                let (west, east) = if bridge {
                    (-0.6, -0.6)
                } else {
                    (
                        ground(Vec2::new(lo.x - 1.0, z0 + grid.cell * 0.5)).max(ROAD_Y),
                        ground(Vec2::new(hi.x + 1.0, z0 + grid.cell * 0.5)).max(ROAD_Y),
                    )
                };
                let prof = profile(w, west, east);
                let buf = bufs.entry(chunk_key(Vec2::new(lo.x + w * 0.5, z0 + grid.cell * 0.5))).or_default();
                for k in 0..prof.len() - 1 {
                    let (a, col) = prof[k];
                    let b = prof[k + 1].0;
                    let d = b - a;
                    let n = Vec3::new(-d.y, d.x, 0.0).normalize_or(Vec3::Y);
                    let v = [
                        Vec3::new(lo.x + a.x, a.y, z0),
                        Vec3::new(lo.x + a.x, a.y, zb),
                        Vec3::new(lo.x + b.x, b.y, zb),
                        Vec3::new(lo.x + b.x, b.y, z0),
                    ];
                    buf.quad(v, [n; 4], [col; 4], n);
                }
                let rail = Color::hex("#cfc9bc");
                if bridge {
                    // Parapets on the bridge's edges over the open channel.
                    if is_channel(IVec2::new(cols.0, row - 1)) {
                        out.blocks.push(Block::new(Vec3::new(lo.x, LOT_Y, z0), Vec3::new(hi.x, LOT_Y + 1.0, z0 + 0.35), rail));
                    }
                    if is_channel(IVec2::new(cols.0, row + 1)) {
                        out.blocks.push(Block::new(Vec3::new(lo.x, LOT_Y, zb - 0.35), Vec3::new(hi.x, LOT_Y + 1.0, zb), rail));
                    }
                } else {
                    // A low wall along each top edge, except where a road or driveway comes in.
                    for (x, nb) in [(lo.x, cols.0 - 1), (hi.x - 0.3, cols.1 + 1)] {
                        if grid.road(IVec2::new(nb, row)).is_none() {
                            let y = if x < lo.x + 1.0 { west } else { east };
                            out.blocks.push(Block::new(Vec3::new(x, y, z0), Vec3::new(x + 0.3, y + 0.9, zb), rail));
                        }
                    }
                }
            }
            // The upstream headwall with its culvert, the weir at the sea.
            for (zz, facing) in [(lo.y, 1.0f32), (hi.y, -1.0)] {
                let west = ground(Vec2::new(lo.x - 1.0, zz + facing * 8.0)).max(ROAD_Y);
                let east = ground(Vec2::new(hi.x + 1.0, zz + facing * 8.0)).max(ROAD_Y);
                let top = west.max(east) + 0.2;
                let prof = profile(w, west, east);
                let n = Vec3::new(0.0, 0.0, facing);
                let buf = bufs.entry(chunk_key(Vec2::new(lo.x + w * 0.5, zz + facing * 8.0))).or_default();
                for k in 0..prof.len() - 1 {
                    let (a, b) = (prof[k].0, prof[k + 1].0);
                    let v = [
                        Vec3::new(lo.x + a.x, a.y, zz),
                        Vec3::new(lo.x + b.x, b.y, zz),
                        Vec3::new(lo.x + b.x, top, zz),
                        Vec3::new(lo.x + a.x, top, zz),
                    ];
                    buf.quad(v, [n; 4], [Color::hex("#9d988c").0; 4], n);
                }
                if facing > 0.0 {
                    // The culvert's dark mouth.
                    let mut mouth = decor(
                        Shape::Box { half: Vec3::new(w * 0.22, 2.1, 0.05) },
                        Vec3::new(lo.x + w * 0.5, BED_Y + 2.1, zz + 0.06),
                        "#15181b",
                    );
                    mouth.look = Look::Unlit;
                    out.decor.push(mouth);
                }
            }
            // Ramps down the walls.
            let mut ramps = Vec::new();
            for r in def.channel_ramp.iter().filter(|r| r.col >= cols.0 && r.col <= cols.1) {
                let (za, zb) = (grid.origin.y + r.rows[0] * grid.cell, grid.origin.y + r.rows[1] * grid.cell);
                let west = r.side != "east";
                // Centre across the channel: from the coping to the bed's edge, 5 m wide.
                let u_at = |t: f32| if west { 3.0 + t * w * WALL } else { w - 3.0 - t * w * WALL };
                let side_top =
                    if west { ground(Vec2::new(lo.x - 1.0, za)) } else { ground(Vec2::new(hi.x + 1.0, za)) }.max(ROAD_Y);
                let steps = 16;
                let ramp_c = Color::hex("#9a958a").0;
                let edge_c = Color::hex("#b8b2a6").0;
                let mut pts = Vec::new();
                for i in 0..=steps {
                    let t = i as f32 / steps as f32;
                    let e = t * t * (3.0 - 2.0 * t);
                    let u = u_at(t);
                    let y = side_top + 0.02 + (BED_Y + 0.02 - side_top - 0.02) * e;
                    pts.push((Vec3::new(lo.x + u, y, za + (zb - za) * t), u));
                }
                for i in 0..steps {
                    let (p, _) = pts[i];
                    let (q, _) = pts[i + 1];
                    let buf = bufs.entry(chunk_key(Vec2::new((p.x + q.x) * 0.5, (p.z + q.z) * 0.5))).or_default();
                    let half = Vec3::X * 2.5;
                    let top = [p - half, q - half, q + half, p + half];
                    let n = (top[1] - top[0]).cross(top[3] - top[0]).normalize_or(Vec3::Y);
                    let n = if n.y < 0.0 { -n } else { n };
                    buf.quad(top, [n; 4], [ramp_c; 4], n);
                    // Its two sides down to the bed.
                    for s in [-1.0f32, 1.0] {
                        let (a, b) = (p + half * s, q + half * s);
                        let side = [a, b, Vec3::new(b.x, BED_Y, b.z), Vec3::new(a.x, BED_Y, a.z)];
                        let nn = Vec3::X * s;
                        buf.quad(side, [nn; 4], [edge_c; 4], nn);
                    }
                }
                // A cap closing the ramp's top end.
                let (p, _) = pts[0];
                let n = Vec3::new(0.0, 0.0, (za - zb).signum());
                let half = Vec3::X * 2.5;
                let cap = [p - half, p + half, Vec3::new(p.x + 2.5, BED_Y, p.z), Vec3::new(p.x - 2.5, BED_Y, p.z)];
                bufs.entry(chunk_key(Vec2::new(p.x, p.z))).or_default().quad(cap, [n; 4], [edge_c; 4], n);
                ramps.push((pts[0].0, pts[steps].0));
            }
            geoms.push(ChannelGeom { x0: lo.x, x1: hi.x, z0: lo.y, z1: hi.y, ramps });
        }
    }
    patches(bufs, out);
    geoms
}
