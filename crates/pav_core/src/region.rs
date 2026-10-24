//! World regions for game cities (PLAN.md §6.5 E14). A game hands the simulation a
//! [`RegionProvider`]: a pure generator (content + seed → chunk) that fills each 32 m chunk the
//! first time something comes near it. Streaming then keeps the chunks around every interest
//! point loaded — farther ahead of fast movers, so a car at 40 m/s never outruns the city — and
//! lets far ones go: untouched chunks are dropped (they regenerate the same), changed ones sleep
//! with their entities. The pavilion's hub, rooms and wilderness stay off for such scenes.
//!
//! The provider lives on the `Sim`, outside `SimState`: a scene's `build` sets it (so restoring a
//! snapshot into a freshly built scene has it again), and it must give the same chunk every time
//! for the same coordinates, because rewind and replays re-run streaming.

use std::collections::BTreeSet;
use std::sync::Arc;

use glam::{IVec2, Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::sim::Sim;
use crate::statics::{Block, CHUNK_SIZE, Decor, Ladder, RegionKey, StaticProp};
use crate::terrain::TerrainPatch;
use crate::zones::{Label, Zone};

/// What a provider puts in one chunk. Positions are world space; anything may cross the chunk's
/// edges (it belongs to the chunk that made it).
#[derive(Clone, Debug, Default)]
pub struct RegionFill {
    pub blocks: Vec<Block>,
    pub decor: Vec<Decor>,
    /// Static triangle meshes with exact colliders: roads on hills, freeway decks and ramps,
    /// terrain, channel walls.
    pub meshes: Vec<Arc<TerrainPatch>>,
    /// Library assets baked in (street dressing).
    pub props: Vec<StaticProp>,
    pub labels: Vec<Label>,
    pub zones: Vec<Zone>,
    /// Climbable zones (a ladder up a wall).
    pub ladders: Vec<Ladder>,
}

/// A game's generator of world regions.
pub trait RegionProvider: Send + Sync {
    /// The chunks that exist, inclusive (chunk coordinates: x = floor(x / 32), z likewise).
    fn bounds(&self) -> (IVec2, IVec2);

    /// Fills chunk (x, z). Must give the same result every time for the same chunk.
    fn fill(&self, x: i32, z: i32, out: &mut RegionFill);
}

/// How streaming keeps regions around the interest points.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct StreamSettings {
    /// Chunks whose centre is within this many metres of an interest point load.
    pub radius: f32,
    /// Chunks farther than this from every point unload.
    pub unload: f32,
    /// Seconds of travel to look ahead of a moving point.
    pub lookahead: f32,
    /// Chunks generated or woken per streaming pass at rest; more when moving fast.
    pub budget: usize,
}

impl Default for StreamSettings {
    fn default() -> Self {
        Self { radius: 112.0, unload: 176.0, lookahead: 1.6, budget: 3 }
    }
}

/// A place streaming keeps loaded, and how fast it moves (m/s).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Interest {
    pub pos: Vec3,
    pub vel: Vec3,
}

fn centre(x: i32, z: i32) -> Vec2 {
    Vec2::new((x as f32 + 0.5) * CHUNK_SIZE, (z as f32 + 0.5) * CHUNK_SIZE)
}

/// The chunks within `radius` of `p`, inside `bounds`.
fn chunks_near(p: Vec2, radius: f32, (lo, hi): (IVec2, IVec2), out: &mut BTreeSet<(i32, i32)>) {
    let r = (radius / CHUNK_SIZE).ceil() as i32 + 1;
    let c = IVec2::new((p.x / CHUNK_SIZE).floor() as i32, (p.y / CHUNK_SIZE).floor() as i32);
    for x in (c.x - r).max(lo.x)..=(c.x + r).min(hi.x) {
        for z in (c.y - r).max(lo.y)..=(c.y + r).min(hi.y) {
            if centre(x, z).distance(p) <= radius + CHUNK_SIZE * 0.71 {
                out.insert((x, z));
            }
        }
    }
}

impl Sim {
    /// Hands the simulation a game's region generator (from a scene's `build`). Streaming starts
    /// with the next pass; `stream_now` loads the start area at once.
    pub fn set_region_provider(&mut self, provider: Arc<dyn RegionProvider>, settings: StreamSettings) {
        self.region_provider = Some(provider);
        self.stream_settings = settings;
    }

    pub fn region_provider(&self) -> Option<&Arc<dyn RegionProvider>> {
        self.region_provider.as_ref()
    }

    /// Loads every chunk the interest points want right now (no budget): a scene's start, a
    /// teleport.
    pub fn stream_now(&mut self) {
        self.stream_regions(usize::MAX);
    }

    /// Where streaming looks: the player (and the vehicle under it), the game's extra points
    /// (`state.world.interest`, e.g. the camera's target), each with its velocity.
    pub fn interest_points(&self) -> Vec<Interest> {
        let mut out = Vec::new();
        if let Some(p) = self.player() {
            let vel = p
                .character
                .as_ref()
                .and_then(|c| c.riding)
                .and_then(|v| self.state.entities.get(v))
                .and_then(|v| v.body)
                .and_then(|b| self.state.physics.bodies.get(b))
                .map(|b| b.linvel())
                .or_else(|| p.character.as_ref().map(|c| c.vel))
                .unwrap_or(Vec3::ZERO);
            out.push(Interest { pos: p.pos, vel });
        }
        out.extend(self.state.world.interest.iter().map(|&pos| Interest { pos, vel: Vec3::ZERO }));
        if out.is_empty() {
            out.push(Interest { pos: self.state.focus, vel: Vec3::ZERO });
        }
        out
    }

    /// One streaming pass for a provided world: load what the points want (nearest first, more
    /// per pass the faster they move), unload what none wants.
    pub(crate) fn stream_regions(&mut self, budget: usize) {
        let Some(provider) = self.region_provider.clone() else { return };
        let s = self.stream_settings;
        let bounds = provider.bounds();
        let points = self.interest_points();
        let mut wanted = BTreeSet::new();
        let mut fastest = 0.0f32;
        for p in &points {
            let at = Vec2::new(p.pos.x, p.pos.z);
            let v = Vec2::new(p.vel.x, p.vel.z);
            fastest = fastest.max(v.length());
            chunks_near(at, s.radius, bounds, &mut wanted);
            if v.length() > 2.0 {
                // Ahead of a moving point, a little narrower.
                chunks_near(at + v * s.lookahead, s.radius * 0.75, bounds, &mut wanted);
            }
        }
        // Nearest first (to the nearest point, its lookahead counting as near).
        let mut order: Vec<(i32, i32)> =
            wanted.iter().copied().filter(|&(x, z)| !self.state.statics.is_active(RegionKey::Chunk(x, z))).collect();
        let dist = |&(x, z): &(i32, i32)| {
            points
                .iter()
                .map(|p| {
                    let ahead = Vec2::new(p.pos.x + p.vel.x * s.lookahead, p.pos.z + p.vel.z * s.lookahead);
                    centre(x, z).distance(Vec2::new(p.pos.x, p.pos.z)).min(centre(x, z).distance(ahead))
                })
                .fold(f32::MAX, f32::min)
        };
        order.sort_by(|a, b| dist(a).total_cmp(&dist(b)).then(a.cmp(b)));
        let budget = if budget == usize::MAX { usize::MAX } else { budget.max(s.budget) + (fastest / 8.0) as usize };
        for (x, z) in order.into_iter().take(budget) {
            self.load_region(&*provider, x, z);
        }
        // Unload: active chunks no point wants.
        let far: Vec<RegionKey> = self
            .state
            .statics
            .chunks
            .keys()
            .copied()
            .filter(|k| match k {
                RegionKey::Chunk(x, z) => points.iter().all(|p| {
                    let c = centre(*x, *z);
                    c.distance(Vec2::new(p.pos.x, p.pos.z)) > s.unload
                        && c.distance(Vec2::new(p.pos.x + p.vel.x * s.lookahead, p.pos.z + p.vel.z * s.lookahead)) > s.unload
                }),
                _ => false,
            })
            .collect();
        for k in far {
            if let RegionKey::Chunk(x, z) = k {
                let lo = Vec3::new(x as f32 * CHUNK_SIZE, -1000.0, z as f32 * CHUNK_SIZE);
                self.sleep_region(k, Some((lo, lo + Vec3::new(CHUNK_SIZE, 3000.0, CHUNK_SIZE))));
            }
        }
    }

    /// Wakes chunk (x, z) or generates it.
    fn load_region(&mut self, provider: &dyn RegionProvider, x: i32, z: i32) {
        let key = RegionKey::Chunk(x, z);
        let st = &mut self.state;
        if st.statics.activate(&mut st.physics, key) {
            self.wake_entities(key);
            return;
        }
        let mut fill = RegionFill::default();
        provider.fill(x, z, &mut fill);
        let st = &mut self.state;
        // An empty chunk still exists, so it is not generated again on every pass.
        st.statics.touch(key);
        for b in fill.blocks {
            st.statics.add_to(&mut st.physics, key, b);
        }
        for d in fill.decor {
            st.statics.add_decor(&mut st.physics, key, d);
        }
        for m in fill.meshes {
            st.statics.add_mesh(&mut st.physics, key, m);
        }
        for p in fill.props {
            if let Err(e) = st.statics.add_prop_to(&mut st.physics, key, p) {
                log::warn!("region ({x}, {z}): {e}");
            }
        }
        for l in fill.labels {
            st.statics.add_label_to(key, l);
        }
        for zn in fill.zones {
            st.statics.add_zone_to(key, zn);
        }
        for l in fill.ladders {
            st.statics.add_ladder_to(key, l);
        }
    }
}
