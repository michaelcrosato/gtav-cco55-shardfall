//! One tick of play (the frontend's Play page): the player on foot or at the wheel, the weapon
//! in hand, getting in and out, the people and the traffic around, the law (crimes, witnesses,
//! the wanted level, the police) and the player's fate (Wasted, Busted, the respawn).

use std::collections::BTreeMap;

use glam::{Vec2, Vec3};
use pav_core::InputFrame;
use pav_core::entity::EntityId;
use pav_core::frame::SimEvent;
use pav_core::input::pad;
use pav_core::sim::Sim;

use crate::fate::Fate;
use crate::heat::{Crime, Heat};
use crate::{Freeway, camera, combat, drive, fate, peds, police, traffic};

/// How often the same kind of crime is counted again (s): a burst of fire is one shooting.
const CRIME_REPEAT: f32 = 3.0;

impl Freeway {
    /// The Play page's tick: returns the inputs for the characters the game drives.
    pub(crate) fn play(
        &mut self,
        sim: &mut Sim,
        input: &InputFrame,
        dt: f32,
        real: f32,
        feet: Vec3,
        events: &mut Vec<SimEvent>,
        sounds: &mut Vec<&'static str>,
    ) -> BTreeMap<EntityId, InputFrame> {
        let mut out = BTreeMap::new();
        self.roam_age += real;
        let alive = self.fate.alive();
        let (notes, was_open) = (self.next_note, self.wheel_open());
        if self.drive.vehicle().is_none() && alive {
            let (open, focus) = (self.wheel_open(), self.wheel.as_ref().and_then(|w| w.focus));
            self.wheel_tick(input, real);
            match (open, self.wheel_open()) {
                (false, true) => sounds.push("ui.slowmo_in"),
                (true, false) => sounds.push(if self.next_note != notes { "wheel.pick" } else { "ui.slowmo_out" }),
                _ => {}
            }
            if self.wheel_open() && self.wheel.as_ref().and_then(|w| w.focus) != focus {
                sounds.push("wheel.slot");
            }
        } else {
            self.wheel = None;
        }
        let look = self.look();
        let Some(city) = self.city.clone() else { return out };
        // The player (not in a reference view's still or on the streaming tour's flight; not
        // while wasted or busted).
        let pid = sim.state.player.filter(|_| self.view.is_none() && self.mode != "tour" && alive);
        let mut deeds = Vec::new();
        let mut res = combat::Outcome::default();
        let mut melee = false;
        if let Some(id) = pid {
            // Getting in or out, and driving.
            if let Some(walk) = self.drive.tick(sim, id, input, dt, &mut deeds, events) {
                out.insert(id, walk);
            }
            self.drive.steer(sim, id, input, dt, &mut deeds, events);
        }
        let riding = self.drive.vehicle().and_then(|car| Some((car, crate::ride(sim, car)?)));
        if (riding.is_some() || !alive || self.drive.busy()) && self.cover.at.is_some() {
            if let Some(id) = sim.state.player {
                self.cover.leave(sim, id);
            }
        }
        if !alive {
            // The camera circles the player; nothing answers the pad.
            self.fate = match self.fate {
                Fate::Wasted { t } => Fate::Wasted { t: t + real },
                Fate::Busted { t } => Fate::Busted { t: t + real },
                Fate::Alive => Fate::Alive,
            };
        } else if let Some((car, r)) = riding {
            // Walls pull the camera in: the distance to the first surface along a ray.
            // (The fixed world only: people and props walking past do not shove the camera.)
            let hit = |o: Vec3, d: Vec3, max: f32| sim.raycast_static(o, d, max).map(|h| h.point.distance(o));
            self.chase.update(input, &r, look, real, &hit);
            self.radio_controls(input, real, sounds);
            // Headlights after dark and in bad weather.
            let dark = !(6.3..19.5).contains(&self.sky.hour) || matches!(self.sky.weather.as_str(), "rain" | "storm" | "fog");
            sim.set_vehicle_lights(car, dark);
        } else if self.wheel_open() {
            let hit = |o: Vec3, d: Vec3, max: f32| sim.raycast_static(o, d, max).map(|h| h.point.distance(o));
            self.cam.update(&InputFrame::default(), feet, look, camera::Moving::default(), real, &hit);
        } else {
            self.radio_wheel = None;
            let (stick, world) = crate::pad::stick_or_move(input);
            // (In cover the stick slides along the wall: the camera does not swing after it.)
            let moving = camera::Moving {
                dir: if self.cover.at.is_some() {
                    Vec2::ZERO
                } else {
                    if world { stick } else { self.cam.world_move(stick) }.clamp_length_max(1.0)
                },
                sprinting: self.foot.sprinting,
                aiming: input.pad.l2 > 0.5 || input.pad.held & pad::L2 != 0,
            };
            let hit = |o: Vec3, d: Vec3, max: f32| sim.raycast_static(o, d, max).map(|h| h.point.distance(o));
            self.cam.update(input, feet, look, moving, real, &hit);
            // △ near a car starts getting in (the walk to its door begins next tick).
            let entering = pid.is_some()
                && !self.drive.busy()
                && crate::pad::pressed(input, pad::NORTH)
                && self.drive.begin_enter(sim, feet);
            if let (Some(id), false, false) = (pid, entering, self.drive.busy()) {
                // R1 takes cover (when not aiming); in cover the cover decides the steps.
                let cam_now = self.cam.intent();
                let view = cam_now.target - cam_now.eye;
                if self.cover.at.is_none()
                    && !moving.aiming
                    && crate::pad::pressed(input, pad::R1)
                    && self.cover.enter(&city, feet, view)
                {
                    sounds.push("cover.in");
                }
                let in_cover = match self.cover.at {
                    Some(_) => self.cover.step(&city, sim, id, input, &self.cam, dt),
                    None => None,
                };
                let (frame, haste) = match in_cover {
                    Some(f) => (f, 0.0),
                    None => self.foot.input(input, &self.cam, 50.0, dt),
                };
                let wdef = combat::def().ok();
                let gun = wdef.as_ref().and_then(|d| d.weapon.get(self.weapon)).is_some_and(|w| w.kind == combat::Kind::Gun);
                let cam = self.cam.intent();
                let look = (cam.target - cam.eye).normalize_or(Vec3::NEG_Z);
                if let Some(c) = sim.state.entities.get_mut(id).and_then(|e| e.character.as_mut()) {
                    c.haste = haste;
                    // Aiming a gun turns the body to the aim (strafing); in cover the wall decides.
                    if moving.aiming && gun {
                        c.face = Some(look.x.atan2(look.z));
                    } else if self.cover.at.is_none() {
                        c.face = None;
                    }
                }
                out.insert(id, frame);
                // The weapon in hand: fire, reload, melee.
                if let Some(wdef) = wdef {
                    let facing = sim.state.entities.get(id).and_then(|e| e.character.as_ref()).map_or(0.0, |c| c.facing);
                    let blind = self.cover.at.is_some() && !moving.aiming;
                    let shooter = combat::Shooter { id, feet, facing, camera: &cam, moving: moving.dir.length() > 0.3, blind };
                    res = self.combat.tick(sim, &wdef, self.weapon, input, &shooter, &mut self.cam, dt, events);
                    melee = wdef.weapon.get(self.weapon).is_some_and(|w| w.kind == combat::Kind::Melee);
                }
            }
        }
        if self.next_note != notes && !was_open {
            sounds.push("ui.toast");
        }
        self.after_deeds(sim, &deeds);
        // Crimes the player committed this tick (judged once the people and police have moved).
        let mut crimes: Vec<(Crime, Vec3)> = Vec::new();
        // Grenades in flight go off.
        for at in self.combat.grenades_tick(sim, dt, events) {
            self.peds.scare(sim, &city.walk, at, 45.0);
            self.peds.incident(sim, at, 60.0);
            crimes.push((Crime::Shooting, at));
        }
        if self.mode == "tour" {
            return out;
        }
        // The people react to what the weapon did.
        if let Some(at) = res.gunfire {
            self.peds.scare(sim, &city.walk, at, 40.0);
            crimes.push((Crime::Shooting, at));
        }
        for &(who, at) in &res.hurt {
            let cop = self.police.is_officer(who);
            if melee && !cop {
                // Tough people fight back; the rest run.
                self.peds.assaulted(sim, &city.walk, who, at);
                self.peds.scare(sim, &city.walk, at, 8.0);
            } else {
                self.peds.scare(sim, &city.walk, at, 15.0);
            }
            self.peds.incident(sim, at, 35.0);
            crimes.push((if cop { Crime::AttackPolice } else { Crime::Assault }, at));
        }
        for &(who, at) in &res.killed {
            let cop = self.police.is_officer(who);
            self.peds.forget(who);
            self.peds.scare(sim, &city.walk, at, 25.0);
            crimes.push((if cop { Crime::KillPolice } else { Crime::KillCivilian }, at));
        }
        for d in &deeds {
            match *d {
                drive::Deed::Smashed { at, .. } | drive::Deed::Jacked { at, .. } => crimes.push((Crime::CarTheft, at)),
                _ => {}
            }
        }
        // A reference view's crowd stands around its camera, not its target.
        let typed: Vec<Crime> = std::mem::take(&mut self.typed_crimes);
        let player = match &self.view {
            Some(v) => Vec3::new(v.eye.x, 0.0, v.eye.z),
            None => sim.state.player.and_then(|id| sim.feet(id)).unwrap_or(feet),
        };
        // Vehicles about to run people over throw them first (before the physics step).
        let on_foot = sim.state.player.filter(|_| self.drive.vehicle().is_none() && self.view.is_none());
        let people: Vec<EntityId> = sim
            .state
            .entities
            .iter()
            .filter(|e| e.character.as_ref().is_some_and(|c| c.riding.is_none()) && !e.name.starts_with("dead:"))
            .filter(|e| e.pos.distance(player) < 120.0)
            .filter(|e| Some(e.id) != sim.state.player || on_foot.is_some())
            .map(|e| e.id)
            .collect();
        crimes.extend(typed.into_iter().map(|c| (c, player)));
        let mine = self.drive.vehicle();
        for hit in self.struck.tick(sim, &people, dt, events) {
            if Some(hit.car) == mine {
                let cop = self.police.is_officer(hit.who);
                let crime = match (cop, hit.speed >= crate::struck::LETHAL) {
                    (true, true) => Crime::KillPolice,
                    (true, false) => Crime::AttackPolice,
                    (false, true) => Crime::KillCivilian,
                    (false, false) => Crime::Vehicular,
                };
                crimes.push((crime, hit.at));
            }
            self.strike(sim, &city, hit, events);
        }
        let gun_out = self.combat.aiming
            && combat::def().ok().and_then(|d| d.weapon.get(self.weapon).map(|w| w.kind == combat::Kind::Gun)).unwrap_or(false);
        let m = peds::Moment {
            city: &city,
            player,
            camera: self.camera(sim),
            time: self.time,
            hour: self.sky.hour,
            dt,
            player_on_foot: on_foot.and_then(|id| sim.feet(id)),
            gun_out,
        };
        let done = self.peds.tick(sim, &m, &mut out);
        if alive {
            for dmg in done.punches {
                self.combat.hurt_player(dmg);
            }
        }
        // Traffic around the player (a reference view's around its camera).
        let sirens = sim
            .state
            .entities
            .iter()
            .filter_map(|e| {
                let v = e.vehicle.as_ref()?;
                v.state.siren_on.then(|| (e.pos, e.rot * Vec3::Z))
            })
            .collect();
        let walkers = self
            .peds
            .list
            .iter()
            .filter_map(|p| sim.feet(p.id))
            .chain(on_foot.and_then(|id| sim.feet(id)))
            .chain(self.police.officers.iter().filter(|o| o.duty != police::Duty::Seated).filter_map(|o| sim.feet(o.id)))
            .filter(|f| f.distance(player) < 160.0)
            .collect();
        let tm = traffic::Moment {
            city: &city,
            player,
            camera: self.camera(sim),
            time: self.time,
            hour: self.sky.hour,
            dt,
            player_car: mine,
            sirens,
            people: walkers,
            gunfire: res.gunfire,
        };
        let mut inputs = BTreeMap::new();
        self.traffic.tick(sim, &tm, &mut inputs, &mut self.drive.locked);
        // The law: brandishing at the police, then each crime's witnesses, the police, the heat.
        if self.view.is_none() {
            if gun_out && pid.is_some() {
                let seen_by_cop = self.police.officers.iter().any(|o| {
                    sim.feet(o.id).is_some_and(|f| {
                        f.distance(player) < 20.0 && crate::heat::sight(sim, f + Vec3::Y * 1.6, player + Vec3::Y)
                    })
                });
                if seen_by_cop {
                    crimes.push((Crime::Brandish, player));
                }
            }
            self.law(sim, &city, player, crimes, &mut inputs, &mut out, events, real, dt);
        }
        self.ai_inputs = inputs;
        // Death.
        if self.fate.alive() && self.combat.health <= 0.0 && pid.is_some() {
            self.go(sim, Fate::Wasted { t: 0.0 }, events);
        }
        // The banner done: wake up somewhere else.
        if !self.fate.alive() && self.fate.since() >= fate::LENGTH {
            self.respawn(sim, &city);
        }
        out
    }

    /// Crimes → witnesses → stars; the police's tick; sight, search, evasion; arrests.
    #[allow(clippy::too_many_arguments)]
    fn law(
        &mut self,
        sim: &mut Sim,
        city: &pav_world::City,
        player: Vec3,
        crimes: Vec<(Crime, Vec3)>,
        inputs: &mut BTreeMap<EntityId, pav_core::vehicle::VehicleInput>,
        out: &mut BTreeMap<EntityId, InputFrame>,
        events: &mut Vec<SimEvent>,
        real: f32,
        dt: f32,
    ) {
        for c in &mut self.crime_clock {
            c.1 -= dt;
        }
        self.crime_clock.retain(|c| c.1 > 0.0);
        let crowd: Vec<EntityId> = self.peds.list.iter().map(|p| p.id).collect();
        for (crime, at) in crimes {
            if self.crime_clock.iter().any(|c| c.0 == crime) {
                continue;
            }
            self.crime_clock.push((crime, CRIME_REPEAT));
            let cops = self.police.officers.iter().any(|o| {
                let eye = sim.feet(o.id).map(|f| f + Vec3::Y * 1.6);
                eye.is_some_and(|e| e.distance(at) < crate::heat::POLICE_SIGHT && crate::heat::sight(sim, e, at + Vec3::Y))
            });
            // Filming bystanders and those in sight all count.
            let mut seen = Heat::witnesses(sim, &crowd, at);
            seen.extend(self.peds.filming().map(|p| p.id));
            self.heat.crime(sim, crime, at, cops, &seen);
            if !cops && !seen.is_empty() {
                // They run, and phone it in on the way.
                self.peds.scare(sim, &city.walk, at, 30.0);
            }
        }
        let feet = player;
        let vel = self
            .drive
            .vehicle()
            .and_then(|c| drive::Car::read(sim, c))
            .map(|c| c.vel)
            .or_else(|| sim.player().and_then(|p| p.character.as_ref()).map(|c| c.vel))
            .unwrap_or(Vec3::ZERO);
        let hidden = self.foot.crouched || matches!(city.zone_at(Vec2::new(feet.x, feet.z)), Some("corridor"));
        let pm = police::Moment {
            city,
            camera: self.camera(sim),
            player: feet,
            player_vel: vel,
            player_car: self.drive.vehicle(),
            stars: self.heat.stars,
            searching: self.heat.searching,
            zone: self.heat.zone(),
            last_known: self.heat.last_known,
            dispatch: self.heat.dispatch,
            night: !(6.3..19.5).contains(&self.sky.hour),
            hidden,
            covered: self.cover.covered(),
            firing: self.crime_clock.iter().any(|c| matches!(c.0, Crime::Shooting | Crime::AttackPolice | Crime::KillPolice)),
            dt,
        };
        let mut moves = BTreeMap::new();
        let rep = self.police.tick(sim, &pm, inputs, &mut moves);
        out.extend(moves);
        if self.fate.alive() {
            for dmg in &rep.hits {
                self.combat.hurt_player(*dmg);
            }
        }
        for &(from, to, hit) in &rep.shots {
            self.combat.trace(from, to, hit);
            events.push(SimEvent::Sound { cue: "gun.pistol".into(), pos: Some(from), gain: 0.9, pitch: 1.05 });
        }
        if !rep.shots.is_empty()
            && let Some(at) = rep.shots.first().map(|s| s.0)
        {
            self.peds.scare(sim, &city.walk, at, 35.0);
        }
        let before = self.heat.stars;
        let cleared = self.heat.tick(sim, feet, rep.sees, hidden, None, dt);
        if cleared {
            self.police.stand_down();
            self.note("police", "Lost them", "The police have given up the search.");
        }
        if self.heat.stars > before {
            self.scanner = 0.5;
        }
        if let Some(news) = self.heat.news.take() {
            let stars = "★".repeat(self.heat.stars as usize);
            self.note("police", &format!("WANTED {stars}"), &news);
        }
        // Scanner chatter under everything at 2★ and up.
        self.scanner -= real;
        if self.heat.stars >= 2 && self.scanner <= 0.0 {
            self.scanner = sim.state.rng.range(6.0, 11.0);
            events.push(SimEvent::Sound { cue: "police.scanner".into(), pos: None, gain: 0.8, pitch: 1.0 });
        }
        // Busted: an officer at arm's length of a player on foot who is barely moving, at 1–2★.
        let slow = vel.length() < 1.5;
        if rep.cuffing && self.heat.stars <= 2 && self.drive.vehicle().is_none() && slow && self.fate.alive() {
            self.cuffed += dt;
            if self.cuffed >= fate::CUFF_TIME {
                self.go(sim, Fate::Busted { t: 0.0 }, events);
            }
        } else {
            self.cuffed = 0.0;
        }
    }

    /// Wasted or Busted: the banner, the slow world; out of any vehicle.
    pub(crate) fn go(&mut self, sim: &mut Sim, fate: Fate, events: &mut Vec<SimEvent>) {
        self.fate = fate;
        self.cuffed = 0.0;
        self.wheel = None;
        self.radio_wheel = None;
        if let (Some(pid), Some(car)) = (sim.state.player, self.drive.vehicle()) {
            sim.exit_vehicle(pid, car);
        }
        self.drive.phase = drive::Phase::Foot;
        self.radio.tune(None);
        events.push(SimEvent::Sound { cue: "ui.banner".into(), pos: None, gain: 1.0, pitch: 0.8 });
        if let Some(pid) = sim.state.player
            && let Fate::Busted { .. } = fate
        {
            // Hands up.
            let clip = pav_core::clips::find_cached("MESH2MOTION/Idle_HandsUp");
            if let Some(c) = sim.state.entities.get_mut(pid).and_then(|e| e.character.as_mut())
                && clip != 0
            {
                c.anim.replay_clip(clip, pav_core::clips::CLAMP, 1.0);
            }
        }
    }

    /// After the banner: outside St. Brigid Medical (wasted) or SPPD Central (busted), fined, the
    /// heat and the police gone.
    pub(crate) fn respawn(&mut self, sim: &mut Sim, city: &pav_world::City) {
        let (place, fee) = match self.fate {
            Fate::Wasted { .. } => ("hospital", fate::WASTED_FEE),
            Fate::Busted { .. } => ("police", fate::busted_fee(self.money)),
            Fate::Alive => return,
        };
        if let Fate::Busted { .. } = self.fate {
            // All ammunition but a magazine.
            self.combat.reserve.fill(0);
        }
        let at = crate::world::door(city, place).unwrap_or(crate::world::SPAWN);
        if let Some(pid) = sim.state.player {
            sim.set_position(pid, at + Vec3::Y * 0.1);
            if let Some(c) = sim.state.entities.get_mut(pid).and_then(|e| e.character.as_mut()) {
                c.vel = Vec3::ZERO;
                c.impulse = Vec3::ZERO;
                c.face = None;
            }
        }
        self.combat.health = combat::MAX_HEALTH;
        self.combat.armor = 0.0;
        self.pay(-fee);
        self.heat.clear();
        self.police.clear(sim);
        self.fate = Fate::Alive;
        self.front.fade_in(self.ui_time);
        self.cam.face(self.cam.yaw);
    }
}
