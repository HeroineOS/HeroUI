//! Built-in elements. All take zero size and are laid out by `row`/`column`.
//! Interactive ones are custom-drawn from the theme so they look the same
//! (and modern) regardless of FLTK's scheme.
//!
//! Cost rules every widget here follows (see `examples/stress.rs`):
//! - No Rust `handle` closure per widget: clickable things are
//!   `Fl_Button`s (FLTK handles press/release natively) and hover comes
//!   from `crate::hover`.
//! - Draw closures share the theme through an `Rc`, not a copy.
//! - Bindings compare before touching a widget.

use std::cell::{Cell, RefCell};
use std::ops::RangeInclusive;
use std::rc::Rc;

use fltk::button::Button;
use fltk::draw;
use fltk::draw::LineStyle;
use fltk::enums::{Align, CallbackTrigger, Color, FrameType};
use fltk::frame::Frame;
use fltk::group::Flex;
use fltk::input::Input;
use fltk::prelude::*;
use fltk::valuator::HorSlider;

use crate::element::{relayout_parent, Ctx, Element};
use crate::hover::is_hovered;
use crate::theme::ROUNDED;

fn text_frame<S: 'static, M: 'static>(ctx: &Ctx<S, M>, size_delta: i32, dim: bool) -> Frame {
    let t = ctx.theme();
    let mut f = Frame::default();
    f.set_frame(FrameType::NoBox);
    f.set_align(Align::Left | Align::Inside | Align::Clip);
    f.set_label_font(t.font());
    f.set_label_size(t.font_size + size_delta);
    f.set_label_color(if dim { t.text_dim } else { t.text });
    f
}

/// Static text.
pub fn label<S: 'static, M: 'static>(text: &str) -> Element<S, M> {
    let text = text.to_string();
    Element::new(move |ctx| {
        let mut f = text_frame(ctx, 0, false);
        f.set_label(&text);
        f.as_base_widget()
    })
}

/// Static larger, bold text.
pub fn heading<S: 'static, M: 'static>(text: &str) -> Element<S, M> {
    let text = text.to_string();
    Element::new(move |ctx| {
        let mut f = text_frame(ctx, 6, false);
        f.set_label_font(ctx.theme().bold_font());
        f.set_label(&text);
        f.as_base_widget()
    })
}

/// Static secondary text.
pub fn caption<S: 'static, M: 'static>(text: &str) -> Element<S, M> {
    let text = text.to_string();
    Element::new(move |ctx| {
        let mut f = text_frame(ctx, -2, true);
        f.set_label(&text);
        f.as_base_widget()
    })
}

/// Text computed from state; the widget is only touched when it changes.
pub fn text<S: 'static, M: 'static>(f: impl Fn(&S) -> String + 'static) -> Element<S, M> {
    Element::new(move |ctx| {
        let frame = text_frame(ctx, 0, false);
        let mut w = frame.clone();
        let mut last = None;
        ctx.bind(move |s| {
            let v = f(s);
            if last.as_ref() != Some(&v) {
                // Also schedules the redraw, background included.
                w.set_label(&v);
                last = Some(v);
            }
        });
        frame.as_base_widget()
    })
}

/// Empty stretchable space; use `.fixed(px)` for a fixed gap.
pub fn spacer<S: 'static, M: 'static>() -> Element<S, M> {
    Element::new(|_| Frame::default().as_base_widget())
}

fn flex<S: 'static, M: 'static>(
    column: bool,
    children: Vec<Element<S, M>>,
) -> Element<S, M> {
    Element::new(move |ctx| {
        let mut flex = Flex::default();
        flex.end();
        flex.set_type(if column { fltk::group::FlexType::Column } else { fltk::group::FlexType::Row });
        flex.set_margin(0);
        flex.set_pad(ctx.theme().spacing);
        ctx.build_children(&mut flex, children);
        flex.as_base_widget()
    })
}

/// Children stacked top to bottom.
pub fn column<S: 'static, M: 'static>(children: Vec<Element<S, M>>) -> Element<S, M> {
    flex(true, children)
}

/// Children side by side, left to right.
pub fn row<S: 'static, M: 'static>(children: Vec<Element<S, M>>) -> Element<S, M> {
    flex(false, children)
}

/// A rounded surface panel holding a column of children.
pub fn card<S: 'static, M: 'static>(children: Vec<Element<S, M>>) -> Element<S, M> {
    Element::new(move |ctx| {
        let t = ctx.theme();
        let mut flex = Flex::default().column();
        flex.end();
        flex.set_frame(ROUNDED);
        flex.set_color(t.surface);
        flex.set_margin(t.padding);
        flex.set_pad(t.spacing);
        ctx.build_children(&mut flex, children);
        flex.as_base_widget()
    })
}

/// A column whose items are rebuilt when `count` changes. `item(i)` builds
/// the element for index `i`; its bindings read `state.items.get(i)` etc.
/// Give items a `.fixed(px)` height or they share the list's space.
pub fn list<S: 'static, M: 'static>(
    count: impl Fn(&S) -> usize + 'static,
    item: impl Fn(usize) -> Element<S, M> + 'static,
) -> Element<S, M> {
    Element::new(move |ctx| {
        let mut flex = Flex::default().column();
        flex.set_margin(0);
        flex.set_pad(ctx.theme().spacing);
        flex.end();
        let child_ctx = ctx.child();
        let mut state = ListState { flex: flex.clone(), len: None, bindings: Vec::new(), ctx: Some(child_ctx) };
        ctx.bind(move |s| {
            let n = count(s);
            if state.len != Some(n) {
                state.rebuild(n, &item);
            }
            for b in state.bindings.iter_mut() {
                b(s);
            }
        });
        flex.as_base_widget()
    })
}

struct ListState<S, M> {
    flex: Flex,
    len: Option<usize>,
    bindings: Vec<crate::element::Binding<S>>,
    /// Template context (emitter + theme) cloned for each rebuild.
    ctx: Option<Ctx<S, M>>,
}

impl<S: 'static, M: 'static> ListState<S, M> {
    fn rebuild(&mut self, n: usize, item: &dyn Fn(usize) -> Element<S, M>) {
        let template = self.ctx.take().expect("list context");
        let mut ctx = template.child();
        self.flex.clear();
        // The trailing spacer keeps items packed at the top instead of
        // stretched over the list.
        let children = (0..n).map(item).chain(std::iter::once(spacer())).collect();
        ctx.build_children(&mut self.flex, children);
        self.bindings = ctx.into_bindings();
        self.ctx = Some(template);
        self.len = Some(n);
        self.flex.recalc();
        relayout_parent(&self.flex);
        self.flex.redraw();
    }
}

/// Redraws a transparent (`FrameType::NoBox`) custom widget together with
/// the background behind it. Use it when a redraw can paint *less* than
/// before (a knob that moved, a hover ring that went away); a plain
/// `redraw()` only paints over the old pixels. Only works on `NoBox`
/// widgets, which everything built with [`custom_button`] is.
pub fn repaint<W: WidgetExt>(w: &mut W) {
    // For NoBox widgets FLTK damages the window area under the widget,
    // so the background is redrawn in the same (clipped) pass.
    w.redraw_label();
}

/// Blends `a` toward `b` by `t` (0.0 = a, 1.0 = b).
pub fn mix(a: Color, b: Color, t: f32) -> Color {
    let (ar, ag, ab) = a.to_rgb();
    let (br, bg, bb) = b.to_rgb();
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color::from_rgb(l(ar, br), l(ag, bg), l(ab, bb))
}

/// An `Fl_Button` drawn entirely by `draw`, with no stock look to fight.
/// FLTK does press/release (release outside cancels) and fires the
/// callback; [`crate::hover`] redraws it on enter/leave. This is the cheap
/// way to make any clickable custom widget: set a callback, don't add a
/// `handle` closure. In `draw`, use `b.value()` (pressed),
/// [`is_hovered`](crate::hover::is_hovered) and `b.active_r()`.
pub fn custom_button(draw: impl FnMut(&mut Button) + 'static) -> Button {
    let mut b = Button::default();
    b.set_frame(FrameType::NoBox);
    b.super_draw(false);
    b.clear_visible_focus();
    b.draw(draw);
    b
}

fn make_button<S: 'static, M: Clone + 'static>(
    label: &str,
    msg: M,
    primary: bool,
) -> Element<S, M> {
    let label = label.to_string();
    Element::new(move |ctx| {
        let t = ctx.theme_rc();
        let mut b = custom_button(move |b| {
            let (bg, fg) = if primary { (t.accent, t.accent_text) } else { (t.surface_alt, t.text) };
            let bg = if !b.active_r() {
                mix(bg, t.background, 0.6)
            } else if b.value() {
                mix(bg, t.background, 0.25)
            } else if is_hovered(b) {
                mix(bg, Color::White, 0.1)
            } else {
                bg
            };
            let fg = if b.active_r() { fg } else { mix(fg, t.background, 0.5) };
            draw::set_draw_color(bg);
            draw::draw_rounded_rectf(b.x(), b.y(), b.w(), b.h(), t.radius.min(b.h() / 2));
            draw::set_draw_color(fg);
            draw::set_font(t.font(), t.font_size);
            draw::draw_text2(&label, b.x(), b.y(), b.w(), b.h(), Align::Center);
        });
        let emit = ctx.emitter();
        b.set_callback(move |_| emit(msg.clone()));
        b.as_base_widget()
    })
}

/// A button that sends `msg` when clicked.
pub fn button<S: 'static, M: Clone + 'static>(label: &str, msg: M) -> Element<S, M> {
    make_button(label, msg, false)
}

/// An accent-colored button for the main action.
pub fn primary_button<S: 'static, M: Clone + 'static>(label: &str, msg: M) -> Element<S, M> {
    make_button(label, msg, true)
}

/// Shared by toggle and checkbox: a custom button showing a bool from state
/// and sending `on_toggle(!current)` when clicked.
fn bool_button<S: 'static, M: 'static>(
    value: impl Fn(&S) -> bool + 'static,
    on_toggle: impl Fn(bool) -> M + 'static,
    ctx: &mut Ctx<S, M>,
    draw: impl Fn(&mut Button, bool) + 'static,
) -> Button {
    let on = Rc::new(Cell::new(false));
    let mut b = custom_button({
        let on = on.clone();
        move |b| draw(b, on.get())
    });
    let emit = ctx.emitter();
    {
        let on = on.clone();
        b.set_callback(move |_| emit(on_toggle(!on.get())));
    }
    let mut w = b.clone();
    ctx.bind(move |s| {
        let v = value(s);
        if on.get() != v {
            on.set(v);
            w.redraw();
        }
    });
    b
}

/// An on/off switch with a label to its left.
pub fn toggle<S: 'static, M: 'static>(
    label: &str,
    value: impl Fn(&S) -> bool + 'static,
    on_toggle: impl Fn(bool) -> M + 'static,
) -> Element<S, M> {
    let label = label.to_string();
    Element::new(move |ctx| {
        let t = ctx.theme_rc();
        let b = bool_button(value, on_toggle, ctx, move |b, on| {
            draw::set_font(t.font(), t.font_size);
            draw::set_draw_color(if b.active_r() { t.text } else { t.text_dim });
            draw::draw_text2(&label, b.x(), b.y(), b.w() - 48, b.h(), Align::Left);
            let (tw, th) = (40, 22);
            let (tx, ty) = (b.x() + b.w() - tw, b.y() + (b.h() - th) / 2);
            let track = if on { t.accent } else { t.surface_alt };
            let track = if is_hovered(b) { mix(track, Color::White, 0.1) } else { track };
            let track = if b.active_r() { track } else { mix(track, t.background, 0.6) };
            draw::set_draw_color(track);
            draw::draw_rounded_rectf(tx, ty, tw, th, th / 2);
            let knob = th - 6;
            let kx = if on { tx + tw - knob - 3 } else { tx + 3 };
            draw::set_draw_color(if on { t.accent_text } else { t.text_dim });
            draw::draw_pie(kx, ty + 3, knob, knob, 0.0, 360.0);
        });
        b.as_base_widget()
    })
}

/// A checkbox with a label to its right.
pub fn checkbox<S: 'static, M: 'static>(
    label: &str,
    value: impl Fn(&S) -> bool + 'static,
    on_toggle: impl Fn(bool) -> M + 'static,
) -> Element<S, M> {
    let label = label.to_string();
    Element::new(move |ctx| {
        let t = ctx.theme_rc();
        let b = bool_button(value, on_toggle, ctx, move |b, on| {
            let s = 18;
            let (x, y) = (b.x(), b.y() + (b.h() - s) / 2);
            let bg = if on { t.accent } else { t.surface_alt };
            let bg = if is_hovered(b) { mix(bg, Color::White, 0.1) } else { bg };
            let bg = if b.active_r() { bg } else { mix(bg, t.background, 0.6) };
            draw::set_draw_color(bg);
            draw::draw_rounded_rectf(x, y, s, s, (t.radius / 2).min(5));
            if on {
                draw::set_draw_color(t.accent_text);
                draw::set_line_style(LineStyle::Solid | LineStyle::CapRound | LineStyle::JoinRound, 2);
                draw::draw_line(x + 4, y + 9, x + 8, y + 13);
                draw::draw_line(x + 8, y + 13, x + 14, y + 5);
                draw::set_line_style(LineStyle::Solid, 0);
            }
            draw::set_font(t.font(), t.font_size);
            draw::set_draw_color(if b.active_r() { t.text } else { t.text_dim });
            draw::draw_text2(&label, x + s + 8, b.y(), b.w() - s - 8, b.h(), Align::Left);
        });
        b.as_base_widget()
    })
}

/// A dropdown: shows `options(state)[selected(state)]`; picking another
/// option sends `on_select(index)`. Options can be a static list
/// (`|_| SIZES` with `const SIZES: &[&str]`) or come from state
/// (`|s| &s.devices`). The popup window only exists while it is open.
pub fn dropdown<S: 'static, M: 'static, T: AsRef<str> + 'static>(
    options: impl Fn(&S) -> &[T] + 'static,
    selected: impl Fn(&S) -> usize + 'static,
    on_select: impl Fn(usize) -> M + 'static,
) -> Element<S, M> {
    Element::new(move |ctx| {
        let t = ctx.theme_rc();
        // Copies of the state's options and selection, updated by the
        // binding only when they change.
        let opts: Rc<RefCell<Vec<String>>> = Rc::default();
        let sel = Rc::new(Cell::new(usize::MAX));
        let mut b = custom_button({
            let (t, opts, sel) = (t.clone(), opts.clone(), sel.clone());
            move |b| {
                let bg = if is_hovered(b) || b.value() { mix(t.surface_alt, Color::White, 0.1) } else { t.surface_alt };
                let bg = if b.active_r() { bg } else { mix(bg, t.background, 0.6) };
                draw::set_draw_color(bg);
                draw::draw_rounded_rectf(b.x(), b.y(), b.w(), b.h(), t.radius.min(b.h() / 2));
                let fg = if b.active_r() { t.text } else { t.text_dim };
                draw::set_draw_color(fg);
                draw::set_font(t.font(), t.font_size);
                if let Some(text) = opts.borrow().get(sel.get()) {
                    draw::draw_text2(text, b.x() + 12, b.y(), b.w() - 40, b.h(), Align::Left | Align::Clip);
                }
                // Chevron.
                let (cx, cy) = (b.x() + b.w() - 18, b.y() + b.h() / 2);
                draw::set_draw_color(t.text_dim);
                draw::set_line_style(LineStyle::Solid | LineStyle::CapRound | LineStyle::JoinRound, 2);
                draw::draw_line(cx - 4, cy - 2, cx, cy + 2);
                draw::draw_line(cx, cy + 2, cx + 4, cy - 2);
                draw::set_line_style(LineStyle::Solid, 0);
            }
        });
        let emit = ctx.emitter();
        let on_select = Rc::new(on_select);
        {
            let (t, opts, sel) = (t.clone(), opts.clone(), sel.clone());
            b.set_callback(move |b| {
                let (emit, on_select) = (emit.clone(), on_select.clone());
                let current = Some(sel.get()).filter(|&i| i < opts.borrow().len());
                crate::popup::open(b, opts.borrow().clone(), current, t.clone(), move |i| emit(on_select(i)));
            });
        }
        let mut w = b.clone();
        ctx.bind(move |s| {
            let new = options(s);
            let mut changed = false;
            {
                let mut cur = opts.borrow_mut();
                if cur.len() != new.len() || cur.iter().zip(new).any(|(a, b)| a != b.as_ref()) {
                    *cur = new.iter().map(|o| o.as_ref().to_owned()).collect();
                    changed = true;
                }
            }
            let i = selected(s);
            if sel.replace(i) != i || changed {
                w.redraw();
            }
        });
        b.as_base_widget()
    })
}

/// A single-line text field showing `value(state)`; every edit sends
/// `on_change(new_text)`. The field is only overwritten when the state
/// differs from what's typed, so the cursor doesn't jump.
pub fn text_input<S: 'static, M: 'static>(
    value: impl Fn(&S) -> String + 'static,
    on_change: impl Fn(String) -> M + 'static,
) -> Element<S, M> {
    Element::new(move |ctx| {
        let t = ctx.theme_rc();
        let mut input = Input::default();
        input.set_frame(ROUNDED);
        input.set_color(t.surface_alt);
        input.set_text_color(t.text);
        input.set_text_font(t.font());
        input.set_text_size(t.font_size);
        input.set_cursor_color(t.accent);
        input.set_selection_color(t.accent);
        input.set_trigger(CallbackTrigger::Changed);
        let emit = ctx.emitter();
        input.set_callback(move |i| emit(on_change(i.value())));
        let mut w = input.clone();
        ctx.bind(move |s| {
            let v = value(s);
            if w.value() != v {
                w.set_value(&v);
            }
        });
        input.as_base_widget()
    })
}

/// A horizontal slider over `range`, sending `on_change(value)` while dragged.
pub fn slider<S: 'static, M: 'static>(
    range: RangeInclusive<f64>,
    value: impl Fn(&S) -> f64 + 'static,
    on_change: impl Fn(f64) -> M + 'static,
) -> Element<S, M> {
    Element::new(move |ctx| {
        let t = ctx.theme_rc();
        let mut s = HorSlider::default();
        s.set_bounds(*range.start(), *range.end());
        // Draw it all ourselves. (A NoBox knob isn't enough: FLTK falls
        // back to an up-box knob when both box types are NoBox.)
        s.set_frame(FrameType::NoBox);
        s.super_draw(false);
        s.draw(move |s| {
            let span = (s.maximum() - s.minimum()).max(f64::EPSILON);
            let frac = ((s.value() - s.minimum()) / span).clamp(0.0, 1.0);
            let knob = 16.min(s.h());
            let (x, w) = (s.x() + knob / 2, s.w() - knob);
            let cy = s.y() + s.h() / 2;
            draw::set_draw_color(t.surface_alt);
            draw::draw_rounded_rectf(x, cy - 3, w, 6, 3);
            let filled = (w as f64 * frac) as i32;
            let accent = if s.active_r() { t.accent } else { mix(t.accent, t.background, 0.6) };
            draw::set_draw_color(accent);
            draw::draw_rounded_rectf(x, cy - 3, filled.max(6), 6, 3);
            draw::draw_pie(x + filled - knob / 2, cy - knob / 2, knob, knob, 0.0, 360.0);
        });
        let emit = ctx.emitter();
        s.set_callback(move |s| {
            // Dragging moves the knob without going through the binding.
            repaint(s);
            emit(on_change(s.value()))
        });
        let mut w = s.clone();
        ctx.bind(move |st| {
            let v = value(st);
            if (w.value() - v).abs() > f64::EPSILON {
                w.set_value(v);
                // The knob moved: clear where it was.
                repaint(&mut w);
            }
        });
        s.as_base_widget()
    })
}

/// A read-only bar showing `value(state)` in 0.0..=1.0.
pub fn progress<S: 'static, M: 'static>(value: impl Fn(&S) -> f64 + 'static) -> Element<S, M> {
    Element::new(move |ctx| {
        let t = ctx.theme_rc();
        let frac = Rc::new(Cell::new(0.0f64));
        let mut f = Frame::default();
        {
            let frac = frac.clone();
            f.draw(move |f| {
                let h = 6.min(f.h());
                let y = f.y() + (f.h() - h) / 2;
                draw::set_draw_color(t.surface_alt);
                draw::draw_rounded_rectf(f.x(), y, f.w(), h, h / 2);
                let w = (f.w() as f64 * frac.get()) as i32;
                if w > 0 {
                    draw::set_draw_color(t.accent);
                    draw::draw_rounded_rectf(f.x(), y, w.max(h), h, h / 2);
                }
            });
        }
        let mut w = f.clone();
        ctx.bind(move |s| {
            let v = value(s).clamp(0.0, 1.0);
            if (frac.get() - v).abs() > 0.001 {
                frac.set(v);
                w.redraw();
            }
        });
        f.as_base_widget()
    })
}
