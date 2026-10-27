//! Babble (PLAN.md §6.5 E8): speech without words, for talk radio, DJs, a police scanner and
//! dialogue. A glottal buzz runs through three formant filters that glide from vowel to vowel;
//! consonants are noise (a stop's burst, a hiss, a hush, a breath) or a voiced murmur (nasals,
//! liquids). Syllables come in phrases: the pitch drifts down over each (a question's rises at
//! the end), stresses lift it, and each phrase ends in a breath. What a syllable is follows
//! from the line's seed and its index alone, so a radio can join a line midway.

use pav_core::radio::Voice;

use crate::dsp::{self, OnePole, Svf};

/// Vowel formants of a man's voice (Hz): F1, F2, F3.
const VOWELS: [[f32; 3]; 8] = [
    [730.0, 1090.0, 2440.0], // "father"
    [530.0, 1840.0, 2480.0], // "bed"
    [270.0, 2290.0, 3010.0], // "beet"
    [570.0, 840.0, 2410.0],  // "bought"
    [300.0, 870.0, 2240.0],  // "boot"
    [660.0, 1720.0, 2410.0], // "bat"
    [640.0, 1190.0, 2390.0], // "but"
    [490.0, 1350.0, 1690.0], // "bird"
];
/// Formant bandwidths (Hz) and levels.
const WIDTH: [f32; 3] = [90.0, 110.0, 160.0];
const LEVEL: [f32; 3] = [1.0, 0.6, 0.32];
/// A nasal murmur's and a liquid's formants.
const NASAL: [f32; 3] = [260.0, 1050.0, 2300.0];
const LIQUID: [f32; 3] = [360.0, 1150.0, 2500.0];

/// How a syllable begins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Onset {
    Open,
    /// A stop: a closure, a burst, then breath ("t", "k", "p").
    Stop,
    /// A hiss ("s"), a hush ("sh"), a breath ("h").
    Hiss,
    Hush,
    Breath,
    /// Voiced and closed ("m", "n"), voiced and open ("l", "r").
    Nasal,
    Liquid,
}

impl Onset {
    /// Seconds before the vowel.
    fn length(self) -> f32 {
        match self {
            Onset::Open => 0.0,
            Onset::Stop => 0.032,
            Onset::Hiss => 0.075,
            Onset::Hush => 0.08,
            Onset::Breath => 0.05,
            Onset::Nasal => 0.055,
            Onset::Liquid => 0.045,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Syllable {
    /// A breath between phrases.
    rest: bool,
    onset: Onset,
    vowel: usize,
    /// Seconds into its slot it starts, and how long it sounds.
    at: f32,
    len: f32,
    /// 0..1: how hard it lands.
    stress: f32,
    /// Its pitch at the start and the end (multiples of the voice's).
    p0: f32,
    p1: f32,
}

/// Syllable slots per phrase (the last one or two are the breath).
const PHRASE: i64 = 8;

fn syllable(seed: u64, i: i64, v: &Voice) -> Syllable {
    let slot = slot(v);
    let h = dsp::mix(seed ^ (i as u64).wrapping_mul(0xa076_1d64_78bd_642f));
    let (phrase, pos) = (i.div_euclid(PHRASE), i.rem_euclid(PHRASE));
    let ph = dsp::mix(seed ^ 0x5eed ^ (phrase as u64).wrapping_mul(0xe703_7ed1_a0b4_28db));
    // Most phrases are seven syllables and a breath, some six and a longer one.
    let words = PHRASE - 1 - ph.is_multiple_of(3) as i64;
    let e = v.energy.clamp(0.0, 1.0);
    let x = pos as f32 / (words - 1).max(1) as f32;
    let question = ph.is_multiple_of(5);
    let stress = if dsp::unit(h, 1) < 0.3 { 1.0 } else { 0.3 + 0.35 * dsp::unit(h, 2) };
    // Declination, then the stress's lift and a little scatter.
    let p0 = (1.08 - 0.18 * x) * (1.0 + e * 0.25 * stress + (dsp::unit(h, 3) - 0.5) * 0.06 * (0.5 + e));
    let glide = if question && pos >= words - 2 { 0.14 } else { (dsp::unit(h, 4) - 0.6) * 0.1 * (0.4 + e) };
    let onset = match h % 20 {
        0..=3 => Onset::Open,
        4..=8 => Onset::Stop,
        9..=11 => Onset::Hiss,
        12 => Onset::Hush,
        13 => Onset::Breath,
        14..=16 => Onset::Nasal,
        _ => Onset::Liquid,
    };
    let at = dsp::unit(h, 5) * 0.08 * slot;
    Syllable {
        rest: pos >= words,
        onset,
        vowel: ((h >> 24) % VOWELS.len() as u64) as usize,
        at,
        len: ((0.7 + 0.18 * dsp::unit(h, 6) + 0.1 * stress) * slot).min(slot - at),
        stress,
        p0,
        p1: p0 * (1.0 + glide),
    }
}

fn slot(v: &Voice) -> f32 {
    1.0 / v.rate.clamp(1.5, 12.0)
}

/// One speaker saying one line: call [`Babble::next`] with the seconds into the line.
pub struct Babble {
    /// Who speaks (a loop may glide its pitch).
    pub voice: Voice,
    seed: u64,
    phase: f32,
    tilt: OnePole,
    formants: [Svf; 3],
    /// The formants' frequencies and levels now (gliding toward each sound's).
    f: [f32; 3],
    g: [f32; 3],
    fric: Svf,
    fric_mode: u8,
    radio_hp: Svf,
    radio_lp: Svf,
    noise: u32,
    ctl: u32,
    cur: Option<(i64, Syllable)>,
}

/// Samples between filter updates.
const CONTROL: u32 = 16;

impl Babble {
    pub fn new(voice: Voice, seed: u64) -> Babble {
        Babble {
            voice,
            seed,
            phase: 0.0,
            tilt: OnePole::default(),
            formants: [Svf::default(); 3],
            f: VOWELS[6],
            g: LEVEL,
            fric: Svf::default(),
            fric_mode: 0,
            radio_hp: Svf::default(),
            radio_lp: Svf::default(),
            noise: (dsp::mix(seed) as u32) | 1,
            ctl: 0,
            cur: None,
        }
    }

    fn white(&mut self) -> f32 {
        self.noise ^= self.noise << 13;
        self.noise ^= self.noise >> 17;
        self.noise ^= self.noise << 5;
        (self.noise as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    /// The sample `t` seconds into a line `seconds` long (silent outside it); `dt` the sample
    /// period.
    pub fn next(&mut self, t: f32, seconds: f32, dt: f32) -> f32 {
        if t < 0.0 || t >= seconds {
            return 0.0;
        }
        let v = self.voice;
        let slot = slot(&v);
        let i = (t / slot).floor() as i64;
        let syl = match self.cur {
            Some((j, s)) if j == i => s,
            _ => {
                let s = syllable(self.seed, i, &v);
                self.cur = Some((i, s));
                s
            }
        };
        let tau = t - i as f32 * slot - syl.at;
        let scale = v.formant.clamp(0.7, 1.5);
        // What sounds now: the voicing, aspiration (breath through the formants), frication
        // (noise on its own) and which formants.
        let (mut voiced, mut breath, mut hiss) = (0.0f32, 0.0f32, 0.0f32);
        let mut target = VOWELS[syl.vowel];
        let mut levels = LEVEL;
        let mut mode = self.fric_mode;
        if !syl.rest && (0.0..syl.len).contains(&tau) {
            let od = syl.onset.length();
            let tail = dsp::ahr(syl.len - tau, 0.04, 1.0, 1.0).min(1.0);
            if tau < od {
                match syl.onset {
                    Onset::Stop => {
                        // Closed, a burst, then breath.
                        if tau > 0.012 {
                            hiss = if tau < 0.02 { 0.9 } else { 0.0 };
                            breath = if tau >= 0.02 { 0.5 } else { 0.0 };
                        }
                        mode = 0;
                    }
                    Onset::Hiss => {
                        hiss = 0.45 * dsp::ahr(tau, 0.02, od - 0.02, 0.02);
                        mode = 1;
                    }
                    Onset::Hush => {
                        hiss = 0.5 * dsp::ahr(tau, 0.02, od - 0.02, 0.02);
                        mode = 2;
                    }
                    Onset::Breath => breath = 0.45,
                    Onset::Nasal => {
                        voiced = 0.45;
                        target = NASAL;
                        levels = [1.0, 0.12, 0.08];
                    }
                    Onset::Liquid => {
                        voiced = 0.75;
                        target = LIQUID;
                    }
                    Onset::Open => {}
                }
            } else {
                let attack = ((tau - od) / 0.015).clamp(0.0, 1.0);
                voiced = attack * tail * (0.72 + 0.28 * syl.stress);
                breath = 0.04 * voiced;
            }
        }
        // The line's end cuts the last syllable short, gently.
        let end = ((seconds - t) / 0.03).clamp(0.0, 1.0);
        // Filters follow at control rate.
        if self.ctl == 0 {
            let k = 1.0 - (-(CONTROL as f32) * dt / 0.022).exp();
            for j in 0..3 {
                self.f[j] += (target[j] * scale - self.f[j]) * k;
                self.g[j] += (levels[j] - self.g[j]) * k;
                self.formants[j].set(self.f[j], self.f[j] / (WIDTH[j] * scale), dt);
            }
            self.fric_mode = mode;
            match mode {
                1 => self.fric.set(5200.0 * scale, 0.8, dt),
                2 => self.fric.set(2600.0 * scale, 1.6, dt),
                _ => self.fric.set(1800.0 * scale, 0.6, dt),
            }
            let r = v.radio.clamp(0.0, 1.0);
            self.radio_hp.set(200.0 + 300.0 * r, 0.8, dt);
            self.radio_lp.set(5200.0 - 2400.0 * r, 0.9, dt);
        }
        self.ctl = (self.ctl + 1) % CONTROL;
        // The glottal buzz: a band-limited saw, tilted down, at the syllable's pitch.
        let k = if syl.len > 0.0 { (tau / syl.len).clamp(0.0, 1.0) } else { 0.0 };
        let f0 = v.pitch.clamp(50.0, 500.0) * (syl.p0 + (syl.p1 - syl.p0) * k);
        let dp = f0 * dt;
        self.phase = dsp::frac(self.phase + dp);
        let buzz = dsp::saw(self.phase, dp);
        let tilt_a = OnePole::coef(1200.0, dt);
        let glottal = self.tilt.lp(buzz, tilt_a);
        let n = self.white();
        let src = glottal * voiced * 2.2 + n * breath * 0.5;
        let mut y = 0.0;
        for j in 0..3 {
            y += self.formants[j].band(src) * self.g[j];
        }
        let fr = match self.fric_mode {
            1 => self.fric.high(n),
            2 => self.fric.band(n),
            _ => self.fric.high(n) * 0.8,
        };
        y += fr * hiss;
        y *= end * 0.9;
        let r = v.radio.clamp(0.0, 1.0);
        if r > 0.0 {
            // A small speaker or a two-way radio: no lows, no highs, driven, a little hiss.
            let hp = self.radio_hp.high(y);
            let band = self.radio_lp.low(hp);
            let drive = 1.0 + 5.0 * r;
            let driven = (band * drive).tanh() / drive.sqrt();
            y = y * (1.0 - r) + (driven * 1.6 + n * 0.012) * r;
        }
        y
    }
}

/// Renders a whole line (checks and captures): `seconds` of `voice` from `seed`.
pub fn render(voice: Voice, seed: u64, seconds: f32, sample_rate: f32) -> Vec<f32> {
    let mut b = Babble::new(voice, seed);
    let dt = 1.0 / sample_rate;
    (0..(seconds * sample_rate) as usize).map(|i| b.next(i as f32 * dt, seconds, dt)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
    }

    #[test]
    fn babble_talks_in_syllables_and_phrases() {
        let sr = 22_050.0;
        let v = Voice::default();
        let out = render(v, 7, 6.0, sr);
        assert!(out.iter().all(|x| x.is_finite() && x.abs() < 1.0));
        let level = rms(&out);
        assert!(level > 0.04 && level < 0.3, "rms {level}");
        // Syllables: the loudness rises and falls several times a second.
        let frame = (sr * 0.02) as usize;
        let env: Vec<f32> = out.chunks(frame).map(rms).collect();
        let mean = env.iter().sum::<f32>() / env.len() as f32;
        let crossings = env.windows(2).filter(|w| (w[0] < mean) != (w[1] < mean)).count();
        let per_second = crossings as f32 / 2.0 / 6.0;
        assert!((2.0..9.0).contains(&per_second), "{per_second} syllables a second");
        // Phrases end in breaths: some frames are near silence.
        assert!(env.iter().filter(|e| **e < mean * 0.1).count() > 10, "pauses between phrases");
        // Silent outside the line, and the same seed says the same thing.
        let mut b = Babble::new(v, 7);
        assert_eq!(b.next(-0.1, 6.0, 1.0 / sr), 0.0);
        assert_eq!(b.next(6.5, 6.0, 1.0 / sr), 0.0);
        assert_eq!(render(v, 7, 1.0, sr), render(v, 7, 1.0, sr));
        assert_ne!(render(v, 8, 1.0, sr), render(v, 7, 1.0, sr));
    }

    #[test]
    fn a_radio_voice_loses_its_lows_and_highs() {
        let sr = 22_050.0;
        let clean = render(Voice::default(), 3, 4.0, sr);
        let radio = render(Voice { radio: 1.0, ..Voice::default() }, 3, 4.0, sr);
        // The share below 250 Hz drops through the radio, and so does the share above 4 kHz.
        let share = |x: &[f32], hz: f32, low: bool| {
            let mut f = Svf::new(hz, 0.7, 1.0 / sr);
            rms(&x.iter().map(|v| if low { f.low(*v) } else { f.high(*v) }).collect::<Vec<_>>()) / rms(x)
        };
        let (lc, lr) = (share(&clean, 250.0, true), share(&radio, 250.0, true));
        let (hc, hr) = (share(&clean, 4000.0, false), share(&radio, 4000.0, false));
        assert!(lr < lc * 0.6, "lows {lr} vs {lc}");
        assert!(hr < hc * 0.8, "highs {hr} vs {hc}");
        assert!(radio.iter().all(|x| x.is_finite() && x.abs() < 1.0));
    }
}
