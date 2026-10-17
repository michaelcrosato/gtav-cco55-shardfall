//! The UI a hook game publishes (`PlayFrame::ui`, drawn by `pav_ui`) for agents: `ui` returns
//! the model as JSON, so the HUD and menus are checkable without pixels; `capture hud=true`
//! and `filmstrip hud=true` draw it over the picture (`Session::render_hud`).

use anyhow::{Result, bail};
use pav_core::issue::{self, Code, Issue};
use pav_core::present::{ScreenBody, UiModel};
use serde_json::{Value, json};

use crate::session::Session;
use crate::tools::{Args, Output, get_bool};

/// `ui`: the running game's UI model as compact JSON (nulls and empty lists left out);
/// `focus=true` reports only the focused element.
pub fn t_ui(s: &mut Session, a: &Args) -> Result<Output> {
    allowed(a, &["focus"])?;
    let Some(frame) = s.sim.present() else {
        bail!("no UI: this scene publishes no PlayFrame (hook games do, from Game::present)");
    };
    if get_bool(a, "focus", false)? {
        return Ok(Output::Json(focus(&frame.ui)));
    }
    let mut v = serde_json::to_value(&frame.ui)?;
    compact(&mut v);
    Ok(Output::Json(v))
}

/// The focused element of the screen, wheel or phone, with the id the pointer would report.
pub fn focus(ui: &UiModel) -> Value {
    if let Some(s) = &ui.screen {
        let tab = s.tabs.get(s.tab).cloned();
        let item = match &s.body {
            ScreenBody::Menu(m) => m.items.get(m.focus).map(|i| json!({"id": i.id, "label": i.label, "value": i.value})),
            ScreenBody::Title(t) => t
                .menu
                .as_ref()
                .and_then(|m| m.items.get(m.focus))
                .map(|i| json!({"id": i.id, "label": i.label, "value": i.value})),
            ScreenBody::Cards(c) => c.cards.get(c.focus).map(|c| json!({"id": c.id, "label": c.title})),
            ScreenBody::Map(m) => Some(json!({"cursor": [m.cursor.x, m.cursor.y], "selected": m.selected})),
            _ => None,
        };
        return json!({"screen": s.id, "tab": tab, "focus": item});
    }
    if let Some(w) = &ui.wheel {
        let slot = w.focus.and_then(|i| w.slots.get(i).map(|s| json!({"id": format!("wheel:{i}"), "label": s.label})));
        return json!({"wheel": w.id, "focus": slot});
    }
    if let Some(p) = &ui.phone {
        let item = if p.tiles.is_empty() {
            p.rows.get(p.focus).map(|r| json!({"id": format!("phone_row:{}", p.focus), "label": r.title}))
        } else {
            p.tiles.get(p.focus).map(|t| json!({"id": t.id, "label": t.label}))
        };
        return json!({"phone": p.app, "focus": item});
    }
    json!({"focus": null})
}

/// Drops nulls and empty lists and objects (defaults) so the reply stays short.
fn compact(v: &mut Value) {
    match v {
        Value::Object(map) => {
            map.values_mut().for_each(compact);
            map.retain(|_, x| {
                !(x.is_null() || x.as_array().is_some_and(Vec::is_empty) || x.as_object().is_some_and(|o| o.is_empty()))
            });
        }
        Value::Array(items) => items.iter_mut().for_each(compact),
        _ => {}
    }
}

fn allowed(a: &Args, known: &[&str]) -> Result<()> {
    // The one-shot CLI applies these before dispatch and keeps them in the argument map.
    if let Some(k) = a.keys().find(|k| !known.contains(&k.as_str()) && !["scene", "seed", "ticks"].contains(&k.as_str())) {
        bail!(
            "{}",
            Issue::error(k.as_str(), Code::UnknownKey, "unknown argument")
                .expected(known.join(", "))
                .maybe_fix(issue::suggestion(k, known.iter().copied()))
        );
    }
    Ok(())
}
