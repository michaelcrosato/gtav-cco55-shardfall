//! Cover points (PLAN.md §6.6.6): spots to crouch behind, generated along building walls that
//! face open ground (high cover), and along the storm channel's low walls and the freeway's
//! barriers (low cover), every 1.5 m, each with the direction away from its wall. Stopped
//! vehicles add their own at runtime (the game's job).

use std::collections::BTreeMap;

use glam::{Vec2, Vec3};

use crate::channel::ChannelGeom;
use crate::freeway::{FreewayGeom, HALF};
use crate::walk::{Walk, WalkMap};
use crate::{Building, ground};

#[derive(Clone, Copy, Debug)]
pub struct CoverPoint {
    /// Where to stand (on the ground) and the horizontal direction away from the wall.
    pub pos: Vec3,
    pub normal: Vec2,
    /// Tall enough to stand behind (else crouch).
    pub high: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Cover {
    pub points: Vec<CoverPoint>,
    index: BTreeMap<(i32, i32), Vec<u32>>,
}

const CELL: f32 = 16.0;
const SPACING: f32 = 1.5;

impl Cover {
    /// Cover along `buildings` and the `extra` boxes (low walls, big props) where they face
    /// walkable ground, the channels' walls and the freeways' barriers.
    pub fn build(
        buildings: &[Building],
        extra: &[Building],
        walk: &WalkMap,
        channels: &[ChannelGeom],
        freeways: &[FreewayGeom],
    ) -> Cover {
        let mut c = Cover::default();
        for b in buildings.iter().chain(extra) {
            let edges = [
                (Vec2::new(b.min.x, b.min.y), Vec2::new(b.max.x, b.min.y), Vec2::NEG_Y),
                (Vec2::new(b.max.x, b.min.y), Vec2::new(b.max.x, b.max.y), Vec2::X),
                (Vec2::new(b.min.x, b.max.y), Vec2::new(b.max.x, b.max.y), Vec2::Y),
                (Vec2::new(b.min.x, b.min.y), Vec2::new(b.min.x, b.max.y), Vec2::NEG_X),
            ];
            for (a, e, n) in edges {
                let len = a.distance(e);
                if len < 1.6 {
                    continue;
                }
                let mut t = 0.9;
                while t <= len - 0.9 {
                    let p = a + (e - a) * (t / len) + n * 1.0;
                    // Only where people can stand: sidewalks, plazas, paths.
                    if walk.at(p) != Walk::Blocked {
                        c.points.push(CoverPoint { pos: Vec3::new(p.x, b.base, p.y), normal: n, high: b.height > 1.8 });
                    }
                    t += SPACING;
                }
            }
        }
        // The Ditch's low walls, from the street side.
        for ch in channels {
            let mut z = ch.z0 + 1.0;
            while z < ch.z1 - 1.0 {
                for (x, n) in [(ch.x0 - 0.6, Vec2::NEG_X), (ch.x1 + 0.6, Vec2::X)] {
                    let p = Vec2::new(x, z);
                    if walk.at(p) != Walk::Blocked {
                        c.points.push(CoverPoint { pos: Vec3::new(x, ground::LOT_Y, z), normal: n, high: false });
                    }
                }
                z += SPACING;
            }
        }
        // Freeway barriers, from the lanes beside them (every other step: they are long).
        for f in freeways {
            let d = &f.deck;
            for i in (0..d.len()).step_by(1) {
                for s in [-1.0f32, 1.0] {
                    let p = d.at(i, s * (HALF - 1.4), 0.0);
                    let n = -(d.right[i] * s);
                    c.points.push(CoverPoint { pos: p, normal: Vec2::new(n.x, n.z), high: false });
                }
            }
        }
        for (i, p) in c.points.iter().enumerate() {
            let k = ((p.pos.x / CELL).floor() as i32, (p.pos.z / CELL).floor() as i32);
            c.index.entry(k).or_default().push(i as u32);
        }
        c
    }

    /// Every cover point within `radius` of `pos` (on about its level), nearest first, whichever
    /// way it faces: a corner shields from threats along the wall it ends, which the facing test
    /// of [`Cover::near`] does not see (callers judge those by lines of sight).
    pub fn within(&self, pos: Vec3, radius: f32) -> Vec<CoverPoint> {
        let r = (radius / CELL).ceil() as i32;
        let (cx, cz) = ((pos.x / CELL).floor() as i32, (pos.z / CELL).floor() as i32);
        let mut out: Vec<(f32, CoverPoint)> = Vec::new();
        for z in cz - r..=cz + r {
            for x in cx - r..=cx + r {
                for &i in self.index.get(&(x, z)).map(|v| v.as_slice()).unwrap_or(&[]) {
                    let p = self.points[i as usize];
                    let d = p.pos.distance(pos);
                    if d <= radius && (p.pos.y - pos.y).abs() < 3.0 {
                        out.push((d, p));
                    }
                }
            }
        }
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        out.into_iter().map(|(_, p)| p).collect()
    }

    /// Cover points within `radius` of `pos` that shield from `threat` (the wall between),
    /// nearest first.
    pub fn near(&self, pos: Vec3, threat: Vec3, radius: f32) -> Vec<CoverPoint> {
        let r = (radius / CELL).ceil() as i32;
        let (cx, cz) = ((pos.x / CELL).floor() as i32, (pos.z / CELL).floor() as i32);
        let mut out: Vec<(f32, CoverPoint)> = Vec::new();
        for z in cz - r..=cz + r {
            for x in cx - r..=cx + r {
                for &i in self.index.get(&(x, z)).map(|v| v.as_slice()).unwrap_or(&[]) {
                    let p = self.points[i as usize];
                    let d = p.pos.distance(pos);
                    let to_threat = Vec2::new(threat.x - p.pos.x, threat.z - p.pos.z);
                    if d <= radius && to_threat.dot(p.normal) < 0.0 && (p.pos.y - pos.y).abs() < 3.0 {
                        out.push((d, p));
                    }
                }
            }
        }
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        out.into_iter().map(|(_, p)| p).collect()
    }
}
