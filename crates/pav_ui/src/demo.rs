//! Sample models covering every widget, and a procedural map picture: the gallery's pages
//! (`examples/gallery.rs`), the tools' tests and a starting point for a game's own models.

use glam::{Vec2, Vec3};
use pav_core::Color;
use pav_core::present::*;

fn hex(s: &str) -> Color {
    Color::hex(s)
}

fn prompt(button: PadButton, label: &str) -> Prompt {
    Prompt { button, label: label.into(), hold: None }
}

fn blip(x: f32, z: f32, icon: BlipIcon, color: &str) -> Blip {
    Blip { pos: Vec2::new(x, z), icon, color: hex(color), label: None, clamp: false, height: 0, pulse: false }
}

/// Every demo page by name, in gallery order.
pub fn pages() -> Vec<(&'static str, UiModel)> {
    vec![
        ("hud", hud()),
        ("driving", driving()),
        ("title", title()),
        ("main_menu", main_menu()),
        ("attract", attract()),
        ("pause_map", pause_map()),
        ("pause_settings", pause_settings()),
        ("stats_cards", stats_cards()),
        ("brief", brief()),
        ("loading", loading()),
        ("results", results()),
        ("weapon_wheel", weapon_wheel()),
        ("phone", phone()),
        ("phone_messages", phone_messages()),
        ("moments", moments()),
        ("wasted", wasted()),
        ("glyphs", glyph_sheet()),
    ]
}

/// A camera behind the demo player ([`HERE`], looking north-east over the park): world to
/// clip space for an `aspect` (width / height) screen.
pub fn camera(aspect: f32) -> glam::Mat4 {
    let at = Vec3::new(HERE.x, 0.0, HERE.y);
    let view = glam::camera::rh::view::look_at_mat4(at + Vec3::new(-12.0, 9.0, 34.0), at + Vec3::new(4.0, 2.0, -10.0), Vec3::Y);
    let proj = glam::camera::rh::proj::directx::perspective(55f32.to_radians(), aspect.max(0.1), 0.1, 2000.0);
    proj * view
}

/// Where the demo player stands: by the park, where the freeway loop passes.
const HERE: Vec2 = Vec2::new(-240.0, 260.0);

/// A blip `dx, dz` metres from [`HERE`].
fn near(dx: f32, dz: f32, icon: BlipIcon, color: &str) -> Blip {
    blip(HERE.x + dx, HERE.y + dz, icon, color)
}

fn radar(heading: f32) -> Minimap {
    let mut blips = vec![
        near(150.0, 60.0, BlipIcon::Letter('D'), "#ffc145"),
        near(-90.0, -60.0, BlipIcon::Safehouse, "#f4f1ea"),
        near(60.0, -150.0, BlipIcon::Gun, "#f4f1ea"),
        near(-140.0, 130.0, BlipIcon::Shop, "#f4f1ea"),
        near(10.0, 180.0, BlipIcon::Police, "#3d8bff"),
        near(95.0, 90.0, BlipIcon::Enemy, "#ff4d4d"),
        near(110.0, 110.0, BlipIcon::Enemy, "#ff4d4d"),
        near(-40.0, 40.0, BlipIcon::Ally, "#5ca0ff"),
    ];
    let mut far = near(900.0, -700.0, BlipIcon::Waypoint, "#ff3e8a");
    far.clamp = true;
    blips.push(far);
    let mut obj = near(-60.0, -120.0, BlipIcon::Objective, "#ffc145");
    obj.pulse = true;
    obj.height = 1;
    blips.push(obj);
    let at = |dx: f32, dz: f32| HERE + Vec2::new(dx, dz);
    Minimap {
        center: HERE,
        heading,
        radius: 220.0,
        player: HERE,
        player_heading: heading + 0.3,
        blips,
        route: vec![at(0.0, 0.0), at(0.0, -96.0), at(-80.0, -96.0), at(-80.0, -150.0)],
        cones: vec![ConeBlip { pos: at(10.0, 180.0), dir: 3.4, angle: 1.0, range: 90.0, color: hex("#3d8bff") }],
        circles: vec![CircleBlip { pos: at(30.0, 150.0), radius: 70.0, color: hex("#3d8bff"), flashing: true }],
    }
}

fn vitals() -> Vitals {
    Vitals { health: 0.78, armor: 0.45, ability: 0.6, ability_color: hex("#ffc145") }
}

/// On foot, armed and wanted.
pub fn hud() -> UiModel {
    UiModel {
        hud: Some(Hud {
            minimap: Some(radar(0.35)),
            vitals: Some(vitals()),
            money: Some(Money { amount: 12_480, delta: 1_500, delta_age: 1.2 }),
            wanted: Some(Wanted { stars: 2, max: 5, flashing: false }),
            weapon: Some(WeaponHud { name: "Pistol".into(), icon: "pistol".into(), clip: Some(12), reserve: Some(48) }),
            objective: Some("Lose the ~b~cops~s~.".into()),
            crosshair: Some(Crosshair { spread: 2.0, hit: 0.0, kill: 0.0, target: false }),
            ..Default::default()
        }),
        prompts: vec![prompt(PadButton::North, "Enter vehicle")],
        time: 12.3,
        ..Default::default()
    }
}

/// In a car: radar turned with the camera, the vehicle card, a timer and a meter.
pub fn driving() -> UiModel {
    UiModel {
        hud: Some(Hud {
            minimap: Some(radar(2.2)),
            vitals: Some(Vitals { health: 0.2, armor: 0.0, ability: 0.95, ability_color: hex("#ffc145") }),
            money: Some(Money { amount: 48_210, delta: -250, delta_age: 0.6 }),
            wanted: Some(Wanted { stars: 3, max: 5, flashing: true }),
            objective: Some("Reach the ~y~drop-off~s~ before the timer runs out.".into()),
            timer: Some(TimerHud { label: "Drop-off".into(), seconds: 47.0, warn: false }),
            vehicle: Some(VehicleHud {
                name: "Okami Kaze".into(),
                station: Some("Freeway FM".into()),
                song: Some("Neon Overpass · The Lane Splitters".into()),
                age: 1.0,
            }),
            meters: vec![Meter { label: "Signal".into(), value: 0.62, color: hex("#6fe28a") }],
            ..Default::default()
        }),
        subtitle: Some(Subtitle {
            speaker: Some("Mack".into()),
            text: "Nobody panic. I'm panicking.".into(),
            color: Some(hex("#ff7a3d")),
        }),
        time: 20.0,
        ..Default::default()
    }
}

/// The title screen over the live city.
pub fn title() -> UiModel {
    UiModel {
        screen: Some(Screen {
            id: "title".into(),
            title: "Grand Theft Freeway-BX".into(),
            tabs: vec![],
            tab: 0,
            hints: vec![],
            body: ScreenBody::Title(TitleCard {
                prompt: Some("Press any button".into()),
                menu: None,
                caption: Some("San Paloma".into()),
                small_logo: false,
            }),
            backdrop: Backdrop::Clear,
        }),
        perf: false,
        time: 0.3,
        ..Default::default()
    }
}

/// The main menu under the logo.
pub fn main_menu() -> UiModel {
    let item = |id: &str, label: &str| MenuItem {
        id: id.into(),
        label: label.into(),
        value: ItemValue::Button,
        enabled: true,
        note: None,
    };
    let mut quit = item("quit", "Quit");
    quit.enabled = true;
    let mut continue_ = item("continue", "Continue");
    continue_.enabled = false;
    UiModel {
        screen: Some(Screen {
            id: "main".into(),
            title: "Grand Theft Freeway-BX".into(),
            tabs: vec![],
            tab: 0,
            hints: vec![prompt(PadButton::South, "Select"), prompt(PadButton::East, "Back")],
            body: ScreenBody::Title(TitleCard {
                prompt: None,
                menu: Some(MenuList {
                    items: vec![
                        continue_,
                        item("new", "New game"),
                        item("roam", "Free roam"),
                        item("settings", "Settings"),
                        item("about", "About"),
                        quit,
                    ],
                    focus: 1,
                }),
                caption: None,
                small_logo: false,
            }),
            backdrop: Backdrop::Clear,
        }),
        perf: false,
        ..Default::default()
    }
}

/// Attract mode: the logo small in a corner.
pub fn attract() -> UiModel {
    UiModel {
        screen: Some(Screen {
            id: "attract".into(),
            title: "Grand Theft Freeway-BX".into(),
            tabs: vec![],
            tab: 0,
            hints: vec![prompt(PadButton::South, "Play")],
            body: ScreenBody::Title(TitleCard {
                prompt: None,
                menu: None,
                caption: Some("A city that reacts".into()),
                small_logo: true,
            }),
            backdrop: Backdrop::Clear,
        }),
        subtitle: Some(Subtitle { speaker: None, text: "Paloma Bay, golden hour.".into(), color: None }),
        perf: false,
        ..Default::default()
    }
}

fn pause_tabs() -> Vec<String> {
    ["Map", "Brief", "Stats", "Settings", "Game"].iter().map(|s| s.to_string()).collect()
}

/// The pause map with its legend and the blip under the cursor described.
pub fn pause_map() -> UiModel {
    let mut blips = radar(0.0).blips;
    blips.retain(|b| b.icon != BlipIcon::Waypoint);
    blips.extend([
        near(-240.0, 200.0, BlipIcon::Hospital, "#ff4d4d"),
        near(520.0, -330.0, BlipIcon::Race, "#f4f1ea"),
        near(-150.0, -380.0, BlipIcon::Collectible, "#6fe28a"),
        near(430.0, 160.0, BlipIcon::Activity, "#ff7a3d"),
        near(-260.0, -110.0, BlipIcon::Garage, "#f4f1ea"),
        near(300.0, -60.0, BlipIcon::Heist, "#6fe28a"),
        near(160.0, 300.0, BlipIcon::Jump, "#f4f1ea"),
        near(600.0, 40.0, BlipIcon::Helicopter, "#3d8bff"),
    ]);
    UiModel {
        screen: Some(Screen {
            id: "pause/map".into(),
            title: "Paloma Bay".into(),
            tabs: pause_tabs(),
            tab: 0,
            hints: vec![
                prompt(PadButton::South, "Waypoint"),
                prompt(PadButton::West, "Legend"),
                prompt(PadButton::R2, "Zoom"),
                prompt(PadButton::East, "Back"),
            ],
            body: ScreenBody::Map(MapView {
                center: HERE + Vec2::new(150.0, -40.0),
                zoom: 1100.0,
                cursor: HERE + Vec2::new(60.0, -150.0),
                legend: true,
                blips,
                route: radar(0.0).route,
                selected: Some("~y~Paloma Arms~s~ · weapons, armour and ammunition. Open all night.".into()),
            }),
            backdrop: Backdrop::Dim,
        }),
        perf: false,
        ..Default::default()
    }
}

/// Settings: toggles, sliders and choices, with the focused item's note.
pub fn pause_settings() -> UiModel {
    let it = |id: &str, label: &str, value: ItemValue, note: Option<&str>| MenuItem {
        id: id.into(),
        label: label.into(),
        value,
        enabled: true,
        note: note.map(String::from),
    };
    UiModel {
        screen: Some(Screen {
            id: "settings/audio".into(),
            title: "Settings".into(),
            tabs: pause_tabs(),
            tab: 3,
            hints: vec![prompt(PadButton::South, "Select"), prompt(PadButton::Left, "Adjust"), prompt(PadButton::East, "Back")],
            body: ScreenBody::Menu(MenuList {
                items: vec![
                    it("subtitles", "Subtitles", ItemValue::Toggle { on: true }, None),
                    it(
                        "subtitle_size",
                        "Subtitle size",
                        ItemValue::Choice { options: vec!["Small".into(), "Medium".into(), "Large".into()], index: 1 },
                        None,
                    ),
                    it(
                        "master",
                        "Master volume",
                        ItemValue::Slider { value: 0.8, text: "80%".into() },
                        Some("How loud everything is. Radio, effects and dialogue have their own levels."),
                    ),
                    it("radio", "Radio volume", ItemValue::Slider { value: 0.65, text: "65%".into() }, None),
                    it(
                        "glyphs",
                        "Button prompts",
                        ItemValue::Choice { options: vec!["Auto".into(), "PlayStation".into()], index: 0 },
                        None,
                    ),
                    it("hud_scale", "HUD scale", ItemValue::Slider { value: 0.5, text: "100%".into() }, None),
                    it("invert", "Invert look", ItemValue::Toggle { on: false }, None),
                    it("vibration", "Vibration", ItemValue::Toggle { on: true }, None),
                ],
                focus: 2,
            }),
            backdrop: Backdrop::Dim,
        }),
        perf: false,
        ..Default::default()
    }
}

/// Stats: one big card per protagonist and one for completion.
pub fn stats_cards() -> UiModel {
    let card = |id: &str, title: &str, sub: &str, accent: &str, icon: &str, lines: &[&str], bars: &[(&str, f32)]| Card {
        id: id.into(),
        title: title.into(),
        subtitle: Some(sub.into()),
        lines: lines.iter().map(|s| s.to_string()).collect(),
        bars: bars.iter().map(|(l, v)| (l.to_string(), *v)).collect(),
        accent: hex(accent),
        icon: Some(icon.into()),
        enabled: true,
    };
    UiModel {
        screen: Some(Screen {
            id: "pause/stats".into(),
            title: "Stats".into(),
            tabs: pause_tabs(),
            tab: 2,
            hints: vec![prompt(PadButton::South, "Details"), prompt(PadButton::East, "Back")],
            body: ScreenBody::Cards(CardRow {
                cards: vec![
                    card(
                        "mack",
                        "Mack",
                        "The driver",
                        "#ff7a3d",
                        "car",
                        &["Cash ~g~$48,210~s~"],
                        &[("Driving", 0.82), ("Shooting", 0.55), ("Stamina", 0.7), ("Stealth", 0.4)],
                    ),
                    card(
                        "dee",
                        "Dee",
                        "The fixer",
                        "#3d8bff",
                        "phone",
                        &["Cash ~g~$112,900~s~"],
                        &[("Driving", 0.48), ("Shooting", 0.74), ("Stamina", 0.5), ("Stealth", 0.86)],
                    ),
                    card(
                        "completion",
                        "62%",
                        "Completion",
                        "#6fe28a",
                        "trophy",
                        &[
                            "Missions  ~y~7~s~ / 12",
                            "Activities  ~y~3~s~ / 9",
                            "Collectibles  ~y~14~s~ / 50",
                            "Stunt jumps  ~y~5~s~ / 20",
                        ],
                        &[],
                    ),
                ],
                focus: 0,
            }),
            backdrop: Backdrop::Dim,
        }),
        perf: false,
        ..Default::default()
    }
}

/// The brief: a text card.
pub fn brief() -> UiModel {
    UiModel {
        screen: Some(Screen {
            id: "pause/brief".into(),
            title: "Brief".into(),
            tabs: pause_tabs(),
            tab: 1,
            hints: vec![prompt(PadButton::East, "Back")],
            body: ScreenBody::Text(TextCard {
                heading: "The Overpass Job".into(),
                lines: vec![
                    "Pick up ~y~Dee~s~ at the marina before the tide turns.".into(),
                    "Lose any ~b~police~s~ attention on the way: the client hates sirens.".into(),
                    "Hold ~btn:north~ to bail out of the car if it goes wrong.".into(),
                    "Payout: ~g~$12,000~s~, split three ways.".into(),
                ],
            }),
            backdrop: Backdrop::Dim,
        }),
        perf: false,
        ..Default::default()
    }
}

/// A loading card with a tip and progress.
pub fn loading() -> UiModel {
    UiModel {
        screen: Some(Screen {
            id: "loading".into(),
            title: String::new(),
            tabs: vec![],
            tab: 0,
            hints: vec![],
            body: ScreenBody::Loading(LoadingCard {
                heading: "Paloma Bay".into(),
                tip: "Hold ~btn:l1~ to slow time and pick a weapon from the wheel.".into(),
                progress: 0.62,
            }),
            backdrop: Backdrop::Solid,
        }),
        perf: false,
        time: 1.1,
        ..Default::default()
    }
}

/// Mission results: medal, five big rows and the cash.
pub fn results() -> UiModel {
    let row = |a: &str, b: &str| (a.to_string(), b.to_string());
    UiModel {
        screen: Some(Screen {
            id: "results".into(),
            title: "Results".into(),
            tabs: vec![],
            tab: 0,
            hints: vec![prompt(PadButton::South, "Continue")],
            body: ScreenBody::Results(Results {
                heading: "Mission passed".into(),
                medal: Some(Medal::Gold),
                rows: vec![
                    row("Time", "4:12"),
                    row("Accuracy", "78%"),
                    row("Headshots", "6"),
                    row("Vehicle damage", "Light"),
                    row("Top speed", "212 km/h"),
                ],
                cash: Some(12_000),
            }),
            backdrop: Backdrop::Dim,
        }),
        perf: false,
        ..Default::default()
    }
}

/// The weapon wheel over a faded HUD, time slowed (a cool tint).
pub fn weapon_wheel() -> UiModel {
    let slot = |label: &str, icon: &str, detail: Option<&str>| WheelSlot {
        label: label.into(),
        icon: icon.into(),
        detail: detail.map(String::from),
        enabled: true,
    };
    let mut sniper = slot("Sniper", "sniper", None);
    sniper.enabled = false;
    let mut m = hud();
    if let Some(h) = &mut m.hud {
        h.opacity = 0.35;
        h.crosshair = None;
    }
    m.prompts.clear();
    m.wheel = Some(Wheel {
        id: "weapons".into(),
        slots: vec![
            slot("Pistol", "pistol", Some("12 / 48")),
            slot("SMG", "smg", Some("30 / 120")),
            slot("Rifle", "rifle", Some("30 / 90")),
            sniper,
            slot("Grenade", "grenade", Some("3")),
            slot("Shotgun", "shotgun", Some("6 / 24")),
            slot("Bat", "bat", None),
            slot("Fists", "fists", None),
        ],
        focus: Some(0),
        center: "Pistol~n~~y~12~s~ / 48".into(),
    });
    m.overlay.tint = Some([0.1, 0.2, 0.5, 0.18]);
    m.overlay.vignette = 0.5;
    m
}

fn phone_base() -> UiModel {
    let mut m = hud();
    if let Some(h) = &mut m.hud {
        h.crosshair = None;
        h.weapon = None;
        h.wanted = None;
        h.money = Some(Money { amount: 12_480, delta: 0, delta_age: 10.0 });
    }
    m.prompts = vec![prompt(PadButton::East, "Back")];
    m
}

/// The phone's home screen.
pub fn phone() -> UiModel {
    let tile = |id: &str, label: &str, icon: &str, badge: u32| PhoneTile {
        id: id.into(),
        label: label.into(),
        icon: icon.into(),
        badge,
    };
    let mut m = phone_base();
    m.phone = Some(Phone {
        app: "home".into(),
        title: "Tue 14 Oct".into(),
        tiles: vec![
            tile("contacts", "Contacts", "contacts", 0),
            tile("messages", "Messages", "messages", 2),
            tile("camera", "Camera", "camera", 0),
            tile("map", "Map", "map", 0),
            tile("jobs", "Jobs", "jobs", 1),
            tile("internet", "Internet", "internet", 0),
            tile("email", "Email", "email", 0),
            tile("radio", "Radio", "music", 0),
            tile("settings", "Settings", "settings", 0),
        ],
        rows: vec![],
        focus: 1,
        clock: "18:42".into(),
    });
    m
}

/// The phone's messages app: rows.
pub fn phone_messages() -> UiModel {
    let row = |t: &str, d: &str| PhoneRow { title: t.into(), detail: d.into() };
    let mut m = phone_base();
    m.phone = Some(Phone {
        app: "messages".into(),
        title: "Messages".into(),
        tiles: vec![],
        rows: vec![
            row("Dee", "Marina, pier 4. Come alone and bring the van."),
            row("Mack", "Who parked a tow truck on my car?"),
            row("Paloma Arms", "Your order is ready for pickup."),
            row("Unknown", "We need to talk about the overpass."),
            row("Lou's Garage", "Respray done. Same colour, new plates."),
            row("Mom", "Call me back. Love you."),
            row("Freeway FM", "You won two tickets to Night Drive!"),
        ],
        focus: 0,
        clock: "18:42".into(),
    });
    m
}

/// Banner, toasts, subtitle, prompts, help and world markers at once.
pub fn moments() -> UiModel {
    // Markers are placed relative to the demo player, like the camera.
    let marker = |x: f32, y: f32, z: f32, icon: BlipIcon, color: &str, label: Option<&str>| ScreenMarker {
        pos: Vec3::new(HERE.x + x, y, HERE.y + z),
        icon,
        color: hex(color),
        label: label.map(String::from),
        distance: true,
    };
    UiModel {
        hud: Some(Hud {
            minimap: Some(radar(0.35)),
            vitals: Some(vitals()),
            money: Some(Money { amount: 24_480, delta: 12_000, delta_age: 0.3 }),
            ..Default::default()
        }),
        banners: vec![Banner {
            kind: BannerKind::Passed,
            title: "Mission passed".into(),
            subtitle: Some("The Overpass Job · ~g~+$12,000~s~".into()),
            age: 1.0,
            duration: 5.0,
        }],
        toasts: vec![
            Toast { id: 1, icon: "heat".into(), title: "Heat".into(), body: "Police search area widened.".into(), age: 2.0 },
            Toast { id: 2, icon: "messages".into(), title: "Dee".into(), body: "Marina, pier 4. Come alone.".into(), age: 0.5 },
        ],
        subtitle: Some(Subtitle {
            speaker: Some("Dee".into()),
            text: "Told you the overpass was a shortcut.".into(),
            color: Some(hex("#3d8bff")),
        }),
        prompts: vec![
            Prompt { button: PadButton::North, label: "Steal car".into(), hold: Some(0.6) },
            prompt(PadButton::Right, "Talk"),
        ],
        help: Some(Help { text: "Press ~btn:l2~ to aim and ~btn:r2~ to shoot. Hold ~btn:r1~ to take cover.".into(), age: 3.0 }),
        markers: vec![
            marker(-14.0, 3.0, -40.0, BlipIcon::Objective, "#ffc145", Some("Drop-off")),
            marker(400.0, 0.0, 300.0, BlipIcon::Waypoint, "#ff3e8a", None),
            marker(26.0, 1.5, -60.0, BlipIcon::Enemy, "#ff4d4d", None),
        ],
        time: 42.0,
        ..Default::default()
    }
}

/// Wasted: the red banner over a vignette (the view greys the world itself).
pub fn wasted() -> UiModel {
    UiModel {
        banners: vec![Banner { kind: BannerKind::Wasted, title: "Wasted".into(), subtitle: None, age: 1.4, duration: 6.0 }],
        overlay: Overlay { fade: 0.0, letterbox: 0.0, desaturate: 1.0, tint: Some([0.5, 0.05, 0.05, 0.12]), vignette: 0.8 },
        perf: false,
        ..Default::default()
    }
}

/// Every glyph inline in text: check the art for each glyph style.
pub fn glyph_sheet() -> UiModel {
    UiModel {
        screen: Some(Screen {
            id: "controls".into(),
            title: "Controls".into(),
            tabs: vec![],
            tab: 0,
            hints: vec![prompt(PadButton::South, "Select"), prompt(PadButton::East, "Back"), prompt(PadButton::Start, "Pause")],
            body: ScreenBody::Text(TextCard {
                heading: "Every button".into(),
                lines: vec![
                    "Face  ~btn:south~ ~btn:east~ ~btn:west~ ~btn:north~   D-pad  ~btn:up~ ~btn:down~ ~btn:left~ ~btn:right~"
                        .into(),
                    "Bumpers and triggers  ~btn:l1~ ~btn:r1~ ~btn:l2~ ~btn:r2~".into(),
                    "Sticks  ~btn:left_stick~ ~btn:right_stick~ ~btn:l3~ ~btn:r3~".into(),
                    "System  ~btn:start~ ~btn:select~ ~btn:touchpad~".into(),
                ],
            }),
            backdrop: Backdrop::Dim,
        }),
        perf: false,
        ..Default::default()
    }
}

/// A procedural map picture of a coastal city: sea to the west and south, a beach, a street
/// grid with avenues, a diagonal boulevard, a freeway loop and parks. 1024 px at 2 m per px,
/// centred on the world origin.
pub fn map() -> MapImage {
    let (w, h) = (1024u32, 1024u32);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    let hash = |x: u32, y: u32| {
        let mut v = x.wrapping_mul(374_761_393) ^ y.wrapping_mul(668_265_263);
        v = (v ^ (v >> 13)).wrapping_mul(1_274_126_177);
        v ^ (v >> 16)
    };
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f32, y as f32);
            let coast = 170.0 + 38.0 * (fy / 85.0).sin() + 18.0 * (fy / 31.0).sin();
            let bay = 880.0 + 26.0 * (fx / 47.0).sin();
            let sea_d = (fx - coast).min(bay - fy);
            let mut c: [u8; 3] = if sea_d < 0.0 {
                // Deeper water further out.
                let k = (-sea_d / 120.0).min(1.0);
                [lerp(28, 14, k), lerp(64, 38, k), lerp(92, 62, k)]
            } else if sea_d < 12.0 {
                [118, 104, 76]
            } else {
                let (bx, by) = (x / 48, y / 48);
                let downtown = ((fx - 600.0).powi(2) + (fy - 420.0).powi(2)).sqrt() < 230.0;
                let v = (hash(bx, by) % 12) as u8;
                if downtown { [44 + v, 49 + v, 58 + v] } else { [34 + v, 38 + v, 46 + v] }
            };
            if sea_d >= 12.0 {
                // Parks.
                let park = |x0: f32, y0: f32, x1: f32, y1: f32| fx > x0 && fx < x1 && fy > y0 && fy < y1;
                if park(300.0, 560.0, 440.0, 700.0) || park(700.0, 150.0, 790.0, 260.0) || park(820.0, 640.0, 960.0, 740.0) {
                    c = [40, 74, 52];
                }
                // Streets every 48 px, avenues every fourth.
                let (gx, gy) = (x % 48, y % 48);
                let (ax, ay) = ((x / 48) % 4 == 0, (y / 48) % 4 == 0);
                let street = gx < 3 || gy < 3;
                let avenue = (ax && gx < 6) || (ay && gy < 6);
                if avenue {
                    c = [84, 91, 104];
                } else if street {
                    c = [62, 68, 80];
                }
                // A diagonal boulevard.
                let d = ((fx - 250.0) + (fy - 900.0)).abs() * std::f32::consts::FRAC_1_SQRT_2;
                if d < 5.0 && fy < 900.0 {
                    c = [92, 99, 112];
                }
            }
            // The freeway loop over everything on land and water.
            let (ex, ey) = ((fx - 600.0) / 300.0, (fy - 470.0) / 250.0);
            let r = (ex * ex + ey * ey).sqrt();
            let band = (r - 1.0).abs() * 260.0;
            if band < 7.0 {
                c = if band < 5.0 { [205, 146, 70] } else { [40, 30, 22] };
            }
            let i = ((y * w + x) * 4) as usize;
            rgba[i..i + 3].copy_from_slice(&c);
            rgba[i + 3] = 255;
        }
    }
    MapImage { width: w, height: h, rgba, origin: Vec2::new(-1024.0, -1024.0), metres_per_px: 2.0, version: 1 }
}

fn lerp(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).round() as u8
}
