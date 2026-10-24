//! Radio stations (PLAN.md §6.5 E8): a station is data, and its songs come from its seed, one
//! per fixed slot of air time from the station's start — the song, then an ident or a DJ break
//! until the next slot. Stations are "live": the sim keeps one clock (game time), so tuning in
//! lands mid-song and switching away and back resumes where the station has got to, as in the
//! source. Pure and shared: the sim picks what plays, the host renders it from the same meta.
//!
//! Talk is data too: a game lays [`Segment`]s on a slot (a talk station's news, ads and
//! reports; a DJ between songs) and shows their words itself; the host renders their sound —
//! a babble [`Voice`], the station's sounder, an ad's jingle. [`speech_seconds`] says how long a
//! line takes to say.

use serde::{Deserialize, Serialize};

/// A babble voice: speech without words, one speaker's pitch, timbre and pace. The host's voice
/// synthesizer (formants, syllables, a pitch contour) renders it; a line lasts as long as its
/// text takes to say ([`speech_seconds`]).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Voice {
    /// The fundamental (Hz): about 95-130 for a low voice, 180-240 for a high one.
    pub pitch: f32,
    /// Formant scale, the size of the vocal tract: 1 for a man's, about 1.15 for a woman's.
    pub formant: f32,
    /// Syllables a second (about 4-6; a fast ad read 6 or more).
    pub rate: f32,
    /// 0 flat .. 1 lively: how far the pitch swings and how hard the stresses land.
    pub energy: f32,
    /// 0 clean .. 1 through a small speaker or a two-way radio (band-limited, driven, hiss).
    pub radio: f32,
}

impl Default for Voice {
    fn default() -> Self {
        Voice { pitch: 120.0, formant: 1.0, rate: 4.6, energy: 0.4, radio: 0.0 }
    }
}

/// What a stretch of a station's air plays.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentKind {
    /// Someone talking (`voice`).
    #[default]
    Voice,
    /// The station's sounder: the chimes that open the news.
    Chime,
    /// A short music bed in the station's key (an ad's jingle; voices talk over it).
    Jingle,
}

/// A stretch of talk on a station's air, `start` seconds into a slot for `seconds`: the news,
/// an ad, a traffic report, the DJ between songs. The game decides what is said and shows it;
/// the host renders the sound.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Segment {
    pub kind: SegmentKind,
    pub start: f32,
    pub seconds: f32,
    /// Who talks (`Voice` segments).
    pub voice: Voice,
    /// What varies its sound: the babble's syllables, the jingle's tune.
    pub seed: u64,
}

/// About how long `text` takes to say at `rate` syllables a second: its syllables (the vowel
/// groups of each word, a digit apiece), a breath at each comma and a pause at each full stop.
pub fn speech_seconds(text: &str, rate: f32) -> f32 {
    let mut syllables = 0usize;
    let mut pauses = 0.0f32;
    for word in text.split_whitespace() {
        let w = word.to_lowercase();
        if !w.chars().any(char::is_alphanumeric) {
            continue;
        }
        let (mut n, mut vowel) = (0usize, false);
        for c in w.chars() {
            let v = "aeiouyáéíóúü".contains(c);
            if v && !vowel {
                n += 1;
            }
            vowel = v;
        }
        // A silent final e ("make"), not "-le" ("table").
        let letters: String = w.chars().filter(|c| c.is_alphabetic()).collect();
        if letters.ends_with('e') && !letters.ends_with("le") && n > 1 {
            n -= 1;
        }
        n += w.chars().filter(char::is_ascii_digit).count();
        syllables += n.max(1);
        if word.ends_with([',', ';', ':']) {
            pauses += 0.25;
        } else if word.ends_with(['.', '!', '?']) {
            pauses += 0.45;
        }
    }
    syllables as f32 / rate.max(1.0) + pauses
}

/// A station as a game's radio data describes it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StationDef {
    /// The id the game uses (`freeway_fm`).
    pub id: String,
    /// What the dial card shows ("Freeway FM").
    pub name: String,
    /// The frequency ("104.04").
    #[serde(default)]
    pub freq: String,
    /// The music renderer's genre (`synthwave`, `boombap`, `surf`, `cumbia`, `smooth`, `talk`).
    pub genre: String,
    /// Tempo range (beats per minute).
    pub bpm: [f32; 2],
    pub seed: u64,
    /// Seconds of air per song slot (the song, then the ident).
    #[serde(default = "slot")]
    pub slot: f32,
    /// Words song titles and artist names are made of.
    #[serde(default)]
    pub title_words: Vec<String>,
    #[serde(default)]
    pub artist_words: Vec<String>,
    /// The DJ's name for the dial card.
    #[serde(default)]
    pub dj: String,
}

fn slot() -> f32 {
    210.0
}

/// One song: everything the host needs to render it and the HUD needs to name it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SongMeta {
    pub seed: u64,
    /// Its slot on the station (0 from the station's start).
    pub index: u64,
    pub title: String,
    pub artist: String,
    pub genre: String,
    pub bpm: f32,
    /// Root note (0 = C … 11 = B) and mode.
    pub key: u8,
    pub minor: bool,
    /// Whole bars, and their length in seconds (the rest of the slot is the ident).
    pub bars: u32,
    pub seconds: f32,
    /// The station's seed: its ident jingle and sounder are its own, the same every break.
    #[serde(default)]
    pub station: u64,
}

/// A small deterministic hash (splitmix64) for picking a song's parts (and a game's picks from a
/// station's seed and slot).
pub fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

fn pick<'a>(words: &'a [String], h: u64, fallback: &'a str) -> &'a str {
    if words.is_empty() { fallback } else { &words[(h % words.len() as u64) as usize] }
}

/// The song in slot `index` of a station.
pub fn song(st: &StationDef, index: u64) -> SongMeta {
    let seed = mix(st.seed ^ mix(index));
    let h = |k: u64| mix(seed ^ k);
    let unit = |k: u64| (h(k) >> 11) as f32 / (1u64 << 53) as f32;
    let bpm = (st.bpm[0] + (st.bpm[1] - st.bpm[0]).max(0.0) * unit(1)).round().max(40.0);
    let bar = 240.0 / bpm;
    // Leave at least 12 s of the slot for the ident; songs run 8 to the slot's bars, in fours.
    let room = ((st.slot - 12.0).max(bar * 8.0) / bar).floor() as u32;
    let bars = (room - (h(2) % (room / 4).max(1) as u64) as u32).max(8) / 4 * 4;
    let title = match h(3) % 3 {
        0 => pick(&st.title_words, h(4), "Night").to_string(),
        1 => format!("{} {}", pick(&st.title_words, h(4), "Night"), pick(&st.title_words, h(5), "Drive")),
        _ => format!("The {} {}", pick(&st.title_words, h(5), "Last"), pick(&st.title_words, h(6), "Exit")),
    };
    let artist = match h(7) % 3 {
        0 => format!("The {}s", pick(&st.artist_words, h(8), "Tarmac")),
        1 => format!("{} {}", pick(&st.artist_words, h(8), "Neon"), pick(&st.artist_words, h(9), "Coast")),
        _ => pick(&st.artist_words, h(9), "Coast").to_string(),
    };
    SongMeta {
        seed,
        index,
        title,
        artist,
        genre: st.genre.clone(),
        bpm,
        key: (h(10) % 12) as u8,
        minor: h(11) % 3 != 0,
        bars,
        seconds: bars as f32 * bar,
        station: st.seed,
    }
}

/// What a station is playing `t` seconds after it began: the song and the seconds into its slot
/// (past `seconds`: the ident between songs).
pub fn at(st: &StationDef, t: f64) -> (SongMeta, f32) {
    let slot = st.slot.max(30.0) as f64;
    let t = t.max(0.0);
    let index = (t / slot).floor() as u64;
    (song(st, index), (t - index as f64 * slot) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn station() -> StationDef {
        StationDef {
            id: "freeway_fm".into(),
            name: "Freeway FM".into(),
            freq: "104.04".into(),
            genre: "synthwave".into(),
            bpm: [95.0, 118.0],
            seed: 7,
            slot: 210.0,
            title_words: ["Freeway", "Dreams", "Neon", "Sunset", "Overdrive"].map(String::from).to_vec(),
            artist_words: ["Tarmac", "Chrome", "Palm", "Vector"].map(String::from).to_vec(),
            dj: "Tarmac Tina".into(),
        }
    }

    #[test]
    fn songs_are_the_same_every_time_and_fit_their_slots() {
        let st = station();
        for i in 0..200 {
            let a = song(&st, i);
            assert_eq!(a, song(&st, i));
            assert!((95.0..=118.0).contains(&a.bpm), "{a:?}");
            assert!(a.bars >= 8 && a.bars.is_multiple_of(4), "{a:?}");
            assert!(a.seconds <= st.slot - 11.9 && a.seconds > 30.0, "{a:?}");
            assert!(!a.title.is_empty() && !a.artist.is_empty());
        }
        assert_ne!(song(&st, 0).seed, song(&st, 1).seed);
    }

    #[test]
    fn the_station_is_live() {
        let st = station();
        // Tuning in 100 s into slot 3 lands 100 s into its song; coming back 50 s later, 150 s.
        let (s, off) = at(&st, 3.0 * 210.0 + 100.0);
        assert_eq!((s.index, off), (3, 100.0));
        let (again, later) = at(&st, 3.0 * 210.0 + 150.0);
        assert_eq!((again.seed, later), (s.seed, 150.0));
        // Hours in, still a slot lookup.
        assert_eq!(at(&st, 36_000.0).0.index, 171);
        assert_eq!(s.station, st.seed, "the song knows its station (its ident)");
    }

    #[test]
    fn a_line_takes_as_long_as_its_syllables() {
        // "Route 404 widened to 47 lanes; commute unchanged.": 12 syllables by vowel groups and
        // five digits, a breath and a full stop.
        let t = speech_seconds("Route 404 widened to 47 lanes; commute unchanged.", 5.0);
        assert!((t - (17.0 / 5.0 + 0.25 + 0.45)).abs() < 1e-4, "{t}");
        assert!(speech_seconds("Hi.", 5.0) < speech_seconds("Hello there, how are you today?", 5.0));
        assert_eq!(speech_seconds(" — ", 5.0), 0.0);
        // Faster speakers take less time.
        assert!(speech_seconds("A fast ad read", 7.0) < speech_seconds("A fast ad read", 4.0));
    }
}
