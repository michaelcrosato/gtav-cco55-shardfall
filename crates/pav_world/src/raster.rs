//! The map picture (PLAN.md §6.6.11): 1 px per metre, a dark night-map palette — land, roads by
//! class, parks, sand, water, building footprints — shared by the radar and the pause map.

use glam::Vec2;
use pav_core::present::MapImage;

use crate::Building;
use crate::def::{DistrictDef, RoadClass};
use crate::grid::{Cell, Grid};

fn rgb(hex: u32) -> [u8; 4] {
    [(hex >> 16) as u8, (hex >> 8) as u8, hex as u8, 255]
}

pub fn draw(grid: &Grid, buildings: &[Building], def: &DistrictDef) -> MapImage {
    let mpp = 1.0;
    let w = (grid.w as f32 * grid.cell / mpp) as u32;
    let h = (grid.h as f32 * grid.cell / mpp) as u32;
    let mut px = vec![0u8; (w * h * 4) as usize];
    let land = rgb(0x1d242c);
    for y in 0..h {
        for x in 0..w {
            let p = grid.origin + Vec2::new(x as f32 + 0.5, y as f32 + 0.5) * mpp;
            let c = grid.cell_of(p);
            let color = match grid.at(c) {
                Some(Cell::Road(r)) => match r {
                    RoadClass::Avenue | RoadClass::Boulevard => rgb(0x77818d),
                    RoadClass::Street => rgb(0x66707b),
                    RoadClass::Alley => rgb(0x4b545e),
                    RoadClass::Channel => rgb(0x5a6f7a),
                    RoadClass::Path => rgb(0x6e6450),
                },
                Some(Cell::Zone(z)) => match def.zones[&grid.zones[z as usize]].ground.as_str() {
                    "grass" => rgb(0x2c4a2e),
                    "sand" => rgb(0x7d6f50),
                    "water" => rgb(0x1b3b5a),
                    _ => land,
                },
                None => land,
            };
            let i = ((y * w + x) * 4) as usize;
            px[i..i + 4].copy_from_slice(&color);
        }
    }
    let shade = rgb(0x323b46);
    for b in buildings {
        let (x0, y0) = to_px(grid, b.min, mpp);
        let (x1, y1) = to_px(grid, b.max, mpp);
        for y in y0.max(0)..y1.min(h as i32) {
            for x in x0.max(0)..x1.min(w as i32) {
                let i = ((y as u32 * w + x as u32) * 4) as usize;
                px[i..i + 4].copy_from_slice(&shade);
            }
        }
    }
    MapImage { width: w, height: h, rgba: px, origin: grid.origin, metres_per_px: mpp, version: 1 }
}

fn to_px(grid: &Grid, p: Vec2, mpp: f32) -> (i32, i32) {
    let q = (p - grid.origin) / mpp;
    (q.x.floor() as i32, q.y.floor() as i32)
}
