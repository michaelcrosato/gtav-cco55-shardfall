//! First-time tips (PLAN.md §5.9, M9: tutorial help for first-time actions): the first time the
//! player is wanted, holds a gun, has a full ability meter or swims, a help box says what to
//! do — once each, in the help box's place.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// How long a tip stays up (s).
pub const SHOWS: f32 = 9.0;

/// The tips: their keys and what they say.
pub const TIPS: &[(&str, &str)] = &[
    (
        "wanted",
        "The ~b~police~s~ are after you. Get out of the flashing search area and out of their sight to lose them; a respray at Paloma Customs helps.",
    ),
    (
        "gun",
        "Hold ~btn:l2~ to aim (it snaps to the nearest target) and ~btn:r2~ to fire. ~btn:east~ reloads, ~btn:r1~ takes cover.",
    ),
    ("fists", "~btn:east~ throws a punch; hold it for a heavy hook."),
    ("ability", "Your special ability is ready: click ~btn:l3~ and ~btn:r3~ together."),
    ("swim", "You're swimming: push ~btn:left_stick~ to swim, and climb out where the bank is low."),
];

/// Which tips have been shown, and the one up now (part of the game's state).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Tips {
    pub seen: BTreeSet<String>,
    /// The tip up now and how long it has been up (s).
    pub now: Option<(String, f32)>,
}

impl Tips {
    /// Puts tip `key` up if it has never been shown (and no other is up).
    pub fn offer(&mut self, key: &str) {
        if self.now.is_some() || self.seen.contains(key) || !TIPS.iter().any(|t| t.0 == key) {
            return;
        }
        self.seen.insert(key.to_string());
        self.now = Some((key.to_string(), 0.0));
    }

    /// Ages the tip up now; it goes after `SHOWS` seconds.
    pub fn tick(&mut self, dt: f32) {
        if let Some((_, age)) = &mut self.now {
            *age += dt;
            if *age > SHOWS {
                self.now = None;
            }
        }
    }

    /// The tip up now, as the HUD's help: its text and age.
    pub fn help(&self) -> Option<(&'static str, f32)> {
        let (key, age) = self.now.as_ref()?;
        TIPS.iter().find(|t| t.0 == key).map(|t| (t.1, *age))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tip_shows_once_and_goes() {
        let mut t = Tips::default();
        t.offer("gun");
        assert!(t.help().is_some_and(|h| h.0.contains("~btn:l2~")));
        t.offer("wanted");
        assert_eq!(t.now.as_ref().map(|n| n.0.as_str()), Some("gun"), "one at a time");
        t.tick(SHOWS + 0.1);
        assert!(t.help().is_none());
        t.offer("gun");
        assert!(t.help().is_none(), "never twice");
        t.offer("wanted");
        assert!(t.help().is_some());
        t.offer("no such tip");
        assert!(!t.seen.contains("no such tip"));
    }
}
