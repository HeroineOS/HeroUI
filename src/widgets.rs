//! Built-in elements. All take zero size and are laid out by `row`/`column`.
//! Interactive ones are custom-drawn from the theme so they look the same
//! (and modern) regardless of FLTK's scheme.

use std::cell::Cell;
use std::ops::RangeInclusive;
use std::rc::Rc;

use fltk::button::Button;
use fltk::draw;
use fltk::enums::{Align, CallbackTrigger, Color, Cursor, Event, FrameType};
use fltk::frame::Frame;
use fltk::group::Flex;
use fltk::input::Input;
use fltk::prelude::*;
use fltk::valuator::HorSlider;

use crate::element::{relayout_parent, Ctx, Element};
use crate::theme::{Theme, ROUNDED};

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
                w.set_label(&v);
                w.redraw();
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
        flex.set_type(if column { fltk::group::FlexType::Column } else { fltk::group::FlexType::Row });
        flex.set_margin(0);
        flex.set_pad(ctx.theme().spacing);
        for child in children {
            let fixed = child.fixed_size();
            let w = child.build(ctx);
            if let Some(px) = fixed {
                flex.fixed(&w, px);
            }
        }
        flex.end();
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
        flex.set_frame(ROUNDED);
        flex.set_color(t.surface);
        flex.set_margin(t.padding);
        flex.set_pad(t.spacing);
        for child in children {
            let fixed = child.fixed_size();
            let w = child.build(ctx);
            if let Some(px) = fixed {
                flex.fixed(&w, px);
            }
        }
        flex.end();
        flex.as_base_widget()
    })
}

/// A column whose items are rebuilt when `count` changes. `item(i)` builds
/// the element for index `i`; its bindings read `state.items[i]` etc.
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
        self.flex.begin();
        for i in 0..n {
            let el = item(i);
            let fixed = el.fixed_size();
            let w = el.build(&mut ctx);
            if let Some(px) = fixed {
                self.flex.fixed(&w, px);
            }
        }
        // Keep items packed at the top instead of stretched over the list.
        Frame::default();
        self.flex.end();
        self.bindings = ctx.into_bindings();
        self.ctx = Some(template);
        self.len = Some(n);
        self.flex.recalc();
        relayout_parent(&self.flex);
        self.flex.redraw();
    }
}

fn set_cursor<W: WidgetExt>(w: &W, cursor: Cursor) {
    if let Some(mut win) = w.window() {
        win.set_cursor(cursor);
    }
}

fn hover_tracking<W: WidgetExt + WidgetBase>(w: &mut W, hover: Rc<Cell<bool>>) {
    w.handle(move |w, ev| match ev {
        Event::Enter => {
            hover.set(true);
            if w.active() {
                set_cursor(w, Cursor::Hand);
            }
            w.redraw();
            true
        }
        Event::Leave => {
            hover.set(false);
            set_cursor(w, Cursor::Default);
            w.redraw();
            true
        }
        _ => false,
    });
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    let (ar, ag, ab) = a.to_rgb();
    let (br, bg, bb) = b.to_rgb();
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color::from_rgb(l(ar, br), l(ag, bg), l(ab, bb))
}

fn make_button<S: 'static, M: Clone + 'static>(
    label: &str,
    msg: M,
    primary: bool,
) -> Element<S, M> {
    let label = label.to_string();
    Element::new(move |ctx| {
        let t: Theme = ctx.theme().clone();
        let mut b = Button::default().with_label(&label);
        b.set_frame(FrameType::NoBox);
        b.set_down_frame(FrameType::NoBox);
        let hover = Rc::new(Cell::new(false));
        hover_tracking(&mut b, hover.clone());
        b.draw(move |b| {
            let (bg, fg) = if primary { (t.accent, t.accent_text) } else { (t.surface_alt, t.text) };
            let bg = if !b.active_r() {
                mix(bg, t.background, 0.6)
            } else if b.value() {
                mix(bg, t.background, 0.25)
            } else if hover.get() {
                mix(bg, Color::White, 0.1)
            } else {
                bg
            };
            let fg = if b.active_r() { fg } else { mix(fg, t.background, 0.5) };
            let r = t.radius.min(b.h() / 2);
            draw::set_draw_color(bg);
            draw::draw_rounded_rectf(b.x(), b.y(), b.w(), b.h(), r);
            draw::set_draw_color(fg);
            draw::set_font(t.font(), t.font_size);
            draw::draw_text2(&b.label(), b.x(), b.y(), b.w(), b.h(), Align::Center);
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

/// A single-line text field showing `value(state)`; every edit sends
/// `on_change(new_text)`. The field is only overwritten when the state
/// differs from what's typed, so the cursor doesn't jump.
pub fn text_input<S: 'static, M: 'static>(
    value: impl Fn(&S) -> String + 'static,
    on_change: impl Fn(String) -> M + 'static,
) -> Element<S, M> {
    Element::new(move |ctx| {
        let t = ctx.theme().clone();
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

/// An on/off switch with a label to its left.
pub fn toggle<S: 'static, M: 'static>(
    label: &str,
    value: impl Fn(&S) -> bool + 'static,
    on_toggle: impl Fn(bool) -> M + 'static,
) -> Element<S, M> {
    let label = label.to_string();
    Element::new(move |ctx| {
        let t = ctx.theme().clone();
        let on = Rc::new(Cell::new(false));
        // The label is drawn by us, not set on the widget: fltk-rs runs a
        // custom draw *after* the widget's own, which would draw it twice.
        let mut f = Frame::default();
        let hover = Rc::new(Cell::new(false));
        {
            let on = on.clone();
            let hover = hover.clone();
            f.draw(move |f| {
                draw::set_font(t.font(), t.font_size);
                draw::set_draw_color(if f.active_r() { t.text } else { t.text_dim });
                draw::draw_text2(&label, f.x(), f.y(), f.w() - 48, f.h(), Align::Left);
                let (tw, th) = (40, 22);
                let (tx, ty) = (f.x() + f.w() - tw, f.y() + (f.h() - th) / 2);
                let track = if on.get() { t.accent } else { t.surface_alt };
                let track = if hover.get() { mix(track, Color::White, 0.1) } else { track };
                draw::set_draw_color(track);
                draw::draw_rounded_rectf(tx, ty, tw, th, th / 2);
                let knob = th - 6;
                let kx = if on.get() { tx + tw - knob - 3 } else { tx + 3 };
                draw::set_draw_color(if on.get() { t.accent_text } else { t.text_dim });
                draw::draw_pie(kx, ty + 3, knob, knob, 0.0, 360.0);
            });
        }
        let emit = ctx.emitter();
        {
            let on = on.clone();
            let hover2 = hover.clone();
            f.handle(move |w, ev| match ev {
                Event::Push => true,
                Event::Released if w.active() => {
                    emit(on_toggle(!on.get()));
                    true
                }
                Event::Enter | Event::Leave => {
                    hover2.set(ev == Event::Enter);
                    w.redraw();
                    true
                }
                _ => false,
            });
        }
        let mut w = f.clone();
        ctx.bind(move |s| {
            let v = value(s);
            if on.get() != v {
                on.set(v);
                w.redraw();
            }
        });
        f.as_base_widget()
    })
}

/// A horizontal slider over `range`, sending `on_change(value)` while dragged.
pub fn slider<S: 'static, M: 'static>(
    range: RangeInclusive<f64>,
    value: impl Fn(&S) -> f64 + 'static,
    on_change: impl Fn(f64) -> M + 'static,
) -> Element<S, M> {
    Element::new(move |ctx| {
        let t = ctx.theme().clone();
        let mut s = HorSlider::default();
        s.set_bounds(*range.start(), *range.end());
        // Draw it all ourselves. (A NoBox knob isn't enough: FLTK falls
        // back to an up-box knob when both box types are NoBox.)
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
        s.set_callback(move |s| emit(on_change(s.value())));
        let mut w = s.clone();
        ctx.bind(move |st| {
            let v = value(st);
            if (w.value() - v).abs() > f64::EPSILON {
                w.set_value(v);
                w.redraw();
            }
        });
        s.as_base_widget()
    })
}

/// A read-only bar showing `value(state)` in 0.0..=1.0.
pub fn progress<S: 'static, M: 'static>(value: impl Fn(&S) -> f64 + 'static) -> Element<S, M> {
    Element::new(move |ctx| {
        let t = ctx.theme().clone();
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
