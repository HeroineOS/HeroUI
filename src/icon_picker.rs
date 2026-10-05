//! A button showing an icon; clicking it opens a searchable library of
//! icons to pick from: HeroUI's built-in ones and the icon theme's (apps,
//! places, devices...), with kind buttons and a name search.
//!
//! Like the color picker, it's a panel drawn inside the app's window (an
//! overlay group added on open, deleted on close): no extra surface, the
//! same on Wayland and X11. Only the visible icons are drawn (and cached).

use std::cell::RefCell;
use std::rc::Rc;

use fltk::button::Button;
use fltk::draw;
use fltk::enums::{Align, CallbackTrigger, Event, FrameType, Key};
use fltk::group::Group;
use fltk::input::Input;
use fltk::prelude::*;

use crate::element::Element;
use crate::hover::hover_amount;
use crate::icons::{self, KINDS};
use crate::theme::ROUNDED;
use crate::widgets::{custom_button, mix, repaint};

/// A button showing `value(state)` (an icon name or path; "" shows
/// "None"); clicking it opens the icon library, which sends
/// `on_change(name)` for the icon picked ("" for "None").
pub fn icon_button<S: 'static, M: 'static>(value: impl Fn(&S) -> String + 'static, on_change: impl Fn(String) -> M + 'static) -> Element<S, M> {
    Element::new(move |ctx| {
        let cur = Rc::new(RefCell::new(String::new()));
        let mut b = custom_button({
            let cur = cur.clone();
            move |b| {
                let t = crate::theme::current();
                let a = if b.value() { 1.0 } else { hover_amount(b) };
                draw::set_draw_color(mix(t.surface_alt, t.accent, 0.25 * a));
                draw::draw_rounded_rectf(b.x(), b.y(), b.w(), b.h(), t.radius.min(b.h() / 2));
                let name = cur.borrow();
                let s = (b.h() - 12).clamp(12, 24);
                if name.is_empty() || !icons::draw(&name, b.x() + 10, b.y() + (b.h() - s) / 2, s, t.text) {
                    draw::set_font(t.font(), t.font_size - 1);
                    draw::set_draw_color(t.text_dim);
                    let label = if name.is_empty() { "None" } else { "?" };
                    draw::draw_text2(label, b.x() + 10, b.y(), b.w() - 20, b.h(), Align::Left | Align::Inside);
                }
                draw::set_font(t.font(), t.font_size - 1);
                draw::set_draw_color(t.text_dim);
                draw::draw_text2("Choose…", b.x(), b.y(), b.w() - 10, b.h(), Align::Right | Align::Inside);
            }
        });
        let emit = ctx.emitter();
        let on_pick: Rc<dyn Fn(String)> = Rc::new(move |n| emit(on_change(n)));
        {
            let cur = cur.clone();
            b.set_callback(move |b| open(b, cur.borrow().clone(), on_pick.clone()));
        }
        let mut w = b.clone();
        ctx.bind(move |s| {
            let v = value(s);
            if *cur.borrow() != v {
                *cur.borrow_mut() = v;
                repaint(&mut w);
            }
        });
        b.as_base_widget()
    })
}

const PAD: i32 = 12;
const ROW_H: i32 = 32;
const CHIP_H: i32 = 26;
const CELL: i32 = 56;
const ICON: i32 = 28;
const FOOT: i32 = 24;
const NONE_W: i32 = 70;

struct Picker {
    /// Panel rectangle in the window.
    rect: (i32, i32, i32, i32),
    current: String,
    query: String,
    /// None: all kinds.
    kind: Option<usize>,
    /// Indexes into the library that match.
    shown: Vec<usize>,
    scroll: i32,
    hover: Option<usize>,
    /// Touch scrolling: the last pointer y.
    drag_y: Option<i32>,
}

impl Picker {
    fn filter(&mut self) {
        let lib = icons::library();
        let q = self.query.trim().to_lowercase();
        self.shown = (0..lib.len()).filter(|&i| self.kind.is_none_or(|k| lib[i].1 == k) && (q.is_empty() || lib[i].0.to_lowercase().contains(&q))).collect();
        self.scroll = 0;
        self.hover = None;
    }
    fn chips_y(&self) -> i32 {
        self.rect.1 + PAD + ROW_H + 8
    }
    /// (kind or None for "All", x, width)
    fn chips(&self) -> Vec<(Option<usize>, i32, i32)> {
        let t = crate::theme::current();
        draw::set_font(t.font(), t.font_size - 2);
        let mut x = self.rect.0 + PAD;
        std::iter::once(None)
            .chain((0..KINDS.len()).map(Some))
            .map(|k| {
                let w = draw::width(k.map_or("All", |k| KINDS[k])).ceil() as i32 + 18;
                let r = (k, x, w);
                x += w + 4;
                r
            })
            .collect()
    }
    fn grid(&self) -> (i32, i32, i32, i32) {
        let (x, y, w, h) = self.rect;
        let gy = self.chips_y() + CHIP_H + 8;
        (x + PAD, gy, w - 2 * PAD, y + h - PAD - FOOT - gy)
    }
    fn cols(&self) -> i32 {
        (self.grid().2 / CELL).max(1)
    }
    /// The cell rectangle of shown icon `i`.
    fn cell(&self, i: usize) -> (i32, i32, i32, i32) {
        let (gx, gy, gw, _) = self.grid();
        let cols = self.cols();
        let x0 = gx + (gw - cols * CELL) / 2;
        (x0 + (i as i32 % cols) * CELL, gy + (i as i32 / cols) * CELL - self.scroll, CELL, CELL)
    }
    fn content_h(&self) -> i32 {
        (self.shown.len() as i32 + self.cols() - 1) / self.cols() * CELL
    }
    /// Scrolls so the chosen icon is in view (when it's in the list).
    fn reveal_current(&mut self) {
        let lib = icons::library();
        let Some(i) = self.shown.iter().position(|&li| lib[li].0 == self.current) else { return };
        let (_, gy, _, gh) = self.grid();
        let (_, y, _, h) = self.cell(i);
        if y < gy {
            self.scroll_by(y - gy);
        } else if y + h > gy + gh {
            self.scroll_by(y + h - gy - gh);
        }
    }

    fn scroll_by(&mut self, dy: i32) {
        self.scroll = (self.scroll + dy).clamp(0, (self.content_h() - self.grid().3).max(0));
    }
    fn at(&self, ex: i32, ey: i32) -> Option<usize> {
        let (gx, gy, gw, gh) = self.grid();
        if !inside((gx, gy, gw, gh), ex, ey) {
            return None;
        }
        (0..self.shown.len()).find(|&i| inside(self.cell(i), ex, ey))
    }
}

fn inside((x, y, w, h): (i32, i32, i32, i32), px: i32, py: i32) -> bool {
    px >= x && px < x + w && py >= y && py < y + h
}

/// Opens the library over `anchor`'s window, below the button (above it
/// if there's no room).
fn open(anchor: &Button, current: String, on_pick: Rc<dyn Fn(String)>) {
    let Some(win) = anchor.window() else { return };
    let (ww, wh) = (win.w(), win.h());
    let (pw, ph) = (460.min(ww - 16), 440.min(wh - 16));
    let below = anchor.y() + anchor.h() + 6;
    let py = if below + ph <= wh { below } else { (anchor.y() - 6 - ph).max(8) };
    let px = (anchor.x() + anchor.w() / 2 - pw / 2).clamp(8, (ww - pw - 8).max(8));
    let st = Rc::new(RefCell::new(Picker {
        rect: (px, py, pw, ph),
        current,
        query: String::new(),
        kind: None,
        shown: vec![],
        scroll: 0,
        hover: None,
        drag_y: None,
    }));
    {
        let mut s = st.borrow_mut();
        s.filter();
        s.reveal_current();
    }

    win.begin();
    let mut overlay = Group::new(0, 0, ww, wh, None);
    overlay.set_frame(FrameType::NoBox);
    let t = crate::theme::current();
    let mut input = Input::new(px + PAD, py + PAD, pw - 2 * PAD - NONE_W - 8, ROW_H, None);
    input.set_frame(ROUNDED);
    input.set_color(t.surface_alt);
    input.set_text_color(t.text);
    input.set_text_font(t.font());
    input.set_text_size(t.font_size);
    input.set_cursor_color(t.accent);
    input.set_selection_color(t.accent);
    input.set_tooltip("Search icons by name");
    let mut none = custom_button(|b| {
        let t = crate::theme::current();
        let a = if b.value() { 1.0 } else { hover_amount(b) };
        draw::set_draw_color(mix(t.surface_alt, t.accent, 0.25 * a));
        draw::draw_rounded_rectf(b.x(), b.y(), b.w(), b.h(), t.radius.min(b.h() / 2));
        draw::set_draw_color(t.text);
        draw::set_font(t.font(), t.font_size - 1);
        draw::draw_text2("None", b.x(), b.y(), b.w(), b.h(), Align::Center);
    });
    none.resize(px + pw - PAD - NONE_W, py + PAD, NONE_W, ROW_H);
    overlay.end();
    win.end();
    crate::drag_scroll::set_blocked(true);

    let close = {
        let overlay = overlay.clone();
        move || {
            if overlay.was_deleted() {
                return;
            }
            crate::drag_scroll::set_blocked(false);
            let mut o = overlay.clone();
            if let Some(mut win) = o.parent() {
                win.redraw();
            }
            o.hide();
            fltk::app::delete_widget(o);
        }
    };
    none.set_callback({
        let (close, on_pick) = (close.clone(), on_pick.clone());
        move |_| {
            on_pick(String::new());
            close();
        }
    });
    input.set_trigger(CallbackTrigger::Changed);
    input.set_callback({
        let st = st.clone();
        let mut overlay = overlay.clone();
        move |i| {
            let mut s = st.borrow_mut();
            s.query = i.value();
            s.filter();
            overlay.redraw();
        }
    });
    overlay.draw({
        let st = st.clone();
        move |_| paint(&st.borrow())
    });
    overlay.super_draw_first(false);
    overlay.super_handle_first(false);
    overlay.handle({
        let close = close.clone();
        let input = input.clone();
        move |o, ev| {
            let (ex, ey) = (fltk::app::event_x(), fltk::app::event_y());
            let mut s = st.borrow_mut();
            let on_child = |w: &dyn WidgetExt| inside((w.x(), w.y(), w.w(), w.h()), ex, ey);
            match ev {
                Event::Push => {
                    if !inside(s.rect, ex, ey) {
                        drop(s);
                        close();
                        return true;
                    }
                    if on_child(&input) || on_child(&none) {
                        return false;
                    }
                    if let Some((k, _, _)) = s.chips().into_iter().find(|&(_, x, w)| inside((x, s.chips_y(), w, CHIP_H), ex, ey)) {
                        s.kind = k;
                        s.filter();
                        o.redraw();
                        return true;
                    }
                    s.drag_y = Some(ey);
                    true
                }
                Event::Drag => {
                    if let Some(y0) = s.drag_y {
                        if (ey - y0).abs() > 4 {
                            s.scroll_by(y0 - ey);
                            s.drag_y = Some(ey);
                            s.hover = None;
                            o.redraw();
                        }
                    }
                    true
                }
                Event::Released => {
                    let Some(y0) = s.drag_y.take() else { return false };
                    // A tap (not a scroll) on an icon picks it.
                    if (ey - y0).abs() <= 4 {
                        if let Some(i) = s.at(ex, ey) {
                            let name = icons::library()[s.shown[i]].0.clone();
                            drop(s);
                            on_pick(name);
                            close();
                        }
                    }
                    true
                }
                Event::MouseWheel => {
                    let dy = match fltk::app::event_dy() {
                        fltk::app::MouseWheel::Down => CELL * 2,
                        fltk::app::MouseWheel::Up => -CELL * 2,
                        _ => 0,
                    };
                    s.scroll_by(dy);
                    o.redraw();
                    true
                }
                Event::Enter | Event::Move => {
                    let h = s.at(ex, ey);
                    if h != s.hover {
                        s.hover = h;
                        o.redraw();
                    }
                    !(on_child(&input) || on_child(&none))
                }
                Event::KeyDown | Event::Shortcut if fltk::app::event_key() == Key::Escape => {
                    drop(s);
                    close();
                    true
                }
                _ => false,
            }
        }
    });
    // A window resize would misplace the panel; close instead.
    overlay.resize_callback(move |_, _, _, _, _| close());
    overlay.redraw();
    input.set_visible_focus();
    let _ = input.take_focus();
}

fn paint(s: &Picker) {
    let t = crate::theme::current();
    let (px, py, pw, ph) = s.rect;
    draw::set_draw_color(mix(t.background, fltk::enums::Color::Black, 0.35));
    draw::draw_rounded_rectf(px - 1, py + 1, pw + 2, ph + 3, t.radius + 2);
    draw::set_draw_color(t.border);
    draw::draw_rounded_rectf(px, py, pw, ph, t.radius);
    draw::set_draw_color(t.surface);
    draw::draw_rounded_rectf(px + 1, py + 1, pw - 2, ph - 2, t.radius);
    // Kinds.
    let cy = s.chips_y();
    for (k, x, w) in s.chips() {
        let on = k == s.kind;
        draw::set_draw_color(if on { t.accent } else { t.surface_alt });
        draw::draw_rounded_rectf(x, cy, w, CHIP_H, t.radius.min(CHIP_H / 2));
        draw::set_font(t.font(), t.font_size - 2);
        draw::set_draw_color(if on { t.accent_text } else { t.text });
        draw::draw_text2(k.map_or("All", |k| KINDS[k]), x, cy, w, CHIP_H, Align::Center);
    }
    // The icons that are in view.
    let lib = icons::library();
    let (gx, gy, gw, gh) = s.grid();
    draw::push_clip(gx, gy, gw, gh);
    for (i, &li) in s.shown.iter().enumerate() {
        let (x, y, w, h) = s.cell(i);
        if y + h < gy || y > gy + gh {
            continue;
        }
        let name = &lib[li].0;
        let on = *name == s.current;
        if on || s.hover == Some(i) {
            draw::set_draw_color(if on { mix(t.surface_alt, t.accent, 0.4) } else { t.surface_alt });
            draw::draw_rounded_rectf(x + 2, y + 2, w - 4, h - 4, t.radius.min(10));
        }
        if !icons::draw(name, x + (w - ICON) / 2, y + (h - ICON) / 2, ICON, t.text) {
            draw::set_draw_color(t.text_dim);
            draw::draw_text2("?", x, y, w, h, Align::Center);
        }
    }
    draw::pop_clip();
    if s.shown.is_empty() {
        draw::set_font(t.font(), t.font_size - 1);
        draw::set_draw_color(t.text_dim);
        draw::draw_text2("No icons with that name", gx, gy, gw, 40, Align::Center);
    }
    // The name of the one pointed at (or the current one), and the count.
    let name = s.hover.map(|i| lib[s.shown[i]].0.as_str()).unwrap_or(&s.current);
    let fy = py + ph - PAD - FOOT;
    draw::set_font(t.font(), t.font_size - 1);
    draw::set_draw_color(t.text);
    draw::draw_text2(name, px + PAD, fy, pw - 2 * PAD - 90, FOOT, Align::Left | Align::Inside | Align::Clip);
    draw::set_draw_color(t.text_dim);
    draw::draw_text2(&format!("{} icons", s.shown.len()), px + PAD, fy, pw - 2 * PAD, FOOT, Align::Right | Align::Inside);
}
