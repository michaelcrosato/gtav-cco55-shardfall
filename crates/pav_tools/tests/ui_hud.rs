//! The UI a hook game publishes reaches the tools: `ui` returns it as JSON, `capture hud=true`
//! and `filmstrip hud=true` draw it (pav_ui, offscreen) over the picture.

use anyhow::{Result, anyhow};
use pav_core::Sim;
use pav_core::games::{self, Entry, Game, GameBox, entry};
use pav_core::present::PlayFrame;
use pav_core::room::RoomDef;
use pav_tools::{Output, Session};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// A game with no rules that shows the UI kit's demo HUD over the starter's room.
#[derive(Clone, Serialize, Deserialize)]
struct HudGame;

impl Game for HudGame {
    const KEY: &'static str = "ui_hud_test";

    fn present(&self, sim: &Sim) -> Option<PlayFrame> {
        let mut ui = pav_ui::demo::hud();
        ui.time = sim.time() as f32;
        Some(PlayFrame { ui, ..Default::default() })
    }
}

fn build(sim: &mut Sim, _rest: Option<&str>) -> Result<()> {
    let def = RoomDef::parse(include_str!("../../pav_core/src/games/starter.toml")).map_err(|e| anyhow!("{e}"))?;
    pav_core::scenes::build_standalone_room(sim, HudGame::KEY, def);
    sim.state.play = Some(GameBox::new(HudGame));
    Ok(())
}

static HUD_GAME: Entry = entry::<HudGame>("Test: the UI kit's demo HUD over a room.", build);

fn call(s: &mut Session, tool: &str, args: Value) -> Result<Output> {
    pav_tools::call(s, tool, args.as_object().unwrap())
}

fn pixels(out: Output) -> (Value, Vec<u8>) {
    let Output::Image { png, meta, .. } = out else { panic!("an image") };
    let mut reader = png::Decoder::new(std::io::Cursor::new(png)).read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    buf.truncate(info.buffer_size());
    (meta, buf)
}

#[test]
fn a_games_ui_shows_in_the_tools() {
    games::register(&HUD_GAME);
    let dir = std::env::temp_dir().join(format!("pav-ui-hud-{}", std::process::id()));
    let out = |name: &str| dir.join(name).to_string_lossy().into_owned();

    // `ui` without a PlayFrame says so; with one it returns the model, compactly.
    let mut empty = Session::new("empty", 1).unwrap();
    let err = call(&mut empty, "ui", json!({})).err().expect("no PlayFrame").to_string();
    assert!(err.contains("PlayFrame"), "{err}");
    let mut s = Session::new(HudGame::KEY, 1).unwrap();
    let Output::Json(ui) = call(&mut s, "ui", json!({})).unwrap() else { panic!("json") };
    assert_eq!(ui["hud"]["money"]["amount"], 12_480, "{ui}");
    assert_eq!(ui["prompts"][0]["button"], "north");
    assert!(ui.get("screen").is_none() && ui.get("toasts").is_none(), "nulls and empty lists are left out: {ui}");
    let Output::Json(focus) = call(&mut s, "ui", json!({"focus": true})).unwrap() else { panic!("json") };
    assert_eq!(focus, json!({"focus": null}), "the HUD has nothing focused");
    let err = call(&mut s, "ui", json!({"focsu": true})).err().expect("unknown argument").to_string();
    assert!(err.contains("focus"), "names the nearest argument: {err}");

    // The same moment with and without the UI: the HUD's corners change, the middle mostly not.
    let (w, h) = (320u32, 180u32);
    let (_, plain) = pixels(call(&mut s, "capture", json!({"width": w, "height": h, "out": out("plain.png")})).unwrap());
    let (meta, hud) =
        pixels(call(&mut s, "capture", json!({"width": w, "height": h, "hud": true, "out": out("hud.png")})).unwrap());
    assert_eq!(meta["hud"], true, "{meta}");
    let changed = |x0: u32, y0: u32, x1: u32, y1: u32| {
        let mut n = 0;
        for y in y0..y1 {
            for x in x0..x1 {
                let i = ((y * w + x) * 4) as usize;
                if (0..3).any(|c| plain[i + c].abs_diff(hud[i + c]) > 24) {
                    n += 1;
                }
            }
        }
        n as f32 / ((x1 - x0) * (y1 - y0)) as f32
    };
    // The radar fills the bottom-left corner (at 180 px high: x 16..79, y 126..165), the money
    // the top right.
    assert!(changed(20, 128, 76, 162) > 0.6, "radar: {}", changed(20, 128, 76, 162));
    assert!(changed(250, 10, 305, 30) > 0.1, "money: {}", changed(250, 10, 305, 30));
    assert!(changed(0, 0, w, h) < 0.5, "most of the world still shows: {}", changed(0, 0, w, h));

    // A scene without a PlayFrame captures as before, and says nothing was drawn.
    let (meta, _) =
        pixels(call(&mut empty, "capture", json!({"width": 64, "height": 36, "hud": true, "out": out("empty.png")})).unwrap());
    assert!(meta["hud"].as_str().is_some_and(|m| m.contains("no PlayFrame")), "{meta}");

    // Filmstrips draw it on every frame.
    let (_, strip) = pixels(
        call(
            &mut s,
            "filmstrip",
            json!({"frames": 2, "every": 1, "columns": 2, "width": 160, "height": 90, "hud": true, "out": out("strip.png")}),
        )
        .unwrap(),
    );
    assert_eq!(strip.len(), 320 * 90 * 4);
    let _ = std::fs::remove_dir_all(&dir);
}
