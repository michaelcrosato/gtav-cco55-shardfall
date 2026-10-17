//! Light text markup (`pav_core::present::UiModel`): `~y~ ~b~ ~r~ ~g~ ~o~ ~p~` colour the words
//! that follow until `~s~`, `~n~` (or a newline) breaks the line, and `~btn:NAME~` draws a
//! controller glyph inline (NAME = a `PadButton` in snake case, or an alias such as `cross`,
//! `a`, `lb`, `options`). Unknown tokens stay as typed, so mistakes show.

use std::sync::Arc;

use egui::{Align, Color32, FontId, Galley, Pos2, Vec2, pos2, vec2};
use pav_core::present::PadButton;

use crate::paint::{Cx, Fx};
use crate::theme::color;

/// A colour from the markup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tint {
    /// The text's own colour.
    Base,
    Yellow,
    Blue,
    Red,
    Green,
    Orange,
    Pink,
}

impl Tint {
    pub fn color(self, base: Color32) -> Color32 {
        match self {
            Tint::Base => base,
            Tint::Yellow => color::WARNING,
            // A lighter police blue: the token colour is too dark for small text.
            Tint::Blue => Color32::from_rgb(0x5c, 0xa0, 0xff),
            Tint::Red => color::DANGER,
            Tint::Green => color::POSITIVE,
            Tint::Orange => color::ACCENT,
            Tint::Pink => color::ACCENT_2,
        }
    }
}

/// One piece of marked-up text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Span<'a> {
    Text(&'a str, Tint),
    Glyph(PadButton),
    Break,
}

/// Splits marked-up text into spans.
pub fn parse(text: &str) -> Vec<Span<'_>> {
    let mut out = Vec::new();
    let mut tint = Tint::Base;
    let mut start = 0; // start of pending literal text
    let mut i = 0;
    let bytes = text.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'~' {
            if let Some(len) = text[i + 1..].find('~') {
                let token = &text[i + 1..i + 1 + len];
                let parsed = match token {
                    "y" => Some(Tok::Tint(Tint::Yellow)),
                    "b" => Some(Tok::Tint(Tint::Blue)),
                    "r" => Some(Tok::Tint(Tint::Red)),
                    "g" => Some(Tok::Tint(Tint::Green)),
                    "o" => Some(Tok::Tint(Tint::Orange)),
                    "p" => Some(Tok::Tint(Tint::Pink)),
                    "s" => Some(Tok::Tint(Tint::Base)),
                    "n" => Some(Tok::Break),
                    t => t.strip_prefix("btn:").and_then(button_from_name).map(Tok::Glyph),
                };
                if let Some(tok) = parsed {
                    if start < i {
                        push_text(&mut out, &text[start..i], tint);
                    }
                    match tok {
                        Tok::Tint(t) => tint = t,
                        Tok::Break => out.push(Span::Break),
                        Tok::Glyph(b) => out.push(Span::Glyph(b)),
                    }
                    i += len + 2;
                    start = i;
                    continue;
                }
            }
        }
        // Step over one UTF-8 character.
        i += text[i..].chars().next().map_or(1, char::len_utf8);
    }
    if start < text.len() {
        push_text(&mut out, &text[start..], tint);
    }
    out
}

/// Pushes literal text, turning newlines into breaks.
fn push_text<'a>(out: &mut Vec<Span<'a>>, s: &'a str, tint: Tint) {
    for (i, part) in s.split('\n').enumerate() {
        if i > 0 {
            out.push(Span::Break);
        }
        if !part.is_empty() {
            out.push(Span::Text(part, tint));
        }
    }
}

enum Tok {
    Tint(Tint),
    Break,
    Glyph(PadButton),
}

/// The text without markup (glyphs as their names in brackets): for measuring and logs.
pub fn plain(text: &str) -> String {
    let mut s = String::new();
    for span in parse(text) {
        match span {
            Span::Text(t, _) => s.push_str(t),
            Span::Glyph(b) => {
                s.push('[');
                s.push_str(button_name(b));
                s.push(']');
            }
            Span::Break => s.push('\n'),
        }
    }
    s
}

/// A `PadButton` from its snake-case name or a controller alias (`cross`, `a`, `lb`, `options`,
/// `ls` for L3...).
pub fn button_from_name(name: &str) -> Option<PadButton> {
    use pav_core::input::pad;
    let n = name.trim().to_ascii_lowercase();
    match n.as_str() {
        "left_stick" | "lstick" => return Some(PadButton::LeftStick),
        "right_stick" | "rstick" => return Some(PadButton::RightStick),
        "ls" => return Some(PadButton::L3),
        "rs" => return Some(PadButton::R3),
        _ => {}
    }
    let bit = pad::from_name(&n)?;
    Some(match bit {
        pad::SOUTH => PadButton::South,
        pad::EAST => PadButton::East,
        pad::WEST => PadButton::West,
        pad::NORTH => PadButton::North,
        pad::L1 => PadButton::L1,
        pad::R1 => PadButton::R1,
        pad::L2 => PadButton::L2,
        pad::R2 => PadButton::R2,
        pad::L3 => PadButton::L3,
        pad::R3 => PadButton::R3,
        pad::UP => PadButton::Up,
        pad::DOWN => PadButton::Down,
        pad::LEFT => PadButton::Left,
        pad::RIGHT => PadButton::Right,
        pad::START => PadButton::Start,
        pad::SELECT => PadButton::Select,
        pad::TOUCHPAD => PadButton::Touchpad,
        _ => return None,
    })
}

/// The snake-case name of a button (as the markup and serde spell it).
pub fn button_name(b: PadButton) -> &'static str {
    match b {
        PadButton::South => "south",
        PadButton::East => "east",
        PadButton::West => "west",
        PadButton::North => "north",
        PadButton::L1 => "l1",
        PadButton::R1 => "r1",
        PadButton::L2 => "l2",
        PadButton::R2 => "r2",
        PadButton::L3 => "l3",
        PadButton::R3 => "r3",
        PadButton::Up => "up",
        PadButton::Down => "down",
        PadButton::Left => "left",
        PadButton::Right => "right",
        PadButton::Start => "start",
        PadButton::Select => "select",
        PadButton::Touchpad => "touchpad",
        PadButton::LeftStick => "left_stick",
        PadButton::RightStick => "right_stick",
    }
}

// ---------------------------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------------------------

/// Marked-up text laid out on lines: runs of same-coloured text and glyphs.
pub(crate) struct Rich {
    pub lines: Vec<RichLine>,
    /// Widest line, and the height from the first row's top to the last row's bottom.
    pub size: Vec2,
    pub line_h: f32,
    /// Cap top and baseline below a row's top.
    pub cap: (f32, f32),
    pub font: FontId,
}

pub(crate) struct RichLine {
    pub items: Vec<RichItem>,
    pub width: f32,
}

pub(crate) enum RichItem {
    Text { galley: Arc<Galley>, x: f32, color: Color32 },
    Glyph { button: PadButton, x: f32, w: f32 },
}

/// One unbreakable piece while breaking lines.
enum Atom<'a> {
    Word { text: &'a str, color: Color32, w: f32 },
    Glyph { button: PadButton, w: f32 },
    Break,
}

impl Cx<'_> {
    /// Lays out marked-up text in `font`, wrapping at `wrap` (points) when given.
    pub fn rich(&self, text: &str, font: FontId, base: Color32, wrap: Option<f32>) -> Rich {
        self.rich_prefixed(None, text, font, base, wrap)
    }

    /// Like [`Cx::rich`], with a lead-in in its own colour (a subtitle's speaker).
    pub fn rich_prefixed(
        &self,
        prefix: Option<(&str, Color32)>,
        text: &str,
        font: FontId,
        base: Color32,
        wrap: Option<f32>,
    ) -> Rich {
        let glyph_h = self.glyph_height(&font);
        let gap = font.size * 0.18;
        let mut atoms = Vec::new();
        let lead = prefix.map(|(t, c)| (Span::Text(t, Tint::Base), Some(c)));
        for (span, own) in lead.into_iter().chain(parse(text).into_iter().map(|s| (s, None))) {
            match span {
                Span::Text(t, tint) => {
                    let color = own.unwrap_or_else(|| tint.color(base));
                    // A word is its letters and the spaces after them, so widths add up.
                    let mut rest = t;
                    while !rest.is_empty() {
                        let word_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
                        let end = rest[word_end..].find(|c: char| !c.is_whitespace()).map_or(rest.len(), |i| word_end + i);
                        let word = &rest[..end];
                        atoms.push(Atom::Word { text: word, color, w: self.text_width(word, &font) });
                        rest = &rest[end..];
                    }
                }
                Span::Glyph(b) => atoms.push(Atom::Glyph { button: b, w: self.glyph_width(b, glyph_h) + gap * 2.0 }),
                Span::Break => atoms.push(Atom::Break),
            }
        }
        // Greedy line breaking.
        let mut lines: Vec<Vec<&Atom>> = vec![Vec::new()];
        let mut x = 0.0;
        for a in &atoms {
            let (w, trimmed) = match a {
                Atom::Word { text, w, .. } => (*w, *w - self.text_width(text.trim_end(), &font).min(*w)),
                Atom::Glyph { w, .. } => (*w, 0.0),
                Atom::Break => {
                    lines.push(Vec::new());
                    x = 0.0;
                    continue;
                }
            };
            if let Some(max) = wrap {
                if x + w - trimmed > max && !lines.last().is_none_or(|l| l.is_empty()) {
                    lines.push(Vec::new());
                    x = 0.0;
                }
            }
            lines.last_mut().expect("a line").push(a);
            x += w;
        }
        // Merge same-coloured words of a line into one galley each.
        let mut out = Vec::new();
        let mut widest: f32 = 0.0;
        for line in lines {
            let mut items = Vec::new();
            let mut x = 0.0;
            let mut run = String::new();
            let mut run_color = base;
            let flush = |run: &mut String, color: Color32, x: &mut f32, items: &mut Vec<RichItem>| {
                if !run.is_empty() {
                    let galley = self.galley(std::mem::take(run), font.clone(), color);
                    let w = galley.size().x;
                    items.push(RichItem::Text { galley, x: *x, color });
                    *x += w;
                }
            };
            let n = line.len();
            for (i, a) in line.into_iter().enumerate() {
                match a {
                    Atom::Word { text, color, .. } => {
                        if *color != run_color {
                            flush(&mut run, run_color, &mut x, &mut items);
                            run_color = *color;
                        }
                        // The line's last word loses its trailing spaces.
                        run.push_str(if i + 1 == n { text.trim_end() } else { text });
                    }
                    Atom::Glyph { button, w } => {
                        flush(&mut run, run_color, &mut x, &mut items);
                        items.push(RichItem::Glyph { button: *button, x: x + gap, w: *w - gap * 2.0 });
                        x += w;
                    }
                    Atom::Break => {}
                }
            }
            flush(&mut run, run_color, &mut x, &mut items);
            widest = widest.max(x);
            out.push(RichLine { items, width: x });
        }
        let line_h = font.size * 1.22;
        let row_h = self.galley("H", font.clone(), base).size().y;
        let height = (out.len().max(1) - 1) as f32 * line_h + row_h;
        Rich { lines: out, size: vec2(widest, height), line_h, cap: self.cap(&font), font }
    }

    /// Paints laid-out text with its rows starting at `top_left.y`; each line is aligned
    /// within `width` from `top_left.x`.
    pub fn paint_rich(&self, rich: &Rich, top_left: Pos2, width: f32, align: Align, fx: Fx) {
        let glyph_h = self.glyph_height(&rich.font);
        for (i, line) in rich.lines.iter().enumerate() {
            let x0 = match align {
                Align::Min => top_left.x,
                Align::Center => top_left.x + (width - line.width) * 0.5,
                Align::Max => top_left.x + width - line.width,
            };
            let y = top_left.y + i as f32 * rich.line_h;
            for item in &line.items {
                match item {
                    RichItem::Text { galley, x, color } => self.paint_galley(pos2(x0 + x, y), galley, *color, fx),
                    RichItem::Glyph { button, x, w } => {
                        let center = pos2(x0 + x + w * 0.5, y + (rich.cap.0 + rich.cap.1) * 0.5);
                        self.paint_glyph(*button, center, glyph_h);
                    }
                }
            }
        }
    }

    /// Glyph height for text in `font`: a little taller than the capitals.
    pub fn glyph_height(&self, font: &FontId) -> f32 {
        let (t, b) = self.cap(font);
        (b - t) * 1.7
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_is_one_span() {
        assert_eq!(parse("Lose the cops."), vec![Span::Text("Lose the cops.", Tint::Base)]);
        assert_eq!(parse(""), vec![]);
    }

    #[test]
    fn colours_run_until_reset() {
        assert_eq!(
            parse("Lose the ~b~cops~s~ now"),
            vec![Span::Text("Lose the ", Tint::Base), Span::Text("cops", Tint::Blue), Span::Text(" now", Tint::Base)]
        );
        assert_eq!(
            parse("~y~Gold ~r~red ~g~green ~o~orange ~p~pink"),
            vec![
                Span::Text("Gold ", Tint::Yellow),
                Span::Text("red ", Tint::Red),
                Span::Text("green ", Tint::Green),
                Span::Text("orange ", Tint::Orange),
                Span::Text("pink", Tint::Pink),
            ]
        );
    }

    #[test]
    fn glyph_tokens_take_names_and_aliases() {
        assert_eq!(
            parse("Press ~btn:south~ to jump"),
            vec![Span::Text("Press ", Tint::Base), Span::Glyph(PadButton::South), Span::Text(" to jump", Tint::Base)]
        );
        assert_eq!(parse("~btn:r2~"), vec![Span::Glyph(PadButton::R2)]);
        assert_eq!(parse("~btn:cross~~btn:y~"), vec![Span::Glyph(PadButton::South), Span::Glyph(PadButton::North)]);
        assert_eq!(parse("~btn:left_stick~"), vec![Span::Glyph(PadButton::LeftStick)]);
        assert_eq!(parse("~btn:options~"), vec![Span::Glyph(PadButton::Start)]);
        assert_eq!(parse("~btn:ls~"), vec![Span::Glyph(PadButton::L3)]);
    }

    #[test]
    fn breaks_from_tokens_and_newlines() {
        assert_eq!(
            parse("one~n~two\nthree"),
            vec![
                Span::Text("one", Tint::Base),
                Span::Break,
                Span::Text("two", Tint::Base),
                Span::Break,
                Span::Text("three", Tint::Base)
            ]
        );
    }

    #[test]
    fn unknown_or_unclosed_tokens_stay_literal() {
        assert_eq!(parse("~q~odd"), vec![Span::Text("~q~odd", Tint::Base)]);
        assert_eq!(parse("~btn:moon~"), vec![Span::Text("~btn:moon~", Tint::Base)]);
        assert_eq!(parse("50~ off"), vec![Span::Text("50~ off", Tint::Base)]);
        // A stray tilde before a real token: the literal part keeps it, the token still works.
        assert_eq!(parse("~ ~y~a"), vec![Span::Text("~ ", Tint::Base), Span::Text("a", Tint::Yellow)]);
    }

    #[test]
    fn multibyte_text_survives() {
        assert_eq!(
            parse("Café ~g~€5~s~ — ok"),
            vec![Span::Text("Café ", Tint::Base), Span::Text("€5", Tint::Green), Span::Text(" — ok", Tint::Base)]
        );
    }

    #[test]
    fn plain_strips_markup() {
        assert_eq!(plain("Hold ~btn:north~ to ~r~bail~s~."), "Hold [north] to bail.");
        assert_eq!(button_from_name("RIGHT_STICK"), Some(PadButton::RightStick));
        assert_eq!(button_from_name("lt"), Some(PadButton::L2));
        assert_eq!(button_from_name("nope"), None);
        for b in [PadButton::South, PadButton::Touchpad, PadButton::LeftStick, PadButton::R3] {
            assert_eq!(button_from_name(button_name(b)), Some(b));
        }
    }
}
