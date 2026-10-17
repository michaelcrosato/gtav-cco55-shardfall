//! Design tokens (PLAN.md §5.9): the colours, the type scale and the fonts. Sizes are pixels at
//! the 1080p reference; [`crate::paint::Cx::px`] scales them to the screen.

use std::sync::Arc;

use egui::{Color32, FontData, FontDefinitions, FontFamily};

/// The display face (logo, banners, titles, big numbers): Anton, a heavy condensed grotesque.
/// Chosen over Bebas Neue (too light at a distance) and Passion One (rounded, reads as cartoon)
/// by a capture comparison; see `examples/gallery.rs fonts`.
pub(crate) const DISPLAY_TTF: &[u8] = include_bytes!("../../../assets/fonts/anton/Anton-Regular.ttf");
/// The UI face: Barlow Semi Condensed, SemiBold for body text and Bold for labels.
pub(crate) const UI_TTF: &[u8] = include_bytes!("../../../assets/fonts/barlowsemicondensed/BarlowSemiCondensed-SemiBold.ttf");
pub(crate) const UI_BOLD_TTF: &[u8] = include_bytes!("../../../assets/fonts/barlowsemicondensed/BarlowSemiCondensed-Bold.ttf");

/// egui family names the kit draws with ([`add_fonts`] defines them).
pub const FAMILY_DISPLAY: &str = "pav_display";
pub const FAMILY_UI: &str = "pav_ui";
pub const FAMILY_UI_BOLD: &str = "pav_ui_bold";

/// Adds the kit's fonts as the named families [`FAMILY_DISPLAY`], [`FAMILY_UI`] and
/// [`FAMILY_UI_BOLD`]. The host's `Proportional` and `Monospace` families are left alone; each
/// new family falls back to the host's proportional fonts for glyphs it lacks.
///
/// ```ignore
/// let mut fonts = egui::FontDefinitions::default();
/// pav_ui::add_fonts(&mut fonts);
/// ctx.set_fonts(fonts);
/// ```
pub fn add_fonts(defs: &mut FontDefinitions) {
    let fallbacks = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    for (key, bytes, family) in [
        ("pav-anton", DISPLAY_TTF, FAMILY_DISPLAY),
        ("pav-barlow-sc-semibold", UI_TTF, FAMILY_UI),
        ("pav-barlow-sc-bold", UI_BOLD_TTF, FAMILY_UI_BOLD),
    ] {
        defs.font_data.insert(key.into(), Arc::new(FontData::from_static(bytes)));
        let mut list = vec![key.to_string()];
        list.extend(fallbacks.iter().filter(|f| f.as_str() != key).cloned());
        defs.families.insert(FontFamily::Name(family.into()), list);
    }
}

/// Installs the kit's fonts into `ctx`, keeping the fonts it has (takes effect next pass).
/// Only inside a pass: egui has no fonts to read before its first one.
pub(crate) fn install_fonts(ctx: &egui::Context) {
    let mut defs = ctx.fonts(|f| f.definitions().clone());
    add_fonts(&mut defs);
    ctx.set_fonts(defs);
}

/// Whether `ctx` already has the kit's families.
pub(crate) fn fonts_ready(ctx: &egui::Context) -> bool {
    ctx.fonts(|f| {
        let families = f.families();
        [FAMILY_DISPLAY, FAMILY_UI, FAMILY_UI_BOLD].iter().all(|n| families.contains(&FontFamily::Name((*n).into())))
    })
}

/// The colour tokens.
pub mod color {
    use egui::Color32;

    /// Dark slate behind text over the world (#0e1116 at 72%).
    pub const BACKPLATE: Color32 = Color32::from_rgba_unmultiplied_const(0x0e, 0x11, 0x16, 184);
    /// The same slate, opaque (solid backdrops, text on accent fills).
    pub const SLATE: Color32 = Color32::from_rgb(0x0e, 0x11, 0x16);
    /// A lighter slate for tracks, empty bars and rims.
    pub const SLATE_2: Color32 = Color32::from_rgb(0x2a, 0x30, 0x3a);
    /// The sunset accent gradient: orange to pink.
    pub const ACCENT: Color32 = Color32::from_rgb(0xff, 0x7a, 0x3d);
    pub const ACCENT_2: Color32 = Color32::from_rgb(0xff, 0x3e, 0x8a);
    pub const TEXT: Color32 = Color32::from_rgb(0xf4, 0xf1, 0xea);
    /// Secondary text (details, reserve ammo, units).
    pub const TEXT_DIM: Color32 = Color32::from_rgb(0xb4, 0xb0, 0xa8);
    pub const POSITIVE: Color32 = Color32::from_rgb(0x6f, 0xe2, 0x8a);
    pub const WARNING: Color32 = Color32::from_rgb(0xff, 0xc1, 0x45);
    pub const DANGER: Color32 = Color32::from_rgb(0xff, 0x4d, 0x4d);
    pub const POLICE_BLUE: Color32 = Color32::from_rgb(0x3d, 0x8b, 0xff);
    pub const POLICE_RED: Color32 = Color32::from_rgb(0xff, 0x3d, 0x3d);
    /// Text shadows and outlines over the world.
    pub const SHADOW: Color32 = Color32::from_rgba_unmultiplied_const(0, 0, 0, 200);
    /// Medal metals.
    pub const GOLD: Color32 = Color32::from_rgb(0xf5, 0xc4, 0x3b);
    pub const SILVER: Color32 = Color32::from_rgb(0xc9, 0xd1, 0xdb);
    pub const BRONZE: Color32 = Color32::from_rgb(0xcd, 0x82, 0x4a);
}

/// The type scale (1080p pixels). The 10-foot rules: body text >= 28, HUD numbers >= 40,
/// titles 64-120.
pub mod size {
    pub const LOGO: f32 = 156.0;
    pub const TITLE: f32 = 88.0;
    pub const BANNER: f32 = 120.0;
    pub const HEADING: f32 = 52.0;
    pub const HUD_NUMBER: f32 = 56.0;
    pub const HUD_SMALL_NUMBER: f32 = 44.0;
    pub const ITEM: f32 = 36.0;
    pub const LABEL: f32 = 32.0;
    pub const BODY: f32 = 28.0;
}

/// A linear-RGB engine colour as an egui colour.
pub fn linear(c: pav_core::Color) -> Color32 {
    let [r, g, b] = c.0;
    egui::Rgba::from_rgb(r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0)).into()
}
