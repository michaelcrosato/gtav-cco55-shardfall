//! Terrain (PLAN.md §6.6.1): ground heights painted as a character grid beside the map,
//! smoothed into a height field of 4 m samples. Cells with height become triangle meshes with
//! exact colliders — the ground of their zone, roads flat across and sloped along, kerbs where
//! they meet — and what stands on them (buildings, trees, lamps) is lifted onto the ground.
//! Cells whose corners are all at zero stay flat slabs, so the city's streets keep their
//! sidewalks and markings and meet the hills without a seam.

use glam::{IVec2, Vec2, Vec3};

use crate::def::DistrictDef;
use crate::grid::Grid;
use crate::ground::{Role, role};

/// Height samples along a map cell's side.
pub const PER: i32 = 4;

/// The height field.
pub struct Terrain {
    /// World (x, z) of sample (0, 0) and the spacing between samples (m).
    pub origin: Vec2,
    pub step: f32,
    /// Samples along x and z.
    pub nx: i32,
    pub nz: i32,
    h: Vec<f32>,
    /// Map cells with any height (they are meshes, not slabs).
    cells: Vec<bool>,
    gw: i32,
    gh: i32,
}

fn level(ch: char) -> Option<f32> {
    match ch {
        '0'..='9' => Some((ch as u32 - '0' as u32) as f32),
        'a'..='z' => Some((ch as u32 - 'a' as u32 + 10) as f32),
        '.' | ' ' => Some(0.0),
        _ => None,
    }
}

impl Terrain {
    /// Builds the height field from the district's `terrain` grid (flat without one).
    pub fn build(def: &DistrictDef, grid: &Grid) -> Result<Terrain, String> {
        let (gw, gh) = (grid.w, grid.h);
        let (nx, nz) = (gw * PER + 1, gh * PER + 1);
        let mut t = Terrain {
            origin: grid.origin,
            step: grid.cell / PER as f32,
            nx,
            nz,
            h: vec![0.0; (nx * nz) as usize],
            cells: vec![false; (gw * gh) as usize],
            gw,
            gh,
        };
        let Some(text) = &def.terrain else { return Ok(t) };
        let rows: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
        if rows.len() != gh as usize {
            return Err(format!("terrain: {} rows, the map has {gh}", rows.len()));
        }
        let mut lv = vec![0.0f32; (gw * gh) as usize];
        for (z, row) in rows.iter().enumerate() {
            let chars: Vec<char> = row.chars().collect();
            if chars.len() != gw as usize {
                return Err(format!("terrain row {z}: {} characters, the map has {gw}", chars.len()));
            }
            for (x, ch) in chars.iter().enumerate() {
                let l = level(*ch).ok_or_else(|| format!("terrain row {z}: '{ch}' is not a height (0-9, a-z)"))?;
                lv[z * gw as usize + x] = l * def.terrain_step;
            }
        }
        let cell = |x: i32, z: i32| lv[(z.clamp(0, gh - 1) * gw + x.clamp(0, gw - 1)) as usize];
        // Corner heights: the mean of the cells around each corner.
        let corner = |i: i32, j: i32| (cell(i - 1, j - 1) + cell(i, j - 1) + cell(i - 1, j) + cell(i, j)) * 0.25;
        let flat_cell = |x: i32, z: i32| {
            corner(x, z) == 0.0 && corner(x + 1, z) == 0.0 && corner(x, z + 1) == 0.0 && corner(x + 1, z + 1) == 0.0
        };
        // Samples: bilinear between corners; fixed where they touch a flat cell.
        let mut fixed = vec![false; t.h.len()];
        for sz in 0..nz {
            for sx in 0..nx {
                let (cx, cz) = ((sx / PER).min(gw - 1), (sz / PER).min(gh - 1));
                let (u, v) = ((sx - cx * PER) as f32 / PER as f32, (sz - cz * PER) as f32 / PER as f32);
                let a = corner(cx, cz) * (1.0 - u) + corner(cx + 1, cz) * u;
                let b = corner(cx, cz + 1) * (1.0 - u) + corner(cx + 1, cz + 1) * u;
                let i = (sz * nx + sx) as usize;
                t.h[i] = a * (1.0 - v) + b * v;
                // The cells this sample touches (up to four).
                let xs = if sx % PER == 0 { [sx / PER - 1, sx / PER] } else { [sx / PER, sx / PER] };
                let zs = if sz % PER == 0 { [sz / PER - 1, sz / PER] } else { [sz / PER, sz / PER] };
                fixed[i] =
                    xs.iter().any(|&x| zs.iter().any(|&z| (0..gw).contains(&x) && (0..gh).contains(&z) && flat_cell(x, z)));
            }
        }
        // Smooth the slopes (the flat city stays exactly flat).
        for _ in 0..2 {
            t.blur(&fixed);
        }
        // Roads: flat across, sloped along; junctions level.
        let roads: Vec<IVec2> = (0..gh)
            .flat_map(|z| (0..gw).map(move |x| IVec2::new(x, z)))
            .filter(|c| grid.road(*c).is_some() && !flat_cell(c.x, c.y))
            .collect();
        for &c in &roads {
            match role(grid, c) {
                Role::Ew { k, i } => {
                    let (z0, z1) = ((c.y - i) * PER, (c.y - i + k) * PER);
                    for sx in c.x * PER..=(c.x + 1) * PER {
                        t.level_span(&fixed, (sx, sx), (z0, z1));
                    }
                }
                Role::Ns { k, i } => {
                    let (x0, x1) = ((c.x - i) * PER, (c.x - i + k) * PER);
                    for sz in c.y * PER..=(c.y + 1) * PER {
                        t.level_span(&fixed, (x0, x1), (sz, sz));
                    }
                }
                Role::Junction | Role::Stub => {}
            }
        }
        for &c in &roads {
            if matches!(role(grid, c), Role::Junction | Role::Stub) {
                t.level_span(&fixed, (c.x * PER, (c.x + 1) * PER), (c.y * PER, (c.y + 1) * PER));
            }
        }
        for l in def.landmark.iter().filter(|l| l.level) {
            let [[x0, z0], [x1, z1]] = l.cells;
            t.level_span(&fixed, (x0 * PER, (x1 + 1) * PER), (z0 * PER, (z1 + 1) * PER));
        }
        for z in 0..gh {
            for x in 0..gw {
                let mut any = false;
                for sz in z * PER..=(z + 1) * PER {
                    for sx in x * PER..=(x + 1) * PER {
                        any |= t.h[(sz * nx + sx) as usize].abs() > 0.005;
                    }
                }
                t.cells[(z * gw + x) as usize] = any;
            }
        }
        Ok(t)
    }

    /// One pass of a 5-tap binomial blur on the samples that are not fixed.
    fn blur(&mut self, fixed: &[bool]) {
        const W: [f32; 5] = [1.0, 4.0, 6.0, 4.0, 1.0];
        let (nx, nz) = (self.nx, self.nz);
        for axis in 0..2 {
            let src = self.h.clone();
            for sz in 0..nz {
                for sx in 0..nx {
                    let i = (sz * nx + sx) as usize;
                    if fixed[i] {
                        continue;
                    }
                    let (mut sum, mut wsum) = (0.0, 0.0);
                    for (k, w) in W.iter().enumerate() {
                        let d = k as i32 - 2;
                        let (x, z) = if axis == 0 { (sx + d, sz) } else { (sx, sz + d) };
                        if x < 0 || z < 0 || x >= nx || z >= nz {
                            continue;
                        }
                        sum += src[(z * nx + x) as usize] * w;
                        wsum += w;
                    }
                    self.h[i] = sum / wsum;
                }
            }
        }
    }

    /// Sets the samples in an inclusive rectangle to their mean (fixed ones keep their value).
    fn level_span(&mut self, fixed: &[bool], (x0, x1): (i32, i32), (z0, z1): (i32, i32)) {
        let (mut sum, mut n) = (0.0, 0);
        let idx = |x: i32, z: i32| (z.clamp(0, self.nz - 1) * self.nx + x.clamp(0, self.nx - 1)) as usize;
        for z in z0..=z1 {
            for x in x0..=x1 {
                if !fixed[idx(x, z)] {
                    sum += self.h[idx(x, z)];
                    n += 1;
                }
            }
        }
        if n == 0 {
            return;
        }
        let mean = sum / n as f32;
        for z in z0..=z1 {
            for x in x0..=x1 {
                if !fixed[idx(x, z)] {
                    self.h[idx(x, z)] = mean;
                }
            }
        }
    }

    /// The sample at (sx, sz), clamped to the field.
    pub fn sample(&self, sx: i32, sz: i32) -> f32 {
        self.h[(sz.clamp(0, self.nz - 1) * self.nx + sx.clamp(0, self.nx - 1)) as usize]
    }

    /// World position (x, z) of a sample.
    pub fn sample_pos(&self, sx: i32, sz: i32) -> Vec2 {
        self.origin + Vec2::new(sx as f32, sz as f32) * self.step
    }

    /// Height above the flat city at a world point (bilinear between samples; clamped at the
    /// edges).
    pub fn height_at(&self, p: Vec2) -> f32 {
        let q = (p - self.origin) / self.step;
        let (fx, fz) = (q.x.floor(), q.y.floor());
        let (u, v) = (q.x - fx, q.y - fz);
        let (sx, sz) = (fx as i32, fz as i32);
        let a = self.sample(sx, sz) * (1.0 - u) + self.sample(sx + 1, sz) * u;
        let b = self.sample(sx, sz + 1) * (1.0 - u) + self.sample(sx + 1, sz + 1) * u;
        a * (1.0 - v) + b * v
    }

    /// The surface normal at a sample (central differences).
    pub fn normal(&self, sx: i32, sz: i32) -> Vec3 {
        let dx = (self.sample(sx + 1, sz) - self.sample(sx - 1, sz)) / (2.0 * self.step);
        let dz = (self.sample(sx, sz + 1) - self.sample(sx, sz - 1)) / (2.0 * self.step);
        Vec3::new(-dx, 1.0, -dz).normalize()
    }

    /// The surface normal at a world point (from the nearest sample).
    pub fn normal_at(&self, p: Vec2) -> Vec3 {
        let q = ((p - self.origin) / self.step).round();
        self.normal(q.x as i32, q.y as i32)
    }

    /// Whether map cell `c` has height (drawn as a mesh).
    pub fn is_terrain(&self, c: IVec2) -> bool {
        c.x >= 0 && c.y >= 0 && c.x < self.gw && c.y < self.gh && self.cells[(c.y * self.gw + c.x) as usize]
    }

    /// The lowest and highest ground in a rectangle (samples inside it and its corners).
    pub fn range(&self, min: Vec2, max: Vec2) -> (f32, f32) {
        let mut lo = f32::MAX;
        let mut hi = f32::MIN;
        for p in [min, max, Vec2::new(min.x, max.y), Vec2::new(max.x, min.y), (min + max) * 0.5] {
            let h = self.height_at(p);
            lo = lo.min(h);
            hi = hi.max(h);
        }
        let a = ((min - self.origin) / self.step).ceil();
        let b = ((max - self.origin) / self.step).floor();
        for sz in a.y as i32..=b.y as i32 {
            for sx in a.x as i32..=b.x as i32 {
                let h = self.sample(sx, sz);
                lo = lo.min(h);
                hi = hi.max(h);
            }
        }
        (lo, hi)
    }

    /// The highest ground anywhere.
    pub fn max_height(&self) -> f32 {
        self.h.iter().copied().fold(0.0, f32::max)
    }
}
