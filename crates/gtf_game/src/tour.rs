//! The streaming tour (`freeway/tour`, PLAN.md §9 M2 acceptance): a camera flying the district's
//! roads at 40 m/s — downtown, Sunset Row, up the Pretzel onto Route 404 and back, the hills, the
//! docks, down into The Ditch — with streaming following it, so tests and filmstrips can check
//! that the city keeps up without holes.

use glam::Vec3;
use pav_core::present::CameraIntent;
use pav_world::City;

/// Metres per second.
pub const SPEED: f32 = 40.0;

/// Places the tour passes through, in order (world x, y, z); it returns to the first.
const WAYPOINTS: &[[f32; 3]] = &[
    [-148.0, 0.0, -236.0],
    [300.0, 0.0, -236.0],
    [478.8, 11.0, -560.0],
    [465.2, 11.0, -600.0],
    [465.2, 11.0, -300.0],
    [-150.0, 0.0, -104.0],
    [-100.0, 9.0, -488.0],
    [-200.0, 28.0, -584.0],
    [-400.0, 0.0, 0.0],
    [-300.0, 0.0, 200.0],
    [300.0, 0.0, 280.0],
    [208.0, -6.0, 200.0],
    [100.0, 0.0, 40.0],
];

pub struct Tour {
    pub points: Vec<Vec3>,
    /// Distance along the tour at each point.
    pub station: Vec<f32>,
    pub length: f32,
}

impl Tour {
    /// The tour through `city`'s lanes (routes between the waypoints).
    pub fn new(city: &City) -> Tour {
        let g = &city.lanes;
        let mut points: Vec<Vec3> = Vec::new();
        let n = WAYPOINTS.len();
        for k in 0..n {
            let a = Vec3::from(WAYPOINTS[k]);
            let b = Vec3::from(WAYPOINTS[(k + 1) % n]);
            let lanes = g.route(a, b);
            for p in g.route_points(&lanes) {
                if points.last().is_none_or(|q| q.distance(p) > 1.0) {
                    points.push(p);
                }
            }
        }
        if points.len() < 2 {
            points = vec![Vec3::ZERO, Vec3::X];
        }
        let mut station = vec![0.0];
        for w in points.windows(2) {
            station.push(station.last().unwrap() + w[0].distance(w[1]));
        }
        let length = *station.last().unwrap();
        Tour { points, station, length }
    }

    /// The point `s` metres along (looping).
    pub fn at(&self, s: f32) -> Vec3 {
        let s = s.rem_euclid(self.length.max(1.0));
        let i = self.station.partition_point(|x| *x <= s).clamp(1, self.points.len() - 1);
        let (a, b) = (self.points[i - 1], self.points[i]);
        let span = (self.station[i] - self.station[i - 1]).max(1e-3);
        a.lerp(b, ((s - self.station[i - 1]) / span).clamp(0.0, 1.0))
    }

    /// The chase camera `s` metres along: above and behind, looking down the road.
    pub fn camera(&self, s: f32) -> CameraIntent {
        let here = self.at(s);
        let ahead = self.at(s + 30.0);
        let back = self.at(s - 14.0);
        let eye = back + Vec3::Y * 7.0;
        CameraIntent {
            eye,
            target: ahead.lerp(here, 0.3) + Vec3::Y * 2.0,
            up: Vec3::Y,
            fov: 60.0,
            near: 0.3,
            far: 2500.0,
            cut: false,
            shake: 0.0,
        }
    }
}
