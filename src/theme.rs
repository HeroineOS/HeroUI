//! Theme: colors, radius, spacing and font, shared by every HeroUI app.
//!
//! Loaded from `$XDG_CONFIG_HOME/heroui/theme.conf` (falling back to
//! `~/.config/heroui/theme.conf`), so one settings app can restyle every
//! HeroUI program. The file is plain `key = value` lines; colors are
//! `#rrggbb`. Unknown keys and bad values are ignored.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI32, Ordering};
use std::{fs, io};

use fltk::app;
use fltk::enums::{Color, Font, FrameType};

#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    /// Window background.
    pub background: Color,
    /// Cards, buttons.
    pub surface: Color,
    /// Hovered surfaces, text input fields, slider tracks.
    pub surface_alt: Color,
    pub text: Color,
    /// Secondary text (hints, captions).
    pub text_dim: Color,
    /// Primary buttons, selection, active toggles and sliders.
    pub accent: Color,
    /// Text drawn on top of `accent`.
    pub accent_text: Color,
    pub border: Color,
    /// Corner radius in pixels.
    pub radius: i32,
    /// Gap between children of rows/columns.
    pub spacing: i32,
    /// Inner margin of cards and the window.
    pub padding: i32,
    pub font_size: i32,
    /// Font family name, e.g. "Inter" or "sans". Empty = FLTK default.
    pub font: String,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            background: Color::from_hex(0x14141c),
            surface: Color::from_hex(0x1f1f2b),
            surface_alt: Color::from_hex(0x2a2a3a),
            text: Color::from_hex(0xe6e6f0),
            text_dim: Color::from_hex(0x9090a8),
            accent: Color::from_hex(0xb46cff),
            accent_text: Color::from_hex(0x14141c),
            border: Color::from_hex(0x33334a),
            radius: 10,
            spacing: 8,
            padding: 12,
            font_size: 14,
            font: String::new(),
        }
    }

    pub fn light() -> Self {
        Self {
            background: Color::from_hex(0xf4f4f8),
            surface: Color::from_hex(0xffffff),
            surface_alt: Color::from_hex(0xe8e8f0),
            text: Color::from_hex(0x1a1a24),
            text_dim: Color::from_hex(0x6a6a80),
            accent: Color::from_hex(0x8a3ffc),
            accent_text: Color::from_hex(0xffffff),
            border: Color::from_hex(0xd4d4e0),
            ..Self::dark()
        }
    }

    pub fn path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("heroui").join("theme.conf"))
    }

    /// The user's theme file layered over the dark default.
    pub fn load() -> Self {
        let mut theme = Self::default();
        if let Some(text) = Self::path().and_then(|p| fs::read_to_string(p).ok()) {
            theme.apply_conf(&text);
        }
        theme
    }

    /// Applies `key = value` lines on top of this theme.
    pub fn apply_conf(&mut self, text: &str) {
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else { continue };
            let (key, value) = (key.trim(), value.trim());
            let color = || parse_color(value);
            let int = || value.parse::<i32>().ok();
            match key {
                "background" => set(&mut self.background, color()),
                "surface" => set(&mut self.surface, color()),
                "surface_alt" => set(&mut self.surface_alt, color()),
                "text" => set(&mut self.text, color()),
                "text_dim" => set(&mut self.text_dim, color()),
                "accent" => set(&mut self.accent, color()),
                "accent_text" => set(&mut self.accent_text, color()),
                "border" => set(&mut self.border, color()),
                "radius" => set(&mut self.radius, int()),
                "spacing" => set(&mut self.spacing, int()),
                "padding" => set(&mut self.padding, int()),
                "font_size" => set(&mut self.font_size, int()),
                "font" => self.font = value.to_string(),
                _ => {}
            }
        }
    }

    pub fn to_conf(&self) -> String {
        let mut s = String::from("# HeroUI theme\n");
        for (k, c) in [
            ("background", self.background),
            ("surface", self.surface),
            ("surface_alt", self.surface_alt),
            ("text", self.text),
            ("text_dim", self.text_dim),
            ("accent", self.accent),
            ("accent_text", self.accent_text),
            ("border", self.border),
        ] {
            let (r, g, b) = c.to_rgb();
            let _ = writeln!(s, "{k} = #{r:02x}{g:02x}{b:02x}");
        }
        for (k, v) in [
            ("radius", self.radius),
            ("spacing", self.spacing),
            ("padding", self.padding),
            ("font_size", self.font_size),
        ] {
            let _ = writeln!(s, "{k} = {v}");
        }
        let _ = writeln!(s, "font = {}", self.font);
        s
    }

    /// Writes this theme as the user's theme file (for a settings app).
    pub fn save(&self) -> io::Result<()> {
        let path = Self::path().ok_or_else(|| io::Error::other("no config directory"))?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(path, self.to_conf())
    }

    /// Regular text font. `font` from the theme file is installed into this
    /// slot by [`Theme::apply`].
    pub fn font(&self) -> Font {
        Font::Helvetica
    }

    pub fn bold_font(&self) -> Font {
        Font::HelveticaBold
    }

    /// Applies the theme to FLTK's global colors, font and box types.
    /// Called by [`crate::run`]; only needed directly for raw-fltk windows.
    pub fn apply(&self) {
        let rgb = |c: Color| c.to_rgb();
        let (r, g, b) = rgb(self.background);
        app::set_background_color(r, g, b);
        let (r, g, b) = rgb(self.surface_alt);
        app::set_background2_color(r, g, b);
        let (r, g, b) = rgb(self.text);
        app::set_foreground_color(r, g, b);
        let (r, g, b) = rgb(self.accent);
        app::set_selection_color(r, g, b);
        if !self.font.is_empty() {
            // FLTK font names: leading ' ' = regular, 'B' = bold.
            Font::set_font(Font::Helvetica, &format!(" {}", self.font));
            Font::set_font(Font::HelveticaBold, &format!("B{}", self.font));
        }
        app::set_font_size(self.font_size);
        app::set_font(self.font());
        app::set_visible_focus(false);
        RADIUS.store(self.radius, Ordering::Relaxed);
        app::set_frame_type_cb(ROUNDED, draw_rounded, 0, 0, 0, 0);
    }
}

/// A box type drawing a filled rounded rectangle with the theme radius, in
/// the widget's own color. Use it for any container that should look like a
/// card: `group.set_frame(heroui::theme::ROUNDED)`.
pub const ROUNDED: FrameType = FrameType::FreeBoxType;

static RADIUS: AtomicI32 = AtomicI32::new(10);

fn draw_rounded(x: i32, y: i32, w: i32, h: i32, c: Color) {
    let r = RADIUS.load(Ordering::Relaxed).min(w / 2).min(h / 2);
    fltk::draw::set_draw_color(c);
    fltk::draw::draw_rounded_rectf(x, y, w, h, r);
}

fn set<T>(slot: &mut T, value: Option<T>) {
    if let Some(v) = value {
        *slot = v;
    }
}

fn parse_color(s: &str) -> Option<Color> {
    let hex = s.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    u32::from_str_radix(hex, 16).ok().map(Color::from_hex)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conf_round_trips() {
        let mut t = Theme::light();
        t.radius = 4;
        t.font = "Inter".into();
        let mut back = Theme::dark();
        back.apply_conf(&t.to_conf());
        assert_eq!(back, t);
    }

    #[test]
    fn bad_lines_are_ignored() {
        let mut t = Theme::dark();
        t.apply_conf("accent = nope\nradius = x\n garbage\n# comment\nspacing = 3");
        assert_eq!(t.accent, Theme::dark().accent);
        assert_eq!(t.radius, Theme::dark().radius);
        assert_eq!(t.spacing, 3);
    }
}
