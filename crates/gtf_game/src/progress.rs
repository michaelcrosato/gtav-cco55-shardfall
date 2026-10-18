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
}

/// A save's line in the slot list: who, when, how far.
pub fn summary(json: &str) -> Option<String> {
    let d: SaveData = serde_json::from_str(json).ok()?;
    let who =
        crate::protagonists::def().ok().and_then(|c| c.protagonist.get(d.current).map(|p| p.short.clone())).unwrap_or_default();
    Some(format!("{who} · {} · {:.0}%", crate::daytime::format_time(d.hour), d.completion * 100.0))
}

impl Freeway {
    /// How much of the game is done (PLAN.md §5.7): story 50%, activities 30%, collectibles 10%,
    /// medals 10% (parts with nothing in them yet count as not done).
    pub fn completion(&self) -> f32 {
        let story: Vec<_> = crate::missions::all().into_iter().filter(|d| d.district.is_empty()).collect();
        let n = story.len().max(1) as f32;
        let passed = story.iter().filter(|d| self.missions.passed(&d.key)).count() as f32;
        let gold = story
            .iter()
            .filter(|d| self.missions.records.get(&d.key).is_some_and(|r| r.medal == Some(pav_core::present::Medal::Gold)))
            .count() as f32;
        0.5 * passed / n + 0.1 * gold / n
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
