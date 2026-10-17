//! The GPS (PLAN.md §5.9): a route over the lane graph from the player to the waypoint (or a
//! mission's target), redone when they stray from it, shown as a ribbon on the road ahead and as
//! a line on the minimap and the pause map. The route is derived from the city, the target and
//! where the player is, so it is a cache outside the game's state.

use glam::{Vec2, Vec3};
use pav_core::Color;
use pav_core::present::RibbonDraw;
use pav_world::City;

/// Leaving the route by this much (m) asks for a new one.
const STRAY: f32 = 18.0;

#[derive(Clone, Debug, Default)]
pub struct Gps {
    target: Option<Vec2>,
    /// The route's points from where it was planned to the target.
    points: Vec<Vec3>,
}

impl Gps {
    /// Points the GPS at `target` (none: off).
    pub fn set(&mut self, target: Option<Vec2>) {
        if self.target != target {
            self.target = target;
            self.points.clear();
        }
    }

    pub fn target(&self) -> Option<Vec2> {
        self.target
    }

    /// Plans (or re-plans) the route when there is none or the player has left it.
    pub fn update(&mut self, city: &City, player: Vec3) {
        let Some(t) = self.target else {
            self.points.clear();
            return;
        };
        if !self.points.is_empty() && self.distance(player).0 < STRAY {
            return;
        }
        let goal = Vec3::new(t.x, city.ground_y(t), t.y);
        let lanes = city.lanes.route(player, goal);
        let mut pts = city.lanes.route_points(&lanes);
        pts.push(goal);
        self.points = pts;
    }

    /// Distance from `p` to the route and the index of the nearest segment.
    fn distance(&self, p: Vec3) -> (f32, usize) {
        let mut best = (f32::MAX, 0);
        for (i, w) in self.points.windows(2).enumerate() {
            let (a, b) = (Vec2::new(w[0].x, w[0].z), Vec2::new(w[1].x, w[1].z));
            let q = Vec2::new(p.x, p.z);
            let ab = b - a;
            let t = if ab.length_squared() > 0.0 { ((q - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
            let d = (a + ab * t).distance(q);
            if d < best.0 {
                best = (d, i);
            }
        }
        best
    }

    /// The route from the player onward, as map points (x, z), thinned.
    pub fn minimap(&self, player: Vec3) -> Vec<Vec2> {
        if self.points.len() < 2 {
            return Vec::new();
        }
        let (_, i) = self.distance(player);
        let mut out = vec![Vec2::new(player.x, player.z)];
        for p in &self.points[i + 1..] {
            let q = Vec2::new(p.x, p.z);
            if out.last().is_none_or(|l| l.distance(q) > 6.0) {
                out.push(q);
            }
        }
        out
    }

    /// The ribbon on the road ahead of the player (the next `ahead` metres).
    pub fn ribbon(&self, player: Vec3, ahead: f32) -> Option<RibbonDraw> {
        if self.points.len() < 2 {
            return None;
        }
        let (_, i) = self.distance(player);
        let mut pts = Vec::new();
        let mut run = 0.0;
        let mut last: Option<Vec3> = None;
        for p in &self.points[i..] {
            if let Some(l) = last {
                run += l.distance(*p);
                if run > ahead {
                    break;
                }
            }
            pts.push(*p + Vec3::Y * 0.06);
            last = Some(*p);
        }
        (pts.len() >= 2).then(|| RibbonDraw { points: pts, width: 1.1, color: Color::hex("#ff3ea5"), emissive: 0.6 })
    }
}
