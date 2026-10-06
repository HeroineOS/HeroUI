//! Theme: colors, radius, spacing and font, shared by every HeroUI app.
//!
//! Loaded from `$XDG_CONFIG_HOME/heroui/theme.conf` (falling back to
//! `~/.config/heroui/theme.conf`), so one settings app can restyle every
//! HeroUI program. The file is plain `key = value` lines; colors are
//! `#rrggbb`. Unknown keys and bad values are ignored.
//!
//! `mode = dark | light | system` picks the base palette (system: the
//! desktop's dark/light preference); the file then only needs the accent
//! and whatever differs from that palette.
//!
//! Running apps follow the file live: [`crate::run`] watches it and calls
//! [`set_current`]. Draw code should read [`current`] at draw time;
//! build-time styling can re-apply itself with [`on_change`].

use std::cell::RefCell;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::rc::Rc;

use fltk::prelude::*;
use std::sync::atomic::{AtomicI32, Ordering};
use std::{fs, io};

use fltk::app;
use fltk::enums::{Color, Font, FrameType};

/// Which base palette a theme starts from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Dark,
    Light,
    /// Follow the desktop's dark/light preference.
    System,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub mode: Mode,
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
    /// Animate state changes (toggle knobs etc.). Off = reduced motion,
    /// also what a battery saver mode should set.
    pub animations: bool,
    /// Animation frames per second: the screen's refresh rate looks
    /// smoothest (60, 120, 144...). Only matters while something moves.
    pub frame_rate: i32,
    /// Freedesktop icon theme for app icons, e.g. "Papirus". Empty = GTK's
    /// setting, then hicolor/Adwaita.
    pub icon_theme: String,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            mode: Mode::Dark,
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
            animations: true,
            frame_rate: 60,
            icon_theme: String::new(),
        }
    }

    pub fn light() -> Self {
        Self {
            mode: Mode::Light,
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

    /// The base palette of `mode` (System asks the desktop), keeping the
    /// mode itself.
    pub fn for_mode(mode: Mode) -> Self {
        let dark = match mode {
            Mode::Dark => true,
            Mode::Light => false,
            Mode::System => system_prefers_dark().unwrap_or(true),
        };
        let mut t = if dark { Self::dark() } else { Self::light() };
        t.mode = mode;
        t
    }

    /// The user's theme file layered over its mode's palette (dark by
    /// default).
    pub fn load() -> Self {
        let text = Self::path().and_then(|p| fs::read_to_string(p).ok()).unwrap_or_default();
        Self::from_conf(&text)
    }

    /// A theme from theme-file text.
    pub fn from_conf(text: &str) -> Self {
        let mut mode_only = Self::dark();
        mode_only.apply_conf(text);
        let mut theme = Self::for_mode(mode_only.mode);
        theme.apply_conf(text);
        // A custom accent without its own text color gets a readable one.
        let has = |k: &str| text.lines().any(|l| l.split_once('=').is_some_and(|(key, _)| key.trim() == k));
        if has("accent") && !has("accent_text") {
            theme.accent_text = contrast_text(theme.accent);
        }
        theme
    }

    /// The palette this theme's colors are compared against when saving.
    fn base(&self) -> Self {
        match self.mode {
            Mode::Dark => Self::dark(),
            Mode::Light => Self::light(),
            // Whichever palette the current background is closer to.
            Mode::System if luminance(self.background) > 0.5 => Self::light(),
            Mode::System => Self::dark(),
        }
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
                "icon_theme" => self.icon_theme = value.to_string(),
                "mode" => {
                    set(
                        &mut self.mode,
                        match value {
                            "dark" => Some(Mode::Dark),
                            "light" => Some(Mode::Light),
                            "system" => Some(Mode::System),
                            _ => None,
                        },
                    )
                }
                "animations" => set(&mut self.animations, parse_bool(value)),
                "frame_rate" => set(&mut self.frame_rate, value.parse().ok().filter(|v| (24..=360).contains(v))),
                _ => {}
            }
        }
    }

    /// The theme-file text: the mode, the accent, colors that differ from
    /// the mode's palette, and the other settings.
    pub fn to_conf(&self) -> String {
        let mut s = String::from("# HeroUI theme (written by Appearance; hand edits are fine)\n");
        let mode = match self.mode {
            Mode::Dark => "dark",
            Mode::Light => "light",
            Mode::System => "system",
        };
        let _ = writeln!(s, "mode = {mode}");
        let base = self.base();
        let hex = |c: Color| {
            let (r, g, b) = c.to_rgb();
            format!("#{r:02x}{g:02x}{b:02x}")
        };
        let _ = writeln!(s, "accent = {}", hex(self.accent));
        if self.accent_text != contrast_text(self.accent) {
            let _ = writeln!(s, "accent_text = {}", hex(self.accent_text));
        }
        for (k, c, b) in [
            ("background", self.background, base.background),
            ("surface", self.surface, base.surface),
            ("surface_alt", self.surface_alt, base.surface_alt),
            ("text", self.text, base.text),
            ("text_dim", self.text_dim, base.text_dim),
            ("border", self.border, base.border),
        ] {
            if c != b {
                let _ = writeln!(s, "{k} = {}", hex(c));
            }
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
        let _ = writeln!(s, "animations = {}", self.animations);
        if self.frame_rate != 60 {
            let _ = writeln!(s, "frame_rate = {}", self.frame_rate);
        }
        if !self.icon_theme.is_empty() {
            let _ = writeln!(s, "icon_theme = {}", self.icon_theme);
        }
        s
    }

    /// Writes this theme as the user's theme file (for a settings app).
    pub fn save(&self) -> io::Result<()> {
        let path = Self::path().ok_or_else(|| io::Error::other("no config directory"))?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        // Atomic: apps watching the file never read half of it.
        let tmp = path.with_extension("conf.tmp");
        fs::write(&tmp, self.to_conf())?;
        fs::rename(tmp, path)
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
        // Tooltips (hover details in bars and apps).
        fltk::misc::Tooltip::set_color(self.surface);
        fltk::misc::Tooltip::set_text_color(self.text);
        fltk::misc::Tooltip::set_font(self.font());
        fltk::misc::Tooltip::set_font_size(self.font_size - 1);
        fltk::misc::Tooltip::set_margin_width(10);
        fltk::misc::Tooltip::set_margin_height(6);
        fltk::misc::Tooltip::set_delay(0.5);
        fltk::misc::Tooltip::set_hoverdelay(0.1);
        RADIUS.store(self.radius, Ordering::Relaxed);
        crate::anim::set_enabled(self.animations);
        crate::anim::set_frame_rate(self.frame_rate);
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

/// Relative luminance, 0.0 (black) to 1.0 (white).
pub fn luminance(c: Color) -> f32 {
    let (r, g, b) = c.to_rgb();
    (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) / 255.0
}

/// A text color readable on `bg`: near-black on light colors, white on
/// dark ones.
pub fn contrast_text(bg: Color) -> Color {
    if luminance(bg) > 0.55 {
        Color::from_hex(0x14141c)
    } else {
        Color::from_hex(0xffffff)
    }
}

/// The desktop's dark/light preference: the freedesktop portal's
/// `color-scheme` (GNOME, KDE, Cosmic, wlroots desktops with
/// xdg-desktop-portal), else GNOME's gsettings. `None` if unknown. Takes a
/// few milliseconds (a process), at most ~1 s if the portal hangs.
pub fn system_prefers_dark() -> Option<bool> {
    use std::process::{Command, Stdio};
    let run = |cmd: &str, args: &[&str]| {
        Command::new(cmd)
            .args(args)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
    };
    // "v u 1" (ReadOne) or "v v u 1" (older Read): 1 dark, 2 light, 0 none.
    for method in ["ReadOne", "Read"] {
        let out = run(
            "busctl",
            &[
                "--user",
                "--timeout=1",
                "call",
                "org.freedesktop.portal.Desktop",
                "/org/freedesktop/portal/desktop",
                "org.freedesktop.portal.Settings",
                method,
                "ss",
                "org.freedesktop.appearance",
                "color-scheme",
            ],
        );
        if let Some(v) = out.and_then(|o| o.split_whitespace().last().map(str::to_owned)) {
            match v.as_str() {
                "1" => return Some(true),
                "2" => return Some(false),
                _ => break,
            }
        }
    }
    let gs = run("gsettings", &["get", "org.gnome.desktop.interface", "color-scheme"])?;
    if gs.contains("dark") {
        Some(true)
    } else if gs.contains("light") {
        Some(false)
    } else {
        None
    }
}

// ---- The live theme ---------------------------------------------------

type Hook = (fltk::widget::Widget, Box<dyn FnMut(&Theme)>);

thread_local! {
    static CURRENT: RefCell<Rc<Theme>> = RefCell::new(Rc::new(Theme::dark()));
    static HOOKS: RefCell<Vec<Hook>> = const { RefCell::new(Vec::new()) };
}

/// The theme in use. Read it in draw code (it's an `Rc`, cheap to get), so
/// a theme change shows on the next redraw.
pub fn current() -> Rc<Theme> {
    CURRENT.with(|c| c.borrow().clone())
}

/// Makes `theme` the one in use: FLTK's globals, restyle hooks, and a
/// redraw of every window. Spacing and padding apply to newly built UI.
pub fn set_current(theme: Theme) {
    theme.apply();
    let t = Rc::new(theme);
    CURRENT.with(|c| *c.borrow_mut() = t.clone());
    let hooks = HOOKS.with(|h| std::mem::take(&mut *h.borrow_mut()));
    let mut kept: Vec<Hook> = hooks.into_iter().filter(|(w, _)| !w.was_deleted()).collect();
    for (_, f) in kept.iter_mut() {
        f(&t);
    }
    HOOKS.with(|h| {
        let mut h = h.borrow_mut();
        // Hooks registered while running the others come after them.
        kept.append(&mut h);
        *h = kept;
    });
    if let Some(wins) = fltk::app::windows() {
        for mut w in wins {
            w.redraw();
        }
    }
}

/// Runs `restyle` with the new theme whenever it changes, as long as
/// `widget` exists. For styling set once at build time (label colors,
/// input colors...).
pub fn on_change<W: fltk::prelude::WidgetExt>(widget: &W, restyle: impl FnMut(&Theme) + 'static) {
    HOOKS.with(|h| {
        let mut h = h.borrow_mut();
        // Drop hooks of deleted widgets now and then (list rebuilds).
        if h.len() % 64 == 63 {
            h.retain(|(w, _)| !w.was_deleted());
        }
        h.push((widget.as_base_widget(), Box::new(restyle)));
    });
}

fn set<T>(slot: &mut T, value: Option<T>) {
    if let Some(v) = value {
        *slot = v;
    }
}

fn parse_bool(s: &str) -> Option<bool> {
    match s {
        "true" | "on" | "yes" | "1" => Some(true),
        "false" | "off" | "no" | "0" => Some(false),
        _ => None,
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
        t.surface = Color::from_hex(0x123456);
        assert_eq!(Theme::from_conf(&t.to_conf()), t);
        let mut d = Theme::dark();
        d.accent = Color::from_hex(0x3ec99a);
        d.accent_text = contrast_text(d.accent);
        assert_eq!(Theme::from_conf(&d.to_conf()), d);
    }

    #[test]
    fn files_are_minimal_and_modes_keep_the_accent() {
        let mut t = Theme::dark();
        t.accent = Color::from_hex(0xff6b8b);
        let conf = t.to_conf();
        assert!(conf.contains("mode = dark") && conf.contains("accent = #ff6b8b"), "{conf}");
        assert!(!conf.contains("background") && !conf.contains("accent_text"), "{conf}");
        // Switching the mode line keeps the custom accent.
        let light = Theme::from_conf(&conf.replace("mode = dark", "mode = light"));
        assert_eq!(light.mode, Mode::Light);
        assert_eq!(light.background, Theme::light().background);
        assert_eq!(light.accent, Color::from_hex(0xff6b8b));
    }

    #[test]
    fn accent_text_contrasts() {
        assert_eq!(contrast_text(Color::from_hex(0xffe0a0)), Color::from_hex(0x14141c));
        assert_eq!(contrast_text(Color::from_hex(0x3a1f4a)), Color::from_hex(0xffffff));
        let t = Theme::from_conf("accent = #1a237e\n");
        assert_eq!(t.accent_text, Color::from_hex(0xffffff));
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
