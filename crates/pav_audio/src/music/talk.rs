//! Between songs and instead of them: each station's ident (a flourish, its own four-note motif
//! on the genre's lead over its chord, then a quiet bed until the next song), a talk station's
//! sounder and news bed, and the jingles under its ads. Everything is in the station's own key
//! (from its seed), the same every break.

use super::Genre;
use super::kit::bell;
use super::kit::{self, Chan, Mem, Note};
use crate::dsp::{self, decay, freq, noise, sine};

const MAJOR: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];
const MINOR: [i32; 7] = [0, 2, 3, 5, 7, 8, 10];

/// A station's key and motif.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Ident {
    pub root: i32,
    pub scale: [i32; 7],
    /// The motif: scale degrees and the beats they start on (the last one rings).
    pub notes: [(i32, f32); 4],
}

/// Beats of the ident's motif, and when it starts after the flourish (s).
const BEAT: f32 = 0.28;
const START: f32 = 0.35;

impl Ident {
    pub fn new(station: u64, genre: Genre) -> Ident {
        let h = dsp::mix(station ^ 0x1de7);
        let minor = match genre {
            Genre::Synthwave | Genre::Surf => true,
            Genre::Cumbia | Genre::Smooth | Genre::Talk => false,
            Genre::Boombap => (h >> 7) & 1 == 1,
        };
        let pool = [0, 2, 4, 7, 9, 4, 2, 7];
        let rhythms = [[0.0, 1.0, 2.0, 3.0], [0.0, 1.0, 1.5, 2.5], [0.0, 0.5, 1.0, 2.0], [0.0, 0.75, 1.5, 2.0]];
        let r = rhythms[((h >> 30) % 4) as usize];
        let mut notes = [(0, 0.0); 4];
        for (k, n) in notes.iter_mut().enumerate() {
            *n = (pool[((h >> (8 + 3 * k)) % 8) as usize], r[k]);
        }
        Ident { root: 48 + (h % 12) as i32, scale: if minor { MINOR } else { MAJOR }, notes }
    }

    pub fn note(&self, deg: i32, oct: i32) -> i32 {
        self.root + self.scale[deg.rem_euclid(7) as usize] + 12 * (deg.div_euclid(7) + oct)
    }

    /// The ident `u` seconds after the song ended: (left, right).
    pub fn play(&self, genre: Genre, u: f32, n: u64, t: f32, dt: f32, mem: &mut [Mem; 4]) -> (f32, f32) {
        if genre == Genre::Talk {
            let v = sounder(self, u);
            return (v, v);
        }
        let (mut l, mut r) = (0.0f32, 0.0f32);
        let mut add = |o: kit::Out| {
            l += o.l + o.send * 0.3;
            r += o.r + o.send * 0.3;
        };
        let s0 = u - START;
        // The flourish into it.
        match genre {
            Genre::Synthwave => {
                let laser =
                    dsp::pulse(dsp::frac(dsp::glide_phase(u, 1800.0, 150.0, 0.1)), 1800.0 * dt, 0.5) * decay(u, 0.3) * 0.08;
                let whoosh = kit::sweep(u, START, 300.0, 6000.0, n, dt, &mut mem[3]) * 0.25;
                add(kit::Out { l: laser + whoosh, r: laser + whoosh, send: 0.0 });
                add(kit::play(genre, Chan::Snare, &Note::hit(1.0), s0, n, t, dt, &mut mem[2]));
            }
            Genre::Boombap => {
                let x = kit::scratch(u, n, dt, &mut mem[3]) * 0.3;
                add(kit::Out { l: x, r: x, send: 0.0 });
                add(kit::play(genre, Chan::Snare, &Note::hit(1.0), s0, n, t, dt, &mut mem[2]));
                let crackle = kit::vinyl(t, n);
                add(kit::Out { l: crackle, r: crackle, send: 0.0 });
            }
            Genre::Surf => {
                // A snare roll into a crash.
                if u < START {
                    add(kit::play(genre, Chan::Snare, &Note::hit(0.3 + u), u % 0.045, n, t, dt, &mut mem[2]));
                } else {
                    add(kit::play(genre, Chan::Perc, &Note::hit(1.0), s0, n, t, dt, &mut mem[2]));
                }
            }
            Genre::Cumbia => {
                add(kit::play(genre, Chan::Perc, &Note { hold: START * 0.9, ..Note::hit(1.0) }, u, n, t, dt, &mut mem[3]));
                add(kit::play(genre, Chan::Snare, &Note { kind: 1, ..Note::hit(0.9) }, s0, n, t, dt, &mut mem[2]));
            }
            Genre::Smooth => {
                let x = kit::sweep(u, START, 2000.0, 7000.0, n, dt, &mut mem[3]) * 0.08;
                add(kit::Out { l: x, r: x, send: 0.0 });
                add(kit::play(genre, Chan::Hat, &Note::hit(0.9), s0, n, t, dt, &mut mem[2]));
            }
            Genre::Talk => {}
        }
        if s0 >= 0.0 {
            add(kit::play(genre, Chan::Kick, &Note::hit(0.9), s0, n, t, dt, &mut mem[2]));
            // The motif on the genre's lead: the latest note started (legato).
            let k = (0..4).rev().find(|&k| s0 >= self.notes[k].1 * BEAT).unwrap_or(0);
            let (deg, on) = self.notes[k];
            let hold = if k == 3 { 1.3 } else { (self.notes[k + 1].1 - on) * BEAT * 0.95 };
            let mut lead = Note::tone(self.note(deg, 1), 0.95, hold);
            lead.keys[1] = self.note(deg - 2, 1);
            lead.n = if genre == Genre::Cumbia { 2 } else { 1 };
            if k > 0 {
                lead.from = self.note(self.notes[k - 1].0, 1);
            }
            if genre == Genre::Surf {
                lead.kind = 1;
                lead.aux = 0.05;
            }
            add(kit::play(genre, Chan::Lead, &lead, s0 - on * BEAT, n, t, dt, &mut mem[0]));
            // Its chord and root under it.
            let mut chord = Note { vel: 0.85, n: 3, hold: 2.2, ..Default::default() };
            for (i, d) in [0, 2, 4].iter().enumerate() {
                chord.keys[i] = self.note(*d, if genre == Genre::Synthwave { 1 } else { 0 });
            }
            add(kit::play(genre, Chan::Keys, &chord, s0, n, t, dt, &mut mem[1]));
            add(kit::play(genre, Chan::Bass, &Note::tone(self.note(0, -1), 0.9, 1.6), s0, n, t, dt, &mut mem[3]));
        }
        // Then a quiet bed in the station's key until the next song.
        let bed = bed(self, u - 2.8, t);
        let crackle = if genre == Genre::Boombap { kit::vinyl(t, n) } else { 0.0 };
        (l + bed + crackle, r + bed + crackle)
    }
}

/// A soft breathing chord, `s` seconds in (silent before 0).
fn bed(id: &Ident, s: f32, t: f32) -> f32 {
    if s < 0.0 {
        return 0.0;
    }
    let breathe = 0.75 + 0.25 * (dsp::TAU * 0.25 * t).sin();
    let v: f32 = [(0, 0.5), (2, 0.35), (4, 0.3)].iter().map(|&(d, g)| sine(freq(id.note(d, 0) as f32) * s) * g).sum();
    v * 0.035 * (s / 1.5).min(1.0) * breathe
}

/// A talk station's sounder `s` seconds in: four bells climbing its motif, a chord of them
/// ringing, three low pulses.
pub(crate) fn sounder(id: &Ident, s: f32) -> f32 {
    if s < 0.0 {
        return 0.0;
    }
    let mut degs: Vec<i32> = id.notes.iter().map(|n| n.0).collect();
    degs.sort();
    let mut v = 0.0;
    for (k, d) in degs.iter().enumerate() {
        v += bell(s - k as f32 * 0.15, freq(id.note(*d, 2) as f32), 0.6) * 0.2;
    }
    for d in [0, 2, 4] {
        v += bell(s - 0.75, freq(id.note(d, 1) as f32), 1.6) * 0.13;
    }
    for k in 0..3 {
        let on = 0.75 + k as f32 * 0.3;
        if s >= on {
            v += sine(dsp::glide_phase(s - on, 90.0, 60.0, 0.05)) * decay(s - on, 0.25) * 0.3;
        }
    }
    v
}

/// Under a talk station's anchor: a low pulse on sixteenths and a held fifth, very quiet.
pub(crate) fn news_bed(id: &Ident, t: f32) -> f32 {
    let six = 60.0 / 90.0 / 4.0;
    let i = (t / six).floor();
    let s = t - i * six;
    let accent = if (i as i64).rem_euclid(4) == 0 { 1.0 } else { 0.55 };
    let pulse = sine(freq(id.note(0, -1) as f32) * s) * decay(s, 0.06) * accent;
    let pad = sine(freq(id.note(0, 0) as f32) * t) * 0.3 + sine(freq(id.note(4, 0) as f32) * t) * 0.2;
    (pulse * 0.5 + pad) * 0.045
}

/// An ad's jingle `s` seconds into its `seconds`: a bright, bouncy loop (kick, claps, a bass
/// on eighths, a marimba hook) that ends on a chord.
pub(crate) fn jingle(s: f32, seconds: f32, seed: u64, n: u64) -> f32 {
    if s < 0.0 || s >= seconds {
        return 0.0;
    }
    let h = dsp::mix(seed ^ 0x1a6);
    let root = 50 + (h % 10) as i32;
    let note = |deg: i32, oct: i32| root + MAJOR[deg.rem_euclid(7) as usize] + 12 * (deg.div_euclid(7) + oct);
    let end = (seconds - 0.9).max(0.0);
    if s >= end {
        // The button: a chord, rung out.
        let b = s - end;
        let v: f32 = [0, 2, 4, 7].iter().map(|d| bell(b, freq(note(*d, 1) as f32), 0.5) * 0.15).sum();
        return v + kit::kick(b, 55.0, 130.0, 0.2, n) * 0.4;
    }
    let spb = 60.0 / 126.0;
    let beat = s / spb;
    let bi = beat.floor();
    let sb = (beat - bi) * spb;
    let bar = (bi / 4.0).floor() as i64;
    let prog = [0, 3, 4, 0][bar.rem_euclid(4) as usize];
    let mut v = kit::kick(sb, 55.0, 130.0, 0.16, n) * 0.45;
    if (bi as i64) % 2 == 1 {
        let clap = decay(sb, 0.004) + decay(sb - 0.011, 0.004) + decay(sb - 0.022, 0.004) + 0.5 * decay(sb - 0.03, 0.08);
        v += noise(n ^ 0xc1a9) * clap * 0.22;
    }
    let e = (beat * 2.0).floor();
    let se = (beat * 2.0 - e) * spb * 0.5;
    let bass = note(prog + if (e as i64) % 2 == 0 { 0 } else { 4 }, -1);
    v += dsp::tri(dsp::frac(freq(bass as f32) * se)) * decay(se, 0.15) * 0.4;
    // The hook: eighths of a bar-long motif (some rests).
    let k = (e as i64).rem_euclid(8) as u64;
    if (h >> (8 + k)) & 1 == 1 || k == 0 {
        let deg = prog + [0, 2, 4, 7, 4, 2, 5, 4][((h >> (16 + 3 * k)) % 8) as usize];
        v += bell(se, freq(note(deg, 1) as f32), 0.25) * 0.2;
    }
    v * (s / 0.05).min(1.0)
}
