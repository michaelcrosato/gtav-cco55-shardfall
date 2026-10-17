//! The sound bank as data: named cues (`ui.accept`, `gun.pistol`) made of layered synthesized
//! patches, with random pitch and volume spread and a cap on how many play at once. The engine
//! ships a default bank (`default_bank.toml`); a game adds or overrides cues with its own file
//! (`Bank::merge`). `SimEvent::Sound { cue, .. }` plays them.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::{Patch, Wave};

/// One layer of a cue: a patch, started `delay` seconds after the cue.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LayerDef {
    wave: String,
    #[serde(default = "f440")]
    freq: f32,
    freq_end: Option<f32>,
    glide: Option<f32>,
    attack: Option<f32>,
    decay: Option<f32>,
    sustain: Option<f32>,
    hold: Option<f32>,
    release: Option<f32>,
    volume: Option<f32>,
    cutoff: Option<f32>,
    cutoff_end: Option<f32>,
    duty: Option<f32>,
    #[serde(default)]
    delay: f32,
}

fn f440() -> f32 {
    440.0
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CueDef {
    layers: Vec<LayerDef>,
    /// Random pitch spread (a fraction: 0.05 = ±5%) and volume spread.
    #[serde(default)]
    pitch: f32,
    #[serde(default)]
    volume: f32,
    /// At most this many of the cue sound at once (0 = no cap).
    #[serde(default)]
    voices: u8,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BankFile {
    #[serde(default)]
    cue: BTreeMap<String, CueDef>,
}

/// A playable cue.
#[derive(Clone, Debug)]
pub struct Cue {
    pub layers: Vec<(Patch, f32)>,
    pub pitch_spread: f32,
    pub volume_spread: f32,
    pub voices: u8,
}

#[derive(Clone, Debug, Default)]
pub struct Bank {
    pub cues: BTreeMap<String, Cue>,
}

fn wave(name: &str) -> Result<Wave, String> {
    Ok(match name {
        "sine" => Wave::Sine,
        "square" => Wave::Square,
        "saw" => Wave::Saw,
        "triangle" => Wave::Triangle,
        "noise" => Wave::Noise,
        o => return Err(format!("unknown wave '{o}' (sine, square, saw, triangle, noise)")),
    })
}

impl Bank {
    /// The engine's default cues (UI moves and accepts, impacts, gunshots, glass...).
    pub fn engine() -> Bank {
        Bank::parse(include_str!("default_bank.toml")).expect("the default sound bank parses")
    }

    /// Reads a bank file. Errors name the cue and the problem.
    pub fn parse(text: &str) -> Result<Bank, String> {
        let file: BankFile = toml::from_str(text).map_err(|e| e.to_string())?;
        let mut cues = BTreeMap::new();
        for (name, def) in file.cue {
            if def.layers.is_empty() {
                return Err(format!("cue '{name}': no layers"));
            }
            let mut layers = Vec::new();
            for l in &def.layers {
                let mut p = Patch::new(wave(&l.wave).map_err(|e| format!("cue '{name}': {e}"))?, l.freq);
                macro_rules! set {
                    ($($f:ident),*) => { $( if let Some(v) = l.$f { p.$f = v; } )* };
                }
                set!(freq_end, glide, attack, decay, sustain, hold, release, volume, cutoff, cutoff_end, duty);
                if l.freq_end.is_none() {
                    p.freq_end = p.freq;
                }
                if l.cutoff_end.is_none() {
                    p.cutoff_end = p.cutoff;
                }
                if !(p.volume.is_finite() && (0.0..=2.0).contains(&p.volume)) {
                    return Err(format!("cue '{name}': volume {} outside 0..2", p.volume));
                }
                layers.push((p, l.delay.max(0.0)));
            }
            cues.insert(
                name,
                Cue {
                    layers,
                    pitch_spread: def.pitch.clamp(0.0, 0.5),
                    volume_spread: def.volume.clamp(0.0, 1.0),
                    voices: def.voices,
                },
            );
        }
        Ok(Bank { cues })
    }

    /// Adds `other`'s cues, replacing same-named ones.
    pub fn merge(&mut self, other: Bank) {
        self.cues.extend(other.cues);
    }

    pub fn get(&self, cue: &str) -> Option<&Cue> {
        self.cues.get(cue)
    }

    /// The cue's layers with its random spread applied (`roll` is 0..1 from the caller's random
    /// stream), at `pitch` and `gain`.
    pub fn voice(&self, cue: &str, pitch: f32, gain: f32, roll: f32) -> Option<(Vec<(Patch, f32)>, u8)> {
        let c = self.get(cue)?;
        let spread = roll * 2.0 - 1.0;
        let k = pitch.max(0.01) * (1.0 + c.pitch_spread * spread);
        let g = gain * (1.0 - c.volume_spread * (1.0 - roll));
        let layers = c
            .layers
            .iter()
            .map(|(p, d)| (Patch { freq: p.freq * k, freq_end: p.freq_end * k, volume: p.volume * g, ..*p }, *d))
            .collect();
        Some((layers, c.voices))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_bank_parses_and_games_extend_it() {
        let mut bank = Bank::engine();
        for cue in ["ui.move", "ui.accept", "ui.back", "ui.error", "ui.tab"] {
            assert!(bank.get(cue).is_some(), "{cue}");
        }
        let game = Bank::parse("[cue.\"ui.accept\"]\nlayers = [{ wave = \"sine\", freq = 500, decay = 0.1 }]\n").unwrap();
        bank.merge(game);
        assert_eq!(bank.get("ui.accept").unwrap().layers.len(), 1);
        let (layers, _) = bank.voice("ui.accept", 2.0, 0.5, 0.5).unwrap();
        assert_eq!(layers[0].0.freq, 1000.0);
        assert!(Bank::parse("[cue.x]\nlayers = [{ wave = \"buzz\" }]").unwrap_err().contains("unknown wave"));
        assert!(Bank::parse("[cue.x]\nlayers = []").unwrap_err().contains("no layers"));
    }
}
