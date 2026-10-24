//! Radio music (PLAN.md §6.5 E8, §5.12): a song from its `SongMeta`, rendered by a
//! sample-accurate step sequencer in the song's genre. Each genre has its own kit, feel and
//! instruments ([`kit`], [`style`]): synthwave's gated snares, arpeggios and saw leads;
//! boom-bap's swung drums, vinyl, electric piano and sub bass; surf's driving drums and driven
//! square guitars through a spring; cumbia's güiro, accordions and bouncing bass; smooth jazz's
//! brushes, seventh chords and sax. A song has sections (intro, verses, choruses, a bridge in
//! longer songs, outro) and plays from any offset, so a station resumes mid-song: each note's
//! sound is a function of the time since it began.
//!
//! Past the song (`seconds`) the station's own ident plays, then a quiet bed until the next
//! slot. A talk station has no songs: its air is its talk segments. Segments
//! (`pav_core::radio::Segment`: voices, the sounder, jingles) play over either, and the music
//! dips under a voice.

mod kit;
mod style;
mod talk;

use pav_core::radio::{Segment, SegmentKind, SongMeta};

use crate::dsp::{self, Echo, OnePole, Spring};
use crate::voice::Babble;
use kit::{CH, CHANS, Mem, Note};
pub use style::{Part, Section, sections};

/// The genres the renderer plays (`StationDef::genre`); any other name plays synthwave.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Genre {
    Synthwave,
    Boombap,
    Surf,
    Cumbia,
    Smooth,
    /// News and talk: no songs, only segments over a news bed.
    Talk,
}

impl Genre {
    pub const ALL: [Genre; 6] = [Genre::Synthwave, Genre::Boombap, Genre::Surf, Genre::Cumbia, Genre::Smooth, Genre::Talk];

    pub fn named(name: &str) -> Genre {
        match name {
            "boombap" => Genre::Boombap,
            "surf" => Genre::Surf,
            "cumbia" => Genre::Cumbia,
            "smooth" => Genre::Smooth,
            "talk" => Genre::Talk,
            _ => Genre::Synthwave,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Genre::Synthwave => "synthwave",
            Genre::Boombap => "boombap",
            Genre::Surf => "surf",
            Genre::Cumbia => "cumbia",
            Genre::Smooth => "smooth",
            Genre::Talk => "talk",
        }
    }

    /// The level that makes every genre about as loud as the others.
    fn gain(self) -> f32 {
        match self {
            Genre::Synthwave => 0.85,
            Genre::Boombap => 0.84,
            Genre::Surf => 0.82,
            Genre::Cumbia => 0.9,
            Genre::Smooth => 0.92,
            Genre::Talk => 1.0,
        }
    }
}

/// What should be playing (a game's `Music` intent).
#[derive(Clone, Debug, PartialEq)]
pub struct MusicTarget {
    pub station: String,
    pub song: SongMeta,
    /// Seconds into the song's slot.
    pub offset: f32,
    pub volume: f32,
    /// 0..1: heard from outside the car (low-passed).
    pub muffle: f32,
    /// Talk on the slot's air.
    pub segments: Vec<Segment>,
}

/// A note sounding: when it began (seconds into the slot) and which filter memory it uses.
#[derive(Clone, Copy, Debug)]
struct Hit {
    at: f64,
    note: Note,
    slot: usize,
}

/// The genre's effect: an echo, a spring, or none.
enum Fx {
    Dry,
    Echo(Box<Echo>),
    Spring(Box<Spring>),
}

/// How loud a voice speaks over the music, and how far the music dips under it.
const VOICE: f32 = 1.1;
const TALKOVER: f32 = 0.4;

/// One song playing: its position, its parts' notes and the filters' memory.
pub struct MusicVoice {
    pub station: String,
    pub song: SongMeta,
    /// Seconds into the slot.
    pub t: f64,
    gain: f32,
    pub target_gain: f32,
    muffle: f32,
    pub target_muffle: f32,
    segments: Vec<Segment>,
    genre: Genre,
    plan: style::Plan,
    ident: talk::Ident,
    sample_rate: f32,
    dt: f32,
    /// The sixteenth last stepped; each part's note now and the one before it (its tail).
    step: Option<i64>,
    cur: [Option<Hit>; CH],
    prev: [Option<Hit>; CH],
    mem: [[Mem; 2]; CH],
    ident_mem: [Mem; 4],
    fx: Fx,
    master: [OnePole; 2],
    out_lp: (f32, f32),
    /// The music's level under talk (dips while a voice speaks).
    talkover: f32,
    babbles: Vec<(u64, Babble)>,
    /// Checks: only this part plays.
    #[cfg(test)]
    solo: Option<usize>,
}

impl MusicVoice {
    pub fn new(target: &MusicTarget, sample_rate: f32) -> MusicVoice {
        let s = &target.song;
        let genre = Genre::named(&s.genre);
        let plan = style::Plan::new(s, genre);
        let dt = 1.0 / sample_rate;
        let fx = match genre {
            // A dotted-eighth ping-pong for the arpeggio and the lead.
            Genre::Synthwave => Fx::Echo(Box::new(Echo::new(plan.spb * 0.75, dt))),
            Genre::Boombap => Fx::Echo(Box::new(Echo::new(plan.spb * 0.5, dt))),
            Genre::Surf => Fx::Spring(Box::new(Spring::new(dt))),
            Genre::Smooth => Fx::Echo(Box::new(Echo::new(0.09, dt))),
            Genre::Cumbia => Fx::Echo(Box::new(Echo::new(0.11, dt))),
            Genre::Talk => Fx::Dry,
        };
        MusicVoice {
            station: target.station.clone(),
            song: s.clone(),
            t: target.offset.max(0.0) as f64,
            gain: 0.0,
            target_gain: target.volume,
            muffle: target.muffle,
            target_muffle: target.muffle,
            segments: target.segments.clone(),
            genre,
            ident: talk::Ident::new(s.station, genre),
            plan,
            sample_rate,
            dt,
            step: None,
            cur: [None; CH],
            prev: [None; CH],
            mem: [[Mem::default(); 2]; CH],
            ident_mem: [Mem::default(); 4],
            fx,
            master: [OnePole::default(); 2],
            out_lp: (0.0, 0.0),
            talkover: 1.0,
            babbles: Vec::new(),
            #[cfg(test)]
            solo: None,
        }
    }

    /// The talk on the slot's air changed (a report said, a DJ break laid out).
    pub fn set_segments(&mut self, segments: &[Segment]) {
        if self.segments != segments {
            self.segments = segments.to_vec();
            self.babbles.retain(|(k, _)| segments.iter().any(|s| key(s) == *k));
        }
    }

    /// True when it has faded out (after `fade`).
    pub fn silent(&self) -> bool {
        self.gain < 1e-4 && self.target_gain <= 0.0
    }

    /// The next stereo sample.
    pub fn next(&mut self, dt: f32) -> (f32, f32) {
        let (l, r) = self.sample();
        self.t += dt as f64;
        let k = 1.0 - (-dt / 0.25).exp();
        self.gain += (self.target_gain - self.gain) * k;
        self.muffle += (self.target_muffle - self.muffle) * k;
        // Muffled (outside the car): a low-pass that closes with `muffle`.
        let a = 1.0 - self.muffle.clamp(0.0, 1.0) * 0.93;
        self.out_lp.0 += (l - self.out_lp.0) * a;
        self.out_lp.1 += (r - self.out_lp.1) * a;
        (self.out_lp.0 * self.gain, self.out_lp.1 * self.gain)
    }

    fn sample(&mut self) -> (f32, f32) {
        let t = self.t;
        let tf = t as f32;
        let n = (t * self.sample_rate as f64) as u64;
        let (l, r) = match self.genre {
            Genre::Talk => (0.0, 0.0),
            _ if tf < self.song.seconds => self.song_sample(t, n),
            g => {
                // The ident's noise counts from the break, so it sounds the same every time.
                let u = tf - self.song.seconds;
                let nu = (u * self.sample_rate) as u64;
                self.ident.play(g, u, nu, u, self.dt, &mut self.ident_mem)
            }
        };
        let (music, voice, speaking) = self.talk(tf, n);
        let k = 1.0 - (-self.dt / 0.05).exp();
        self.talkover += (if speaking { TALKOVER } else { 1.0 } - self.talkover) * k;
        let g = self.genre.gain();
        (knee((l + music) * self.talkover * g + voice), knee((r + music) * self.talkover * g + voice))
    }

    /// The segments on the air at `t`: their music (sounder, jingles, the news bed), their
    /// voices, and whether anyone is speaking.
    fn talk(&mut self, t: f32, n: u64) -> (f32, f32, bool) {
        let (mut music, mut voice, mut speaking, mut bed) = (0.0, 0.0, false, true);
        for i in 0..self.segments.len() {
            let seg = self.segments[i];
            let s = t - seg.start;
            if s < 0.0 || s >= seg.seconds {
                continue;
            }
            match seg.kind {
                SegmentKind::Voice => {
                    speaking = true;
                    let dt = self.dt;
                    voice += self.babble(&seg).next(s, seg.seconds, dt) * VOICE;
                }
                SegmentKind::Chime => {
                    bed = false;
                    music += talk::sounder(&self.ident, s);
                }
                SegmentKind::Jingle => {
                    bed = false;
                    music += talk::jingle(s, seg.seconds, seg.seed, n) * 0.6;
                }
            }
        }
        if self.genre == Genre::Talk && speaking && bed {
            music += talk::news_bed(&self.ident, t);
        }
        (music, voice, speaking)
    }

    fn babble(&mut self, seg: &Segment) -> &mut Babble {
        let k = key(seg);
        let i = match self.babbles.iter().position(|(j, _)| *j == k) {
            Some(i) => i,
            None => {
                self.babbles.push((k, Babble::new(seg.voice, seg.seed)));
                self.babbles.len() - 1
            }
        };
        &mut self.babbles[i].1
    }

    /// Starts the notes of sixteenth `g`: each part's new note takes over, the old one rings out.
    fn trigger(&mut self, g: i64) {
        let mut ev = [None; CH];
        self.plan.events(g, &mut ev);
        let at = self.plan.onset(g);
        for (ch, note) in ev.into_iter().enumerate() {
            let Some(note) = note else { continue };
            let slot = self.cur[ch].map_or(0, |c| 1 - c.slot);
            if let Some(mut c) = self.cur[ch].take() {
                c.note.hold = c.note.hold.min((at - c.at) as f32);
                self.prev[ch] = Some(c);
            }
            kit::fresh(&mut self.mem[ch][slot]);
            self.cur[ch] = Some(Hit { at, note, slot });
        }
    }

    /// Joins the song at sixteenth `g`: the notes still sounding from the last two bars.
    fn rewind(&mut self, g: i64) {
        self.cur = [None; CH];
        self.prev = [None; CH];
        for k in (g - 32).max(0)..=g {
            self.trigger(k);
        }
    }

    fn song_sample(&mut self, t: f64, n: u64) -> (f32, f32) {
        let g = self.plan.step_at(t);
        match self.step {
            Some(s) if g == s => {}
            Some(s) if g > s && g <= s + 64 => {
                for k in s + 1..=g {
                    self.trigger(k);
                }
            }
            _ => self.rewind(g),
        }
        self.step = Some(g);
        let genre = self.genre;
        let (dt, tf) = (self.dt, t as f32);
        let (mut l, mut r, mut send) = (0.0f32, 0.0f32, 0.0f32);
        for (ch, &part) in CHANS.iter().enumerate() {
            #[cfg(test)]
            if self.solo.is_some_and(|s| s != ch) {
                continue;
            }
            for prev in [true, false] {
                let hit = if prev { self.prev[ch] } else { self.cur[ch] };
                let Some(h) = hit else { continue };
                let s = (t - h.at) as f32;
                if s > h.note.hold + kit::tail(genre, part) {
                    if prev {
                        self.prev[ch] = None;
                    } else {
                        self.cur[ch] = None;
                    }
                    continue;
                }
                let o = kit::play(genre, part, &h.note, s, n, tf, dt, &mut self.mem[ch][h.slot]);
                l += o.l;
                r += o.r;
                send += o.send;
            }
        }
        let a = |hz: f32| OnePole::coef(hz, dt);
        let (wl, wr, wet) = match (&mut self.fx, genre) {
            (Fx::Echo(e), Genre::Synthwave) => {
                let (x, y) = e.run(send, send, 0.38, a(3000.0));
                (x, y, 0.5)
            }
            (Fx::Echo(e), Genre::Boombap) => {
                let (x, y) = e.run(send, send, 0.25, a(2500.0));
                (x, y, 0.35)
            }
            (Fx::Echo(e), Genre::Smooth) => {
                let (x, y) = e.run(send, send, 0.3, a(3500.0));
                (x, y, 0.45)
            }
            (Fx::Echo(e), _) => {
                let (x, y) = e.run(send, send, 0.2, a(4000.0));
                (x, y, 0.35)
            }
            (Fx::Spring(s), _) => {
                let (x, y) = s.run(send, 0.62, a(3500.0));
                (x, y, 0.6)
            }
            (Fx::Dry, _) => (0.0, 0.0, 0.0),
        };
        l += wl * wet;
        r += wr * wet;
        if genre == Genre::Boombap {
            // The record: crackle and hiss, and its dull top.
            let v = kit::vinyl(tf, n);
            let lp = a(7500.0);
            l = self.master[0].lp(l + v, lp);
            r = self.master[1].lp(r + v, lp);
        }
        let fade_in = (tf / 0.5).min(1.0);
        let fade_out = ((self.song.seconds - tf) / 1.5).clamp(0.0, 1.0);
        let k = fade_in * fade_out;
        (l * k, r * k)
    }
}

/// Transparent up to 0.85, then a soft knee that never passes 1.
fn knee(x: f32) -> f32 {
    let a = x.abs();
    if a < 0.85 { x } else { x.signum() * (0.85 + 0.15 * ((a - 0.85) / 0.15).tanh()) }
}

fn key(s: &Segment) -> u64 {
    dsp::mix(s.seed ^ ((s.start.to_bits() as u64) << 32) ^ (s.seconds.to_bits() as u64))
}

/// Renders `seconds` of a song from `offset` as interleaved stereo (checks and captures).
pub fn render(target: &MusicTarget, seconds: f32, sample_rate: f32) -> Vec<f32> {
    let mut v = MusicVoice::new(target, sample_rate);
    v.gain = target.volume;
    let dt = 1.0 / sample_rate;
    let mut out = Vec::with_capacity((seconds * sample_rate) as usize * 2);
    for _ in 0..(seconds * sample_rate) as usize {
        let (l, r) = v.next(dt);
        out.push(l);
        out.push(r);
    }
    out
}

#[cfg(test)]
mod tests;
