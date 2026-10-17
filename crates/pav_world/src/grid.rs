//! The macro map as cells: each a road of some class or a zone; roads' connectivity (runs,
//! junctions) and blocks (connected zone cells between roads).

use glam::{IVec2, Vec2};

use crate::def::{DistrictDef, RoadClass};

/// One map cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cell {
    Road(RoadClass),
    /// Index into `Grid::zones`.
    Zone(u16),
}

pub struct Grid {
    pub w: i32,
    pub h: i32,
    pub cell: f32,
    pub origin: Vec2,
    pub cells: Vec<Cell>,
    /// Zone names by index (`Cell::Zone`).
    pub zones: Vec<String>,
    /// Cells a landmark owns (its index), else None.
    pub landmark: Vec<Option<u16>>,
}

impl Grid {
    pub fn parse(def: &DistrictDef) -> Result<Grid, String> {
        let rows: Vec<&str> = def.map.lines().map(|l| l.trim_end()).filter(|l| !l.is_empty()).collect();
        if rows.is_empty() {
            return Err("map: no rows".into());
        }
        let w = rows[0].chars().count() as i32;
        let h = rows.len() as i32;
        let mut zones: Vec<String> = Vec::new();
        let mut cells = Vec::with_capacity((w * h) as usize);
        for (z, row) in rows.iter().enumerate() {
            if row.chars().count() as i32 != w {
                return Err(format!("map row {}: {} characters, the first row has {w}", z + 1, row.chars().count()));
            }
            for (x, ch) in row.chars().enumerate() {
                let key = ch.to_string();
                let Some(entry) = def.legend.get(&key) else {
                    return Err(format!("map row {}, column {}: '{ch}' is not in [legend]", z + 1, x + 1));
                };
                let cell = match (&entry.zone, entry.road) {
                    (Some(zone), None) => {
                        if !def.zones.contains_key(zone) {
                            return Err(format!("legend '{ch}': zone '{zone}' has no [zones.{zone}]"));
                        }
                        let i = zones.iter().position(|n| n == zone).unwrap_or_else(|| {
                            zones.push(zone.clone());
                            zones.len() - 1
                        });
                        Cell::Zone(i as u16)
                    }
                    (None, Some(road)) => Cell::Road(road),
                    _ => return Err(format!("legend '{ch}': give exactly one of zone or road")),
                };
                cells.push(cell);
            }
        }
        let mut landmark = vec![None; (w * h) as usize];
        for (i, l) in def.landmark.iter().enumerate() {
            let [[x0, z0], [x1, z1]] = l.cells;
            if x0 > x1 || z0 > z1 || x0 < 0 || z0 < 0 || x1 >= w || z1 >= h {
                return Err(format!("landmark '{}': cells {:?} outside the {w} x {h} map", l.key, l.cells));
            }
            for z in z0..=z1 {
                for x in x0..=x1 {
                    landmark[(z * w + x) as usize] = Some(i as u16);
                }
            }
        }
        Ok(Grid { w, h, cell: def.cell, origin: Vec2::new(def.origin[0], def.origin[1]), cells, zones, landmark })
    }

    pub fn inside(&self, c: IVec2) -> bool {
        c.x >= 0 && c.y >= 0 && c.x < self.w && c.y < self.h
    }

    pub fn at(&self, c: IVec2) -> Option<Cell> {
        self.inside(c).then(|| self.cells[(c.y * self.w + c.x) as usize])
    }

    pub fn road(&self, c: IVec2) -> Option<RoadClass> {
        match self.at(c) {
            Some(Cell::Road(r)) => Some(r),
            _ => None,
        }
    }

    pub fn zone(&self, c: IVec2) -> Option<&str> {
        match self.at(c) {
            Some(Cell::Zone(i)) => Some(&self.zones[i as usize]),
            _ => None,
        }
    }

    pub fn landmark_at(&self, c: IVec2) -> Option<u16> {
        if self.inside(c) { self.landmark[(c.y * self.w + c.x) as usize] } else { None }
    }

    /// World (x, z) of a cell's north-west corner.
    pub fn corner(&self, c: IVec2) -> Vec2 {
        self.origin + Vec2::new(c.x as f32, c.y as f32) * self.cell
    }

    /// The cell holding a world point.
    pub fn cell_of(&self, p: Vec2) -> IVec2 {
        let q = (p - self.origin) / self.cell;
        IVec2::new(q.x.floor() as i32, q.y.floor() as i32)
    }

    /// Which of the four neighbours (north, east, south, west) are drivable roads.
    pub fn road_sides(&self, c: IVec2) -> [bool; 4] {
        DIRS.map(|d| self.road(c + d).is_some_and(|r| r != RoadClass::Path))
    }

    /// Connected zone cells of the same zone (4-neighbours), as lists of cells; landmark cells
    /// are left out (the landmark builds there).
    pub fn blocks(&self) -> Vec<(u16, Vec<IVec2>)> {
        let mut seen = vec![false; self.cells.len()];
        let mut out = Vec::new();
        for z in 0..self.h {
            for x in 0..self.w {
                let i = (z * self.w + x) as usize;
                let Cell::Zone(zone) = self.cells[i] else { continue };
                if seen[i] || self.landmark[i].is_some() {
                    continue;
                }
                let mut stack = vec![IVec2::new(x, z)];
                seen[i] = true;
                let mut cells = Vec::new();
                while let Some(c) = stack.pop() {
                    cells.push(c);
                    for d in DIRS {
                        let n = c + d;
                        if !self.inside(n) {
                            continue;
                        }
                        let j = (n.y * self.w + n.x) as usize;
                        if !seen[j] && self.cells[j] == Cell::Zone(zone) && self.landmark[j].is_none() {
                            seen[j] = true;
                            stack.push(n);
                        }
                    }
                }
                cells.sort_by_key(|c| (c.y, c.x));
                out.push((zone, cells));
            }
        }
        out
    }
}

/// North (-z), east (+x), south (+z), west (-x).
pub const DIRS: [IVec2; 4] = [IVec2::new(0, -1), IVec2::new(1, 0), IVec2::new(0, 1), IVec2::new(-1, 0)];

/// Splits a set of cells into rectangles (greedy: widest run first, then grown downward), so a
/// block of any shape becomes a few rectangles to put lots in.
pub fn rectangles(cells: &[IVec2]) -> Vec<(IVec2, IVec2)> {
    use std::collections::BTreeSet;
    let mut left: BTreeSet<(i32, i32)> = cells.iter().map(|c| (c.y, c.x)).collect();
    let mut out = Vec::new();
    while let Some(&(z, x)) = left.iter().next() {
        // Run east.
        let mut x1 = x;
        while left.contains(&(z, x1 + 1)) {
            x1 += 1;
        }
        // Grow south while the whole run is there.
        let mut z1 = z;
        while (x..=x1).all(|xx| left.contains(&(z1 + 1, xx))) {
            z1 += 1;
        }
        for zz in z..=z1 {
            for xx in x..=x1 {
                left.remove(&(zz, xx));
            }
        }
        out.push((IVec2::new(x, z), IVec2::new(x1, z1)));
    }
    out
}
