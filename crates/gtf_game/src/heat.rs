//! The wanted level (PLAN.md §5.6.5). A crime has a star value; police who see it raise the
//! stars at once, civilians who see it (in sight, within 30 m) run and phone it in 4–8 s later
//! unless they are aimed at, killed or out of range when the call ends. Unreported crimes add no
//! heat. While an officer sees the player the stars are solid and the last known position
//! follows the player (sightings are shared by radio); two seconds unseen and the stars flash: a
//! search zone opens on the last sighting (its radius by level). Leaving the zone and staying
//! unseen for 5 + 5 × stars seconds clears the heat — staying out of their sight inside it, twice
//! that (a hide spot, which halves how far they see, helps with that). Missions may floor or cap
//! the level and switch dispatch off.

use glam::{Vec2, Vec3};
use pav_core::entity::EntityId;
use pav_core::sim::Sim;
use serde::{Deserialize, Serialize};

/// The search zone's radius at each level (0–5 stars).
pub const ZONE: [f32; 6] = [0.0, 120.0, 160.0, 220.0, 300.0, 400.0];
/// Stars flash (searching) after this long unseen (s).
pub const SEARCH_AFTER: f32 = 2.0;
/// Civilians see crimes this far away, police this far.
pub const CIVILIAN_SIGHT: f32 = 30.0;
pub const POLICE_SIGHT: f32 = 45.0;

/// A crime the player commits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Crime {
    Assault,
    CarTheft,
    Brandish,
    Vehicular,
    Shooting,
    KillCivilian,
    HoldUp,
    AttackPolice,
    KillPolice,
    DestroyPolice,
}

impl Crime {
    /// Its star value (§5.6.5).
    pub fn stars(self) -> u8 {
        match self {
            Crime::Assault | Crime::CarTheft | Crime::Brandish | Crime::Vehicular => 1,
            Crime::Shooting | Crime::KillCivilian | Crime::HoldUp => 2,
            Crime::AttackPolice => 3,
            Crime::KillPolice | Crime::DestroyPolice => 4,
        }
    }

    /// What the report says (the toast).
    pub fn reason(self) -> &'static str {
        match self {
            Crime::Assault => "Assault reported",
            Crime::CarTheft => "Vehicle theft reported",
            Crime::Brandish => "Armed suspect",
            Crime::Vehicular => "Hit and run",
            Crime::Shooting => "Shots fired",
            Crime::KillCivilian => "Homicide reported",
            Crime::HoldUp => "Armed robbery",
            Crime::AttackPolice => "Officer under attack",
            Crime::KillPolice => "Officer down",
            Crime::DestroyPolice => "Police vehicle destroyed",
        }
    }
}

/// A civilian on the phone to the police about a crime.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub reporter: EntityId,
    pub crime: Crime,
    pub at: Vec3,
    /// Seconds until the call goes through.
    pub left: f32,
}

/// The player's heat (part of the game's state).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Heat {
    pub stars: u8,
    /// Missions: the level never drops below `floor` nor rises above `cap`; `dispatch` off sends
    /// nobody.
    pub floor: u8,
    pub cap: u8,
    pub dispatch: bool,
    /// Where the police last saw the player, seconds since, and searching (the stars flash).
    pub last_known: Vec3,
    pub unseen: f32,
    pub searching: bool,
    /// Seconds unseen counting toward evading.
    pub evade: f32,
    pub reports: Vec<Report>,
    /// The latest report's reason, for the toast (taken by the game).
    pub news: Option<String>,
    /// Seconds wanted (stats) and the highest level reached.
    pub wanted_for: f32,
    pub peak: u8,
    pub cleared: u32,
    /// The car the police last saw the player in (another, taken while unseen, drops a star).
    pub seen_car: Option<EntityId>,
}

impl Default for Heat {
    fn default() -> Self {
        Heat {
            stars: 0,
            floor: 0,
            cap: 5,
            dispatch: true,
            last_known: Vec3::ZERO,
            unseen: 0.0,
            searching: false,
            evade: 0.0,
            reports: Vec::new(),
            news: None,
            wanted_for: 0.0,
            peak: 0,
            cleared: 0,
            seen_car: None,
        }
    }
}

/// Whether `from` can see `to` (nothing fixed in between).
pub fn sight(sim: &Sim, from: Vec3, to: Vec3) -> bool {
    let d = to - from;
    let len = d.length();
    len < 0.5 || sim.raycast_static(from, d / len, len - 0.3).is_none()
}

impl Heat {
    /// Sets the level to at least `stars` (within the mission's floor and cap), the police
    /// knowing the player is at `at`.
    pub fn raise(&mut self, stars: u8, at: Vec3) {
        let s = stars.max(self.floor).min(self.cap.max(self.floor)).min(5);
        if s > self.stars {
            self.stars = s;
        }
        self.peak = self.peak.max(self.stars);
        self.last_known = at;
        self.evade = 0.0;
    }

    /// A crime at `at`. `police` saw it (an officer in sight) — the level rises now; otherwise
    /// the `witnesses` (civilians in sight) start their calls.
    pub fn crime(&mut self, sim: &mut Sim, crime: Crime, at: Vec3, police: bool, witnesses: &[EntityId]) {
        if police {
            // Already wanted: a crime as serious as the level, in front of the police, escalates
            // it (running someone over at 1★ makes 2★; at 3★ it takes attacking an officer).
            let s = if self.stars > 0 && crime.stars() == self.stars { (self.stars + 1).min(5) } else { crime.stars() };
            self.raise(s, at);
            self.unseen = 0.0;
            self.searching = false;
            self.news = Some(crime.reason().into());
            return;
        }
        for &w in witnesses {
            if self.reports.iter().any(|r| r.reporter == w) {
                continue;
            }
            let left = sim.state.rng.range(4.0, 8.0);
            self.reports.push(Report { reporter: w, crime, at, left });
        }
    }

    /// The civilians near `at` who can see it: (id) of those within sight range with a clear
    /// line, alive.
    pub fn witnesses(sim: &Sim, people: &[EntityId], at: Vec3) -> Vec<EntityId> {
        people
            .iter()
            .copied()
            .filter(|&id| {
                let Some(e) = sim.state.entities.get(id) else { return false };
                if e.name.starts_with("dead:") || e.name.starts_with("cop:") {
                    return false;
                }
                let eye = e.pos + Vec3::Y * 0.7;
                eye.distance(at) < CIVILIAN_SIGHT && sight(sim, eye, at + Vec3::Y)
            })
            .collect()
    }

    /// One tick: calls in progress, the police's sight of the player, search and evasion. `seen`
    /// is whether an officer sees the player now. `aimed` is who the player aims at (it cancels
    /// their call). Returns true when the heat was cleared this tick.
    pub fn tick(&mut self, sim: &Sim, player: Vec3, seen: bool, aimed: Option<EntityId>, dt: f32) -> bool {
        // Calls: cancelled by death, intimidation or wandering off; else the stars rise.
        let mut landed = Vec::new();
        self.reports.retain_mut(|r| {
            let Some(e) = sim.state.entities.get(r.reporter) else { return false };
            if e.name.starts_with("dead:") || aimed == Some(r.reporter) {
                return false;
            }
            r.left -= dt;
            if r.left > 0.0 {
                return true;
            }
            if e.pos.distance(r.at) < 90.0 {
                landed.push(*r);
            }
            false
        });
        for r in landed {
            let s = r.crime.stars();
            if s > self.stars {
                self.raise(s, r.at);
                // They know where it happened, not where the player went.
                self.searching = true;
                self.unseen = SEARCH_AFTER;
            }
            self.news = Some(r.crime.reason().into());
        }
        if self.stars == 0 {
            self.searching = false;
            self.unseen = 0.0;
            return false;
        }
        self.wanted_for += dt;
        if seen {
            self.unseen = 0.0;
            self.searching = false;
            self.last_known = player;
            self.evade = 0.0;
            return false;
        }
        self.unseen += dt;
        if self.unseen >= SEARCH_AFTER {
            self.searching = true;
        }
        if !self.searching {
            // Recently seen: not searching yet (the last sighting stands).
            return false;
        }
        let r = ZONE[self.stars as usize];
        let outside = Vec2::new(player.x - self.last_known.x, player.z - self.last_known.z).length() > r;
        // Outside the zone the clock runs at full speed; out of their sight inside it, at half.
        self.evade += if outside { dt } else { dt * 0.5 };
        let needed = 5.0 + 5.0 * self.stars as f32;
        if self.evade >= needed && self.floor == 0 {
            self.clear();
            self.cleared += 1;
            return true;
        }
        false
    }

    /// The police lose sight of the player at `at` (a mission's getaway): the search starts
    /// there, as after two seconds unseen.
    pub fn lose_sight(&mut self, at: Vec3) {
        if self.stars > 0 {
            self.last_known = at;
            self.unseen = self.unseen.max(SEARCH_AFTER);
            self.searching = true;
            self.evade = 0.0;
        }
    }

    /// Back to no heat (evaded, busted, wasted, a respray).
    pub fn clear(&mut self) {
        self.stars = self.floor;
        self.searching = false;
        self.unseen = 0.0;
        self.evade = 0.0;
        self.reports.clear();
        self.seen_car = None;
    }

    /// Into `car` while unseen: if the police are looking for another, the level drops a star
    /// (true when it did).
    pub fn swapped_car(&mut self, car: EntityId) -> bool {
        if self.stars == 0 || !self.searching || self.seen_car.is_none_or(|c| c == car) {
            return false;
        }
        self.stars = self.stars.saturating_sub(1).max(self.floor);
        self.seen_car = None;
        self.news = Some("Suspect vehicle not found".into());
        true
    }

    /// The search zone (centre, radius) while searching.
    pub fn zone(&self) -> Option<(Vec3, f32)> {
        (self.searching && self.stars > 0).then(|| (self.last_known, ZONE[self.stars as usize]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_raise_flash_and_clear() {
        let mut h = Heat::default();
        h.raise(2, Vec3::ZERO);
        assert_eq!(h.stars, 2);
        h.raise(1, Vec3::ZERO);
        assert_eq!(h.stars, 2, "a lesser crime does not lower it");
        let sim = Sim::empty(1);
        // Unseen for two seconds: searching; then far outside the zone for 15 s: cleared.
        for _ in 0..130 {
            h.tick(&sim, Vec3::ZERO, false, None, 1.0 / 60.0);
        }
        assert!(h.searching);
        let far = Vec3::new(500.0, 0.0, 0.0);
        let mut cleared = false;
        for _ in 0..(16 * 60) {
            cleared |= h.tick(&sim, far, false, None, 1.0 / 60.0);
        }
        assert!(cleared && h.stars == 0, "evaded: {h:?}");
        // A mission's cap.
        h.cap = 2;
        h.raise(4, Vec3::ZERO);
        assert_eq!(h.stars, 2);
    }

    #[test]
    fn a_getaway_the_police_lost_sight_of_starts_the_search_where_the_player_is() {
        let mut h = Heat::default();
        let at = Vec3::new(60.0, 0.0, 360.0);
        h.lose_sight(at);
        assert!(!h.searching, "no heat, no search");
        h.raise(1, Vec3::ZERO);
        h.lose_sight(at);
        assert!(h.searching && h.last_known == at && h.zone() == Some((at, ZONE[1])));
        // Out of the zone and unseen for 10 s at one star: cleared.
        let sim = Sim::empty(1);
        let far = at + Vec3::new(200.0, 0.0, 0.0);
        let cleared = (0..(11 * 60)).any(|_| h.tick(&sim, far, false, None, 1.0 / 60.0));
        assert!(cleared && h.stars == 0, "{h:?}");
    }

    #[test]
    fn out_of_sight_inside_the_zone_the_heat_clears_in_twice_the_time() {
        let mut h = Heat::default();
        h.raise(2, Vec3::ZERO);
        let sim = Sim::empty(1);
        let mut t = 0;
        let mut cleared = false;
        while !cleared && t < 120 * 60 {
            cleared = h.tick(&sim, Vec3::new(20.0, 0.0, 0.0), false, None, 1.0 / 60.0);
            t += 1;
        }
        // Two seconds to start the search, then twice 5 + 5 × 2.
        let secs = t as f32 / 60.0;
        assert!(cleared && (secs - (SEARCH_AFTER + 30.0)).abs() < 0.2, "cleared after {secs:.1} s");
    }
}
