//! The radio, listened to: every genre plays in range and about as loud as the others, keeps
//! its tempo (from the onsets' autocorrelation), swings where it should, builds from the intro
//! to the chorus, sounds the same from any offset; idents are each station's own; the talk
//! station babbles (speech-band syllables) rather than plays; the music dips under a voice.

use super::*;
use pav_core::radio::{StationDef, Voice, at};

const SR: f32 = 22_050.0;

/// PLAN.md §5.12's tempo bands.
const BANDS: [(&str, [f32; 2]); 5] = [
    ("synthwave", [95.0, 118.0]),
    ("boombap", [82.0, 94.0]),
    ("surf", [140.0, 170.0]),
    ("cumbia", [90.0, 105.0]),
    ("smooth", [70.0, 90.0]),
];

fn station(genre: &str, bpm: [f32; 2], seed: u64) -> StationDef {
    StationDef {
        id: "test".into(),
        name: "Test FM".into(),
        freq: "100.0".into(),
        genre: genre.into(),
        bpm,
        seed,
        slot: 210.0,
        title_words: vec![],
        artist_words: vec![],
        dj: String::new(),
    }
}

fn target_of(st: &StationDef, index: u64, offset: f32) -> MusicTarget {
    let (song, _) = at(st, index as f64 * 210.0);
    MusicTarget { station: st.id.clone(), song, offset, volume: 1.0, muffle: 0.0, segments: Vec::new() }
}

fn target(genre: &str, offset: f32) -> MusicTarget {
    let band = BANDS.iter().find(|b| b.0 == genre).map_or([100.0, 110.0], |b| b.1);
    target_of(&station(genre, band, 3), 0, offset)
}

fn mono(x: &[f32]) -> Vec<f32> {
    x.chunks(2).map(|p| 0.5 * (p[0] + p[1])).collect()
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
}

/// Seconds into the slot of the first bar of the first `part` (and that the song has one).
fn start_of(t: &MusicTarget, part: Part) -> f32 {
    let s = sections(t.song.bars).into_iter().find(|s| s.part == part).expect("the song has the part");
    s.start as f32 * 240.0 / t.song.bpm
}

/// The onset envelope (hop `HOP`): the rise of each frame's energy of the high-passed signal.
const HOP: usize = 64;
fn onsets(x: &[f32]) -> Vec<f32> {
    let hp: Vec<f32> = x.windows(2).map(|w| w[1] - w[0]).collect();
    let e: Vec<f32> =
        hp.chunks(HOP).map(|c| (1.0 + 1000.0 * c.iter().map(|v| v * v).sum::<f32>() / c.len() as f32).ln()).collect();
    let mut f = vec![0.0];
    f.extend(e.windows(2).map(|w| (w[1] - w[0]).max(0.0)));
    f
}

/// The autocorrelation of `f` at a fractional lag (in hops), normalized.
fn corr(f: &[f32], lag: f32) -> f32 {
    let (i, k) = (lag.floor() as usize, lag.fract());
    let at = |l: usize| -> f32 { f.iter().zip(&f[l.min(f.len())..]).map(|(a, b)| a * b).sum::<f32>() };
    let zero = at(0).max(1e-9);
    (at(i) * (1.0 - k) + at(i + 1) * k) / zero
}

/// Beats a minute from the onsets' autocorrelation, searched over `band` (± 5).
fn tempo(x: &[f32], band: [f32; 2]) -> f32 {
    let f = onsets(x);
    let hop = HOP as f32 / SR;
    let mut best = (0.0, f32::MIN);
    let mut bpm = band[0] - 5.0;
    while bpm <= band[1] + 5.0 {
        let lag = 60.0 / bpm / hop;
        let score = corr(&f, lag) + 0.5 * corr(&f, lag * 2.0) + 0.5 * corr(&f, lag * 4.0);
        if score > best.1 {
            best = (bpm, score);
        }
        bpm += 0.25;
    }
    best.0
}

/// The onsets folded over the beat: the strongest phase (0..1) within `window`.
fn peak_phase(x: &[f32], offset: f32, spb: f32, window: (f32, f32)) -> f32 {
    let f = onsets(x);
    let hop = HOP as f32 / SR;
    let mut bins = [0.0f32; 96];
    for (i, v) in f.iter().enumerate() {
        // A frame's middle, to the nearest bin.
        let t = offset + (i as f32 + 0.5) * hop;
        let ph = (t / spb).fract();
        bins[(ph * 96.0).round() as usize % 96] += v;
    }
    let (lo, hi) = ((window.0 * 96.0) as usize, (window.1 * 96.0) as usize);
    let k = (lo..hi).max_by(|a, b| bins[*a].total_cmp(&bins[*b])).unwrap();
    k as f32 / 96.0
}

/// Levels by genre, section and part (`cargo test -p pav_audio levels -- --ignored --nocapture`).
#[test]
#[ignore]
fn levels() {
    for (g, _) in BANDS {
        let t = target(g, 0.0);
        for part in [Part::Intro, Part::Verse, Part::Chorus] {
            let at = start_of(&t, part) + 1.0;
            let mut whole = MusicVoice::new(&MusicTarget { offset: at, ..t.clone() }, SR);
            whole.gain = 1.0;
            let x: Vec<f32> = (0..(4.0 * SR) as usize).map(|_| whole.next(1.0 / SR).0).collect();
            let mut parts = Vec::new();
            for (ch, part) in CHANS.iter().enumerate() {
                let mut v = MusicVoice::new(&MusicTarget { offset: at, ..t.clone() }, SR);
                v.gain = 1.0;
                v.solo = Some(ch);
                let y: Vec<f32> = (0..(4.0 * SR) as usize).map(|_| v.next(1.0 / SR).0).collect();
                parts.push(format!("{part:?} {:.3}", rms(&y)));
            }
            let peak = x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            eprintln!("{g} {part:?}: rms {:.3} peak {peak:.2} | {}", rms(&x), parts.join(", "));
        }
        let from = start_of(&t, Part::Chorus);
        let out = mono(&render(&MusicTarget { offset: from, ..t.clone() }, 12.0, SR));
        let f = onsets(&out);
        let hop = HOP as f32 / SR;
        let mut bins = [0.0f32; 24];
        for (i, v) in f.iter().enumerate() {
            let ph = ((from + (i as f32 + 0.5) * hop) / (60.0 / t.song.bpm)).fract();
            bins[(ph * 24.0).round() as usize % 24] += v;
        }
        let top = bins.iter().fold(0.0f32, |m, v| m.max(*v));
        eprintln!("{g} onsets over the beat: {}", bins.iter().map(|b| format!("{:.0}", b / top * 9.0)).collect::<String>());
    }
}

#[test]
fn every_genre_plays_in_range_and_about_as_loud_as_the_others() {
    let mut levels = Vec::new();
    for (g, _) in BANDS {
        let t = target(g, 0.0);
        let at = start_of(&t, Part::Chorus) + 1.0;
        let out = render(&MusicTarget { offset: at, ..t }, 5.0, SR);
        let level = rms(&out);
        let peak = out.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        eprintln!("{g}: rms {level:.3} peak {peak:.3}");
        assert!(out.iter().all(|v| v.is_finite()), "{g}");
        assert!(peak < 1.0, "{g}: clips at {peak}");
        assert!(level > 0.06 && level < 0.35, "{g}: rms {level}");
        levels.push(level);
    }
    let (lo, hi) = levels.iter().fold((f32::MAX, 0.0f32), |(a, b), v| (a.min(*v), b.max(*v)));
    assert!(hi / lo < 2.0, "within 6 dB of each other: {levels:?}");
}

#[test]
fn each_genre_keeps_its_tempo() {
    for (g, band) in BANDS {
        for seed in [3, 4] {
            let t = target_of(&station(g, band, seed), 0, 0.0);
            let from = start_of(&t, Part::Verse);
            let out = mono(&render(&MusicTarget { offset: from, ..t.clone() }, 14.0, SR));
            let bpm = tempo(&out, band);
            eprintln!("{g} seed {seed}: song {} BPM, heard {bpm}", t.song.bpm);
            assert!((band[0]..=band[1]).contains(&t.song.bpm));
            assert!((bpm - t.song.bpm).abs() / t.song.bpm < 0.04, "{g}: {bpm} vs {}", t.song.bpm);
        }
    }
}

#[test]
fn boom_bap_and_smooth_jazz_swing_and_synthwave_does_not() {
    let swing = |g: &str, window: (f32, f32), straight: f32| {
        let t = target(g, 0.0);
        let from = start_of(&t, Part::Chorus);
        let out = mono(&render(&MusicTarget { offset: from, ..t.clone() }, 12.0, SR));
        let p = peak_phase(&out, from, 60.0 / t.song.bpm, window);
        // The off-beat at `p` splits its pair (straight at the middle): the long note's length
        // over the short one's.
        let r = p / (2.0 * straight - p);
        eprintln!("{g}: off-beat at {p:.3} of the beat, swing ratio {r:.2}");
        r
    };
    // Sixteenths: the second one of each pair (straight at 0.25 of a beat).
    assert!(swing("boombap", (0.2, 0.45), 0.25) > 1.3, "boom-bap's sixteenths swing");
    assert!((0.8..1.2).contains(&swing("synthwave", (0.2, 0.4), 0.25)), "synthwave is straight");
    // Eighths: the "and" (straight at 0.5).
    assert!(swing("smooth", (0.45, 0.8), 0.5) > 1.3, "smooth jazz's eighths swing");
}

#[test]
fn a_song_builds_from_its_intro_to_its_chorus() {
    for (g, _) in BANDS {
        let t = target(g, 0.0);
        let intro = rms(&render(&MusicTarget { offset: 1.0, ..t.clone() }, 3.0, SR));
        let chorus = rms(&render(&MusicTarget { offset: start_of(&t, Part::Chorus) + 1.0, ..t.clone() }, 3.0, SR));
        eprintln!("{g}: intro {intro:.3}, chorus {chorus:.3}");
        assert!(chorus > intro * 1.2, "{g}: the chorus is fuller ({chorus} vs {intro})");
    }
}

#[test]
fn a_song_sounds_the_same_from_any_offset() {
    // Starting at 40 s, or at 39 s and listening on, gives the same samples once the filters
    // and echoes settle.
    for (g, _) in BANDS {
        let a = render(&target(g, 40.0), 1.0, SR);
        let b = render(&target(g, 39.0), 2.0, SR);
        let tail = |x: &[f32]| x[x.len() - 4000..].to_vec();
        let d: f32 = tail(&a).iter().zip(tail(&b)).map(|(p, q)| (p - q).abs()).sum::<f32>() / 4000.0;
        assert!(d < 0.02, "{g}: mean difference {d}");
    }
}

#[test]
fn the_ident_is_the_stations_own() {
    let ident = |st: &StationDef, index: u64| {
        let t = target_of(st, index, 0.0);
        let after = t.song.seconds;
        mono(&render(&MusicTarget { offset: after, ..t }, 2.4, SR))
    };
    let similarity = |a: &[f32], b: &[f32]| {
        let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
        dot / (a.iter().map(|x| x * x).sum::<f32>().sqrt() * b.iter().map(|x| x * x).sum::<f32>().sqrt()).max(1e-9)
    };
    for (g, band) in BANDS.iter().filter(|b| b.0 != "boombap") {
        let st = station(g, *band, 3);
        let other = station(g, *band, 77);
        let (a, b, c) = (ident(&st, 0), ident(&st, 5), ident(&other, 0));
        assert!(rms(&a) > 0.03, "{g}: the ident sounds");
        assert!(similarity(&a, &b) > 0.99, "{g}: the same every break");
        assert!(similarity(&a, &c) < 0.9, "{g}: another station's is its own ({})", similarity(&a, &c));
    }
}

fn talk_station() -> MusicTarget {
    let mut t = target_of(&station("talk", [90.0, 90.0], 1040), 0, 0.0);
    let anchor = Voice { pitch: 112.0, formant: 1.0, rate: 4.8, energy: 0.35, radio: 0.2 };
    let ad = Voice { pitch: 190.0, formant: 1.15, rate: 6.0, energy: 0.9, radio: 0.2 };
    t.segments = vec![
        Segment { kind: SegmentKind::Chime, start: 0.0, seconds: 3.2, ..Default::default() },
        Segment { kind: SegmentKind::Voice, start: 3.4, seconds: 8.0, voice: anchor, seed: 1 },
        Segment { kind: SegmentKind::Jingle, start: 12.0, seconds: 9.0, seed: 2, ..Default::default() },
        Segment { kind: SegmentKind::Voice, start: 12.6, seconds: 7.5, voice: ad, seed: 3 },
    ];
    t
}

#[test]
fn the_talk_station_talks_rather_than_plays() {
    let t = talk_station();
    let at = |from: f32, len: f32| mono(&render(&MusicTarget { offset: from, ..t.clone() }, len, SR));
    let chime = at(0.0, 3.0);
    let anchor = at(4.0, 6.5);
    let ad = at(13.0, 6.0);
    let after = at(22.0, 2.0);
    eprintln!("chime {:.3}, anchor {:.3}, ad {:.3}, after {:.4}", rms(&chime), rms(&anchor), rms(&ad), rms(&after));
    assert!(rms(&chime) > 0.03 && rms(&anchor) > 0.04 && rms(&ad) > 0.04, "the sounder, the anchor and the ad all sound");
    assert!(rms(&after) < 0.01, "and nothing plays where nothing is scheduled");
    // Speech-band: little below 90 Hz or above 5 kHz.
    let band = |x: &[f32], hz: f32, low: bool| {
        let mut f = OnePole::default();
        let a = OnePole::coef(hz, 1.0 / SR);
        let y: Vec<f32> = x.iter().map(|v| if low { f.lp(*v, a) } else { f.hp(*v, a) }).collect();
        rms(&y) / rms(x)
    };
    assert!(band(&anchor, 90.0, true) < 0.35, "little low end: {}", band(&anchor, 90.0, true));
    assert!(band(&anchor, 5000.0, false) < 0.3, "little top: {}", band(&anchor, 5000.0, false));
    // Syllables: the loudness rises and falls three to nine times a second.
    let frame = (SR * 0.02) as usize;
    let env: Vec<f32> = anchor.chunks(frame).map(rms).collect();
    let mean = env.iter().sum::<f32>() / env.len() as f32;
    let rate = env.windows(2).filter(|w| (w[0] < mean) != (w[1] < mean)).count() as f32 / 2.0 / 6.5;
    assert!((2.5..9.0).contains(&rate), "{rate} syllables a second");
    // No steady beat: its onsets (the mean taken out) repeat far less than a song's do.
    let periodic = |x: &[f32]| {
        let f = onsets(x);
        let mean = f.iter().sum::<f32>() / f.len() as f32;
        let f: Vec<f32> = f.iter().map(|v| v - mean).collect();
        let hop = HOP as f32 / SR;
        (60..240).step_by(2).map(|bpm| corr(&f, 60.0 / bpm as f32 / hop)).fold(0.0f32, f32::max)
    };
    let song = mono(&render(&target("synthwave", start_of(&target("synthwave", 0.0), Part::Verse)), 6.5, SR));
    eprintln!("beat strength: talk {:.2}, song {:.2}", periodic(&anchor), periodic(&song));
    assert!(periodic(&anchor) < periodic(&song) * 0.8, "talk has no beat");
}

#[test]
fn the_music_dips_under_the_dj() {
    let t = target("synthwave", 0.0);
    let break_at = t.song.seconds;
    let dj = Segment { kind: SegmentKind::Voice, start: break_at + 3.0, seconds: 4.0, voice: Voice::default(), seed: 9 };
    let plain = render(&MusicTarget { offset: break_at + 3.5, ..t.clone() }, 3.0, SR);
    let talked = render(&MusicTarget { offset: break_at + 3.5, segments: vec![dj], ..t.clone() }, 3.0, SR);
    eprintln!("the bed {:.4}, with the DJ {:.4}", rms(&plain), rms(&talked));
    assert!(rms(&talked) > rms(&plain) * 2.0, "the DJ is heard over the bed");
    // Before the DJ starts nothing changes.
    let before = render(&MusicTarget { offset: break_at + 0.5, segments: vec![dj], ..t.clone() }, 2.0, SR);
    assert_eq!(before, render(&MusicTarget { offset: break_at + 0.5, ..t }, 2.0, SR));
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
        (0..60).map(|_| (Vec::new(), Some(pav_core::present::AudioScene { master: 1.0, effects: 1.0, ..Default::default() }))),
    );
    let out = crate::render_run(&ticks, dt, 0.0, SR, &crate::Listener::default());
    let third = out.len() / 3;
    assert!(rms(&out[..third]) > 0.02 && rms(&out[third..2 * third]) > 0.02, "the radio plays on both stations");
    assert!(rms(&out[out.len() - 4000..]) < 0.01, "and fades out when switched off");
}

#[test]
fn the_ident_plays_between_songs() {
    let t = target("synthwave", 0.0);
    let after = t.song.seconds + 0.2;
    let chime = render(&MusicTarget { offset: after, ..t.clone() }, 0.6, SR);
    assert!(rms(&chime) > 0.01, "an ident after the song");
}
