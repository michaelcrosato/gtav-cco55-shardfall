//! What each genre plays when (PLAN.md §5.12): its feel (swing), scale and progressions, a
//! song's sections (intro, verses, choruses, a bridge in longer songs, outro), each part's
//! pattern, and the melodies made from the song's seed.

use pav_core::radio::SongMeta;

use super::Genre;
use super::kit::{CH, Chan, Note};
use crate::dsp;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Intro,
    Verse,
    Chorus,
    Bridge,
    Outro,
}

/// A stretch of a song: `bars` bars from bar `start`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Section {
    pub part: Part,
    pub start: u32,
    pub bars: u32,
}

/// A song's sections over `bars` bars: a four-bar intro and outro, and between them verses and
/// choruses of eight bars (a bridge before the last chorus when there is room), ending on a
/// chorus. Short songs are all chorus.
pub fn sections(bars: u32) -> Vec<Section> {
    let bars = bars.max(8) / 4 * 4;
    let units = ((bars - 8) / 4) as usize;
    let mut parts: Vec<Part> = match units {
        0 => Vec::new(),
        1 | 2 => vec![Part::Chorus; units],
        _ => (0..units).map(|i| [Part::Verse, Part::Verse, Part::Chorus, Part::Chorus][i % 4]).collect(),
    };
    if units >= 10 {
        // The bridge, then a last chorus.
        parts[units - 4] = Part::Bridge;
        parts[units - 3] = Part::Bridge;
        parts[units - 2] = Part::Chorus;
        parts[units - 1] = Part::Chorus;
    } else if units >= 3 {
        parts[units - 1] = Part::Chorus;
        if units >= 4 {
            parts[units - 2] = Part::Chorus;
        }
    }
    let mut out = vec![Section { part: Part::Intro, start: 0, bars: 4 }];
    for p in parts {
        match out.last_mut() {
            Some(s) if s.part == p && s.bars < 8 => s.bars += 4,
            _ => {
                let start = out.last().map_or(0, |s| s.start + s.bars);
                out.push(Section { part: p, start, bars: 4 });
            }
        }
    }
    let start = out.last().map_or(0, |s| s.start + s.bars);
    out.push(Section { part: Part::Outro, start, bars: 4 });
    out
}

const MAJOR: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];
const MINOR: [i32; 7] = [0, 2, 3, 5, 7, 8, 10];
const DORIAN: [i32; 7] = [0, 2, 3, 5, 7, 9, 10];
const HARMONIC: [i32; 7] = [0, 2, 3, 5, 7, 8, 11];
const PHRYGIAN_DOMINANT: [i32; 7] = [0, 1, 4, 5, 7, 8, 10];

/// A chord on a scale degree: a triad (3), a seventh (4) or a ninth (5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chord {
    pub deg: i32,
    pub size: u8,
}

/// The parts that start a note at one sixteenth.
pub(crate) type Events = [Option<Note>; CH];

fn set(o: &mut Events, ch: Chan, n: Note) {
    o[ch as usize] = Some(n);
}

/// Where a sixteenth falls in the song.
pub(crate) struct At {
    pub bar: u32,
    pub step: u32,
    pub part: Part,
    /// The bar within its section, the section's length, its index.
    pub bi: u32,
    pub len: u32,
    pub section: usize,
    pub chord: Chord,
    pub next: Chord,
    /// A hash for this bar's variations.
    pub h: u64,
}

impl At {
    fn last(&self) -> bool {
        self.bi + 1 == self.len
    }
}

/// A song worked out from its meta: everything the sequencer needs.
pub(crate) struct Plan {
    pub genre: Genre,
    /// Seconds a beat.
    pub spb: f32,
    pub sections: Vec<Section>,
    by_bar: Vec<(usize, u32)>,
    /// The key's root (MIDI, around C3) and its scale.
    pub root: i32,
    pub scale: [i32; 7],
    /// Verse, chorus and bridge progressions: a chord a bar.
    pub prog: [[Chord; 4]; 3],
    /// The verse's and the chorus's two-bar melodies: (scale degree, sixteenths held) at the
    /// sixteenths they start on.
    pub motif: [[Option<(i32, u8)>; 32]; 2],
    /// How late the off-beat sixteenths and eighths fall (fractions of a sixteenth, an eighth).
    pub swing16: f32,
    pub swing8: f32,
    pub seed: u64,
}

/// A melody's rhythm: (sixteenth it starts on, sixteenths held), over two bars.
type Rhythm = &'static [(u8, u8)];

fn rhythms(g: Genre) -> [Rhythm; 2] {
    match g {
        Genre::Boombap => {
            [&[(0, 3), (6, 2), (10, 6), (16, 3), (22, 2), (26, 6)], &[(0, 3), (3, 3), (6, 4), (16, 3), (19, 3), (22, 2), (24, 6)]]
        }
        Genre::Surf => [
            &[(0, 6), (6, 2), (8, 8), (16, 6), (22, 2), (24, 8)],
            &[(0, 2), (2, 2), (4, 4), (8, 2), (10, 2), (12, 4), (16, 2), (18, 2), (20, 4), (24, 8)],
        ],
        Genre::Cumbia => [
            &[(0, 2), (3, 1), (4, 2), (8, 4), (14, 2), (16, 2), (19, 1), (20, 2), (24, 6)],
            &[(0, 1), (2, 1), (4, 2), (6, 2), (8, 1), (10, 1), (12, 4), (16, 1), (18, 1), (20, 2), (22, 2), (24, 6), (30, 2)],
        ],
        Genre::Smooth => [
            &[(0, 6), (6, 2), (8, 6), (16, 6), (24, 4), (30, 2)],
            &[(0, 3), (3, 3), (6, 2), (8, 8), (16, 3), (19, 3), (22, 2), (24, 8)],
        ],
        _ => [
            &[(0, 2), (4, 2), (8, 4), (14, 2), (16, 2), (20, 2), (24, 6)],
            &[(0, 4), (4, 2), (6, 2), (8, 6), (16, 4), (20, 2), (22, 2), (24, 4), (28, 4)],
        ],
    }
}

/// A two-bar melody: a walk over the scale from the tonic's chord, kept within `lo..=hi`.
fn motif(seed: u64, rhythm: Rhythm, lo: i32, hi: i32) -> [Option<(i32, u8)>; 32] {
    let mut m = [None; 32];
    let mut d = lo + [0, 2, 4][(seed % 3) as usize] + 2;
    for (i, &(at, len)) in rhythm.iter().enumerate() {
        let h = dsp::mix(seed ^ (i as u64 + 1).wrapping_mul(0x9e37_79b9_7f4a_7c15));
        d = (d + [-2, -1, -1, 1, 1, 2, 0, 3, -3][(h % 9) as usize]).clamp(lo, hi);
        m[at as usize] = Some((d, len));
    }
    m
}

impl Plan {
    pub fn new(song: &SongMeta, genre: Genre) -> Plan {
        let seed = song.seed;
        let pick = |k: u64, n: usize| (dsp::mix(seed ^ k) % n as u64) as usize;
        let minor = song.minor;
        let (scale, progs): ([i32; 7], [[i32; 4]; 4]) = match (genre, minor) {
            (Genre::Boombap, true) => (DORIAN, [[0, 0, 3, 3], [0, 3, 0, 3], [0, 6, 3, 4], [0, 2, 3, 6]]),
            (Genre::Boombap, false) => (MAJOR, [[1, 4, 0, 0], [0, 5, 1, 4], [3, 2, 1, 0], [0, 3, 2, 5]]),
            (Genre::Surf, true) => (PHRYGIAN_DOMINANT, [[0, 1, 0, 1], [0, 1, 3, 1], [0, 3, 1, 0], [0, 6, 1, 0]]),
            (Genre::Surf, false) => (MAJOR, [[0, 3, 4, 3], [0, 5, 3, 4], [0, 3, 0, 4], [0, 0, 3, 4]]),
            (Genre::Cumbia, true) => (HARMONIC, [[0, 3, 4, 0], [0, 4, 0, 4], [0, 3, 0, 4], [0, 5, 3, 4]]),
            (Genre::Cumbia, false) => (MAJOR, [[0, 4, 4, 0], [0, 3, 4, 0], [0, 4, 0, 4], [0, 3, 0, 4]]),
            (Genre::Smooth, true) => (DORIAN, [[0, 3, 0, 3], [0, 3, 6, 2], [0, 1, 0, 3], [0, 6, 3, 2]]),
            (Genre::Smooth, false) => (MAJOR, [[0, 5, 1, 4], [1, 4, 0, 0], [3, 2, 1, 4], [0, 3, 2, 5]]),
            (_, true) => (MINOR, [[0, 5, 2, 6], [0, 3, 5, 4], [0, 6, 5, 6], [0, 5, 3, 4]]),
            (_, false) => (MAJOR, [[0, 4, 5, 3], [0, 5, 3, 4], [3, 0, 4, 5], [0, 3, 0, 4]]),
        };
        let size = |deg: i32| -> u8 {
            match genre {
                Genre::Boombap | Genre::Smooth => 4 + (deg % 2 == 0) as u8,
                // The dominant gets its seventh.
                Genre::Cumbia if deg == 4 => 4,
                _ => 3,
            }
        };
        let chords = |p: [i32; 4]| p.map(|deg| Chord { deg, size: size(deg) });
        let (v, c) = (pick(1, 4), pick(2, 4));
        let c = if c == v { (c + 1) % 4 } else { c };
        let b = (v + 2) % 4;
        let [rv, rc] = rhythms(genre);
        let sections = sections(song.bars);
        let mut by_bar = Vec::new();
        for (i, s) in sections.iter().enumerate() {
            for b in 0..s.bars {
                by_bar.push((i, b));
            }
        }
        let u = |k: u64| dsp::unit(seed, k);
        let (swing16, swing8) = match genre {
            Genre::Boombap => (0.2 + 0.12 * u(3), 0.0),
            Genre::Cumbia => (0.06, 0.0),
            Genre::Smooth => (0.0, 0.22 + 0.1 * u(4)),
            _ => (0.0, 0.0),
        };
        Plan {
            genre,
            spb: 60.0 / song.bpm.max(40.0),
            by_bar,
            sections,
            root: 48 + song.key as i32 % 12,
            scale,
            prog: [chords(progs[v]), chords(progs[c]), chords(progs[b])],
            motif: [motif(dsp::mix(seed ^ 11), rv, 2, 9), motif(dsp::mix(seed ^ 12), rc, 5, 12)],
            swing16,
            swing8,
            seed,
        }
    }

    pub fn six(&self) -> f32 {
        self.spb / 4.0
    }

    pub fn bar_seconds(&self) -> f32 {
        self.spb * 4.0
    }

    /// The note `deg` scale degrees above the key's root, `oct` octaves up.
    pub fn note(&self, deg: i32, oct: i32) -> i32 {
        self.root + self.scale[deg.rem_euclid(7) as usize] + 12 * (deg.div_euclid(7) + oct)
    }

    /// A chord's notes, `oct` octaves up.
    pub fn chord_keys(&self, c: Chord, oct: i32) -> ([i32; 5], u8) {
        let mut keys = [0; 5];
        for (i, k) in keys.iter_mut().enumerate().take(c.size as usize) {
            *k = self.note(c.deg + 2 * i as i32, oct);
        }
        (keys, c.size)
    }

    /// The chord tone of `c` nearest scale degree `deg`.
    fn snap(&self, deg: i32, c: Chord) -> i32 {
        (deg - 3..=deg + 3)
            .filter(|d| (d - c.deg).rem_euclid(7) % 2 == 0 && (d - c.deg).rem_euclid(7) <= 4)
            .min_by_key(|d| (d - deg).abs())
            .unwrap_or(deg)
    }

    /// Beats from a bar's start to its sixteenth `s` (swung).
    pub fn step_beat(&self, s: u32) -> f32 {
        let and = 0.5 + 0.5 * self.swing8;
        let pos = match s % 4 {
            0 => 0.0,
            1 => 0.25 + 0.25 * self.swing16,
            2 => and,
            _ => and + (1.0 - and) * (0.5 + 0.5 * self.swing16),
        };
        (s / 4) as f32 + pos
    }

    /// Seconds into the song that sixteenth `g` (counted from its start) begins.
    pub fn onset(&self, g: i64) -> f64 {
        let bar = g.div_euclid(16);
        (bar as f64 * 4.0 + self.step_beat(g.rem_euclid(16) as u32) as f64) * self.spb as f64
    }

    /// The sixteenth sounding `t` seconds into the song.
    pub fn step_at(&self, t: f64) -> i64 {
        let beats = t / self.spb as f64;
        let bar = (beats / 4.0).floor();
        let x = (beats - bar * 4.0) as f32;
        let s = (0..16u32).take_while(|&k| self.step_beat(k) <= x + 1e-6).last().unwrap_or(0);
        bar as i64 * 16 + s as i64
    }

    pub fn chord(&self, bar: u32) -> Chord {
        let Some(&(si, bi)) = self.by_bar.get(bar as usize) else { return self.prog[0][0] };
        let p = match self.sections[si].part {
            Part::Chorus => 1,
            Part::Bridge => 2,
            _ => 0,
        };
        self.prog[p][(bi % 4) as usize]
    }

    /// The parts that start a note at sixteenth `g`.
    pub fn events(&self, g: i64, out: &mut Events) {
        *out = [None; CH];
        if g < 0 {
            return;
        }
        let (bar, step) = ((g / 16) as u32, (g % 16) as u32);
        let Some(&(si, bi)) = self.by_bar.get(bar as usize) else { return };
        let s = self.sections[si];
        let at = At {
            bar,
            step,
            part: s.part,
            bi,
            len: s.bars,
            section: si,
            chord: self.chord(bar),
            next: self.chord(bar + 1),
            h: dsp::mix(self.seed ^ (bar as u64).wrapping_mul(0x2545_f491_4f6c_dd1d)),
        };
        match self.genre {
            Genre::Boombap => self.boombap(&at, out),
            Genre::Surf => self.surf(&at, out),
            Genre::Cumbia => self.cumbia(&at, out),
            Genre::Smooth => self.smooth(&at, out),
            Genre::Talk => {}
            Genre::Synthwave => self.synthwave(&at, out),
        }
    }

    /// The melody's note at this sixteenth, if one starts: from motif `which`, `oct` octaves
    /// up, with the previous note to slide from.
    fn melody(&self, a: &At, which: usize, oct: i32, vel: f32) -> Option<Note> {
        let m = ((a.bi * 16 + a.step) % 32) as usize;
        let (deg, len) = self.motif[which][m]?;
        let deg = if m.is_multiple_of(8) { self.snap(deg, a.chord) } else { deg };
        let prev = (1..32).find_map(|k| self.motif[which][(m + 32 - k) % 32]).map(|(d, _)| self.note(d, oct));
        let accent = if m.is_multiple_of(8) { 1.0 } else { 0.85 + 0.15 * dsp::unit(a.h, m as u64) };
        let mut n = Note::tone(self.note(deg, oct), vel * accent, len as f32 * self.six() * 0.95);
        n.from = prev.unwrap_or(0);
        // A third below, for parts that play in thirds.
        n.keys[1] = self.note(deg - 2, oct);
        Some(n)
    }

    fn synthwave(&self, a: &At, o: &mut Events) {
        let six = self.six();
        let full = matches!(a.part, Part::Verse | Part::Chorus);
        let fill = full && a.last() && a.step >= 12;
        let builds = (a.part == Part::Intro && a.last()) || (a.part == Part::Outro && a.bi < 2);
        if a.step.is_multiple_of(4) && (full || builds) {
            set(o, Chan::Kick, Note::hit(1.0));
        }
        if (full && !fill && (a.step == 4 || a.step == 12)) || (a.part == Part::Bridge && a.step == 12) {
            set(o, Chan::Snare, Note::hit(if a.part == Part::Bridge { 0.8 } else { 1.0 }));
        }
        if fill {
            let mut t = Note::hit(0.9);
            t.keys[0] = [52, 49, 45, 41][(a.step - 12) as usize];
            set(o, Chan::Tom, t);
        }
        if a.part != Part::Bridge && a.step % 4 == 2 {
            set(o, Chan::Hat, Note::hit(0.8));
        } else if a.part == Part::Chorus && a.step % 2 == 1 {
            set(o, Chan::Hat, Note::hit(0.35));
        }
        if a.step == 0 && a.bi == 0 && full {
            set(o, Chan::Perc, Note::hit(0.9));
        }
        if a.part != Part::Intro && a.step.is_multiple_of(2) {
            let k = self.note(a.chord.deg, -1) + if a.step % 4 == 2 { 12 } else { 0 };
            set(o, Chan::Bass, Note::tone(k, 0.9, six * 1.6));
        }
        if a.step == 0 {
            let (keys, n) = self.chord_keys(a.chord, 1);
            set(o, Chan::Keys, Note { vel: 0.9, keys, n, hold: self.bar_seconds() * 0.97, ..Default::default() });
        }
        if full || a.part == Part::Bridge || (a.part == Part::Intro && a.bi >= 2) {
            let up = [0, 2, 4, 7][(a.step % 4) as usize];
            let deg = a.chord.deg + if (a.step / 4) % 2 == 1 { 7 - up } else { up };
            set(o, Chan::Arp, Note::tone(self.note(deg, 2), if a.step.is_multiple_of(4) { 0.9 } else { 0.6 }, six * 0.8));
        }
        if matches!(a.part, Part::Chorus | Part::Bridge)
            && let Some(n) = self.melody(a, 1, 1, 0.9)
        {
            set(o, Chan::Lead, n);
        }
    }

    fn boombap(&self, a: &At, o: &mut Events) {
        let six = self.six();
        let variant = dsp::mix(self.seed ^ a.section as u64) % 3;
        let kicks: &[u32] = match (a.part, variant) {
            (Part::Bridge, _) => &[0],
            (_, 0) => &[0, 8, 10],
            (_, 1) => &[0, 3, 8],
            _ => &[0, 7, 10],
        };
        let drums = match a.part {
            Part::Intro => a.bi >= 2,
            Part::Outro => a.bi < 2,
            _ => true,
        };
        if drums && kicks.contains(&a.step) {
            set(o, Chan::Kick, Note::hit(if a.step == 0 { 1.0 } else { 0.8 }));
            // The sub follows the kick, held to the next one.
            let next = kicks.iter().find(|&&k| k > a.step).copied().unwrap_or(16);
            let deg = if a.step >= 10 { a.chord.deg + 4 } else { a.chord.deg };
            let mut b = Note::tone(self.note(deg, -1), 0.9, (next - a.step) as f32 * six * 0.95);
            if a.step == 0 {
                b.from = self.note(self.chord(a.bar.saturating_sub(1)).deg, -1);
            }
            set(o, Chan::Bass, b);
        }
        if drums && ((a.part != Part::Bridge && (a.step == 4 || a.step == 12)) || (a.part == Part::Bridge && a.step == 12)) {
            set(o, Chan::Snare, Note::hit(1.0));
        } else if drums && a.step == 15 && a.h.is_multiple_of(3) {
            set(o, Chan::Snare, Note::hit(0.25));
        }
        let hats = match a.part {
            Part::Intro => a.bi >= 1,
            Part::Bridge => false,
            _ => true,
        };
        if hats {
            let v = if a.step.is_multiple_of(4) {
                0.8
            } else if a.step.is_multiple_of(2) {
                0.65
            } else {
                0.3
            };
            set(o, Chan::Hat, Note::hit(v));
        }
        if drums && a.last() && a.step == 14 {
            set(o, Chan::Perc, Note { kind: 1, ..Note::hit(0.7) });
        } else if a.part == Part::Chorus && a.step == 6 && a.h.is_multiple_of(2) {
            set(o, Chan::Perc, Note::hit(0.5));
        }
        let comp = if a.h.is_multiple_of(2) { 10 } else { 11 };
        if a.step == 0 || a.step == comp {
            let (keys, n) = self.chord_keys(a.chord, 0);
            let hold = if a.step == 0 { 6.0 } else { 3.0 } * six;
            set(o, Chan::Keys, Note { vel: if a.step == 0 { 0.9 } else { 0.7 }, keys, n, hold, ..Default::default() });
        }
        if a.part == Part::Chorus
            && let Some(n) = self.melody(a, 1, 1, 0.8)
        {
            set(o, Chan::Lead, Note { n: 1, ..n });
        }
    }

    fn surf(&self, a: &At, o: &mut Events) {
        let six = self.six();
        let full = matches!(a.part, Part::Verse | Part::Chorus) || (a.part == Part::Outro && a.bi < 2);
        match a.part {
            Part::Intro => {
                // A floor-tom roll building up, a snare fill into the verse.
                if a.last() && a.step >= 8 {
                    set(o, Chan::Snare, Note::hit(0.5 + 0.06 * (a.step - 8) as f32));
                } else if a.step.is_multiple_of(2) {
                    let mut t = Note::hit(0.45 + 0.15 * a.bi as f32 + if a.step.is_multiple_of(4) { 0.15 } else { 0.0 });
                    t.keys[0] = 41;
                    set(o, Chan::Tom, t);
                }
            }
            Part::Bridge => {
                // The tom-tom beat.
                if a.step.is_multiple_of(2) {
                    let mut t = Note::hit(if a.step.is_multiple_of(4) { 0.95 } else { 0.6 });
                    t.keys[0] = if a.step % 8 == 4 { 45 } else { 41 };
                    set(o, Chan::Tom, t);
                }
                if a.step == 0 || a.step == 8 {
                    set(o, Chan::Kick, Note::hit(0.9));
                }
            }
            _ => {}
        }
        if full {
            let fill = a.last() && a.step >= 12 && a.part != Part::Outro;
            if a.step == 0 || a.step == 8 || (a.part == Part::Chorus && a.step == 10) {
                set(o, Chan::Kick, Note::hit(1.0));
            }
            if fill {
                set(o, Chan::Snare, Note { kind: 1, ..Note::hit(0.55 + 0.12 * (a.step - 12) as f32) });
            } else if a.step == 4 || a.step == 12 {
                set(o, Chan::Snare, Note::hit(1.0));
            }
            if a.step.is_multiple_of(2) {
                set(o, Chan::Hat, Note::hit(if a.step.is_multiple_of(4) { 0.9 } else { 0.55 }));
            }
            if a.step == 0 && a.bi == 0 {
                set(o, Chan::Perc, Note::hit(0.9));
            }
        }
        if a.part == Part::Outro && a.bi == 2 && a.step == 0 {
            set(o, Chan::Perc, Note::hit(1.0));
            set(o, Chan::Kick, Note::hit(1.0));
        }
        let bass = !(a.part == Part::Intro && a.bi < 2) && !(a.part == Part::Outro && a.bi >= 2);
        if bass && a.step.is_multiple_of(2) {
            let deg = a.chord.deg + [0, 0, 4, 4, 0, 0, 5, 4][(a.step / 2) as usize];
            set(o, Chan::Bass, Note::tone(self.note(deg, -1), if a.step.is_multiple_of(4) { 0.95 } else { 0.75 }, six * 1.7));
        }
        // The rhythm guitar: stabs in the verse, eighths in the chorus.
        let stab = match a.part {
            Part::Verse => [0, 6, 8, 14].contains(&a.step),
            Part::Chorus => a.step.is_multiple_of(2),
            Part::Intro => a.last() && a.step == 0,
            Part::Outro => a.bi == 2 && a.step == 0,
            Part::Bridge => false,
        };
        if stab {
            let (keys, n) = self.chord_keys(a.chord, 0);
            let hold = match a.part {
                Part::Intro => 6.0 * six,
                Part::Outro => 16.0 * six,
                _ => 1.4 * six,
            };
            let vel = if a.step.is_multiple_of(4) { 0.9 } else { 0.7 };
            set(o, Chan::Keys, Note { vel, keys, n, hold, ..Default::default() });
        }
        // The lead tremolo-picks the melody; slides down into the verse and out of the bridge.
        let tremolo = |n: Note| Note { kind: 1, aux: six * 0.5, n: 1, ..n };
        match a.part {
            Part::Verse => {
                if let Some(n) = self.melody(a, 0, 1, 0.85) {
                    set(o, Chan::Lead, tremolo(n));
                }
            }
            Part::Chorus => {
                if let Some(n) = self.melody(a, 1, 1, 0.95) {
                    set(o, Chan::Lead, tremolo(n));
                }
            }
            Part::Intro | Part::Bridge if a.last() && a.step == 8 => {
                let mut n = Note::tone(self.note(a.chord.deg + 14, 0), 0.9, 7.0 * six);
                n.kind = 2;
                set(o, Chan::Lead, n);
            }
            _ => {}
        }
    }

    fn cumbia(&self, a: &At, o: &mut Events) {
        let six = self.six();
        let beat = self.spb;
        let full = matches!(a.part, Part::Verse | Part::Chorus) || (a.part == Part::Outro && a.bi < 2);
        // The güiro: long, short, short on every beat (half of it in the bridge).
        let scrape = a.part != Part::Bridge || a.step < 8;
        if scrape {
            match a.step % 4 {
                0 => set(o, Chan::Perc, Note { hold: beat * 0.45, ..Note::hit(0.9) }),
                2 => set(o, Chan::Perc, Note { hold: beat * 0.11, ..Note::hit(0.7) }),
                3 => set(o, Chan::Perc, Note { hold: beat * 0.11, ..Note::hit(0.6) }),
                _ => {}
            }
        }
        if (full || (a.part == Part::Intro && a.last())) && (a.step == 0 || a.step == 8) {
            set(o, Chan::Kick, Note::hit(0.9));
        }
        if full {
            if a.last() && a.step >= 12 && a.part != Part::Outro {
                set(o, Chan::Snare, Note { kind: 1, ..Note::hit(0.6 + 0.1 * (a.step - 12) as f32) });
            } else if a.step % 4 == 2 {
                set(o, Chan::Snare, Note::hit(0.7));
            }
            let conga = match a.step {
                3 | 11 => Some((2, 0.6)),
                6 | 7 => Some((0, 0.7)),
                14 | 15 => Some((1, 0.6)),
                _ => None,
            };
            if let Some((kind, v)) = conga {
                set(o, Chan::Tom, Note { kind, ..Note::hit(v) });
            }
            if a.part == Part::Chorus && a.step.is_multiple_of(2) {
                set(o, Chan::Hat, Note::hit(if a.step.is_multiple_of(4) { 0.5 } else { 0.35 }));
            }
        }
        // The bass bounces: the root on one, the fifth on three, pickups before each.
        let bass = !(a.part == Part::Intro && a.bi < 2);
        if bass {
            let b = match a.step {
                0 => Some((0, 0.95, 1.8)),
                6 => Some((0, 0.6, 1.0)),
                8 => Some((4, 0.9, 1.8)),
                14 => Some((4, 0.6, 1.0)),
                _ => None,
            };
            if let Some((d, v, len)) = b {
                set(o, Chan::Bass, Note::tone(self.note(a.chord.deg + d, -1), v, len * six));
            }
        }
        // Accordion chords on the off-beats (the verse, the intro, the bridge).
        if matches!(a.part, Part::Verse | Part::Intro | Part::Bridge | Part::Outro) && a.step % 4 == 2 {
            let (keys, n) = self.chord_keys(a.chord, 1);
            set(o, Chan::Keys, Note { vel: 0.7, keys, n: n.min(3), hold: six * 0.8, ..Default::default() });
        }
        // The accordion melody, in thirds.
        let lead = match a.part {
            Part::Chorus => self.melody(a, 1, 1, 0.95),
            Part::Verse => self.melody(a, 0, 1, 0.8),
            Part::Intro => self.melody(a, 1, 1, 0.7),
            _ => None,
        };
        if let Some(n) = lead {
            set(o, Chan::Lead, Note { n: 2, ..n });
        }
    }

    fn smooth(&self, a: &At, o: &mut Events) {
        let six = self.six();
        let beat = self.spb;
        let drums = !(a.part == Part::Intro && a.bi < 2);
        if drums {
            let extra = if a.h.is_multiple_of(2) { 7 } else { 10 };
            match a.step {
                0 => set(o, Chan::Kick, Note::hit(0.8)),
                8 => set(o, Chan::Kick, Note::hit(0.6)),
                s if s == extra && a.part != Part::Intro => set(o, Chan::Kick, Note::hit(0.45)),
                _ => {}
            }
            if a.step == 4 || a.step == 12 {
                let kind = matches!(a.part, Part::Chorus | Part::Bridge) as u8;
                set(o, Chan::Snare, Note { kind, ..Note::hit(0.7) });
            }
            // The ride: ding, ding-a, ding, ding-a (the "a" swung).
            let ride = match a.step {
                0 | 8 => Some(0.7),
                4 | 12 => Some(0.85),
                6 | 14 => Some(0.5),
                _ => None,
            };
            if let Some(v) = ride {
                set(o, Chan::Hat, Note::hit(v));
            }
        }
        if a.step.is_multiple_of(4) {
            set(o, Chan::Perc, Note { hold: beat * 0.85, ..Note::hit(0.6) });
        }
        let bass = !(a.part == Part::Intro && a.bi < 2);
        if bass {
            let b = match a.step {
                0 => Some((a.chord.deg, 5.5)),
                6 => Some((a.chord.deg + 4, 1.8)),
                8 => Some((a.chord.deg + 7, 3.5)),
                12 => Some((a.chord.deg + 2, 1.8)),
                // A step into the next bar's root.
                14 => Some((a.next.deg + if a.h.is_multiple_of(2) { 1 } else { -1 }, 1.8)),
                _ => None,
            };
            if let Some((d, len)) = b {
                set(o, Chan::Bass, Note::tone(self.note(d, -1), 0.8, len * six));
            }
        }
        let comp = match a.step {
            0 => Some((7.0, 0.8)),
            6 if !a.h.is_multiple_of(3) => Some((1.5, 0.55)),
            10 => Some((5.0, 0.65)),
            _ => None,
        };
        if let Some((len, v)) = comp {
            let (keys, n) = self.chord_keys(a.chord, 0);
            set(o, Chan::Keys, Note { vel: v, keys, n, hold: len * six, ..Default::default() });
        }
        let sax = match a.part {
            Part::Verse => self.melody(a, 0, 1, 0.85),
            Part::Chorus => self.melody(a, 1, 1, 0.95),
            Part::Bridge => self.melody(a, 1, 0, 0.7),
            Part::Outro if a.bi < 2 => self.melody(a, 1, 1, 0.8),
            _ => None,
        };
        if let Some(n) = sax {
            set(o, Chan::Lead, Note { n: 1, ..n });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn songs_have_an_intro_verses_choruses_and_an_outro() {
        for bars in (8..=96).step_by(4) {
            let s = sections(bars);
            assert_eq!(s.first().unwrap().part, Part::Intro);
            assert_eq!(s.last().unwrap().part, Part::Outro);
            assert_eq!(s.iter().map(|x| x.bars).sum::<u32>(), bars, "{bars}: {s:?}");
            for w in s.windows(2) {
                assert_eq!(w[0].start + w[0].bars, w[1].start);
            }
            if bars >= 24 {
                assert!(s.iter().any(|x| x.part == Part::Verse) && s.iter().any(|x| x.part == Part::Chorus), "{s:?}");
                // The last section before the outro is a chorus.
                assert_eq!(s[s.len() - 2].part, Part::Chorus, "{s:?}");
            }
            if bars >= 48 {
                assert!(s.iter().any(|x| x.part == Part::Bridge), "{bars}: {s:?}");
            }
        }
    }

    #[test]
    fn swing_moves_the_off_beats_late() {
        let song = SongMeta { seed: 5, bpm: 90.0, bars: 32, genre: "boombap".into(), ..Default::default() };
        let p = Plan::new(&song, Genre::Boombap);
        assert!(p.swing16 > 0.15, "boom-bap swings its sixteenths");
        assert!(p.step_beat(1) > 0.25 && p.step_beat(2) == 0.5 && p.step_beat(3) > 0.75);
        // The sixteenth at any moment is the last one begun.
        for g in 0..64 {
            let t = p.onset(g);
            assert_eq!(p.step_at(t + 1e-4), g);
            assert_eq!(p.step_at(t - 1e-4), g - 1);
        }
        let smooth = Plan::new(&SongMeta { genre: "smooth".into(), ..song }, Genre::Smooth);
        assert!(smooth.step_beat(2) > 0.6, "smooth jazz swings its eighths");
    }
}
