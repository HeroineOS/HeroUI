//! Icons: a small built-in set of line icons drawn in any color (they
//! follow the theme), and app icons from the freedesktop icon themes
//! (what `.desktop` files name in `Icon=`).
//!
//! Every icon is rasterized once per (name, size, color) and kept at the
//! size it's drawn at, so a 24 px app icon costs ~2 KB however big the
//! file is. Lookups that fail are remembered too.
//!
//! ```ignore
//! heroui::icons::draw("battery-60-charging", x, y, 16, theme.text);
//! heroui::icons::draw("firefox-esr", x, y, 24, theme.text); // theme icon
//! ```

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use fltk::enums::Color;
use fltk::image::{RgbImage, SharedImage, SvgImage};
use fltk::prelude::*;

/// Names of the built-in icons. `battery-<0..100>` (rounded to 10),
/// `battery-<n>-charging` and `network-wireless-<0..100>` (signal, in 4
/// steps) are built from the level.
pub const BUILTIN: &[&str] = &[
    "apps",
    "app",
    "arrow-down",
    "arrow-up",
    "battery",
    "bluetooth",
    "bluetooth-connected",
    "bluetooth-off",
    "brightness",
    "check",
    "clock",
    "cpu",
    "folder",
    "lock",
    "memory",
    "microphone",
    "microphone-muted",
    "network-wired",
    "network-wireless",
    "network-offline",
    "power",
    "refresh",
    "search",
    "settings",
    "terminal",
    "volume-high",
    "volume-low",
    "volume-muted",
];

/// The body of a built-in icon (24×24 grid, 2 px round strokes), with
/// `C` where its color goes.
fn builtin_body(name: &str) -> Option<String> {
    if let Some(rest) = name.strip_prefix("battery") {
        let (level, charging) = match rest.strip_suffix("-charging") {
            Some(r) => (r, true),
            None => (rest, false),
        };
        let level = level.strip_prefix('-').map_or(Some(100), |l| l.parse::<u32>().ok())?;
        return Some(battery(level.min(100), charging));
    }
    if let Some(level) = name.strip_prefix("network-wireless-") {
        return Some(wifi(level.parse::<u32>().ok()?.min(100)));
    }
    let wifi = r#"<path d="M9.2 16.2A4 4 0 0 1 14.8 16.2M6.3 13.3A8 8 0 0 1 17.7 13.3M3.5 10.5A12 12 0 0 1 20.5 10.5"/><circle cx="12" cy="19.5" r="1.2" fill="C" stroke="none"/>"#;
    let rune = r#"<path d="M7 7.5l10 9-5 4.5V3l5 4.5-10 9"/>"#;
    let mic = r#"<rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5.5 11a6.5 6.5 0 0 0 13 0M12 17.5V21"/>"#;
    let speaker = r#"<path d="M11 5L6.5 9H3v6h3.5L11 19z"/>"#;
    Some(match name {
        "apps" => r#"<rect x="4" y="4" width="6.5" height="6.5" rx="1.5"/><rect x="13.5" y="4" width="6.5" height="6.5" rx="1.5"/><rect x="4" y="13.5" width="6.5" height="6.5" rx="1.5"/><rect x="13.5" y="13.5" width="6.5" height="6.5" rx="1.5"/>"#.into(),
        "app" => r#"<rect x="3" y="4" width="18" height="16" rx="2.5"/><path d="M3 9h18"/>"#.into(),
        "arrow-down" => r#"<path d="M12 4v15M6 13l6 6 6-6"/>"#.into(),
        "arrow-up" => r#"<path d="M12 20V5M6 11l6-6 6 6"/>"#.into(),
        "bluetooth" => rune.into(),
        "bluetooth-connected" => format!(r#"{rune}<circle cx="3.5" cy="12" r="1.3" fill="C" stroke="none"/><circle cx="20.5" cy="12" r="1.3" fill="C" stroke="none"/>"#),
        "bluetooth-off" => format!(r#"<g opacity="0.45">{rune}</g><path d="M4 4l16 16"/>"#),
        "check" => r#"<path d="M5 12.5l4.5 4.5L19 7"/>"#.into(),
        "folder" => r#"<path d="M3 7.5a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2V17a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>"#.into(),
        "lock" => r#"<rect x="5" y="11" width="14" height="10" rx="2"/><path d="M8 11V7.5a4 4 0 0 1 8 0V11"/>"#.into(),
        "microphone" => mic.into(),
        "microphone-muted" => format!(r#"{mic}<path d="M4 4l16 16"/>"#),
        "refresh" => r#"<path d="M20 12a8 8 0 1 1-2.3-5.7M20 4v5h-5"/>"#.into(),
        "clock" => r#"<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3.5 2"/>"#.into(),
        "brightness" => r#"<circle cx="12" cy="12" r="4"/><path d="M12 2.5v2M12 19.5v2M2.5 12h2M19.5 12h2M5.3 5.3l1.4 1.4M17.3 17.3l1.4 1.4M5.3 18.7l1.4-1.4M17.3 6.7l1.4-1.4"/>"#.into(),
        "cpu" => r#"<rect x="6" y="6" width="12" height="12" rx="2"/><rect x="9.5" y="9.5" width="5" height="5" rx="1" fill="C" stroke="none"/><path d="M9.5 2.5v3M14.5 2.5v3M9.5 18.5v3M14.5 18.5v3M2.5 9.5h3M2.5 14.5h3M18.5 9.5h3M18.5 14.5h3"/>"#.into(),
        "memory" => r#"<rect x="2.5" y="7" width="19" height="9" rx="1.5"/><path d="M6.5 10v3M10.5 10v3M13.5 10v3M17.5 10v3M6 16v3M10 16v3M14 16v3M18 16v3"/>"#.into(),
        "network-wired" => r#"<rect x="9" y="3" width="6" height="5" rx="1"/><rect x="3" y="16" width="6" height="5" rx="1"/><rect x="15" y="16" width="6" height="5" rx="1"/><path d="M12 8v4M6 16v-4h12v4"/>"#.into(),
        "network-wireless" => wifi.into(),
        "network-offline" => format!(r#"<g opacity="0.45">{wifi}</g><path d="M4 4l16 16"/>"#),
        "power" => r#"<path d="M12 3v8M6.4 6.6a8 8 0 1 0 11.2 0"/>"#.into(),
        "search" => r#"<circle cx="10.5" cy="10.5" r="6.5"/><path d="M15.5 15.5L21 21"/>"#.into(),
        "settings" => r#"<path d="M4 7h9M18 7h2M4 17h2M11 17h9"/><circle cx="15.5" cy="7" r="2.5"/><circle cx="8.5" cy="17" r="2.5"/>"#.into(),
        "terminal" => r#"<rect x="2.5" y="4" width="19" height="16" rx="2.5"/><path d="M7 9.5l3 2.5-3 2.5M12.5 15h4.5"/>"#.into(),
        "volume-high" => format!(r#"{speaker}<path d="M15 9a4 4 0 0 1 0 6M18 6a8 8 0 0 1 0 12"/>"#),
        "volume-low" => format!(r#"{speaker}<path d="M15 9a4 4 0 0 1 0 6"/>"#),
        "volume-muted" => format!(r#"{speaker}<path d="M15.5 9.5l5 5M20.5 9.5l-5 5"/>"#),
        _ => return None,
    })
}

/// Wi-Fi with 0-3 of its arcs lit for the signal (the rest faint).
fn wifi(level: u32) -> String {
    let lit = match level {
        75.. => 3,
        50..=74 => 2,
        25..=49 => 1,
        _ => 0,
    };
    let arcs = ["M9.2 16.2A4 4 0 0 1 14.8 16.2", "M6.3 13.3A8 8 0 0 1 17.7 13.3", "M3.5 10.5A12 12 0 0 1 20.5 10.5"];
    let mut s = String::from(r#"<circle cx="12" cy="19.5" r="1.2" fill="C" stroke="none"/>"#);
    for (i, d) in arcs.iter().enumerate() {
        let faint = if i < lit { "" } else { r#" opacity="0.3""# };
        s += &format!(r#"<path d="{d}"{faint}/>"#);
    }
    s
}

fn battery(level: u32, charging: bool) -> String {
    // Rounded to 10 % so a draining battery doesn't make a new icon each
    // percent.
    let level = (level + 5) / 10 * 10;
    let fill = 13.0 * level as f32 / 100.0;
    let mut s = String::from(r#"<rect x="2" y="7" width="17" height="10" rx="2"/><path d="M22 10.5v3"/>"#);
    if charging {
        // A bolt instead of the level (the text says how full it is).
        s += r#"<path d="M11.5 8.5L8.5 12h4l-3 3.5" stroke-width="1.6"/>"#;
    } else if fill > 0.5 {
        s += &format!(r#"<rect x="4" y="9" width="{fill:.1}" height="6" rx="1" fill="C" stroke="none"/>"#);
    }
    s
}

/// The SVG of a built-in icon in `color`, or None.
pub fn builtin_svg(name: &str, color: Color) -> Option<String> {
    let (r, g, b) = color.to_rgb();
    let c = format!("#{r:02x}{g:02x}{b:02x}");
    let body = builtin_body(name)?.replace("\"C\"", &format!("\"{c}\""));
    Some(format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24"><g fill="none" stroke="{c}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">{body}</g></svg>"#
    ))
}

/// True if `name` is a built-in icon (including `battery-<n>[-charging]`).
pub fn is_builtin(name: &str) -> bool {
    builtin_body(name).is_some()
}

type Key = (String, i32, u32);

thread_local! {
    static CACHE: RefCell<HashMap<Key, Option<RgbImage>>> = RefCell::new(HashMap::new());
    static PATHS: RefCell<HashMap<String, Option<PathBuf>>> = RefCell::new(HashMap::new());
}

/// Most icons kept; past this the cache starts over (icons are cheap to
/// make again, a leak isn't).
const CACHE_MAX: usize = 256;

/// Draws icon `name` in a `size`×`size` box at (x, y). Built-in icons take
/// `color`; theme icons and image files keep their own colors. `name` can
/// also be a path to a png/svg/xpm. Returns false (drawing nothing) if
/// there's no such icon.
pub fn draw(name: &str, x: i32, y: i32, size: i32, color: Color) -> bool {
    if name.is_empty() || size <= 0 {
        return false;
    }
    let builtin = is_builtin(name);
    let key = (name.to_owned(), size, if builtin { color.bits() } else { 0 });
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if !c.contains_key(&key) {
            if c.len() >= CACHE_MAX {
                c.clear();
            }
            let img = render(name, size, color, builtin);
            c.insert(key.clone(), img);
        }
        match c.get_mut(&key).and_then(|i| i.as_mut()) {
            Some(img) => {
                img.draw(x, y, size, size);
                true
            }
            None => false,
        }
    })
}

/// True if [`draw`] would find `name`.
pub fn exists(name: &str) -> bool {
    is_builtin(name) || Path::new(name).is_file() || find(name).is_some()
}

/// Rasterizes at the screen's pixel size (crisp on HiDPI), drawn at `size`.
fn render(name: &str, size: i32, color: Color, builtin: bool) -> Option<RgbImage> {
    let scale = fltk::app::screen_scale(0).max(1.0);
    let px = (size as f32 * scale).round() as i32;
    let mut img = if builtin {
        svg_to_rgb(SvgImage::from_data(&builtin_svg(name, color)?).ok()?, px)?
    } else {
        let path = if Path::new(name).is_absolute() { Some(PathBuf::from(name)) } else { find(name) }?;
        load_sized(&path, px)?
    };
    img.scale(size, size, true, true);
    Some(img)
}

/// SVGs rasterize lazily; `normalize` does it now, at `px`.
fn svg_to_rgb(svg: SvgImage, px: i32) -> Option<RgbImage> {
    let mut img = svg.copy_sized(px, px);
    img.normalize();
    img.to_rgb().ok()
}

fn load_sized(path: &Path, px: i32) -> Option<RgbImage> {
    fltk::image::Image::set_scaling_algorithm(fltk::image::RgbScaling::Bilinear);
    if path.extension().is_some_and(|e| e == "svg") {
        // Straight from the file, so nothing big is kept around.
        return svg_to_rgb(SvgImage::load(path).ok()?, px);
    }
    let img = SharedImage::load(path).ok()?;
    if img.w() <= 0 || img.h() <= 0 {
        return None;
    }
    // Keep the aspect ratio inside the square.
    let (w, h) = if img.w() >= img.h() {
        (px, (px * img.h() / img.w()).max(1))
    } else {
        ((px * img.w() / img.h()).max(1), px)
    };
    img.copy_sized(w, h).to_rgb().ok()
}

/// The file of a freedesktop theme icon (`Icon=` of a .desktop file), from
/// the user's icon theme, then hicolor, Adwaita and /usr/share/pixmaps.
/// Results are cached.
pub fn find(name: &str) -> Option<PathBuf> {
    if let Some(hit) = PATHS.with(|p| p.borrow().get(name).cloned()) {
        return hit;
    }
    let found = search(name);
    PATHS.with(|p| p.borrow_mut().insert(name.to_owned(), found.clone()));
    found
}

fn data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    match std::env::var_os("XDG_DATA_HOME") {
        Some(d) => dirs.push(PathBuf::from(d)),
        None => dirs.extend(home.as_ref().map(|h| h.join(".local/share"))),
    }
    let sys = std::env::var("XDG_DATA_DIRS").unwrap_or_default();
    let sys = if sys.is_empty() { "/usr/local/share:/usr/share".to_owned() } else { sys };
    dirs.extend(sys.split(':').filter(|s| !s.is_empty()).map(PathBuf::from));
    dirs
}

/// The configured icon theme: HeroUI's `icon-theme`, else GTK's setting.
fn icon_theme() -> Option<String> {
    let t = crate::theme::current().icon_theme.clone();
    if !t.is_empty() {
        return Some(t);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    let ini = std::fs::read_to_string(home.join(".config/gtk-3.0/settings.ini")).ok()?;
    ini.lines()
        .filter_map(|l| l.split_once('='))
        .find(|(k, _)| k.trim() == "gtk-icon-theme-name")
        .map(|(_, v)| v.trim().trim_matches('"').to_owned())
}

/// The themes to look in, in order, following `Inherits=` once.
fn themes(bases: &[PathBuf]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let push = |t: &str, out: &mut Vec<String>| {
        if !t.is_empty() && !out.iter().any(|o| o == t) {
            out.push(t.to_owned());
        }
    };
    if let Some(t) = icon_theme() {
        push(&t, &mut out);
        for b in bases {
            if let Ok(index) = std::fs::read_to_string(b.join(&t).join("index.theme")) {
                if let Some(inherits) = index.lines().find_map(|l| l.strip_prefix("Inherits=")) {
                    for i in inherits.split(',') {
                        push(i.trim(), &mut out);
                    }
                }
                break;
            }
        }
    }
    for t in ["hicolor", "Adwaita", "breeze", "Papirus"] {
        push(t, &mut out);
    }
    out
}

fn search(name: &str) -> Option<PathBuf> {
    if name.contains('/') {
        let p = PathBuf::from(name);
        return p.is_file().then_some(p);
    }
    let data = data_dirs();
    let mut bases: Vec<PathBuf> = data.iter().map(|d| d.join("icons")).collect();
    if let Some(h) = std::env::var_os("HOME") {
        bases.insert(0, PathBuf::from(h).join(".icons"));
    }
    // Scalable first, then big enough bitmaps, then whatever exists.
    const SIZES: [&str; 10] = ["scalable", "48x48", "64x64", "96x96", "128x128", "256x256", "32x32", "24x24", "22x22", "16x16"];
    const KINDS: [&str; 5] = ["apps", "places", "devices", "status", "categories"];
    for theme in themes(&bases) {
        for base in &bases {
            let dir = base.join(&theme);
            if !dir.is_dir() {
                continue;
            }
            for size in SIZES {
                for kind in KINDS {
                    // Both layouts: <size>/<kind> (hicolor) and <kind>/<size> (Papirus is
                    // the former, some themes the latter).
                    for sub in [dir.join(size).join(kind), dir.join(kind).join(size.split('x').next().unwrap_or(size))] {
                        for ext in ["svg", "png"] {
                            let p = sub.join(format!("{name}.{ext}"));
                            if p.is_file() {
                                return Some(p);
                            }
                        }
                    }
                }
            }
        }
    }
    for d in &data {
        for ext in ["svg", "png", "xpm"] {
            let p = d.join("pixmaps").join(format!("{name}.{ext}"));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_are_valid_svg() {
        for name in BUILTIN.iter().copied().chain(["battery-0", "battery-45-charging", "battery-100", "network-wireless-0", "network-wireless-80"]) {
            let svg = builtin_svg(name, Color::from_rgb(1, 2, 3)).unwrap_or_else(|| panic!("{name}"));
            assert!(svg.contains("#010203") && !svg.contains("\"C\""), "{name}: {svg}");
        }
        assert!(!is_builtin("battery-x") && !is_builtin("nope"));
    }
}
