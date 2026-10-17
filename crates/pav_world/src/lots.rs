//! Blocks become lots, lots become buildings: each block (connected cells of one zone) is cut
//! into rectangles, each rectangle split along its long side into lots of the zone's widths,
//! and each lot gets a building of one of the zone's styles — a few boxes each (one collider
//! per mass): podium and tower for downtown, stucco shops with awnings, bungalows with stepped
//! roofs, sheds and warehouses. Landmarks build their own placeholder masses with a sign.

use glam::{IVec2, Quat, Vec2, Vec3};
use pav_core::Color;
use pav_core::rng::Rng;
use pav_core::shape::{Look, Shape};
use pav_core::statics::{Block, Decor};
use pav_core::zones::{Label, LabelMode};

use crate::def::{DistrictDef, ZoneRules};
use crate::grid::{Grid, rectangles};
use crate::ground::LOT_Y;
use crate::{Building, Out, rng};

/// A lot: its rectangle and the side facing the street (0 N, 1 E, 2 S, 3 W).
#[derive(Clone, Copy, Debug)]
struct Lot {
    min: Vec2,
    max: Vec2,
    front: usize,
}

fn pick<'a>(r: &mut Rng, list: &[&'a str]) -> &'a str {
    list[r.below(list.len() as u32) as usize]
}

fn color(r: &mut Rng, list: &[&str]) -> Color {
    Color::hex(pick(r, list))
}

/// Splits a rectangle into lots no wider than the zone allows (`depth` limits the other side).
fn split(min: Vec2, max: Vec2, rules: &ZoneRules, r: &mut Rng, out: &mut Vec<(Vec2, Vec2)>) {
    let size = max - min;
    let (long, short) = if size.x >= size.y { (size.x, size.y) } else { (size.y, size.x) };
    let along_x = size.x >= size.y;
    let [lo, hi] = rules.lot;
    if long <= hi && short <= rules.depth.max(hi) {
        out.push((min, max));
        return;
    }
    // Cut the long side (or the short one if it is too deep for one lot).
    let (len, cut_x) = if short > rules.depth && long <= hi { (short, !along_x) } else { (long, along_x) };
    let cut = if len >= lo * 2.0 { r.range(lo, (len - lo).min(hi)) } else { len * 0.5 };
    if cut_x {
        split(min, Vec2::new(min.x + cut, max.y), rules, r, out);
        split(Vec2::new(min.x + cut, min.y), max, rules, r, out);
    } else {
        split(min, Vec2::new(max.x, min.y + cut), rules, r, out);
        split(Vec2::new(min.x, min.y + cut), max, rules, r, out);
    }
}

fn block(min: Vec2, max: Vec2, y0: f32, y1: f32, c: Color) -> Block {
    Block::new(Vec3::new(min.x, y0, min.y), Vec3::new(max.x, y1, max.y), c)
}

/// A non-solid box (trim, awning, roof detail).
fn trim(min: Vec3, max: Vec3, c: Color) -> Decor {
    Decor {
        shape: Shape::Box { half: (max - min) * 0.5 },
        pos: (min + max) * 0.5,
        rot: Quat::IDENTITY,
        color: c,
        look: Look::Cel,
        emissive: 0.0,
        solid: false,
        collider: None,
        sway: Default::default(),
        material: None,
    }
}

pub fn build(def: &DistrictDef, grid: &Grid, out: &mut Out) {
    for (zone, cells) in grid.blocks() {
        let name = &grid.zones[zone as usize];
        let rules = &def.zones[name];
        if rules.styles.is_empty() {
            continue;
        }
        for (c0, c1) in rectangles(&cells) {
            let min = grid.corner(c0);
            let max = grid.corner(c1 + IVec2::ONE);
            let mut r = rng(def.seed, 0x107, c0.x, c0.y);
            let mut rects = Vec::new();
            split(min, max, rules, &mut r, &mut rects);
            for (lmin, lmax) in rects {
                let front = facing(grid, lmin, lmax);
                let lot = Lot { min: lmin, max: lmax, front };
                let style = rules.styles[r.below(rules.styles.len() as u32) as usize].clone();
                building(&style, lot, rules, zone, &mut r, out);
            }
        }
    }
    for (i, l) in def.landmark.iter().enumerate() {
        landmark(def, grid, i, l, out);
    }
}

/// The side of a lot nearest a road (looked up one metre outside each side's middle).
fn facing(grid: &Grid, min: Vec2, max: Vec2) -> usize {
    let mid = (min + max) * 0.5;
    let probes = [
        Vec2::new(mid.x, min.y - 1.0),
        Vec2::new(max.x + 1.0, mid.y),
        Vec2::new(mid.x, max.y + 1.0),
        Vec2::new(min.x - 1.0, mid.y),
    ];
    probes.iter().position(|p| grid.road(grid.cell_of(*p)).is_some()).unwrap_or(2)
}

fn building(style: &str, lot: Lot, rules: &ZoneRules, zone: u16, r: &mut Rng, out: &mut Out) {
    let gap = 0.6;
    let mut min = lot.min + Vec2::splat(gap);
    let mut max = lot.max - Vec2::splat(gap);
    // Setback on the street side.
    let s = rules.setback;
    match lot.front {
        0 => min.y += s,
        1 => max.x -= s,
        2 => max.y -= s,
        _ => min.x += s,
    }
    if (max - min).min_element() < 4.0 {
        return;
    }
    let h = r.range(rules.height[0], rules.height[1]);
    let y0 = LOT_Y;
    let size = max - min;
    let inset = |k: f32| (min + size * k, max - size * k);
    match style {
        "tower_glass" | "tower_concrete" => {
            let glass = style == "tower_glass";
            let podium_h = r.range(7.0, 12.0).min(h * 0.3);
            let base = color(r, &["#cfc7b8", "#bdb4a4", "#d8d0c2", "#a9a092"]);
            out.blocks.push(block(min, max, y0, y0 + podium_h, base));
            let (tmin, tmax) = inset(r.range(0.12, 0.22));
            let body = if glass {
                color(r, &["#5f7d96", "#6d8ba1", "#4f6b80", "#7a96a8", "#58738a"])
            } else {
                color(r, &["#b8b0a2", "#a59c8d", "#c4bcae", "#9a9286"])
            };
            out.blocks.push(block(tmin, tmax, y0 + podium_h, y0 + h, body));
            // A setback crown and a roof box.
            let (cmin, cmax) = inset(r.range(0.28, 0.36));
            let crown_h = r.range(3.0, 7.0);
            out.blocks.push(block(cmin, cmax, y0 + h, y0 + h + crown_h, base));
            // Floor bands on concrete towers.
            if !glass {
                let band = Color::hex("#5d6670");
                let mut y = y0 + podium_h + 3.5;
                while y < y0 + h - 1.0 {
                    out.decor.push(trim(
                        Vec3::new(tmin.x - 0.05, y, tmin.y - 0.05),
                        Vec3::new(tmax.x + 0.05, y + 1.2, tmax.y + 0.05),
                        band,
                    ));
                    y += 3.6;
                }
            }
            out.buildings.push(Building { min, max, height: h + crown_h, zone, color: body.0 });
        }
        "stucco_shop" | "apartment" => {
            let apt = style == "apartment";
            let wall = color(r, &["#ead9be", "#f1caa1", "#dab48d", "#cad4c1", "#e7b9a3", "#f3e3c5", "#e0c7a6", "#d7c9b3"]);
            out.blocks.push(block(min, max, y0, y0 + h, wall));
            // Parapet trim.
            let tc = color(r, &["#8c6f56", "#6f7a80", "#a0826a", "#5f6d5b"]);
            out.decor.push(trim(
                Vec3::new(min.x - 0.12, y0 + h, min.y - 0.12),
                Vec3::new(max.x + 0.12, y0 + h + 0.6, max.y + 0.12),
                tc,
            ));
            // An awning (shops) or balconies (apartments) on the street side.
            let (a0, a1, out_dir) = front_edge(min, max, lot.front);
            if apt {
                let mut y = y0 + 3.2;
                while y < y0 + h - 2.0 {
                    out.decor.push(trim(
                        Vec3::new(a0.x, y, a0.y).min(Vec3::new(a1.x, y, a1.y) + out_dir * 1.2),
                        Vec3::new(a0.x, y + 0.15, a0.y).max(Vec3::new(a1.x, y + 0.15, a1.y) + out_dir * 1.2),
                        tc,
                    ));
                    y += 3.0;
                }
            } else {
                let awning = color(r, &["#c0392b", "#2e86ab", "#e08e1b", "#27ae60", "#8e44ad", "#16a085"]);
                let y = y0 + 3.0;
                let p0 = Vec3::new(a0.x, y, a0.y);
                let p1 = Vec3::new(a1.x, y + 0.25, a1.y) + out_dir * 1.6;
                out.decor.push(trim(p0.min(p1), p0.max(p1), awning));
                // A sign band over the awning.
                let sign = color(r, &["#f4f1ea", "#1f2933", "#ffd166", "#ef476f"]);
                let q0 = Vec3::new(a0.x, y + 0.6, a0.y) + out_dir * 0.05;
                let q1 = Vec3::new(a1.x, y + 1.6, a1.y) + out_dir * 0.25;
                out.decor.push(trim(q0.min(q1), q0.max(q1), sign));
            }
            out.buildings.push(Building { min, max, height: h, zone, color: wall.0 });
        }
        "bungalow" => {
            let w = color(r, &["#f2e3c9", "#e8cdb0", "#d9e2d0", "#efd3c7", "#e6dcc3", "#c9d6df"]);
            let (bmin, bmax) = inset(0.12);
            let hh = h.min(4.2);
            out.blocks.push(block(bmin, bmax, y0, y0 + hh, w));
            // A stepped hip roof in terracotta or grey shingle.
            let roof = color(r, &["#b5563a", "#a4492f", "#6e6a66", "#8a5a44"]);
            let mut lo = bmin - Vec2::splat(0.5);
            let mut hi = bmax + Vec2::splat(0.5);
            let mut y = y0 + hh;
            for _ in 0..3 {
                if (hi - lo).min_element() < 1.0 {
                    break;
                }
                out.decor.push(trim(Vec3::new(lo.x, y, lo.y), Vec3::new(hi.x, y + 0.45, hi.y), roof));
                lo += Vec2::splat(1.0);
                hi -= Vec2::splat(1.0);
                y += 0.45;
            }
            out.buildings.push(Building { min: bmin, max: bmax, height: hh + 1.4, zone, color: w.0 });
        }
        "shed" | "warehouse" => {
            let w = color(r, &["#9aa3a8", "#b9a07a", "#8f9a9e", "#a7a199", "#7f8c8d", "#b0a48a"]);
            out.blocks.push(block(min, max, y0, y0 + h, w));
            // Sawtooth roof ridges along the long side.
            let ridge = Color::hex("#6c7a80");
            let long_x = size.x >= size.y;
            let n = ((if long_x { size.x } else { size.y }) / 6.0).floor().max(1.0) as i32;
            for i in 0..n {
                let t = (i as f32 + 0.5) / n as f32;
                let (p0, p1) = if long_x {
                    let x = min.x + size.x * t;
                    (Vec3::new(x - 1.2, y0 + h, min.y), Vec3::new(x + 1.2, y0 + h + 1.2, max.y))
                } else {
                    let z = min.y + size.y * t;
                    (Vec3::new(min.x, y0 + h, z - 1.2), Vec3::new(max.x, y0 + h + 1.2, z + 1.2))
                };
                out.decor.push(trim(p0, p1, ridge));
            }
            // Loading doors on the street side.
            let (a0, a1, d) = front_edge(min, max, lot.front);
            let door = Color::hex("#3c4247");
            let len = a0.distance(a1);
            let along = (a1 - a0) / len.max(1e-3);
            let mut t = 2.0;
            while t + 4.0 < len {
                let p = a0 + along * t;
                let q = p + along * 3.6;
                let (p3, q3) = (Vec3::new(p.x, y0, p.y) + d * 0.05, Vec3::new(q.x, y0 + 4.0, q.y) + d * 0.1);
                out.decor.push(trim(p3.min(q3), p3.max(q3), door));
                t += 7.0;
            }
            out.buildings.push(Building { min, max, height: h + 1.2, zone, color: w.0 });
        }
        _ => {
            out.blocks.push(block(min, max, y0, y0 + h, Color::hex("#c8c0b0")));
            out.buildings.push(Building { min, max, height: h, zone, color: Color::hex("#c8c0b0").0 });
        }
    }
}

/// The street-side edge of a rectangle (its two ends) and the outward direction.
fn front_edge(min: Vec2, max: Vec2, front: usize) -> (Vec2, Vec2, Vec3) {
    match front {
        0 => (Vec2::new(min.x, min.y), Vec2::new(max.x, min.y), Vec3::NEG_Z),
        1 => (Vec2::new(max.x, min.y), Vec2::new(max.x, max.y), Vec3::X),
        2 => (Vec2::new(min.x, max.y), Vec2::new(max.x, max.y), Vec3::Z),
        _ => (Vec2::new(min.x, min.y), Vec2::new(min.x, max.y), Vec3::NEG_X),
    }
}

/// A landmark's placeholder: masses by assembly, and its name on a sign (authored assemblies
/// replace these as content lands).
fn landmark(def: &DistrictDef, grid: &Grid, i: usize, l: &crate::def::Landmark, out: &mut Out) {
    let [[x0, z0], [x1, z1]] = l.cells;
    let min = grid.corner(IVec2::new(x0, z0)) + Vec2::splat(2.0);
    let max = grid.corner(IVec2::new(x1 + 1, z1 + 1)) - Vec2::splat(2.0);
    let mut r = rng(def.seed, 0x1a9 + i as u64, x0, z0);
    let size = max - min;
    let y0 = LOT_Y;
    let zone = grid.at(IVec2::new(x0, z0)).and_then(|c| if let crate::Cell::Zone(z) = c { Some(z) } else { None }).unwrap_or(0);
    // Plaza paving under it.
    out.blocks.push(block(min - Vec2::splat(1.5), max + Vec2::splat(1.5), y0 - 0.05, y0 + 0.02, Color::hex("#cfc6b4")));
    let h = if l.height > 0.0 { l.height } else { 12.0 };
    let (top, body): (f32, Color) = match l.assembly.as_str() {
        "tower" | "tower_bank" => {
            let (bmin, bmax) = (min + size * 0.2, max - size * 0.2);
            out.blocks.push(block(min + size * 0.05, max - size * 0.05, y0, y0 + 10.0, Color::hex("#d9d2c3")));
            out.blocks.push(block(bmin, bmax, y0 + 10.0, y0 + h, Color::hex("#3f5c74")));
            out.blocks.push(block(bmin + size * 0.1, bmax - size * 0.1, y0 + h, y0 + h + 8.0, Color::hex("#d9d2c3")));
            (h + 8.0, Color::hex("#3f5c74"))
        }
        "police" => {
            out.blocks.push(block(min, max - Vec2::new(0.0, size.y * 0.4), y0, y0 + h, Color::hex("#d8d4c8")));
            out.blocks.push(block(
                Vec2::new(min.x, min.y + size.y * 0.15),
                Vec2::new(min.x + size.x * 0.25, min.y + size.y * 0.45),
                y0 + h,
                y0 + h + 3.0,
                Color::hex("#2f4a7a"),
            ));
            (h, Color::hex("#d8d4c8"))
        }
        "hospital" => {
            out.blocks.push(block(min, max, y0, y0 + h, Color::hex("#eae7df")));
            let c = (min + max) * 0.5;
            // The helipad on the roof.
            out.decor.push(Decor {
                shape: Shape::Cylinder { half_height: 0.1, radius: (size.min_element() * 0.3).min(9.0) },
                pos: Vec3::new(c.x, y0 + h + 0.1, c.y),
                rot: Quat::IDENTITY,
                color: Color::hex("#3d4248"),
                look: Look::Cel,
                emissive: 0.0,
                solid: true,
                collider: None,
                sway: Default::default(),
                material: None,
            });
            (h, Color::hex("#eae7df"))
        }
        "open" => (0.0, Color::hex("#cfc6b4")),
        _ => {
            let c = color(&mut r, &["#e7d6bd", "#d8c6ae", "#c9b9a3"]);
            out.blocks.push(block(min, max, y0, y0 + h, c));
            (h, c)
        }
    };
    if top > 0.0 {
        out.buildings.push(Building { min, max, height: top, zone, color: body.0 });
    }
    // The name over the front, facing south.
    out.labels.push(Label {
        text: l.name.to_uppercase(),
        pos: Vec3::new((min.x + max.x) * 0.5, y0 + top.clamp(4.0, 9.0) + 1.0, max.y + 0.4),
        size: 1.4,
        color: Color::hex("#f4f1ea"),
        mode: LabelMode::Billboard,
        facing: Default::default(),
    });
}
