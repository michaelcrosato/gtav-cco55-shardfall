//! Grand Theft Freeway-BX's city pack (`LIB/gtf_city`, a shipped pack) and its base materials
//! (`MAT/gtf/*`): every name the game may place resolves from the build alone, as in the browser
//! and a lone exe, the parts the game drives are there, and every material a city prop wears
//! resolves. Its own process, since it points the props root at an empty folder.

use std::collections::BTreeSet;
use std::path::Path;

use pav_core::material::Library;
use pav_core::material::library::MATERIALS;
use pav_core::props;

/// The city props, `LIB/gtf_city/<name>`.
const PROPS: &[&str] = &[
    // street lighting and signals
    "street_lamp",
    "street_lamp_double",
    "traffic_signal",
    "ped_signal",
    "stop_sign",
    "speed_sign",
    "street_name_sign",
    // curb furniture
    "fire_hydrant",
    "trash_bin",
    "dumpster",
    "bench",
    "bus_stop",
    "newspaper_box",
    "parking_meter",
    "mailbox",
    "planter",
    "bollard",
    // utilities and rooftops
    "power_pole",
    "ac_unit",
    "water_tank",
    "antenna_mast",
    "billboard",
    "rooftop_sign_frame",
    // vegetation
    "palm_tall",
    "palm_short",
    "street_tree",
    "shrub",
    "hedge",
    // construction and roadside
    "jersey_barrier",
    "traffic_cone",
    "barricade",
    "chain_link_fence",
    "wood_fence",
    "low_wall",
    "guard_rail",
    // docks and industrial
    "shipping_container",
    "gantry_crane",
    "pallet_stack",
    "oil_drum",
    "forklift",
    // beach
    "lifeguard_tower",
    "beach_umbrella",
    "volleyball_net",
    "trash_can_beach",
    // landmarks: shops and their interiors
    "shop_counter",
    "store_shelf",
    "drinks_cooler",
    "gas_pump",
    "gun_rack",
    "car_lift",
    "tool_chest",
    "tyre_stack",
    "espresso_machine",
    "cafe_table_set",
    "diner_booth",
    "bar_stool",
    "office_desk",
    "office_chair",
    // landmarks: homes
    "bed",
    "sofa",
    "tv_stand",
    "kitchenette",
    "picnic_table",
    "bbq_grill",
    // landmarks: yards and civic
    "pallet_rack",
    "pipe_stack",
    "fuel_tank",
    "floodlight_mast",
    "boom_barrier",
    "flagpole",
    // landmarks: the pier
    "food_stall",
    "arcade_cabinet",
    "big_wheel_rim",
    "big_wheel_car",
];

/// Parts the game drives by name (night lamps, signal states, text and ad panels).
const DRIVEN: &[(&str, &[&str])] = &[
    ("street_lamp", &["lamp"]),
    ("street_lamp_double", &["lamp", "lamp~z"]),
    ("traffic_signal", &["red", "amber", "green"]),
    ("ped_signal", &["walk", "dont_walk"]),
    ("billboard", &["panel", "lamp[0]", "lamp[3]"]),
    ("bus_stop", &["ad"]),
    ("barricade", &["panel", "lamp"]),
    ("speed_sign", &["face"]),
    ("stop_sign", &["face"]),
    ("street_name_sign", &["blade_a", "blade_b"]),
];

/// The base materials, `MAT/gtf/<name>` (`assets/materials/gtf/<name>.json`).
const MATERIAL_NAMES: &[&str] = &[
    "asphalt",
    "asphalt_worn",
    "concrete",
    "sidewalk",
    "stucco",
    "brick",
    "corrugated_metal",
    "terracotta_tile",
    "sand",
    "lawn",
    "dry_grass",
    "dirt",
    "car_paint",
    "chrome",
    "rubber",
    "tinted_glass",
    "painted_metal",
    "wood_boardwalk",
    "chain_link",
];

fn repo() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

/// The library a build resolves `MAT/` names against: the embedded materials, plus the files of
/// `assets/materials/gtf/` that the build does not embed yet (native tools read them from disk).
fn city_library() -> Library {
    let mut sources: Vec<(String, String)> = MATERIALS.iter().map(|(n, t)| (n.to_string(), t.to_string())).collect();
    let folder = repo().join("assets/materials/gtf");
    let mut files: Vec<_> = std::fs::read_dir(&folder)
        .unwrap_or_else(|e| panic!("{}: {e}", folder.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    for path in files {
        let name = format!("gtf/{}", path.file_stem().unwrap().to_string_lossy());
        if !sources.iter().any(|(n, _)| *n == name) {
            sources.push((name, std::fs::read_to_string(&path).unwrap()));
        }
    }
    let borrowed: Vec<(&str, &str)> = sources.iter().map(|(n, t)| (n.as_str(), t.as_str())).collect();
    Library::from_sources(&borrowed)
}

/// Every `MAT/…` name a document mentions: `mat`, its materials table and inline `base`s.
fn material_names(text: &str) -> BTreeSet<String> {
    text.match_indices("\"MAT/")
        .map(|(at, _)| {
            let rest = &text[at + 1..];
            rest[..rest.find('"').unwrap_or(rest.len())].to_owned()
        })
        .collect()
}

#[test]
fn the_city_pack_and_its_materials_ship() {
    let empty = std::env::temp_dir().join(format!("gtf_city_no_assets_{}", std::process::id()));
    std::fs::create_dir_all(&empty).unwrap();
    // SAFETY: set before any other code in this test binary reads the environment.
    unsafe { std::env::set_var("PAV_ASSETS", &empty) };

    let library = city_library();
    let mut missing = Vec::new();
    for leaf in MATERIAL_NAMES {
        let name = format!("MAT/gtf/{leaf}");
        if library.get(&name).is_none() {
            let why = library.issues().get(&name).map(|i| format!("{i:?}")).unwrap_or_else(|| "no file".into());
            missing.push(format!("{name}: {why}"));
        }
    }
    assert!(missing.is_empty(), "base materials that do not resolve: {missing:#?}");
    // Once the build embeds assets/materials/gtf (every build, the browser's too), each must be in it.
    if MATERIALS.iter().any(|(name, _)| name.starts_with("gtf/")) {
        let core = Library::core();
        let unembedded: Vec<&str> =
            MATERIAL_NAMES.iter().copied().filter(|leaf| core.get(&format!("MAT/gtf/{leaf}")).is_none()).collect();
        assert!(unembedded.is_empty(), "MAT/gtf materials the build does not embed: {unembedded:?}");
    }

    let rows: BTreeSet<&str> = props::index::shipped().iter().map(|row| row.name.as_str()).collect();
    for leaf in PROPS {
        let name = format!("LIB/gtf_city/{leaf}");
        assert!(props::store::shipped(&name), "{name} ships");
        assert!(!props::store::embedded(&name), "{name} keeps its own tier");
        let text = props::store::shipped_file(&name, &format!("{leaf}.json")).unwrap_or_else(|| {
            panic!("{name} is not in the build: rebuild after publishing it (assets/props/lib/gtf_city/{leaf}.json)")
        });
        assert!(rows.contains(name.as_str()), "{name} has no row in the embedded lib/gtf_city/index.tsv");
        let def = props::get(&name).unwrap_or_else(|| panic!("{name} does not load from the build"));
        assert!(!def.compiled.parts.is_empty(), "{name} draws nothing");
        for mat in material_names(text) {
            assert!(library.get(&mat).is_some(), "{name} wears {mat}, which does not resolve");
        }
    }
    for (leaf, parts) in DRIVEN {
        let def = props::get(&format!("LIB/gtf_city/{leaf}")).unwrap();
        for part in *parts {
            assert!(def.compiled.parts.iter().any(|p| p.id == *part), "LIB/gtf_city/{leaf} has no part {part}");
        }
    }
    assert!(!props::store::shipped("LIB/kenney_mini_dungeon/barrel"), "other packs stay on disk");
    std::fs::remove_dir_all(&empty).ok();
}
