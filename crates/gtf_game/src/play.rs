//! One tick of play (the frontend's Play page): the player on foot or at the wheel, the weapon
//! in hand, getting in and out, the people and the traffic around, the law (crimes, witnesses,
//! the wanted level, the police) and the player's fate (Wasted, Busted, the respawn).

use std::collections::BTreeMap;

use glam::{Vec2, Vec3};
use pav_core::InputFrame;
use pav_core::entity::EntityId;
use pav_core::frame::SimEvent;
use pav_core::input::pad;
use pav_core::present::CameraIntent;
use pav_core::sim::Sim;

use crate::fate::Fate;
use crate::heat::{Crime, Heat};
use crate::{Freeway, camera, combat, drive, fate, peds, police, traffic};

/// The weapon to fire from a vehicle: the one in hand if it can be, else the first that can
/// with rounds left (None: nothing to fire).
fn driveby_weapon(def: &combat::WeaponsDef, current: usize, c: &combat::Combat) -> Option<usize> {
    let ok = |i: usize| {
        def.weapon.get(i).is_some_and(|w| w.driveby)
            && c.mag.get(i).copied().unwrap_or(0) + c.reserve.get(i).copied().unwrap_or(0) > 0
    };
    if ok(current) { Some(current) } else { (0..def.weapon.len()).find(|&i| ok(i)) }
}

/// How near Paloma Customs' door a stopped car must be for the respray (m).
const RESPRAY_REACH: f32 = 11.0;

/// Seconds into BUSTED (real time) the hands go behind the head on the knees.
const KNEEL: f32 = 1.4;
/// A witness's call shows (on the phone, a marker over them) for its last seconds.
pub(crate) const CALL_SHOWS: f32 = 3.0;

/// Plays clip `name` on character `id` (`flags`: `pav_core::clips`), if the clip exists.
pub(crate) fn play_clip(sim: &mut Sim, id: EntityId, name: &str, flags: u8) {
    let clip = pav_core::clips::find_cached(name);
    if let Some(c) = sim.state.entities.get_mut(id).and_then(|e| e.character.as_mut())
        && clip != 0
    {
        c.anim.replay_clip(clip, flags, 1.0);
    }
}

/// How often the same kind of crime is counted again (s): a burst of fire is one shooting.
const CRIME_REPEAT: f32 = 3.0;
/// A police unit this close to the evasion bot while they search is what it runs from (m).
const CLOSE_UNIT: f32 = 140.0;

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
        crate::world::sweep_unloaded(sim);
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
        // The weapon in hand shows (a gun or a grenade; in a vehicle only while aiming a drive-by).
        if let Some(id) = pid {
            let shown = combat::def().ok().and_then(|d| d.weapon.get(self.weapon).map(|w| w.asset.clone())).unwrap_or_default();
            let driveby = input.pad.held & pad::L1 != 0;
            let hand = if self.drive.vehicle().is_none() && !self.drive.busy() || driveby { shown } else { String::new() };
            combat::hold(sim, id, &hand);
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
                Fate::Busted { t } => {
                    // Hands up, then down on the knees for the cuffs.
                    if t < KNEEL
                        && t + real >= KNEEL
                        && let Some(pid) = sim.state.player
                    {
                        play_clip(sim, pid, "WORKSHOP/Arrested", pav_core::clips::CLAMP);
                    }
                    Fate::Busted { t: t + real }
                }
                Fate::Alive => Fate::Alive,
            };
        } else if let Some((car, r)) = riding {
            // Walls pull the camera in: the distance to the first surface along a ray.
            // (The fixed world only: people and props walking past do not shove the camera.)
            let hit = |o: Vec3, d: Vec3, max: f32| sim.raycast_static(o, d, max).map(|h| h.point.distance(o));
            self.chase.update(input, &r, look, real, &hit);
            self.radio_controls(input, real, sounds);
            // Drive-by: L1 held aims down the camera's line, R1 fires a pistol or an SMG.
            if let Some(id) = pid
                && input.pad.held & pad::L1 != 0
                && let Ok(wdef) = combat::def()
                && let Some(w) = driveby_weapon(&wdef, self.weapon, &self.combat)
            {
                if w != self.weapon {
                    self.last_weapon = self.weapon;
                    self.weapon = w;
                }
                // Level, from the driver's head along the camera's heading — onto the nearest
                // person within 12° of that line and in range (a soft lock: drive-bys on a pad).
                let view = self.camera(sim);
                let flat = Vec3::new(view.target.x - view.eye.x, 0.0, view.target.z - view.eye.z).normalize_or(Vec3::Z);
                let seat = sim.feet(id).unwrap_or(feet);
                let head = seat + Vec3::Y * 1.25;
                let range = wdef.weapon.get(self.weapon).map_or(30.0, |w| w.range);
                let lock = sim
                    .state
                    .entities
                    .iter()
                    .filter(|e| {
                        e.id != id && e.character.as_ref().is_some_and(|c| c.riding.is_none()) && !e.name.starts_with("dead:")
                    })
                    .filter_map(|e| {
                        let chest = e.pos + Vec3::Y * 0.4;
                        let to = chest - head;
                        let d = to.length();
                        let off = Vec3::new(to.x, 0.0, to.z).normalize_or(Vec3::Z).angle_between(flat);
                        (d < range && off < 12f32.to_radians()).then_some((chest, off))
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|x| x.0);
                let cam = CameraIntent { eye: head - flat * 0.3, target: lock.unwrap_or(head + flat * 25.0), ..view };
                let look = cam.target - cam.eye;
                let shooter = combat::Shooter {
                    id,
                    feet: seat,
                    facing: look.x.atan2(look.z),
                    camera: &cam,
                    moving: true,
                    blind: false,
                    exclude: Some(car),
                };
                let mut f = InputFrame::default();
                f.pad.l2 = 1.0;
                if input.pad.held & pad::R1 != 0 {
                    f.pad.r2 = 1.0;
                }
                if input.pad.pressed & pad::R1 != 0 {
                    f.pad.pressed |= pad::R2;
                }
                res = self.combat.tick(sim, &wdef, self.weapon, &f, &shooter, &mut self.cam, dt, events);
            } else {
                self.combat.aiming = false;
            }
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
                    && self.cover.enter(&city, sim, feet, view)
                {
                    sounds.push("cover.in");
                }
                let in_cover = match self.cover.at {
                    Some(_) => self.cover.step(&city, sim, id, input, &self.cam, dt),
                    None => None,
                };
                let stamina = self.crew.me().map_or(50.0, |m| m.stats.stamina);
                let (frame, haste) = match in_cover {
                    Some(f) => (f, 0.0),
                    None => self.foot.input(input, &self.cam, stamina, self.settings.sprint_toggle, dt),
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
                    let shooter = combat::Shooter {
                        id,
                        feet,
                        facing,
                        camera: &cam,
                        moving: moving.dir.length() > 0.3,
                        blind,
                        exclude: None,
                    };
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
            let near: Vec<EntityId> = sim
                .state
                .entities
                .iter()
                .filter(|e| e.vehicle.is_some() && e.pos.distance(at) < combat::BLAST)
                .map(|e| e.id)
                .collect();
            for id in near {
                self.police.harmed(id);
            }
        }
        crate::glass::shots(sim, &city, &res.walls, events);
        for &id in &res.vehicles {
            self.police.harmed(id);
        }
        if self.mode == "tour" {
            return out;
        }
        // The people react to what the weapon did.
        if let Some(at) = res.gunfire {
            self.peds.scare(sim, &city.walk, at, 40.0);
            // (An officer on vigilante duty fires without a report.)
            if !self.on_duty() {
                crimes.push((Crime::Shooting, at));
            }
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
            // (A vigilante's suspects and a purse snatcher are fair game.)
            if !self.fair_game(who) {
                crimes.push((if cop { Crime::AttackPolice } else { Crime::Assault }, at));
            }
        }
        for &(who, at) in &res.killed {
            let cop = self.police.is_officer(who);
            self.peds.forget(who);
            self.peds.scare(sim, &city.walk, at, 25.0);
            if !self.fair_game(who) {
                crimes.push((if cop { Crime::KillPolice } else { Crime::KillCivilian }, at));
            }
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
        let shove = self.drive.input.and_then(|(car, i)| {
            let way = if i.throttle > 0.1 {
                1.0
            } else if i.brake > 0.1 {
                -1.0
            } else {
                return None;
            };
            Some((car, way))
        });
        for hit in self.struck.tick(sim, &people, shove, dt, events) {
            if Some(hit.car) == mine && !self.fair_game(hit.who) {
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
                // Cars pull over for sirens on the road, not for a helicopter's light bar.
                let v = e.vehicle.as_ref().filter(|v| v.def.kind != pav_core::vehicle::VehicleKind::Helicopter)?;
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
            police: self.police.units.iter().map(|u| u.vehicle).collect(),
            officers: self
                .police
                .officers
                .iter()
                .filter(|o| o.duty != police::Duty::Seated)
                .filter_map(|o| sim.feet(o.id))
                .collect(),
            wanted: self.heat.stars > 0,
        };
        // The evasion bot runs from the nearest police unit, or out of the search zone.
        // Near the map's edge it runs from the edge as well (along it, then back in), not into
        // the end of the world with the police behind.
        let (lo, hi) = (city.grid.origin, city.grid.origin + Vec2::new(city.grid.w as f32, city.grid.h as f32) * city.grid.cell);
        self.traffic.escape = (self.bot.escape && self.heat.stars > 0).then(|| {
            let nearest = self
                .police
                .units
                .iter()
                .filter_map(|u| sim.state.entities.get(u.vehicle).map(|e| e.pos))
                .min_by(|a, b| a.distance(player).total_cmp(&b.distance(player)));
            // (Searching: away from where they last saw it — unless a unit is close, which it
            // keeps clear of rather than running into.)
            let mut from = match (self.heat.zone(), nearest) {
                (Some(_), Some(n)) if n.distance(player) < CLOSE_UNIT => n,
                (Some((centre, _)), _) => centre,
                (None, n) => n.unwrap_or(self.heat.last_known),
            };
            // (A quarter of the map at most: a small district has a middle too.)
            let edge = ((hi - lo) * 0.25).min(Vec2::splat(250.0));
            if player.x < lo.x + edge.x {
                from.x = player.x - edge.x;
            } else if player.x > hi.x - edge.x {
                from.x = player.x + edge.x;
            }
            if player.z < lo.y + edge.y {
                from.z = player.z - edge.y;
            } else if player.z > hi.y - edge.y {
                from.z = player.z + edge.y;
            }
            from
        });
        let mut inputs = BTreeMap::new();
        self.traffic.tick(sim, &tm, &mut inputs, &mut self.drive.locked);
        // People in front of a fleeing car jump clear (off the road first, then away).
        for at in std::mem::take(&mut self.traffic.honks) {
            self.peds.dodge(sim, &city.walk, at, 4.0);
            self.peds.scare(sim, &city.walk, at, 9.0);
        }
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
            // On vigilante duty, harm to the public is not the police's to chase: a death docks
            // the run's pay instead (attacks on officers are reported as ever).
            if self.on_duty() {
                let deaths = crimes.iter().filter(|c| c.0 == Crime::KillCivilian).count();
                crimes.retain(|c| !matches!(c.0, Crime::Assault | Crime::KillCivilian | Crime::Vehicular));
                if deaths > 0 {
                    self.vigilante_casualties(deaths);
                }
            }
            self.law(sim, &city, player, crimes, &mut inputs, &mut out, events, real, dt);
        }
        self.ai_inputs = inputs;
        // The bot's next presses (tests, soaks, attract).
        if self.bot.escape && self.fate.alive() {
            let feet = pid.and_then(|id| sim.feet(id)).unwrap_or(player);
            let mut police: Vec<EntityId> = self.police.units.iter().map(|u| u.vehicle).collect();
            police.extend(self.police.blocks.iter().flat_map(|b| b.cars.iter().copied()));
            let threat = self
                .police
                .officers
                .iter()
                .filter_map(|o| sim.feet(o.id))
                .chain(self.police.units.iter().filter_map(|u| sim.state.entities.get(u.vehicle).map(|e| e.pos)))
                .min_by(|a, b| a.distance(feet).total_cmp(&b.distance(feet)));
            let officers: Vec<Vec3> =
                self.police.officers.iter().filter(|o| o.duty != police::Duty::Seated).filter_map(|o| sim.feet(o.id)).collect();
            let bm = crate::bot::Moment {
                city: &city,
                feet,
                car: self.drive.vehicle(),
                busy: self.drive.busy() || matches!(self.drive.phase, drive::Phase::Stopping { .. }),
                threat,
                police: &police,
                officers: &officers,
                wanted: self.heat.stars > 0,
                stars: self.heat.stars,
                keep: self.kept_car(sim),
                dt,
            };
            self.bot.update(sim, &bm);
        }
        // The ability meter fills by how the protagonist plays.
        self.fill_meter(sim, &res, events, real);
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
        let gunfire = crimes.iter().find(|c| c.0 == Crime::Shooting).map(|c| c.1);
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
        // Police wrecks join the other wrecks (cleared away once far and out of view).
        for car in std::mem::take(&mut self.police.wrecks) {
            for p in crate::traffic::occupants(sim, car) {
                sim.despawn(p);
            }
            self.traffic.loose.push(car);
        }
        if self.fate.alive() {
            for dmg in &rep.hits {
                self.combat.hurt_player(*dmg);
            }
        }
        for &(from, to, hit) in &rep.shots {
            self.combat.trace(from, to, hit);
            events.push(SimEvent::Sound { cue: "gun.pistol".into(), pos: Some(from), gain: 0.9, pitch: 1.05 });
        }
        // Armed opponents: cover, peeks, flanks, grenades.
        if !self.enemies.list.is_empty() {
            let em = crate::enemies::Moment {
                city,
                player: feet,
                player_vel: vel,
                cover: self.cover.at.map(|s| (s.pos, s.normal)),
                covered: self.cover.covered(),
                in_car: self.drive.vehicle().is_some(),
                gunfire,
                dt,
            };
            let mut moves = BTreeMap::new();
            let er = self.enemies.tick(sim, &em, &self.combat.hp, &mut moves);
            out.extend(moves);
            if self.fate.alive() {
                for dmg in &er.hits {
                    self.combat.hurt_player(*dmg);
                }
            }
            for &(from, to, hit) in &er.shots {
                self.combat.trace(from, to, hit);
                events.push(SimEvent::Sound { cue: "gun.pistol".into(), pos: Some(from), gain: 0.9, pitch: 0.95 });
            }
            for &(pos, vel) in &er.grenades {
                self.combat.grenades.push(combat::Grenade { pos, vel, fuse: combat::FUSE });
            }
            if let Some(&(at, _, _)) = er.shots.first() {
                self.peds.scare(sim, &city.walk, at, 35.0);
            }
        }
        if !rep.shots.is_empty()
            && let Some(at) = rep.shots.first().map(|s| s.0)
        {
            self.peds.scare(sim, &city.walk, at, 35.0);
        }
        // The car they saw the player in (on foot: none — any car is one they have not seen).
        if rep.sees {
            self.heat.seen_car = self.drive.vehicle();
        }
        let before = self.heat.stars;
        let cleared = self.heat.tick(sim, feet, rep.sees, None, dt);
        // Callers in the last seconds of their call stop and talk on the phone.
        let callers: Vec<EntityId> = self.heat.reports.iter().filter(|r| r.left < CALL_SHOWS).map(|r| r.reporter).collect();
        self.peds.phoning(sim, &callers);
        if cleared {
            self.police.stand_down();
            self.note("police", "Lost them", "The police have given up the search.");
        }
        // Paloma Customs: a car brought to the door while the police are not looking comes out
        // resprayed and repaired, and the heat is over.
        if let Some(car) = self.drive.vehicle()
            && self.heat.stars > 0
            && !rep.sees
            && vel.length() < 1.0
            && let Some(door) = self.customs_door(city)
            && sim.state.entities.get(car).is_some_and(|e| e.pos.distance(door) < RESPRAY_REACH)
        {
            if self.money >= fate::RESPRAY_FEE {
                self.money -= fate::RESPRAY_FEE;
                sim.repair_vehicle(car);
                crate::vehicles::repaint(sim, car);
                self.heat.clear();
                self.police.stand_down();
                self.resprays += 1;
                events.push(SimEvent::Sound { cue: "ui.cash".into(), pos: None, gain: 1.0, pitch: 1.0 });
                self.note("garage", "Paloma Customs", &format!("New paint, no questions. -${}", fate::RESPRAY_FEE));
            } else if !self.told_price {
                self.note("garage", "Paloma Customs", &format!("A respray is ${}.", fate::RESPRAY_FEE));
                self.told_price = true;
            }
        } else if self.told_price && self.drive.vehicle().is_none() {
            self.told_price = false;
        }
        if self.heat.stars > before {
            self.scanner = 0.5;
        }
        if let Some(news) = self.heat.news.take() {
            let stars = "★".repeat(self.heat.stars as usize);
            self.note("police", &format!("WANTED {stars}"), &news);
        }
        // Scanner chatter under everything at 2★ and up: the squelch, a dispatcher's line (a code,
        // a unit, where the police last saw the player: its babble and its words), the squelch.
        self.scanner -= real;
        if self.heat.stars >= 2 && self.scanner <= 0.0 {
            self.scanner = sim.state.rng.range(6.0, 11.0);
            events.push(SimEvent::Sound { cue: "police.scanner".into(), pos: None, gain: 0.8, pitch: 1.0 });
            if let Ok(def) = crate::radio::def() {
                let at = def.places.name(city, self.heat.last_known);
                self.chatter = crate::radio::chatter(&def, self.chatters, self.heat.stars, &at);
                self.chatters = self.chatters.wrapping_add(1);
            }
        }
        if let Some(c) = &mut self.chatter {
            c.age += real;
            if c.age >= c.seconds {
                self.chatter = None;
                events.push(SimEvent::Sound { cue: "police.scanner_out".into(), pos: None, gain: 0.8, pitch: 1.0 });
            }
        }
        // Busted: an officer at arm's length of a player on foot who is barely moving, at 1–2★ —
        // or of one sat in a car boxed in to a standstill (pulled out of it, a little slower).
        let car_speed = self.drive.vehicle().and_then(|c| drive::Car::read(sim, c)).map(|c| c.vel.length());
        match self.law_feet {
            Some(p) if Vec2::new(feet.x - p.x, feet.z - p.z).length() < 1.0 => self.law_still += dt,
            _ => {
                self.law_feet = Some(feet);
                self.law_still = 0.0;
            }
        }
        let held = match car_speed {
            Some(v) => v < 0.5,
            None => vel.length() < 1.5 || self.law_still > 0.7,
        };
        let cuff_time = if car_speed.is_some() { fate::CUFF_TIME * 2.0 } else { fate::CUFF_TIME };
        // (Getting in or out is not standing still for them: the time starts again after it.)
        if rep.cuffing && self.heat.stars <= 2 && held && self.fate.alive() && !self.drive.busy() {
            self.cuffed += dt;
            if self.cuffed >= cuff_time {
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
            play_clip(sim, pid, "WORKSHOP/Hands_Up", 0);
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
            // Up off the knees (or the ground).
            if let Some(c) = sim.state.entities.get_mut(pid).and_then(|e| e.character.as_mut()) {
                c.anim.stop_clip();
            }
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
        self.enemies.clear(sim);
        self.fate = Fate::Alive;
        self.front.fade_in(self.ui_time);
        self.cam.face(self.cam.yaw);
    }
}
