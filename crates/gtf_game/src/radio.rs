//! The car radio (PLAN.md §5.6.13): the stations of `radio.toml`, all live on the game clock, so
//! tuning in lands mid-song. D-pad left/right steps through them in a vehicle (and the `radio`
//! command does anywhere, for tests and the attract director); tuning in shows the dial card —
//! station, song and artist — for a few seconds. The host renders the song from its meta.

use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use pav_core::present::Music;
use pav_core::radio::{SongMeta, StationDef, at};
use serde::{Deserialize, Serialize};

/// `radio.toml`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RadioDef {
    pub station: Vec<StationDef>,
}

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
    let d: RadioDef = toml::from_str(&text).map_err(|e| anyhow!("content/freeway/radio.toml: {e}"))?;
    if d.station.is_empty() {
        return Err(anyhow!("content/freeway/radio.toml: no stations"));
    }
    let d = Arc::new(d);
    *cache = Some((text.into_owned(), d.clone()));
    Ok(d)
}

/// The player's radio (part of the game's state).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Radio {
    /// The station playing (an index into `radio.toml`), or none.
    pub on: Option<usize>,
    /// The station last chosen (the next car tunes to it).
    pub last: usize,
}

impl Radio {
    /// Seconds a station has been on air at game time `t` (each began at its own moment, so
    /// stations are not in step).
    pub fn clock(st: &StationDef, t: f64) -> f64 {
        t + (st.seed % 1000) as f64 * 7.3
    }

    /// What is playing at game time `t`: the station, its song and the seconds into the song's
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

    /// The music intent for the host at game time `t`.
    pub fn music(&self, def: &RadioDef, t: f64, volume: f32) -> Option<Music> {
        let (st, song, offset) = self.now(def, t)?;
        Some(Music { station: st.id.clone(), song, offset, volume, muffle: 0.0 })
    }

    /// The dial card at game time `t`: ("FREEWAY FM 104.04", "Freeway Dreams — The Tarmacs").
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
}
