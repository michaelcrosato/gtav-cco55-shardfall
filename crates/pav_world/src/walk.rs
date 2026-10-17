//! Where people walk (PLAN.md §6.6.7): a map of 2 m tiles over the district — sidewalks,
//! crosswalks (each tied to its junction's walk signal), the boardwalk and pier, parks, plazas
//! and the beach — that pedestrians find their way over and that spawning samples. Tiles make
//! every sidewalk loop, corner and crossing one connected surface without a hand-built graph.

use std::collections::{BTreeMap, BinaryHeap};

use glam::{IVec2, Vec2};
use pav_core::rng::Rng;

use crate::Building;
use crate::def::{DistrictDef, RoadClass};
use crate::grid::{Cell, Grid};
use crate::ground::{Role, role, sidewalk_rects};
use crate::lanes::LaneGraph;

/// What a tile is for people on foot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Walk {
    Blocked,
    Sidewalk,
    /// A zebra crossing over a road that runs north-south (`ns`) or east-west, at a junction.
    Crossing {
        junction: u32,
        ns: bool,
    },
    /// Paths, the boardwalk and the pier.
    Path,
    /// Parks, plazas and the beach.
    Open,
}

/// The tile map.
#[derive(Clone, Debug, Default)]
pub struct WalkMap {
    pub origin: Vec2,
    pub tile: f32,
    pub w: i32,
    pub h: i32,
    /// 0 blocked, 1 sidewalk, 2 path, 3 open, 4 + i crossing i.
    tiles: Vec<u16>,
    crossings: Vec<(u32, bool)>,
}

const TILE: f32 = 2.0;

impl WalkMap {
    pub fn build(def: &DistrictDef, grid: &Grid, buildings: &[Building], lanes: &LaneGraph) -> WalkMap {
        let per = (grid.cell / TILE) as i32;
        let (w, h) = (grid.w * per, grid.h * per);
        let mut m = WalkMap { origin: grid.origin, tile: TILE, w, h, tiles: vec![0; (w * h) as usize], crossings: Vec::new() };
        // Parks, plazas, beach and paths first; sidewalks over them.
        for cz in 0..grid.h {
            for cx in 0..grid.w {
                let c = IVec2::new(cx, cz);
                let v = match grid.at(c).unwrap() {
                    Cell::Road(RoadClass::Path) => 2,
                    Cell::Zone(z) => match def.zones[&grid.zones[z as usize]].ground.as_str() {
                        "grass" | "sand" | "plaza" => 3,
                        _ => 0,
                    },
                    _ => 0,
                };
                if v != 0 {
                    let lo = grid.corner(c);
                    m.fill(lo, lo + Vec2::splat(grid.cell), v);
                }
            }
        }
        // Buildings stand on plazas and parks.
        for b in buildings {
            m.fill(b.min, b.max, 0);
        }
        for cz in 0..grid.h {
            for cx in 0..grid.w {
                for (a, b) in sidewalk_rects(grid, IVec2::new(cx, cz)) {
                    m.fill(a, b, 1);
                }
            }
        }
        // Crossings: the zebra strip of each band cell beside a junction, 1–4 m from its edge.
        let mut junction_of: BTreeMap<(i32, i32), u32> = BTreeMap::new();
        for (j, jn) in lanes.junctions.iter().enumerate() {
            for c in &jn.cells {
                junction_of.insert((c.x, c.y), j as u32);
            }
        }
        for cz in 0..grid.h {
            for cx in 0..grid.w {
                let c = IVec2::new(cx, cz);
                let (ns, d) = match role(grid, c) {
                    Role::Ew { .. } => (false, IVec2::X),
                    Role::Ns { .. } => (true, IVec2::Y),
                    _ => continue,
                };
                if grid.road(c).is_none_or(|r| r == RoadClass::Channel) {
                    continue;
                }
                let lo = grid.corner(c);
                let hi = lo + Vec2::splat(grid.cell);
                for s in [d, -d] {
                    let Some(&j) = junction_of.get(&((c + s).x, (c + s).y)) else { continue };
                    let (a0, a1) = match (s.x + s.y > 0, ns) {
                        (true, false) => (hi.x - 4.0, hi.x - 1.0),
                        (false, false) => (lo.x + 1.0, lo.x + 4.0),
                        (true, true) => (hi.y - 4.0, hi.y - 1.0),
                        (false, true) => (lo.y + 1.0, lo.y + 4.0),
                    };
                    let (a, b) =
                        if ns { (Vec2::new(lo.x, a0), Vec2::new(hi.x, a1)) } else { (Vec2::new(a0, lo.y), Vec2::new(a1, hi.y)) };
                    let id = m.crossings.iter().position(|x| *x == (j, ns)).unwrap_or_else(|| {
                        m.crossings.push((j, ns));
                        m.crossings.len() - 1
                    });
                    m.fill_if(a, b, 4 + id as u16, |v| v == 0);
                }
            }
        }
        m
    }

    fn fill(&mut self, a: Vec2, b: Vec2, v: u16) {
        self.fill_if(a, b, v, |_| true);
    }

    /// Sets tiles whose centres lie inside the rectangle.
    fn fill_if(&mut self, a: Vec2, b: Vec2, v: u16, ok: impl Fn(u16) -> bool) {
        let t0 = ((a - self.origin) / self.tile - Vec2::splat(0.5)).ceil();
        let t1 = ((b - self.origin) / self.tile - Vec2::splat(0.5)).floor();
        for z in (t0.y as i32).max(0)..=(t1.y as i32).min(self.h - 1) {
            for x in (t0.x as i32).max(0)..=(t1.x as i32).min(self.w - 1) {
                let i = (z * self.w + x) as usize;
                if ok(self.tiles[i]) {
                    self.tiles[i] = v;
                }
            }
        }
    }

    /// The tile under a world point (x, z).
    pub fn tile_of(&self, p: Vec2) -> IVec2 {
        let q = ((p - self.origin) / self.tile).floor();
        IVec2::new(q.x as i32, q.y as i32)
    }

    /// A tile's centre.
    pub fn centre(&self, t: IVec2) -> Vec2 {
        self.origin + (t.as_vec2() + Vec2::splat(0.5)) * self.tile
    }

    pub fn get(&self, t: IVec2) -> Walk {
        if t.x < 0 || t.y < 0 || t.x >= self.w || t.y >= self.h {
            return Walk::Blocked;
        }
        match self.tiles[(t.y * self.w + t.x) as usize] {
            0 => Walk::Blocked,
            1 => Walk::Sidewalk,
            2 => Walk::Path,
            3 => Walk::Open,
            i => {
                let (junction, ns) = self.crossings[(i - 4) as usize];
                Walk::Crossing { junction, ns }
            }
        }
    }

    /// What is at a world point.
    pub fn at(&self, p: Vec2) -> Walk {
        self.get(self.tile_of(p))
    }

    pub fn walkable(&self, t: IVec2) -> bool {
        self.get(t) != Walk::Blocked
    }

    /// How many tiles of each kind: (sidewalk, crossing, path, open).
    pub fn counts(&self) -> (usize, usize, usize, usize) {
        let mut c = (0, 0, 0, 0);
        for &v in &self.tiles {
            match v {
                0 => {}
                1 => c.0 += 1,
                2 => c.2 += 1,
                3 => c.3 += 1,
                _ => c.1 += 1,
            }
        }
        c
    }

    /// A random tile of a kind `keep` accepts (`None` after many misses).
    pub fn random(&self, rng: &mut Rng, keep: impl Fn(Walk) -> bool) -> Option<Vec2> {
        for _ in 0..400 {
            let t = IVec2::new(rng.below(self.w as u32) as i32, rng.below(self.h as u32) as i32);
            if keep(self.get(t)) {
                return Some(self.centre(t));
            }
        }
        None
    }

    /// A* over walkable tiles (8 neighbours, no corner cutting), at most `budget` tiles opened:
    /// the tile centres from `from` to `to`, or empty.
    pub fn path(&self, from: Vec2, to: Vec2, budget: usize) -> Vec<Vec2> {
        let (a, b) = (self.tile_of(from), self.tile_of(to));
        if !self.walkable(a) || !self.walkable(b) {
            return Vec::new();
        }
        #[derive(PartialEq)]
        struct Node(f32, i32, i32);
        impl Eq for Node {}
        impl PartialOrd for Node {
            fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(o))
            }
        }
        impl Ord for Node {
            fn cmp(&self, o: &Self) -> std::cmp::Ordering {
                o.0.total_cmp(&self.0).then((o.1, o.2).cmp(&(self.1, self.2)))
            }
        }
        let h = |t: IVec2| (t - b).as_vec2().length();
        let mut cost: BTreeMap<(i32, i32), f32> = BTreeMap::new();
        let mut came: BTreeMap<(i32, i32), (i32, i32)> = BTreeMap::new();
        let mut heap = BinaryHeap::new();
        cost.insert((a.x, a.y), 0.0);
        heap.push(Node(h(a), a.x, a.y));
        let mut opened = 0;
        while let Some(Node(_, x, z)) = heap.pop() {
            let t = IVec2::new(x, z);
            if t == b {
                let mut out = vec![self.centre(b)];
                let mut cur = (b.x, b.y);
                while let Some(&p) = came.get(&cur) {
                    out.push(self.centre(IVec2::new(p.0, p.1)));
                    cur = p;
                }
                out.reverse();
                return out;
            }
            opened += 1;
            if opened > budget {
                break;
            }
            let c0 = cost[&(x, z)];
            for dz in -1..=1 {
                for dx in -1..=1 {
                    if dx == 0 && dz == 0 {
                        continue;
                    }
                    let n = t + IVec2::new(dx, dz);
                    if !self.walkable(n)
                        || (dx != 0
                            && dz != 0
                            && (!self.walkable(t + IVec2::new(dx, 0)) || !self.walkable(t + IVec2::new(0, dz))))
                    {
                        continue;
                    }
                    // Prefer sidewalks and crossings over open ground.
                    let step = if dx != 0 && dz != 0 { 1.414 } else { 1.0 };
                    let k = match self.get(n) {
                        Walk::Open => 1.4,
                        _ => 1.0,
                    };
                    let c = c0 + step * k;
                    if cost.get(&(n.x, n.y)).is_none_or(|&old| c < old) {
                        cost.insert((n.x, n.y), c);
                        came.insert((n.x, n.y), (x, z));
                        heap.push(Node(c + h(n), n.x, n.y));
                    }
                }
            }
        }
        Vec::new()
    }
}
