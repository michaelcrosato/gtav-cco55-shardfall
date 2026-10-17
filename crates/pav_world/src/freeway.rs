//! Elevated freeways (PLAN.md §6.6.1): a deck swept along a smoothed centre line — three lanes
//! each way with their markings, jersey barriers on both edges and down the median, the slab's
//! fascia and underside — on piers every 30 m, with ramps to the streets that leave the deck
//! through gaps in its barrier. Triangle meshes with exact colliders, binned by chunk; the
//! sampled centre lines are kept for the lane graph and the map.

use std::collections::BTreeMap;

use glam::{Quat, Vec2, Vec3};
use pav_core::Color;
use pav_core::shape::{Look, Shape};
use pav_core::statics::Decor;
use pav_core::zones::{Label, LabelMode};

use crate::def::{DistrictDef, Freeway, RampKind};
use crate::grid::Grid;
use crate::ground::{MeshBuf, ROAD_Y, chunk_key, patches};
use crate::terrain::Terrain;
use crate::{Out, rng};

/// Spacing of the samples along a centre line (m): dashes are one sample long.
pub const STEP: f32 = 3.0;
/// Half the deck's width (barrier outsides) and the slab's depth below the road.
pub const HALF: f32 = 15.2;
const SLAB: f32 = 1.25;
/// Lane centres from the deck's centre line (m), inner to outer, on each side.
pub const LANES: [f32; 3] = [3.2, 6.8, 10.4];
/// A ramp's half width (barrier outsides) and how long it runs level beside the deck.
pub const RAMP_HALF: f32 = 3.6;
const MERGE: f32 = 40.0;
const FLAT_END: f32 = 8.0;

/// A sampled centre line: points (the road surface's height in y), the horizontal right of
/// each, and the distance along it.
#[derive(Clone, Debug, Default)]
pub struct Path {
    pub points: Vec<Vec3>,
    pub right: Vec<Vec3>,
    pub station: Vec<f32>,
}

impl Path {
    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// The point `u` metres to the right of sample `i`, `v` above the road.
    pub fn at(&self, i: usize, u: f32, v: f32) -> Vec3 {
        self.points[i] + self.right[i] * u + Vec3::Y * v
    }

    /// The direction of travel at sample `i` (horizontal).
    pub fn tangent(&self, i: usize) -> Vec3 {
        Vec3::new(self.right[i].z, 0.0, -self.right[i].x)
    }
}

/// A ramp's geometry.
#[derive(Clone, Debug)]
pub struct RampGeom {
    pub kind: RampKind,
    /// From its deck end to its street end.
    pub path: Path,
    /// Which side of the ramp the deck is on (+1 right, -1 left, along the path).
    pub inner: f32,
    /// The deck sample its deck end meets.
    pub deck_sample: usize,
}

/// A freeway's geometry: its deck's centre line (the road surface, the median's middle) and its
/// ramps.
#[derive(Clone, Debug)]
pub struct FreewayGeom {
    pub key: String,
    pub name: String,
    pub deck: Path,
    pub lanes: u8,
    pub ramps: Vec<RampGeom>,
    pub broken_end: bool,
    pub closed_start: bool,
}

/// Uniform Catmull-Rom through `pts`, resampled every `step` metres along its length.
pub fn smooth(pts: &[Vec2], step: f32) -> Vec<Vec2> {
    if pts.len() < 2 {
        return pts.to_vec();
    }
    let n = pts.len();
    let get = |i: isize| pts[i.clamp(0, n as isize - 1) as usize];
    let mut dense = Vec::new();
    for k in 0..n - 1 {
        let (p0, p1, p2, p3) = (get(k as isize - 1), get(k as isize), get(k as isize + 1), get(k as isize + 2));
        let seg = p1.distance(p2).max(1.0);
        let m = (seg / 0.5).ceil() as usize;
        for j in 0..m {
            let t = j as f32 / m as f32;
            let (t2, t3) = (t * t, t * t * t);
            dense.push(
                0.5 * ((2.0 * p1)
                    + (-p0 + p2) * t
                    + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
                    + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3),
            );
        }
    }
    dense.push(pts[n - 1]);
    // Resample by arc length.
    let mut out = vec![dense[0]];
    let mut carry = 0.0;
    for w in dense.windows(2) {
        let (a, b) = (w[0], w[1]);
        let d = a.distance(b);
        let mut t = step - carry;
        while t <= d {
            out.push(a + (b - a) * (t / d));
            t += step;
        }
        carry = d - (t - step);
    }
    if out.last().unwrap().distance(*dense.last().unwrap()) > step * 0.3 {
        out.push(*dense.last().unwrap());
    }
    out
}

/// A path through flat points with heights from `height(station)`.
fn path(flat: &[Vec2], height: impl Fn(f32) -> f32) -> Path {
    let n = flat.len();
    let mut p = Path::default();
    let mut s = 0.0;
    for i in 0..n {
        if i > 0 {
            s += flat[i].distance(flat[i - 1]);
        }
        let (a, b) = (flat[i.saturating_sub(1)], flat[(i + 1).min(n - 1)]);
        let t = (b - a).normalize_or(Vec2::Y);
        p.points.push(Vec3::new(flat[i].x, height(s), flat[i].y));
        p.right.push(Vec3::new(-t.y, 0.0, t.x));
        p.station.push(s);
    }
    p
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A cross-section: points (u right, v up) going around the deck, each with the colour of the
/// face to the next point.
type Profile = Vec<(Vec2, [f32; 3])>;

struct Paint {
    asphalt: [f32; 3],
    white: [f32; 3],
    yellow: [f32; 3],
    concrete: [f32; 3],
    under: [f32; 3],
}

fn paint() -> Paint {
    Paint {
        asphalt: Color::hex("#3a3c40").0,
        white: Color::hex("#e8e5da").0,
        yellow: Color::hex("#e2b53c").0,
        concrete: Color::hex("#b9b3a7").0,
        under: Color::hex("#8f8a80").0,
    }
}

/// The road surface from `u0` on (to the next point pushed) with lines centred at `lines`.
fn road(prof: &mut Profile, u0: f32, lines: &[(f32, [f32; 3])], asphalt: [f32; 3]) {
    let w = 0.075;
    let mut u = u0;
    let mut sorted: Vec<(f32, [f32; 3])> = lines.to_vec();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (c, col) in sorted {
        prof.push((Vec2::new(u, 0.0), asphalt));
        prof.push((Vec2::new(c - w, 0.0), col));
        u = c + w;
    }
    prof.push((Vec2::new(u, 0.0), asphalt));
}

/// A jersey barrier rising from the road at `edge` (u) and 0.6 m outward (`sign` +1: to the
/// right), its outer face then being the slab's fascia; or, in a gap, the road running on.
fn barrier(prof: &mut Profile, edge: f32, sign: f32, gap: bool, c: &Paint) {
    let s = sign;
    if gap {
        prof.push((Vec2::new(edge, 0.0), c.asphalt));
        prof.push((Vec2::new(edge + s * 0.6, 0.0), c.under));
    } else {
        for (du, v) in [(0.0, 0.0), (0.08, 0.08), (0.28, 0.33), (0.38, 0.85), (0.6, 0.85)] {
            prof.push((Vec2::new(edge + s * du, v), c.concrete));
        }
        // The last point's face is the fascia.
        prof.last_mut().unwrap().1 = c.under;
    }
}

/// The main deck's cross-section, going around: right road, right barrier, underside, left
/// barrier, left road, median.
fn deck_profile(gap_r: bool, gap_l: bool, dash: bool, c: &Paint) -> Profile {
    let dashc = if dash { c.white } else { c.asphalt };
    let mut p = Profile::new();
    road(&mut p, 0.4, &[(1.4, c.yellow), (5.0, dashc), (8.6, dashc), (12.2, c.white)], c.asphalt);
    barrier(&mut p, 14.6, 1.0, gap_r, c);
    p.push((Vec2::new(HALF, -SLAB), c.under));
    p.push((Vec2::new(-HALF, -SLAB), c.under));
    // The left barrier, from its outside back to the road.
    if gap_l {
        p.push((Vec2::new(-HALF, 0.0), c.asphalt));
    } else {
        for (du, v) in [(0.6, 0.85), (0.38, 0.85), (0.28, 0.33), (0.08, 0.08)] {
            p.push((Vec2::new(-14.6 - du, v), c.concrete));
        }
    }
    road(&mut p, -14.6, &[(-12.2, c.white), (-8.6, dashc), (-5.0, dashc), (-1.4, c.yellow)], c.asphalt);
    for (u, v) in [(-0.4, 0.0), (-0.3, 0.33), (-0.15, 0.85), (0.15, 0.85), (0.3, 0.33)] {
        p.push((Vec2::new(u, v), c.concrete));
    }
    p
}

/// A ramp's cross-section (one lane between shoulders).
fn ramp_profile(gap_r: bool, gap_l: bool, c: &Paint) -> Profile {
    let mut p = Profile::new();
    road(&mut p, -3.0, &[(-2.0, c.yellow), (2.0, c.white)], c.asphalt);
    barrier(&mut p, 3.0, 1.0, gap_r, c);
    p.push((Vec2::new(RAMP_HALF, -0.9), c.under));
    p.push((Vec2::new(-RAMP_HALF, -0.9), c.under));
    if gap_l {
        p.push((Vec2::new(-RAMP_HALF, 0.0), c.asphalt));
    } else {
        for (du, v) in [(0.6, 0.85), (0.38, 0.85), (0.28, 0.33), (0.08, 0.08)] {
            p.push((Vec2::new(-3.0 - du, v), c.concrete));
        }
    }
    p
}

/// Sweeps a profile from sample `i` to `i + 1` of a path.
fn sweep(buf: &mut MeshBuf, path: &Path, i: usize, prof: &Profile) {
    let n = prof.len();
    for k in 0..n {
        let (a, col) = prof[k];
        let b = prof[(k + 1) % n].0;
        let d = b - a;
        if d.length_squared() < 1e-8 {
            continue;
        }
        let n2 = Vec2::new(-d.y, d.x).normalize();
        let ni = path.right[i] * n2.x + Vec3::Y * n2.y;
        let nj = path.right[i + 1] * n2.x + Vec3::Y * n2.y;
        let v = [path.at(i, a.x, a.y), path.at(i + 1, a.x, a.y), path.at(i + 1, b.x, b.y), path.at(i, b.x, b.y)];
        buf.quad(v, [ni, nj, nj, ni], [col; 4], (ni + nj) * 0.5);
    }
}

fn decor(shape: Shape, pos: Vec3, rot: Quat, color: &str, solid: bool) -> Decor {
    Decor {
        shape,
        pos,
        rot,
        color: Color::hex(color),
        look: Look::Cel,
        emissive: 0.0,
        solid,
        collider: None,
        sway: Default::default(),
        material: None,
    }
}

/// A row of jersey barriers across a path at sample `i`, from `u0` to `u1`, leaving out the
/// stretch `gap` (u range) if any.
fn closure(path: &Path, i: usize, u0: f32, u1: f32, gap: Option<(f32, f32)>, out: &mut Out) {
    let t = path.tangent(i);
    let rot = Quat::from_rotation_y(t.x.atan2(t.z));
    let mut u = u0 + 1.5;
    while u <= u1 - 1.5 {
        if gap.is_none_or(|(a, b)| u + 1.5 < a || u - 1.5 > b) {
            let p = path.at(i, u, 0.43);
            out.decor.push(decor(Shape::Box { half: Vec3::new(1.5, 0.43, 0.3) }, p, rot, "#c9c2b4", true));
            // Orange and white stripes.
            out.decor.push(decor(Shape::Box { half: Vec3::new(1.3, 0.08, 0.31) }, p + Vec3::Y * 0.2, rot, "#e8722c", false));
        }
        u += 3.05;
    }
}

/// A road-closed sign on two posts facing along `-t` (toward oncoming drivers).
fn sign(at: Vec3, t: Vec3, text: &str, out: &mut Out) {
    let rot = Quat::from_rotation_y(t.x.atan2(t.z));
    let side = rot * Vec3::X;
    for s in [-1.6f32, 1.6] {
        out.decor.push(decor(
            Shape::Cylinder { half_height: 1.2, radius: 0.06 },
            at + side * s + Vec3::Y * 1.2,
            Quat::IDENTITY,
            "#5a6066",
            false,
        ));
    }
    out.decor.push(decor(Shape::Box { half: Vec3::new(2.2, 0.55, 0.05) }, at + Vec3::Y * 2.7, rot, "#e8722c", false));
    out.labels.push(Label {
        text: text.into(),
        pos: at + Vec3::Y * 2.7 - t * 0.08,
        size: 0.42,
        color: Color::hex("#1d1f22"),
        mode: LabelMode::Billboard,
        facing: Default::default(),
    });
}

/// Builds every freeway in the district; returns their geometry.
pub(crate) fn build(def: &DistrictDef, grid: &Grid, terrain: &Terrain, out: &mut Out) -> Vec<FreewayGeom> {
    def.freeway.iter().map(|f| one(def, grid, terrain, f, out)).collect()
}

fn one(def: &DistrictDef, grid: &Grid, terrain: &Terrain, f: &Freeway, out: &mut Out) -> FreewayGeom {
    let world = |c: [f32; 2]| grid.origin + Vec2::new(c[0], c[1]) * grid.cell;
    let cp: Vec<Vec2> = f.points.iter().map(|c| world(*c)).collect();
    let flat = smooth(&cp, STEP);
    let top = ROAD_Y + f.height;
    let deck = path(&flat, |_| top);
    let n = deck.len();
    let c = paint();
    // Ramps first: their level stretches open gaps in the deck's barriers.
    let mut gap_r = vec![false; n];
    let mut gap_l = vec![false; n];
    let mut ramps = Vec::new();
    for r in &f.ramps {
        let (a, b) = (world(r.from), world(r.to));
        let len = a.distance(b);
        let flat_r = smooth(&[a, b], STEP);
        let h = f.height;
        let rp = path(&flat_r, |s| {
            if s <= MERGE {
                ROAD_Y + h
            } else if s >= len - FLAT_END {
                ROAD_Y + 0.03
            } else {
                let t = smoothstep((s - MERGE) / (len - FLAT_END - MERGE));
                ROAD_Y + h * (1.0 - t) + 0.03 * t
            }
        });
        // The nearest deck sample to the ramp's deck end, and which side of the ramp the deck is.
        let nearest = |p: Vec3| {
            (0..n).min_by(|&i, &j| deck.points[i].distance_squared(p).total_cmp(&deck.points[j].distance_squared(p))).unwrap()
        };
        let k = nearest(rp.points[0]);
        let inner = if (deck.points[k] - rp.points[0]).dot(rp.right[0]) > 0.0 { 1.0 } else { -1.0 };
        for i in 0..rp.len() {
            if rp.station[i] <= MERGE + STEP {
                let j = nearest(rp.points[i]);
                let side = (rp.points[i] - deck.points[j]).dot(deck.right[j]);
                if side > 0.0 {
                    gap_r[j] = true;
                    gap_r[j.saturating_sub(1)] = true;
                } else {
                    gap_l[j] = true;
                    gap_l[j.saturating_sub(1)] = true;
                }
            }
        }
        ramps.push(RampGeom { kind: r.kind, path: rp, inner, deck_sample: k });
    }
    let mut bufs: BTreeMap<(i32, i32), MeshBuf> = BTreeMap::new();
    let key_of = |p: &Path, i: usize| {
        let m = (p.points[i] + p.points[i + 1]) * 0.5;
        chunk_key(Vec2::new(m.x, m.z))
    };
    for i in 0..n - 1 {
        let prof = deck_profile(gap_r[i], gap_l[i], i % 4 == 0, &c);
        sweep(bufs.entry(key_of(&deck, i)).or_default(), &deck, i, &prof);
    }
    for r in &ramps {
        for i in 0..r.path.len() - 1 {
            let level = r.path.station[i] < MERGE;
            let prof = ramp_profile(level && r.inner > 0.0, level && r.inner < 0.0, &c);
            sweep(bufs.entry(key_of(&r.path, i)).or_default(), &r.path, i, &prof);
        }
    }
    // The broken end: the slab's raw face and rebar sticking out of it.
    if f.broken_end {
        let i = n - 1;
        let t = deck.tangent(i);
        let v = [deck.at(i, -HALF, 0.0), deck.at(i, -HALF, -SLAB), deck.at(i, HALF, -SLAB), deck.at(i, HALF, 0.0)];
        let p = deck.points[i];
        bufs.entry(chunk_key(Vec2::new(p.x, p.z))).or_default().quad(v, [t; 4], [Color::hex("#7d776c").0; 4], t);
        let mut r = rng(def.seed, 0xbe0, i as i32, 0);
        for _ in 0..22 {
            let (u, v) = (r.range(-HALF + 0.5, HALF - 0.5), r.range(-SLAB + 0.15, -0.15));
            let len = r.range(0.6, 2.6);
            let dir = (t + Vec3::new(r.range(-0.25, 0.25), r.range(-0.5, 0.15), r.range(-0.25, 0.25))).normalize();
            let rot = Quat::from_rotation_arc(Vec3::Y, dir);
            out.decor.push(decor(
                Shape::Cylinder { half_height: len * 0.5, radius: 0.025 },
                deck.at(i, u, v) + dir * len * 0.5,
                rot,
                "#6e4a35",
                false,
            ));
        }
        // Closed to traffic well before the end, with a car-wide gap someone has pushed open.
        let k = n.saturating_sub((150.0 / STEP) as usize);
        closure(&deck, k, -HALF, HALF, Some((3.0, 8.5)), out);
        sign(deck.at(k, -9.0, 0.0) - deck.tangent(k) * 4.0, -deck.tangent(k), "ROAD CLOSED", out);
        for j in k + 4..n - 4 {
            if j % 5 == 0 {
                let mut r = rng(def.seed, 0xc0e, j as i32, 0);
                let u = r.range(-13.0, 13.0);
                out.decor.push(decor(
                    Shape::Cylinder { half_height: 0.36, radius: 0.16 },
                    deck.at(j, u, 0.36),
                    Quat::IDENTITY,
                    "#ef6c2a",
                    false,
                ));
            }
        }
    }
    if f.closed_start {
        closure(&deck, 2, -HALF, HALF, None, out);
        sign(deck.at(6, 9.0, 0.0), deck.tangent(6), "ROAD CLOSED - FREEWAY WIDENING", out);
        sign(deck.at(6, -9.0, 0.0), deck.tangent(6), "ROAD CLOSED - FREEWAY WIDENING", out);
    }
    // Piers every 30 m (not standing in a road), lamps on the median every 45 m.
    let bottom = top - SLAB;
    let mut i = 5;
    while i < n - 2 {
        let p = deck.points[i];
        let flat = Vec2::new(p.x, p.z);
        if grid.road(grid.cell_of(flat)).is_some_and(|r| r.drivable()) {
            i += 1;
            continue;
        }
        let ground = crate::ground_at(def, grid, terrain, flat).min(ROAD_Y);
        let t = deck.tangent(i);
        let rot = Quat::from_rotation_y(t.x.atan2(t.z));
        let cap_y = bottom - 0.7;
        let col_h = (cap_y - 0.7 - ground).max(0.5);
        out.decor.push(decor(
            Shape::Box { half: Vec3::new(0.85, col_h * 0.5, 1.5) },
            Vec3::new(p.x, ground + col_h * 0.5, p.z),
            rot,
            "#a8a297",
            true,
        ));
        out.decor.push(decor(
            Shape::Box { half: Vec3::new(HALF - 1.2, 0.7, 1.0) },
            Vec3::new(p.x, cap_y, p.z),
            rot,
            "#a8a297",
            true,
        ));
        i += 10;
    }
    for i in (8..n - 2).step_by(15) {
        // A two-armed lamp on the median, arms over both carriageways.
        let at = deck.at(i, 0.0, 0.85);
        out.props.push(crate::dress::prop("LIB/gtf_city/street_lamp_double", at, crate::dress::yaw_to(deck.right[i]), 1.0, 0));
    }
    patches(bufs, out);
    FreewayGeom {
        key: f.key.clone(),
        name: f.name.clone(),
        deck,
        lanes: f.lanes,
        ramps,
        broken_end: f.broken_end,
        closed_start: f.closed_start,
    }
}
