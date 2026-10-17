//! Things placed by hand in a district file: stunt ramps (wedges with exact colliders) and signs
//! (words on a post).

use std::collections::BTreeMap;

use glam::{Quat, Vec2, Vec3};
use pav_core::Color;
use pav_core::shape::{Look, Shape};
use pav_core::statics::Decor;
use pav_core::zones::{Label, LabelMode};

use crate::def::DistrictDef;
use crate::grid::Grid;
use crate::ground::{MeshBuf, chunk_key, patches};
use crate::terrain::Terrain;
use crate::{Out, ground_at};

pub(crate) fn build(def: &DistrictDef, grid: &Grid, terrain: &Terrain, out: &mut Out) {
    let world = |c: [f32; 2]| grid.origin + Vec2::new(c[0], c[1]) * grid.cell;
    let mut bufs: BTreeMap<(i32, i32), MeshBuf> = BTreeMap::new();
    let top = Color::hex("#b8b2a4").0;
    let side = Color::hex("#8d877c").0;
    let stripe = Color::hex("#e8b23a").0;
    for j in &def.jump {
        let c = world(j.at);
        let yaw = j.yaw.to_radians();
        let fwd = Vec2::new(yaw.sin(), -yaw.cos());
        let right = Vec2::new(-fwd.y, fwd.x);
        let y0 = ground_at(def, grid, terrain, c) + 0.01;
        let (hl, hw) = (j.length * 0.5, j.width * 0.5);
        let at = |f: f32, r: f32, h: f32| {
            let p = c + fwd * f + right * r;
            Vec3::new(p.x, y0 + h, p.y)
        };
        let buf = bufs.entry(chunk_key(c)).or_default();
        // The slope (a yellow lip at the top), the two sides, the back.
        let lip = hl - 0.6;
        let h_lip = j.height * (lip + hl) / j.length;
        let slope = [at(-hl, -hw, 0.0), at(lip, -hw, h_lip), at(lip, hw, h_lip), at(-hl, hw, 0.0)];
        let n = (slope[1] - slope[0]).cross(slope[3] - slope[0]).normalize();
        let n = if n.y < 0.0 { -n } else { n };
        buf.quad(slope, [n; 4], [top; 4], n);
        let lipq = [at(lip, -hw, h_lip), at(hl, -hw, j.height), at(hl, hw, j.height), at(lip, hw, h_lip)];
        buf.quad(lipq, [n; 4], [stripe; 4], n);
        for s in [-1.0f32, 1.0] {
            let r = hw * s;
            let face = [at(-hl, r, 0.0), at(hl, r, 0.0), at(hl, r, j.height), at(hl - 0.001, r, j.height)];
            let nn = Vec3::new(right.x, 0.0, right.y) * s;
            buf.quad(face, [nn; 4], [side; 4], nn);
        }
        let back = [at(hl, -hw, 0.0), at(hl, hw, 0.0), at(hl, hw, j.height), at(hl, -hw, j.height)];
        let nb = Vec3::new(fwd.x, 0.0, fwd.y);
        buf.quad(back, [nb; 4], [side; 4], nb);
    }
    patches(bufs, out);
    for s in &def.sign {
        let p = world(s.at);
        let y = ground_at(def, grid, terrain, p);
        out.decor.push(Decor {
            shape: Shape::Cylinder { half_height: s.height * 0.5, radius: 0.07 },
            pos: Vec3::new(p.x, y + s.height * 0.5, p.y),
            rot: Quat::IDENTITY,
            color: Color::hex("#4b5157"),
            look: Look::Cel,
            emissive: 0.0,
            solid: false,
            collider: None,
            sway: Default::default(),
            material: None,
            glow: Default::default(),
        });
        out.labels.push(Label {
            text: s.text.clone(),
            pos: Vec3::new(p.x, y + s.height + 0.6, p.y),
            size: s.size,
            color: Color::hex("#f4f1ea"),
            mode: LabelMode::Billboard,
            facing: Default::default(),
        });
    }
}
