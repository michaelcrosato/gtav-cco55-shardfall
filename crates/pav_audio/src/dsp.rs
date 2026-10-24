//! Small signal-processing parts the music, the voices and the loops share: band-limited
//! oscillators (PolyBLEP), deterministic noise, a one-pole filter, a state-variable filter that
//! stays stable at any cutoff, and delay lines (echoes, a spring-style reverb).

pub const TAU: f32 = std::f32::consts::TAU;
pub const PI: f32 = std::f32::consts::PI;

/// A small deterministic hash (splitmix64).
pub fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

/// 0..1 from a hash and a key (one stream of picks per seed).
pub fn unit(h: u64, k: u64) -> f32 {
    (mix(h ^ k.wrapping_mul(0xd6e8_feb8_6659_fd93)) >> 40) as f32 / (1u64 << 24) as f32
}

/// Deterministic white noise for sample `n` (-1..1): the same sound from any offset.
pub fn noise(n: u64) -> f32 {
    let mut x = n.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    x ^= x >> 29;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 32;
    (x >> 40) as f32 / (1u64 << 23) as f32 - 1.0
}

/// MIDI note to Hz.
pub fn freq(midi: f32) -> f32 {
    440.0 * 2f32.powf((midi - 69.0) / 12.0)
}

/// Decibels to a gain.
pub fn db(x: f32) -> f32 {
    10f32.powf(x / 20.0)
}

pub fn frac(x: f32) -> f32 {
    x - x.floor()
}

/// The PolyBLEP correction of a step at phase 0 (`dp`: the phase step per sample).
fn blep(p: f32, dp: f32) -> f32 {
    if dp <= 0.0 {
        0.0
    } else if p < dp {
        let x = p / dp;
        x + x - x * x - 1.0
    } else if p > 1.0 - dp {
        let x = (p - 1.0) / dp;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

/// A band-limited saw at phase `p` (0..1) that advances `dp` a sample.
pub fn saw(p: f32, dp: f32) -> f32 {
    2.0 * p - 1.0 - blep(p, dp)
}

/// A band-limited pulse of duty `d` (0.5: a square).
pub fn pulse(p: f32, dp: f32, d: f32) -> f32 {
    let v = if p < d { 1.0 } else { -1.0 };
    v + blep(p, dp) - blep(frac(p - d), dp)
}

pub fn tri(p: f32) -> f32 {
    1.0 - 4.0 * (p - 0.5).abs()
}

pub fn sine(p: f32) -> f32 {
    (p * TAU).sin()
}

/// Exponential decay `e^(-s / tau)` (1 at 0, nothing before 0).
pub fn decay(s: f32, tau: f32) -> f32 {
    if s < 0.0 { 0.0 } else { (-s / tau.max(1e-4)).exp() }
}

/// An attack, hold and release envelope: rises over `a`, holds until `hold`, falls over `r`.
pub fn ahr(s: f32, a: f32, hold: f32, r: f32) -> f32 {
    if s < 0.0 {
        0.0
    } else if s < a {
        s / a.max(1e-4)
    } else if s < hold {
        1.0
    } else {
        (1.0 - (s - hold) / r.max(1e-4)).max(0.0)
    }
}

/// The phase (in cycles) `s` seconds into a note whose pitch glides exponentially from `f0` to
/// `f1` (time constant `tau`): a function of time, so a note sounds the same from any offset.
pub fn glide_phase(s: f32, f0: f32, f1: f32, tau: f32) -> f32 {
    let tau = tau.max(1e-4);
    f1 * s + (f0 - f1) * tau * (1.0 - (-s / tau).exp())
}

/// A soft clip into ±1.
pub fn soft(x: f32) -> f32 {
    x / (1.0 + x.abs())
}

/// A one-pole low-pass (`a` from [`OnePole::coef`]).
#[derive(Clone, Copy, Debug, Default)]
pub struct OnePole {
    pub y: f32,
}

impl OnePole {
    pub fn coef(cutoff: f32, dt: f32) -> f32 {
        1.0 - (-TAU * cutoff * dt).exp()
    }

    pub fn lp(&mut self, x: f32, a: f32) -> f32 {
        self.y += (x - self.y) * a;
        self.y
    }

    /// The high-passed rest (`x` minus its low-pass).
    pub fn hp(&mut self, x: f32, a: f32) -> f32 {
        x - self.lp(x, a)
    }
}

/// A state-variable filter with zero-delay feedback (A. Simper's): low, band and high outputs,
/// stable at any cutoff below Nyquist, so cutoffs can sweep freely.
#[derive(Clone, Copy, Debug, Default)]
pub struct Svf {
    ic1: f32,
    ic2: f32,
    a1: f32,
    a2: f32,
    a3: f32,
    k: f32,
    /// The cutoff and resonance the coefficients are for.
    fc: f32,
    q: f32,
}

impl Svf {
    pub fn new(cutoff: f32, q: f32, dt: f32) -> Svf {
        let mut f = Svf::default();
        f.set(cutoff, q, dt);
        f
    }

    /// Sets the cutoff (Hz) and resonance (Q: 0.5 soft .. 10 ringing). Cheap when nothing
    /// moved: a sweep recomputes only every 0.4% of cutoff (a fifteenth of a semitone).
    pub fn set(&mut self, cutoff: f32, q: f32, dt: f32) {
        if q == self.q && (cutoff - self.fc).abs() <= self.fc * 0.004 {
            return;
        }
        self.fc = cutoff;
        self.q = q;
        let g = (PI * (cutoff * dt).clamp(1e-5, 0.49)).tan();
        self.k = 1.0 / q.max(0.3);
        self.a1 = 1.0 / (1.0 + g * (g + self.k));
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
    }

    /// (low, band, high) for one input sample.
    pub fn run(&mut self, x: f32) -> (f32, f32, f32) {
        let v3 = x - self.ic2;
        let v1 = self.a1 * self.ic1 + self.a2 * v3;
        let v2 = self.ic2 + self.a2 * self.ic1 + self.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        (v2, v1, x - self.k * v1 - v2)
    }

    pub fn low(&mut self, x: f32) -> f32 {
        self.run(x).0
    }

    /// The band-pass, unity gain at its centre.
    pub fn band(&mut self, x: f32) -> f32 {
        let k = self.k;
        self.run(x).1 * k
    }

    pub fn high(&mut self, x: f32) -> f32 {
        self.run(x).2
    }
}

/// A delay line of a fixed length.
#[derive(Clone, Debug)]
pub struct Delay {
    buf: Vec<f32>,
    i: usize,
}

impl Delay {
    pub fn new(samples: usize) -> Delay {
        Delay { buf: vec![0.0; samples.max(1)], i: 0 }
    }

    /// The sample written `n` samples ago (1 ..= length).
    pub fn tap(&self, n: usize) -> f32 {
        let len = self.buf.len();
        self.buf[(self.i + len - n.clamp(1, len)) % len]
    }

    pub fn push(&mut self, x: f32) {
        self.buf[self.i] = x;
        self.i = (self.i + 1) % self.buf.len();
    }
}

/// A stereo echo: two delay lines crossing over (ping-pong), the repeats darkening.
#[derive(Clone, Debug)]
pub struct Echo {
    l: Delay,
    r: Delay,
    n: usize,
    damp: [OnePole; 2],
}

impl Echo {
    pub fn new(seconds: f32, dt: f32) -> Echo {
        let n = (seconds / dt).round().max(1.0) as usize;
        Echo { l: Delay::new(n), r: Delay::new(n), n, damp: [OnePole::default(); 2] }
    }

    /// The wet pair for an input pair; `fb` the feedback (0..0.9), `a` the damping low-pass.
    pub fn run(&mut self, l: f32, r: f32, fb: f32, a: f32) -> (f32, f32) {
        let wl = self.l.tap(self.n);
        let wr = self.r.tap(self.n);
        let dl = self.damp[0].lp(wl, a);
        let dr = self.damp[1].lp(wr, a);
        // Ping-pong: the left repeats on the right and back.
        self.l.push(l + dr * fb);
        self.r.push(r + dl * fb);
        (wl, wr)
    }
}

/// A spring-style reverb: a chain of all-passes smears the input into a chirp (the spring's
/// "drip"), then two damped feedback delays ring it out, a little apart for width.
#[derive(Clone, Debug)]
pub struct Spring {
    passes: Vec<(Delay, usize)>,
    combs: [(Delay, usize, OnePole); 2],
}

impl Spring {
    pub fn new(dt: f32) -> Spring {
        let n = |s: f32| (s / dt).round().max(1.0) as usize;
        let passes = [0.0021f32, 0.0033, 0.0047, 0.0061].iter().map(|s| (Delay::new(n(*s)), n(*s))).collect();
        Spring {
            passes,
            combs: [(Delay::new(n(0.037)), n(0.037), OnePole::default()), (Delay::new(n(0.043)), n(0.043), OnePole::default())],
        }
    }

    /// The wet pair for a mono input; `fb` how long it rings (0..0.85), `a` its damping.
    pub fn run(&mut self, x: f32, fb: f32, a: f32) -> (f32, f32) {
        let mut s = x;
        for (d, len) in &mut self.passes {
            // A Schroeder all-pass: g = 0.62.
            let b = d.tap(*len);
            let v = s + b * 0.62;
            d.push(v);
            s = b - v * 0.62;
        }
        let mut out = [0.0f32; 2];
        for (o, (d, len, lp)) in out.iter_mut().zip(self.combs.iter_mut()) {
            let y = d.tap(*len);
            let damped = lp.lp(y, a);
            d.push(s + damped * fb);
            *o = y;
        }
        (out[0], out[1])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_filters_stay_stable_and_pass_what_they_should() {
        let dt = 1.0 / 22_050.0;
        // A low-pass at 500 Hz keeps a 100 Hz sine and drops a 5 kHz one.
        let level = |f: f32, hz: f32| {
            let mut svf = Svf::new(hz, 0.707, dt);
            let mut peak = 0.0f32;
            for i in 0..22_050 {
                let y = svf.low(sine(f * i as f32 * dt));
                if i > 11_025 {
                    peak = peak.max(y.abs());
                }
            }
            peak
        };
        assert!(level(100.0, 500.0) > 0.9 && level(5000.0, 500.0) < 0.05);
        // A cutoff near Nyquist stays finite.
        let mut svf = Svf::new(10_900.0, 8.0, dt);
        assert!((0..10_000).all(|i| svf.low(noise(i)).is_finite()));
        // Band-limited oscillators stay in range.
        for i in 0..1000 {
            let p = frac(i as f32 * 0.0137);
            assert!(saw(p, 0.0137).abs() <= 1.2 && pulse(p, 0.0137, 0.3).abs() <= 2.2);
        }
        // The glide's phase starts at f0's rate and ends at f1's.
        let rate = |s: f32| (glide_phase(s + 1e-3, 200.0, 100.0, 0.05) - glide_phase(s, 200.0, 100.0, 0.05)) / 1e-3;
        assert!((rate(0.0) - 200.0).abs() < 3.0 && (rate(1.0) - 100.0).abs() < 1.0);
    }
}
