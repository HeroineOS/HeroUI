//! Themed popup list (dropdown menus). The window exists only while open:
//! nothing is allocated for a closed dropdown.

use std::cell::Cell;
use std::rc::Rc;

use fltk::app;
use fltk::draw;
use fltk::enums::{Align, Event, Key};
use fltk::group::Group;
use fltk::prelude::*;
use fltk::window::Window;

use crate::theme::Theme;

const MAX_ROWS: usize = 10;

/// Opens a list of `items` under (or above, if it doesn't fit) `anchor`.
/// `on_pick(i)` runs when the user picks one; any click outside, Escape or
/// focus loss closes it.
pub(crate) fn open<W: WidgetExt>(
    anchor: &W,
    items: Vec<String>,
    selected: Option<usize>,
    theme: Rc<Theme>,
    on_pick: impl Fn(usize) + 'static,
) {
    let Some(parent) = anchor.window() else { return };
    if items.is_empty() {
        return;
    }
    let t = theme;
    let row_h = t.font_size + 14;
    let pad = 4;
    let rows = items.len().min(MAX_ROWS);
    let (w, h) = (anchor.w().max(80), rows as i32 * row_h + 2 * pad);
    let x = parent.x_root() + anchor.x();
    let below = parent.y_root() + anchor.y() + anchor.h() + 2;
    let (_, sy, _, sh) = app::screen_work_area(app::screen_num(x, below));
    let y = if below + h > sy + sh { parent.y_root() + anchor.y() - h - 2 } else { below };

    let hover = Rc::new(Cell::new(selected));
    let top = Rc::new(Cell::new(selected.map_or(0, |s| s.saturating_sub(rows - 1)).min(items.len() - rows)));

    // A top-level window, not a child of whatever group is open.
    Group::set_current(None::<&Group>);
    let mut pop = Window::new(x, y, w, h, None);
    pop.end();
    pop.set_border(false);
    pop.set_override();
    pop.set_color(t.surface);

    let items = Rc::new(items);
    {
        let (items, hover, top, t) = (items.clone(), hover.clone(), top.clone(), t.clone());
        pop.draw(move |p| {
            draw::set_draw_color(t.surface);
            draw::draw_rectf(0, 0, p.w(), p.h());
            draw::set_draw_color(t.border);
            draw::draw_rect(0, 0, p.w(), p.h());
            draw::set_font(t.font(), t.font_size);
            for row in 0..rows {
                let i = top.get() + row;
                let ry = pad + row as i32 * row_h;
                if hover.get() == Some(i) {
                    draw::set_draw_color(t.surface_alt);
                    draw::draw_rounded_rectf(pad, ry, p.w() - 2 * pad, row_h, t.radius.min(row_h / 2));
                }
                draw::set_draw_color(if selected == Some(i) { t.accent } else { t.text });
                draw::draw_text2(&items[i], pad + 10, ry, p.w() - 2 * pad - 20, row_h, Align::Left);
            }
        });
    }

    let row_at = move |ey: i32, top: usize| {
        let row = (ey - pad).div_euclid(row_h);
        (0..rows as i32).contains(&row).then(|| top + row as usize)
    };
    let close = |p: &mut Window| {
        app::set_grab(None::<Window>);
        p.hide();
        app::delete_widget(p.clone());
    };
    pop.handle(move |p, ev| {
        let (ex, ey) = (app::event_x(), app::event_y());
        let inside = ex >= 0 && ey >= 0 && ex < p.w() && ey < p.h();
        let set_hover = |p: &mut Window, i: Option<usize>| {
            if hover.replace(i) != i {
                p.redraw();
            }
        };
        match ev {
            Event::Move | Event::Drag => {
                set_hover(p, if inside { row_at(ey, top.get()) } else { None });
                true
            }
            Event::Push => {
                if !inside {
                    close(p);
                }
                true
            }
            Event::Released => {
                if let Some(i) = inside.then(|| row_at(ey, top.get())).flatten() {
                    close(p);
                    on_pick(i);
                }
                true
            }
            Event::MouseWheel => {
                let max = items.len() - rows;
                let next = (top.get() as i32 + app::event_dy_value().signum()).clamp(0, max as i32) as usize;
                if next != top.get() {
                    top.set(next);
                    p.redraw();
                }
                true
            }
            Event::KeyDown => {
                let last = items.len() - 1;
                let cur = hover.get();
                match app::event_key() {
                    Key::Escape => close(p),
                    Key::Enter | Key::KPEnter => {
                        if let Some(i) = cur {
                            close(p);
                            on_pick(i);
                        }
                    }
                    k @ (Key::Up | Key::Down) => {
                        let i = match (k == Key::Down, cur) {
                            (true, None) => 0,
                            (true, Some(i)) => (i + 1).min(last),
                            (false, None) => last,
                            (false, Some(i)) => i.saturating_sub(1),
                        };
                        // Keep the highlighted row in view.
                        if i < top.get() {
                            top.set(i);
                        } else if i >= top.get() + rows {
                            top.set(i + 1 - rows);
                        }
                        hover.set(Some(i));
                        p.redraw();
                    }
                    _ => {}
                }
                true
            }
            Event::Hide => {
                app::set_grab(None::<Window>);
                false
            }
            _ => false,
        }
    });
    pop.show();
    app::set_grab(Some(pop));
}
