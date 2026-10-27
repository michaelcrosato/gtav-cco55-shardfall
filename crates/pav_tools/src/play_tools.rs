//! Tools for games built on the engine (`pav_core::games`): the starter and any game an agent
//! adds there.

use anyhow::{Result, bail};
use pav_core::Sim;
use pav_core::games::{self, Outcome};
use serde_json::json;

use crate::session::Session;
use crate::tools::{Args, Output, get_bool, get_str, get_u64};

/// `play`: the running game's HUD, outcome and state; `bot=true` lets its bot play on;
/// `seeds=N` plays fresh runs on seeds 1..N with the bot (each given `cmd` first, if any) and
/// leaves the session alone.
pub fn t_play(s: &mut Session, a: &Args) -> Result<Output> {
    let keys = || games::all().into_iter().map(|g| g.key).collect::<Vec<_>>().join(", ");
    let Some(key) = s.sim.play().map(|g| g.key()) else {
        bail!("no game is running: `load scene=<key>` with one of: {}", keys());
    };
    let bot = get_bool(a, "bot", false)?;
    let ticks = get_u64(a, "ticks", 3600)?;
    let hz = s.sim.config.tick_rate.hz() as f64;
    if a.contains_key("seeds") {
        if !bot {
            bail!("seeds= runs the bot: add bot=true");
        }
        let n = get_u64(a, "seeds", 1)?.clamp(1, 200);
        let scene = s.sim.state.scene.clone();
        let (mut runs, mut not_won) = (Vec::new(), Vec::new());
        let mut wins = 0;
        for seed in 1..=n {
            let mut sim = Sim::new(&scene, seed)?;
            sim.config = s.sim.config.clone();
            if let Some(c) = get_str(a, "cmd") {
                sim.step(&pav_core::InputFrame { cmds: vec![c.to_string()], activity: true, ..Default::default() });
            }
            let (outcome, played) = sim.run_bot(ticks);
            runs.push(json!([seed, outcome, round2(played as f64 / hz)]));
            if outcome == Outcome::Won {
                wins += 1;
            } else {
                // The state where the bot lost or ran out of time, to see why.
                not_won.push(json!({ "seed": seed, "status": sim.play().map(|g| g.status(&sim)) }));
            }
        }
        return Ok(Output::Json(json!({
            "game": key,
            "wins": format!("{wins}/{n}"),
            "cols": "seed outcome seconds",
            "runs": runs,
            "not_won": not_won,
        })));
    }
    if let Some(c) = get_str(a, "cmd") {
        let f = pav_core::InputFrame { cmds: vec![c.to_string()], activity: true, ..Default::default() };
        s.sim.step(&f);
        s.keep_events();
        s.sync_camera();
    }
    let mut played = None;
    if bot {
        let (_, t) = s.sim.run_bot(ticks);
        s.keep_events();
        s.sync_camera();
        played = Some(round2(t as f64 / hz));
    }
    let g = s.sim.play().expect("the game is still running");
    let mut out = json!({
        "game": key,
        "outcome": g.outcome(),
        "bot_seconds": played,
        "time": round2(s.sim.time()),
        "hud": g.hud(&s.sim),
        "status": g.status(&s.sim),
    });
    if let Some(p) = s.sim.present() {
        out["present"] = present_summary(&p);
    }
    Ok(Output::Json(out))
}

/// What a game shows, in a few words: the page and its focused element, the objective, the
/// wheel, toasts, banners and where the camera is (`ui` has the whole model).
fn present_summary(p: &pav_core::present::PlayFrame) -> serde_json::Value {
    use pav_core::present::ScreenBody;
    let ui = &p.ui;
    let focus = ui.screen.as_ref().and_then(|s| {
        let menu = match &s.body {
            ScreenBody::Menu(m) => Some(m),
            ScreenBody::Title(t) => t.menu.as_ref(),
            _ => None,
        };
        match (&s.body, menu) {
            (_, Some(m)) => m.items.get(m.focus).map(|i| i.label.clone()),
            (ScreenBody::Cards(c), _) => c.cards.get(c.focus).map(|c| c.title.clone()),
            _ => None,
        }
    });
    let r3 = |v: glam::Vec3| [round2(v.x as f64), round2(v.y as f64), round2(v.z as f64)];
    json!({
        "screen": ui.screen.as_ref().map(|s| s.id.clone()),
        "tab": ui.screen.as_ref().and_then(|s| s.tabs.get(s.tab).cloned()),
        "focus": focus,
        "objective": ui.hud.as_ref().and_then(|h| h.objective.clone()),
        "wheel": ui.wheel.as_ref().map(|w| w.focus.and_then(|f| w.slots.get(f)).map(|s| s.label.clone())),
        "toasts": ui.toasts.iter().map(|t| t.title.clone()).collect::<Vec<_>>(),
        "banners": ui.banners.iter().map(|b| b.title.clone()).collect::<Vec<_>>(),
        "camera": p.camera.map(|c| json!({ "eye": r3(c.eye), "target": r3(c.target), "fov": c.fov })),
    })
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}
