//! Dropdown lists, on FLTK's own menu code (`Fl_Menu_Item::pulldown`, the
//! one `Fl_Choice` uses), styled from the theme. That makes the list a
//! real popup on every backend: an `xdg_popup` on Wayland (placed and
//! flipped by the compositor, grab handled), an override-redirect window
//! on X11. Nothing is allocated while no list is open except one hidden,
//! unparented `Fl_Menu_Button` per app that carries the styling.

use std::cell::RefCell;

use fltk::enums::{Color, FrameType};
use fltk::group::Group;
use fltk::menu::{MenuButton, MenuFlag};
use fltk::prelude::*;

use crate::theme::{Theme, ROUNDED};

thread_local! {
    static STYLE: RefCell<Option<MenuButton>> = const { RefCell::new(None) };
}

/// Shows `items` in a list right under `anchor` and blocks until the user
/// picks one (returns its index) or dismisses it (`None`). `selected` is
/// drawn in the accent color.
pub(crate) fn pick<W: WidgetExt>(anchor: &W, items: &[String], selected: Option<usize>, t: &Theme) -> Option<usize> {
    if items.is_empty() {
        return None;
    }
    STYLE.with(|style| {
        let mut style = style.borrow_mut();
        let menu = style.get_or_insert_with(|| {
            // Not part of any window; it only carries colors and fonts.
            Group::set_current(None::<&Group>);
            let mut m = MenuButton::default();
            m.hide();
            m
        });
        menu.clear();
        // FLTK styles the list from the menu widget: box/color for the
        // list, down_box/selection_color for the highlighted row.
        menu.set_frame(FrameType::FlatBox);
        menu.set_color(t.surface);
        menu.set_down_frame(ROUNDED);
        menu.set_selection_color(crate::widgets::mix(t.surface_alt, Color::White, 0.05));
        menu.set_text_font(t.font());
        menu.set_text_size(t.font_size);
        menu.set_text_color(t.text);
        for (i, item) in items.iter().enumerate() {
            let idx = menu.add(&escape(item), fltk::enums::Shortcut::None, MenuFlag::Normal, |_| {});
            if Some(i) == selected {
                if let Some(mut it) = menu.at(idx) {
                    it.set_label_color(t.accent);
                }
            }
        }
        let list = menu.menu()?;
        // Spacing between rows, like the rest of HeroUI.
        let spacing = fltk::app::menu_linespacing();
        fltk::app::set_menu_linespacing(t.font_size / 2 + 6);
        let picked = list.pulldown(anchor.x(), anchor.y(), anchor.w(), anchor.h(), None, Some(&*menu));
        fltk::app::set_menu_linespacing(spacing);
        let picked = picked?;
        (0..items.len() as i32).position(|i| menu.at(i).as_ref() == Some(&picked))
    })
}

/// Menu labels are parsed by `Fl_Menu_::add` ('/' makes submenus, '\\'
/// escapes, a leading '_' adds a divider) and drawn as FLTK labels ('&'
/// underlines a shortcut, '@' starts a symbol). Show text literally.
fn escape(label: &str) -> String {
    let mut out = String::with_capacity(label.len() + 4);
    for c in label.chars() {
        match c {
            '\\' | '/' | '_' => {
                out.push('\\');
                out.push(c);
            }
            '&' => out.push_str("&&"),
            '@' => out.push_str("@@"),
            _ => out.push(c),
        }
    }
    out
}
