//! M2 acceptance: the city is a pure function of its district file — two builds give the same
//! buildings, lanes, walk tiles, cover and chunk contents, bit for bit.

use pav_core::region::{RegionFill, RegionProvider};
use pav_world::City;

/// FNV-1a over what a build produced.
struct Digest(u64);

impl Digest {
    fn bytes(&mut self, b: &[u8]) {
        for &x in b {
            self.0 = (self.0 ^ x as u64).wrapping_mul(0x100_0000_01b3);
        }
    }
    fn f(&mut self, x: f32) {
        self.bytes(&x.to_bits().to_le_bytes());
    }
    fn v(&mut self, v: &[f32]) {
        v.iter().for_each(|x| self.f(*x));
    }
    fn s(&mut self, s: &str) {
        self.bytes(s.as_bytes());
    }
}

fn digest(city: &City) -> (u64, usize) {
    let mut d = Digest(0xcbf2_9ce4_8422_2325);
    for b in &city.buildings {
        d.v(&[b.min.x, b.min.y, b.max.x, b.max.y, b.base, b.height]);
        d.v(&b.color);
        d.bytes(&b.zone.to_le_bytes());
    }
    for l in &city.lanes.lanes {
        l.points.iter().for_each(|p| d.v(&p.to_array()));
        d.s(&format!("{:?}{:?}{:?}{}", l.kind, l.next, l.turn, l.closed));
    }
    for p in &city.lanes.parking {
        d.v(&p.pos.to_array());
    }
    for c in &city.cover.points {
        d.v(&c.pos.to_array());
    }
    d.s(&format!("{:?}", city.walk.counts()));
    let (lo, hi) = city.bounds();
    let mut chunks = 0;
    for z in lo.y..=hi.y {
        for x in lo.x..=hi.x {
            let mut out = RegionFill::default();
            city.fill(x, z, &mut out);
            if out.blocks.is_empty() && out.meshes.is_empty() && out.props.is_empty() && out.decor.is_empty() {
                continue;
            }
            chunks += 1;
            for b in &out.blocks {
                d.v(&b.min.to_array());
                d.v(&b.max.to_array());
                d.s(b.material.as_deref().unwrap_or("-"));
            }
            for k in &out.decor {
                d.v(&k.pos.to_array());
            }
            for p in &out.props {
                d.s(&p.asset);
                d.v(&p.pos.to_array());
                d.f(p.scale);
            }
            for m in &out.meshes {
                m.positions.iter().for_each(|p| d.v(p));
                d.bytes(&(m.indices.len() as u64).to_le_bytes());
            }
            for l in &out.labels {
                d.s(&l.text);
            }
        }
    }
    (d.0, chunks)
}

#[test]
fn the_city_is_the_same_every_build() {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../content/freeway/district.toml")).unwrap();
    let (a, chunks) = digest(&City::from_toml(&text).unwrap());
    let (b, again) = digest(&City::from_toml(&text).unwrap());
    eprintln!("city digest {a:016x} over {chunks} chunks");
    assert!(chunks > 500, "{chunks} chunks with content");
    assert_eq!((a, chunks), (b, again), "two builds of Paloma Bay differ");
}
