//! Continuous sounds with live parameters: engines (stacked harmonics whose pitch follows the
//! revs and whose filter opens with the load), sirens (wail and yelp sweeps), tyre squeal (noise
//! by slip), helicopter rotors (pulsed noise) and ambience beds (city, surf, wind, rain, crowd,
//! harbour). The host hands the mixer what should be sounding every frame
//! (`Mixer::set_loops`), matched by id: new loops fade in, missing ones fade out, the rest glide
//! to their new gain, pan, pitch and parameter.

/// An ambience bed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bed {
    City,
    Surf,
    Wind,
    Rain,
    Crowd,
    Harbor,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LoopKind {
    /// `base` Hz at idle; `grit` adds exhaust rasp (0..1); `sub` the low octave's weight.
    Engine {
        base: f32,
        grit: f32,
        sub: f32,
    },
    Siren {
        yelp: bool,
    },
    Tyre,
    Rotor,
    /// A car horn (two notes a third apart), and a car alarm (alternating beeps).
    Horn,
    Alarm,
    Bed(Bed),
}

impl LoopKind {
    /// The loop a cue names: `engine.<class>`, `siren.wail`, `siren.yelp`, `tyre`, `rotor`,
    /// `horn`, `alarm`, `amb.<bed>`.
    pub fn from_cue(cue: &str) -> Option<LoopKind> {
        Some(match cue {
            "engine.compact" => LoopKind::Engine { base: 52.0, grit: 0.15, sub: 0.3 },
            "engine.sedan" => LoopKind::Engine { base: 44.0, grit: 0.2, sub: 0.45 },
            "engine.sports" => LoopKind::Engine { base: 58.0, grit: 0.35, sub: 0.35 },
            "engine.muscle" => LoopKind::Engine { base: 36.0, grit: 0.6, sub: 0.7 },
            "engine.truck" => LoopKind::Engine { base: 28.0, grit: 0.5, sub: 0.8 },
            "engine.bike" => LoopKind::Engine { base: 70.0, grit: 0.45, sub: 0.2 },
            "engine.van" => LoopKind::Engine { base: 40.0, grit: 0.3, sub: 0.6 },
            "siren.wail" => LoopKind::Siren { yelp: false },
            "siren.yelp" => LoopKind::Siren { yelp: true },
            "tyre" => LoopKind::Tyre,
            "rotor" => LoopKind::Rotor,
            "horn" => LoopKind::Horn,
            "alarm" => LoopKind::Alarm,
            "amb.city" => LoopKind::Bed(Bed::City),
            "amb.surf" => LoopKind::Bed(Bed::Surf),
            "amb.wind" => LoopKind::Bed(Bed::Wind),
            "amb.rain" => LoopKind::Bed(Bed::Rain),
            "amb.crowd" => LoopKind::Bed(Bed::Crowd),
            "amb.harbor" => LoopKind::Bed(Bed::Harbor),
            _ => return None,
        })
    }

    pub fn is_bed(&self) -> bool {
        matches!(self, LoopKind::Bed(_))
    }
}

/// What a loop should be doing now; the voice glides there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoopTarget {
    pub id: u64,
    pub kind: LoopKind,
    /// Volume after distance (0..1), pan (-1 left .. 1 right).
    pub gain: f32,
    pub pan: f32,
    /// Pitch factor (engines: revs, 1 = idle .. ~4 = redline; rotors: spin).
    pub pitch: f32,
    /// The kind's own parameter (engine load, tyre slip, wind gusts, siren on), 0..1.
    pub param: f32,
}

pub(crate) struct LoopVoice {
    pub target: LoopTarget,
    gain: f32,
    pan: f32,
    pitch: f32,
    param: f32,
    /// Fading out (removed when silent).
    pub leaving: bool,
    phase: [f32; 4],
    lp: [f32; 2],
    bp: [f32; 2],
    noise: u32,
    t: f32,
}

fn glide(v: &mut f32, to: f32, rate: f32, dt: f32) {
    *v += (to - *v) * (1.0 - (-rate * dt).exp());
}

impl LoopVoice {
    pub fn new(target: LoopTarget, seed: u32) -> Self {
        Self {
            target,
            gain: 0.0,
            pan: target.pan,
            pitch: target.pitch,
            param: target.param,
            leaving: false,
            phase: [0.0, 0.25, 0.5, 0.75],
            lp: [0.0; 2],
            bp: [0.0; 2],
            noise: seed | 1,
            t: 0.0,
        }
    }

    pub fn silent(&self) -> bool {
        self.leaving && self.gain < 1e-4
    }

    fn noise(&mut self) -> f32 {
        self.noise ^= self.noise << 13;
        self.noise ^= self.noise >> 17;
        self.noise ^= self.noise << 5;
        (self.noise as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    fn lowpass(&mut self, i: usize, x: f32, cutoff: f32, dt: f32) -> f32 {
        let a = 1.0 - (-std::f32::consts::TAU * cutoff * dt).exp();
        self.lp[i] += (x - self.lp[i]) * a;
        self.lp[i]
    }

    /// A two-pole resonant band-pass (state-variable filter).
    fn bandpass(&mut self, x: f32, centre: f32, q: f32, dt: f32) -> f32 {
        let f = 2.0 * (std::f32::consts::PI * centre * dt).sin();
        let [low, band] = &mut self.bp;
        *low += f * *band;
        let high = x - *low - *band / q.max(0.5);
        *band += f * high;
        *band
    }

    /// One stereo sample.
    pub fn next(&mut self, dt: f32) -> (f32, f32) {
        let t = self.target;
        let fade = if self.leaving { 0.0 } else { t.gain };
        glide(&mut self.gain, fade, 6.0, dt);
        glide(&mut self.pan, t.pan, 10.0, dt);
        glide(&mut self.pitch, t.pitch, 8.0, dt);
        glide(&mut self.param, t.param, 8.0, dt);
        self.t += dt;
        let saw = |p: f32| p * 2.0 - 1.0;
        let s = match t.kind {
            LoopKind::Engine { base, grit, sub } => {
                let f = base * self.pitch.clamp(0.5, 5.0);
                for (i, mul) in [1.0f32, 2.0, 0.5, 3.0].iter().enumerate() {
                    self.phase[i] = (self.phase[i] + f * mul * dt * (1.0 + i as f32 * 0.003)).fract();
                }
                let tone = saw(self.phase[0]) * 0.55
                    + (if self.phase[1] < 0.5 { 0.35 } else { -0.35 })
                    + saw(self.phase[2]) * sub
                    + saw(self.phase[3]) * 0.12;
                let rasp = self.noise() * grit * (0.3 + self.param) * (1.0 - self.phase[0]);
                let cutoff = 350.0 + self.param * 3200.0 + self.pitch * 180.0;
                self.lowpass(0, tone + rasp, cutoff, dt) * 0.35
            }
            LoopKind::Siren { yelp } => {
                let period = if yelp { 0.32 } else { 4.2 };
                let ph = (self.t / period).fract();
                let sweep = if ph < 0.5 { ph * 2.0 } else { 2.0 - ph * 2.0 };
                let f = 640.0 + sweep * 860.0;
                self.phase[0] = (self.phase[0] + f * dt).fract();
                let tri = 1.0 - 4.0 * (self.phase[0] - 0.5).abs();
                let sq = if self.phase[0] < 0.5 { 1.0 } else { -1.0 };
                self.lowpass(0, tri * 0.7 + sq * 0.3, 3200.0, dt) * 0.3 * self.param.max(0.0)
            }
            LoopKind::Tyre => {
                let n = self.noise();
                let squeal = self.bandpass(n, 1500.0 + self.param * 900.0, 6.0, dt);
                squeal * 0.25 * self.param
            }
            LoopKind::Rotor => {
                let rate = 16.0 * self.pitch.clamp(0.2, 2.0);
                self.phase[0] = (self.phase[0] + rate * dt).fract();
                let pulse = (1.0 - self.phase[0]).powi(6);
                let n = self.noise();
                let body = self.lowpass(0, n, 380.0, dt);
                body * (0.25 + pulse * 1.6) * 0.8
            }
            LoopKind::Horn => {
                // Two reedy notes a major third apart (F#4 and A#4), a little detuned.
                let mut s = 0.0;
                for (i, f) in [370.0f32, 466.2].iter().enumerate() {
                    self.phase[i] = (self.phase[i] + f * self.pitch.clamp(0.5, 2.0) * dt).fract();
                    s += if self.phase[i] < 0.42 { 0.5 } else { -0.5 };
                }
                self.lowpass(0, s, 2400.0, dt) * 0.3
            }
            LoopKind::Alarm => {
                // Two tones swapping three times a second, pulsing.
                let hi = (self.t * 3.0).fract() < 0.5;
                let f = if hi { 1350.0 } else { 950.0 };
                self.phase[0] = (self.phase[0] + f * dt).fract();
                let sq = if self.phase[0] < 0.5 { 1.0 } else { -1.0 };
                let gate = if (self.t * 12.0).fract() < 0.8 { 1.0 } else { 0.0 };
                self.lowpass(0, sq * gate, 3000.0, dt) * 0.18
            }
            LoopKind::Bed(bed) => {
                let n = self.noise();
                match bed {
                    Bed::City => {
                        let hum = self.lowpass(0, n, 260.0, dt);
                        let wob = 0.8 + 0.2 * (self.t * 0.13).sin();
                        hum * 0.9 * wob
                    }
                    Bed::Surf => {
                        let swell = 0.35 + 0.65 * (0.5 + 0.5 * (self.t * std::f32::consts::TAU / 7.5).sin()).powi(3);
                        let wash = self.lowpass(0, n, 500.0 + swell * 1500.0, dt);
                        wash * swell * 0.7
                    }
                    Bed::Wind => {
                        let gust = 0.5 + 0.5 * (self.t * 0.37).sin() * (self.t * 0.11).cos();
                        let air = self.bandpass(n, 300.0 + gust * 500.0, 1.5, dt);
                        air * (0.2 + 0.8 * self.param.max(gust * 0.5)) * 0.6
                    }
                    Bed::Rain => {
                        let low = self.lowpass(0, n, 1800.0, dt);
                        (n - low) * 0.3
                    }
                    Bed::Crowd => {
                        let chatter = self.bandpass(n, 700.0 + 300.0 * (self.t * 1.7).sin(), 2.0, dt);
                        let am = 0.6 + 0.4 * (self.t * 5.3).sin() * (self.t * 3.1).sin();
                        chatter * am * 0.5
                    }
                    Bed::Harbor => {
                        self.phase[0] = (self.phase[0] + 58.0 * dt).fract();
                        let hum = (self.phase[0] * std::f32::consts::TAU).sin() * 0.25;
                        let water = self.lowpass(0, n, 220.0, dt);
                        hum + water * 0.8
                    }
                }
            }
        };
        let a = (self.pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
        let g = self.gain;
        (s * a.cos() * g, s * a.sin() * g)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_named_loop_sounds_and_fades() {
        let names = [
            "engine.compact",
            "engine.sedan",
            "engine.sports",
            "engine.muscle",
            "engine.truck",
            "engine.bike",
            "engine.van",
            "siren.wail",
            "siren.yelp",
            "tyre",
            "rotor",
            "horn",
            "alarm",
            "amb.city",
            "amb.surf",
            "amb.wind",
            "amb.rain",
            "amb.crowd",
            "amb.harbor",
        ];
        let dt = 1.0 / 22050.0;
        for (i, name) in names.iter().enumerate() {
            let kind = LoopKind::from_cue(name).unwrap();
            let mut v =
                LoopVoice::new(LoopTarget { id: i as u64, kind, gain: 1.0, pan: 0.0, pitch: 2.0, param: 1.0 }, 7 + i as u32);
            let mut peak = 0.0f32;
            for _ in 0..22050 {
                let (l, r) = v.next(dt);
                assert!(l.is_finite() && r.is_finite(), "{name}");
                peak = peak.max(l.abs().max(r.abs()));
            }
            assert!(peak > 0.01 && peak < 1.5, "{name}: peak {peak}");
            v.leaving = true;
            for _ in 0..22050 * 3 {
                v.next(dt);
            }
            assert!(v.silent(), "{name} fades out");
        }
        assert!(LoopKind::from_cue("engine.unknown").is_none());
    }
}
