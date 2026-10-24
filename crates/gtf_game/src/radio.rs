//! The car radio (PLAN.md §5.6.13, §5.12): the stations of `radio.toml`, all live on the
//! radio's clock, so tuning in lands mid-song. D-pad left/right steps through them in a vehicle
//! (and the `radio` command does anywhere, for tests and the attract director); tuning in shows
//! the dial card — station, song and artist — for a few seconds. The host renders the music from
//! the song's meta and the talk from its segments; this module decides what is said.
//!
//! - Music stations: between songs the station's ident, and on most breaks the DJ says a line
//!   (a babble voice on the air, the DJ card on the HUD).
//! - A talk station (`genre = "talk"`, SPNN 1040 AM): a program laid out per slot from the
//!   station's seed (its `[talk.<id>]` rundown) — the sounder, the anchor's headlines, ads over
//!   jingles, call-ins, the weather, and traffic reports that say where the player's police
//!   pursuit is. Reports are worked out as they go on air and kept, so a line never changes
//!   halfway.
//! - The police scanner at 2★ and up (`[scanner]`): a code, a unit and the player's street or
//!   zone ([`Places`]).
//!
//! Every pick comes from a station's seed and the slot (or the count of scanner lines), never
//! the sim's random stream, so tuning in changes nothing else in the game.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use glam::{IVec2, Vec2, Vec3};
use pav_core::present::Music;
use pav_core::radio::{Segment, SegmentKind, SongMeta, StationDef, Voice, at, mix, speech_seconds};
use serde::{Deserialize, Serialize};

/// `radio.toml`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RadioDef {
    pub station: Vec<StationDef>,
    /// The DJs between songs, by station id.
    #[serde(default)]
    pub dj: BTreeMap<String, Dj>,
    /// Talk stations' programs, by station id.
    #[serde(default)]
    pub talk: BTreeMap<String, Talk>,
    #[serde(default)]
    pub scanner: Scanner,
    #[serde(default)]
    pub places: Places,
}

/// A DJ between songs.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dj {
    pub voice: Voice,
    /// What they say: `{title}` and `{artist}` name the song just played, `{station}` the
    /// station.
    pub lines: Vec<String>,
    /// Breaks in a hundred they talk in.
    #[serde(default = "most")]
    pub talks: u32,
}

fn most() -> u32 {
    70
}

/// A talk station's program.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Talk {
    /// Who is on air by role: `anchor` reads everything that has no voice of its own (`ads`,
    /// `traffic`, `caller`).
    pub voices: BTreeMap<String, Speaker>,
    /// Each slot's running order: `sounder`, `intro`, `headline`, `ad`, `traffic`, `weather`,
    /// `call`, `signoff`.
    pub rundown: Vec<String>,
    pub intro: Vec<String>,
    pub headlines: Vec<String>,
    pub ads: Vec<Ad>,
    #[serde(default)]
    pub calls: Vec<Call>,
    #[serde(default)]
    pub traffic: Traffic,
    /// Lines by weather (`clear`, `hazy`, `overcast`, `rain`, `storm`, `fog`).
    #[serde(default)]
    pub weather: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub signoff: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Speaker {
    pub name: String,
    pub voice: Voice,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ad {
    pub name: String,
    pub lines: Vec<String>,
}

/// A call-in: the host asks, the caller answers, the host replies.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Call {
    pub host: String,
    pub caller: String,
    pub reply: String,
}

/// Traffic reports by the player's heat: `{at}` is a place, `{stars}` the level in words.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Traffic {
    /// Nobody wanted: ordinary traffic somewhere.
    pub quiet: Vec<String>,
    /// Wanted and in sight: where the player is.
    pub pursuit: Vec<String>,
    /// Wanted and searched for: where the search is.
    pub search: Vec<String>,
    /// Four stars and up.
    pub manhunt: Vec<String>,
}

/// The police scanner.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Scanner {
    /// Who the subtitles say is talking ("SPPD DISPATCH").
    pub name: String,
    /// The dispatcher's voice (1 = 120 Hz) and how lively.
    pub pitch: f32,
    pub energy: f32,
    pub codes: Vec<String>,
    pub units: Vec<String>,
    /// What dispatch says: `{code}`, `{unit}`, `{at}` (where the police last saw the player).
    pub lines: Vec<String>,
    /// From three stars, now and then the air unit: the same words.
    pub air: Vec<String>,
}

/// Names for where things happen, as phrases a report can use ("near the Pretzel").
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Places {
    /// By the district's zone (`commercial = "on Sunset Row"`).
    pub zones: BTreeMap<String, String>,
    /// By landmark key, for landmarks that read better otherwise than "near <name>".
    pub landmarks: BTreeMap<String, String>,
    /// Named stretches of the map, checked first.
    pub area: Vec<Area>,
    /// How near a landmark counts as near it (m).
    pub reach: f32,
    /// Up on the freeway.
    pub freeway: String,
    /// Anywhere else.
    pub elsewhere: String,
}

/// A named stretch of the map: its phrase and its map cells, inclusive ([[x0, z0], [x1, z1]]).
/// A `wide` one (a whole street) gives way to the landmarks in it.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Area {
    pub at: String,
    pub cells: [[i32; 2]; 2],
    #[serde(default)]
    pub wide: bool,
}

/// The rundown's blocks.
const BLOCKS: [&str; 8] = ["sounder", "intro", "headline", "ad", "traffic", "weather", "call", "signoff"];

/// The game's stations (cached; re-read when the file changes during native hot reload).
pub fn def() -> Result<Arc<RadioDef>> {
    static CACHE: Mutex<Option<(String, Arc<RadioDef>)>> = Mutex::new(None);
    let text = crate::content::text("radio.toml", include_str!("../../../content/freeway/radio.toml"));
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((t, d)) = cache.as_ref() {
        if *t == *text {
            return Ok(d.clone());
        }
    }
    let d = parse(&text).map_err(|e| anyhow!("content/freeway/radio.toml: {e}"))?;
    let d = Arc::new(d);
    *cache = Some((text.into_owned(), d.clone()));
    Ok(d)
}

/// Reads and checks a radio file.
pub fn parse(text: &str) -> Result<RadioDef> {
    let d: RadioDef = toml::from_str(text)?;
    if d.station.is_empty() {
        return Err(anyhow!("no stations"));
    }
    let known = |id: &str| d.station.iter().any(|s| s.id == id);
    for id in d.dj.keys() {
        if !known(id) {
            return Err(anyhow!("[dj.{id}]: no station '{id}'"));
        }
    }
    for (id, t) in &d.talk {
        if !known(id) {
            return Err(anyhow!("[talk.{id}]: no station '{id}'"));
        }
        if !t.voices.contains_key("anchor") {
            return Err(anyhow!("[talk.{id}]: no anchor among the voices"));
        }
        if let Some(b) = t.rundown.iter().find(|b| !BLOCKS.contains(&b.as_str())) {
            return Err(anyhow!("[talk.{id}] rundown: '{b}' is not one of {}", BLOCKS.join(", ")));
        }
        let pools = [
            ("intro", t.intro.len()),
            ("headline", t.headlines.len()),
            ("ad", t.ads.len()),
            ("call", t.calls.len()),
            ("signoff", t.signoff.len()),
        ];
        for (block, n) in pools {
            if n == 0 && t.rundown.iter().any(|b| b == block) {
                return Err(anyhow!("[talk.{id}] rundown has '{block}' but there are none"));
            }
        }
    }
    Ok(d)
}

/// What a stretch of talk says.
#[derive(Clone, Debug, PartialEq)]
pub enum Words {
    /// Its words, fixed (a headline, an ad, the DJ's line).
    Said(String),
    /// Worked out as it goes on air: the traffic (from the heat), the weather.
    Traffic,
    Weather,
    /// A sounder or a jingle.
    Nothing,
}

/// One stretch of a slot: its sound for the host, its words for the HUD.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub seg: Segment,
    /// Who speaks (empty for sounders and jingles).
    pub speaker: String,
    pub words: Words,
}

/// Seconds a new song's card shows (the card of tuning in).
pub const NEW_SONG_CARD: f32 = 4.0;

/// Seconds kept on air for the reports whose words come as they begin.
const TRAFFIC: f32 = 9.0;
const WEATHER: f32 = 7.0;

/// Fills `{key}`s (a name ending in a full stop does not double the sentence's).
fn fill(text: &str, keys: &[(&str, &str)]) -> String {
    let mut s = text.to_string();
    for (k, v) in keys {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    let mut s = s.replace(".. ", ". ");
    if s.ends_with("..") && !s.ends_with("...") {
        s.pop();
    }
    s
}

/// The `k`th pick of a pool, all different until it runs out: a stride through it from a
/// start the hash gives.
fn nth<T>(pool: &[T], h: u64, k: usize) -> &T {
    let n = pool.len();
    let gcd = |mut a: usize, mut b: usize| {
        while b != 0 {
            (a, b) = (b, a % b);
        }
        a
    };
    let stride = [7, 5, 3, 11, 13, 1].into_iter().find(|s| gcd(*s, n) == 1).unwrap_or(1);
    &pool[((h % n as u64) as usize + k * stride) % n]
}

/// A slot of a talk station: its lines in order (jingles before the ad lines over them).
fn program(st: &StationDef, talk: &Talk, index: u64) -> Vec<Line> {
    let h0 = mix(st.seed ^ mix(index) ^ 0x7a1c);
    let slot = st.slot.max(30.0);
    let speaker = |role: &str| talk.voices.get(role).or_else(|| talk.voices.get("anchor"));
    let station = format!("{} {}", st.name, st.freq);
    let anchor = talk.voices.get("anchor").map_or(String::new(), |s| s.name.clone());
    let keys = [("station", station.as_str()), ("anchor", anchor.as_str())];
    let mut out = Vec::new();
    let mut t = 0.0f32;
    let mut used: BTreeMap<&str, usize> = BTreeMap::new();
    let voice = |out: &mut Vec<Line>, t: &mut f32, role: &str, text: String, h: u64| {
        let Some(s) = speaker(role) else { return };
        let seconds = speech_seconds(&text, s.voice.rate).clamp(1.0, 14.0);
        out.push(Line {
            seg: Segment { kind: SegmentKind::Voice, start: *t, seconds, voice: s.voice, seed: h },
            speaker: s.name.clone(),
            words: Words::Said(text),
        });
        *t += seconds + 0.7;
    };
    for (b, block) in talk.rundown.iter().enumerate() {
        if t > slot - 6.0 {
            break;
        }
        let h = mix(h0 ^ (b as u64 + 1).wrapping_mul(0x9e37_79b9_7f4a_7c15));
        let k = {
            let e = used.entry(block.as_str()).or_default();
            *e += 1;
            *e - 1
        };
        let pick = |pool: &[String], salt: u64| fill(nth(pool, mix(h0 ^ salt), k), &keys);
        match block.as_str() {
            "sounder" => {
                out.push(Line {
                    seg: Segment { kind: SegmentKind::Chime, start: t, seconds: 3.2, seed: h, ..Default::default() },
                    speaker: String::new(),
                    words: Words::Nothing,
                });
                t += 3.6;
            }
            "intro" => voice(&mut out, &mut t, "anchor", pick(&talk.intro, 1), h),
            "headline" => voice(&mut out, &mut t, "anchor", pick(&talk.headlines, 2), h),
            "signoff" => voice(&mut out, &mut t, "anchor", pick(&talk.signoff, 3), h),
            "ad" => {
                let ad = nth(&talk.ads, mix(h0 ^ 4), k);
                let Some(s) = speaker("ads") else { continue };
                let jingle = out.len();
                out.push(Line {
                    seg: Segment { kind: SegmentKind::Jingle, start: t, seconds: 0.0, seed: h, ..Default::default() },
                    speaker: String::new(),
                    words: Words::Nothing,
                });
                let mut u = t + 0.9;
                for (i, l) in ad.lines.iter().enumerate() {
                    let seconds = speech_seconds(l, s.voice.rate).clamp(1.0, 10.0);
                    out.push(Line {
                        seg: Segment { kind: SegmentKind::Voice, start: u, seconds, voice: s.voice, seed: h ^ (i as u64 + 1) },
                        speaker: s.name.clone(),
                        words: Words::Said(fill(l, &keys)),
                    });
                    u += seconds + 0.35;
                }
                // The jingle runs under the lines and ends on its chord.
                out[jingle].seg.seconds = u - t + 1.0;
                t = u + 1.6;
            }
            "traffic" | "weather" => {
                let traffic = block == "traffic";
                let Some(s) = speaker(if traffic { "traffic" } else { "anchor" }) else { continue };
                let window = if traffic { TRAFFIC } else { WEATHER };
                out.push(Line {
                    seg: Segment { kind: SegmentKind::Voice, start: t, seconds: window, voice: s.voice, seed: h },
                    speaker: s.name.clone(),
                    words: if traffic { Words::Traffic } else { Words::Weather },
                });
                t += window + 0.7;
            }
            "call" => {
                let c = nth(&talk.calls, mix(h0 ^ 5), k);
                voice(&mut out, &mut t, "anchor", fill(&c.host, &keys), h);
                voice(&mut out, &mut t, "caller", fill(&c.caller, &keys), h ^ 1);
                voice(&mut out, &mut t, "anchor", fill(&c.reply, &keys), h ^ 2);
            }
            _ => {}
        }
    }
    // Nothing runs past the slot.
    out.retain(|l| l.seg.start < slot - 1.0);
    for l in &mut out {
        l.seg.seconds = l.seg.seconds.min(slot - l.seg.start);
    }
    out
}

/// The DJ's line in the break after `song`, if they talk this time.
fn dj_line(def: &RadioDef, st: &StationDef, song: &SongMeta) -> Option<Line> {
    let dj = def.dj.get(&st.id)?;
    if dj.lines.is_empty() {
        return None;
    }
    let h = mix(st.seed ^ mix(song.index) ^ 0xd1);
    if h % 100 >= dj.talks as u64 {
        return None;
    }
    let text = fill(
        &dj.lines[((h >> 8) % dj.lines.len() as u64) as usize],
        &[("title", &song.title), ("artist", &song.artist), ("station", &st.name)],
    );
    // After the ident's motif; done before the next song.
    let start = song.seconds + 2.8;
    let room = (st.slot.max(30.0) - start - 1.0).max(1.0);
    let seconds = speech_seconds(&text, dj.voice.rate).clamp(1.0, room);
    Some(Line {
        seg: Segment { kind: SegmentKind::Voice, start, seconds, voice: dj.voice, seed: h },
        speaker: st.dj.clone(),
        words: Words::Said(text),
    })
}

/// The lines on a station's air in slot `song.index` (cached: the HUD and the host ask every
/// tick).
pub fn lines(def: &Arc<RadioDef>, si: usize, song: &SongMeta) -> Arc<Vec<Line>> {
    type Key = (usize, usize, u64);
    static CACHE: Mutex<Option<(Key, Arc<RadioDef>, Arc<Vec<Line>>)>> = Mutex::new(None);
    let key = (Arc::as_ptr(def) as usize, si, song.index);
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((k, _, l)) = cache.as_ref()
        && *k == key
    {
        return l.clone();
    }
    let Some(st) = def.station.get(si) else { return Arc::new(Vec::new()) };
    let l = Arc::new(match def.talk.get(&st.id) {
        Some(talk) if st.genre == "talk" => program(st, talk, song.index),
        _ => dj_line(def, st, song).into_iter().collect(),
    });
    *cache = Some((key, def.clone(), l.clone()));
    l
}

/// What the police and the weather are doing, for a report as it goes on air.
#[derive(Clone, Debug, Default)]
pub struct News {
    pub stars: u8,
    pub searching: bool,
    /// Where the player is, and where the police are looking ([`Places::name`]).
    pub player: String,
    pub zone: String,
    /// The weather's name (`clear`, `rain`...).
    pub weather: String,
}

/// A report's words, worked out from the news (`h` picks among the lines).
pub fn report(def: &RadioDef, station: &str, words: &Words, h: u64, news: &News) -> String {
    let Some(talk) = def.talk.get(station) else { return String::new() };
    const STARS: [&str; 6] = ["no-star", "one-star", "two-star", "three-star", "four-star", "five-star"];
    let stars = STARS[news.stars.min(5) as usize];
    let pick = |pool: &[String], at: &str| {
        if pool.is_empty() {
            return String::new();
        }
        fill(&pool[(h % pool.len() as u64) as usize], &[("at", at), ("stars", stars)])
    };
    match words {
        Words::Traffic => {
            let tr = &talk.traffic;
            match news.stars {
                0 => pick(&tr.quiet, &def.places.any(h >> 16)),
                s if s >= 4 && !tr.manhunt.is_empty() => {
                    pick(&tr.manhunt, if news.searching { &news.zone } else { &news.player })
                }
                _ if news.searching && !tr.search.is_empty() => pick(&tr.search, &news.zone),
                _ => pick(&tr.pursuit, &news.player),
            }
        }
        Words::Weather => {
            let pool = talk.weather.get(&news.weather).or_else(|| talk.weather.get("clear"));
            pool.map_or(String::new(), |p| pick(p, &news.player))
        }
        Words::Said(s) => s.clone(),
        Words::Nothing => String::new(),
    }
}

impl Places {
    /// A place's phrase for where `p` is: a named area, up on the freeway, near a landmark, a
    /// wide area, or the zone around.
    pub fn name(&self, city: &pav_world::City, p: Vec3) -> String {
        let flat = Vec2::new(p.x, p.z);
        let cell = city.grid.cell_of(flat);
        let area = |wide: bool| {
            self.area.iter().find(|a| {
                let [[x0, z0], [x1, z1]] = a.cells;
                a.wide == wide && (x0..=x1).contains(&cell.x) && (z0..=z1).contains(&cell.y)
            })
        };
        if let Some(a) = area(false) {
            return a.at.clone();
        }
        if !self.freeway.is_empty() && p.y > city.ground_y(flat) + 6.0 {
            return self.freeway.clone();
        }
        let reach = if self.reach > 0.0 { self.reach } else { 40.0 };
        let half = Vec2::splat(city.grid.cell * 0.5);
        let near = city
            .def
            .landmark
            .iter()
            .map(|l| {
                let [[x0, z0], [x1, z1]] = l.cells;
                let lo = city.cell_centre(IVec2::new(x0, z0)) - half;
                let hi = city.cell_centre(IVec2::new(x1, z1)) + half;
                (flat.clamp(lo, hi).distance(flat), l)
            })
            .filter(|(d, _)| *d < reach)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, l)) = near {
            return self.landmarks.get(&l.key).cloned().unwrap_or_else(|| format!("near {}", l.name));
        }
        if let Some(a) = area(true) {
            return a.at.clone();
        }
        // Roads have no zone: the nearest zoned cell around.
        for r in 0..4i32 {
            for dz in -r..=r {
                for dx in -r..=r {
                    if dx.abs().max(dz.abs()) != r {
                        continue;
                    }
                    if let Some(z) = city.grid.zone(cell + IVec2::new(dx, dz))
                        && let Some(ph) = self.zones.get(z)
                    {
                        return ph.clone();
                    }
                }
            }
        }
        self.elsewhere.clone()
    }

    /// Somewhere (`h` picks): a named area or a zone's phrase.
    pub fn any(&self, h: u64) -> String {
        let all: Vec<&String> = self.area.iter().map(|a| &a.at).chain(self.zones.values()).collect();
        if all.is_empty() { self.elsewhere.clone() } else { all[(h % all.len() as u64) as usize].clone() }
    }
}

/// A line of police scanner chatter on air.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Chatter {
    pub text: String,
    /// Seconds since the squelch, and how long until the closing one.
    pub age: f32,
    pub seconds: f32,
}

/// Seconds of squelch and beep before the dispatcher talks.
pub const CHATTER_LEAD: f32 = 0.25;
/// The scanner voice's syllables a second (`pav_audio`'s voice loops).
const CHATTER_RATE: f32 = 5.2;

/// The scanner's `n`th line at `stars`, about where the police last saw the player (`at`).
pub fn chatter(def: &RadioDef, n: u32, stars: u8, at: &str) -> Option<Chatter> {
    let s = &def.scanner;
    if s.lines.is_empty() {
        return None;
    }
    let h = mix(0x5ca7 ^ (n as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ stars as u64);
    let pool = if stars >= 3 && !s.air.is_empty() && h.is_multiple_of(3) { &s.air } else { &s.lines };
    let pick = |p: &[String], k: u64| p.get((k % p.len().max(1) as u64) as usize).cloned().unwrap_or_default();
    let text = fill(&pick(pool, h >> 8), &[("code", &pick(&s.codes, h >> 24)), ("unit", &pick(&s.units, h >> 40)), ("at", at)]);
    let seconds = CHATTER_LEAD + speech_seconds(&text, CHATTER_RATE) + 0.15;
    Some(Chatter { text, age: 0.0, seconds })
}

/// A report said on air: which station, slot and line, and its words.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Said {
    pub station: usize,
    pub slot: u64,
    pub line: usize,
    pub text: String,
}

/// What is being said on the radio now.
#[derive(Clone, Debug, PartialEq)]
pub struct OnAir {
    pub speaker: String,
    pub text: String,
    /// Seconds since the line began, and its length.
    pub since: f32,
    pub seconds: f32,
    /// A DJ between songs (else a talk station's line).
    pub dj: bool,
}

/// The player's radio (part of the game's state).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Radio {
    /// The station playing (an index into `radio.toml`), or none.
    pub on: Option<usize>,
    /// The station last chosen (the next car tunes to it).
    pub last: usize,
    /// Seconds the stations have been on air: the sim's real time, so slow motion and the pause
    /// menu neither stretch nor stop them (the host's song never has to jump back).
    pub clock: f64,
    /// Reports said lately (the newest last).
    pub said: Vec<Said>,
}

impl Radio {
    /// Seconds a station has been on air at radio time `t` (each began at its own moment, so
    /// stations are not in step).
    pub fn clock(st: &StationDef, t: f64) -> f64 {
        t + (st.seed % 1000) as f64 * 7.3
    }

    /// What is playing at radio time `t`: the station, its song and the seconds into the song's
    /// slot.
    pub fn now<'a>(&self, def: &'a RadioDef, t: f64) -> Option<(&'a StationDef, SongMeta, f32)> {
        let st = def.station.get(self.on?)?;
        let (song, offset) = at(st, Self::clock(st, t));
        Some((st, song, offset))
    }

    /// Tunes to a station (by index), or off.
    pub fn tune(&mut self, station: Option<usize>) {
        self.on = station;
        if let Some(i) = station {
            self.last = i;
        }
    }

    /// The next (`dir` 1) or previous (-1) station, wrapping round; from off, the last one.
    pub fn step(&mut self, def: &RadioDef, dir: i32) {
        let n = def.station.len() as i32;
        let i = match self.on {
            Some(i) => (i as i32 + dir).rem_euclid(n) as usize,
            None => self.last.min(n as usize - 1),
        };
        self.tune(Some(i));
    }

    /// The radio command: `radio <station id>`, `radio off`, `radio next`, `radio prev`.
    pub fn command(&mut self, def: &RadioDef, arg: &str) -> Result<()> {
        match arg {
            "off" => self.tune(None),
            "next" => self.step(def, 1),
            "prev" => self.step(def, -1),
            id => match def.station.iter().position(|s| s.id == id) {
                Some(i) => self.tune(Some(i)),
                None => {
                    let ids: Vec<&str> = def.station.iter().map(|s| s.id.as_str()).collect();
                    return Err(anyhow!("radio: no station '{id}' (stations: {}, off, next, prev)", ids.join(", ")));
                }
            },
        }
        Ok(())
    }

    /// `radio seek <seconds>|end` (tools and checks): the tuned station jumps to that many
    /// seconds into its current slot, or to just before its song ends (every station moves
    /// with it: they share the clock).
    pub fn seek(&mut self, def: &RadioDef, to: &str) -> Result<()> {
        let Some((st, song, offset)) = self.now(def, self.clock) else { return Err(anyhow!("radio seek: the radio is off")) };
        let s = match to {
            "end" => song.seconds - 1.0,
            x => x.parse::<f32>().map_err(|_| anyhow!("radio seek: '{x}' is not seconds or 'end'"))?,
        };
        let slot = st.slot.max(30.0);
        self.clock += (s.clamp(0.0, slot - 0.01) - offset) as f64;
        Ok(())
    }

    /// The words of line `i` of a slot: fixed, or as said when the report went on air.
    fn words(&self, si: usize, slot: u64, i: usize, l: &Line) -> Option<String> {
        match &l.words {
            Words::Said(s) => Some(s.clone()),
            Words::Traffic | Words::Weather => {
                self.said.iter().rev().find(|s| s.station == si && s.slot == slot && s.line == i).map(|s| s.text.clone())
            }
            Words::Nothing => None,
        }
    }

    /// A line's seconds on air: a report's follow its words once said.
    fn span(&self, si: usize, slot: u64, i: usize, l: &Line) -> f32 {
        match (&l.words, self.words(si, slot, i, l)) {
            (Words::Traffic | Words::Weather, Some(text)) => speech_seconds(&text, l.seg.voice.rate).max(1.0).min(l.seg.seconds),
            _ => l.seg.seconds,
        }
    }

    /// The music intent for the host at radio time `t`: the song, and the talk on its slot's
    /// air.
    pub fn music(&self, def: &Arc<RadioDef>, t: f64, volume: f32) -> Option<Music> {
        let (st, song, offset) = self.now(def, t)?;
        let si = self.on?;
        let lines = lines(def, si, &song);
        let segments =
            lines.iter().enumerate().map(|(i, l)| Segment { seconds: self.span(si, song.index, i, l), ..l.seg }).collect();
        Some(Music { station: st.id.clone(), song, offset, volume, muffle: 0.0, segments })
    }

    /// What is being said at radio time `t`, if anything.
    pub fn on_air(&self, def: &Arc<RadioDef>, t: f64) -> Option<OnAir> {
        let (st, song, offset) = self.now(def, t)?;
        let si = self.on?;
        let lines = lines(def, si, &song);
        let (i, l, seconds) = lines.iter().enumerate().find_map(|(i, l)| {
            let seconds = self.span(si, song.index, i, l);
            (l.seg.kind == SegmentKind::Voice && (l.seg.start..l.seg.start + seconds).contains(&offset))
                .then_some((i, l, seconds))
        })?;
        let text = self.words(si, song.index, i, l)?;
        Some(OnAir { speaker: l.speaker.clone(), text, since: offset - l.seg.start, seconds, dj: st.genre != "talk" })
    }

    /// A report on air at radio time `t` whose words are not worked out yet: (station, slot,
    /// line, what kind, a hash to pick its words with).
    pub fn pending(&self, def: &Arc<RadioDef>, t: f64) -> Option<(Said, Words, u64)> {
        let (st, song, offset) = self.now(def, t)?;
        let si = self.on?;
        let lines = lines(def, si, &song);
        let (i, l) = lines.iter().enumerate().find(|(_, l)| {
            matches!(l.words, Words::Traffic | Words::Weather) && (l.seg.start..l.seg.start + l.seg.seconds).contains(&offset)
        })?;
        if self.words(si, song.index, i, l).is_some() {
            return None;
        }
        let key = Said { station: si, slot: song.index, line: i, text: String::new() };
        Some((key, l.words.clone(), mix(st.seed ^ mix(song.index) ^ i as u64)))
    }

    /// Keeps a report's words (the last few).
    pub fn say(&mut self, said: Said) {
        self.said.push(said);
        if self.said.len() > 6 {
            self.said.remove(0);
        }
    }

    /// The dial card at radio time `t`: ("FREEWAY FM 104.04", "Freeway Dreams — The Tarmacs").
    pub fn card(&self, def: &RadioDef, t: f64) -> Option<(String, String)> {
        let (st, song, _) = self.now(def, t)?;
        let head = format!("{} {}", st.name.to_uppercase(), st.freq).trim().to_string();
        let line = if st.genre == "talk" {
            format!("{} with {}", song.title, st.dj)
        } else {
            format!("{} — {}", song.title, song.artist)
        };
        Some((head, line))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stations_load_and_play_live() {
        let def = def().unwrap();
        assert!(def.station.len() >= 5, "at least four music stations and the talk station");
        let mut r = Radio::default();
        assert!(r.music(&def, 100.0, 0.7).is_none(), "off until tuned");
        r.command(&def, "freeway_fm").unwrap();
        let a = r.music(&def, 1000.0, 0.7).unwrap();
        let b = r.music(&def, 1030.0, 0.7).unwrap();
        // Live: thirty seconds later the same song (or the next) has moved on thirty seconds.
        if a.song.index == b.song.index {
            assert!((b.offset - a.offset - 30.0).abs() < 1e-3);
        }
        // Away and back: the station kept going.
        r.command(&def, "k_surf").unwrap();
        r.command(&def, "freeway_fm").unwrap();
        assert_eq!(r.music(&def, 1030.0, 0.7).unwrap(), b);
        let (head, line) = r.card(&def, 1030.0).unwrap();
        assert_eq!(head, "FREEWAY FM 104.04");
        assert!(line.contains(" — "), "{line}");
        // Stepping wraps; off remembers the last station.
        r.step(&def, -1);
        assert_eq!(r.on, Some(def.station.len() - 1));
        r.command(&def, "off").unwrap();
        r.step(&def, 1);
        assert_eq!(r.on, Some(def.station.len() - 1));
        assert!(r.command(&def, "nope").is_err());
    }

    const TALK: &str = r#"
[[station]]
id = "news"
name = "News"
freq = "1"
genre = "talk"
bpm = [90.0, 90.0]
seed = 5

[[station]]
id = "tunes"
name = "Tunes"
genre = "synthwave"
bpm = [100.0, 110.0]
seed = 6
dj = "Dee"

[dj.tunes]
voice = { pitch = 200.0, formant = 1.15, rate = 5.0, energy = 0.7 }
lines = ["That was {title} by {artist} on {station}."]
talks = 100

[talk.news]
rundown = ["sounder", "intro", "headline", "headline", "ad", "traffic", "headline", "call", "weather", "signoff"]
intro = ["This is {station}. I'm {anchor}."]
headlines = ["One.", "Two.", "Three."]
ads = [{ name = "Kale", lines = ["Eat kale.", "Now."] }]
calls = [{ host = "Caller?", caller = "Hi.", reply = "Bye." }]
signoff = ["Goodnight."]
traffic = { quiet = ["All clear {at}."], pursuit = ["Pursuit {at}, {stars}."], search = ["Search {at}."], manhunt = ["Manhunt {at}."] }
weather = { clear = ["Sunny."], rain = ["Wet."] }

[talk.news.voices]
anchor = { name = "Ann", voice = { pitch = 110.0 } }
traffic = { name = "Tia", voice = { pitch = 210.0, formant = 1.15 } }
ads = { name = "Max", voice = { pitch = 150.0, rate = 6.0, energy = 0.9 } }
caller = { name = "Caller", voice = { pitch = 140.0, radio = 0.7 } }

[places]
zones = { downtown = "downtown" }
elsewhere = "in town"
"#;

    #[test]
    fn a_talk_slot_runs_its_rundown_in_order_and_on_time() {
        let def = Arc::new(parse(TALK).unwrap());
        let st = &def.station[0];
        let p = program(st, &def.talk["news"], 3);
        assert_eq!(p, program(st, &def.talk["news"], 3), "the same slot, the same program");
        assert_ne!(p, program(st, &def.talk["news"], 4), "another slot, another order");
        assert_eq!(p[0].seg.kind, SegmentKind::Chime);
        let said: Vec<&str> =
            p.iter().filter_map(|l| if let Words::Said(s) = &l.words { Some(s.as_str()) } else { None }).collect();
        assert_eq!(said[0], "This is News 1. I'm Ann.");
        // Three headlines, all different.
        let heads: Vec<&&str> = said.iter().filter(|s| ["One.", "Two.", "Three."].contains(s)).collect();
        assert_eq!(heads.len(), 3);
        assert!(heads[0] != heads[1] && heads[1] != heads[2] && heads[0] != heads[2], "{heads:?}");
        // The ad: a jingle under both of its lines.
        let j = p.iter().find(|l| l.seg.kind == SegmentKind::Jingle).unwrap();
        let ad: Vec<&Line> = p.iter().filter(|l| l.speaker == "Max").collect();
        assert_eq!(ad.len(), 2);
        assert!(ad.iter().all(|l| l.seg.start >= j.seg.start && l.seg.start + l.seg.seconds <= j.seg.start + j.seg.seconds));
        // Voices never overlap except over a jingle, and everything fits the slot.
        let voices: Vec<&Line> = p.iter().filter(|l| l.seg.kind == SegmentKind::Voice).collect();
        for w in voices.windows(2) {
            assert!(w[0].seg.start + w[0].seg.seconds <= w[1].seg.start, "{:?} then {:?}", w[0], w[1]);
        }
        assert!(p.iter().all(|l| l.seg.start + l.seg.seconds <= st.slot.max(30.0)));
        assert!(p.iter().any(|l| l.words == Words::Traffic && l.speaker == "Tia"));
        assert!(p.iter().any(|l| l.speaker == "Caller" && l.seg.voice.radio > 0.5), "the caller is on the phone");
        // A bad rundown is refused, naming the block.
        let bad = TALK.replace("\"signoff\"]", "\"gossip\"]");
        assert!(parse(&bad).unwrap_err().to_string().contains("gossip"));
    }

    #[test]
    fn traffic_reports_say_where_the_pursuit_is() {
        let def = parse(TALK).unwrap();
        let quiet = News { stars: 0, ..Default::default() };
        assert_eq!(report(&def, "news", &Words::Traffic, 0, &quiet), "All clear downtown.");
        let chase = News { stars: 3, player: "near the Pretzel".into(), zone: "in the Flats".into(), ..Default::default() };
        assert_eq!(report(&def, "news", &Words::Traffic, 0, &chase), "Pursuit near the Pretzel, three-star.");
        let search = News { searching: true, ..chase.clone() };
        assert_eq!(report(&def, "news", &Words::Traffic, 0, &search), "Search in the Flats.");
        let manhunt = News { stars: 5, ..chase };
        assert_eq!(report(&def, "news", &Words::Traffic, 0, &manhunt), "Manhunt near the Pretzel.");
        let rain = News { weather: "rain".into(), ..Default::default() };
        assert_eq!(report(&def, "news", &Words::Weather, 0, &rain), "Wet.");
        assert_eq!(report(&def, "news", &Words::Weather, 0, &News { weather: "fog".into(), ..Default::default() }), "Sunny.");
    }

    #[test]
    fn a_report_is_worked_out_once_and_kept() {
        let def = Arc::new(parse(TALK).unwrap());
        let mut r = Radio::default();
        r.command(&def, "news").unwrap();
        // Find a moment the traffic is on air.
        let t = (0..4200)
            .map(|k| k as f64 * 0.1)
            .find(|t| r.pending(&def, *t).is_some_and(|p| p.1 == Words::Traffic))
            .expect("a traffic report");
        let (key, words, h) = r.pending(&def, t).unwrap();
        assert!(r.on_air(&def, t).is_none(), "no words until it is worked out");
        let text = report(&def, "news", &words, h, &News { stars: 2, player: "on Sunset Row".into(), ..Default::default() });
        r.say(Said { text: text.clone(), ..key });
        assert!(r.pending(&def, t).is_none());
        let on = r.on_air(&def, t + 0.5).unwrap();
        assert_eq!((on.text.as_str(), on.speaker.as_str(), on.dj), (text.as_str(), "Tia", false));
        // The host hears it as long as its words.
        let m = r.music(&def, t, 0.5).unwrap();
        let seg = m.segments.iter().find(|s| s.voice.pitch == 210.0).unwrap();
        assert!((seg.seconds - speech_seconds(&text, seg.voice.rate)).abs() < 1e-4);
    }

    #[test]
    fn the_dj_talks_between_songs() {
        let def = Arc::new(parse(TALK).unwrap());
        let mut r = Radio::default();
        r.command(&def, "tunes").unwrap();
        let st = &def.station[1];
        let (song, _) = at(st, 0.0);
        let start = Radio::clock(st, 0.0);
        // During the song: nothing said; in the break: the DJ's line, naming the song.
        let mid = song.seconds as f64 * 0.5 - start;
        assert!(r.on_air(&def, mid).is_none());
        let brk = song.seconds as f64 + 3.0 - start;
        let on = r.on_air(&def, brk).unwrap();
        assert!(on.dj && on.speaker == "Dee", "{on:?}");
        assert_eq!(on.text, format!("That was {} by {} on Tunes.", song.title, song.artist));
        let m = r.music(&def, brk, 0.5).unwrap();
        assert_eq!(m.segments.len(), 1);
        assert_eq!(m.segments[0].voice.pitch, 200.0);
        // `radio seek`: into the slot, or to the song's end.
        r.clock = 100.0;
        r.seek(&def, "60").unwrap();
        assert!((r.now(&def, r.clock).unwrap().2 - 60.0).abs() < 1e-3);
        r.seek(&def, "end").unwrap();
        let (_, s, off) = r.now(&def, r.clock).unwrap();
        assert!((off - (s.seconds - 1.0)).abs() < 1e-3);
        assert!(r.seek(&def, "soon").is_err());
    }

    #[test]
    fn scanner_lines_name_a_code_a_unit_and_the_place() {
        let def = parse(
            r#"
[[station]]
id = "a"
name = "A"
genre = "synthwave"
bpm = [100.0, 100.0]
seed = 1

[scanner]
codes = ["10-80"]
units = ["Paloma-7"]
lines = ["{unit}, {code} {at}."]
air = ["Air-1 over {at}."]
"#,
        )
        .unwrap();
        let c = chatter(&def, 0, 2, "on Sunset Row").unwrap();
        assert_eq!(c.text, "Paloma-7, 10-80 on Sunset Row.");
        // A place ending in a full stop ends the sentence once.
        assert_eq!(chatter(&def, 0, 2, "outside Bang Bang Bros.").unwrap().text, "Paloma-7, 10-80 outside Bang Bang Bros.");
        assert!(c.seconds > CHATTER_LEAD + 1.0);
        // At three stars and up, the air unit sometimes.
        assert!((0..30).any(|n| chatter(&def, n, 3, "downtown").unwrap().text.starts_with("Air-1")));
        assert!((0..30).all(|n| !chatter(&def, n, 2, "downtown").unwrap().text.starts_with("Air-1")));
    }
}
