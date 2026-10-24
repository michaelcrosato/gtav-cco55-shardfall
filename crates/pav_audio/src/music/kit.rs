//! The instruments: drums and tuned voices, each a function of the seconds since its note began
//! (and the sample, for noise) plus its own filter memory, so a song sounds the same from any
//! offset once the filters settle. Every genre has its own patch for each part (§5.12):
//! synthwave's gated snare and saw lead, boom-bap's dusty kit and electric piano, surf's
//! driven guitar, cumbia's güiro and accordion, smooth jazz's brushes and sax.

use super::Genre;
use crate::dsp::{self, OnePole, PI, Svf, TAU, decay, freq, noise, sine};

/// The parts of a song: each plays one note at a time (the previous one's tail rings on).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Chan {
    Kick,
    Snare,
    Hat,
    /// Crash, open hat, güiro, brush sweep, rim.
    Perc,
    /// Toms, congas.
    Tom,
    Bass,
    /// Chords: pad, electric piano, rhythm guitar, accordion.
    Keys,
    /// The melody.
    Lead,
    Arp,
}

pub(crate) const CH: usize = 9;
pub(crate) const CHANS: [Chan; CH] =
    [Chan::Kick, Chan::Snare, Chan::Hat, Chan::Perc, Chan::Tom, Chan::Bass, Chan::Keys, Chan::Lead, Chan::Arp];

/// A note a part plays.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Note {
    pub vel: f32,
    /// MIDI notes: a chord's `n` of them, a melody's first (cumbia's second: the third below).
    pub keys: [i32; 5],
    pub n: u8,
    /// Seconds it is held before its release.
    pub hold: f32,
    /// The variant: a drum's sound, a guitar's tremolo or slide.
    pub kind: u8,
    /// The MIDI note it slides from (0: none).
    pub from: i32,
    /// The patch's own number: the tremolo's pick period (s).
    pub aux: f32,
}

impl Note {
    pub fn hit(vel: f32) -> Note {
        Note { vel, ..Default::default() }
    }

    pub fn tone(key: i32, vel: f32, hold: f32) -> Note {
        let mut keys = [0; 5];
        keys[0] = key;
        Note { vel, keys, n: 1, hold, ..Default::default() }
    }
}

/// The filter memory of one note.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Mem {
    pub f: Svf,
    pub g: Svf,
    pub a: OnePole,
    pub b: OnePole,
}

/// One note's sound: dry left and right, and its send into the genre's effect.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Out {
    pub l: f32,
    pub r: f32,
    pub send: f32,
}

fn mono(x: f32, pan: f32, send: f32) -> Out {
    let a = (pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
    Out { l: x * a.cos() * 1.414, r: x * a.sin() * 1.414, send: x * send }
}

/// Seconds a note rings after its hold (past it the part is silent).
pub(crate) fn tail(genre: Genre, ch: Chan) -> f32 {
    match (genre, ch) {
        (_, Chan::Kick) => 0.7,
        (Genre::Synthwave, Chan::Snare) => 0.45,
        (_, Chan::Snare) => 0.4,
        (Genre::Smooth, Chan::Hat) => 1.4,
        (_, Chan::Hat) => 0.3,
        (_, Chan::Perc) => 1.6,
        (_, Chan::Tom) => 0.5,
        (_, Chan::Bass) => 0.6,
        (_, Chan::Keys) => 1.2,
        (_, Chan::Lead) => 0.5,
        (_, Chan::Arp) => 0.4,
    }
}

/// An 808-style metal: six detuned squares (hats, rides, crashes once high-passed).
pub(crate) fn metal(s: f32) -> f32 {
    const F: [f32; 6] = [205.3, 304.4, 369.6, 522.7, 540.0, 800.0];
    F.iter().map(|f| if dsp::frac(f * 1.7 * s) < 0.5 { 1.0 } else { -1.0 }).sum::<f32>() / 6.0
}

/// A kick: a sine whose pitch drops from `hi` to `lo` Hz, and a click.
pub(crate) fn kick(s: f32, lo: f32, hi: f32, dec: f32, n: u64) -> f32 {
    if s < 0.0 {
        return 0.0;
    }
    sine(dsp::glide_phase(s, hi, lo, 0.03)) * decay(s, dec) + noise(n) * decay(s, 0.003) * 0.25
}

/// A tom or a conga: a pitched thump that settles from 1.3 times its pitch.
pub(crate) fn tom(s: f32, f: f32, dec: f32) -> f32 {
    if s < 0.0 {
        return 0.0;
    }
    sine(dsp::glide_phase(s, f * 1.3, f, 0.02)) * decay(s, dec)
}

/// An electric piano tine (two-operator FM: the bell fades into a round tone).
pub(crate) fn ep(s: f32, f: f32) -> f32 {
    let index = 1.5 * decay(s, 0.3) + 0.2;
    let car = (TAU * f * s + index * (TAU * f * s).sin()).sin();
    car * decay(s, 1.8) + (TAU * 7.03 * f * s).sin() * decay(s, 0.05) * 0.12
}

/// A bell or a marimba bar: a sine and its inharmonic partials.
pub(crate) fn bell(s: f32, f: f32, dec: f32) -> f32 {
    if s < 0.0 {
        return 0.0;
    }
    (sine(f * s) + sine(2.76 * f * s) * 0.4 * decay(s, dec * 0.3) + sine(5.4 * f * s) * 0.2 * decay(s, dec * 0.1)) * decay(s, dec)
}

/// A reed (the accordion): two pulses a few cents apart (the musette's beating) and the octave
/// below.
fn reed(s: f32, f: f32, dt: f32) -> f32 {
    let p = |k: f32, d: f32| dsp::pulse(dsp::frac(f * k * s), f * k * dt, d);
    p(1.0, 0.32) + p(1.0035, 0.45) * 0.8 + p(0.5, 0.5) * 0.25
}

impl Mem {
    fn reset(&mut self) {
        *self = Mem::default();
    }
}

/// The sound of `note`, `s` seconds after it began, on part `ch` of a `genre` song (`n` the
/// sample, `t` the seconds into the slot for slow wobbles).
pub(crate) fn play(genre: Genre, ch: Chan, note: &Note, s: f32, n: u64, t: f32, dt: f32, m: &mut Mem) -> Out {
    if s < 0.0 {
        return Out::default();
    }
    let v = note.vel;
    let f0 = freq(note.keys[0] as f32);
    let held = |a: f32, r: f32| dsp::ahr(s, a, note.hold.max(a), r);
    match (genre, ch) {
        // Synthwave (and the talk station's beds): the full 80s kit.
        (Genre::Synthwave | Genre::Talk, Chan::Kick) => mono(kick(s, 48.0, 150.0, 0.3, n) * v * 0.25, 0.0, 0.0),
        (Genre::Synthwave | Genre::Talk, Chan::Snare) => {
            // The gated-reverb snare: a body, a burst held up by the "room", then cut.
            m.f.set(1900.0, 0.7, dt);
            let body = sine(190.0 * s) * decay(s, 0.08) + sine(281.0 * s) * decay(s, 0.05) * 0.5;
            let gate = if s < 0.2 { 0.5 + 0.5 * decay(s, 0.035) } else { 0.5 * decay(s - 0.2, 0.012) };
            let x = body * 0.6 + m.f.band(noise(n)) * gate * 1.1;
            mono(x * v * 0.6, 0.0, 0.25)
        }
        (Genre::Synthwave | Genre::Talk, Chan::Hat) => {
            m.f.set(7500.0, 0.7, dt);
            let x = m.f.high(metal(s) * 0.6 + noise(n) * 0.5) * decay(s, 0.035);
            mono(x * v * 1.3, 0.3, 0.1)
        }
        (Genre::Synthwave | Genre::Talk, Chan::Perc) => {
            m.f.set(5000.0, 0.7, dt);
            let x = m.f.high(metal(s) * 0.5 + noise(n)) * decay(s, 0.9);
            mono(x * v * 0.3, -0.35, 0.2)
        }
        (Genre::Synthwave | Genre::Talk, Chan::Tom) => mono(tom(s, f0, 0.22) * v * 0.8, 0.2, 0.3),
        (Genre::Synthwave | Genre::Talk, Chan::Bass) => {
            let dp = f0 * dt;
            m.f.set(160.0 + 1400.0 * decay(s, 0.09) + 400.0 * v, 2.4, dt);
            let x = m.f.low(dsp::saw(dsp::frac(f0 * s), dp)) * held(0.002, 0.04);
            mono(x * v * 0.15, 0.0, 0.0)
        }
        (Genre::Synthwave | Genre::Talk, Chan::Keys) => {
            // A pad: detuned saws per chord note, left and right detuned apart.
            let (mut l, mut r) = (0.0, 0.0);
            for &k in &note.keys[..note.n as usize] {
                let f = freq(k as f32);
                let o = |d: f32| dsp::saw(dsp::frac(f * d * s), f * d * dt);
                l += o(1.0059) + o(0.9981);
                r += o(0.9963) + o(1.0031);
            }
            let a = OnePole::coef(1700.0, dt);
            let env = held(0.25, 0.7) * v * 0.06;
            let (l, r) = (m.a.lp(l, a) * env, m.b.lp(r, a) * env);
            Out { l, r, send: (l + r) * 0.1 }
        }
        (Genre::Synthwave | Genre::Talk, Chan::Lead) => {
            // A saw lead: two saws, a glide in, a vibrato after a moment.
            let from = if note.from > 0 { freq(note.from as f32) } else { f0 };
            let vib = (s - 0.18).clamp(0.0, 0.3) / 0.3 * 0.0035;
            let ph = dsp::glide_phase(s, from, f0, 0.035) - vib * f0 / (TAU * 5.6) * (TAU * 5.6 * s).cos();
            let dp = f0 * dt;
            let x = dsp::saw(dsp::frac(ph), dp) + dsp::saw(dsp::frac(ph * 1.005), dp);
            m.f.set(2600.0 + 1800.0 * decay(s, 0.25), 0.9, dt);
            let y = m.f.low(x) * held(0.008, 0.18) * v * 0.11;
            mono(y, -0.12, 0.35)
        }
        (Genre::Synthwave | Genre::Talk, Chan::Arp) => {
            let x = dsp::pulse(dsp::frac(f0 * s), f0 * dt, 0.25);
            let y = m.a.lp(x, OnePole::coef(3200.0, dt)) * decay(s, 0.12) * held(0.002, 0.02);
            mono(y * v * 0.11, 0.15, 0.5)
        }

        // Boom-bap: a dusty kit (the snare laid back), sub bass, electric piano.
        (Genre::Boombap, Chan::Kick) => mono(dsp::soft(kick(s, 52.0, 120.0, 0.42, n) * 1.6) * v * 0.38, 0.0, 0.0),
        (Genre::Boombap, Chan::Snare) => {
            let s = s - 0.012;
            if s < 0.0 {
                return Out::default();
            }
            m.f.set(3800.0, 0.6, dt);
            let body = sine(175.0 * s) * decay(s, 0.12) + sine(330.0 * s) * decay(s, 0.04) * 0.3;
            let x = body * 0.55 + m.f.low(noise(n)) * decay(s, 0.17) * 1.2;
            mono(dsp::soft(x * 1.3) * v * 0.75, 0.0, 0.0)
        }
        (Genre::Boombap, Chan::Hat) => {
            m.f.set(6000.0, 0.7, dt);
            mono(m.f.high(noise(n)) * decay(s, 0.028) * v * 0.75, 0.25, 0.0)
        }
        (Genre::Boombap, Chan::Perc) => {
            if note.kind == 1 {
                // An open hat.
                m.f.set(6500.0, 0.7, dt);
                mono(m.f.high(noise(n)) * decay(s, 0.28) * v * 0.3, 0.3, 0.0)
            } else {
                // A rim click.
                let x = sine(1650.0 * s) * decay(s, 0.015) + noise(n) * decay(s, 0.004) * 0.5;
                mono(x * v * 0.45, -0.3, 0.0)
            }
        }
        (Genre::Boombap, Chan::Bass) => {
            let from = if note.from > 0 { freq(note.from as f32) } else { f0 };
            let ph = dsp::glide_phase(s, from, f0, 0.05);
            let x = sine(ph) + 0.25 * sine(2.0 * ph);
            let env = held(0.006, 0.12) * (0.6 + 0.4 * decay(s, 0.5));
            mono(dsp::soft(x * 1.3) * env * v * 0.3, 0.0, 0.0)
        }
        (Genre::Boombap, Chan::Keys | Chan::Lead) => {
            // The electric piano, warbling a little (the record's wow), panned by its tremolo.
            let wow = 1.0 + 0.003 * (TAU * 0.55 * t).sin();
            let mut x = 0.0;
            for &k in &note.keys[..note.n as usize] {
                x += ep(s, freq(k as f32) * wow);
            }
            let y = m.a.lp(x, OnePole::coef(2600.0, dt)) * held(0.002, 0.3);
            let trem = 0.3 * (TAU * 4.2 * s).sin();
            let g = if ch == Chan::Lead { 0.2 } else { 0.095 } * v;
            Out { l: y * g * (1.0 + trem), r: y * g * (1.0 - trem), send: y * g * 0.2 }
        }

        // Surf: a driving acoustic kit, a plucked bass, guitars through a spring.
        (Genre::Surf, Chan::Kick) => mono((kick(s, 58.0, 110.0, 0.18, n) + noise(n) * decay(s, 0.01) * 0.2) * v * 0.3, 0.0, 0.0),
        (Genre::Surf, Chan::Snare) => {
            // A tight crack: a band of noise round 2.2 kHz over a firm body.
            m.f.set(2200.0, 0.75, dt);
            let body = sine(205.0 * s) * decay(s, 0.07) + sine(330.0 * s) * decay(s, 0.04) * 0.4;
            let x = body * 0.75 + m.f.band(noise(n)) * decay(s, 0.09) * 1.1;
            mono(x * v * 0.5, 0.0, 0.15)
        }
        (Genre::Surf, Chan::Hat) => {
            m.f.set(6500.0, 0.7, dt);
            mono(m.f.high(metal(s * 1.1) * 0.5 + noise(n) * 0.4) * decay(s, 0.05) * v * 0.8, 0.3, 0.0)
        }
        (Genre::Surf, Chan::Perc) => {
            m.f.set(4000.0, 0.7, dt);
            mono(m.f.high(metal(s) * 0.5 + noise(n)) * decay(s, 1.1) * v * 0.3, -0.35, 0.1)
        }
        (Genre::Surf, Chan::Tom) => mono((tom(s, f0, 0.28) + noise(n) * decay(s, 0.01) * 0.3) * v * 0.22, 0.15, 0.2),
        (Genre::Surf, Chan::Bass) => {
            let p = dsp::frac(f0 * s);
            let x = dsp::tri(p) + 0.3 * dsp::pulse(p, f0 * dt, 0.5);
            let y = m.a.lp(x, OnePole::coef(900.0, dt)) * decay(s, 0.25) * held(0.002, 0.04);
            mono(y * v * 0.22, 0.0, 0.0)
        }
        (Genre::Surf, Chan::Keys | Chan::Lead) => {
            // A driven square guitar. The lead tremolo-picks (`aux`: the pick period) or slides
            // down (kind 2); both feed the spring.
            let lead = ch == Chan::Lead;
            let mut x = 0.0;
            for &k in &note.keys[..note.n.max(1) as usize] {
                let f = freq(k as f32);
                // The slide falls an octave over the hold: the phase of f·2^(-s/hold).
                let (ph, fi) = if note.kind == 2 {
                    let h = note.hold.max(0.1);
                    let ln2 = std::f32::consts::LN_2;
                    let u = s.min(h);
                    let p = f * h / ln2 * (1.0 - 2f32.powf(-u / h)) + (s - u) * f * 0.5;
                    (p, f * 2f32.powf(-u / h))
                } else {
                    (f * s, f)
                };
                let p = dsp::frac(ph);
                x += dsp::pulse(p, fi * dt, 0.5)
                    + if lead { 0.3 * dsp::pulse(dsp::frac(ph * 2.0), 2.0 * fi * dt, 0.5) } else { 0.0 };
            }
            let driven = (x * 3.5).tanh();
            m.f.set(if lead { 3400.0 } else { 3000.0 }, 1.4, dt);
            let y = m.f.low(driven);
            let pick = if note.kind == 1 && note.aux > 0.0 {
                0.45 + 0.55 * decay(s % note.aux, 0.035)
            } else {
                0.35 + 0.65 * decay(s, 0.3)
            };
            let env = held(0.003, 0.06) * pick * v;
            if lead { mono(y * env * 0.09, -0.15, 0.75) } else { mono(y * env * 0.09, 0.2, 0.6) }
        }

        // Cumbia: güiro scrapes, rim and congas, a bouncing bass, accordions.
        (Genre::Cumbia, Chan::Kick) => mono(kick(s, 60.0, 120.0, 0.2, n) * v * 0.47, 0.0, 0.0),
        (Genre::Cumbia, Chan::Snare) => {
            let x = if note.kind == 1 {
                // A timbale.
                sine(dsp::glide_phase(s, 620.0, 450.0, 0.02)) * decay(s, 0.12) + noise(n) * decay(s, 0.03) * 0.4
            } else {
                sine(2100.0 * s) * decay(s, 0.01) + sine(980.0 * s) * decay(s, 0.03) + noise(n) * decay(s, 0.005) * 0.5
            };
            mono(x * v * 0.5, 0.1, 0.05)
        }
        (Genre::Cumbia, Chan::Hat) => {
            m.f.set(7000.0, 1.0, dt);
            mono(m.f.band(noise(n)) * dsp::ahr(s, 0.01, 0.02, 0.04) * v * 0.65, 0.25, 0.0)
        }
        (Genre::Cumbia, Chan::Perc) => {
            // The güiro: noise over ridges, scraped for `hold` seconds.
            m.f.set(3300.0, 2.0, dt);
            let ridges = if dsp::frac(s * 46.0) < 0.35 { 1.0 } else { 0.18 };
            let x = m.f.band(noise(n)) * ridges * dsp::ahr(s, 0.008, note.hold, 0.02);
            mono(x * v * 0.36, -0.35, 0.0)
        }
        (Genre::Cumbia, Chan::Tom) => {
            let f = [210.0, 320.0, 330.0][note.kind.min(2) as usize];
            let slap = if note.kind == 2 { noise(n) * decay(s, 0.008) * 0.8 } else { 0.0 };
            mono((sine(dsp::glide_phase(s, f * 1.25, f, 0.008)) * decay(s, 0.16) + slap) * v * 0.34, 0.35, 0.05)
        }
        (Genre::Cumbia, Chan::Bass) => {
            m.f.set(600.0, 1.4, dt);
            let x = m.f.low(dsp::pulse(dsp::frac(f0 * s), f0 * dt, 0.32));
            mono(x * held(0.003, 0.04) * decay(s, 0.3) * v * 0.24, 0.0, 0.0)
        }
        (Genre::Cumbia, Chan::Keys | Chan::Lead) => {
            // Accordions: chords on the off-beats, the melody in thirds (both keys).
            let mut x = 0.0;
            let count = if ch == Chan::Lead { note.n.clamp(1, 2) } else { note.n.max(1) };
            for &k in &note.keys[..count as usize] {
                x += reed(s, freq(k as f32), dt);
            }
            m.f.set(2600.0, 1.1, dt);
            let bellows = 1.0 + 0.07 * (TAU * 5.2 * s).sin();
            let y = m.f.low(x) * held(0.02, 0.06) * bellows * v;
            if ch == Chan::Lead { mono(y * 0.045, -0.1, 0.25) } else { mono(y * 0.055, 0.2, 0.2) }
        }

        // Smooth jazz: brushes and a ride, a round bass, electric piano, the sax.
        (Genre::Smooth, Chan::Kick) => mono(kick(s, 50.0, 95.0, 0.3, n) * v * 0.45, 0.0, 0.0),
        (Genre::Smooth, Chan::Snare) => {
            let x = if note.kind == 1 {
                m.f.set(2800.0, 0.8, dt);
                m.f.band(noise(n)) * dsp::ahr(s, 0.004, 0.004, 0.11) * 0.9
            } else {
                // A cross-stick.
                sine(1200.0 * s) * decay(s, 0.02) + sine(520.0 * s) * decay(s, 0.04) + noise(n) * decay(s, 0.004) * 0.5
            };
            mono(x * v * 0.65, -0.1, 0.2)
        }
        (Genre::Smooth, Chan::Hat) => {
            // The ride: a ping on a wash.
            m.f.set(4000.0, 0.7, dt);
            let x = m.f.high(metal(s * 2.2) * 0.4 + noise(n) * 0.2) * decay(s, 0.55) + sine(4100.0 * s) * decay(s, 0.3) * 0.15;
            mono(x * v * 0.45, 0.35, 0.1)
        }
        (Genre::Smooth, Chan::Perc) => {
            // A brush swept round the head.
            m.f.set(3600.0, 0.6, dt);
            let sweep = dsp::ahr(s, note.hold * 0.4, note.hold * 0.45, note.hold * 0.55);
            mono(m.f.band(noise(n)) * sweep * v * 0.16, -0.3, 0.0)
        }
        (Genre::Smooth, Chan::Bass) => {
            let ph = dsp::glide_phase(s, f0 * 0.983, f0, 0.045);
            let x = sine(ph) + 0.3 * dsp::tri(dsp::frac(ph));
            let y = m.a.lp(x, OnePole::coef(1100.0, dt)) * held(0.01, 0.08) * (0.7 + 0.3 * decay(s, 0.8));
            mono(y * v * 0.18, 0.0, 0.0)
        }
        (Genre::Smooth, Chan::Keys) => {
            let mut x = 0.0;
            for &k in &note.keys[..note.n as usize] {
                x += ep(s, freq(k as f32));
            }
            let y = m.a.lp(x, OnePole::coef(3200.0, dt)) * held(0.002, 0.35);
            let trem = 0.35 * (TAU * 3.6 * s).sin();
            let g = 0.09 * v;
            Out { l: y * g * (1.0 + trem), r: y * g * (1.0 - trem), send: y * g * 0.25 }
        }
        (Genre::Smooth, Chan::Lead) => {
            // The sax: a saw scooped up into the note, vibrato after a moment, its filter
            // brightest at the attack, and breath.
            let from = if note.from > 0 { freq(note.from as f32) } else { f0 * 0.96 };
            let vib = (s - 0.25).clamp(0.0, 0.35) / 0.35 * 0.006;
            let ph = dsp::glide_phase(s, from, f0, 0.06) - vib * f0 / (TAU * 5.2) * (TAU * 5.2 * s).cos();
            let x = dsp::saw(dsp::frac(ph), f0 * dt);
            m.f.set(650.0 + 2000.0 * decay(s, 0.12) + 500.0 * v, 1.3, dt);
            m.g.set(1600.0, 1.0, dt);
            let breath = m.g.band(noise(n)) * (0.12 * decay(s, 0.05) + 0.03);
            let y = (m.f.low(x) + breath) * held(0.03, 0.09) * v;
            mono(y * 0.15, -0.12, 0.3)
        }
        _ => Out::default(),
    }
}

/// Clears a part's filter memory for a fresh note.
pub(crate) fn fresh(m: &mut Mem) {
    m.reset();
}

/// Boom-bap's record: crackle (sparse clicks, now and then a pop) and hiss, `t` seconds in.
pub(crate) fn vinyl(t: f32, n: u64) -> f32 {
    let slot = (t as f64 * 6000.0) as u64;
    let h = dsp::mix(slot ^ 0x7a11);
    let click = if h.is_multiple_of(380) {
        let a = ((h >> 12) & 0xff) as f32 / 255.0;
        let big = if h.is_multiple_of(380 * 23) { 2.5 } else { 1.0 };
        a * big * if (h >> 20) & 1 == 0 { 1.0 } else { -1.0 }
    } else {
        0.0
    };
    click * 0.22 + noise(n ^ 0x55) * 0.006
}

/// A glissando whoosh: noise swept by a band-pass from `lo` to `hi` Hz over `len` seconds.
pub(crate) fn sweep(s: f32, len: f32, lo: f32, hi: f32, n: u64, dt: f32, m: &mut Mem) -> f32 {
    if s < 0.0 || s > len + 0.05 {
        return 0.0;
    }
    let k = (s / len).clamp(0.0, 1.0);
    m.g.set(lo * (hi / lo).powf(k), 3.0, dt);
    m.g.band(noise(n)) * dsp::ahr(s, len * 0.8, len, 0.05)
}

/// A record scratch: a resonant band whose pitch rocks fast back and forth.
pub(crate) fn scratch(s: f32, n: u64, dt: f32, m: &mut Mem) -> f32 {
    if !(0.0..0.32).contains(&s) {
        return 0.0;
    }
    let rock = (PI * s / 0.08).sin().abs();
    m.g.set(400.0 + 2200.0 * rock, 6.0, dt);
    m.g.band(noise(n)) * dsp::ahr(s, 0.01, 0.28, 0.04) * 1.4
}
