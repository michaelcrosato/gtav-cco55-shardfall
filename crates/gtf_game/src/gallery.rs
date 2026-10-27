//! The UI gallery (a developer screen: the `freeway/gallery` scene, not in the player's menus):
//! every widget of the 10-foot UI kit with sample data from the game, one page at a time (L1/R1
//! or left/right), so the whole kit can be reviewed in the window and in headless captures
//! (`gtf capture hud=true`).

use glam::{Vec2, Vec3};
use pav_core::Color;
use pav_core::present::*;

use crate::hud::{WheelState, lot_blips};

/// Page names, in order.
pub const PAGES: [&str; 10] = ["hud", "title", "menu", "cards", "map", "text", "results", "wheel", "phone", "moments"];

fn screen(id: &str, title: &str, body: ScreenBody, backdrop: Backdrop) -> Screen {
    Screen {
        id: format!("gallery/{id}"),
        title: title.into(),
        tabs: Vec::new(),
        tab: 0,
        hints: vec![
            Prompt { button: PadButton::L1, label: "Previous".into(), hold: None },
            Prompt { button: PadButton::R1, label: "Next".into(), hold: None },
            Prompt { button: PadButton::East, label: "Back".into(), hold: None },
        ],
        body,
        backdrop,
    }
}

fn blip(x: f32, z: f32, icon: BlipIcon, hex: &str, label: &str, pulse: bool) -> Blip {
    Blip { pos: Vec2::new(x, z), icon, color: Color::hex(hex), label: Some(label.into()), clamp: true, height: 0, pulse }
}

/// The HUD with everything on: radar with route, cones and search circle, vitals, cash with a
/// payout, three flashing stars, weapon and ammo, objective, timer, crosshair, a meter.
pub fn hud_page(player: Vec2) -> UiModel {
    let mut blips = lot_blips();
    blips.push(blip(player.x + 25.0, player.y - 30.0, BlipIcon::Police, "#3d8bff", "Police", false));
    blips.push(blip(player.x - 40.0, player.y + 10.0, BlipIcon::Objective, "#ffc145", "Drop-off", true));
    let hud = Hud {
        minimap: Some(Minimap {
            center: player,
            heading: 0.3,
            radius: 60.0,
            player,
            player_heading: 0.5,
            blips,
            route: vec![
                player,
                player + Vec2::new(0.0, -18.0),
                player + Vec2::new(-22.0, -18.0),
                player + Vec2::new(-40.0, 10.0),
            ],
            cones: vec![ConeBlip {
                pos: player + Vec2::new(25.0, -30.0),
                dir: 2.6,
                angle: 0.9,
                range: 22.0,
                color: Color::hex("#3d8bff"),
            }],
            circles: vec![CircleBlip {
                pos: player + Vec2::new(10.0, -5.0),
                radius: 35.0,
                color: Color::hex("#ff4d4d"),
                flashing: true,
            }],
        }),
        vitals: Some(Vitals { health: 0.72, armor: 0.4, ability: 0.55, ability_color: Color::hex("#ffc145") }),
        money: Some(Money { amount: 12_480, delta: 1_500, delta_age: 0.8 }),
        wanted: Some(Wanted { stars: 3, max: 5, flashing: true }),
        weapon: Some(WeaponHud { name: "Pistol".into(), icon: "pistol".into(), clip: Some(9), reserve: Some(48) }),
        objective: Some("Lose the cops. Then take the van to the ~y~drop-off~s~.".into()),
        timer: Some(TimerHud { label: "TIME".into(), seconds: 74.5, warn: false }),
        vehicle: Some(VehicleHud {
            name: "Brisa GT".into(),
            station: Some("Freeway FM".into()),
            song: Some("Okami Kaze — Night Lanes".into()),
            age: 1.0,
        }),
        crosshair: Some(Crosshair { spread: 2.0, hit: 0.6, kill: 0.0, target: true }),
        meters: vec![Meter { label: "DETECTION".into(), value: 0.35, color: Color::hex("#ffc145") }],
        opacity: 1.0,
    };
    UiModel {
        hud: Some(hud),
        prompts: vec![Prompt { button: PadButton::North, label: "Enter vehicle".into(), hold: None }],
        help: Some(Help { text: "Hold ~btn:l2~ to aim and ~btn:r2~ to fire. ~btn:r1~ takes cover.".into(), age: 1.0 }),
        subtitle: Some(Subtitle {
            speaker: Some("MACK".into()),
            text: "Nobody panic. I'm panicking.".into(),
            color: Some(Color::hex("#3d8bff")),
        }),
        ..Default::default()
    }
}

pub fn title_page() -> UiModel {
    UiModel {
        screen: Some(Screen {
            id: "gallery/title".into(),
            title: "GRAND THEFT FREEWAY".into(),
            tabs: Vec::new(),
            tab: 0,
            hints: Vec::new(),
            body: ScreenBody::Title(TitleCard {
                prompt: Some("PRESS ANY BUTTON".into()),
                menu: None,
                caption: Some("PALOMA BAY · BX".into()),
                small_logo: false,
            }),
            backdrop: Backdrop::Clear,
        }),
        ..Default::default()
    }
}

pub fn menu_page() -> UiModel {
    let item = |id: &str, label: &str, value: ItemValue, enabled: bool, note: Option<&str>| MenuItem {
        id: id.into(),
        label: label.into(),
        value,
        enabled,
        note: note.map(String::from),
    };
    let menu = MenuList {
        items: vec![
            item("music", "Radio and music", ItemValue::Slider { value: 0.7, text: "70%".into() }, true, None),
            item("subtitles", "Subtitles", ItemValue::Toggle { on: true }, true, None),
            item(
                "glyphs",
                "Button prompts",
                ItemValue::Choice { options: vec!["Automatic".into(), "PlayStation".into(), "Xbox".into()], index: 1 },
                true,
                Some("Prompts follow the last device you used."),
            ),
            item("continue", "Continue", ItemValue::Button, false, Some("No saved game yet")),
            item("free_roam", "Free Roam", ItemValue::Button, true, None),
        ],
        focus: 2,
    };
    let mut s = screen("menu", "SETTINGS", ScreenBody::Menu(menu), Backdrop::Dim);
    s.tabs = vec!["AUDIO".into(), "DISPLAY".into(), "CONTROLS".into(), "ACCESSIBILITY".into()];
    s.tab = 2;
    s.hints = vec![
        Prompt { button: PadButton::L1, label: "Tab".into(), hold: None },
        Prompt { button: PadButton::South, label: "Change".into(), hold: None },
        Prompt { button: PadButton::East, label: "Back".into(), hold: None },
    ];
    UiModel { screen: Some(s), ..Default::default() }
}

pub fn cards_page() -> UiModel {
    let card = |id: &str, title: &str, sub: &str, lines: &[&str], bars: &[(&str, f32)], hex: &str| Card {
        id: id.into(),
        title: title.into(),
        subtitle: Some(sub.into()),
        lines: lines.iter().map(|l| l.to_string()).collect(),
        bars: bars.iter().map(|(l, v)| (l.to_string(), *v)).collect(),
        accent: Color::hex(hex),
        icon: None,
        enabled: true,
    };
    let row = CardRow {
        cards: vec![
            card("dez", "DEZ", "Slipstream", &["$12,480"], &[("Driving", 0.9), ("Shooting", 0.45), ("Stamina", 0.6)], "#6fe28a"),
            card("mack", "MACK", "Deadeye", &["$48,020"], &[("Driving", 0.55), ("Shooting", 0.85), ("Stamina", 0.4)], "#3d8bff"),
            card("roxy", "ROXY", "Overdrive", &["$7,915"], &[("Driving", 0.7), ("Shooting", 0.6), ("Stamina", 0.9)], "#ff3e8a"),
        ],
        focus: 1,
    };
    let mut s = screen("cards", "STATS", ScreenBody::Cards(row), Backdrop::Dim);
    s.tabs = vec!["MAP".into(), "BRIEF".into(), "STATS".into(), "SETTINGS".into(), "GAME".into()];
    s.tab = 2;
    UiModel { screen: Some(s), ..Default::default() }
}

pub fn map_page(player: Vec2) -> UiModel {
    let mut blips = lot_blips();
    blips.push(blip(player.x, player.y, BlipIcon::Waypoint, "#ff3e8a", "Waypoint", false));
    let view = MapView {
        center: player,
        zoom: 160.0,
        cursor: player + Vec2::new(14.0, 5.0),
        legend: true,
        blips,
        route: vec![player, player + Vec2::new(14.0, 5.0)],
        selected: Some("Bayside Arms — weapons and armour".into()),
    };
    let mut s = screen("map", "MAP", ScreenBody::Map(view), Backdrop::Solid);
    s.tabs = vec!["MAP".into(), "BRIEF".into(), "STATS".into(), "SETTINGS".into(), "GAME".into()];
    s.hints = vec![
        Prompt { button: PadButton::South, label: "Waypoint".into(), hold: None },
        Prompt { button: PadButton::West, label: "Legend".into(), hold: None },
        Prompt { button: PadButton::L2, label: "Zoom".into(), hold: None },
        Prompt { button: PadButton::East, label: "Back".into(), hold: None },
    ];
    UiModel { screen: Some(s), ..Default::default() }
}

pub fn text_page(age: f32) -> UiModel {
    let lines = [
        "The Freeway Job: hit the armoured truck on Route 404.",
        "Pick the ~y~approach~s~ on the planning board.",
        "Roxy needs a fast car; Mack wants the right crew.",
    ];
    let text = TextCard { heading: "BRIEF".into(), lines: lines.iter().map(|l| l.to_string()).collect() };
    let mut s = screen("text", "BRIEF", ScreenBody::Text(text), Backdrop::Dim);
    s.tabs = vec!["MAP".into(), "BRIEF".into(), "STATS".into(), "SETTINGS".into(), "GAME".into()];
    s.tab = 1;
    let mut ui = UiModel { screen: Some(s), ..Default::default() };
    // A loading card is a page of its own; show its bar under the brief's title in a toast.
    ui.toasts.push(Toast { id: 1, icon: "phone".into(), title: "HOLLIS".into(), body: "The truck runs at 22:00.".into(), age });
    ui
}

pub fn loading_page() -> UiModel {
    let card = LoadingCard {
        heading: "THE FREEWAY JOB".into(),
        tip: "Tip: ~btn:r1~ while driving locks the handbrake for tight turns.".into(),
        progress: 0.62,
        tag: None,
    };
    UiModel { screen: Some(screen("loading", "", ScreenBody::Loading(card), Backdrop::Solid)), ..Default::default() }
}

pub fn results_page() -> UiModel {
    let results = Results {
        heading: "MISSION PASSED".into(),
        medal: Some(Medal::Gold),
        rows: vec![
            ("Time".into(), "06:42".into()),
            ("Accuracy".into(), "78%".into()),
            ("Headshots".into(), "11".into()),
            ("Vehicle damage".into(), "Light".into()),
        ],
        cash: Some(25_000),
    };
    UiModel { screen: Some(screen("results", "", ScreenBody::Results(results), Backdrop::Dim)), ..Default::default() }
}

pub fn wheel_page(player: Vec2) -> UiModel {
    let wheel = WheelState { open_for: 1.0, focus: Some(1) }.ui(1, false);
    let (hud, _, _) = crate::hud::hud(&crate::hud::HudInput {
        player,
        heading: 0.0,
        camera_yaw: 0.0,
        money: (12_480, 0, 10.0),
        weapon: 1,
        bat: false,
        ability: (0.8, pav_core::Color::hex("#6fe28a")),
        help: None,
        map_blips: lot_blips(),
        route: Vec::new(),
        vitals: (1.0, 0.5),
        ammo: Some((12, 48)),
        crosshair: None,
    });
    UiModel { hud: Some(hud), wheel: Some(wheel), overlay: Overlay { vignette: 0.5, ..Default::default() }, ..Default::default() }
}

pub fn phone_page() -> UiModel {
    let tile = |id: &str, label: &str, icon: &str, badge: u32| PhoneTile {
        id: id.into(),
        label: label.into(),
        icon: icon.into(),
        badge,
    };
    let phone = Phone {
        app: "home".into(),
        title: "Papaya".into(),
        tiles: vec![
            tile("contacts", "Contacts", "contacts", 0),
            tile("messages", "Messages", "messages", 2),
            tile("camera", "Camera", "camera", 0),
            tile("map", "Map", "map", 0),
            tile("jobs", "Jobs", "jobs", 1),
            tile("radio", "Radio", "radio", 0),
            tile("settings", "Settings", "settings", 0),
            tile("lifehakr", "LifeHakr", "web", 0),
        ],
        rows: Vec::new(),
        focus: 1,
        clock: "21:47".into(),
    };
    UiModel { phone: Some(phone), ..Default::default() }
}

/// Banner, toasts, subtitle, prompts with a hold ring, help, world markers.
pub fn moments_page(age: f32, player: Vec3) -> UiModel {
    UiModel {
        banners: vec![Banner {
            kind: BannerKind::Passed,
            title: "MISSION PASSED".into(),
            subtitle: Some("The Freeway Job — Smart approach".into()),
            age: 1.2,
            duration: 6.0,
        }],
        toasts: vec![
            Toast { id: 1, icon: "messages".into(), title: "ROXY".into(), body: "Bring the Brisa, not the van.".into(), age },
            Toast { id: 2, icon: "police".into(), title: "WANTED LEVEL".into(), body: "Lost the cops.".into(), age: age + 1.5 },
        ],
        subtitle: Some(Subtitle {
            speaker: Some("DEZ".into()),
            text: "Freeway's clear. Go, go, go!".into(),
            color: Some(Color::hex("#6fe28a")),
        }),
        prompts: vec![
            Prompt { button: PadButton::North, label: "Steal the Brisa".into(), hold: Some(0.45) },
            Prompt { button: PadButton::Right, label: "Talk".into(), hold: None },
        ],
        help: Some(Help { text: "Press ~btn:up~ to take out your phone.".into(), age: 2.0 }),
        markers: vec![
            ScreenMarker {
                pos: player + Vec3::new(6.0, 1.5, -10.0),
                icon: BlipIcon::Objective,
                color: Color::hex("#ffc145"),
                label: Some("Drop-off".into()),
                distance: true,
            },
            ScreenMarker {
                pos: player + Vec3::new(-200.0, 2.0, 40.0),
                icon: BlipIcon::Letter('R'),
                color: Color::hex("#ff3e8a"),
                label: Some("Roxy".into()),
                distance: true,
            },
        ],
        ..Default::default()
    }
}

/// Gallery page `i` (wraps), with live values where the page animates.
pub fn page(i: usize, age: f32, player: Vec3) -> UiModel {
    let p2 = Vec2::new(player.x, player.z);
    let mut ui = match i % PAGES.len() {
        0 => hud_page(p2),
        1 => title_page(),
        2 => menu_page(),
        3 => cards_page(),
        4 => map_page(p2),
        5 => {
            if (age as u32 / 4).is_multiple_of(2) {
                text_page(age)
            } else {
                loading_page()
            }
        }
        6 => results_page(),
        7 => wheel_page(p2),
        8 => phone_page(),
        _ => moments_page(age, player),
    };
    if ui.screen.is_none() && !i.is_multiple_of(PAGES.len()) {
        // Pages without a screen name themselves in a help box.
        ui.help.get_or_insert(Help {
            text: format!("UI gallery: {} — ~btn:l1~ ~btn:r1~ pages, ~btn:east~ back", PAGES[i % PAGES.len()]),
            age,
        });
    }
    ui
}
