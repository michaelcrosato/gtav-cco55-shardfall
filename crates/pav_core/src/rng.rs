//! Small deterministic RNG (PCG32) that lives inside snapshots.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rng {
    state: u64,
    inc: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        let mut r = Rng { state: 0, inc: (seed << 1) | 1 };
        r.next_u32();
        r.state = r.state.wrapping_add(seed ^ 0x853c_49e6_748f_ea9b);
        r.next_u32();
        r
    }
    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(6364136223846793005).wrapping_add(self.inc);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }
    /// Uniform in [0, 1).
    pub fn f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f32()
    }
    pub fn below(&mut self, n: u32) -> u32 {
        if n == 0 { 0 } else { self.next_u32() % n }
    }

    /// A generator of its own, seeded from this one's state and `salt`, leaving this one where
    /// it is: for choices added to old content (a look's hair and accessories) that must not
    /// shift the draws after them. Deterministic like the rest: the same state and salt give
    /// the same stream.
    pub fn fork(&self, salt: u64) -> Rng {
        Rng::new(hash3(self.state ^ salt, (self.inc >> 1) as i32, (self.inc >> 33) as i32, 0x5eed))
    }
}

/// Stateless hash for seed-derived procedural content (terrain, placement).
pub fn hash3(seed: u64, x: i32, y: i32, z: i32) -> u64 {
    let mut h = seed ^ 0x9E37_79B9_7F4A_7C15;
    for v in [x as i64 as u64, y as i64 as u64, z as i64 as u64] {
        h ^= v.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h = (h ^ (h >> 31)).wrapping_mul(0x94D0_49BB_1331_11EB);
        h ^= h >> 29;
    }
    h
}

/// Hash to a float in [0, 1).
pub fn hash_f32(seed: u64, x: i32, y: i32, z: i32) -> f32 {
    (hash3(seed, x, y, z) >> 40) as f32 / (1u64 << 24) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fork_leaves_the_stream_alone_and_differs_by_state_and_salt() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        let mut f = a.fork(1);
        assert_eq!(a.next_u32(), b.next_u32(), "forking draws nothing");
        let mut g = a.fork(1);
        assert_ne!(f.next_u32(), g.next_u32(), "a later state forks another stream");
        assert_eq!(a.fork(2).next_u32(), a.fork(2).next_u32(), "the same state and salt, the same stream");
        assert_ne!(a.fork(2).next_u32(), a.fork(3).next_u32());
    }
}
