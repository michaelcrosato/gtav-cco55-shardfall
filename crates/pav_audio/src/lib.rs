//! Synthesized sound: every sound is built from oscillators, noise, envelopes, pitch sweeps
//! and a filter. The mixer runs in the audio callback (cpal) or offline (to .wav). The game
//! runs fine without a sound device.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use glam::Vec3;
use pav_core::SimEvent;

pub mod bank;
pub mod dsp;
pub mod fx;
pub mod loops;
pub mod music;
pub mod voice;

pub use bank::Bank;
pub use loops::{LoopKind, LoopTarget};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Wave {
    Sine,
    Square,
    Saw,
    Triangle,
    Noise,
}

/// One synthesized sound.
#[derive(Clone, Copy, Debug)]
pub struct Patch {
    pub wave: Wave,
    /// Start / end frequency (Hz) and the glide time between them (s, exponential).
    pub freq: f32,
    pub freq_end: f32,
    pub glide: f32,
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub hold: f32,
    pub release: f32,
    pub volume: f32,
    /// One-pole low-pass cutoff (Hz) at start and end of the sound.
    pub cutoff: f32,
    pub cutoff_end: f32,
    /// Square-wave duty cycle.
    pub duty: f32,
}

impl Patch {
    pub const fn new(wave: Wave, freq: f32) -> Self {
        Self {
            wave,
            freq,
            freq_end: freq,
            glide: 0.1,
            attack: 0.002,
            decay: 0.1,
            sustain: 0.0,
            hold: 0.0,
            release: 0.05,
            volume: 0.3,
            cutoff: 12000.0,
            cutoff_end: 12000.0,
            duty: 0.5,
        }
    }
    pub fn length(&self) -> f32 {
        self.attack + self.decay + self.hold + self.release
    }
}

struct Voice {
    p: Patch,
    t: f32,
    delay: f32,
    phase: f32,
    lp: f32,
    noise: u32,
    gain_l: f32,
    gain_r: f32,
    /// The bank cue it belongs to (0 = none) and which play of it, for per-cue voice caps.
    group: u32,
    inst: u32,
}

impl Voice {
    fn env(&self) -> f32 {
        let p = &self.p;
        let t = self.t;
        if t < p.attack {
            return t / p.attack.max(1e-5);
        }
        let t = t - p.attack;
        if t < p.decay {
            return 1.0 + (p.sustain - 1.0) * (t / p.decay.max(1e-5));
        }
        let t = t - p.decay;
        if t < p.hold {
            return p.sustain;
        }
        let t = t - p.hold;
        let start = if p.decay > 0.0 || p.hold > 0.0 { p.sustain } else { 1.0 };
        (start * (1.0 - t / p.release.max(1e-5))).max(0.0)
    }

    fn done(&self) -> bool {
        self.t > self.p.length() + 0.01
    }

    fn next(&mut self, dt: f32) -> f32 {
        if self.delay > 0.0 {
            self.delay -= dt;
            return 0.0;
        }
        let p = self.p;
        let k = (self.t / p.glide.max(1e-4)).min(1.0);
        let freq = p.freq * (p.freq_end / p.freq.max(1.0)).powf(k);
        self.phase = (self.phase + freq * dt).fract();
        let ph = self.phase;
        let raw = match p.wave {
            Wave::Sine => (ph * std::f32::consts::TAU).sin(),
            Wave::Square => {
                if ph < p.duty {
                    1.0
                } else {
                    -1.0
                }
            }
            Wave::Saw => ph * 2.0 - 1.0,
            Wave::Triangle => 1.0 - 4.0 * (ph - 0.5).abs(),
            Wave::Noise => {
                self.noise ^= self.noise << 13;
                self.noise ^= self.noise >> 17;
                self.noise ^= self.noise << 5;
                (self.noise as f32 / u32::MAX as f32) * 2.0 - 1.0
            }
        };
        let cutoff = p.cutoff * (p.cutoff_end / p.cutoff.max(1.0)).powf((self.t / p.length().max(1e-3)).min(1.0));
        let a = 1.0 - (-std::f32::consts::TAU * cutoff * dt).exp();
        self.lp += (raw - self.lp) * a;
        let s = self.lp * self.env() * p.volume;
        self.t += dt;
        s
    }
}

/// Mixes one-shot voices and loops into interleaved stereo, through the reverb send and the
/// master limiter.
pub struct Mixer {
    voices: Vec<Voice>,
    loops: Vec<loops::LoopVoice>,
    /// The radio: the song playing, and songs fading out after a switch.
    music: Option<music::MusicVoice>,
    music_out: Vec<music::MusicVoice>,
    reverb: fx::Reverb,
    /// Reverb send (0 = dry, which also skips its cost) and room size.
    pub reverb_mix: f32,
    pub room: f32,
    /// Decibels the radio and the ambience beds duck by (dialogue, menus): 0 for none.
    pub duck: f32,
    /// The ducked level now (gliding to `duck`'s).
    duck_gain: f32,
    pub sample_rate: f32,
    pub master: f32,
    /// The cues `SimEvent::Sound` plays (offline renders; the live output resolves cues itself).
    pub bank: Arc<Bank>,
    seed: u32,
    next_inst: u32,
}

const MAX_VOICES: usize = 64;
const MAX_LOOPS: usize = 24;

/// A cue name's group number for voice caps.
fn group_of(cue: &str) -> u32 {
    cue.bytes().fold(0x811c_9dc5u32, |h, b| (h ^ b as u32).wrapping_mul(0x0100_0193)) | 1
}

impl Mixer {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            voices: Vec::new(),
            loops: Vec::new(),
            music: None,
            music_out: Vec::new(),
            reverb: fx::Reverb::new(sample_rate),
            reverb_mix: 0.0,
            room: 0.5,
            duck: 0.0,
            duck_gain: 1.0,
            sample_rate,
            master: 0.8,
            bank: Arc::new(Bank::engine()),
            seed: 0x9e37_79b9,
            next_inst: 1,
        }
    }

    fn roll(&mut self) -> u32 {
        self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.seed
    }

    /// pan -1 (left) .. 1 (right); delay in seconds.
    pub fn play(&mut self, p: Patch, pan: f32, gain: f32, delay: f32) {
        self.play_grouped(p, pan, gain, delay, 0, 0);
    }

    fn play_grouped(&mut self, p: Patch, pan: f32, gain: f32, delay: f32, group: u32, inst: u32) {
        if self.voices.len() >= MAX_VOICES {
            // Drop the oldest.
            self.voices.remove(0);
        }
        let noise = self.roll() | 1;
        let pan = pan.clamp(-1.0, 1.0);
        let a = (pan + 1.0) * std::f32::consts::FRAC_PI_4;
        self.voices.push(Voice {
            p,
            t: 0.0,
            delay,
            phase: 0.0,
            lp: 0.0,
            noise,
            gain_l: a.cos() * gain,
            gain_r: a.sin() * gain,
            group,
            inst,
        });
    }

    /// Plays a resolved cue's layers as one instance; with `cap` > 0, the oldest instance of
    /// the same cue stops when that many already sound.
    pub fn play_cue(&mut self, layers: &[(Patch, f32)], pan: f32, gain: f32, group: u32, cap: u8) {
        if cap > 0 && group != 0 {
            let mut insts: Vec<u32> = self.voices.iter().filter(|v| v.group == group).map(|v| v.inst).collect();
            insts.dedup();
            if insts.len() >= cap as usize {
                let oldest = insts[0];
                self.voices.retain(|v| !(v.group == group && v.inst == oldest));
            }
        }
        let inst = self.next_inst;
        self.next_inst = self.next_inst.wrapping_add(1).max(1);
        for (p, delay) in layers {
            self.play_grouped(*p, pan, gain, *delay, group, inst);
        }
    }

    /// Plays a bank cue (`ui.accept`) from this mixer's bank. False if the bank has no such cue.
    pub fn play_named(&mut self, cue: &str, pan: f32, gain: f32, pitch: f32) -> bool {
        let roll = (self.roll() >> 8) as f32 / (1u32 << 24) as f32;
        let Some((layers, cap)) = self.bank.voice(cue, pitch, 1.0, roll) else { return false };
        self.play_cue(&layers, pan, gain, group_of(cue), cap);
        true
    }

    /// What should be looping now, matched by id: new loops fade in, missing ones fade out.
    pub fn set_loops(&mut self, targets: &[LoopTarget]) {
        for v in &mut self.loops {
            match targets.iter().find(|t| t.id == v.target.id && t.kind == v.target.kind) {
                Some(t) => {
                    v.target = *t;
                    v.leaving = false;
                }
                None => v.leaving = true,
            }
        }
        for t in targets {
            if self.loops.len() >= MAX_LOOPS {
                break;
            }
            if !self.loops.iter().any(|v| v.target.id == t.id && v.target.kind == t.kind) {
                let seed = self.roll();
                self.loops.push(loops::LoopVoice::new(*t, seed));
            }
        }
    }

    /// What the radio should play (None: nothing). The same song keeps playing on the mixer's own
    /// clock (it jumps only when it drifts more than 0.75 s from the game's); a new station or song
    /// crossfades.
    pub fn set_music(&mut self, target: Option<&music::MusicTarget>) {
        let same = |v: &music::MusicVoice, t: &music::MusicTarget| v.station == t.station && v.song.seed == t.song.seed;
        match (target, self.music.as_mut()) {
            (Some(t), Some(v)) if same(v, t) => {
                v.target_gain = t.volume;
                v.target_muffle = t.muffle;
                v.set_segments(&t.segments);
                if (v.t - t.offset as f64).abs() > 0.75 {
                    v.t = t.offset as f64;
                }
            }
            (t, _) => {
                if let Some(mut old) = self.music.take() {
                    old.target_gain = 0.0;
                    self.music_out.push(old);
                }
                self.music = t.map(|t| music::MusicVoice::new(t, self.sample_rate));
            }
        }
    }

    /// The song the radio plays, if any (its station, and seconds into its slot).
    pub fn music(&self) -> Option<(&str, f64)> {
        self.music.as_ref().map(|v| (v.station.as_str(), v.t))
    }

    /// Renders into an interleaved stereo buffer (overwrites).
    pub fn render(&mut self, out: &mut [f32]) {
        let dt = 1.0 / self.sample_rate;
        let duck_to = dsp::db(-self.duck.clamp(0.0, 60.0));
        let glide = 1.0 - (-dt / 0.08).exp();
        for frame in out.chunks_mut(2) {
            self.duck_gain += (duck_to - self.duck_gain) * glide;
            let duck = self.duck_gain;
            let (mut l, mut r) = (0.0, 0.0);
            for v in &mut self.voices {
                let s = v.next(dt);
                l += s * v.gain_l;
                r += s * v.gain_r;
            }
            for v in &mut self.loops {
                let (a, b) = v.next(dt);
                let k = if v.target.kind.is_bed() { duck } else { 1.0 };
                l += a * k;
                r += b * k;
            }
            // The radio ducks under dialogue and menus like the beds.
            for v in self.music.iter_mut().chain(self.music_out.iter_mut()) {
                let (a, b) = v.next(dt);
                l += a * duck;
                r += b * duck;
            }
            if self.reverb_mix > 0.0 {
                let (wl, wr) = self.reverb.next(l, r, self.room);
                l += wl * self.reverb_mix;
                r += wr * self.reverb_mix;
            }
            let m = self.master;
            frame[0] = fx::limit(l * m);
            if frame.len() > 1 {
                frame[1] = fx::limit(r * m);
            }
        }
        self.voices.retain(|v| !v.done());
        self.loops.retain(|v| !v.silent());
        self.music_out.retain(|v| !v.silent());
    }

    pub fn active(&self) -> usize {
        self.voices.len()
    }

    /// Loops sounding (fading ones included).
    pub fn active_loops(&self) -> usize {
        self.loops.len()
    }
}

/// Where the ears are: position and the camera's right vector (for panning).
#[derive(Clone, Copy, Debug)]
pub struct Listener {
    pub pos: Vec3,
    pub right: Vec3,
}

impl Default for Listener {
    fn default() -> Self {
        Self { pos: Vec3::ZERO, right: Vec3::X }
    }
}

/// The sound bank: which patches play for a simulation event.
pub fn sounds_for(ev: &SimEvent) -> Vec<(Patch, f32)> {
    use Wave::*;
    match ev {
        SimEvent::Jump { .. } => vec![(
            Patch {
                freq_end: 640.0,
                glide: 0.11,
                decay: 0.12,
                volume: 0.18,
                cutoff: 3500.0,
                cutoff_end: 2500.0,
                duty: 0.3,
                ..Patch::new(Square, 280.0)
            },
            0.0,
        )],
        SimEvent::Land { speed, .. } => {
            let v = (speed / 14.0).clamp(0.15, 1.0);
            vec![
                (Patch { decay: 0.12, volume: 0.35 * v, cutoff: 1100.0, cutoff_end: 200.0, ..Patch::new(Noise, 1.0) }, 0.0),
                (Patch { freq_end: 45.0, glide: 0.1, decay: 0.14, volume: 0.45 * v, ..Patch::new(Sine, 110.0) }, 0.0),
            ]
        }
        SimEvent::Step { .. } => {
            vec![(Patch { decay: 0.045, volume: 0.07, cutoff: 1800.0, cutoff_end: 500.0, ..Patch::new(Noise, 1.0) }, 0.0)]
        }
        SimEvent::Throw { .. } => vec![(
            Patch { attack: 0.03, decay: 0.16, volume: 0.1, cutoff: 900.0, cutoff_end: 4500.0, ..Patch::new(Noise, 1.0) },
            0.0,
        )],
        SimEvent::Explosion { radius, .. } => {
            let v = (radius / 1.35).clamp(0.5, 2.0);
            vec![
                (
                    Patch {
                        attack: 0.004,
                        decay: 0.25,
                        sustain: 0.35,
                        hold: 0.1,
                        release: 0.6,
                        volume: 0.6 * v,
                        cutoff: 4000.0,
                        cutoff_end: 120.0,
                        ..Patch::new(Noise, 1.0)
                    },
                    0.0,
                ),
                (Patch { freq_end: 28.0, glide: 0.5, decay: 0.7, volume: 0.7 * v, ..Patch::new(Sine, 85.0) }, 0.0),
            ]
        }
        SimEvent::EnterRoom { .. } => vec![
            (Patch { decay: 0.25, volume: 0.12, ..Patch::new(Triangle, 659.3) }, 0.0),
            (Patch { decay: 0.35, volume: 0.12, ..Patch::new(Triangle, 987.8) }, 0.09),
        ],
        SimEvent::Hit { strength, .. } => {
            let v = (strength / 8.0).clamp(0.4, 1.2);
            vec![
                (Patch { decay: 0.09, volume: 0.3 * v, cutoff: 2500.0, cutoff_end: 300.0, ..Patch::new(Noise, 1.0) }, 0.0),
                (
                    Patch { freq_end: 90.0, glide: 0.12, decay: 0.16, volume: 0.3 * v, duty: 0.4, ..Patch::new(Square, 220.0) },
                    0.0,
                ),
            ]
        }
        // Shardfall.
        SimEvent::Swing { heavy, .. } => {
            let h = if *heavy { 1.4 } else { 1.0 };
            vec![(
                Patch {
                    attack: 0.02,
                    decay: 0.12 * h,
                    volume: 0.16 * h,
                    cutoff: 1800.0,
                    cutoff_end: 5200.0,
                    ..Patch::new(Noise, 1.0)
                },
                0.0,
            )]
        }
        SimEvent::Strike { power, element, crit, .. } => {
            let v = (power.sqrt() / 4.0).clamp(0.35, 1.2) * if *crit { 1.3 } else { 1.0 };
            let tone = [150.0, 120.0, 520.0, 900.0, 260.0][(*element as usize).min(4)];
            let mut out = vec![
                (Patch { decay: 0.07, volume: 0.24 * v, cutoff: 3200.0, cutoff_end: 400.0, ..Patch::new(Noise, 1.0) }, 0.0),
                (
                    Patch {
                        freq_end: tone * 0.5,
                        glide: 0.08,
                        decay: 0.1,
                        volume: 0.2 * v,
                        duty: 0.35,
                        ..Patch::new(Square, tone)
                    },
                    0.0,
                ),
            ];
            if *crit {
                out.push((Patch { decay: 0.18, volume: 0.12, ..Patch::new(Triangle, 1760.0) }, 0.01));
            }
            out
        }
        SimEvent::Slain { size, .. } => {
            let k = size.clamp(0.3, 1.5);
            vec![
                (
                    Patch {
                        freq_end: 60.0,
                        glide: 0.3,
                        decay: 0.3,
                        volume: 0.22 * k,
                        duty: 0.5,
                        ..Patch::new(Square, 180.0 / k)
                    },
                    0.0,
                ),
                (Patch { decay: 0.25, volume: 0.15, cutoff: 1500.0, cutoff_end: 200.0, ..Patch::new(Noise, 1.0) }, 0.02),
            ]
        }
        SimEvent::Slam { radius, .. } => {
            let k = (radius / 2.5).clamp(0.6, 1.5);
            vec![
                (Patch { freq_end: 40.0, glide: 0.35, decay: 0.4, volume: 0.45 * k, ..Patch::new(Sine, 110.0) }, 0.0),
                (Patch { decay: 0.3, volume: 0.3 * k, cutoff: 2200.0, cutoff_end: 150.0, ..Patch::new(Noise, 1.0) }, 0.0),
            ]
        }
        SimEvent::Blast { element, .. } => {
            let cut = [2500.0, 3500.0, 6000.0, 8000.0, 2000.0][(*element as usize).min(4)];
            vec![
                (
                    Patch { attack: 0.003, decay: 0.3, volume: 0.35, cutoff: cut, cutoff_end: 200.0, ..Patch::new(Noise, 1.0) },
                    0.0,
                ),
                (Patch { freq_end: 50.0, glide: 0.25, decay: 0.3, volume: 0.25, ..Patch::new(Sine, 140.0) }, 0.0),
            ]
        }
        SimEvent::Spell { element, .. } => {
            let f = [300.0, 260.0, 900.0, 1400.0, 420.0][(*element as usize).min(4)];
            vec![(
                Patch {
                    freq_end: f * 1.8,
                    glide: 0.15,
                    attack: 0.02,
                    decay: 0.18,
                    volume: 0.12,
                    duty: 0.25,
                    ..Patch::new(Saw, f)
                },
                0.0,
            )]
        }
        SimEvent::Coin { .. } => vec![
            (Patch { decay: 0.08, volume: 0.1, ..Patch::new(Triangle, 1980.0) }, 0.0),
            (Patch { decay: 0.12, volume: 0.09, ..Patch::new(Triangle, 2640.0) }, 0.05),
        ],
        // Drops chime by rarity: rares ring, uniques ring a chord.
        SimEvent::Loot { rarity, .. } => match rarity {
            0 => vec![(Patch { decay: 0.05, volume: 0.05, cutoff: 2500.0, ..Patch::new(Triangle, 880.0) }, 0.0)],
            1 => vec![(Patch { decay: 0.12, volume: 0.08, ..Patch::new(Triangle, 1320.0) }, 0.0)],
            2 => vec![
                (Patch { decay: 0.25, volume: 0.12, ..Patch::new(Triangle, 1568.0) }, 0.0),
                (Patch { decay: 0.35, volume: 0.1, ..Patch::new(Sine, 2093.0) }, 0.08),
            ],
            _ => vec![
                (Patch { decay: 0.5, volume: 0.14, ..Patch::new(Triangle, 784.0) }, 0.0),
                (Patch { decay: 0.5, volume: 0.13, ..Patch::new(Triangle, 988.0) }, 0.06),
                (Patch { decay: 0.6, volume: 0.13, ..Patch::new(Triangle, 1175.0) }, 0.12),
                (Patch { decay: 0.9, volume: 0.12, ..Patch::new(Sine, 1568.0) }, 0.2),
            ],
        },
        SimEvent::Pickup { .. } => {
            vec![(Patch { freq_end: 1400.0, glide: 0.06, decay: 0.07, volume: 0.08, ..Patch::new(Triangle, 700.0) }, 0.0)]
        }
        SimEvent::Travel { .. } => vec![
            (Patch { freq_end: 1200.0, glide: 0.5, decay: 0.6, volume: 0.12, ..Patch::new(Sine, 200.0) }, 0.0),
            (Patch { freq_end: 2400.0, glide: 0.4, decay: 0.5, volume: 0.06, ..Patch::new(Triangle, 600.0) }, 0.1),
        ],
        SimEvent::Potion { .. } => {
            vec![(Patch { freq_end: 700.0, glide: 0.3, decay: 0.3, volume: 0.14, ..Patch::new(Sine, 300.0) }, 0.0)]
        }
        SimEvent::Block { .. } => {
            vec![(Patch { decay: 0.2, volume: 0.2, cutoff: 6000.0, cutoff_end: 3000.0, ..Patch::new(Square, 1200.0) }, 0.0)]
        }
        SimEvent::LevelUp { .. } => vec![
            (Patch { decay: 0.2, volume: 0.16, ..Patch::new(Triangle, 523.0) }, 0.0),
            (Patch { decay: 0.2, volume: 0.16, ..Patch::new(Triangle, 659.0) }, 0.1),
            (Patch { decay: 0.2, volume: 0.16, ..Patch::new(Triangle, 784.0) }, 0.2),
            (Patch { decay: 0.5, volume: 0.18, ..Patch::new(Triangle, 1047.0) }, 0.3),
        ],
        SimEvent::Respawn { .. } => vec![
            (Patch { freq_end: 880.0, glide: 0.18, decay: 0.2, volume: 0.12, ..Patch::new(Sine, 220.0) }, 0.0),
            (Patch { decay: 0.15, volume: 0.08, ..Patch::new(Triangle, 1320.0) }, 0.16),
        ],
        SimEvent::CourseStart { .. } => vec![(Patch { decay: 0.12, volume: 0.14, ..Patch::new(Square, 880.0) }, 0.0)],
        SimEvent::Gate { ok, .. } => {
            if *ok {
                vec![(Patch { decay: 0.1, volume: 0.12, ..Patch::new(Triangle, 1046.5) }, 0.0)]
            } else {
                vec![(
                    Patch { freq_end: 160.0, glide: 0.2, decay: 0.25, volume: 0.14, duty: 0.3, ..Patch::new(Square, 330.0) },
                    0.0,
                )]
            }
        }
        SimEvent::CourseFinish { new_best, .. } => {
            let mut v = vec![
                (Patch { decay: 0.18, volume: 0.13, ..Patch::new(Triangle, 523.3) }, 0.0),
                (Patch { decay: 0.18, volume: 0.13, ..Patch::new(Triangle, 659.3) }, 0.1),
                (Patch { decay: 0.4, volume: 0.14, ..Patch::new(Triangle, 784.0) }, 0.2),
            ];
            if *new_best {
                v.push((Patch { decay: 0.5, volume: 0.13, ..Patch::new(Triangle, 1046.5) }, 0.34));
            }
            v
        }
        SimEvent::Checkpoint { .. } => vec![
            (Patch { decay: 0.12, volume: 0.1, ..Patch::new(Sine, 1174.7) }, 0.0),
            (Patch { decay: 0.2, volume: 0.1, ..Patch::new(Sine, 1568.0) }, 0.07),
        ],
        SimEvent::Splash { .. } => vec![(
            Patch { attack: 0.01, decay: 0.35, volume: 0.25, cutoff: 3000.0, cutoff_end: 400.0, ..Patch::new(Noise, 1.0) },
            0.0,
        )],
        SimEvent::Pad { .. } => {
            vec![(Patch { freq_end: 1200.0, glide: 0.08, decay: 0.12, volume: 0.1, ..Patch::new(Sine, 600.0) }, 0.0)]
        }
        SimEvent::Grab { .. } => {
            vec![(Patch { decay: 0.05, volume: 0.12, cutoff: 1500.0, cutoff_end: 600.0, ..Patch::new(Noise, 1.0) }, 0.0)]
        }
        SimEvent::Break { .. } => vec![
            (Patch { decay: 0.18, volume: 0.3, cutoff: 6000.0, cutoff_end: 900.0, ..Patch::new(Noise, 1.0) }, 0.0),
            (Patch { freq_end: 70.0, glide: 0.15, decay: 0.2, volume: 0.25, ..Patch::new(Sine, 160.0) }, 0.0),
        ],
        SimEvent::Crack { .. } => {
            vec![(Patch { decay: 0.04, volume: 0.12, cutoff: 5000.0, cutoff_end: 2000.0, ..Patch::new(Noise, 1.0) }, 0.0)]
        }
        SimEvent::Bounce { .. } => {
            vec![(Patch { freq_end: 900.0, glide: 0.18, decay: 0.22, volume: 0.16, ..Patch::new(Sine, 180.0) }, 0.0)]
        }
        SimEvent::Roll { .. } => vec![(
            Patch { attack: 0.02, decay: 0.2, volume: 0.1, cutoff: 600.0, cutoff_end: 2400.0, ..Patch::new(Noise, 1.0) },
            0.0,
        )],
        SimEvent::Spotted { .. } => vec![
            (Patch { decay: 0.18, volume: 0.16, ..Patch::new(Square, 990.0) }, 0.0),
            (Patch { decay: 0.25, volume: 0.16, ..Patch::new(Square, 660.0) }, 0.16),
        ],
        SimEvent::Shot { .. } => {
            vec![(Patch { freq_end: 420.0, glide: 0.06, decay: 0.06, volume: 0.05, ..Patch::new(Square, 1500.0) }, 0.0)]
        }
        SimEvent::Damage { .. } => {
            vec![(Patch { decay: 0.03, volume: 0.08, cutoff: 3500.0, cutoff_end: 1200.0, ..Patch::new(Noise, 1.0) }, 0.0)]
        }
        SimEvent::Destroyed { size, .. } => {
            let v = size.clamp(0.4, 2.0);
            vec![
                (
                    Patch {
                        attack: 0.003,
                        decay: 0.3,
                        volume: 0.35 * v,
                        cutoff: 5000.0,
                        cutoff_end: 200.0,
                        ..Patch::new(Noise, 1.0)
                    },
                    0.0,
                ),
                (Patch { freq_end: 40.0, glide: 0.35, decay: 0.4, volume: 0.4 * v, ..Patch::new(Sine, 140.0) }, 0.0),
            ]
        }
        _ => Vec::new(),
    }
}

/// Position of an event (None = not spatial).
pub fn event_pos(ev: &SimEvent) -> Option<Vec3> {
    match ev {
        SimEvent::Jump { pos }
        | SimEvent::Land { pos, .. }
        | SimEvent::Step { pos }
        | SimEvent::Throw { pos }
        | SimEvent::Explosion { pos, .. }
        | SimEvent::Impact { pos, .. }
        | SimEvent::Hit { pos, .. }
        | SimEvent::Splash { pos }
        | SimEvent::Grab { pos }
        | SimEvent::Roll { pos }
        | SimEvent::Break { pos }
        | SimEvent::Crack { pos }
        | SimEvent::Bounce { pos }
        | SimEvent::Shot { pos }
        | SimEvent::Damage { pos }
        | SimEvent::Destroyed { pos, .. }
        | SimEvent::Swing { pos, .. }
        | SimEvent::Strike { pos, .. }
        | SimEvent::Slain { pos, .. }
        | SimEvent::Slam { pos, .. }
        | SimEvent::Blast { pos, .. }
        | SimEvent::Spell { pos, .. }
        | SimEvent::Coin { pos }
        | SimEvent::Loot { pos, .. }
        | SimEvent::Pickup { pos, .. }
        | SimEvent::Block { pos } => Some(*pos),
        _ => None,
    }
}

/// Pan and distance gain of a sound at `pos` (None = not positional) for the listener.
pub fn place(pos: Option<Vec3>, l: &Listener) -> (f32, f32) {
    match pos {
        Some(p) => {
            let d = p - l.pos;
            ((d.dot(l.right) / 12.0).clamp(-0.85, 0.85), 1.0 / (1.0 + d.length() / 14.0))
        }
        None => (0.0, 1.0),
    }
}

/// Plays an event's sounds with distance attenuation and panning (`SimEvent::Sound` from the
/// mixer's bank).
pub fn play_event(mixer: &mut Mixer, ev: &SimEvent, l: &Listener, sfx: f32) {
    if let SimEvent::Sound { cue, pos, gain, pitch } = ev {
        let (pan, g) = place(*pos, l);
        if !mixer.play_named(cue, pan, g * gain * sfx, *pitch) {
            log::warn!("no sound cue '{cue}'");
        }
        return;
    }
    let (pan, gain) = place(event_pos(ev), l);
    for (patch, delay) in sounds_for(ev) {
        mixer.play(patch, pan, gain * sfx, delay);
    }
}

/// The radio a game's audio scene asks for (its volume times the effects volume is the game's).
pub fn music_target(scene: &pav_core::present::AudioScene) -> Option<music::MusicTarget> {
    scene.music.as_ref().map(|m| music::MusicTarget {
        station: m.station.clone(),
        song: m.song.clone(),
        offset: m.offset,
        volume: m.volume,
        muffle: m.muffle,
        segments: Vec::new(),
    })
}

/// The loops a game's audio scene asks for, placed for the listener (distance and pan).
pub fn loop_targets(scene: &pav_core::present::AudioScene, l: &Listener) -> Vec<LoopTarget> {
    let mut out = Vec::new();
    for s in &scene.loops {
        let Some(kind) = LoopKind::from_cue(&s.cue) else { continue };
        let (pan, dist) = place(s.pos, l);
        out.push(LoopTarget { id: s.id, kind, gain: s.gain.clamp(0.0, 2.0) * dist, pan, pitch: s.pitch, param: s.param });
    }
    out
}

/// Renders timed events offline: (time in seconds, event). Returns interleaved stereo.
pub fn render_events(events: &[(f32, SimEvent)], duration: f32, sample_rate: f32, l: &Listener) -> Vec<f32> {
    let mut mixer = Mixer::new(sample_rate);
    let total = (duration * sample_rate) as usize;
    let mut out = vec![0.0f32; total * 2];
    let block = 256;
    let mut i = 0;
    let mut ev = events.iter().peekable();
    while i < total {
        let t = i as f32 / sample_rate;
        while let Some((et, e)) = ev.peek() {
            if *et > t {
                break;
            }
            play_event(&mut mixer, e, l, 1.0);
            ev.next();
        }
        let n = block.min(total - i);
        mixer.render(&mut out[i * 2..(i + n) * 2]);
        i += n;
    }
    out
}

/// Writes interleaved stereo f32 samples as a 16-bit PCM WAV file.
/// Renders a run tick by tick: each tick's events, then the loops, radio and bus its audio scene
/// asks for (a hook game's `PlayFrame::audio`), then `tail` seconds more. Interleaved stereo.
pub fn render_run(
    ticks: &[(Vec<SimEvent>, Option<pav_core::present::AudioScene>)],
    dt: f32,
    tail: f32,
    sample_rate: f32,
    l: &Listener,
) -> Vec<f32> {
    let mut mixer = Mixer::new(sample_rate);
    let per = (dt * sample_rate).round().max(1.0) as usize;
    let mut out = Vec::with_capacity((ticks.len() * per + (tail * sample_rate) as usize) * 2);
    let mut buf = vec![0.0f32; per * 2];
    for (events, scene) in ticks {
        let sfx = scene.as_ref().map_or(1.0, |s| s.effects);
        for e in events {
            play_event(&mut mixer, e, l, sfx);
        }
        if let Some(scene) = scene {
            let mut loops = loop_targets(scene, l);
            for t in &mut loops {
                t.gain *= scene.effects;
            }
            mixer.set_loops(&loops);
            mixer.set_music(music_target(scene).as_ref());
            mixer.reverb_mix = scene.reverb;
            mixer.duck = scene.ducking;
            mixer.master = 0.8 * scene.master;
        }
        mixer.render(&mut buf);
        out.extend_from_slice(&buf);
    }
    let mut rest = vec![0.0f32; (tail * sample_rate) as usize * 2];
    mixer.render(&mut rest);
    out.extend(rest);
    out
}

pub fn write_wav(path: &std::path::Path, samples: &[f32], sample_rate: u32) -> std::io::Result<()> {
    let data_len = (samples.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&2u16.to_le_bytes()); // stereo
    b.extend_from_slice(&sample_rate.to_le_bytes());
    b.extend_from_slice(&(sample_rate * 4).to_le_bytes());
    b.extend_from_slice(&4u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        b.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    if let Some(d) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(path, b)
}

enum Cmd {
    Play(Patch, f32, f32, f32),
    Cue(Vec<(Patch, f32)>, f32, f32, u32, u8),
    Loops(Vec<LoopTarget>),
    Bus { reverb: f32, room: f32, duck: f32 },
    Master(f32),
    Music(Option<music::MusicTarget>),
}

/// Live output through the default sound device.
pub struct AudioOut {
    _stream: cpal::Stream,
    tx: Sender<Cmd>,
    pub sample_rate: u32,
    pub device: String,
    /// The cues `SimEvent::Sound` names (the engine's, plus a game's).
    pub bank: Arc<Bank>,
    roll: std::cell::Cell<u32>,
}

impl AudioOut {
    pub fn start() -> anyhow::Result<Self> {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or_else(|| anyhow::anyhow!("no sound output device"))?;
        let name = device.description().map(|d| d.to_string()).unwrap_or_else(|_| "default".into());
        let supported = device.default_output_config()?;
        let format = supported.sample_format();
        let config: cpal::StreamConfig = supported.config();
        let rate = config.sample_rate as u32;
        let channels = config.channels as usize;
        let (tx, rx) = channel::<Cmd>();
        let stream = match format {
            cpal::SampleFormat::F32 => build::<f32>(&device, config, rx, channels)?,
            cpal::SampleFormat::I16 => build::<i16>(&device, config, rx, channels)?,
            cpal::SampleFormat::U16 => build::<u16>(&device, config, rx, channels)?,
            other => anyhow::bail!("unsupported sample format {other:?}"),
        };
        stream.play()?;
        Ok(Self {
            _stream: stream,
            tx,
            sample_rate: rate,
            device: name,
            bank: Arc::new(Bank::engine()),
            roll: std::cell::Cell::new(0x2545_f491),
        })
    }

    pub fn play_event(&self, ev: &SimEvent, l: &Listener, sfx: f32) {
        if let SimEvent::Sound { cue, pos, gain, pitch } = ev {
            self.play_cue(cue, *pos, l, gain * sfx, *pitch);
            return;
        }
        let (pan, gain) = place(event_pos(ev), l);
        for (patch, delay) in sounds_for(ev) {
            let _ = self.tx.send(Cmd::Play(patch, pan, gain * sfx, delay));
        }
    }

    /// Plays a bank cue at `pos` (None = not positional).
    pub fn play_cue(&self, cue: &str, pos: Option<Vec3>, l: &Listener, gain: f32, pitch: f32) {
        let r = self.roll.get().wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.roll.set(r);
        let Some((layers, cap)) = self.bank.voice(cue, pitch, 1.0, (r >> 8) as f32 / (1u32 << 24) as f32) else {
            log::warn!("no sound cue '{cue}'");
            return;
        };
        let (pan, g) = place(pos, l);
        let _ = self.tx.send(Cmd::Cue(layers, pan, g * gain, group_of(cue), cap));
    }

    /// What should be looping now (from a game's audio scene: `loop_targets`).
    pub fn set_loops(&self, targets: Vec<LoopTarget>) {
        let _ = self.tx.send(Cmd::Loops(targets));
    }

    /// The effects bus: reverb send and room size, and the decibels the radio and the beds
    /// duck by.
    pub fn set_bus(&self, reverb: f32, room: f32, duck: f32) {
        let _ = self.tx.send(Cmd::Bus { reverb, room, duck });
    }

    /// What the radio should play (from a game's audio scene: `music_target`).
    pub fn set_music(&self, target: Option<music::MusicTarget>) {
        let _ = self.tx.send(Cmd::Music(target));
    }

    pub fn set_master(&self, v: f32) {
        let _ = self.tx.send(Cmd::Master(v));
    }

    /// Starts output again. Browsers keep sound off until the page gets a click or key press,
    /// so the browser build calls this on the first input.
    pub fn resume(&self) {
        use cpal::traits::StreamTrait;
        let _ = self._stream.play();
    }
}

fn build<T>(device: &cpal::Device, config: cpal::StreamConfig, rx: Receiver<Cmd>, channels: usize) -> anyhow::Result<cpal::Stream>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    use cpal::traits::DeviceTrait;
    let mut mixer = Mixer::new(config.sample_rate as f32);
    let mut buf: Vec<f32> = Vec::new();
    let stream = device.build_output_stream::<T, _, _>(
        config,
        move |data: &mut [T], _| {
            while let Ok(c) = rx.try_recv() {
                match c {
                    Cmd::Play(p, pan, gain, delay) => mixer.play(p, pan, gain, delay),
                    Cmd::Cue(layers, pan, gain, group, cap) => mixer.play_cue(&layers, pan, gain, group, cap),
                    Cmd::Loops(t) => mixer.set_loops(&t),
                    Cmd::Bus { reverb, room, duck } => {
                        mixer.reverb_mix = reverb;
                        mixer.room = room;
                        mixer.duck = duck;
                    }
                    Cmd::Master(v) => mixer.master = v,
                    Cmd::Music(t) => mixer.set_music(t.as_ref()),
                }
            }
            let frames = data.len() / channels.max(1);
            buf.resize(frames * 2, 0.0);
            mixer.render(&mut buf);
            for (i, frame) in data.chunks_mut(channels.max(1)).enumerate() {
                for (c, s) in frame.iter_mut().enumerate() {
                    *s = T::from_sample(buf[i * 2 + c.min(1)]);
                }
            }
        },
        |e| log::warn!("audio stream error: {e}"),
        None,
    )?;
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sound_cues_play_from_the_bank_and_caps_hold() {
        let mut m = Mixer::new(22050.0);
        let l = Listener::default();
        let ev = SimEvent::Sound { cue: "ui.accept".into(), pos: None, gain: 1.0, pitch: 1.0 };
        play_event(&mut m, &ev, &l, 1.0);
        assert_eq!(m.active(), 2, "two layers");
        for _ in 0..5 {
            m.play_named("ui.move", 0.0, 1.0, 1.0);
        }
        assert_eq!(m.active(), 2 + 2, "ui.move plays at most two at once");
        let mut buf = vec![0.0; 4410];
        m.render(&mut buf);
        assert!(buf.iter().any(|s| s.abs() > 0.01));
    }

    #[test]
    fn loops_fade_in_glide_and_fade_out() {
        let mut m = Mixer::new(22050.0);
        let t =
            LoopTarget { id: 7, kind: LoopKind::from_cue("engine.sports").unwrap(), gain: 0.8, pan: 0.0, pitch: 1.0, param: 0.2 };
        m.set_loops(&[t]);
        let mut buf = vec![0.0; 22050 * 2];
        m.render(&mut buf);
        let idle = buf[22050..].iter().fold(0.0f32, |a, s| a.max(s.abs()));
        assert!(idle > 0.02, "the engine is audible: {idle}");
        m.set_loops(&[LoopTarget { pitch: 3.5, param: 1.0, ..t }]);
        assert_eq!(m.active_loops(), 1, "same id: the same voice glides");
        m.set_loops(&[]);
        for _ in 0..4 {
            m.render(&mut buf);
        }
        assert_eq!(m.active_loops(), 0, "faded out and gone");
    }

    #[test]
    fn explosion_is_loud_then_silent() {
        let ev = SimEvent::Explosion { pos: Vec3::ZERO, radius: 1.35 };
        let out = render_events(&[(0.0, ev)], 2.0, 22050.0, &Listener::default());
        let peak_early = out[..22050].iter().fold(0.0f32, |m, s| m.max(s.abs()));
        let peak_late = out[out.len() - 2000..].iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak_early > 0.1, "audible: {peak_early}");
        assert!(peak_late < 1e-3, "decays: {peak_late}");
    }
}
