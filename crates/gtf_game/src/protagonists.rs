//! The three protagonists (PLAN.md §5.4): who they are and what the stats page shows. Their
//! switching, abilities and life spots arrive in M6.

use pav_core::Color;
use pav_core::present::{Card, CardRow};

/// Key, name, ability, accent colour, cash, and stats (driving, shooting, stamina, stealth).
pub const CREW: [(&str, &str, &str, &str, i64, [f32; 4]); 3] = [
    ("dez", "DEZ", "Slipstream", "#6fe28a", 12_480, [0.9, 0.45, 0.6, 0.5]),
    ("mack", "MACK", "Deadeye", "#3d8bff", 48_020, [0.55, 0.85, 0.4, 0.7]),
    ("roxy", "ROXY", "Overdrive", "#ff3e8a", 7_915, [0.7, 0.6, 0.9, 0.35]),
];

fn cash(v: i64) -> String {
    let s = v.abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    format!("{}${out}", if v < 0 { "-" } else { "" })
}

/// The pause menu's stats page: one big card per protagonist (no tables).
pub fn stat_cards(focus: usize) -> CardRow {
    CardRow {
        cards: CREW
            .iter()
            .map(|(key, name, ability, hex, money, stats)| Card {
                id: (*key).into(),
                title: (*name).into(),
                subtitle: Some((*ability).into()),
                lines: vec![cash(*money)],
                bars: ["Driving", "Shooting", "Stamina", "Stealth"].iter().zip(stats).map(|(l, v)| (l.to_string(), *v)).collect(),
                accent: Color::hex(hex),
                icon: None,
                enabled: true,
            })
            .collect(),
        focus: focus.min(CREW.len() - 1),
    }
}

/// Money as the HUD writes it.
pub fn money_text(v: i64) -> String {
    cash(v)
}

#[cfg(test)]
mod tests {
    #[test]
    fn money_has_thousands_separators() {
        assert_eq!(super::money_text(12_480), "$12,480");
        assert_eq!(super::money_text(-1_000_000), "-$1,000,000");
        assert_eq!(super::money_text(999), "$999");
    }
}
