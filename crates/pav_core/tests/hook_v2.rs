//! The game hook's second half (`pav_core::games::Game`): a game can hold the world still, slow
//! it down, drive any vehicle, keep the engine demo's controls away from its player, hand the
//! host a presentation, and pick its rewind window — and all of it replays exactly. Also: input
//! frames gained controller fields without changing how old ones read and write.

use std::collections::BTreeMap;

use glam::{Vec2, Vec3};
use pav_core::entity::EntityId;
use pav_core::frame::SimEvent;
use pav_core::games::{self, Entry, Game, GameBox};
use pav_core::input::{Device, PadState, buttons, pad};
use pav_core::present::{CameraIntent, Hud, PlayFrame, UiModel};
use pav_core::room::RoomDef;
use pav_core::vehicle::VehicleInput;
use pav_core::{BodyKind, Color, InputFrame, Shape, Sim, Spawn, Visual};
use serde::{Deserialize, Serialize};

/// A test game: commands in the input switch its rules (`pause`, `scale 0.25`, `drive 1 0`).
#[derive(Clone, Default, Serialize, Deserialize)]
struct Hook {
    paused: bool,
    scale: f32,
    legacy: bool,
    /// Throttle and steer for the car.
    drive: Option<(f32, f32)>,
    car: u32,
    /// The `dt` each `pre` saw.
    seen: Vec<f32>,
    history: Option<f32>,
}

impl Game for Hook {
    const KEY: &'static str = "hook_test";

    fn pre(&mut self, _sim: &mut Sim, input: &InputFrame, dt: f32, _: &mut Vec<SimEvent>) -> BTreeMap<EntityId, InputFrame> {
        self.seen.push(dt);
        for c in &input.cmds {
            let words: Vec<&str> = c.split_whitespace().collect();
            match words.as_slice() {
                ["pause"] => self.paused = !self.paused,
                ["scale", s] => self.scale = s.parse().unwrap(),
                ["drive", t, s] => self.drive = Some((t.parse().unwrap(), s.parse().unwrap())),
                _ => panic!("unknown command {c}"),
            }
        }
        BTreeMap::new()
    }

    fn time_scale(&self) -> f32 {
        self.scale
    }

    fn world_paused(&self) -> bool {
        self.paused
    }

    fn legacy_controls(&self) -> bool {
        self.legacy
    }

    fn history_seconds(&self) -> Option<f32> {
        self.history
    }

    fn vehicles(&mut self, _sim: &mut Sim, _dt: f32) -> BTreeMap<EntityId, VehicleInput> {
        let mut out = BTreeMap::new();
        if let Some((throttle, steer)) = self.drive {
            out.insert(EntityId(self.car), VehicleInput { throttle, steer, ..Default::default() });
        }
        out
    }

    fn present(&self, sim: &Sim) -> Option<PlayFrame> {
        let focus = sim.state.focus;
        Some(PlayFrame {
            camera: Some(CameraIntent {
                eye: focus + Vec3::new(0.0, 3.0, 6.0),
                target: focus,
                up: Vec3::Y,
                fov: 55.0,
                near: 0.1,
                far: 800.0,
                cut: false,
                shake: 0.0,
            }),
            ui: UiModel {
                hud: Some(Hud { objective: Some("Drive to the ~y~docks~s~.".into()), ..Default::default() }),
                ..Default::default()
            },
            ..Default::default()
        })
    }
}

static ENTRY: Entry = games::entry::<Hook>("Hook v2 test game.", build);

fn build(sim: &mut Sim, _rest: Option<&str>) -> anyhow::Result<()> {
    let def = RoomDef::parse(include_str!("vehicle_room.toml")).expect("room parses");
    pav_core::scenes::build_standalone_room(sim, Hook::KEY, def);
    let car = sim.state.entities.find("car").unwrap().id.0;
    sim.state.play = Some(GameBox::new(Hook { scale: 1.0, legacy: true, car, ..Default::default() }));
    Ok(())
}

fn sim() -> Sim {
    games::register(&ENTRY);
    let mut sim = Sim::new(Hook::KEY, 1).unwrap();
    sim.run(30, &InputFrame::default());
    sim
}

fn hook(sim: &mut Sim) -> &mut Hook {
    sim.state.play.as_mut().unwrap().get_mut::<Hook>().unwrap()
}

fn cmd(c: &str) -> InputFrame {
    InputFrame { cmds: vec![c.into()], ..Default::default() }
}

fn drop_box(sim: &mut Sim, at: Vec3) -> EntityId {
    let look = Visual::new(Shape::Box { half: Vec3::splat(0.3) }, Color::hex("#e8704a"));
    sim.spawn(Spawn::new("box", at).visual(look).body(BodyKind::Dynamic))
}

fn velocity(sim: &Sim, id: EntityId) -> Vec3 {
    let body = sim.state.entities.get(id).unwrap().body.unwrap();
    sim.state.physics.bodies[body].linvel()
}

#[test]
fn a_paused_world_stands_still_while_the_game_runs() {
    let mut sim = sim();
    let b = drop_box(&mut sim, Vec3::new(0.0, 8.0, 0.0));
    sim.run(10, &InputFrame::default());
    sim.step(&cmd("pause"));
    let (pos, tick) = (sim.state.entities.get(b).unwrap().pos, sim.state.tick);
    let player = sim.player().unwrap().pos;
    hook(&mut sim).seen.clear();
    // Even the player's keys do nothing while the world is paused.
    sim.run(60, &InputFrame { move_dir: Vec2::X, ..Default::default() });
    assert_eq!(sim.state.tick, tick + 60, "ticks still count");
    assert_eq!(sim.state.entities.get(b).unwrap().pos, pos, "the box hangs in the air");
    assert_eq!(sim.player().unwrap().pos, player, "the player stays put");
    assert!(hook(&mut sim).seen.iter().all(|dt| *dt == 0.0), "the game sees dt = 0 while paused");
    sim.step(&cmd("pause"));
    sim.run(10, &InputFrame::default());
    assert!(sim.state.entities.get(b).unwrap().pos.y < pos.y, "falls again after unpausing");
}

#[test]
fn slow_motion_slows_the_whole_world() {
    let mut fast = sim();
    let mut slow = sim();
    slow.step(&cmd("scale 0.25"));
    fast.step(&InputFrame::default());
    let (a, b) = (drop_box(&mut fast, Vec3::new(0.0, 30.0, 0.0)), drop_box(&mut slow, Vec3::new(0.0, 30.0, 0.0)));
    fast.run(30, &InputFrame::default());
    slow.run(30, &InputFrame::default());
    let (vf, vs) = (velocity(&fast, a).y, velocity(&slow, b).y);
    assert!(vf < -4.0, "the box falls: {vf}");
    assert!((vs / vf - 0.25).abs() < 0.02, "a quarter of the speed: {vs} vs {vf}");
    let tick = slow.dt();
    let last = *hook(&mut slow).seen.last().unwrap();
    assert!((last - tick * 0.25).abs() < 1e-6, "the game's dt is scaled too");
}

#[test]
fn a_game_drives_a_vehicle_with_its_own_controls() {
    let mut sim = sim();
    hook(&mut sim).legacy = false;
    let pid = sim.state.player.unwrap();
    let car = sim.state.entities.find("car").unwrap().id;
    sim.enter_vehicle(pid, car);
    let start = sim.state.entities.get(car).unwrap().pos;
    sim.step(&cmd("drive 1 0"));
    sim.run(120, &InputFrame::default());
    let p = sim.state.entities.get(car).unwrap().pos;
    // The car faces north (-Z).
    assert!(start.z - p.z > 8.0, "drove north: {start} -> {p}");
    let fwd = |sim: &Sim| {
        let b = sim.state.entities.get(car).unwrap().body.unwrap();
        *sim.state.physics.bodies[b].rotation() * Vec3::Z
    };
    let before = fwd(&sim);
    sim.step(&cmd("drive 0.6 1"));
    sim.run(40, &InputFrame::default());
    let after = fwd(&sim);
    // Seen from above (+Y), a right turn is clockwise: the cross product points down.
    assert!(before.cross(after).y < -0.05, "turned right: {before} -> {after}");
}

#[test]
fn own_controls_keep_the_demo_actions_away_from_the_player() {
    let mut sim = sim();
    hook(&mut sim).legacy = false;
    let pid = sim.state.player.unwrap();
    let car = sim.state.entities.find("car").unwrap();
    let near = car.pos + Vec3::new(1.6, -0.8, 0.0);
    sim.set_position(pid, near);
    sim.run(5, &InputFrame::default());
    let before = sim.player().unwrap().pos;
    sim.step(&InputFrame { pressed: buttons::INTERACT, held: buttons::INTERACT, ..Default::default() });
    sim.run(30, &InputFrame { move_dir: Vec2::X, held: buttons::USE, ..Default::default() });
    let p = sim.player().unwrap();
    assert_eq!(p.character.as_ref().unwrap().riding, None, "interact does not board the car");
    assert!(p.pos.distance(before) < 0.05, "the raw input does not walk the player: {before} -> {}", p.pos);
    assert!(sim.state.entities.iter().all(|e| e.bomb.is_none()), "no bombs");
}

#[test]
fn the_presentation_reaches_the_render_frame_and_reads_as_json() {
    let mut sim = sim();
    let frame = sim.frame();
    let play = frame.present.expect("the game presents");
    let hud = play.ui.hud.as_ref().unwrap();
    assert_eq!(hud.objective.as_deref(), Some("Drive to the ~y~docks~s~."));
    let json = serde_json::to_string(&*play).unwrap();
    let back: PlayFrame = serde_json::from_str(&json).unwrap();
    assert_eq!(back.ui, play.ui);
    assert_eq!(back.camera, play.camera);
    // A scene without a game presents nothing.
    assert!(Sim::new("starter", 1).unwrap().frame().present.is_none());
}

#[test]
fn commands_pauses_and_slow_motion_replay_exactly() {
    let mut sim = sim();
    hook(&mut sim).legacy = false;
    let b = drop_box(&mut sim, Vec3::new(2.0, 12.0, 2.0));
    let script: Vec<InputFrame> = (0..200)
        .map(|i| match i {
            20 => cmd("scale 0.5"),
            60 => cmd("pause"),
            90 => cmd("pause"),
            120 => cmd("drive 1 0.3"),
            _ => InputFrame { pad: PadState { stick_l: Vec2::new(0.2, 1.0), ..Default::default() }, ..Default::default() },
        })
        .collect();
    let car = sim.state.entities.find("car").unwrap().id;
    let pid = sim.state.player.unwrap();
    sim.enter_vehicle(pid, car);
    let start = sim.snapshot();
    let start_recording = sim.recording.ticks();
    for f in &script {
        sim.step(f);
    }
    let hash = sim.state_hash();
    // Rewind across the pause and the speed change, then come back.
    let tick = sim.state.tick;
    assert!(sim.rewind_to(tick - 150));
    assert!(sim.rewind_to(tick));
    assert_eq!(sim.state_hash(), hash, "rewind is exact");
    // Replaying the recorded inputs from the snapshot lands on the same state.
    let mut again = Sim::new(Hook::KEY, 1).unwrap();
    again.restore(start);
    for f in sim.recording.iter().skip(start_recording as usize) {
        again.step(f);
    }
    assert_eq!(again.state_hash(), hash, "replay is exact");
    assert!(sim.state.entities.get(b).is_some());
}

#[test]
fn a_game_picks_its_rewind_window() {
    let mut sim = sim();
    hook(&mut sim).history = Some(0.0);
    sim.history.clear();
    sim.run(120, &InputFrame::default());
    assert!(sim.history.snapshots.is_empty(), "rewind off");
    hook(&mut sim).history = Some(2.0);
    sim.run(600, &InputFrame::default());
    let span = sim.history.newest_tick().unwrap() - sim.history.oldest_tick().unwrap();
    assert!(span <= 2 * 60 + 20, "about two seconds kept: {span} ticks");
}

#[test]
fn old_input_frames_read_and_write_as_before() {
    // A frame written before the controller fields existed.
    let old = r#"{"move_dir":[0.5,-1.0],"vertical":0.0,"aim":null,"held":5,"pressed":1}"#;
    let f: InputFrame = serde_json::from_str(old).unwrap();
    assert_eq!(f.move_dir, Vec2::new(0.5, -1.0));
    assert_eq!((f.held, f.pressed), (5, 1));
    assert!(f.pad.is_idle() && f.cmds.is_empty() && !f.activity && f.device == Device::Keyboard);
    // Untouched new fields are not written, so old readers and stored replays see the same text.
    assert_eq!(serde_json::to_string(&f).unwrap(), old);
    // New fields round-trip.
    let g = InputFrame {
        pad: PadState {
            held: pad::SOUTH | pad::R2,
            pressed: pad::SOUTH,
            stick_l: Vec2::new(0.0, 1.0),
            r2: 0.8,
            ..Default::default()
        },
        mouse_delta: Vec2::new(3.0, -2.0),
        wheel: 1.0,
        activity: true,
        device: Device::PlayStation,
        cmds: vec!["ui.accept continue".into()],
        ..Default::default()
    };
    let back: InputFrame = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
    assert_eq!(back, g);
    assert_eq!(pad::from_name("cross"), Some(pad::SOUTH));
    assert_eq!(pad::from_name("RT"), Some(pad::R2));
    assert_eq!(pad::name(pad::TOUCHPAD), Some("touchpad"));
    assert_eq!(Device::from_pad_name("DualSense Wireless Controller"), Device::PlayStation);
    assert_eq!(Device::from_pad_name("Xbox Series X Controller"), Device::Xbox);
    assert_eq!(Device::from_pad_name("8BitDo Pro 2"), Device::Gamepad);
}
