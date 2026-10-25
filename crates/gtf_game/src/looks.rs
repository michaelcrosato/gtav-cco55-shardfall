//! People's looks (PLAN.md §5.11, §6.8.4): the game's biped — believable proportions, a smaller
//! head on a longer neck and longer legs than the engine's default — and the hair, beards,
//! headwear and accessories people are dressed in, drawn from a kind of person's chances (a
//! peds.toml archetype's `wear`, an enemies.toml side's, an activity's look). The picks come
//! from a fork of the sim's rng (`Rng::fork`): dressing someone draws nothing from the main
//! stream, so every draw after a look is the one it was before looks had hair.

use std::collections::BTreeMap;

use pav_core::params::ChoiceParam;
use pav_core::puppet::{BagKind, BeardStyle, BodyPlan, HairStyle, HelmKind, PuppetDef};
use pav_core::rng::Rng;
use serde::Deserialize;

/// The proportions everyone in the game is built on (peds.toml `[body]`), before each
/// person's height and build: metres at scale 1, as `PuppetDef`'s.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Body {
    pub head_radius: f32,
    pub neck: f32,
    pub torso_length: f32,
    pub torso_radius: f32,
    pub shoulder_width: f32,
    pub hip_width: f32,
    pub leg_length: f32,
    pub arm_length: f32,
    pub limb_radius: f32,
    /// How far the eyes lean toward a camera above (0..1): a little, so they stay under the
    /// hairline.
    pub eyes_to_camera: f32,
}

impl Default for Body {
    fn default() -> Self {
        Self {
            head_radius: 0.13,
            neck: 0.12,
            torso_length: 0.47,
            torso_radius: 0.165,
            shoulder_width: 0.21,
            hip_width: 0.1,
            leg_length: 0.9,
            arm_length: 0.62,
            limb_radius: 0.065,
            eyes_to_camera: 0.3,
        }
    }
}

impl Body {
    /// A biped built on these proportions (the engine's colours until dressed).
    pub fn biped(&self) -> PuppetDef {
        PuppetDef {
            head_radius: self.head_radius,
            neck: self.neck,
            torso_length: self.torso_length,
            torso_radius: self.torso_radius,
            shoulder_width: self.shoulder_width,
            hip_width: self.hip_width,
            leg_length: self.leg_length,
            arm_length: self.arm_length,
            limb_radius: self.limb_radius,
            eyes_to_camera: self.eyes_to_camera,
            ..PuppetDef::preset(BodyPlan::Biped)
        }
    }

    /// Standing height at scale 1 (m): feet to crown, as the puppet stands.
    pub fn height(&self) -> f32 {
        self.leg_length * 0.97 + self.torso_length + self.neck + self.head_radius * 1.95
    }
}

/// The game's body (peds.toml `[body]`, its defaults when the file does not read).
pub fn body() -> Body {
    crate::peds::def().map(|d| d.body.clone()).unwrap_or_default()
}

/// The game's biped, undressed.
pub fn person() -> PuppetDef {
    body().biped()
}

/// What a kind of person wears beyond the clothes' colours (a `wear` table). Every key is
/// optional:
/// - `hair`: weights by style (`short = 3, long = 1`; buzz, short, long, bun, ponytail, curly,
///   afro, bald or none); empty: a mix of them all. `hair_color`: the palette (empty: natural
///   colours).
/// - `beard`: the chance of each style (`stubble = 0.1, full = 0.05`: stubble, mustache,
///   goatee, full), at most one; a quarter as likely with long hair, a bun or a ponytail.
/// - `accessories`: the chance of each. Headwear, at most one: `ballcap`, `beanie`, `hardhat`,
///   `helmet` (tactical), `hood` (the shirt's colour); `sunglasses`; a bag, at most one:
///   `backpack`, `shoulder_bag`; `badge`, `vest`, `tie`.
/// - `hats`, `bags`, `vests`, `ties`: colour palettes for those (empty: defaults).
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Wear {
    pub hair: BTreeMap<String, f32>,
    pub hair_color: Vec<String>,
    pub beard: BTreeMap<String, f32>,
    pub accessories: BTreeMap<String, f32>,
    pub hats: Vec<String>,
    pub bags: Vec<String>,
    pub vests: Vec<String>,
    pub ties: Vec<String>,
}

/// The headwear and bag keys of `accessories` (each group gives at most one), in the order
/// their chances are stacked.
const HEADWEAR: [&str; 5] = ["ballcap", "beanie", "hardhat", "helmet", "hood"];
const BAGS: [&str; 2] = ["backpack", "shoulder_bag"];
const OTHER: [&str; 4] = ["sunglasses", "badge", "vest", "tie"];

const HAIR_MIX: [(&str, f32); 8] =
    [("short", 3.0), ("buzz", 1.0), ("long", 1.5), ("ponytail", 1.0), ("bun", 0.6), ("curly", 0.8), ("afro", 0.3), ("bald", 0.5)];
const HAIR_COLORS: [&str; 8] = ["#1c1714", "#2b1d14", "#3a271b", "#5a3b24", "#7a5434", "#8a3f22", "#b88a4a", "#d9c08a"];
const HATS: [&str; 7] = ["#1f2a44", "#c8242c", "#f2f2ee", "#22223b", "#3a5a40", "#e8c21c", "#111111"];
const HARDHATS: [&str; 3] = ["#f2c814", "#f4f4f0", "#f07c1c"];
const BAG_COLORS: [&str; 6] = ["#3e2723", "#212529", "#4a3424", "#1d3557", "#7a1f2b", "#a5a58d"];
const VESTS: [&str; 1] = ["#2a2d33"];
const TIES: [&str; 4] = ["#7a1f2b", "#1d3557", "#2b2d42", "#6a4c93"];
const SHADES: [&str; 4] = ["#111316", "#1b1d22", "#3a2a18", "#20303f"];
const BADGE: &str = "#d9b444";

fn choice<T: ChoiceParam>(name: &str) -> Option<T> {
    T::NAMES.iter().position(|n| *n == name).map(T::from_index)
}

fn pick<'a>(palette: &'a [String], fallback: &'a [&'a str], r: &mut Rng) -> String {
    if palette.is_empty() {
        fallback[r.below(fallback.len() as u32) as usize].to_string()
    } else {
        palette[r.below(palette.len() as u32) as usize].clone()
    }
}

/// One of `keys` by their stacked chances in `table` (scaled by `k`), or None.
fn one_of<'a>(table: &BTreeMap<String, f32>, keys: impl IntoIterator<Item = &'a str>, k: f32, r: &mut Rng) -> Option<&'a str> {
    let roll = r.f32();
    let mut sum = 0.0;
    for key in keys {
        sum += table.get(key).copied().unwrap_or(0.0) * k;
        if roll < sum {
            return Some(key);
        }
    }
    None
}

impl Wear {
    /// What is wrong with it: unknown styles or accessories, chances outside 0..1 or adding up
    /// past 1 in a group, colours that are not `#rrggbb`.
    pub fn check(&self) -> Result<(), String> {
        for (style, w) in &self.hair {
            if choice::<HairStyle>(style).is_none() {
                return Err(format!("hair style '{style}' (one of {})", HairStyle::NAMES.join(", ")));
            }
            if *w < 0.0 {
                return Err(format!("hair '{style}': a weight below 0"));
            }
        }
        for style in self.beard.keys() {
            if choice::<BeardStyle>(style).is_none_or(|b| b == BeardStyle::None) {
                return Err(format!("beard style '{style}' (stubble, mustache, goatee or full)"));
            }
        }
        for key in self.accessories.keys() {
            if !HEADWEAR.contains(&key.as_str()) && !BAGS.contains(&key.as_str()) && !OTHER.contains(&key.as_str()) {
                let all: Vec<&str> = HEADWEAR.iter().chain(&BAGS).chain(&OTHER).copied().collect();
                return Err(format!("accessory '{key}' (one of {})", all.join(", ")));
            }
        }
        let sum = |table: &BTreeMap<String, f32>, keys: &[&str]| keys.iter().filter_map(|k| table.get(*k)).sum::<f32>();
        let beards: f32 = self.beard.values().sum();
        for (what, total) in
            [("headwear", sum(&self.accessories, &HEADWEAR)), ("bag", sum(&self.accessories, &BAGS)), ("beard", beards)]
        {
            if total > 1.0 + 1e-4 {
                return Err(format!("{what} chances add up to {total} (at most 1)"));
            }
        }
        if let Some((k, c)) = self.accessories.iter().chain(&self.beard).find(|(_, c)| !(0.0..=1.0).contains(*c)) {
            return Err(format!("'{k}': chance {c} (0 to 1)"));
        }
        let colours = self.hair_color.iter().chain(&self.hats).chain(&self.bags).chain(&self.vests).chain(&self.ties);
        if let Some(c) = colours.into_iter().find(|c| pav_core::Color::try_hex(c).is_none()) {
            return Err(format!("colour '{c}' is not #rrggbb"));
        }
        Ok(())
    }

    /// Dresses `p` (its clothes' colours already chosen): hair, beard, headwear, sunglasses, a
    /// bag, badge, vest and tie by their chances, drawn from a fork of `rng`.
    pub fn dress(&self, p: &mut PuppetDef, rng: &Rng) {
        let r = &mut rng.fork(0x0100_6b5f_3a11);
        let mix: Vec<(&str, f32)> =
            if self.hair.is_empty() { HAIR_MIX.to_vec() } else { self.hair.iter().map(|(s, w)| (s.as_str(), *w)).collect() };
        let total: f32 = mix.iter().map(|(_, w)| w).sum();
        let mut roll = r.f32() * total;
        let style = mix.iter().find(|(_, w)| {
            roll -= w;
            roll < 0.0
        });
        p.hair = style.and_then(|(s, _)| choice(s)).unwrap_or_default();
        p.hair_color = pick(&self.hair_color, &HAIR_COLORS, r);
        let tied = matches!(p.hair, HairStyle::Long | HairStyle::Bun | HairStyle::Ponytail);
        let beards: Vec<&str> = self.beard.keys().map(String::as_str).collect();
        p.beard = one_of(&self.beard, beards, if tied { 0.25 } else { 1.0 }, r).and_then(choice).unwrap_or_default();
        let a = &self.accessories;
        let chance = |key: &str, r: &mut Rng| r.f32() < a.get(key).copied().unwrap_or(0.0);
        match one_of(a, HEADWEAR, 1.0, r) {
            Some("ballcap") => (p.gear.helm, p.gear.helm_color) = (HelmKind::Ballcap, pick(&self.hats, &HATS, r)),
            Some("beanie") => (p.gear.helm, p.gear.helm_color) = (HelmKind::Cap, pick(&self.hats, &HATS, r)),
            Some("hardhat") => (p.gear.helm, p.gear.helm_color) = (HelmKind::Hardhat, pick(&self.hats, &HARDHATS, r)),
            Some("helmet") => (p.gear.helm, p.gear.helm_color) = (HelmKind::Tactical, pick(&self.hats, &["#23272e"], r)),
            Some("hood") => (p.gear.helm, p.gear.helm_color) = (HelmKind::Hood, p.shirt.clone()),
            _ => {}
        }
        if chance("sunglasses", r) {
            p.gear.glasses = pick(&[], &SHADES, r);
        }
        match one_of(a, BAGS, 1.0, r) {
            Some("backpack") => (p.gear.bag, p.gear.bag_color) = (BagKind::Backpack, pick(&self.bags, &BAG_COLORS, r)),
            Some("shoulder_bag") => (p.gear.bag, p.gear.bag_color) = (BagKind::Shoulder, pick(&self.bags, &BAG_COLORS, r)),
            _ => {}
        }
        if chance("badge", r) {
            p.gear.badge = BADGE.into();
        }
        if chance("vest", r) {
            p.gear.vest = pick(&self.vests, &VESTS, r);
        }
        if chance("tie", r) {
            p.gear.tie = pick(&self.ties, &TIES, r);
        }
    }
}

/// A style or colour named in a fixed look (protagonists.toml): None for an empty name, an
/// error for an unknown one.
pub fn named<T: ChoiceParam>(what: &str, name: Option<&str>) -> Result<Option<T>, String> {
    match name {
        None | Some("") => Ok(None),
        Some(n) => choice(n).map(Some).ok_or_else(|| format!("{what} '{n}' (one of {})", T::NAMES.join(", "))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wear(text: &str) -> Wear {
        toml::from_str(text).unwrap()
    }

    #[test]
    fn the_body_stands_as_tall_as_its_parts_and_believably_proportioned() {
        let b = Body::default();
        let def = b.biped();
        let parts = pav_core::puppet::pose(&def, &Default::default(), None, glam::Vec3::ZERO, glam::Vec3::NEG_Z);
        let top = parts.iter().map(|p| p.a.y.max(p.b.y) + p.ra.max(p.rb)).fold(0.0, f32::max);
        assert!((top - b.height()).abs() < 0.03, "stands {top} m, computed {}", b.height());
        // Smaller head, longer legs than the engine's default.
        let engine = PuppetDef::default();
        assert!(def.head_radius < engine.head_radius * 0.7 && def.leg_length > engine.leg_length);
        assert!((1.6..1.8).contains(&b.height()));
    }

    #[test]
    fn dressing_follows_the_chances_and_draws_nothing_from_the_stream() {
        let w = wear(
            r##"hair = { long = 1, buzz = 1 }
            hair_color = ["#123456"]
            beard = { full = 1.0 }
            accessories = { ballcap = 0.5, hood = 0.5, sunglasses = 1.0, backpack = 1.0, badge = 0.0 }
            hats = ["#c8242c"]"##,
        );
        w.check().unwrap();
        let mut rng = Rng::new(3);
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..60 {
            let before = rng.clone().next_u32();
            let mut p = person();
            p.shirt = "#445566".into();
            w.dress(&mut p, &rng);
            assert_eq!(rng.clone().next_u32(), before, "the stream is where it was");
            rng.next_u32();
            assert!(matches!(p.hair, HairStyle::Long | HairStyle::Buzz) && p.hair_color == "#123456");
            assert!(p.gear.glasses.len() == 7 && p.gear.bag == BagKind::Backpack && p.gear.badge.is_empty());
            match p.gear.helm {
                HelmKind::Ballcap => assert_eq!(p.gear.helm_color, "#c8242c"),
                HelmKind::Hood => assert_eq!(p.gear.helm_color, "#445566", "a hood is the shirt's"),
                other => panic!("{other:?}"),
            }
            seen.insert((p.hair as u8, p.gear.helm as u8, p.beard as u8));
        }
        assert!(seen.len() >= 6, "the picks vary: {seen:?}");
        // The same state dresses alike (replays).
        let (mut a, mut b) = (person(), person());
        w.dress(&mut a, &rng);
        w.dress(&mut b, &rng);
        assert_eq!(a, b);
    }

    #[test]
    fn the_crowd_wears_what_its_kinds_wear() {
        let d = crate::peds::def().unwrap();
        let mut rng = Rng::new(11);
        let mut styles = std::collections::BTreeSet::new();
        let (mut hats, mut shades, mut bags) = (0, 0, 0);
        for a in &d.archetype {
            for _ in 0..30 {
                let p = crate::peds::look(a, &mut rng);
                assert!((p.head_radius - d.body.head_radius).abs() < 1e-6 && p.neck > 0.0, "{}: the game's body", a.id);
                styles.insert(p.hair as u8);
                hats += usize::from(p.gear.helm != HelmKind::None);
                shades += usize::from(!p.gear.glasses.is_empty());
                bags += usize::from(p.gear.bag != BagKind::None);
            }
        }
        let n = d.archetype.len() * 30;
        assert!(styles.len() >= 7, "hair styles: {styles:?}");
        for (what, count) in [("headwear", hats), ("sunglasses", shades), ("bags", bags)] {
            assert!(count * 20 > n && count * 2 < n, "{what}: {count} of {n}");
        }
        let worker = d.archetype.iter().find(|a| a.id == "worker").unwrap();
        let hard = (0..60).filter(|_| crate::peds::look(worker, &mut rng).gear.helm == HelmKind::Hardhat).count();
        assert!((15..50).contains(&hard), "hard hats on {hard} of 60 workers");
    }

    #[test]
    fn officers_crews_and_the_protagonists_are_dressed_for_the_part() {
        let mut sim = pav_core::Sim::empty(1);
        for _ in 0..6 {
            let cop = crate::police::uniform(&mut sim);
            assert_eq!(cop.gear.helm, HelmKind::Ballcap, "a cap");
            assert!(!cop.gear.badge.is_empty() && cop.shirt == "#1d2b4f", "a badge on the navy uniform");
            let swat = crate::enemies::side_look(&mut sim, "swat").unwrap();
            assert!(swat.gear.helm == HelmKind::Tactical && !swat.gear.vest.is_empty(), "a helmet and a vest");
        }
        let crew = crate::protagonists::def().unwrap();
        let by = |k: &str| crew.protagonist.iter().find(|p| p.key == k).unwrap().puppet();
        let (dez, mack, roxy) = (by("dez"), by("mack"), by("roxy"));
        assert!(dez.gear.helm == HelmKind::Ballcap && dez.beard == BeardStyle::Goatee);
        assert!(!mack.gear.glasses.is_empty() && mack.hair == HairStyle::Short);
        assert!(roxy.hair == HairStyle::Ponytail && roxy.gear.helm == HelmKind::None);
        assert!(dez.scale < mack.scale && mack.torso_radius > dez.torso_radius, "heights and builds");
    }

    #[test]
    fn mistakes_in_a_wear_table_are_named() {
        assert!(wear("hair = { mullet = 1 }").check().unwrap_err().contains("mullet"));
        assert!(wear("accessories = { monocle = 0.2 }").check().unwrap_err().contains("monocle"));
        assert!(wear("accessories = { ballcap = 0.7, beanie = 0.6 }").check().unwrap_err().contains("headwear"));
        assert!(wear("beard = { full = 1.5 }").check().is_err());
        assert!(wear(r#"hats = ["red"]"#).check().unwrap_err().contains("red"));
        assert!(toml::from_str::<Wear>("hat = []").is_err(), "unknown keys are refused");
    }
}
