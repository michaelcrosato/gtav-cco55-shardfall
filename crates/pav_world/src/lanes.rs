//! The lane graph (PLAN.md §6.6.2–6.6.3): every drivable lane in the district as a polyline in
//! its direction of travel — the street grid's bands (right-hand traffic), connectors through
//! junctions (straight, left, right; U-turns at dead ends), the freeway decks and ramps, the
//! storm channel's bed and ramps — with lane-change neighbours, signal plans that are a pure
//! function of time, parking spots along the curbs, a spatial index for nearest-lane queries and
//! A* routing.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BinaryHeap};

use glam::{IVec2, Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::channel::{BED_Y, ChannelGeom};
use crate::def::{RampKind, RoadClass};
use crate::freeway::{FreewayGeom, LANES};
use crate::grid::{DIRS, Grid};
use crate::ground::{ROAD_Y, Role, role};
use crate::terrain::Terrain;

/// What a lane belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaneKind {
    Street,
    Avenue,
    Boulevard,
    Alley,
    Channel,
    Freeway,
    Ramp,
    /// Through a junction (or a U-turn at a dead end).
    Connector,
}

/// A connector's manoeuvre.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Turn {
    None,
    Straight,
    Left,
    Right,
    UTurn,
}

/// A traffic light's state for one approach.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Light {
    Green,
    Amber,
    Red,
}

#[derive(Clone, Debug)]
pub struct Lane {
    /// Points in the direction of travel (the road surface).
    pub points: Vec<Vec3>,
    pub length: f32,
    pub kind: LaneKind,
    /// Speed limit (m/s) and width (m).
    pub speed: f32,
    pub width: f32,
    pub next: Vec<u32>,
    pub prev: Vec<u32>,
    /// Lane-change neighbours in the same direction (left and right of travel).
    pub left: Option<u32>,
    pub right: Option<u32>,
    /// Connectors: the junction they cross. Lanes ending at a junction: that junction.
    pub junction: Option<u32>,
    /// Lanes ending at a junction: the side they enter from (0 N, 1 E, 2 S, 3 W).
    pub approach: Option<u8>,
    /// Lanes ending at a signal or stop: where to stop, metres before the lane's end.
    pub stop: f32,
    pub turn: Turn,
    /// Lane index from the centre (0 = inner).
    pub index: u8,
    /// No ambient traffic (closed spur, channel bed, dead ends).
    pub closed: bool,
    /// Traffic enters or leaves the district here (a freeway lane at the map's edge).
    pub source: bool,
}

impl Lane {
    fn new(points: Vec<Vec3>, kind: LaneKind, speed: f32, width: f32) -> Lane {
        let length = points.windows(2).map(|w| w[0].distance(w[1])).sum();
        Lane {
            points,
            length,
            kind,
            speed,
            width,
            next: Vec::new(),
            prev: Vec::new(),
            left: None,
            right: None,
            junction: None,
            approach: None,
            stop: 0.0,
            turn: Turn::None,
            index: 0,
            closed: false,
            source: false,
        }
    }

    /// The point `d` metres along the lane (clamped) and the direction of travel there.
    pub fn at(&self, d: f32) -> (Vec3, Vec3) {
        let mut left = d.clamp(0.0, self.length);
        for w in self.points.windows(2) {
            let seg = w[0].distance(w[1]);
            if left <= seg || seg <= 0.0 {
                let t = if seg > 0.0 { left / seg } else { 0.0 };
                return (w[0] + (w[1] - w[0]) * t, (w[1] - w[0]).normalize_or(Vec3::Z));
            }
            left -= seg;
        }
        let n = self.points.len();
        let dir = if n >= 2 { (self.points[n - 1] - self.points[n - 2]).normalize_or(Vec3::Z) } else { Vec3::Z };
        (self.points[n - 1], dir)
    }

    /// The distance along the lane of the point nearest `p` and its distance from `p` (levels
    /// weigh three times: a deck above a street is far from it).
    pub fn project(&self, p: Vec3) -> (f32, f32) {
        let mut best = (0.0, f32::MAX);
        let mut along = 0.0;
        for w in self.points.windows(2) {
            let (a, b) = (w[0], w[1]);
            let ab = b - a;
            let len2 = ab.length_squared();
            let t = if len2 > 0.0 { ((p - a).dot(ab) / len2).clamp(0.0, 1.0) } else { 0.0 };
            let q = a + ab * t;
            let d = p - q;
            let dist = (d.x * d.x + d.z * d.z + 9.0 * d.y * d.y).sqrt();
            if dist < best.1 {
                best = (along + ab.length() * t, dist);
            }
            along += ab.length();
        }
        best
    }
}

/// A signal plan: north-south green, all red, east-west green, all red; shifted by `offset`.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct SignalPlan {
    pub ns_green: f32,
    pub ew_green: f32,
    pub amber: f32,
    pub all_red: f32,
    pub offset: f32,
}

impl SignalPlan {
    pub fn cycle(&self) -> f32 {
        self.ns_green + self.ew_green + 2.0 * (self.amber + self.all_red)
    }

    /// The light for traffic along `ns` (true: north-south) at time `t` (s).
    pub fn light(&self, ns: bool, t: f32) -> Light {
        let c = self.cycle();
        let t = (t + self.offset).rem_euclid(c);
        let ns_end = self.ns_green + self.amber;
        let ew_start = ns_end + self.all_red;
        let ew_end = ew_start + self.ew_green + self.amber;
        let (green0, green1) = if ns { (0.0, self.ns_green) } else { (ew_start, ew_start + self.ew_green) };
        let amber1 = green1 + self.amber;
        let _ = ew_end;
        if t >= green0 && t < green1 {
            Light::Green
        } else if t >= green1 && t < amber1 {
            Light::Amber
        } else {
            Light::Red
        }
    }

    /// Seconds of walking time left for people crossing a road that runs north-south (`ns`)
    /// at time `t`: they walk while the cross traffic has its green.
    pub fn walk(&self, ns: bool, t: f32) -> Option<f32> {
        let c = self.cycle();
        let t = (t + self.offset).rem_euclid(c);
        let ew_start = self.ns_green + self.amber + self.all_red;
        let (g0, g1) = if ns { (ew_start, ew_start + self.ew_green) } else { (0.0, self.ns_green) };
        (t >= g0 && t < g1 - 2.0).then_some(g1 - 2.0 - t)
    }
}

/// A junction: its cells, its bounds and centre, the sides with incoming lanes, its signal.
#[derive(Clone, Debug)]
pub struct Junction {
    pub cells: Vec<IVec2>,
    pub min: Vec2,
    pub max: Vec2,
    pub sides: [bool; 4],
    pub signal: Option<SignalPlan>,
}

impl Junction {
    pub fn centre(&self) -> Vec2 {
        (self.min + self.max) * 0.5
    }
}

/// A parking spot by the curb: where a car stands and the way it faces.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ParkingSpot {
    pub pos: Vec3,
    pub dir: Vec3,
    /// The travel lane beside it.
    pub lane: u32,
}

/// The lane graph.
#[derive(Clone, Debug, Default)]
pub struct LaneGraph {
    pub lanes: Vec<Lane>,
    pub junctions: Vec<Junction>,
    pub parking: Vec<ParkingSpot>,
    index: BTreeMap<(i32, i32), Vec<u32>>,
}

const INDEX_CELL: f32 = 16.0;

/// Lane centres from a band's centre line, inner first, and the parking lane's centre.
fn layout(class: RoadClass, k: i32) -> (&'static [f32], Option<f32>, f32) {
    match (class, k) {
        (RoadClass::Alley, _) => (&[1.5], None, 3.0),
        (RoadClass::Boulevard, _) => (&[2.3, 6.2], None, 3.6),
        (RoadClass::Avenue, _) | (_, 2) => (&[2.3, 6.2], Some(11.0), 3.6),
        _ => (&[1.8], Some(4.8), 3.6),
    }
}

fn kind_of(class: RoadClass) -> LaneKind {
    match class {
        RoadClass::Alley => LaneKind::Alley,
        RoadClass::Avenue => LaneKind::Avenue,
        RoadClass::Boulevard => LaneKind::Boulevard,
        RoadClass::Channel => LaneKind::Channel,
        _ => LaneKind::Street,
    }
}

/// A straight run of band cells between junctions or ends.
struct Segment {
    class: RoadClass,
    /// Along x (east-west) or z.
    ew: bool,
    /// Thickness in cells and the band's first row/column across.
    k: i32,
    across0: i32,
    /// First and last cell along.
    a: i32,
    b: i32,
}

/// How far short of the map's edge a road that runs off it ends (m): its turnaround and the
/// barriers beyond it fit in between.
const EDGE_PULL: f32 = 16.0;

/// How far short of a dead end (a lot, a building) a road's lanes end (m), so the U-turn there
/// stays on the road.
const DEAD_PULL: f32 = 11.0;

/// A turnaround a car can drive: a keyhole from the end of `p0` (heading `d0`), swinging out
/// toward the kerb and round (4 m either side of the centre line, 8.3 m on), back in to the start
/// of the lane the other way, `p2`.
fn turnaround(p0: Vec3, d0: Vec3, p2: Vec3) -> Vec<Vec3> {
    // (across from p0's side to p2's, along the way in), the ends at ±1.
    const SHAPE: [(f32, f32); 13] = [
        (-1.0, 0.0),
        (-1.45, 1.5),
        (-2.0, 3.0),
        (-2.22, 4.5),
        (-1.83, 6.5),
        (-1.0, 7.8),
        (0.0, 8.3),
        (1.0, 7.8),
        (1.83, 6.5),
        (2.22, 4.5),
        (2.0, 3.0),
        (1.45, 1.5),
        (1.0, 0.0),
    ];
    let mid = (p0 + p2) * 0.5;
    let half = (p2 - p0) * 0.5;
    // Across scales with the lanes' spacing up to 4 m out; along it is fixed.
    let unit = half.normalize_or_zero();
    let h = half.length();
    SHAPE
        .iter()
        .map(|&(x, y)| {
            mid + unit * if x.abs() <= 1.0 { x * h } else { x.signum() * (h + (x.abs() - 1.0) * (4.0 - h) / 1.22) } + d0 * y
        })
        .collect()
}

/// Points every ~4 m from `p` to `q` on the ground (terrain heights added).
fn line(p: Vec2, q: Vec2, terrain: &Terrain) -> Vec<Vec3> {
    let n = ((p.distance(q) / 4.0).ceil() as usize).max(1);
    (0..=n)
        .map(|i| {
            let t = i as f32 / n as f32;
            let x = p + (q - p) * t;
            Vec3::new(x.x, ROAD_Y + terrain.height_at(x), x.y)
        })
        .collect()
}

/// A quadratic curve from `p0` (heading `d0`) to `p2` (heading `d2`).
fn curve(p0: Vec3, d0: Vec3, p2: Vec3, d2: Vec3) -> Vec<Vec3> {
    let a = Vec2::new(p0.x, p0.z);
    let b = Vec2::new(p2.x, p2.z);
    let (u, v) = (Vec2::new(d0.x, d0.z), Vec2::new(d2.x, d2.z));
    // Where the two headings meet; the midpoint for (nearly) parallel ones.
    let den = u.perp_dot(v);
    let ctrl = if den.abs() > 0.2 {
        let t = (b - a).perp_dot(v) / den;
        let c = a + u * t;
        if t > 0.0 && t < a.distance(b) * 1.5 { c } else { (a + b) * 0.5 }
    } else {
        (a + b) * 0.5
    };
    let n = ((a.distance(b) / 2.5).ceil() as usize).clamp(4, 16);
    (0..=n)
        .map(|i| {
            let t = i as f32 / n as f32;
            let q = a * (1.0 - t) * (1.0 - t) + ctrl * 2.0 * t * (1.0 - t) + b * t * t;
            Vec3::new(q.x, p0.y + (p2.y - p0.y) * t, q.y)
        })
        .collect()
}

impl LaneGraph {
    pub fn build(grid: &Grid, terrain: &Terrain, freeways: &[FreewayGeom], channels: &[ChannelGeom], seed: u64) -> LaneGraph {
        let mut g = LaneGraph::default();
        let _ = seed;
        g.junctions = junctions(grid);
        let junction_at = {
            let mut m: BTreeMap<(i32, i32), u32> = BTreeMap::new();
            for (j, jn) in g.junctions.iter().enumerate() {
                for c in &jn.cells {
                    m.insert((c.x, c.y), j as u32);
                }
            }
            m
        };
        // Incoming and outgoing lanes at each junction side: (junction, side) -> lane ids.
        let mut incoming: BTreeMap<(u32, u8), Vec<u32>> = BTreeMap::new();
        let mut outgoing: BTreeMap<(u32, u8), Vec<u32>> = BTreeMap::new();
        // Dead ends: the lanes arriving at a band end with no junction, and the inner lane out.
        let mut dead: Vec<(Vec<u32>, u32)> = Vec::new();
        // Where the freeway's ramps come down to the streets (a road ending there carries on up
        // the ramp: it is no dead end to shorten).
        let ramp_feet: Vec<Vec2> = freeways
            .iter()
            .flat_map(|f| f.ramps.iter())
            .flat_map(|r| [r.path.points.first(), r.path.points.last()])
            .flatten()
            .filter(|p| p.y < ROAD_Y + 2.0)
            .map(|p| Vec2::new(p.x, p.z))
            .collect();
        for seg in segments(grid) {
            let (lanes, parking, width) = layout(seg.class, seg.k);
            let cs = grid.cell;
            let centre = grid.origin + Vec2::splat(seg.across0 as f32 * cs + seg.k as f32 * cs * 0.5);
            let (start, end) = (seg.a as f32 * cs, (seg.b + 1) as f32 * cs);
            // World points of the segment's centre line ends.
            let (p, q) = if seg.ew {
                (Vec2::new(grid.origin.x + start, centre.y), Vec2::new(grid.origin.x + end, centre.y))
            } else {
                (Vec2::new(centre.x, grid.origin.y + start), Vec2::new(centre.x, grid.origin.y + end))
            };
            let along = (q - p).normalize();
            let right = Vec2::new(-along.y, along.x);
            // The junctions (or none) at each end, by the cells just beyond.
            let beyond = |cell: i32| -> Option<u32> {
                let c = if seg.ew { IVec2::new(cell, seg.across0) } else { IVec2::new(seg.across0, cell) };
                junction_at.get(&(c.x, c.y)).copied()
            };
            let (ja, jb) = (beyond(seg.a - 1), beyond(seg.b + 1));
            // A road that runs off the map stops short of its edge (where it is closed off), so
            // its turnaround lies on the map.
            let last = if seg.ew { grid.w } else { grid.h };
            let ramp = |end: Vec2| ramp_feet.iter().any(|r| r.distance(end) < cs);
            let pull_a = if ja.is_some() || ramp(p) {
                0.0
            } else if seg.a == 0 {
                EDGE_PULL
            } else {
                DEAD_PULL
            };
            let pull_b = if jb.is_some() || ramp(q) {
                0.0
            } else if seg.b + 1 == last {
                EDGE_PULL
            } else {
                DEAD_PULL
            };
            let p = p + along * pull_a;
            let q = q - along * pull_b;
            // Too short a stub left to turn round on (a junction right by the edge): no lanes, the
            // junction just has no way on that side.
            if pull_a.max(pull_b) > 0.0 && p.distance(q) < pull_a.max(pull_b) + 6.0 {
                continue;
            }
            // Sides of the junctions the segment enters: at its far end it enters jb from the
            // side facing back along it, and ja from the side facing forward.
            let side_b: u8 = if seg.ew { 3 } else { 0 };
            let side_a: u8 = if seg.ew { 1 } else { 2 };
            let kind = kind_of(seg.class);
            let speed = seg.class.lanes().1;
            let mut fwd_ids = Vec::new();
            let mut back_ids = Vec::new();
            for (idx, off) in lanes.iter().enumerate() {
                // Forward (along +x / +z) on the right of the centre line.
                let a = p + right * *off;
                let b = q + right * *off;
                let mut lane = Lane::new(line(a, b, terrain), kind, speed, width);
                lane.index = idx as u8;
                if let Some(j) = jb {
                    lane.junction = Some(j);
                    lane.approach = Some(side_b);
                }
                let id = g.lanes.len() as u32;
                g.lanes.push(lane);
                fwd_ids.push(id);
                let a = q - right * *off;
                let b = p - right * *off;
                let mut lane = Lane::new(line(a, b, terrain), kind, speed, width);
                lane.index = idx as u8;
                if let Some(j) = ja {
                    lane.junction = Some(j);
                    lane.approach = Some(side_a);
                }
                let id = g.lanes.len() as u32;
                g.lanes.push(lane);
                back_ids.push(id);
            }
            for ids in [&fwd_ids, &back_ids] {
                for w in ids.windows(2) {
                    // Inner lanes are on the left of travel.
                    g.lanes[w[0] as usize].right = Some(w[1]);
                    g.lanes[w[1] as usize].left = Some(w[0]);
                }
            }
            match jb {
                Some(j) => {
                    incoming.entry((j, side_b)).or_default().extend(&fwd_ids);
                    outgoing.entry((j, side_b)).or_default().extend(&back_ids);
                }
                // A road ending where a ramp comes down carries on up it: no turnaround.
                None if ramp(q) => {}
                None => dead.push((fwd_ids.clone(), back_ids[0])),
            }
            match ja {
                Some(j) => {
                    incoming.entry((j, side_a)).or_default().extend(&back_ids);
                    outgoing.entry((j, side_a)).or_default().extend(&fwd_ids);
                }
                None if ramp(p) => {}
                None => dead.push((back_ids.clone(), fwd_ids[0])),
            }
            // Parking along the curbs, clear of the ends.
            if let Some(pk) = parking {
                let len = p.distance(q);
                let mut d = 9.0;
                while d < len - 9.0 {
                    for (sign, lane) in [(1.0f32, fwd_ids[fwd_ids.len() - 1]), (-1.0, back_ids[back_ids.len() - 1])] {
                        let at = p + along * d + right * pk * sign;
                        let dir = along * sign;
                        g.parking.push(ParkingSpot {
                            pos: Vec3::new(at.x, ROAD_Y + terrain.height_at(at), at.y),
                            dir: Vec3::new(dir.x, 0.0, dir.y),
                            lane,
                        });
                    }
                    d += 6.5;
                }
            }
        }
        // Connectors through junctions.
        for (j, jn) in g.junctions.clone().iter().enumerate() {
            let j = j as u32;
            for s_in in 0..4u8 {
                let Some(ins) = incoming.get(&(j, s_in)).cloned() else { continue };
                for s_out in 0..4u8 {
                    if s_out == s_in {
                        continue;
                    }
                    let Some(outs) = outgoing.get(&(j, s_out)).cloned() else { continue };
                    // Entering from side s_in means heading away from it.
                    let turn = turn_of(s_in, s_out);
                    for &li in &ins {
                        let lin = &g.lanes[li as usize];
                        let multi = ins.len() > 1;
                        let ok = match turn {
                            Turn::Left => !multi || lin.index == 0,
                            Turn::Right => !multi || lin.index as usize == ins.len() - 1,
                            _ => true,
                        };
                        if !ok {
                            continue;
                        }
                        let target = match turn {
                            Turn::Left => outs[0],
                            Turn::Right => outs[outs.len() - 1],
                            _ => outs[(lin.index as usize).min(outs.len() - 1)],
                        };
                        let (p0, d0) = lin.at(lin.length);
                        let lout = &g.lanes[target as usize];
                        let (p2, d2) = lout.at(0.0);
                        let mut c = Lane::new(curve(p0, d0, p2, d2), LaneKind::Connector, lin.speed.min(lout.speed), lin.width);
                        c.junction = Some(j);
                        c.turn = turn;
                        c.speed = match turn {
                            Turn::Straight => c.speed,
                            _ => c.speed.min(7.0),
                        };
                        let cid = g.lanes.len() as u32;
                        g.lanes.push(c);
                        g.link(li, cid);
                        g.link(cid, target);
                    }
                }
            }
            // An approach with no way through (every other arm of the junction is gone): its
            // lanes turn round in the box, onto the lanes leaving the same way they came.
            for s_in in 0..4u8 {
                let Some(ins) = incoming.get(&(j, s_in)).cloned() else { continue };
                let Some(outs) = outgoing.get(&(j, s_in)).cloned() else { continue };
                if ins.iter().any(|&li| !g.lanes[li as usize].next.is_empty()) {
                    continue;
                }
                for &li in &ins {
                    let lin = &g.lanes[li as usize];
                    let target = outs[(lin.index as usize).min(outs.len() - 1)];
                    let (p0, d0) = lin.at(lin.length);
                    let (p2, _) = g.lanes[target as usize].at(0.0);
                    let mut c = Lane::new(turnaround(p0, d0, p2), LaneKind::Connector, 4.0, lin.width);
                    c.junction = Some(j);
                    c.turn = Turn::UTurn;
                    let cid = g.lanes.len() as u32;
                    g.lanes.push(c);
                    g.link(li, cid);
                    g.link(cid, target);
                }
            }
            // Lights where four roads meet or a side road meets an avenue; two streets meeting in
            // a T just yield.
            let sides: Vec<bool> = (0..4u8).map(|s| incoming.contains_key(&(j, s))).collect();
            let n_in = sides.iter().filter(|s| **s).count();
            let major = (0..4u8).any(|s| {
                incoming.get(&(j, s)).is_some_and(|ids| {
                    ids.iter().any(|&i| matches!(g.lanes[i as usize].kind, LaneKind::Avenue | LaneKind::Boulevard))
                })
            });
            let _ = jn;
            let jm = &mut g.junctions[j as usize];
            jm.sides = [sides[0], sides[1], sides[2], sides[3]];
            if n_in == 4 || (n_in == 3 && major) {
                let c = jm.centre();
                // Green longer on the wider road; a green wave east-west at ~13 m/s.
                let ew_wide = jm.max.y - jm.min.y >= grid.cell * 2.0;
                let ns_wide = jm.max.x - jm.min.x >= grid.cell * 2.0;
                let (ns_green, ew_green) = match (ns_wide, ew_wide) {
                    (true, false) => (18.0, 11.0),
                    (false, true) => (11.0, 18.0),
                    _ => (14.0, 14.0),
                };
                let plan = SignalPlan { ns_green, ew_green, amber: 3.0, all_red: 1.5, offset: 0.0 };
                let offset = (c.x / 13.0).rem_euclid(plan.cycle());
                jm.signal = Some(SignalPlan { offset, ..plan });
            }
        }
        // Stop lines: signalised approaches stop 8 m back (room for a bus swinging round the
        // corner); others yield at the line.
        for l in g.lanes.iter_mut() {
            if l.kind != LaneKind::Connector && l.junction.is_some() {
                l.stop = 8.0;
            }
        }
        // U-turns at dead ends.
        for (intos, out) in dead {
            for into in intos {
                let (p0, d0) = g.lanes[into as usize].at(f32::MAX);
                let (p2, _) = g.lanes[out as usize].at(0.0);
                let mut c = Lane::new(turnaround(p0, d0, p2), LaneKind::Connector, 4.0, 3.6);
                c.turn = Turn::UTurn;
                let cid = g.lanes.len() as u32;
                g.lanes.push(c);
                g.link(into, cid);
                g.link(cid, out);
            }
        }
        for f in freeways {
            g.freeway(f, grid);
        }
        for c in channels {
            g.channel(c, grid);
        }
        g.close_dead_ends();
        g.reindex();
        g
    }

    fn link(&mut self, a: u32, b: u32) {
        self.lanes[a as usize].next.push(b);
        self.lanes[b as usize].prev.push(a);
    }

    /// The nearest lane to the end of `id` whose start is within `r` metres and heading the same
    /// way (joins a ramp or channel lane onto the street grid).
    fn nearest_start(&self, p: Vec3, dir: Vec3, r: f32, skip: &[u32]) -> Option<u32> {
        let mut best: Option<(u32, f32)> = None;
        for (i, l) in self.lanes.iter().enumerate() {
            if l.kind == LaneKind::Connector || skip.contains(&(i as u32)) {
                continue;
            }
            let (s, d) = l.at(0.0);
            let dist = s.distance(p);
            if dist < r && d.dot(dir) > 0.5 && best.is_none_or(|(_, b)| dist < b) {
                best = Some((i as u32, dist));
            }
        }
        best.map(|b| b.0)
    }

    fn nearest_end(&self, p: Vec3, dir: Vec3, r: f32, skip: &[u32]) -> Option<u32> {
        let mut best: Option<(u32, f32)> = None;
        for (i, l) in self.lanes.iter().enumerate() {
            if l.kind == LaneKind::Connector || skip.contains(&(i as u32)) {
                continue;
            }
            let (e, d) = l.at(l.length);
            let dist = e.distance(p);
            if dist < r && d.dot(dir) > 0.5 && best.is_none_or(|(_, b)| dist < b) {
                best = Some((i as u32, dist));
            }
        }
        best.map(|b| b.0)
    }

    /// A freeway: its deck lanes split where ramps join, the ramps' lanes blended across the
    /// shoulder over their level stretch and joined to the streets at their ends.
    fn freeway(&mut self, f: &FreewayGeom, grid: &Grid) {
        let deck = &f.deck;
        let n = deck.len();
        // A closed start's lanes stop at its turnaround (and loop round there, below).
        let turn = if f.closed_start { deck.sample_at(crate::freeway::TURNAROUND) } else { 0 };
        let mut splits: Vec<usize> = vec![turn, n - 1];
        for r in f.ramps.iter().filter(|r| r.deck_sample > turn) {
            splits.push(r.deck_sample);
        }
        splits.sort_unstable();
        splits.dedup();
        let speed = 29.0;
        let lanes_n = (f.lanes as usize).min(LANES.len());
        // Right side runs along the path; left side against it. ids[side][lane] = pieces in order
        // of travel.
        let mut pieces: [Vec<Vec<u32>>; 2] = [vec![Vec::new(); lanes_n], vec![Vec::new(); lanes_n]];
        let mut piece_at: BTreeMap<(usize, usize, usize), u32> = BTreeMap::new();
        for w in splits.windows(2) {
            let (a, b) = (w[0], w[1]);
            for (side, sign) in [(0usize, 1.0f32), (1, -1.0)] {
                for (li, u) in LANES.iter().take(lanes_n).enumerate() {
                    let mut pts: Vec<Vec3> = (a..=b).map(|i| deck.at(i, u * sign, 0.0)).collect();
                    if side == 1 {
                        pts.reverse();
                    }
                    let mut lane = Lane::new(pts, LaneKind::Freeway, speed, 3.6);
                    lane.index = li as u8;
                    // A broken end goes nowhere.
                    lane.closed = b == n - 1 && f.broken_end;
                    let id = self.lanes.len() as u32;
                    self.lanes.push(lane);
                    pieces[side][li].push(id);
                    piece_at.insert((side, li, a), id);
                }
            }
        }
        for (side, sides) in pieces.iter().enumerate() {
            for list in sides {
                // Pieces were made in path order; the left side travels them backwards.
                let order: Vec<u32> = if side == 0 { list.clone() } else { list.iter().rev().copied().collect() };
                for w in order.windows(2) {
                    self.link(w[0], w[1]);
                }
            }
            // Lane-change neighbours within each piece (inner to outer).
            for pair in sides.windows(2) {
                for (&a, &b) in pair[0].iter().zip(&pair[1]) {
                    self.lanes[a as usize].right = Some(b);
                    self.lanes[b as usize].left = Some(a);
                }
            }
        }
        // The turnaround: each lane against the path loops round into its opposite number along it
        // (concentric half circles, bulging toward the barrier).
        if f.closed_start && turn + 1 < n {
            let back = (deck.points[turn] - deck.points[turn + 1]).normalize_or_zero();
            for (li, u) in LANES.iter().take(lanes_n).enumerate() {
                let (p0, p2) = (deck.at(turn, -u, 0.0), deck.at(turn, *u, 0.0));
                let (c, r) = ((p0 + p2) * 0.5, p0.distance(p2) * 0.5);
                let across = (p2 - p0).normalize_or_zero();
                let pts: Vec<Vec3> = (0..=12)
                    .map(|k| {
                        let t = std::f32::consts::PI * k as f32 / 12.0;
                        c - across * r * t.cos() + back * r * t.sin()
                    })
                    .collect();
                let mut lane = Lane::new(pts, LaneKind::Connector, 4.0, 3.6);
                lane.turn = Turn::UTurn;
                lane.index = li as u8;
                let id = self.lanes.len() as u32;
                self.lanes.push(lane);
                let (from, to) = (pieces[1][li][0], pieces[0][li][0]);
                self.link(from, id);
                self.link(id, to);
            }
        }
        let outer = lanes_n - 1;
        for r in &f.ramps {
            let k = r.deck_sample;
            // The deck's side the ramp is on (right of the path: side 0).
            let off = (r.path.points[0] - deck.points[k]).dot(deck.right[k]);
            let side = if off > 0.0 { 0 } else { 1 };
            let sign = if side == 0 { 1.0 } else { -1.0 };
            // The ramp's lane along its path, blended from the deck's outer lane.
            let m = r.path.len();
            let deck_lane_pt = |i: usize| {
                let p = r.path.points[i];
                let j = (0..n)
                    .min_by(|&a, &b| deck.points[a].distance_squared(p).total_cmp(&deck.points[b].distance_squared(p)))
                    .unwrap();
                deck.at(j, LANES[outer] * sign, 0.0)
            };
            let mut pts: Vec<Vec3> = Vec::new();
            for i in 0..m {
                let s = r.path.station[i];
                let t = (s / 40.0).clamp(0.0, 1.0);
                let t = t * t * (3.0 - 2.0 * t);
                let ramp_c = r.path.at(i, 0.0, 0.0);
                let p = if t < 1.0 { deck_lane_pt(i).lerp(ramp_c, t) } else { ramp_c };
                pts.push(p);
            }
            // Trim the street end to where the street's own lane begins (the stub's far edge).
            let street_end = *pts.last().unwrap();
            let cell = grid.cell_of(Vec2::new(street_end.x, street_end.z));
            let lo = grid.corner(cell);
            let hi = lo + Vec2::splat(grid.cell);
            let inside = |p: &Vec3| p.x >= lo.x - 0.5 && p.x <= hi.x + 0.5 && p.z >= lo.y - 0.5 && p.z <= hi.y + 0.5;
            while pts.len() > 3 && inside(&pts[pts.len() - 2]) {
                pts.pop();
            }
            let on = r.kind == RampKind::On;
            if on {
                pts.reverse();
            }
            let lane = Lane::new(pts, LaneKind::Ramp, 20.0, 4.0);
            let id = self.lanes.len() as u32;
            self.lanes.push(lane);
            // The deck pieces before and after the split at k, in travel order.
            let piece_from = |a_sample: usize| piece_at.get(&(side, outer, a_sample)).copied();
            let starts: Vec<usize> = splits.iter().copied().filter(|s| *s < n - 1).collect();
            let before_k = starts.iter().copied().filter(|s| *s < k).max();
            let at_k = piece_from(k);
            let (up, down) = if side == 0 {
                (before_k.and_then(piece_from), at_k)
            } else {
                // Travelling against the path: the piece starting at k is behind, the one
                // ending at k ahead.
                (at_k, before_k.and_then(piece_from))
            };
            let (pid, _) = (self.lanes[id as usize].at(0.0), 0);
            let _ = pid;
            if !on {
                if let Some(u) = up {
                    self.link(u, id);
                }
                let (e, d) = self.lanes[id as usize].at(f32::MAX);
                if let Some(s) = self.nearest_start(e, d, 14.0, &[id]) {
                    self.link(id, s);
                }
            } else {
                if let Some(dn) = down {
                    self.link(id, dn);
                }
                let (s, d) = self.lanes[id as usize].at(0.0);
                if let Some(e) = self.nearest_end(s, d, 14.0, &[id]) {
                    self.link(e, id);
                }
            }
        }
    }

    /// A storm channel's bed: a lane each way, ramps down to it both ways, all closed to ambient
    /// traffic (chases only).
    fn channel(&mut self, c: &ChannelGeom, grid: &Grid) {
        let x = c.centre_x();
        let (z0, z1) = (c.z0 + 6.0, c.z1 - 6.0);
        // Split points where ramps reach the bed.
        let mut cuts: Vec<f32> = vec![z0, z1];
        for (_, bottom) in &c.ramps {
            cuts.push(bottom.z.clamp(z0, z1));
        }
        cuts.sort_by(f32::total_cmp);
        cuts.dedup_by(|a, b| (*a - *b).abs() < 1.0);
        let mut north: Vec<u32> = Vec::new();
        let mut south: Vec<u32> = Vec::new();
        for w in cuts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let mk = |xx: f32, from: f32, to: f32| {
                let n = (((to - from).abs() / 4.0).ceil() as usize).max(1);
                (0..=n).map(|i| Vec3::new(xx, BED_Y, from + (to - from) * i as f32 / n as f32)).collect::<Vec<_>>()
            };
            let mut s = Lane::new(mk(x - 3.5, a, b), LaneKind::Channel, 20.0, 3.6);
            s.closed = true;
            let id = self.lanes.len() as u32;
            self.lanes.push(s);
            south.push(id);
            let mut nn = Lane::new(mk(x + 3.5, b, a), LaneKind::Channel, 20.0, 3.6);
            nn.closed = true;
            let id = self.lanes.len() as u32;
            self.lanes.push(nn);
            north.push(id);
        }
        for w in south.windows(2) {
            self.link(w[0], w[1]);
        }
        let rev: Vec<u32> = north.iter().rev().copied().collect();
        for w in rev.windows(2) {
            self.link(w[0], w[1]);
        }
        // Ends: turn around.
        if let (Some(&s_last), Some(&n_first)) = (south.last(), north.last()) {
            self.link(s_last, n_first);
        }
        if let (Some(&n_last), Some(&s_first)) = (north.first(), south.first()) {
            self.link(n_last, s_first);
        }
        for (k, (top, bottom)) in c.ramps.iter().enumerate() {
            // Down the ramp along its surface's centre line: level for the landing, then the drop.
            let len = Vec2::new(bottom.x - top.x, bottom.z - top.z).length();
            let line = c.ramp_lines.get(k).cloned().unwrap_or_else(|| vec![*top, *bottom]);
            let slope = |t: f32| {
                // The point `t` of the way down the line (by its z, which runs evenly).
                let z = top.z + (bottom.z - top.z) * t;
                let i = line.iter().position(|p| (p.z - top.z).abs() >= (z - top.z).abs()).unwrap_or(line.len() - 1);
                if i == 0 {
                    return line[0];
                }
                let (a, b) = (line[i - 1], line[i]);
                let f = if (b.z - a.z).abs() > 1e-3 { (z - a.z) / (b.z - a.z) } else { 0.0 };
                a.lerp(b, f.clamp(0.0, 1.0))
            };
            let n = ((len / 4.0).ceil() as usize).max(2);
            let descent: Vec<Vec3> = (0..=n).map(|i| slope(i as f32 / n as f32)).collect();
            // The landing's end, where the drop begins: the lead-ins from and to the street turn
            // there (over the level apron).
            let landing = slope((crate::channel::LANDING / len.max(1.0)).min(0.5));
            let dir_down = (*bottom - *top).normalize_or(Vec3::Z);
            // A turn from `from` (heading `fdir`) to `to` (heading `tdir`): a quadratic curve
            // through the corner where the two headings cross.
            let bend = |from: Vec3, fdir: Vec3, to: Vec3, tdir: Vec3| -> Vec<Vec3> {
                let flat = |v: Vec3| Vec2::new(v.x, v.z);
                let (p, d, q, e) = (flat(from), flat(fdir).normalize_or_zero(), flat(to), flat(tdir).normalize_or_zero());
                let den = d.x * e.y - d.y * e.x;
                let corner = if den.abs() > 1e-3 {
                    let s = ((q - p).x * e.y - (q - p).y * e.x) / den;
                    p + d * s
                } else {
                    (p + q) * 0.5
                };
                (0..=8)
                    .map(|i| {
                        let t = i as f32 / 8.0;
                        let c = p * (1.0 - t) * (1.0 - t) + corner * 2.0 * t * (1.0 - t) + q * t * t;
                        Vec3::new(c.x, from.y + (to.y - from.y) * t, c.y)
                    })
                    .collect()
            };
            // The driveway that comes in at the top, and the one that leaves it.
            let skip = [u32::MAX];
            let into = self.nearest_end(*top, dir_down, 18.0, &skip).or_else(|| self.nearest_end_any(*top, 18.0, &skip));
            let out_of = self.nearest_start_any(*top, 18.0, &skip);
            let mut down_pts = Vec::new();
            if let Some(e) = into {
                let l = &self.lanes[e as usize];
                let (end, d) = l.at(l.length);
                down_pts.extend(bend(end, d, landing, dir_down));
                down_pts.extend(descent.iter().copied().filter(|p| (p.z - top.z).abs() > (landing.z - top.z).abs() + 0.5));
            } else {
                down_pts.extend(descent.iter().copied());
            }
            let mut up_pts: Vec<Vec3> =
                descent.iter().rev().copied().filter(|p| (p.z - top.z).abs() > (landing.z - top.z).abs() + 0.5).collect();
            match out_of {
                Some(s0) => {
                    let l = &self.lanes[s0 as usize];
                    let (start, d) = l.at(0.0);
                    up_pts.extend(bend(landing, -dir_down, start, d));
                }
                None => up_pts
                    .extend(descent.iter().rev().copied().filter(|p| (p.z - top.z).abs() <= (landing.z - top.z).abs() + 0.5)),
            }
            let mut down = Lane::new(down_pts, LaneKind::Ramp, 10.0, 4.0);
            down.closed = true;
            let did = self.lanes.len() as u32;
            self.lanes.push(down);
            let mut up = Lane::new(up_pts, LaneKind::Ramp, 10.0, 4.0);
            up.closed = true;
            let uid = self.lanes.len() as u32;
            self.lanes.push(up);
            // Bed lanes starting or ending at the ramp's foot.
            let foot = bottom.z;
            let near = |l: &Lane, at_start: bool| {
                let p = if at_start { l.points[0] } else { *l.points.last().unwrap() };
                (p.z - foot).abs() < 2.0
            };
            let starts: Vec<u32> = south.iter().chain(&north).copied().filter(|&i| near(&self.lanes[i as usize], true)).collect();
            let ends: Vec<u32> = south.iter().chain(&north).copied().filter(|&i| near(&self.lanes[i as usize], false)).collect();
            for s in starts {
                self.link(did, s);
            }
            for e in ends {
                self.link(e, uid);
            }
            let _ = grid;
            if let Some(e) = into {
                self.link(e, did);
            }
            if let Some(s) = out_of {
                self.link(uid, s);
            }
        }
    }

    fn nearest_end_any(&self, p: Vec3, r: f32, skip: &[u32]) -> Option<u32> {
        self.lanes
            .iter()
            .enumerate()
            .filter(|(i, l)| l.kind != LaneKind::Connector && !skip.contains(&(*i as u32)))
            .map(|(i, l)| (i as u32, l.points.last().unwrap().distance(p)))
            .filter(|(_, d)| *d < r)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|b| b.0)
    }

    fn nearest_start_any(&self, p: Vec3, r: f32, skip: &[u32]) -> Option<u32> {
        self.lanes
            .iter()
            .enumerate()
            .filter(|(i, l)| l.kind != LaneKind::Connector && !skip.contains(&(*i as u32)))
            .map(|(i, l)| (i as u32, l.points[0].distance(p)))
            .filter(|(_, d)| *d < r)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|b| b.0)
    }

    /// Lanes that lead nowhere (the closed spur) or come from nowhere get no ambient traffic.
    fn close_dead_ends(&mut self) {
        let n = self.lanes.len();
        loop {
            let mut changed = false;
            for i in 0..n {
                if self.lanes[i].closed {
                    continue;
                }
                let l = &self.lanes[i];
                // Changing lanes is a way on (and a way in) when a lane across does go on (come
                // from) somewhere — two lanes side by side that both stop do not keep each other open.
                let mut across = Vec::new();
                for right in [false, true] {
                    let mut at = if right { l.right } else { l.left };
                    while let Some(j) = at.filter(|j| !across.contains(j) && *j as usize != i) {
                        across.push(j);
                        let x = &self.lanes[j as usize];
                        at = if right { x.right } else { x.left };
                    }
                }
                let open = |j: &u32| !self.lanes[*j as usize].closed;
                let open_next =
                    l.next.iter().any(open) || across.iter().any(|j| open(j) && self.lanes[*j as usize].next.iter().any(open));
                let open_prev =
                    l.prev.iter().any(open) || across.iter().any(|j| open(j) && self.lanes[*j as usize].prev.iter().any(open));
                // Freeway lanes at the map edge are sources and sinks.
                let edge = l.source;
                if (!open_next && !(edge && l.next.is_empty())) || (!open_prev && !(edge && l.prev.is_empty())) {
                    self.lanes[i].closed = true;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }

    fn reindex(&mut self) {
        self.index.clear();
        for (i, l) in self.lanes.iter().enumerate() {
            for w in l.points.windows(2) {
                let lo = w[0].min(w[1]) - Vec3::splat(2.0);
                let hi = w[0].max(w[1]) + Vec3::splat(2.0);
                for z in (lo.z / INDEX_CELL).floor() as i32..=(hi.z / INDEX_CELL).floor() as i32 {
                    for x in (lo.x / INDEX_CELL).floor() as i32..=(hi.x / INDEX_CELL).floor() as i32 {
                        let v = self.index.entry((x, z)).or_default();
                        if v.last() != Some(&(i as u32)) {
                            v.push(i as u32);
                        }
                    }
                }
            }
        }
    }

    /// The lane nearest `p` that `keep` accepts: (lane, distance along it, distance from it).
    pub fn nearest(&self, p: Vec3, keep: impl Fn(&Lane) -> bool) -> Option<(u32, f32, f32)> {
        let (cx, cz) = ((p.x / INDEX_CELL).floor() as i32, (p.z / INDEX_CELL).floor() as i32);
        let mut best: Option<(u32, f32, f32)> = None;
        for r in 0..4 {
            for z in cz - r..=cz + r {
                for x in cx - r..=cx + r {
                    if (z - cz).abs() != r && (x - cx).abs() != r {
                        continue;
                    }
                    for &i in self.index.get(&(x, z)).map(|v| v.as_slice()).unwrap_or(&[]) {
                        let l = &self.lanes[i as usize];
                        if !keep(l) {
                            continue;
                        }
                        let (along, d) = l.project(p);
                        if best.is_none_or(|b| d < b.2) {
                            best = Some((i, along, d));
                        }
                    }
                }
            }
            if best.is_some_and(|b| b.2 < r as f32 * INDEX_CELL) {
                break;
            }
        }
        best
    }

    /// Like [`LaneGraph::nearest`], but only lanes passing within `rise` metres of `p`'s height
    /// (a car under the freeway is not on the deck above it), searched a little wider.
    pub fn nearest_level(&self, p: Vec3, rise: f32, keep: impl Fn(&Lane) -> bool) -> Option<(u32, f32, f32)> {
        let (cx, cz) = ((p.x / INDEX_CELL).floor() as i32, (p.z / INDEX_CELL).floor() as i32);
        let mut best: Option<(u32, f32, f32)> = None;
        for r in 0..7 {
            for z in cz - r..=cz + r {
                for x in cx - r..=cx + r {
                    if (z - cz).abs() != r && (x - cx).abs() != r {
                        continue;
                    }
                    for &i in self.index.get(&(x, z)).map(|v| v.as_slice()).unwrap_or(&[]) {
                        let l = &self.lanes[i as usize];
                        if !keep(l) {
                            continue;
                        }
                        let (along, d) = l.project(p);
                        if (l.at(along).0.y - p.y).abs() > rise {
                            continue;
                        }
                        if best.is_none_or(|b| d < b.2) {
                            best = Some((i, along, d));
                        }
                    }
                }
            }
            if best.is_some_and(|b| b.2 < r as f32 * INDEX_CELL) {
                break;
            }
        }
        best
    }

    /// A* from the lane nearest `from` to the lane nearest `to`: the lanes in order (empty if
    /// there is no way).
    pub fn route(&self, from: Vec3, to: Vec3) -> Vec<u32> {
        let open = |l: &Lane| l.kind != LaneKind::Connector;
        let (Some((a, _, _)), Some((b, _, _))) = (self.nearest(from, open), self.nearest(to, open)) else { return Vec::new() };
        self.route_lanes(a, b)
    }

    /// A* between two lanes.
    pub fn route_lanes(&self, a: u32, b: u32) -> Vec<u32> {
        self.route_lanes_avoiding(a, b, &[])
    }

    /// A* between two lanes that never enters the lanes in `avoid` (a road found blocked): the
    /// lanes in order, empty if there is no other way.
    pub fn route_lanes_avoiding(&self, a: u32, b: u32, avoid: &[u32]) -> Vec<u32> {
        #[derive(PartialEq)]
        struct Node(f32, u32);
        impl Eq for Node {}
        impl PartialOrd for Node {
            fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
                Some(self.cmp(o))
            }
        }
        impl Ord for Node {
            fn cmp(&self, o: &Self) -> Ordering {
                o.0.total_cmp(&self.0).then(o.1.cmp(&self.1))
            }
        }
        let goal = self.lanes[b as usize].points[0];
        let h = |i: u32| self.lanes[i as usize].points.last().unwrap().distance(goal) / 30.0;
        let mut cost: BTreeMap<u32, f32> = BTreeMap::new();
        let mut came: BTreeMap<u32, u32> = BTreeMap::new();
        let mut heap = BinaryHeap::new();
        cost.insert(a, 0.0);
        heap.push(Node(h(a), a));
        while let Some(Node(_, i)) = heap.pop() {
            if i == b {
                let mut path = vec![b];
                let mut cur = b;
                while let Some(&p) = came.get(&cur) {
                    path.push(p);
                    cur = p;
                }
                path.reverse();
                return path;
            }
            let ci = cost[&i];
            let here = &self.lanes[i as usize];
            // On along the links, or across to a neighbouring lane (a lane change costs a moment).
            let changes = here.left.into_iter().chain(here.right).map(|j| (j, 1.5));
            for (j, extra) in here.next.iter().map(|&j| (j, 0.0)).chain(changes) {
                if avoid.contains(&j) {
                    continue;
                }
                let l = &self.lanes[j as usize];
                let c = ci + l.length / l.speed.max(1.0) + extra;
                if cost.get(&j).is_none_or(|&old| c < old) {
                    cost.insert(j, c);
                    came.insert(j, i);
                    heap.push(Node(c + h(j), j));
                }
            }
        }
        Vec::new()
    }

    /// A route's points (for the GPS ribbon), from the lanes' polylines.
    pub fn route_points(&self, lanes: &[u32]) -> Vec<Vec3> {
        let mut out: Vec<Vec3> = Vec::new();
        for &i in lanes {
            for p in &self.lanes[i as usize].points {
                if out.last().is_none_or(|q| q.distance(*p) > 0.5) {
                    out.push(*p);
                }
            }
        }
        out
    }

    /// The light for a lane that ends at a signalised junction, at time `t`.
    pub fn light(&self, lane: u32, t: f32) -> Option<Light> {
        let l = &self.lanes[lane as usize];
        let j = l.junction?;
        let side = l.approach?;
        let plan = self.junctions[j as usize].signal?;
        Some(plan.light(side == 0 || side == 2, t))
    }
}

/// Junction cells grouped into junctions.
fn junctions(grid: &Grid) -> Vec<Junction> {
    let is_j =
        |c: IVec2| grid.road(c).is_some_and(|r| r.drivable() && r != RoadClass::Channel) && role(grid, c) == Role::Junction;
    let mut seen = vec![false; (grid.w * grid.h) as usize];
    let mut out = Vec::new();
    for z in 0..grid.h {
        for x in 0..grid.w {
            let c = IVec2::new(x, z);
            if seen[(z * grid.w + x) as usize] || !is_j(c) {
                continue;
            }
            let mut cells = Vec::new();
            let mut stack = vec![c];
            seen[(z * grid.w + x) as usize] = true;
            while let Some(p) = stack.pop() {
                cells.push(p);
                for d in DIRS {
                    let q = p + d;
                    if grid.inside(q) && !seen[(q.y * grid.w + q.x) as usize] && is_j(q) {
                        seen[(q.y * grid.w + q.x) as usize] = true;
                        stack.push(q);
                    }
                }
            }
            cells.sort_by_key(|c| (c.y, c.x));
            let lo = cells.iter().fold(IVec2::MAX, |a, b| a.min(*b));
            let hi = cells.iter().fold(IVec2::MIN, |a, b| a.max(*b));
            out.push(Junction {
                cells,
                min: grid.corner(lo),
                max: grid.corner(hi + IVec2::ONE),
                sides: [false; 4],
                signal: None,
            });
        }
    }
    out
}

/// Straight runs of band cells (east-west and north-south) between junctions or ends.
fn segments(grid: &Grid) -> Vec<Segment> {
    let mut out = Vec::new();
    let band = |c: IVec2| -> Option<(bool, i32, i32, RoadClass)> {
        let class = grid.road(c)?;
        if !class.drivable() || class == RoadClass::Channel {
            return None;
        }
        match role(grid, c) {
            Role::Ew { k, i } => Some((true, k, c.y - i, class)),
            Role::Ns { k, i } => Some((false, k, c.x - i, class)),
            _ => None,
        }
    };
    // East-west: scan rows of each band's first row.
    for z in 0..grid.h {
        let mut x = 0;
        while x < grid.w {
            let c = IVec2::new(x, z);
            match band(c) {
                Some((true, k, across0, class)) if across0 == z => {
                    let a = x;
                    while x + 1 < grid.w && band(IVec2::new(x + 1, z)) == Some((true, k, across0, class)) {
                        x += 1;
                    }
                    out.push(Segment { class, ew: true, k, across0, a, b: x });
                }
                _ => {}
            }
            x += 1;
        }
    }
    for x in 0..grid.w {
        let mut z = 0;
        while z < grid.h {
            let c = IVec2::new(x, z);
            match band(c) {
                Some((false, k, across0, class)) if across0 == x => {
                    let a = z;
                    while z + 1 < grid.h && band(IVec2::new(x, z + 1)) == Some((false, k, across0, class)) {
                        z += 1;
                    }
                    out.push(Segment { class, ew: false, k, across0, a, b: z });
                }
                _ => {}
            }
            z += 1;
        }
    }
    out
}

/// The manoeuvre from entering at side `s_in` to leaving at side `s_out` (sides 0 N, 1 E, 2 S,
/// 3 W; entering from the north means heading south).
fn turn_of(s_in: u8, s_out: u8) -> Turn {
    match (s_out + 4 - s_in) % 4 {
        2 => Turn::Straight,
        // Heading south (entered from N=0) and leaving east (1): that is a left turn.
        1 => Turn::Left,
        3 => Turn::Right,
        _ => Turn::UTurn,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turns_follow_right_hand_traffic() {
        // Entering from the north heads south: east is on the left, west on the right.
        assert_eq!(turn_of(0, 1), Turn::Left);
        assert_eq!(turn_of(0, 3), Turn::Right);
        assert_eq!(turn_of(0, 2), Turn::Straight);
        // Entering from the west heads east: north is left.
        assert_eq!(turn_of(3, 0), Turn::Left);
        assert_eq!(turn_of(3, 2), Turn::Right);
    }

    #[test]
    fn a_signal_never_greens_both_ways() {
        let p = SignalPlan { ns_green: 14.0, ew_green: 11.0, amber: 3.0, all_red: 1.5, offset: 7.0 };
        let mut t = 0.0;
        while t < p.cycle() * 2.0 {
            let (a, b) = (p.light(true, t), p.light(false, t));
            assert!(!(a != Light::Red && b != Light::Red), "conflict at {t}: {a:?} {b:?}");
            if p.walk(true, t).is_some() {
                assert_eq!(p.light(true, t), Light::Red, "walk across a north-south road only on its red");
            }
            t += 0.25;
        }
    }
}
