//! The edge of the map: an invisible wall all the way round (hidden blocks, one per chunk along
//! each side, from below the sea bed to well above any helicopter), and where a road runs off the
//! map a row of jersey barriers across it with a ROAD CLOSED sign, so the wall is never a surprise.
//! A road that ends at the water's edge (a quay) is closed off the same way, with a DEAD END sign,
//! so nothing drives off it into the sea.

use glam::{IVec2, Quat, Vec2, Vec3};
use pav_core::Color;
use pav_core::shape::Shape;
use pav_core::statics::{Block, CHUNK_SIZE, block_flags};

use crate::def::{DistrictDef, RoadClass};
use crate::freeway::{decor, sign};
use crate::grid::Grid;
use crate::terrain::Terrain;
use crate::{Out, ground_at};

/// The wall's thickness (inside the map) and its bottom and top.
const THICK: f32 = 0.5;
const BOTTOM: f32 = -40.0;
const TOP: f32 = 600.0;

pub(crate) fn build(def: &DistrictDef, grid: &Grid, terrain: &Terrain, out: &mut Out) {
    let lo = grid.origin;
    let hi = grid.origin + Vec2::new(grid.w as f32, grid.h as f32) * grid.cell;
    // Segments break at chunk edges, so each belongs to the chunk it stands in.
    let cuts = |a: f32, b: f32| {
        let mut v = vec![a];
        let mut k = (a / CHUNK_SIZE).floor() + 1.0;
        while k * CHUNK_SIZE < b {
            v.push(k * CHUNK_SIZE);
            k += 1.0;
        }
        v.push(b);
        v
    };
    let wall = |min: Vec3, max: Vec3| Block::new(min, max, Color::hex("#808080")).with_flags(block_flags::HIDDEN);
    for w in cuts(lo.y, hi.y).windows(2) {
        out.blocks.push(wall(Vec3::new(lo.x, BOTTOM, w[0]), Vec3::new(lo.x + THICK, TOP, w[1])));
        out.blocks.push(wall(Vec3::new(hi.x - THICK, BOTTOM, w[0]), Vec3::new(hi.x, TOP, w[1])));
    }
    for w in cuts(lo.x, hi.x).windows(2) {
        out.blocks.push(wall(Vec3::new(w[0], BOTTOM, lo.y), Vec3::new(w[1], TOP, lo.y + THICK)));
        out.blocks.push(wall(Vec3::new(w[0], BOTTOM, hi.y - THICK), Vec3::new(w[1], TOP, hi.y)));
    }
    // Roads off the map: each side's edge cells, with the direction into the map.
    let sides = [
        ((0..grid.h).map(|r| IVec2::new(0, r)).collect::<Vec<_>>(), Vec2::X),
        ((0..grid.h).map(|r| IVec2::new(grid.w - 1, r)).collect(), Vec2::NEG_X),
        ((0..grid.w).map(|c| IVec2::new(c, 0)).collect(), Vec2::Y),
        ((0..grid.w).map(|c| IVec2::new(c, grid.h - 1)).collect(), Vec2::NEG_Y),
    ];
    for (cells, inward) in sides {
        let along = Vec2::new(-inward.y, inward.x);
        let mut run: Vec<IVec2> = Vec::new();
        for (i, &c) in cells.iter().enumerate() {
            // A road that carries on into the map (not one running along the edge).
            let off_map = grid.road(c).is_some_and(|r| r != RoadClass::Path && r != RoadClass::Channel)
                && grid.road(c + inward.as_ivec2()).is_some();
            if off_map {
                run.push(c);
            }
            if (!off_map || i + 1 == cells.len()) && !run.is_empty() {
                close(def, grid, terrain, &run, inward, along, "ROAD CLOSED", out);
                run.clear();
            }
        }
    }
    // Roads that end at the water: the last cells of each (with the road behind them and water
    // beyond), in runs across it.
    for out_dir in [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y] {
        let across = IVec2::new(-out_dir.y, out_dir.x);
        let mut ends: Vec<IVec2> = (0..grid.h)
            .flat_map(|z| (0..grid.w).map(move |x| IVec2::new(x, z)))
            .filter(|&c| {
                grid.road(c).is_some_and(|r| r != RoadClass::Path && r != RoadClass::Channel)
                    && grid.road(c - out_dir).is_some()
                    && grid.zone(c + out_dir) == Some("water")
            })
            .collect();
        // By row (along the road), then across it.
        ends.sort_by_key(|c| (c.dot(out_dir), c.dot(across)));
        let mut run: Vec<IVec2> = Vec::new();
        for (i, &c) in ends.iter().enumerate() {
            run.push(c);
            let next = ends.get(i + 1).copied();
            if next != Some(c + across) {
                close(def, grid, terrain, &run, -out_dir.as_vec2(), across.as_vec2(), "DEAD END", out);
                run.clear();
            }
        }
    }
}

/// Jersey barriers across a run of road cells at the edge, 2 m in, and the sign before them.
#[allow(clippy::too_many_arguments)]
fn close(
    def: &DistrictDef,
    grid: &Grid,
    terrain: &Terrain,
    run: &[IVec2],
    inward: Vec2,
    along: Vec2,
    words: &str,
    out: &mut Out,
) {
    let centre = |c: IVec2| grid.corner(c) + Vec2::splat(grid.cell * 0.5);
    let (a, b) = (centre(run[0]), centre(run[run.len() - 1]));
    let mid = (a + b) * 0.5;
    // The line 2 m in from the edge, across the whole run.
    let edge = mid - inward * (grid.cell * 0.5 - 2.0);
    let half = (b - a).dot(along).abs() * 0.5 + grid.cell * 0.5;
    let rot = Quat::from_rotation_y(inward.x.atan2(inward.y));
    let mut u = -half + 1.6;
    while u <= half - 1.5 {
        let p = edge + along * u;
        let y = ground_at(def, grid, terrain, p) + 0.43;
        let at = Vec3::new(p.x, y, p.y);
        out.decor.push(decor(Shape::Box { half: Vec3::new(1.5, 0.43, 0.3) }, at, rot, "#c9c2b4", true));
        out.decor.push(decor(Shape::Box { half: Vec3::new(1.3, 0.08, 0.31) }, at + Vec3::Y * 0.2, rot, "#e8722c", false));
        u += 3.05;
    }
    // At the side of the road, 4 m before the barriers, facing the drivers coming up to them.
    let s = edge + inward * 4.0 + along * (half - 2.5);
    sign(Vec3::new(s.x, ground_at(def, grid, terrain, s), s.y), Vec3::new(inward.x, 0.0, inward.y), words, out);
}
