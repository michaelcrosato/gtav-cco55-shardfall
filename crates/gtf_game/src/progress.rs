//! Saving and loading (PLAN.md §5.6.14, §6.10): the save is JSON, versioned — each
//! protagonist's cash, weapons, ammunition, stats, health and whereabouts, who is played,
//! missions passed with their medals and the story's choices, the phone, the clock and the
//! weather. The host keeps it (files beside the executable, the browser's local storage) in
//! three slots and `auto`, written after every mission passed. Settings are kept apart.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, anyhow};
use glam::Vec3;
use pav_core::present::SaveRequest;
use pav_core::sim::Sim;
use serde::{Deserialize, Serialize};

use crate::Freeway;
use crate::missions::Record;
use crate::phone::{Message, Photo};
use crate::protagonists::Member;

/// Completion's parts (each 0..1) and the counts the stats page shows (done, of).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Parts {
    pub story: f32,
    pub activities: f32,
    pub shields: f32,
    pub gold: f32,
    pub story_count: (usize, usize),
    pub shield_count: (usize, usize),
    pub jump_count: (usize, usize),
    pub gold_count: (usize, usize),
}

/// The save's format version.
pub const VERSION: u32 = 1;

/// A saved game.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SaveData {
    pub version: u32,
    /// Bigger is newer (Continue loads the newest).
    pub stamp: u64,
    /// Game seconds played.
    pub playtime: f32,
    pub members: Vec<Member>,
    pub current: usize,
    pub unlocked: bool,
    pub records: BTreeMap<String, Record>,
    pub story: BTreeSet<String>,
    pub hour: f32,
    pub weather: String,
    pub messages: Vec<Message>,
    pub gallery: Vec<Photo>,
    pub resprays: u32,
    /// Completion when saved (the slot list shows it).
    pub completion: f32,
    /// Activities and collectibles done (`activities::Activities::done`).
    #[serde(default)]
    pub activities: BTreeSet<String>,
}

/// A save's line in the slot list: who, when, how far.
pub fn summary(json: &str) -> Option<String> {
    let d: SaveData = serde_json::from_str(json).ok()?;
    let who =
        crate::protagonists::def().ok().and_then(|c| c.protagonist.get(d.current).map(|p| p.short.clone())).unwrap_or_default();
    Some(format!("{who} · {} · {:.0}%", crate::daytime::format_time(d.hour), d.completion * 100.0))
}

impl Freeway {
    /// How much of the game is done (PLAN.md §5.7): story 50% (story missions passed), activities
    /// 30% (activity missions passed, stunt jumps, vigilante, random events), collectibles 10%
    /// (the Lost Shields), medals 10% (gold on every mission with medals); a part with nothing
    /// in it counts as not done.
    pub fn completion(&self) -> f32 {
        let p = self.completion_parts();
        0.5 * p.story + 0.3 * p.activities + 0.1 * p.shields + 0.1 * p.gold
    }

    /// Completion's four parts, each 0..1, and what they count.
    pub fn completion_parts(&self) -> Parts {
        let missions: Vec<_> = crate::missions::all().into_iter().filter(|d| d.district.is_empty()).collect();
        let share = |done: usize, of: usize| if of == 0 { 0.0 } else { done as f32 / of as f32 };
        let story: Vec<_> = missions.iter().filter(|d| !d.activity).collect();
        let story_done = story.iter().filter(|d| self.missions.passed(&d.key)).count();
        let def = crate::activities::def().ok();
        let units = def.as_ref().map(|d| d.units()).unwrap_or_default();
        let jobs: Vec<_> = missions.iter().filter(|d| d.activity).collect();
        let jobs_done = jobs.iter().filter(|d| self.missions.passed(&d.key)).count();
        let units_done = units.iter().filter(|u| self.activities.done.contains(*u)).count();
        let shields = def.as_ref().map(|d| d.shields()).unwrap_or_default();
        let shields_done = shields.iter().filter(|s| self.activities.done.contains(*s)).count();
        let medalled: Vec<_> = missions.iter().filter(|d| !d.medals.is_empty()).collect();
        let gold = medalled
            .iter()
            .filter(|d| self.missions.records.get(&d.key).is_some_and(|r| r.medal == Some(pav_core::present::Medal::Gold)))
            .count();
        let jumps = def.as_ref().map_or(0, |d| d.jump.len());
        Parts {
            story: share(story_done, story.len()),
            activities: share(jobs_done + units_done, jobs.len() + units.len()),
            shields: share(shields_done, shields.len()),
            gold: share(gold, medalled.len()),
            story_count: (story_done, story.len()),
            shield_count: (shields_done, shields.len()),
            jump_count: (self.activities.done.iter().filter(|d| d.starts_with("jump:")).count(), jumps),
            gold_count: (gold, medalled.len()),
        }
    }

    /// The stats page's own cards after the protagonists': completion and the activities.
    pub(crate) fn progress_cards(&self) -> Vec<pav_core::present::Card> {
        use pav_core::Color;
        use pav_core::present::Card;
        let p = self.completion_parts();
        let n = |(a, b): (usize, usize)| format!("{a}/{b}");
        let passed = |k: &str| self.missions.passed(k);
        let events = crate::activities::def().ok().map_or((0, 0), |d| {
            (d.event.iter().filter(|e| self.activities.done.contains(&format!("event:{}", e.id))).count(), d.event.len())
        });
        vec![
            Card {
                id: "completion".into(),
                title: format!("{}%", (self.completion() * 100.0).floor() as i32),
                subtitle: Some("Completion".into()),
                lines: vec![format!("Story missions {}", n(p.story_count)), format!("Gold medals {}", n(p.gold_count))],
                bars: vec![
                    ("Story".into(), p.story),
                    ("Activities".into(), p.activities),
                    ("Lost Shields".into(), p.shields),
                    ("Gold medals".into(), p.gold),
                ],
                accent: Color::hex("#ffc145"),
                icon: Some("trophy".into()),
                enabled: true,
            },
            Card {
                id: "activities".into(),
                title: "ACTIVITIES".into(),
                subtitle: Some("Side jobs and finds".into()),
                lines: vec![
                    format!("Lost Shields {}", n(p.shield_count)),
                    format!("Stunt jumps {}", n(p.jump_count)),
                    format!("Pretzel Run {}", if passed("act_pretzel_run") { "won" } else { "-" }),
                    format!("Hold-ups {}/2", ["act_holdup_gas", "act_holdup_grind"].iter().filter(|k| passed(k)).count()),
                    format!("Vigilante {}", if self.activities.done.contains("vigilante") { "done" } else { "-" }),
                    format!("Random events {}", n(events)),
                ],
                bars: Vec::new(),
                accent: Color::hex("#ff3e8a"),
                icon: Some("jobs".into()),
                enabled: true,
            },
        ]
    }

    /// The game as a save.
    pub fn save_data(&mut self, sim: &Sim) -> SaveData {
        self.stash_member(sim);
        self.save_stamp += 1;
        SaveData {
            version: VERSION,
            stamp: self.save_stamp,
            playtime: self.time,
            members: self.crew.members.clone(),
            current: self.crew.current,
            unlocked: self.crew.unlocked,
            records: self.missions.records.clone(),
            story: self.missions.story.clone(),
            hour: self.sky.hour,
            weather: self.sky.weather.clone(),
            messages: self.phone.messages.clone(),
            gallery: self.phone.gallery.clone(),
            resprays: self.resprays,
            completion: self.completion(),
            activities: self.activities.done.clone(),
        }
    }

    /// Saves into `slot` (the host writes it).
    pub fn save_game(&mut self, sim: &Sim, slot: &str) {
        let data = self.save_data(sim);
        match serde_json::to_string(&data) {
            Ok(json) => {
                self.saves.insert(slot.to_string(), json.clone());
                self.save_serial += 1;
                self.save_req = Some(SaveRequest { serial: self.save_serial, slot: slot.into(), json: json.into() });
            }
            Err(e) => log::error!("save: {e}"),
        }
    }

    /// Loads a save: the crew and their things, the player as who was played where they were,
    /// the missions, the phone, the clock and the weather.
    pub fn load_game(&mut self, sim: &mut Sim, json: &str) -> Result<()> {
        let d: SaveData = serde_json::from_str(json).map_err(|e| anyhow!("unreadable save: {e}"))?;
        if d.version > VERSION {
            return Err(anyhow!("save version {} is newer than this game's {VERSION}", d.version));
        }
        if self.missions.run.is_some() {
            self.end_mission(sim);
        }
        self.save_stamp = self.save_stamp.max(d.stamp);
        self.crew.members = d.members;
        self.crew.unlocked = d.unlocked;
        self.crew.current = d.current.min(self.crew.members.len().saturating_sub(1));
        self.crew.switch = None;
        self.crew.ability = false;
        self.load_member(self.crew.current);
        self.dress_player(sim);
        self.missions.records = d.records;
        self.missions.story = d.story;
        self.activities = crate::activities::Activities { done: d.activities, ..Default::default() };
        self.sky.hour = d.hour;
        if let Ok(world) = crate::daytime::def() {
            let _ = self.sky.set_weather(&world, &d.weather, false);
        }
        self.phone.messages = d.messages;
        self.phone.gallery = d.gallery;
        self.resprays = d.resprays;
        self.heat.clear();
        self.police.clear(sim);
        // Where they were (or home).
        let at = self.crew.me().and_then(|m| m.at).or_else(|| {
            let city = self.city.clone()?;
            let home = crate::protagonists::def().ok()?.protagonist.get(self.crew.current)?.home.clone();
            crate::anchor::Anchor::parse(&home).ok().and_then(|a| crate::anchor::foot(&city, &a))
        });
        if let Some((feet, yaw)) = at {
            self.put_player(sim, feet + Vec3::Y * 0.05, yaw);
        }
        self.front.fade_in(self.ui_time);
        Ok(())
    }

    /// The newest save the host has, if any.
    pub fn newest_save(&self) -> Option<String> {
        self.saves
            .values()
            .filter_map(|j| Some((serde_json::from_str::<SaveData>(j).ok()?.stamp, j)))
            .max_by_key(|x| x.0)
            .map(|x| x.1.clone())
    }
}
