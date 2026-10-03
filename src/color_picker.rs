//! A color swatch that opens a picker: a saturation/brightness square, a
//! hue strip, and a hex field (for pasting). Changes are sent live while
//! dragging, so an app can preview them.
//!
//! The picker is a popover drawn inside the app's own window (an overlay
//! group added on open and deleted on close), not a separate window: no
//! extra surface, no popup-placement rules, the same on Wayland and X11.
//! Nothing exists while it's closed except the swatch.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use fltk::button::Button;
use fltk::draw;
use fltk::enums::{Align, CallbackTrigger, Color, ColorDepth, Event, FrameType, Key};
use fltk::group::Group;
use fltk::image::RgbImage;
use fltk::input::Input;
use fltk::prelude::*;

use crate::element::Element;
use crate::hover::{hover_amount, is_hovered};
use crate::theme::ROUNDED;
use crate::widgets::{custom_button, mix, repaint};

/// A swatch showing `value(state)`; clicking it opens a color picker that
/// sends `on_change(color)` as the user picks.
pub fn color_button<S: 'static, M: 'static>(
    value: impl Fn(&S) -> Color + 'static,
    on_change: impl Fn(Color) -> M + 'static,
) -> Element<S, M> {
    Element::new(move |ctx| {
        let color = Rc::new(Cell::new(Color::Black));
        let mut b = custom_button({
            let color = color.clone();
            move |b| {
                let t = crate::theme::current();
                let s = b.w().min(b.h()) - 6;
                let (x, y) = (b.x() + (b.w() - s) / 2, b.y() + (b.h() - s) / 2);
                let ring = if is_hovered(b) || b.value() { t.text_dim } else { t.border };
                draw::set_draw_color(ring);
                draw::draw_rounded_rectf(x - 2, y - 2, s + 4, s + 4, 8);
                draw::set_draw_color(color.get());
                draw::draw_rounded_rectf(x, y, s, s, 6);
            }
        });
        let emit = ctx.emitter();
        let on_pick: Rc<dyn Fn(Color)> = Rc::new(move |c| emit(on_change(c)));
        {
            let color = color.clone();
            b.set_callback(move |b| open(b, color.get(), on_pick.clone()));
        }
        let mut w = b.clone();
        ctx.bind(move |s| {
            let c = value(s);
            if color.replace(c) != c {
                repaint(&mut w);
            }
        });
        b.as_base_widget()
    })
}

// Picker layout, in pixels.
const PAD: i32 = 12;
const AREA_W: i32 = 240;
const SV_H: i32 = 150;
const HUE_H: i32 = 14;
const ROW_H: i32 = 32;
const PANEL_W: i32 = AREA_W + 2 * PAD;
const PANEL_H: i32 = PAD + SV_H + 10 + HUE_H + 12 + ROW_H + PAD;
const DONE_W: i32 = 70;

#[derive(Clone, Copy, PartialEq)]
enum Part {
    Sv,
    Hue,
}

struct Picker {
    hsv: (f64, f64, f64),
    /// Panel position inside the overlay.
    px: i32,
    py: i32,
    drag: Option<Part>,
    /// The saturation/brightness square for the hue it was made for.
    sv: Option<(f64, RgbImage)>,
    hue: Option<RgbImage>,
}

impl Picker {
    fn color(&self) -> Color {
        let (r, g, b) = hsv_to_rgb(self.hsv.0, self.hsv.1, self.hsv.2);
        Color::from_rgb(r, g, b)
    }
    fn sv_rect(&self) -> (i32, i32, i32, i32) {
        (self.px + PAD, self.py + PAD, AREA_W, SV_H)
    }
    fn hue_rect(&self) -> (i32, i32, i32, i32) {
        (self.px + PAD, self.py + PAD + SV_H + 10, AREA_W, HUE_H)
    }
    fn row_y(&self) -> i32 {
        self.py + PANEL_H - PAD - ROW_H
    }
}

fn inside((x, y, w, h): (i32, i32, i32, i32), px: i32, py: i32) -> bool {
    px >= x && px < x + w && py >= y && py < y + h
}

/// Opens the picker over `anchor`'s window, below the swatch (above it if
/// there's no room).
fn open(anchor: &Button, start: Color, on_pick: Rc<dyn Fn(Color)>) {
    let Some(win) = anchor.window() else { return };
    let (ww, wh) = (win.w(), win.h());
    let below = anchor.y() + anchor.h() + 6;
    let py = if below + PANEL_H <= wh { below } else { (anchor.y() - 6 - PANEL_H).max(0) };
    let px = (anchor.x() + anchor.w() / 2 - PANEL_W / 2).clamp(0, (ww - PANEL_W).max(0));
    let (r, g, b) = start.to_rgb();
    let state = Rc::new(RefCell::new(Picker {
        hsv: rgb_to_hsv(r, g, b),
        px,
        py,
        drag: None,
        sv: None,
        hue: None,
    }));

    win.begin();
    let mut overlay = Group::new(0, 0, ww, wh, None);
    overlay.set_frame(FrameType::NoBox);
    let t = crate::theme::current();
    let row_y = state.borrow().row_y();
    let mut input = Input::new(px + PAD + ROW_H + 8, row_y, 110, ROW_H, None);
    input.set_frame(ROUNDED);
    input.set_color(t.surface_alt);
    input.set_text_color(t.text);
    input.set_text_font(t.font());
    input.set_text_size(t.font_size);
    input.set_cursor_color(t.accent);
    input.set_selection_color(t.accent);
    input.set_value(&hex(start));
    let mut done = custom_button(|b| {
        let t = crate::theme::current();
        let bg = if b.value() {
            mix(t.accent, t.background, 0.25)
        } else {
            mix(t.accent, Color::White, 0.1 * hover_amount(b))
        };
        draw::set_draw_color(bg);
        draw::draw_rounded_rectf(b.x(), b.y(), b.w(), b.h(), t.radius.min(b.h() / 2));
        draw::set_draw_color(t.accent_text);
        draw::set_font(t.font(), t.font_size);
        draw::draw_text2("Done", b.x(), b.y(), b.w(), b.h(), Align::Center);
    });
    done.resize(px + PANEL_W - PAD - DONE_W, row_y, DONE_W, ROW_H);
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
    done.set_callback({
        let close = close.clone();
        move |_| close()
    });

    // Typing or pasting a hex color.
    input.set_trigger(CallbackTrigger::Changed);
    input.set_callback({
        let state = state.clone();
        let on_pick = on_pick.clone();
        let mut overlay = overlay.clone();
        move |i| {
            if let Some(c) = parse_hex(&i.value()) {
                let (r, g, b) = c.to_rgb();
                state.borrow_mut().hsv = rgb_to_hsv(r, g, b);
                overlay.redraw();
                on_pick(c);
            }
        }
    });

    overlay.draw({
        let state = state.clone();
        move |_| paint(&mut state.borrow_mut())
    });
    // The panel is painted first, the field and button on top.
    overlay.super_draw_first(false);
    // This handler decides first; unhandled events go to the children.
    overlay.super_handle_first(false);
    overlay.handle({
        let close = close.clone();
        let mut input = input.clone();
        move |o, ev| {
            let (ex, ey) = (fltk::app::event_x(), fltk::app::event_y());
            let mut st = state.borrow_mut();
            let panel = (st.px, st.py, PANEL_W, PANEL_H);
            let on_child = |w: &dyn WidgetExt| inside((w.x(), w.y(), w.w(), w.h()), ex, ey);
            match ev {
                Event::Push => {
                    if !inside(panel, ex, ey) {
                        drop(st);
                        close();
                        return true;
                    }
                    st.drag = if inside(st.sv_rect(), ex, ey) {
                        Some(Part::Sv)
                    } else if inside(st.hue_rect(), ex, ey) {
                        Some(Part::Hue)
                    } else {
                        None
                    };
                    if st.drag.is_none() {
                        // The field and the button take their own clicks;
                        // the rest of the panel swallows them.
                        return !(on_child(&input) || on_child(&done));
                    }
                    pick(&mut st, ex, ey, o, &mut input, &on_pick);
                    true
                }
                Event::Drag if st.drag.is_some() => {
                    pick(&mut st, ex, ey, o, &mut input, &on_pick);
                    true
                }
                Event::Released if st.drag.is_some() => {
                    st.drag = None;
                    true
                }
                Event::KeyDown | Event::Shortcut if fltk::app::event_key() == Key::Escape => {
                    drop(st);
                    close();
                    true
                }
                Event::KeyDown | Event::Shortcut if matches!(fltk::app::event_key(), Key::Enter | Key::KPEnter) => {
                    drop(st);
                    close();
                    true
                }
                // Nothing under the overlay reacts (hover, wheel) while
                // it's open, except the field and button.
                Event::Enter | Event::Move => !(on_child(&input) || on_child(&done)),
                Event::MouseWheel => true,
                _ => false,
            }
        }
    });
    // A window resize would misplace the panel; close instead.
    overlay.resize_callback(move |_, _, _, _, _| close());
    overlay.redraw();
    let _ = input.take_focus();
}

/// Updates the color from the pointer while dragging in a part.
fn pick(st: &mut Picker, ex: i32, ey: i32, o: &mut Group, input: &mut Input, on_pick: &Rc<dyn Fn(Color)>) {
    match st.drag {
        Some(Part::Sv) => {
            let (x, y, w, h) = st.sv_rect();
            st.hsv.1 = ((ex - x) as f64 / (w - 1) as f64).clamp(0.0, 1.0);
            st.hsv.2 = 1.0 - ((ey - y) as f64 / (h - 1) as f64).clamp(0.0, 1.0);
        }
        Some(Part::Hue) => {
            let (x, _, w, _) = st.hue_rect();
            st.hsv.0 = ((ex - x) as f64 / (w - 1) as f64).clamp(0.0, 1.0) * 360.0;
        }
        None => return,
    }
    let c = st.color();
    input.set_value(&hex(c));
    o.redraw();
    on_pick(c);
}

/// Pixels per logical pixel, for crisp gradients on scaled screens.
fn scale() -> i32 {
    (fltk::app::screen_scale(0).max(1.0).ceil()) as i32
}

fn gradient(w: i32, h: i32, f: impl Fn(f64, f64) -> (u8, u8, u8)) -> Option<RgbImage> {
    let k = scale();
    let (pw, ph) = (w * k, h * k);
    let mut buf = Vec::with_capacity((pw * ph * 3) as usize);
    for j in 0..ph {
        for i in 0..pw {
            let (r, g, b) = f(i as f64 / (pw - 1) as f64, j as f64 / (ph - 1) as f64);
            buf.extend_from_slice(&[r, g, b]);
        }
    }
    let mut img = RgbImage::new(&buf, pw, ph, ColorDepth::Rgb8).ok()?;
    img.scale(w, h, false, true);
    Some(img)
}

fn paint(st: &mut Picker) {
    let t = crate::theme::current();
    let (px, py) = (st.px, st.py);
    // Shadow, border, panel.
    draw::set_draw_color(mix(t.background, Color::Black, 0.35));
    draw::draw_rounded_rectf(px - 1, py + 1, PANEL_W + 2, PANEL_H + 3, t.radius + 2);
    draw::set_draw_color(t.border);
    draw::draw_rounded_rectf(px, py, PANEL_W, PANEL_H, t.radius);
    draw::set_draw_color(t.surface);
    draw::draw_rounded_rectf(px + 1, py + 1, PANEL_W - 2, PANEL_H - 2, t.radius);

    let hue = st.hsv.0;
    if st.sv.as_ref().map(|(h, _)| *h) != Some(hue) {
        st.sv = gradient(AREA_W, SV_H, |s, v| hsv_to_rgb(hue, s, 1.0 - v)).map(|i| (hue, i));
    }
    if st.hue.is_none() {
        st.hue = gradient(AREA_W, HUE_H, |h, _| hsv_to_rgb(h * 360.0, 1.0, 1.0));
    }
    let (sx, sy, sw, sh) = st.sv_rect();
    if let Some((_, img)) = st.sv.as_mut() {
        img.draw(sx, sy, sw, sh);
    }
    let (hx, hy, hw, hh) = st.hue_rect();
    if let Some(img) = st.hue.as_mut() {
        img.draw(hx, hy, hw, hh);
    }
    // Markers: a ring at the picked saturation/brightness, a bar on the hue.
    let (_, s, v) = st.hsv;
    let (mx, my) = (sx + (s * (sw - 1) as f64) as i32, sy + ((1.0 - v) * (sh - 1) as f64) as i32);
    draw::set_line_style(draw::LineStyle::Solid, 2);
    draw::set_draw_color(Color::Black);
    draw::draw_arc(mx - 7, my - 7, 14, 14, 0.0, 360.0);
    draw::set_draw_color(Color::White);
    draw::draw_arc(mx - 6, my - 6, 12, 12, 0.0, 360.0);
    let bx = hx + (hue / 360.0 * (hw - 1) as f64) as i32;
    draw::set_draw_color(Color::Black);
    draw::draw_rect(bx - 3, hy - 3, 7, hh + 6);
    draw::set_draw_color(Color::White);
    draw::draw_rect(bx - 2, hy - 2, 5, hh + 4);
    draw::set_line_style(draw::LineStyle::Solid, 0);
    // The picked color.
    let ry = st.row_y();
    draw::set_draw_color(t.border);
    draw::draw_rounded_rectf(px + PAD, ry, ROW_H, ROW_H, 7);
    draw::set_draw_color(st.color());
    draw::draw_rounded_rectf(px + PAD + 1, ry + 1, ROW_H - 2, ROW_H - 2, 6);
}

fn hex(c: Color) -> String {
    let (r, g, b) = c.to_rgb();
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// "#rgb" or "#rrggbb" (the # is optional).
pub fn parse_hex(s: &str) -> Option<Color> {
    let h = s.trim();
    let h = h.strip_prefix('#').unwrap_or(h);
    if !h.is_ascii() {
        return None;
    }
    let v = |i: usize, n: usize| u8::from_str_radix(&h[i..i + n], 16).ok();
    match h.len() {
        3 => Some(Color::from_rgb(v(0, 1)? * 17, v(1, 1)? * 17, v(2, 1)? * 17)),
        6 => Some(Color::from_rgb(v(0, 2)?, v(2, 2)?, v(4, 2)?)),
        _ => None,
    }
}

/// h in 0..360, s and v in 0..=1.
pub fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (u8, u8, u8) {
    let c = v * s;
    let hp = (h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    let u = |f: f64| ((f + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    (u(r), u(g), u(b))
}

pub fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f64, f64, f64) {
    let (r, g, b) = (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let s = if max == 0.0 { 0.0 } else { d / max };
    (h, s, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsv_round_trips() {
        for &(r, g, b) in &[(0, 0, 0), (255, 255, 255), (180, 108, 255), (62, 201, 154), (229, 72, 77), (1, 2, 3)] {
            let (h, s, v) = rgb_to_hsv(r, g, b);
            assert_eq!(hsv_to_rgb(h, s, v), (r, g, b));
        }
        assert_eq!(parse_hex("#b46cff").map(|c| c.to_rgb()), Some((0xb4, 0x6c, 0xff)));
        assert_eq!(parse_hex("fff").map(|c| c.to_rgb()), Some((255, 255, 255)));
        assert!(parse_hex("#12345").is_none() && parse_hex("#ééé").is_none());
    }
}
