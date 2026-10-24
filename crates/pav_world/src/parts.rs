//! Landmarks built from parts (PLAN.md §6.6.4: authored assemblies and interiors): rooms with
//! walls, doors, glass, a floor, a roof and lights; solid masses; walls, fences and railings with
//! gates; slabs and canopies on columns; ramps cars drive up; library props; parking places;
//! signs with neon; light fittings; Ferris wheels; ships' hulls. Everything is placed in the
//! landmark's frame (metres from the north-west corner of its cells, heights above its ground)
//! and baked into the chunks like the rest of the city, plus what a game needs at runtime:
//! breakable panes, lot places, turning wheels, and marks for the walk map (interiors people can
//! stand in, doors, yards) and cover.

use std::collections::BTreeMap;

use glam::{IVec2, Quat, Vec2, Vec3};
use pav_core::Color;
use pav_core::shape::{Look, Shape};
use pav_core::statics::{Block, CHUNK_SIZE, Decor, Glow, StaticProp, block_flags};
use pav_core::zones::{Label, LabelMode};

use crate::def::{
    DistrictDef, Fence, Glass, Hull, Lamp, Landmark, Mass, Opening, Park, PropPart, RampPart, Room, Side, SignPart, Slab, Wall,
    WallKind, WheelPart,
};
use crate::grid::Grid;
use crate::ground::{LOT_Y, MeshBuf, chunk_key, patches};
use crate::terrain::Terrain;
use crate::{Building, LotPlace, Out, Pane, WheelGeom};

/// A room's floor stands this far over its landmark's ground (paving tops out at 2 cm: the
/// two never fight for the same plane).
const FLOOR: f32 = 0.04;

/// What a walk-map mark makes of the tiles under it (applied in order, after buildings).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// A yard, a lot, a deck: open ground.
    Open,
    /// A room's floor and its doorways: walkable, but not where crowds spawn or wander.
    Indoor,
    Blocked,
}

/// A rectangle of the walk map a landmark marks, and the height people stand at there (its
/// paving, deck or floor; for `Blocked`, its ground).
#[derive(Clone, Copy, Debug)]
pub struct MarkRect {
    pub min: Vec2,
    pub max: Vec2,
    pub mark: Mark,
    pub y: f32,
}

fn mark(out: &mut Out, min: Vec2, max: Vec2, mark: Mark, y: f32) {
    out.marks.push(MarkRect { min: min.min(max), max: min.max(max), mark, y });
}

/// A landmark's frame: its north-west corner (x, z), its ground's height, its zone and index.
struct Frame {
    o: Vec2,
    base: f32,
    zone: u16,
    index: u16,
    /// What its open ground is on the walk map (`Indoor` for a gated yard).
    open: Mark,
}

impl Frame {
    fn p(&self, at: [f32; 2]) -> Vec2 {
        self.o + Vec2::new(at[0], at[1])
    }

    fn v(&self, x: f32, y: f32, z: f32) -> Vec3 {
        Vec3::new(self.o.x + x, self.base + y, self.o.y + z)
    }

    /// A rectangle in world (x, z), corners in order.
    fn rect(&self, r: [f32; 4]) -> (Vec2, Vec2) {
        let (a, b) = (self.p([r[0], r[1]]), self.p([r[2], r[3]]));
        (a.min(b), a.max(b))
    }
}

/// A colour or a material: `#rrggbb`, or `MAT/name` worn over `fallback` (or over the colour
/// after it: `MAT/name #rrggbb`).
fn surface(s: &str, fallback: &str) -> (Color, Option<String>) {
    let mut words = s.split_whitespace();
    let first = words.next().unwrap_or("");
    if first.starts_with("MAT/") {
        let base = words.next().and_then(Color::try_hex).unwrap_or(Color::hex(fallback));
        (base, Some(first.to_string()))
    } else {
        (Color::try_hex(first).unwrap_or(Color::hex(fallback)), None)
    }
}

/// A part's outer surface: its colour, with its `material` worn over it when it has one
/// (`MAT/name` over the colour, or `MAT/name #rrggbb` over its own base colour).
fn faced(color: &str, material: Option<&str>, fallback: &str) -> (Color, Option<String>) {
    let (c, m) = surface(color, fallback);
    let mut words = material.unwrap_or("").split_whitespace();
    match words.next() {
        Some(name) if name.starts_with("MAT/") => (words.next().and_then(Color::try_hex).unwrap_or(c), Some(name.to_string())),
        _ => (c, m),
    }
}

/// Whether a surface string reads (`#rrggbb`, `MAT/name`, `MAT/name #rrggbb`).
fn surface_ok(s: &str) -> bool {
    let w: Vec<&str> = s.split_whitespace().collect();
    match w.as_slice() {
        [c] => c.starts_with("MAT/") || Color::try_hex(c).is_some(),
        [m, c] => m.starts_with("MAT/") && Color::try_hex(c).is_some(),
        _ => false,
    }
}

/// Pushes a floor cut at the chunk lines, so whatever stands on it is held up by the piece in
/// its own chunk (a block lives in the chunk of its middle).
fn push_floor(out: &mut Out, b: Block) {
    let cuts = |lo: f32, hi: f32| {
        let mut v = Vec::new();
        let mut a = lo;
        while a < hi - 1e-3 {
            let e = (((a / CHUNK_SIZE).floor() + 1.0) * CHUNK_SIZE).min(hi);
            v.push((a, e));
            a = e;
        }
        v
    };
    for (x0, x1) in cuts(b.min.x, b.max.x) {
        for (z0, z1) in cuts(b.min.z, b.max.z) {
            let mut piece = b.clone();
            (piece.min.x, piece.max.x, piece.min.z, piece.max.z) = (x0, x1, z0, z1);
            out.blocks.push(piece);
        }
    }
}

fn block(min: Vec3, max: Vec3, (color, material): &(Color, Option<String>)) -> Block {
    let mut b = Block::new(min, max, *color);
    b.material = material.clone();
    b
}

/// A box of decor (`solid`: with a collider), turned by `rot` about its middle.
fn decor(shape: Shape, pos: Vec3, rot: Quat, color: Color, solid: bool) -> Decor {
    Decor {
        shape,
        pos,
        rot,
        color,
        look: Look::Cel,
        emissive: 0.0,
        solid,
        collider: None,
        sway: Default::default(),
        material: None,
        glow: Default::default(),
    }
}

fn boxed(min: Vec3, max: Vec3, color: Color) -> Decor {
    decor(Shape::Box { half: (max - min).abs() * 0.5 }, (min + max) * 0.5, Quat::IDENTITY, color, false)
}

/// A bar from `a` to `b`, `w` thick (a leg, a brace, a rail).
fn bar(a: Vec3, b: Vec3, w: f32, color: Color, solid: bool) -> Decor {
    let d = b - a;
    let len = d.length().max(1e-3);
    let rot = Quat::from_rotation_arc(Vec3::Y, d / len);
    decor(Shape::Box { half: Vec3::new(w * 0.5, len * 0.5, w * 0.5) }, (a + b) * 0.5, rot, color, solid)
}

/// The direction a yaw (degrees from north toward east) points on the ground.
fn heading(yaw: f32) -> Vec3 {
    let a = yaw.to_radians();
    Vec3::new(a.sin(), 0.0, -a.cos())
}

/// The turn that points a prop's front (+z) along a yaw (degrees from north).
fn prop_turn(yaw: f32) -> Quat {
    let d = heading(yaw);
    Quat::from_rotation_y(d.x.atan2(d.z))
}

/// About how wide a line of text is (m) at a letter height.
pub fn text_width(text: &str, size: f32) -> f32 {
    text.chars()
        .map(|c| {
            if c == ' ' {
                0.32
            } else if c.is_ascii_uppercase() || c.is_ascii_digit() {
                0.6
            } else {
                0.5
            }
        })
        .sum::<f32>()
        * size
}

/// Checks a district's landmark parts before building: colours, rectangles, assets.
pub(crate) fn check(def: &DistrictDef) -> Result<(), String> {
    for p in &def.prop {
        pav_core::props::resolve_parts(&p.asset, p.seed).map_err(|e| format!("prop {}: {e}", p.asset))?;
    }
    let color = |key: &str, what: &str, s: &str| -> Result<(), String> {
        if surface_ok(s) {
            Ok(())
        } else {
            Err(format!("landmark '{key}': {what} '{s}' is not #rrggbb, MAT/name or MAT/name #rrggbb"))
        }
    };
    let rect = |key: &str, what: &str, r: [f32; 4]| -> Result<(), String> {
        if r.iter().all(|v| v.is_finite()) && (r[2] - r[0]).abs() > 0.01 && (r[3] - r[1]).abs() > 0.01 {
            Ok(())
        } else {
            Err(format!("landmark '{key}': {what} rect {r:?} is empty"))
        }
    };
    for l in &def.landmark {
        let k = l.key.as_str();
        if let Some(g) = &l.ground {
            color(k, "ground", g)?;
        }
        for r in &l.room {
            rect(k, "room", r.rect)?;
            color(k, "room color", &r.color)?;
            if let Some(m) = &r.material {
                color(k, "room material", m)?;
            }
            for s in [&r.inside, &r.floor, &r.roof].into_iter().flatten() {
                color(k, "room surface", s)?;
            }
            for g in &r.glass {
                color(k, "glass frame", &g.frame)?;
            }
            if r.height <= 0.5 {
                return Err(format!("landmark '{k}': a room {:.1} m tall", r.height));
            }
        }
        for m in &l.mass {
            rect(k, "mass", m.rect)?;
            color(k, "mass color", &m.color)?;
            if let Some(mat) = &m.material {
                color(k, "mass material", mat)?;
            }
        }
        for w in &l.wall {
            if w.from[0] != w.to[0] && w.from[1] != w.to[1] {
                return Err(format!("landmark '{k}': wall {:?} -> {:?} runs neither along x nor along z", w.from, w.to));
            }
        }
        for f in &l.fence {
            rect(k, "fence", f.rect)?;
        }
        for s in &l.slab {
            rect(k, "slab", s.rect)?;
            color(k, "slab color", &s.color)?;
        }
        for r in &l.ramp {
            rect(k, "ramp", r.rect)?;
        }
        for p in &l.prop {
            pav_core::props::resolve_parts(&p.asset, p.seed).map_err(|e| format!("landmark '{k}': prop {}: {e}", p.asset))?;
        }
        for w in &l.wheel {
            for a in [&w.rim, &w.car] {
                pav_core::props::resolve_parts(a, 0).map_err(|e| format!("landmark '{k}': wheel {a}: {e}"))?;
            }
        }
        for s in &l.sign {
            color(k, "sign color", &s.color)?;
            for c in [&s.board, &s.neon].into_iter().flatten() {
                color(k, "sign board or neon", c)?;
            }
        }
    }
    Ok(())
}

/// Builds landmark `index` from its parts.
pub(crate) fn build(grid: &Grid, terrain: &Terrain, index: usize, l: &Landmark, zone: u16, out: &mut Out) {
    let [[x0, z0], [x1, z1]] = l.cells;
    let o = grid.corner(IVec2::new(x0, z0));
    let far = grid.corner(IVec2::new(x1 + 1, z1 + 1));
    // On a slope the landmark stands on a plinth, a little above the middle of the ground under it
    // (`level = true` flattens the ground first).
    let (lo, hi) = terrain.range(o, far);
    let floor = lo + (hi - lo) * 0.6;
    if hi - lo > 0.05 {
        out.blocks.push(Block::new(
            Vec3::new(o.x, LOT_Y + lo - 0.4, o.y),
            Vec3::new(far.x, LOT_Y + floor, far.y),
            Color::hex("#a39a88"),
        ));
    }
    let open = if l.crowds { Mark::Open } else { Mark::Indoor };
    let f = Frame { o, base: LOT_Y + floor, zone, index: index as u16, open };
    let mut meshes: BTreeMap<(i32, i32), MeshBuf> = BTreeMap::new();
    if let Some(g) = &l.ground {
        let s = surface(g, "#b9b3a6");
        push_floor(out, block(Vec3::new(o.x, f.base - 0.08, o.y), Vec3::new(far.x, f.base + 0.02, far.y), &s));
        mark(out, o, far, f.open, f.base + 0.02);
    }
    for s in &l.slab {
        slab(&f, s, out);
    }
    for m in &l.mass {
        mass(&f, m, out);
    }
    for r in &l.room {
        room(&f, r, out);
    }
    for w in &l.wall {
        wall(&f, w, out);
    }
    for fc in &l.fence {
        fence(&f, fc, out);
    }
    for r in &l.ramp {
        ramp(&f, r, &mut meshes);
    }
    for p in &l.prop {
        prop(&f, p, out);
    }
    for p in &l.park {
        park(&f, p, out);
    }
    for s in &l.sign {
        sign(&f, s, out);
    }
    for lamp in &l.lamp {
        light(&f, lamp, out);
    }
    for w in &l.wheel {
        wheel(&f, w, out);
    }
    for h in &l.hull {
        hull(&f, h, &mut meshes, out);
    }
    for lad in &l.ladder {
        ladder(&f, lad, out);
    }
    patches(meshes, out);
}

/// A ladder: rails and rungs drawn by the engine from its climbable zone, a strip against the
/// wall's face on the climber's side.
fn ladder(f: &Frame, l: &crate::def::LadderPart, out: &mut Out) {
    let n = l.side.dir();
    let c = f.p(l.at);
    let along = Vec2::new(n.y.abs(), n.x.abs()) * (l.width * 0.5);
    let (a, b) = (c - along, c + along + n * 0.3);
    let (min, max) = (a.min(b), a.max(b));
    let facing = match l.side {
        Side::North => pav_core::statics::Facing::South,
        Side::South => pav_core::statics::Facing::North,
        Side::East => pav_core::statics::Facing::West,
        Side::West => pav_core::statics::Facing::East,
    };
    out.ladders.push(pav_core::statics::Ladder {
        min: Vec3::new(min.x, f.base + l.y, min.y),
        max: Vec3::new(max.x, f.base + l.y + l.height, max.y),
        facing,
        color: Color::try_hex(&l.color).unwrap_or(Color::hex("#8e9399")),
    });
}

/// A slab, and the columns under it.
fn slab(f: &Frame, s: &Slab, out: &mut Out) {
    let (min, max) = f.rect(s.rect);
    let y0 = f.base + s.y;
    let (lo, hi) = (Vec3::new(min.x, y0, min.y), Vec3::new(max.x, y0 + s.thick, max.y));
    let sf = surface(&s.color, "#a7a39a");
    if s.glow > 0.0 {
        // Glowing slabs (lit canopies) are decor: blocks have no glow.
        let mut d = boxed(lo, hi, sf.0);
        d.solid = s.solid;
        d.emissive = s.glow;
        d.look = Look::Unlit;
        d.material = sf.1.clone();
        d.glow = if s.night { Glow::Night } else { Glow::Always };
        out.decor.push(d);
    } else {
        let b = block(lo, hi, &sf);
        push_floor(out, if s.solid { b } else { b.with_flags(block_flags::GHOST) });
    }
    if s.walk {
        mark(out, min, max, f.open, y0 + s.thick);
    }
    if s.piles > 0.0 {
        // Timber piles round the edge, down through the water to the sea bed.
        let pile = Color::hex("#5d4b3a");
        let (a, b) = (min + Vec2::splat(0.3), max - Vec2::splat(0.3));
        let top = y0;
        let bottom = crate::ground::SEABED_Y;
        let h = (top - bottom).max(0.5);
        let n = |len: f32| ((len / s.piles).round() as i32).max(1);
        let (nx, nz) = (n(b.x - a.x), n(b.y - a.y));
        let mut at: Vec<Vec2> = Vec::new();
        for i in 0..=nx {
            let x = a.x + (b.x - a.x) * i as f32 / nx as f32;
            at.push(Vec2::new(x, a.y));
            at.push(Vec2::new(x, b.y));
        }
        for k in 1..nz {
            let z = a.y + (b.y - a.y) * k as f32 / nz as f32;
            at.push(Vec2::new(a.x, z));
            at.push(Vec2::new(b.x, z));
        }
        for p in at {
            out.decor.push(decor(
                Shape::Cylinder { half_height: h * 0.5, radius: 0.22 },
                Vec3::new(p.x, bottom + h * 0.5, p.y),
                Quat::IDENTITY,
                pile,
                false,
            ));
        }
    }
    if s.columns > 0.0 && s.y > 0.3 {
        let col = s.column_color.as_deref().map_or(sf.0, |c| surface(c, "#a7a39a").0);
        let inset = 0.45;
        let (a, b) = (min + Vec2::splat(inset), max - Vec2::splat(inset));
        let mut posts: Vec<Vec2> = Vec::new();
        let n = |len: f32| ((len / s.columns).round() as i32).max(1);
        let (nx, nz) = (n(b.x - a.x), n(b.y - a.y));
        for i in 0..=nx {
            let x = a.x + (b.x - a.x) * i as f32 / nx as f32;
            posts.push(Vec2::new(x, a.y));
            posts.push(Vec2::new(x, b.y));
        }
        for k in 1..nz {
            let z = a.y + (b.y - a.y) * k as f32 / nz as f32;
            posts.push(Vec2::new(a.x, z));
            posts.push(Vec2::new(b.x, z));
        }
        for p in posts {
            let h = s.y * 0.5;
            out.decor.push(decor(
                Shape::Cylinder { half_height: h, radius: 0.22 },
                Vec3::new(p.x, f.base + h, p.y),
                Quat::IDENTITY,
                col,
                true,
            ));
        }
    }
}

fn mass(f: &Frame, m: &Mass, out: &mut Out) {
    let (min, max) = f.rect(m.rect);
    let y0 = f.base + m.y;
    let sf = faced(&m.color, m.material.as_deref(), "#e9dcc4");
    let b = block(Vec3::new(min.x, y0, min.y), Vec3::new(max.x, y0 + m.height, max.y), &sf);
    out.blocks.push(if m.solid { b } else { b.with_flags(block_flags::GHOST) });
    let mut top = m.height;
    if let Some(cap) = &m.cap {
        let c = surface(cap, "#d8d0c2");
        let g = Vec2::splat(0.25);
        out.blocks.push(block(
            Vec3::new(min.x - g.x, y0 + m.height, min.y - g.y),
            Vec3::new(max.x + g.x, y0 + m.height + 0.5, max.y + g.y),
            &c,
        ));
        top += 0.5;
    }
    if m.building {
        out.buildings.push(Building { min, max, base: y0, height: top, zone: f.zone, color: sf.0.0 });
    }
    // On the ground it blocks the walk map (over any paving marked before it), and the tiles
    // hard against it, so no step between two tiles crosses it.
    if m.solid && m.y < 1.0 {
        mark(out, min - Vec2::splat(0.5), max + Vec2::splat(0.5), Mark::Blocked, f.base);
    }
}

/// One opening cut into a wall, along its axis (world coordinates).
struct Cut<'a> {
    s0: f32,
    s1: f32,
    door: Option<&'a Opening>,
    glass: Option<&'a Glass>,
}

/// A room: its walls round its outline (doors and glass cut in), roof, floor, lining, lights.
fn room(f: &Frame, r: &Room, out: &mut Out) {
    let (min, max) = f.rect(r.rect);
    let t = r.wall.clamp(0.1, 1.0);
    let y0 = f.base + r.y;
    let top = y0 + r.height;
    let roof_t = 0.3;
    let wall_top = top - roof_t;
    let outer = faced(&r.color, r.material.as_deref(), "#e9dcc4");
    let lining = r.inside.as_deref().map(|c| surface(c, "#efe9df"));
    let line = 0.04;
    // Its footprint and a ring of tiles round it block the walk map (over any paving), so no
    // step between two tiles goes through a wall; its floor and doorways open it again.
    if r.y < 1.0 {
        mark(out, min - Vec2::splat(1.0), max + Vec2::splat(1.0), Mark::Blocked, f.base);
    }
    for side in [Side::North, Side::East, Side::South, Side::West] {
        // The wall's extent along its axis and its strip across (east and west walls fit
        // between the north and south ones).
        let (a0, a1, c0, c1) = match side {
            Side::North => (min.x, max.x, min.y, min.y + t),
            Side::South => (min.x, max.x, max.y - t, max.y),
            Side::West => (min.y + t, max.y - t, min.x, min.x + t),
            Side::East => (min.y + t, max.y - t, max.x - t, max.x),
        };
        // The side the room is on, across the strip.
        let inner_high = matches!(side, Side::North | Side::West);
        let along_x = side.along_x();
        let origin = if along_x { f.o.x } else { f.o.y };
        let world = |s0: f32, s1: f32, k0: f32, k1: f32, ya: f32, yb: f32| -> (Vec3, Vec3) {
            if along_x { (Vec3::new(s0, ya, k0), Vec3::new(s1, yb, k1)) } else { (Vec3::new(k0, ya, s0), Vec3::new(k1, yb, s1)) }
        };
        // A solid piece of wall (with its lining on the inner face).
        let solid = |s0: f32, s1: f32, ya: f32, yb: f32, out: &mut Out| {
            if s1 - s0 < 0.01 || yb - ya < 0.01 {
                return;
            }
            match &lining {
                Some(lin) => {
                    let (k0, k1, l0, l1) =
                        if inner_high { (c0, c1 - line, c1 - line, c1) } else { (c0 + line, c1, c0, c0 + line) };
                    let (a, b) = world(s0, s1, k0, k1, ya, yb);
                    out.blocks.push(block(a, b, &outer));
                    let (a, b) = world(s0, s1, l0, l1, ya, yb);
                    out.blocks.push(block(a, b, lin).with_flags(block_flags::GHOST));
                }
                None => {
                    let (a, b) = world(s0, s1, c0, c1, ya, yb);
                    out.blocks.push(block(a, b, &outer));
                }
            }
        };
        let mut cuts: Vec<Cut> = Vec::new();
        for d in r.door.iter().filter(|d| d.side == side) {
            let m = origin + d.at;
            cuts.push(Cut { s0: m - d.width * 0.5, s1: m + d.width * 0.5, door: Some(d), glass: None });
        }
        for g in r.glass.iter().filter(|g| g.side == side) {
            cuts.push(Cut { s0: origin + g.from.min(g.to), s1: origin + g.from.max(g.to), door: None, glass: Some(g) });
        }
        cuts.sort_by(|a, b| a.s0.total_cmp(&b.s0));
        let mut s = a0;
        for c in &cuts {
            let (s0, s1) = (c.s0.max(s).max(a0), c.s1.min(a1));
            if s1 <= s0 {
                continue;
            }
            solid(s, s0, y0, wall_top, out);
            if let Some(d) = c.door {
                solid(s0, s1, y0 + d.height, wall_top, out);
                // Its doorway on the walk map, through the wall and a step either side.
                let mid = (s0 + s1) * 0.5;
                let half = ((s1 - s0) * 0.5).max(1.0);
                let (a, b) = world(mid - half, mid + half, c0 - 1.6, c1 + 1.6, 0.0, 0.0);
                if r.walk && r.y < 1.0 {
                    mark(out, Vec2::new(a.x, a.z), Vec2::new(b.x, b.z), Mark::Indoor, y0 + FLOOR);
                }
                if d.width >= 2.6 {
                    // Wide enough for a vehicle: no parking at the kerb outside.
                    let (a, b) = world(s0 - 1.5, s1 + 1.5, c0 - 14.0, c1 + 14.0, 0.0, 0.0);
                    out.driveways.push((Vec2::new(a.x, a.z), Vec2::new(b.x, b.z)));
                }
            }
            if let Some(g) = c.glass {
                let ceiling = r.height - roof_t;
                let gtop = g.top.unwrap_or(ceiling - 0.5).min(ceiling);
                solid(s0, s1, y0, y0 + g.sill, out);
                solid(s0, s1, y0 + gtop, wall_top, out);
                glass(f, g, (s0, s1), (c0, c1), (y0 + g.sill, y0 + gtop), &world, out);
            }
            s = s1;
        }
        solid(s, a1, y0, wall_top, out);
    }
    // The roof, its parapet, the ceiling's lining.
    let roof = surface(r.roof.as_deref().unwrap_or("#8f8b84"), "#8f8b84");
    out.blocks.push(block(Vec3::new(min.x - 0.05, wall_top, min.y - 0.05), Vec3::new(max.x + 0.05, top, max.y + 0.05), &roof));
    if r.parapet > 0.0 {
        let (p0, p1) = (top, top + r.parapet);
        let w = 0.25;
        for (a, b) in [
            (Vec2::new(min.x - 0.05, min.y - 0.05), Vec2::new(max.x + 0.05, min.y - 0.05 + w)),
            (Vec2::new(min.x - 0.05, max.y + 0.05 - w), Vec2::new(max.x + 0.05, max.y + 0.05)),
            (Vec2::new(min.x - 0.05, min.y + w), Vec2::new(min.x - 0.05 + w, max.y - w)),
            (Vec2::new(max.x + 0.05 - w, min.y + w), Vec2::new(max.x + 0.05, max.y - w)),
        ] {
            out.decor.push(boxed(Vec3::new(a.x, p0, a.y), Vec3::new(b.x, p1, b.y), outer.0));
        }
    }
    let (imin, imax) = (min + Vec2::splat(t), max - Vec2::splat(t));
    if let Some(lin) = &lining {
        out.blocks.push(
            block(Vec3::new(imin.x, wall_top - line, imin.y), Vec3::new(imax.x, wall_top, imax.y), lin)
                .with_flags(block_flags::GHOST),
        );
    }
    if let Some(fl) = &r.floor {
        let fs = surface(fl, "#8a8278");
        let bottom = if r.y > 0.05 { y0 - 0.25 } else { y0 - 0.1 };
        push_floor(out, block(Vec3::new(imin.x, bottom, imin.y), Vec3::new(imax.x, y0 + FLOOR, imax.y), &fs));
    }
    // Lights down the middle of the longer side.
    if r.lights > 0 {
        let size = imax - imin;
        let long_x = size.x >= size.y;
        let n = r.lights as f32;
        for i in 0..r.lights {
            let k = (i as f32 + 0.5) / n;
            let c = if long_x {
                Vec2::new(imin.x + size.x * k, (imin.y + imax.y) * 0.5)
            } else {
                Vec2::new((imin.x + imax.x) * 0.5, imin.y + size.y * k)
            };
            let y = wall_top - line - 0.05;
            let half = if long_x { Vec3::new(0.6, 0.04, 0.22) } else { Vec3::new(0.22, 0.04, 0.6) };
            let mut d = decor(Shape::Box { half }, Vec3::new(c.x, y, c.y), Quat::IDENTITY, Color::hex("#fff3dc"), false);
            d.emissive = 1.1;
            d.look = Look::Unlit;
            out.decor.push(d);
        }
    }
    if r.building {
        out.buildings.push(Building { min, max, base: y0, height: r.height + r.parapet, zone: f.zone, color: outer.0.0 });
    }
    if r.walk && r.y < 1.0 {
        mark(out, imin, imax, Mark::Indoor, y0 + FLOOR);
    }
}

/// Glass in a wall: invisible panes (colliders) between mullions, a frame round them, a frosted
/// band. Panes and bands are destructible (explosions break them). Breakable glass is listed for
/// the game pane by pane, each with its mullions and its stretch of the sill bar, so a car going
/// through takes the whole bay with it.
#[allow(clippy::too_many_arguments)]
fn glass(
    f: &Frame,
    g: &Glass,
    (s0, s1): (f32, f32),
    (c0, c1): (f32, f32),
    (ya, yb): (f32, f32),
    world: &dyn Fn(f32, f32, f32, f32, f32, f32) -> (Vec3, Vec3),
    out: &mut Out,
) {
    let len = s1 - s0;
    let n = if g.panes > 0 { g.panes } else { ((len / 2.4).round() as u32).max(1) };
    let w = len / n as f32;
    let frame = surface(&g.frame, "#2d3236");
    let mid = (c0 + c1) * 0.5;
    let fw = 0.09;
    let breaks = if g.breakable { block_flags::DESTRUCTIBLE } else { 0 };
    // Mullions at both ends and between the panes, a bar along the sill (per pane) and the head.
    for i in 0..=n {
        let s = s0 + w * i as f32;
        let (a, b) = world((s - fw * 0.5).max(s0), (s + fw * 0.5).min(s1), mid - 0.07, mid + 0.07, ya, yb);
        out.blocks.push(block(a, b, &frame).with_flags(breaks));
    }
    let (a, b) = world(s0, s1, mid - 0.08, mid + 0.08, yb - 0.07, yb);
    out.blocks.push(block(a, b, &frame));
    for i in 0..n {
        let (q0, q1) = (s0 + w * i as f32, s0 + w * (i + 1) as f32);
        let (a, b) = world(q0, q1, mid - 0.08, mid + 0.08, ya, ya + 0.07);
        out.blocks.push(block(a, b, &frame).with_flags(breaks));
        let (p0, p1) = (q0 + fw * 0.5, q1 - fw * 0.5);
        let (a, b) = world(p0, p1, mid - 0.05, mid + 0.05, ya + 0.07, yb - 0.07);
        // An invisible pane: it stops people, shots and cars until it breaks.
        out.blocks.push(Block::new(a, b, Color::hex("#9fc4d6")).with_flags(block_flags::HIDDEN | block_flags::DESTRUCTIBLE));
        if g.band > 0.0 && g.band + 0.3 < yb - ya {
            let y = ya - g.sill + g.band;
            let (ba, bb) = world(p0, p1, mid - 0.012, mid + 0.012, y, y + 0.28);
            out.blocks.push(Block::new(ba, bb, Color::hex("#dfe9ec")).with_flags(block_flags::GHOST | block_flags::DESTRUCTIBLE));
        }
        if g.breakable {
            // The pane, its band, its stretch of sill and the mullions either side (their
            // middles), not the head.
            let (a, b) = world(q0 - 0.01, q1 + 0.01, mid - 0.1, mid + 0.1, ya - 0.01, yb - 0.1);
            out.panes.push(Pane { min: a.min(b), max: a.max(b), landmark: f.index });
        }
    }
    if g.breakable {
        // A car may come out through it: no parking at the kerb outside.
        let (a, b) = world(s0, s1, c0 - 14.0, c1 + 14.0, 0.0, 0.0);
        out.driveways.push((Vec2::new(a.x, a.z), Vec2::new(b.x, b.z)));
    }
}

/// A wall line (along x or z), cut at its gaps.
fn wall(f: &Frame, w: &Wall, out: &mut Out) {
    let along_x = w.from[1] == w.to[1];
    let (a, b) = (f.p(w.from), f.p(w.to));
    let (s_lo, s_hi) = if along_x { (a.x.min(b.x), a.x.max(b.x)) } else { (a.y.min(b.y), a.y.max(b.y)) };
    let across = if along_x { a.y } else { a.x };
    let origin = if along_x { f.o.x } else { f.o.y };
    let mut gaps: Vec<(f32, f32)> =
        w.gap.iter().map(|g| (origin + g.at - g.width * 0.5, origin + g.at + g.width * 0.5)).collect();
    gaps.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut runs = Vec::new();
    let mut s = s_lo;
    for (g0, g1) in gaps {
        if g0 > s {
            runs.push((s, g0.min(s_hi)));
        }
        s = s.max(g1);
        if (g1 - g0) >= 2.6 && w.kind != WallKind::Rail {
            // A gate a vehicle fits through: no parking at the kerb outside.
            let (p, q) = if along_x {
                (Vec2::new(g0 - 1.5, across - 14.0), Vec2::new(g1 + 1.5, across + 14.0))
            } else {
                (Vec2::new(across - 14.0, g0 - 1.5), Vec2::new(across + 14.0, g1 + 1.5))
            };
            out.driveways.push((p, q));
        }
    }
    if s < s_hi {
        runs.push((s, s_hi));
    }
    let y0 = f.base + w.y;
    let half = w.thick * 0.5;
    let rect = |s0: f32, s1: f32, h: f32| -> (Vec2, Vec2) {
        if along_x {
            (Vec2::new(s0, across - h), Vec2::new(s1, across + h))
        } else {
            (Vec2::new(across - h, s0), Vec2::new(across + h, s1))
        }
    };
    for (s0, s1) in runs {
        if s1 - s0 < 0.05 {
            continue;
        }
        let (p, q) = rect(s0, s1, half);
        match w.kind {
            WallKind::Wall => {
                let mut sf = surface(w.color.as_deref().unwrap_or("#d9cdb6"), "#d9cdb6");
                if sf.1.is_none() {
                    sf.1 = w.material.clone();
                }
                out.blocks.push(block(Vec3::new(p.x, y0, p.y), Vec3::new(q.x, y0 + w.height, q.y), &sf));
            }
            WallKind::Fence => {
                let fabric = Color::try_hex(w.color.as_deref().unwrap_or("#9aa2a6")).unwrap_or(Color::hex("#9aa2a6"));
                let (p, q) = rect(s0, s1, 0.025);
                let mut b = Block::new(Vec3::new(p.x, y0, p.y), Vec3::new(q.x, y0 + w.height, q.y), fabric);
                b.material = Some(w.material.clone().unwrap_or_else(|| "MAT/gtf/chain_link".into()));
                out.blocks.push(b);
                let post = Color::hex("#7d8488");
                let n = ((s1 - s0) / 3.0).ceil().max(1.0) as i32;
                for i in 0..=n {
                    let s = s0 + (s1 - s0) * i as f32 / n as f32;
                    let at = if along_x { Vec3::new(s, y0, across) } else { Vec3::new(across, y0, s) };
                    out.decor.push(decor(
                        Shape::Cylinder { half_height: w.height * 0.5 + 0.05, radius: 0.045 },
                        at + Vec3::Y * (w.height * 0.5 + 0.05),
                        Quat::IDENTITY,
                        post,
                        false,
                    ));
                }
                let (p, q) = rect(s0, s1, 0.035);
                out.decor.push(boxed(Vec3::new(p.x, y0 + w.height, p.y), Vec3::new(q.x, y0 + w.height + 0.07, q.y), post));
            }
            WallKind::Rail => {
                let col = Color::try_hex(w.color.as_deref().unwrap_or("#e9e4d8")).unwrap_or(Color::hex("#e9e4d8"));
                // Solid to people, drawn as posts and two rails.
                let (p, q) = rect(s0, s1, 0.05);
                out.blocks.push(
                    Block::new(Vec3::new(p.x, y0, p.y), Vec3::new(q.x, y0 + w.height, q.y), col).with_flags(block_flags::HIDDEN),
                );
                for (h, t) in [(w.height - 0.06, 0.07), (w.height * 0.5, 0.05)] {
                    let (p, q) = rect(s0, s1, t * 0.5 + 0.01);
                    out.decor.push(boxed(Vec3::new(p.x, y0 + h - t * 0.5, p.y), Vec3::new(q.x, y0 + h + t * 0.5, q.y), col));
                }
                let n = ((s1 - s0) / 2.5).ceil().max(1.0) as i32;
                for i in 0..=n {
                    let s = s0 + (s1 - s0) * i as f32 / n as f32;
                    let at = if along_x { Vec3::new(s, y0, across) } else { Vec3::new(across, y0, s) };
                    out.decor.push(boxed(at - Vec3::new(0.04, 0.0, 0.04), at + Vec3::new(0.04, w.height, 0.04), col));
                }
            }
        }
        // Thin lines on the ground block the 2 m walk tiles they cross (the walk map is of the
        // ground: a deck's parapet leaves it alone).
        if w.y < 1.0 {
            let (bp, bq) = rect(s0, s1, half.max(1.0));
            mark(out, bp, bq, Mark::Blocked, y0);
        }
        if w.cover {
            out.cover.push(Building { min: p, max: q, base: y0, height: w.height, zone: f.zone, color: [0.0; 3] });
        }
    }
}

/// A fence round a rectangle: four wall lines, the gates as their gaps.
fn fence(f: &Frame, fc: &Fence, out: &mut Out) {
    let r = fc.rect;
    let (x0, z0, x1, z1) = (r[0].min(r[2]), r[1].min(r[3]), r[0].max(r[2]), r[1].max(r[3]));
    let gaps = |side: Side| -> Vec<crate::def::Gap> {
        fc.gate.iter().filter(|g| g.side == side).map(|g| crate::def::Gap { at: g.at, width: g.width }).collect()
    };
    let thick = fc.thick.unwrap_or(if fc.kind == WallKind::Wall { 0.3 } else { 0.08 });
    for (side, from, to) in [
        (Side::North, [x0, z0], [x1, z0]),
        (Side::South, [x0, z1], [x1, z1]),
        (Side::West, [x0, z0], [x0, z1]),
        (Side::East, [x1, z0], [x1, z1]),
    ] {
        let w = Wall {
            from,
            to,
            height: fc.height,
            y: 0.0,
            thick,
            kind: fc.kind,
            color: fc.color.clone(),
            material: fc.material.clone(),
            gap: gaps(side),
            cover: false,
        };
        wall(f, &w, out);
    }
}

/// A ramp's mesh: its sloped top, kerbs along its sides, solid sides down to the ground.
fn ramp(f: &Frame, r: &RampPart, meshes: &mut BTreeMap<(i32, i32), MeshBuf>) {
    let (min, max) = f.rect(r.rect);
    let (lo_y, hi_y) = (f.base + r.y0, f.base + r.y1);
    // Corners round the top, low end first: (a, b) at the low end, (c, d) at the high end.
    let (a, b, c, d) = match r.rise {
        Side::North => (Vec2::new(min.x, max.y), Vec2::new(max.x, max.y), Vec2::new(max.x, min.y), Vec2::new(min.x, min.y)),
        Side::South => (Vec2::new(max.x, min.y), Vec2::new(min.x, min.y), Vec2::new(min.x, max.y), Vec2::new(max.x, max.y)),
        Side::East => (Vec2::new(min.x, min.y), Vec2::new(min.x, max.y), Vec2::new(max.x, max.y), Vec2::new(max.x, min.y)),
        Side::West => (Vec2::new(max.x, max.y), Vec2::new(max.x, min.y), Vec2::new(min.x, min.y), Vec2::new(min.x, max.y)),
    };
    let at = |p: Vec2, y: f32| Vec3::new(p.x, y, p.y);
    let col = surface(&r.color, "#a7a39a").0.0;
    let side_c = Color::hex("#8d8a82").0;
    let buf = meshes.entry(chunk_key((min + max) * 0.5)).or_default();
    let top = [at(a, lo_y), at(b, lo_y), at(c, hi_y), at(d, hi_y)];
    let n = (top[1] - top[0]).cross(top[3] - top[0]).normalize_or(Vec3::Y);
    let n = if n.y < 0.0 { -n } else { n };
    buf.quad(top, [n; 4], [col; 4], n);
    // Sides down to its low end's level (a solid wedge standing there), and the high end's face.
    let ground = lo_y - 0.05;
    for (p, q) in [(a, d), (b, c)] {
        let face = [at(p, ground), at(q, ground), at(q, if q == c || q == d { hi_y } else { lo_y }), at(p, lo_y)];
        let out_dir = {
            let m = (p + q) * 0.5;
            let centre = (min + max) * 0.5;
            let d = m - centre;
            let along = (q - p).normalize_or_zero();
            let perp = d - along * d.dot(along);
            Vec3::new(perp.x, 0.0, perp.y).normalize_or(Vec3::X)
        };
        buf.quad(face, [out_dir; 4], [side_c; 4], out_dir);
    }
    let end_dir = {
        let e = ((c + d) * 0.5 - (a + b) * 0.5).normalize_or_zero();
        Vec3::new(e.x, 0.0, e.y)
    };
    buf.quad([at(d, ground), at(c, ground), at(c, hi_y), at(d, hi_y)], [end_dir; 4], [side_c; 4], end_dir);
    if r.kerbs {
        // A low kerb along each side, on the slope.
        let across = (b - a).normalize_or_zero();
        let kerb_c = Color::hex("#c9c4b8").0;
        for (p, q, inward) in [(a, d, across), (b, c, -across)] {
            let w = 0.3;
            let (p2, q2) = (p + inward * w, q + inward * w);
            let (yp, yq) = (lo_y + 0.15, hi_y + 0.15);
            let quad = [at(p, yp), at(p2, yp), at(q2, yq), at(q, yq)];
            let nn = (quad[1] - quad[0]).cross(quad[3] - quad[0]).normalize_or(Vec3::Y);
            let nn = if nn.y < 0.0 { -nn } else { nn };
            buf.quad(quad, [nn; 4], [kerb_c; 4], nn);
            let face_n = Vec3::new(inward.x, 0.0, inward.y);
            buf.quad([at(p2, lo_y), at(q2, hi_y), at(q2, yq), at(p2, yp)], [face_n; 4], [kerb_c; 4], face_n);
        }
    }
}

fn prop(f: &Frame, p: &PropPart, out: &mut Out) {
    let rot = prop_turn(p.yaw);
    for k in 0..p.count.max(1) {
        let at = [p.at[0] + p.step[0] * k as f32, p.at[1] + p.step[1] * k as f32];
        let pos = f.v(at[0], p.y, at[1]);
        let sp = StaticProp::new(&p.asset, pos, rot, p.scale, p.seed, p.solid);
        if let Some(b) = sp.solid_bounds() {
            let (min, max) = (Vec2::new(b.min.x, b.min.z), Vec2::new(b.max.x, b.max.z));
            // A solid prop standing on the ground: people go round it (and none is put inside).
            if p.y < 1.0 {
                mark(out, min, max, Mark::Blocked, f.base);
            }
            if p.cover {
                out.cover.push(Building { min, max, base: b.min.y, height: b.max.y - b.min.y, zone: f.zone, color: [0.0; 3] });
            }
        }
        out.props.push(sp);
    }
}

/// A prop the district file places on its own (`[[prop]]`), at `origin` (its `at`) on ground
/// `base` high.
pub(crate) fn loose_prop(p: &PropPart, origin: Vec2, base: f32, zone: u16, out: &mut Out) {
    let f = Frame { o: origin, base, zone, index: u16::MAX, open: Mark::Open };
    prop(&f, &PropPart { at: [0.0, 0.0], ..p.clone() }, out);
}

fn park(f: &Frame, p: &Park, out: &mut Out) {
    for k in 0..p.count.max(1) {
        let at = [p.at[0] + p.step[0] * k as f32, p.at[1] + p.step[1] * k as f32];
        out.lots.push(LotPlace {
            pos: f.v(at[0], p.y, at[1]),
            dir: heading(p.yaw),
            kind: p.kind.clone(),
            always: p.always,
            landmark: f.index,
        });
    }
}

/// A sign: its words on the wall (or a board), a neon tube round the board, posts under a pole
/// sign.
fn sign(f: &Frame, s: &SignPart, out: &mut Out) {
    let n2 = s.facing.dir();
    let n = Vec3::new(n2.x, 0.0, n2.y);
    let right = Vec3::Y.cross(n).normalize_or(Vec3::X);
    let centre = f.v(s.at[0], s.y, s.at[1]);
    let width = text_width(&s.text, s.size) + s.size * 0.9;
    let height = s.size * 1.6;
    let depth = 0.14;
    let color = Color::try_hex(&s.color).unwrap_or(Color::hex("#f4f1ea"));
    let mut front = 0.03;
    if let Some(board) = &s.board {
        let bc = surface(board, "#1f2933").0;
        let half = right.abs() * (width * 0.5) + Vec3::Y * (height * 0.5) + n.abs() * (depth * 0.5);
        out.decor.push(decor(Shape::Box { half }, centre + n * (depth * 0.5), Quat::IDENTITY, bc, s.pole));
        front += depth;
        if s.pole {
            let foot = f.base;
            for k in [-0.3f32, 0.3] {
                let p = centre + right * (width * k) + n * (depth * 0.5);
                let h = (p.y - height * 0.5 - foot).max(0.1);
                out.decor.push(decor(
                    Shape::Cylinder { half_height: h * 0.5, radius: 0.12 },
                    Vec3::new(p.x, foot + h * 0.5, p.z),
                    Quat::IDENTITY,
                    Color::hex("#5b6166"),
                    true,
                ));
            }
        }
    }
    if let Some(neon) = &s.neon {
        let nc = surface(neon, "#ff3ea5").0;
        let (w, h) = (width + 0.25, height + 0.25);
        let tube = 0.07;
        let c = centre + n * (front + 0.02);
        let r = right.abs();
        let across = r * (w * 0.5) + (Vec3::ONE - r) * (tube * 0.5);
        let upright = Vec3::Y * (h * 0.5) + (Vec3::ONE - Vec3::Y) * (tube * 0.5);
        for (k, (off, half)) in [
            (Vec3::Y * (h * 0.5), across),
            (-Vec3::Y * (h * 0.5), across),
            (right * (w * 0.5), upright),
            (-right * (w * 0.5), upright),
        ]
        .into_iter()
        .enumerate()
        {
            let mut d = decor(Shape::Box { half }, c + off, Quat::IDENTITY, nc, false);
            d.look = Look::Unlit;
            // One tube lights the wall round the sign; the rest only glow.
            d.emissive = if k == 0 { 0.9 } else { 0.5 };
            d.glow = Glow::Night;
            out.decor.push(d);
        }
        front += 0.05;
    }
    out.labels.push(Label {
        text: s.text.clone(),
        pos: centre + n * (front + 0.02),
        size: s.size,
        color,
        mode: LabelMode::Wall,
        facing: s.facing.facing(),
    });
}

fn light(f: &Frame, l: &Lamp, out: &mut Out) {
    let c = Color::try_hex(&l.color).unwrap_or(Color::hex("#fff0d0"));
    let mut d = decor(
        Shape::Box { half: Vec3::new(l.size[0] * 0.5, 0.04, l.size[1] * 0.5) },
        f.v(l.at[0], l.y + 0.04, l.at[1]),
        Quat::IDENTITY,
        c,
        false,
    );
    d.emissive = l.glow;
    d.look = Look::Unlit;
    d.glow = if l.night { Glow::Night } else { Glow::Always };
    out.decor.push(d);
}

/// A Ferris wheel's legs (an A-frame each side of the wheel, braced) and boarding platform; its
/// rim and cars are the game's to draw (`City::wheels`).
fn wheel(f: &Frame, w: &WheelPart, out: &mut Out) {
    let hub = f.v(w.at[0], w.hub, w.at[1]);
    let face = heading(w.yaw);
    let across = Vec3::Y.cross(face).normalize_or(Vec3::X);
    let legs = Color::try_hex(&w.color).unwrap_or(Color::hex("#e8e4dc"));
    let spread = w.radius * 0.5;
    for s in [-1.0f32, 1.0] {
        let top = hub + face * (w.axle * s);
        let feet = [
            top + across * spread - Vec3::Y * w.hub + face * (0.9 * s),
            top - across * spread - Vec3::Y * w.hub + face * (0.9 * s),
        ];
        for foot in feet {
            out.decor.push(bar(foot, top, 0.7, legs, true));
        }
        // A brace across the A at a third of its height.
        let k = 0.33;
        out.decor.push(bar(feet[0].lerp(top, k), feet[1].lerp(top, k), 0.35, legs, true));
    }
    // The boarding platform under the wheel's lowest point, square to the wheel.
    let low = hub - Vec3::Y * w.hub;
    let turn = Quat::from_rotation_y(face.x.atan2(face.z));
    out.decor.push(decor(Shape::Box { half: Vec3::new(3.2, 0.3, 2.4) }, low + Vec3::Y * 0.3, turn, Color::hex("#b7a58a"), true));
    out.wheels.push(WheelGeom {
        hub,
        face,
        radius: w.radius,
        cars: w.cars.max(1),
        period: w.period.max(5.0),
        rim: w.rim.clone(),
        rim_hub: w.rim_hub,
        car: w.car.clone(),
        car_pivot: w.car_pivot,
    });
}

/// A ship's hull: straight sides with a pointed bow, a flat stern, a deck on top; a band at the
/// waterline; the superstructure as a block on the deck at the stern.
fn hull(f: &Frame, h: &Hull, meshes: &mut BTreeMap<(i32, i32), MeshBuf>, out: &mut Out) {
    let (min, max) = f.rect(h.rect);
    // Along the hull (stern to bow) and across it.
    let (len_axis, bow_sign) = match h.bow {
        Side::East => (Vec2::X, 1.0),
        Side::West => (Vec2::X, -1.0),
        Side::South => (Vec2::Y, 1.0),
        Side::North => (Vec2::Y, -1.0),
    };
    let across_axis = Vec2::new(len_axis.y, len_axis.x);
    let centre = (min + max) * 0.5;
    let size = max - min;
    let half_len = size.dot(len_axis) * 0.5;
    let half_beam = size.dot(across_axis) * 0.5;
    let fwd = len_axis * bow_sign;
    // The outline round the deck, stern corners to the bow's point (in plan).
    let bow_len = (half_len * 0.28).min(half_beam * 2.5);
    let p = |u: f32, v: f32| centre + fwd * u + across_axis * v;
    let outline = [
        p(-half_len, -half_beam),
        p(half_len - bow_len, -half_beam),
        p(half_len - bow_len * 0.35, -half_beam * 0.62),
        p(half_len, 0.0),
        p(half_len - bow_len * 0.35, half_beam * 0.62),
        p(half_len - bow_len, half_beam),
        p(-half_len, half_beam),
    ];
    let deck_y = f.base + h.deck;
    let keel = crate::ground::SEABED_Y + 0.2;
    let water = crate::ground::WATER_Y;
    let hull_c = Color::try_hex(&h.color).unwrap_or(Color::hex("#7a2a24")).0;
    let band_c = Color::try_hex(&h.band).unwrap_or(Color::hex("#20242a")).0;
    let deck_c = Color::try_hex(&h.deck_color).unwrap_or(Color::hex("#4f6b52")).0;
    let rail_c = Color::hex("#e9e6df").0;
    let at = |q: Vec2, y: f32| Vec3::new(q.x, y, q.y);
    // Sides in strips: dark below the waterline band, the hull's colour above, a pale sheer line.
    for i in 0..outline.len() {
        let (a, b) = (outline[i], outline[(i + 1) % outline.len()]);
        let mid = (a + b) * 0.5;
        let e = (b - a).normalize_or_zero();
        let mut nrm = Vec3::new(e.y, 0.0, -e.x);
        if Vec2::new(nrm.x, nrm.z).dot(mid - centre) < 0.0 {
            nrm = -nrm;
        }
        let buf = meshes.entry(chunk_key(mid)).or_default();
        for (y0, y1, c) in
            [(keel, water + 1.6, band_c), (water + 1.6, deck_y - 0.5, hull_c), (deck_y - 0.5, deck_y + 0.9, rail_c)]
        {
            buf.quad([at(a, y0), at(b, y0), at(b, y1), at(a, y1)], [nrm; 4], [c; 4], nrm);
        }
    }
    // The deck: triangles fanned from a point aft of the middle, in pieces by chunk (it is long).
    let hub = p(-half_len * 0.2, 0.0);
    for i in 0..outline.len() {
        let (a, b) = (outline[i], outline[(i + 1) % outline.len()]);
        let steps = ((a.distance(b) / 24.0).ceil() as i32).max(1);
        for k in 0..steps {
            let (q0, q1) = (a.lerp(b, k as f32 / steps as f32), a.lerp(b, (k + 1) as f32 / steps as f32));
            let buf = meshes.entry(chunk_key((q0 + q1 + hub) / 3.0)).or_default();
            buf.tri([at(hub, deck_y), at(q0, deck_y), at(q1, deck_y)], Vec3::Y, deck_c);
        }
    }
    if let Some(r) = h.house {
        let (hmin, hmax) = f.rect(r);
        let white = Color::hex("#ece9e2");
        let ht = h.house_height.max(3.0);
        let mut b = Block::new(Vec3::new(hmin.x, deck_y, hmin.y), Vec3::new(hmax.x, deck_y + ht, hmax.y), white);
        b.material = Some("MAT/gtf/office_concrete".into());
        out.blocks.push(b);
        // The bridge's wings across the beam near the top, and a funnel aft of the house.
        let la = Vec3::new(len_axis.x, 0.0, len_axis.y);
        let ca = Vec3::new(across_axis.x, 0.0, across_axis.y);
        let c = Vec3::new((hmin.x + hmax.x) * 0.5, deck_y + ht - 1.0, (hmin.y + hmax.y) * 0.5);
        let wing = ca * (half_beam + 1.2) + la * 1.2 + Vec3::Y * 0.2;
        out.decor.push(boxed(c - wing.abs(), c + wing.abs(), white));
        let house_len = (hmax - hmin).dot(len_axis).abs();
        let funnel = Vec3::new(c.x, deck_y + ht + 3.5, c.z) - Vec3::new(fwd.x, 0.0, fwd.y) * (house_len * 0.3);
        out.decor.push(decor(
            Shape::Cylinder { half_height: 3.5, radius: 2.0 },
            funnel,
            Quat::IDENTITY,
            Color::hex("#b8352c"),
            true,
        ));
        out.buildings.push(Building { min: hmin, max: hmax, base: deck_y, height: ht + 7.0, zone: f.zone, color: white.0 });
    }
    out.buildings.push(Building { min, max, base: water, height: deck_y - water, zone: f.zone, color: hull_c });
}
