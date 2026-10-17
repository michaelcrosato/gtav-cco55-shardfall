//! The map picture behind the radar and the pause map (`MapImage`): for now drawn from the
//! level's blocks (ground, kerbs, walls); the city generator's raster replaces it in M2.

use glam::Vec2;
use pav_core::present::MapImage;
use pav_core::sim::Sim;

/// Pixels per metre.
const PX: f32 = 4.0;

fn srgb8(c: f32) -> u8 {
    let s = if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
    (s.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Draws the loaded blocks top-down, darkened like a night map, with a margin around them.
pub fn raster(sim: &Sim, version: u64) -> MapImage {
    let blocks: Vec<_> = sim.state.statics.chunks.values().flat_map(|c| c.blocks.iter()).filter(|b| b.alive).collect();
    let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
    for b in &blocks {
        lo = lo.min(Vec2::new(b.min.x, b.min.z));
        hi = hi.max(Vec2::new(b.max.x, b.max.z));
    }
    if blocks.is_empty() {
        (lo, hi) = (Vec2::splat(-16.0), Vec2::splat(16.0));
    }
    let margin = 24.0;
    lo -= Vec2::splat(margin);
    hi += Vec2::splat(margin);
    let (w, h) = (((hi.x - lo.x) * PX).ceil() as u32, ((hi.y - lo.y) * PX).ceil() as u32);
    // Open ground around the level: the map's dark land colour.
    let mut rgba = [21u8, 26, 33, 255].repeat((w * h) as usize);
    // Lower blocks first, so walls paint over the ground they stand on.
    let mut sorted = blocks;
    sorted.sort_by(|a, b| a.max.y.total_cmp(&b.max.y));
    for b in sorted {
        let tall = b.max.y > 0.3;
        let c = b.color.0;
        // Ground keeps a dim tint of its colour; walls and kerbs read brighter.
        let k = if tall { 0.9 } else { 0.55 };
        let px = [srgb8(c[0] * k), srgb8(c[1] * k), srgb8(c[2] * k), 255];
        let x0 = ((b.min.x - lo.x) * PX).floor().max(0.0) as u32;
        let x1 = (((b.max.x - lo.x) * PX).ceil() as u32).min(w);
        let y0 = ((b.min.z - lo.y) * PX).floor().max(0.0) as u32;
        let y1 = (((b.max.z - lo.y) * PX).ceil() as u32).min(h);
        for y in y0..y1 {
            for x in x0..x1 {
                let i = ((y * w + x) * 4) as usize;
                rgba[i..i + 4].copy_from_slice(&px);
            }
        }
    }
    MapImage { width: w, height: h, rgba, origin: lo, metres_per_px: 1.0 / PX, version }
}
