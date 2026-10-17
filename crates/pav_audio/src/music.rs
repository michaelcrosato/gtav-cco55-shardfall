//! Radio music (PLAN.md §6.5 E8): a song from its `SongMeta`, rendered by a sample-accurate step
//! sequencer — drums, bass, an arpeggio or keys, and pad chords over a four-chord progression in
//! the song's key — from any offset, so a station resumes mid-song. Each note's sound is a
//! function of the time since it began, so a song starts anywhere without replaying the past.
//! Past the song (`seconds`) the station ident plays: a chime, then a quiet bed until the next
//! song. The genre picks the groove and the instruments; `synthwave` is the full kit, the other
//! genres vary it until their own patches land.

use pav_core::radio::SongMeta;

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
}

const MINOR: [i32; 7] = [0, 2, 3, 5, 7, 8, 10];
const MAJOR: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];
const MINOR_PROGRESSIONS: [[i32; 4]; 4] = [[0, 5, 2, 6], [0, 3, 5, 4], [0, 6, 5, 6], [0, 5, 3, 4]];
const MAJOR_PROGRESSIONS: [[i32; 4]; 4] = [[0, 4, 5, 3], [0, 5, 3, 4], [3, 0, 4, 5], [0, 3, 0, 4]];

/// How a genre plays the kit.
#[derive(Clone, Copy, Debug)]
struct Groove {
    /// Kick on every beat (four on the floor), or on 1 and the "and" of 2.
    four: bool,
    /// 0 straight, up to 0.33 for swung 16ths.
    swing: f32,
    /// The lead: an arpeggio of 16ths (true) or chord stabs on the off-beats.
    arp: bool,
    /// Square (true) or sine-ish leads.
    square: bool,
    snare_decay: f32,
    /// Scrapes on 16ths (güiro).
    scrape: bool,
}

fn groove(genre: &str) -> Groove {
    match genre {
        "boombap" => Groove { four: false, swing: 0.22, arp: false, square: false, snare_decay: 0.12, scrape: false },
        "surf" => Groove { four: true, swing: 0.0, arp: true, square: true, snare_decay: 0.1, scrape: false },
        "cumbia" => Groove { four: false, swing: 0.08, arp: false, square: true, snare_decay: 0.08, scrape: true },
        "smooth" => Groove { four: false, swing: 0.12, arp: false, square: false, snare_decay: 0.1, scrape: false },
        _ => Groove { four: true, swing: 0.0, arp: true, square: false, snare_decay: 0.3, scrape: false },
    }
}

fn freq(midi: i32) -> f32 {
    440.0 * 2f32.powf((midi - 69) as f32 / 12.0)
}

fn frac(x: f32) -> f32 {
    x - x.floor()
}

fn saw(f: f32, t: f32) -> f32 {
    2.0 * frac(f * t) - 1.0
}

fn square(f: f32, t: f32) -> f32 {
    if frac(f * t) < 0.5 { 1.0 } else { -1.0 }
}

fn sine(f: f32, t: f32) -> f32 {
    (f * t * std::f32::consts::TAU).sin()
}

/// Deterministic white noise for sample `n` (so a song sounds the same from any offset).
fn noise(n: u64) -> f32 {
    let mut x = n.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    x ^= x >> 29;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 32;
    (x >> 40) as f32 / (1u64 << 23) as f32 - 1.0
}

/// One song playing: its position and the filters' memory.
pub struct MusicVoice {
    pub station: String,
    pub song: SongMeta,
    /// Seconds into the slot.
    pub t: f64,
    gain: f32,
    pub target_gain: f32,
    muffle: f32,
    pub target_muffle: f32,
    groove: Groove,
    /// MIDI root of the key (around C3) and the chords (scale degrees) of the progression.
    root: i32,
    scale: [i32; 7],
    chords: [i32; 4],
    sample_rate: f32,
    bass_lp: f32,
    pad_lp: f32,
    out_lp: (f32, f32),
}

impl MusicVoice {
    pub fn new(target: &MusicTarget, sample_rate: f32) -> MusicVoice {
        let s = &target.song;
        let pick = (s.seed >> 17) as usize % 4;
        MusicVoice {
            station: target.station.clone(),
            song: s.clone(),
            t: target.offset.max(0.0) as f64,
            gain: 0.0,
            target_gain: target.volume,
            muffle: target.muffle,
            target_muffle: target.muffle,
            groove: groove(&s.genre),
            root: 48 + s.key as i32,
            scale: if s.minor { MINOR } else { MAJOR },
            chords: if s.minor { MINOR_PROGRESSIONS[pick] } else { MAJOR_PROGRESSIONS[pick] },
            sample_rate,
            bass_lp: 0.0,
            pad_lp: 0.0,
            out_lp: (0.0, 0.0),
        }
    }

    /// The note `step` scale degrees above the chord root of `degree`, in `octave` octaves above
    /// the key's root.
    fn note(&self, degree: i32, step: i32, octave: i32) -> i32 {
        let d = degree + step;
        self.root + self.scale[d.rem_euclid(7) as usize] + 12 * (d.div_euclid(7) + octave)
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
        let (seconds, bpm, bars) = (self.song.seconds, self.song.bpm, self.song.bars as i32);
        let t = self.t as f32;
        let n = (self.t * self.sample_rate as f64) as u64;
        if t >= seconds {
            return self.ident(t - seconds);
        }
        let g = self.groove;
        let spb = 60.0 / bpm.max(40.0);
        let beat = t / spb;
        let bar = (beat / 4.0).floor() as i32;
        let chord = self.chords[(bar.rem_euclid(4)) as usize];
        let intro = bar < 4;
        let outro = bar >= bars - 4;
        let drums = !intro && !outro;
        // Sixteenths, swung: odd 16ths come late.
        let sixteenth = spb / 4.0;
        let i16 = (t / sixteenth).floor();
        let start16 = i16 * sixteenth + if (i16 as i32) % 2 == 1 { g.swing * sixteenth } else { 0.0 };
        let since16 = (t - start16).max(0.0);
        let step = (i16 as i32).rem_euclid(16);
        let since_beat = (beat - beat.floor()) * spb;
        let mut mono = 0.0;
        // Kick: a falling sine thump.
        if drums {
            // Every beat, or on 1, the "and" of 2 and 3 (the latest kick step so far, with its tail).
            let since_kick = if g.four {
                since_beat
            } else {
                let k = [0, 6, 10].into_iter().rfind(|&k| k <= step).unwrap_or(0);
                (step - k) as f32 * sixteenth + since16
            };
            let f = 45.0 + 80.0 * (-since_kick / 0.03).exp();
            mono += (since_kick * f * std::f32::consts::TAU).sin() * (-since_kick / 0.22).exp() * 0.9;
            // Snare on beats 2 and 4: noise and a 180 Hz body.
            let b = beat.floor() as i32 % 4;
            if b == 1 || b == 3 {
                let e = (-since_beat / g.snare_decay).exp();
                mono += (noise(n) * 0.55 + sine(180.0, since_beat) * 0.35) * e * 0.6;
            }
            // Hats on the 8th off-beats.
            if step % 4 == 2 {
                mono += noise(n.wrapping_add(7)) * (-since16 / 0.035).exp() * 0.18;
            }
            if g.scrape {
                mono += noise(n.wrapping_add(13)) * (-since16 / 0.05).exp() * 0.1 * (1.0 + sine(40.0, since16));
            }
        } else if intro && step % 4 == 2 {
            mono += noise(n.wrapping_add(7)) * (-since16 / 0.035).exp() * 0.1;
        }
        // Bass: the chord's root on 8ths (root and fifth for the bouncier genres).
        if !intro {
            let eighth = (step / 2) % 2;
            let note = if g.scrape && eighth == 1 { self.note(chord, 4, -1) } else { self.note(chord, 0, -1) };
            let since8 = if step % 2 == 0 { since16 } else { since16 + sixteenth };
            let raw = saw(freq(note), since8) * (-since8 / 0.22).exp();
            self.bass_lp += (raw - self.bass_lp) * 0.08;
            mono += self.bass_lp * 0.5;
        }
        // Pad: the chord, detuned saws through a low-pass, swelling each bar.
        let since_bar = (beat - (bar * 4) as f32) * spb;
        let swell = (1.0 - (-since_bar / 0.6).exp()) * if outro { (bars as f32 - beat / 4.0).clamp(0.0, 4.0) / 4.0 } else { 1.0 };
        let mut pad = 0.0;
        for k in [0, 2, 4] {
            let f = freq(self.note(chord, k, 1));
            pad += saw(f, t) + saw(f * 1.006, t);
        }
        self.pad_lp += (pad / 6.0 - self.pad_lp) * 0.05;
        let pad = self.pad_lp * swell * 0.35;
        // Lead: a 16th arpeggio over the chord, or off-beat stabs.
        let lead = if g.arp {
            let k = [0, 2, 4, 7][(step % 4) as usize];
            let f = freq(self.note(chord, k, 2));
            let v = if g.square { square(f, since16) } else { saw(f, since16) * 0.8 };
            v * (-since16 / 0.11).exp() * 0.16 * if intro { (beat / 16.0).clamp(0.0, 1.0) } else { 1.0 }
        } else if step % 4 == 2 {
            let mut c = 0.0;
            for k in [0, 2, 4] {
                let f = freq(self.note(chord, k, 2));
                c += if g.square { square(f, since16) * 0.5 } else { sine(f, since16) + 0.3 * sine(f * 2.0, since16) };
            }
            c * (-since16 / 0.18).exp() * 0.08
        } else {
            0.0
        };
        // A little width: the lead leans right, the pad left.
        let fade_in = (t / 0.5).min(1.0);
        let fade_out = ((seconds - t) / 1.5).clamp(0.0, 1.0);
        let k = fade_in * fade_out * 0.7;
        ((mono + pad * 1.1 + lead * 0.8) * k, (mono + pad * 0.9 + lead * 1.2) * k)
    }

    /// Between songs: a three-note chime, then a quiet bed of the key's root chord.
    fn ident(&self, u: f32) -> (f32, f32) {
        let mut v = 0.0;
        for (i, k) in [0, 4, 7].into_iter().enumerate() {
            let at = i as f32 * 0.28;
            if u >= at {
                let d = u - at;
                let f = freq(self.note(0, k, 2));
                v += (sine(f, d) + 0.4 * sine(f * 2.76, d)) * (-d / 0.9).exp() * 0.18;
            }
        }
        let bed = (sine(freq(self.note(0, 0, 0)), u) + sine(freq(self.note(0, 2, 0)), u) * 0.6) * 0.03 * (u / 2.0).min(1.0);
        (v + bed, v + bed)
    }
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
mod tests {
    use super::*;
    use pav_core::radio::{StationDef, at};

    fn target(genre: &str, offset: f32) -> MusicTarget {
        let st = StationDef {
            id: "test".into(),
            name: "Test FM".into(),
            freq: "100.0".into(),
            genre: genre.into(),
            bpm: [100.0, 110.0],
            seed: 3,
            slot: 210.0,
            title_words: vec![],
            artist_words: vec![],
            dj: String::new(),
        };
        let (song, _) = at(&st, 0.0);
        MusicTarget { station: "test".into(), song, offset, volume: 1.0, muffle: 0.0 }
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
    }

    #[test]
    fn every_genre_plays_and_stays_in_range() {
        for g in ["synthwave", "boombap", "surf", "cumbia", "smooth", "talk"] {
            let out = render(&target(g, 30.0), 2.0, 22_050.0);
            let level = rms(&out);
            assert!(out.iter().all(|v| v.is_finite() && v.abs() <= 1.5), "{g}");
            assert!(level > 0.02 && level < 0.8, "{g}: rms {level}");
        }
    }

    #[test]
    fn a_song_sounds_the_same_from_any_offset() {
        // Starting at 30 s, or at 29 s and skipping one second, gives the same samples once the
        // filters settle.
        let a = render(&target("synthwave", 30.0), 0.5, 22_050.0);
        let b = render(&target("synthwave", 29.0), 1.5, 22_050.0);
        let tail = |x: &[f32]| x[x.len() - 4000..].to_vec();
        let d: f32 = tail(&a).iter().zip(tail(&b)).map(|(p, q)| (p - q).abs()).sum::<f32>() / 4000.0;
        assert!(d < 0.02, "mean difference {d}");
    }

    #[test]
    fn a_game_run_plays_its_radio_and_switches_stations() {
        let t = target("synthwave", 40.0);
        let scene = |station: &str, offset: f32| pav_core::present::AudioScene {
            music: Some(pav_core::present::Music {
                station: station.into(),
                song: t.song.clone(),
                offset,
                volume: 0.8,
                muffle: 0.0,
            }),
            master: 1.0,
            effects: 1.0,
            ..Default::default()
        };
        let dt = 1.0 / 60.0;
        let mut ticks: Vec<_> = (0..60).map(|i| (Vec::new(), Some(scene("a", 40.0 + i as f32 * dt)))).collect();
        ticks.extend((0..60).map(|i| (Vec::new(), Some(scene("b", 10.0 + i as f32 * dt)))));
        ticks.extend(
            (0..60)
                .map(|_| (Vec::new(), Some(pav_core::present::AudioScene { master: 1.0, effects: 1.0, ..Default::default() }))),
        );
        let out = crate::render_run(&ticks, dt, 0.0, 22_050.0, &crate::Listener::default());
        let third = out.len() / 3;
        assert!(rms(&out[..third]) > 0.02 && rms(&out[third..2 * third]) > 0.02, "the radio plays on both stations");
        assert!(rms(&out[out.len() - 4000..]) < 0.01, "and fades out when switched off");
    }

    #[test]
    fn the_ident_plays_between_songs() {
        let t = target("synthwave", 0.0);
        let after = t.song.seconds + 0.2;
        let chime = render(&MusicTarget { offset: after, ..t.clone() }, 0.6, 22_050.0);
        assert!(rms(&chime) > 0.01, "a chime after the song");
    }
}
