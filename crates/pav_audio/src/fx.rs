//! The effects bus: a small Freeverb-style reverb (tunnels, interiors, the city's slap-back)
//! and the master limiter.

struct Comb {
    buf: Vec<f32>,
    i: usize,
    store: f32,
}

impl Comb {
    fn new(len: usize) -> Self {
        Self { buf: vec![0.0; len.max(1)], i: 0, store: 0.0 }
    }

    fn next(&mut self, x: f32, feedback: f32, damp: f32) -> f32 {
        let y = self.buf[self.i];
        self.store = y * (1.0 - damp) + self.store * damp;
        self.buf[self.i] = x + self.store * feedback;
        self.i = (self.i + 1) % self.buf.len();
        y
    }
}

struct Allpass {
    buf: Vec<f32>,
    i: usize,
}

impl Allpass {
    fn new(len: usize) -> Self {
        Self { buf: vec![0.0; len.max(1)], i: 0 }
    }

    fn next(&mut self, x: f32) -> f32 {
        let b = self.buf[self.i];
        self.buf[self.i] = x + b * 0.5;
        self.i = (self.i + 1) % self.buf.len();
        b - x
    }
}

/// Four combs and two all-passes per side (Freeverb's tunings, scaled to the sample rate).
pub struct Reverb {
    combs: [Vec<Comb>; 2],
    passes: [Vec<Allpass>; 2],
}

impl Reverb {
    pub fn new(sample_rate: f32) -> Self {
        let k = sample_rate / 44_100.0;
        let len = |n: f32| (n * k) as usize;
        let side = |spread: f32| {
            (
                [1116.0, 1188.0, 1277.0, 1356.0].iter().map(|n| Comb::new(len(n + spread))).collect(),
                [556.0, 441.0].iter().map(|n| Allpass::new(len(n + spread))).collect(),
            )
        };
        let (cl, pl) = side(0.0);
        let (cr, pr) = side(23.0);
        Self { combs: [cl, cr], passes: [pl, pr] }
    }

    /// The wet signal for one stereo input sample (`room` 0..1: small .. large).
    pub fn next(&mut self, l: f32, r: f32, room: f32) -> (f32, f32) {
        let feedback = 0.7 + room.clamp(0.0, 1.0) * 0.26;
        let damp = 0.3;
        let input = (l + r) * 0.015;
        let mut out = [0.0f32; 2];
        for (side, o) in out.iter_mut().enumerate() {
            let mut s = 0.0;
            for c in &mut self.combs[side] {
                s += c.next(input, feedback, damp);
            }
            for a in &mut self.passes[side] {
                s = a.next(s);
            }
            *o = s;
        }
        (out[0], out[1])
    }
}

/// A gentle master limiter: transparent below about 0.6, then a soft knee into ±1.
pub fn limit(x: f32) -> f32 {
    let a = x.abs();
    if a < 0.6 { x } else { x.signum() * (0.6 + 0.4 * ((a - 0.6) / 0.4).tanh()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reverb_rings_on_and_dies_away() {
        let mut r = Reverb::new(22050.0);
        let (mut early, mut late) = (0.0f32, 0.0f32);
        for i in 0..22050 * 4 {
            let x = if i == 0 { 1.0 } else { 0.0 };
            let (l, rr) = r.next(x, x, 0.6);
            if (2000..6000).contains(&i) {
                early = early.max(l.abs().max(rr.abs()));
            }
            if i > 22050 * 3 {
                late = late.max(l.abs());
            }
        }
        assert!(early > 1e-4, "an echo tail: {early}");
        assert!(late < early * 0.1, "it decays: {late} vs {early}");
        assert!((limit(0.3) - 0.3).abs() < 1e-6 && limit(5.0) <= 1.0 && limit(-5.0) >= -1.0 && limit(0.9) < 0.9);
    }
}
