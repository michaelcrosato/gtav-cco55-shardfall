//! M7 (PLAN §7.8): the content. Every file under `content/freeway/` loads and validates, and every
//! name the game and its data refer to exists: props (`LIB/…`) and materials (`MAT/…`) in the
//! embedded build, motion clips, sound cues (the game's bank over the engine's), and the spots and
//! landmarks that missions, activities and the code stand on.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use gtf_game::anchor::{self, Anchor};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every file under `dir` ending in `.ext`, with its text (sorted by path).
fn files(dir: &Path, ext: &str) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap_or_else(|e| panic!("{}: {e}", d.display())).flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == ext) {
                let text = std::fs::read_to_string(&p).unwrap();
                out.push((p, text));
            }
        }
    }
    out.sort();
    out
}

/// Where names are written: the game's data files and its code.
fn sources() -> Vec<(PathBuf, String)> {
    let mut v = files(&root().join("content/freeway"), "toml");
    v.extend(files(&root().join("crates/gtf_game/src"), "rs"));
    v
}

/// Every quoted string in `text` that starts with `prefix`, with the file and line it is on.
fn quoted(path: &Path, text: &str, prefix: &str) -> Vec<(String, String)> {
    let pat = format!("\"{prefix}");
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let mut rest = line;
        while let Some(i) = rest.find(&pat) {
            let s = &rest[i + 1..];
            let end = s.find('"').unwrap_or(s.len());
            out.push((s[..end].to_string(), format!("{}:{}", path.strip_prefix(root()).unwrap_or(path).display(), n + 1)));
            rest = &s[end..];
        }
    }
    out
}

/// The names (with where they are written) starting with `prefix` across the sources, leaving out
/// templates (`{}`) and bare prefixes.
fn named(prefix: &str) -> Vec<(String, String)> {
    let mut seen = BTreeSet::new();
    sources()
        .iter()
        .flat_map(|(p, t)| quoted(p, t, prefix))
        .filter(|(n, _)| n.len() > prefix.len() && !n.contains('{'))
        .filter(|(n, _)| seen.insert(n.clone()))
        .collect()
}

#[test]
fn every_content_file_loads_and_validates() {
    gtf_game::register();
    gtf_game::world::city().expect("district.toml builds");
    gtf_game::world::dev_city().expect("dev_district.toml builds");
    // Every file reads and checks, every mission validates against its district (the same as
    // `play cmd="validate"`); a broken mission file is left out of `all`.
    let mut errs = gtf_game::validate_content();
    assert_eq!(gtf_game::missions::all().len(), gtf_game::missions::FILES.len(), "a mission file does not read");
    // And every TOML file under content/freeway is one of these (no stray file goes unread).
    let known = [
        "activities.toml",
        "attract.toml",
        "dev_district.toml",
        "district.toml",
        "enemies.toml",
        "frontend.toml",
        "peds.toml",
        "protagonists.toml",
        "radio.toml",
        "shops.toml",
        "sounds.toml",
        "traffic.toml",
        "vehicles.toml",
        "views.toml",
        "weapons.toml",
        "world.toml",
    ];
    for (p, _) in files(&root().join("content/freeway"), "toml") {
        let rel = p.strip_prefix(root().join("content/freeway")).unwrap().to_string_lossy().replace('\\', "/");
        let mission = rel.strip_prefix("missions/").and_then(|m| m.strip_suffix(".toml"));
        let listed = match mission {
            Some(k) => gtf_game::missions::FILES.iter().any(|(key, _)| *key == k),
            None => known.contains(&rel.as_str()),
        };
        if !listed {
            errs.push(format!("content/freeway/{rel}: nothing reads it"));
        }
    }
    assert!(errs.is_empty(), "{}", errs.join("\n"));
}

#[test]
fn every_prop_and_material_named_exists() {
    let mut missing = Vec::new();
    let props = named("LIB/");
    for (name, at) in &props {
        if let Err(e) = pav_core::props::resolve_placed(name, &pav_core::props::Params::new()) {
            missing.push(format!("{at}: {name}: {e}"));
        }
    }
    let lib = pav_core::material::Library::current();
    let mats = named("MAT/");
    for (name, at) in &mats {
        // (A tint may follow the name: `MAT/gtf/brick #7a4a36`.)
        let base = name.split_whitespace().next().unwrap_or(name);
        if lib.get(base).is_none() {
            missing.push(format!("{at}: no material {base}"));
        }
    }
    eprintln!("{} props and {} materials named", props.len(), mats.len());
    assert!(props.len() > 20 && mats.len() > 5, "the scan finds the names");
    assert!(missing.is_empty(), "{}", missing.join("\n"));
}

#[test]
fn every_clip_named_exists() {
    let mut missing = Vec::new();
    let mut n = 0;
    for set in ["CMU", "MESH2MOTION", "QUATERNIUS", "STYLE100", "WORKSHOP"] {
        for (name, at) in named(&format!("{set}/")) {
            n += 1;
            if pav_core::clips::find(&name).is_none() {
                missing.push(format!("{at}: no clip {name}"));
            }
        }
    }
    eprintln!("{n} clips named");
    assert!(n > 30, "the scan finds the clips");
    assert!(missing.is_empty(), "{}", missing.join("\n"));
}

/// The cues a bank defines (`[cue."name"]` headers).
fn cues(text: &str) -> BTreeSet<String> {
    text.lines().filter_map(|l| l.trim().strip_prefix("[cue.\"").and_then(|r| r.split('"').next())).map(str::to_string).collect()
}

#[test]
fn every_sound_cue_played_exists() {
    let read = |rel: &str| std::fs::read_to_string(root().join(rel)).unwrap();
    let mut bank = cues(&read("content/freeway/sounds.toml"));
    bank.extend(cues(&read("crates/pav_audio/src/default_bank.toml")));
    // The continuous loops the mixer synthesizes (`"siren.wail" => LoopKind::…`).
    let loops = read("crates/pav_audio/src/loops.rs");
    bank.extend(
        loops
            .lines()
            .filter(|l| l.contains("=> LoopKind::"))
            .filter_map(|l| l.trim().strip_prefix('"')?.split('"').next().map(str::to_string)),
    );
    // Cues are named where the code plays them: `cue: "…"`, and the menus' `sounds.push("…")`.
    let mut played = BTreeSet::new();
    for (p, text) in files(&root().join("crates/gtf_game/src"), "rs") {
        for (n, line) in text.lines().enumerate() {
            if !(line.contains("cue:") || line.contains("sounds.push(")) {
                continue;
            }
            for (name, _) in quoted(&p, line, "") {
                let cue_like = name.contains('.')
                    && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '_');
                if cue_like {
                    played.insert((name, format!("{}:{}", p.strip_prefix(root()).unwrap().display(), n + 1)));
                }
            }
        }
    }
    let missing: Vec<String> =
        played.iter().filter(|(c, _)| !bank.contains(c)).map(|(c, at)| format!("{at}: no cue {c}")).collect();
    eprintln!("{} cues played, {} in the banks", played.len(), bank.len());
    assert!(played.len() > 20, "the scan finds the cues");
    assert!(missing.is_empty(), "{}", missing.join("\n"));
}

#[test]
fn every_spot_and_landmark_named_exists() {
    gtf_game::register();
    let city = gtf_game::world::city().unwrap();
    let dev = gtf_game::world::dev_city().unwrap();
    let mut missing = Vec::new();
    let mut n = 0;
    for prefix in ["spot:", "landmark:"] {
        for (name, at) in named(prefix) {
            n += 1;
            let found = Anchor::parse(&name)
                .ok()
                .is_some_and(|a| anchor::point(&city, &a).is_some() || anchor::point(&dev, &a).is_some());
            if !found {
                missing.push(format!("{at}: {name} is nowhere"));
            }
        }
    }
    // The landmarks the code finds doors of (`door(&city, "…")`).
    for (p, text) in files(&root().join("crates/gtf_game/src"), "rs") {
        for (k, line) in text.lines().enumerate() {
            let Some(i) = line.find("door(") else { continue };
            let Some(key) = quoted(&p, &line[i..], "").first().map(|q| q.0.clone()) else { continue };
            if key.is_empty() || key.contains('{') {
                continue;
            }
            n += 1;
            if gtf_game::world::door(&city, &key).is_none() {
                missing.push(format!("{}:{}: no landmark door '{key}'", p.strip_prefix(root()).unwrap().display(), k + 1));
            }
        }
    }
    eprintln!("{n} spots, landmarks and doors named");
    assert!(n > 30, "the scan finds the places");
    assert!(missing.is_empty(), "{}", missing.join("\n"));
}
