//! World regions for game cities (`pav_core::region`, PLAN.md E14): a provider fills chunks on
//! demand; streaming keeps the area around the player loaded even at freeway speed, lets far
//! chunks go, bounds what it keeps, and stays exact under rewind; static meshes collide; entities
//! sleep, despawn or stay with their region as asked.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use glam::{IVec2, Vec3};
use pav_core::entity::Unload;
use pav_core::region::{RegionFill, RegionProvider, StreamSettings};
use pav_core::statics::{Block, CHUNK_SIZE, RegionKey};
use pav_core::terrain::TerrainPatch;
use pav_core::{BodyKind, Color, InputFrame, Shape, Sim, Spawn, Visual};

/// A test town: every chunk has a ground mesh and a building whose height comes from its
/// coordinates; it counts how often it is asked.
struct Town {
    fills: AtomicUsize,
}

impl RegionProvider for Town {
    fn bounds(&self) -> (IVec2, IVec2) {
        (IVec2::new(-16, -16), IVec2::new(15, 15))
    }

    fn fill(&self, x: i32, z: i32, out: &mut RegionFill) {
        self.fills.fetch_add(1, Ordering::Relaxed);
        let (x0, z0) = (x as f32 * CHUNK_SIZE, z as f32 * CHUNK_SIZE);
        let s = CHUNK_SIZE;
        // The ground: one quad as a mesh (a city's road deck), its top at y = 0.
        let p = vec![[x0, 0.0, z0], [x0 + s, 0.0, z0], [x0 + s, 0.0, z0 + s], [x0, 0.0, z0 + s]];
        let n = vec![[0.0, 1.0, 0.0]; 4];
        let c = vec![[0.2, 0.2, 0.22]; 4];
        out.meshes.push(Arc::new(TerrainPatch::from_mesh((x, z), p, n, c, vec![0, 2, 1, 0, 3, 2])));
        let h = 6.0 + ((x * 7 + z * 13).rem_euclid(5)) as f32 * 4.0;
        out.blocks.push(Block::new(
            Vec3::new(x0 + 20.0, 0.0, z0 + 20.0),
            Vec3::new(x0 + 28.0, h, z0 + 28.0),
            Color::hex("#c8b8a0"),
        ));
    }
}

fn town() -> (Sim, Arc<Town>) {
    let provider = Arc::new(Town { fills: AtomicUsize::new(0) });
    let mut sim = Sim::empty(1);
    sim.state.spawn = Vec3::new(4.0, 0.0, 4.0);
    sim.set_region_provider(provider.clone(), StreamSettings::default());
    sim.stream_now();
    sim.spawn_player();
    sim.run(10, &InputFrame::default());
    (sim, provider)
}

fn chunk(p: Vec3) -> RegionKey {
    RegionKey::chunk_of(p)
}

#[test]
fn the_player_stands_on_a_generated_mesh() {
    let (mut sim, _) = town();
    sim.run(60, &InputFrame::default());
    let feet = sim.feet(sim.state.player.unwrap()).unwrap();
    assert!(feet.y.abs() < 0.2, "standing on the ground mesh, not falling: {feet}");
    assert!(sim.state.statics.is_active(chunk(feet)));
    let c = &sim.state.statics.chunks[&chunk(feet)];
    assert_eq!(c.meshes.len(), 1);
    assert_eq!(c.mesh_colliders.len(), 1);
}

#[test]
fn a_fast_drive_never_outruns_the_city() {
    let (mut sim, provider) = town();
    let pid = sim.state.player.unwrap();
    // A "car" at 40 m/s across town: move the player along a line and stream like the sim does
    // (one pass every 15 ticks).
    let (mut active_max, mut dormant_max) = (0, 0);
    // The start of the drive is a teleport: load it at once, as a game does after one.
    sim.set_position(pid, Vec3::new(-400.0, 2.0, 10.0));
    sim.stream_now();
    for t in 0..(60 * 18) {
        let p = Vec3::new(-400.0 + t as f32 * (40.0 / 60.0), 2.0, 10.0);
        sim.set_position(pid, p);
        sim.state.entities.get_mut(pid).unwrap().character.as_mut().unwrap().vel = Vec3::new(40.0, 0.0, 0.0);
        sim.step(&InputFrame::default());
        // Every chunk within 64 m ahead and around is there.
        for dx in [-64.0f32, -32.0, 0.0, 32.0, 64.0] {
            for dz in [-32.0f32, 0.0, 32.0] {
                let q = p + Vec3::new(dx, 0.0, dz);
                let (lo, hi) = provider.bounds();
                let k = chunk(q);
                let RegionKey::Chunk(x, z) = k else { unreachable!() };
                if x < lo.x || x > hi.x || z < lo.y || z > hi.y {
                    continue;
                }
                assert!(sim.state.statics.is_active(k), "tick {t}: hole at {q} ({k:?})");
            }
        }
        active_max = active_max.max(sim.state.statics.chunks.len());
        dormant_max = dormant_max.max(sim.state.statics.dormant.len() + sim.state.world.dormant_entities.len());
    }
    assert!(active_max <= 80, "active chunks stay bounded: {active_max}");
    assert_eq!(dormant_max, 0, "untouched chunks are dropped, not kept: {dormant_max}");
    assert!(provider.fills.load(Ordering::Relaxed) > 60, "the drive generated the town as it went");
}

#[test]
fn entities_sleep_despawn_or_stay_with_their_region() {
    let (mut sim, _) = town();
    let look = |c: &str| Visual::new(Shape::Box { half: Vec3::splat(0.4) }, Color::hex(c));
    let at = Vec3::new(40.0, 0.5, 8.0);
    let parked = sim.spawn(Spawn::new("parked", at).visual(look("#e8443a")).body(BodyKind::Dynamic));
    let walker = sim.spawn(Spawn::new("walker", at + Vec3::X).visual(look("#3d8bff")).body(BodyKind::Dynamic));
    let actor = sim.spawn(Spawn::new("actor", at + Vec3::X * 2.0).visual(look("#ffc145")).body(BodyKind::Dynamic));
    sim.state.entities.get_mut(walker).unwrap().unload = Unload::Despawn;
    sim.state.entities.get_mut(actor).unwrap().unload = Unload::Keep;
    sim.run(30, &InputFrame::default());
    // Walk away (teleport) far enough for their chunk to unload.
    let pid = sim.state.player.unwrap();
    sim.set_position(pid, Vec3::new(-450.0, 1.0, -450.0));
    for _ in 0..8 {
        sim.run(15, &InputFrame::default());
    }
    assert!(!sim.state.statics.is_active(chunk(at)), "their chunk unloaded");
    assert!(sim.state.entities.get(parked).is_none(), "the parked car sleeps (out of the world)");
    assert!(sim.state.world.dormant_entities.values().flatten().any(|d| d.entity.id == parked), "kept with its chunk");
    assert!(sim.state.entities.get(walker).is_none());
    assert!(!sim.state.world.dormant_entities.values().flatten().any(|d| d.entity.id == walker), "the walker is gone for good");
    assert!(sim.state.entities.get(actor).is_some(), "the mission actor stays");
    // Come back: the parked car wakes where it was.
    sim.set_position(pid, at + Vec3::new(0.0, 1.0, 6.0));
    for _ in 0..8 {
        sim.run(15, &InputFrame::default());
    }
    let p = sim.state.entities.get(parked).expect("woke with its chunk").pos;
    assert!(p.distance(at) < 1.0, "where it was parked: {p}");
}

#[test]
fn streaming_replays_exactly() {
    let (mut sim, _) = town();
    let pid = sim.state.player.unwrap();
    let start = sim.snapshot();
    let base = sim.recording.ticks() as usize;
    for t in 0..400 {
        let f = InputFrame { move_dir: glam::Vec2::new(1.0, 0.3), ..Default::default() };
        sim.step(&f);
        if t == 200 {
            // Rewind across chunk loads and back.
            let tick = sim.state.tick;
            let hash = sim.state_hash();
            assert!(sim.rewind_to(tick - 150));
            assert!(sim.rewind_to(tick));
            assert_eq!(sim.state_hash(), hash);
        }
    }
    let hash = sim.state_hash();
    let provider = sim.region_provider().unwrap().clone();
    let mut again = Sim::empty(1);
    again.set_region_provider(provider, StreamSettings::default());
    again.restore(start);
    for f in sim.recording.iter().skip(base) {
        again.step(f);
    }
    assert_eq!(again.state_hash(), hash);
    assert!(sim.feet(pid).unwrap().x > 30.0, "walked across chunk borders");
}
