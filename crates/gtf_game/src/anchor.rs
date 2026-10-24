//! Places in the district as missions and profiles name them (PLAN.md §6.8.6): `spot:<name>`
//! (the district file's named spots), `landmark:<key>[:<point>]` (its door, `centre`, or the
//! spot `<key>_<point>`), `at:<x>,<z>` (world metres), `lane_near:<x>,<z>` (the nearest road),
//! `deck:<x>,<z>` (the Route 404 lane nearest there, up on the deck), `player`, and
//! `actor:<id>` (a mission's actor, which only the mission can resolve). A person is put on the
//! nearest walkable ground, a vehicle on the nearest lane and along it.

use glam::{Vec2, Vec3};
use pav_world::City;

/// A parsed anchor.
#[derive(Clone, Debug, PartialEq)]
pub enum Anchor {
    Spot(String),
    Landmark(String, Option<String>),
    At(Vec2),
    LaneNear(Vec2),
    /// On the freeway's deck: its lane nearest the point, at the deck's height.
    Deck(Vec2),
    Player,
    Actor(String),
}

fn xz(s: &str) -> Option<Vec2> {
    let (x, z) = s.split_once(',')?;
    Some(Vec2::new(x.trim().parse().ok()?, z.trim().parse().ok()?))
}

impl Anchor {
    pub fn parse(s: &str) -> Result<Anchor, String> {
        let bad = || {
            format!(
                "'{s}' is not an anchor (spot:<name>, landmark:<key>[:<point>], at:<x>,<z>, lane_near:<x>,<z>, deck:<x>,<z>, player, actor:<id>)"
            )
        };
        if s == "player" {
            return Ok(Anchor::Player);
        }
        let (kind, rest) = s.split_once(':').ok_or_else(bad)?;
        match kind {
            "spot" if !rest.is_empty() => Ok(Anchor::Spot(rest.into())),
            "actor" if !rest.is_empty() => Ok(Anchor::Actor(rest.into())),
            "landmark" if !rest.is_empty() => Ok(match rest.split_once(':') {
                Some((k, p)) => Anchor::Landmark(k.into(), Some(p.into())),
                None => Anchor::Landmark(rest.into(), None),
            }),
            "at" => xz(rest).map(Anchor::At).ok_or_else(bad),
            "lane_near" => xz(rest).map(Anchor::LaneNear).ok_or_else(bad),
            "deck" => xz(rest).map(Anchor::Deck).ok_or_else(bad),
            _ => Err(bad()),
        }
    }

    /// Whether the district has what this anchor names (actors and the player always pass:
    /// they are checked elsewhere).
    pub fn exists(&self, city: &City) -> bool {
        match self {
            Anchor::Spot(n) => city.spot(n).is_some(),
            Anchor::Landmark(k, p) => {
                city.def.landmark.iter().any(|l| &l.key == k)
                    && p.as_ref().is_none_or(|p| p == "centre" || p == "door" || city.spot(&format!("{k}_{p}")).is_some())
            }
            _ => true,
        }
    }
}

/// The middle of a landmark's cells, on the map.
fn landmark_centre(city: &City, key: &str) -> Option<Vec2> {
    let l = city.def.landmark.iter().find(|l| l.key == key)?;
    let [[x0, z0], [x1, z1]] = l.cells;
    Some((city.cell_centre(glam::IVec2::new(x0, z0)) + city.cell_centre(glam::IVec2::new(x1, z1))) * 0.5)
}

/// The anchor's own point on the map and the facing it gives (None for actors and the player,
/// and for names the district lacks).
pub fn raw(city: &City, a: &Anchor) -> Option<(Vec2, Option<f32>)> {
    match a {
        Anchor::Spot(n) => city.spot(n).map(|(p, yaw)| (p, Some(yaw))),
        Anchor::Landmark(k, None) => crate::world::door(city, k).map(|d| (Vec2::new(d.x, d.z), None)),
        Anchor::Landmark(k, Some(p)) => match p.as_str() {
            "centre" => landmark_centre(city, k).map(|c| (c, None)),
            "door" => crate::world::door(city, k).map(|d| (Vec2::new(d.x, d.z), None)),
            _ => city.spot(&format!("{k}_{p}")).map(|(p, yaw)| (p, Some(yaw))),
        },
        Anchor::At(p) | Anchor::LaneNear(p) | Anchor::Deck(p) => Some((*p, None)),
        Anchor::Player | Anchor::Actor(_) => None,
    }
}

/// The ground under a map point.
pub fn ground(city: &City, p: Vec2) -> Vec3 {
    Vec3::new(p.x, city.ground_y(p), p.y)
}

/// Where a person stands for a map point: the nearest walkable ground (sidewalk, path,
/// crossing, open ground) within 30 m, else the point itself.
pub fn foot_at(city: &City, p: Vec2) -> Vec3 {
    let q = city.walk.nearest_walkable(p, 15).unwrap_or(p);
    ground(city, q)
}

/// Where a vehicle stands for a map point: on the nearest street-level lane and along it (one
/// running within 60° of `yaw` when `facing` says the anchor has one), else the point facing
/// `yaw`.
pub fn car_at(city: &City, p: Vec2, yaw: f32) -> (Vec3, f32) {
    car_facing(city, p, yaw, false)
}

fn car_facing(city: &City, p: Vec2, yaw: f32, facing: bool) -> (Vec3, f32) {
    use pav_world::lanes::LaneKind::*;
    let keep = |l: &pav_world::lanes::Lane| !l.closed && matches!(l.kind, Street | Avenue | Boulevard | Alley);
    let dir = Vec3::new(yaw.sin(), 0.0, yaw.cos());
    let along = |l: &pav_world::lanes::Lane| {
        keep(l) && l.points.len() >= 2 && (l.points[l.points.len() - 1] - l.points[0]).normalize_or_zero().dot(dir) > 0.5
    };
    let at = ground(city, p);
    let lane = if facing { city.lanes.nearest_level(at, 4.0, along) } else { None };
    match lane.or_else(|| city.lanes.nearest_level(at, 4.0, keep)).or_else(|| city.lanes.nearest(at, keep)) {
        Some((lane, along, d)) if d < 40.0 => {
            let (q, dir) = city.lanes.lanes[lane as usize].at(along);
            (q, dir.x.atan2(dir.z))
        }
        _ => (at, yaw),
    }
}

/// The freeway's lane nearest a map point, up on its deck: the point there and the heading.
pub fn deck_at(city: &City, p: Vec2) -> Option<(Vec3, f32)> {
    use pav_world::lanes::LaneKind;
    let (lane, along, d) = city.lanes.nearest(Vec3::new(p.x, 11.0, p.y), |l| l.kind == LaneKind::Freeway)?;
    if d > 60.0 {
        return None;
    }
    let (q, dir) = city.lanes.lanes[lane as usize].at(along);
    Some((q, dir.x.atan2(dir.z)))
}

/// A person's place and facing for a (non-actor) anchor.
pub fn foot(city: &City, a: &Anchor) -> Option<(Vec3, f32)> {
    if let Anchor::Deck(p) = a {
        return deck_at(city, *p);
    }
    let (p, yaw) = raw(city, a)?;
    let feet = foot_at(city, p);
    // Facing: the spot's, else toward the anchor's own point (a landmark's door faces its door).
    let to = Vec2::new(p.x - feet.x, p.y - feet.z);
    Some((feet, yaw.unwrap_or(if to.length() > 0.5 { to.x.atan2(to.y) } else { 0.0 })))
}

/// A vehicle's place and heading for a (non-actor) anchor: `lane_near` and spots are put on the
/// nearest lane; `at` stays exactly where it says.
pub fn car(city: &City, a: &Anchor) -> Option<(Vec3, f32)> {
    if let Anchor::Deck(p) = a {
        return deck_at(city, *p);
    }
    let (p, yaw) = raw(city, a)?;
    Some(match a {
        Anchor::At(_) => (ground(city, p), yaw.unwrap_or(0.0)),
        _ => car_facing(city, p, yaw.unwrap_or(0.0), yaw.is_some()),
    })
}

/// The anchor's point on the ground (destinations, markers, the GPS).
pub fn point(city: &City, a: &Anchor) -> Option<Vec3> {
    if let Anchor::Deck(p) = a {
        return deck_at(city, *p).map(|x| x.0);
    }
    raw(city, a).map(|(p, _)| ground(city, p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchors_parse_and_reject_nonsense() {
        assert_eq!(Anchor::parse("spot:dez_home"), Ok(Anchor::Spot("dez_home".into())));
        assert_eq!(Anchor::parse("landmark:customs:bay"), Ok(Anchor::Landmark("customs".into(), Some("bay".into()))));
        assert_eq!(Anchor::parse("landmark:customs"), Ok(Anchor::Landmark("customs".into(), None)));
        assert_eq!(Anchor::parse("at:12.5,-40"), Ok(Anchor::At(Vec2::new(12.5, -40.0))));
        assert_eq!(Anchor::parse("lane_near:1,2"), Ok(Anchor::LaneNear(Vec2::new(1.0, 2.0))));
        assert_eq!(Anchor::parse("deck:480,-100"), Ok(Anchor::Deck(Vec2::new(480.0, -100.0))));
        assert_eq!(Anchor::parse("player"), Ok(Anchor::Player));
        assert_eq!(Anchor::parse("actor:repo_car"), Ok(Anchor::Actor("repo_car".into())));
        for bad in ["", "spot:", "at:1", "nowhere:3", "at:x,y"] {
            assert!(Anchor::parse(bad).is_err(), "{bad}");
        }
    }
}
