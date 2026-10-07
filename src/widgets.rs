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
use fltk::enums::{Align, CallbackTrigger, Color, Event, FrameType};
use fltk::frame::Frame;
use fltk::group::Flex;
use fltk::input::Input;
use fltk::prelude::*;
use fltk::valuator::HorSlider;

pub use crate::color_picker::color_button;
pub use crate::icon_picker::icon_button;
pub use crate::popover::{popover, popover_at, press_button};
pub use crate::popover::set_radius as set_popover_radius;
pub use crate::popover::offset as popover_offset;
pub use crate::popover::dragged_out as popover_dragged_out;
use crate::element::{relayout_parent, Ctx, Element};
use crate::hover::{hover_amount, press_amount};
use crate::theme::{Theme, ROUNDED};

fn text_frame<S: 'static, M: 'static>(ctx: &Ctx<S, M>, size_delta: i32, dim: bool, bold: bool) -> Frame {
    let mut f = Frame::default();
    f.set_frame(FrameType::NoBox);
    f.set_align(Align::Left | Align::Inside | Align::Clip);
    let mut style = {
        let mut f = f.clone();
        move |t: &Theme| {
            f.set_label_font(if bold { t.bold_font() } else { t.font() });
            f.set_label_size(t.font_size + size_delta);
            f.set_label_color(if dim { t.text_dim } else { t.text });
        }
    };
    style(ctx.theme());
    crate::theme::on_change(&f, style);
    f
}

/// Static text.
pub fn label<S: 'static, M: 'static>(text: &str) -> Element<S, M> {
    let text = text.to_string();
    Element::new(move |ctx| {
        let mut f = text_frame(ctx, 0, false, false);
        f.set_label(&text);
        f.as_base_widget()
    })
}

/// Static larger, bold text.
pub fn heading<S: 'static, M: 'static>(text: &str) -> Element<S, M> {
    let text = text.to_string();
    Element::new(move |ctx| {
        let mut f = text_frame(ctx, 6, false, true);
        f.set_label(&text);
        f.as_base_widget()
    })
}

/// Static secondary text.
pub fn caption<S: 'static, M: 'static>(text: &str) -> Element<S, M> {
    let text = text.to_string();
    Element::new(move |ctx| {
        let mut f = text_frame(ctx, -2, true, false);
        f.set_label(&text);
        f.as_base_widget()
    })
}

/// Text computed from state; the widget is only touched when it changes.
pub fn text<S: 'static, M: 'static>(f: impl Fn(&S) -> String + 'static) -> Element<S, M> {
    Element::new(move |ctx| {
        let frame = text_frame(ctx, 0, false, false);
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
        crate::theme::on_change(&flex, {
            let mut f = flex.clone();
            move |t| f.set_color(t.surface)
        });
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
        // Without an explicit size, the list reports its natural height
        // (its items' fixed heights) for containers like `scroll`.
        let natural = ctx.size_hint().filter(|h| h.get() < 0);
        let mut state = ListState { flex: flex.clone(), len: None, bindings: Vec::new(), ctx: Some(child_ctx), natural };
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
    natural: Option<Rc<Cell<i32>>>,
}

impl<S: 'static, M: 'static> ListState<S, M> {
    fn rebuild(&mut self, n: usize, item: &dyn Fn(usize) -> Element<S, M>) {
        let template = self.ctx.take().expect("list context");
        let mut ctx = template.child();
        self.flex.clear();
        // The trailing spacer keeps items packed at the top instead of
        // stretched over the list.
        let children: Vec<Element<S, M>> = (0..n).map(item).collect();
        if let Some(h) = &self.natural {
            let items: i32 = children.iter().map(|c| c.fixed_size().unwrap_or(0)).sum();
            h.set(items + self.flex.pad() * (n as i32 - 1).max(0));
        }
        let children = children.into_iter().chain(std::iter::once(spacer())).collect();
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
/// [`is_hovered`](crate::hover::is_hovered) (or the fading
/// [`hover_amount`](crate::hover::hover_amount)) and `b.active_r()`.
pub fn custom_button(draw: impl FnMut(&mut Button) + 'static) -> Button {
    let mut b = Button::default();
    b.set_frame(FrameType::NoBox);
    b.super_draw(false);
    b.clear_visible_focus();
    b.draw(draw);
    b
}

/// A button's rounded background, squeezed in a little (up to 1.5 px a
/// side, between pixels) by the press amount `p`: the press feedback.
pub fn press_shape(b: &Button, color: Color, radius: i32, p: f32) {
    let i = 1.5 * p as f64;
    crate::fx::fill_rounded(b.x() as f64 + i, b.y() as f64 + i, b.w() as f64 - 2.0 * i, b.h() as f64 - 2.0 * i, radius as f64 - i, color, 1.0);
}

thread_local! {
    /// The keyboard is being used to move around (Tab, arrows, Enter):
    /// the focused button shows a ring. A click hides it again.
    static KEYBOARD: Cell<bool> = const { Cell::new(false) };
}

thread_local! {
    /// The widget to focus once the window has the keyboard.
    static AUTOFOCUS: RefCell<Option<fltk::widget::Widget>> = const { RefCell::new(None) };
}

pub(crate) fn focus_when_ready(w: fltk::widget::Widget) {
    AUTOFOCUS.with(|a| *a.borrow_mut() = Some(w));
}

/// Gives the waiting widget the focus (the window has the keyboard now).
/// FLTK still settles the window's own focus after the focus event, so
/// this runs then and again on the next loop turn; on the first key press
/// it's done for good (`done`). A click first cancels it (see
/// [`cancel_autofocus`]): the user chose.
pub(crate) fn take_autofocus(done: bool) {
    let Some(mut w) = AUTOFOCUS.with(|a| a.borrow().clone()) else { return };
    if w.visible_r() && w.take_focus().is_ok() {
        set_keyboard_focus(true);
        w.redraw();
    }
    if done {
        AUTOFOCUS.with(|a| *a.borrow_mut() = None);
    }
}

pub(crate) fn cancel_autofocus() {
    AUTOFOCUS.with(|a| *a.borrow_mut() = None);
}

pub(crate) fn set_keyboard_focus(on: bool) {
    if KEYBOARD.with(|k| k.replace(on)) != on {
        if let Some(mut w) = fltk::app::focus() {
            w.redraw();
        }
    }
}

/// Whether `w` has the keyboard focus and the keyboard is in use (draw a
/// focus ring then; see [`focus_ring`]).
pub fn keyboard_focused<W: WidgetExt>(w: &W) -> bool {
    KEYBOARD.with(Cell::get) && fltk::app::focus().is_some_and(|f| f.as_widget_ptr() == w.as_widget_ptr())
}

/// A ring inside `w`'s edge when it has the keyboard focus.
pub fn focus_ring<W: WidgetExt>(w: &W, radius: i32) {
    if keyboard_focused(w) {
        let t = crate::theme::current();
        draw::set_draw_color(t.accent);
        draw::set_line_style(fltk::draw::LineStyle::Solid, 2);
        draw::draw_rounded_rect(w.x() + 1, w.y() + 1, w.w() - 2, w.h() - 2, radius);
        draw::set_line_style(fltk::draw::LineStyle::Solid, 0);
    }
}

/// A button the keyboard can reach: Tab and the arrows move focus to it,
/// Enter or Space presses it, and it shows a ring (see [`focus_ring`]).
pub fn focusable_button(draw: impl FnMut(&mut Button) + 'static) -> Button {
    let mut b = custom_button(draw);
    b.set_visible_focus();
    b
}

fn make_button<S: 'static, M: Clone + 'static>(
    label: &str,
    msg: M,
    primary: bool,
) -> Element<S, M> {
    let label = label.to_string();
    Element::new(move |ctx| {
        let mut b = focusable_button(move |b| {
            let t = crate::theme::current();
            let (bg, fg) = if primary { (t.accent, t.accent_text) } else { (t.surface_alt, t.text) };
            let p = press_amount(b);
            let bg = if !b.active_r() {
                mix(bg, t.background, 0.6)
            } else {
                mix(mix(bg, Color::White, 0.1 * hover_amount(b)), t.background, 0.22 * p)
            };
            let fg = if b.active_r() { fg } else { mix(fg, t.background, 0.5) };
            press_shape(b, bg, t.radius.min(b.h() / 2), p);
            draw::set_draw_color(fg);
            draw::set_font(t.font(), t.font_size);
            draw::draw_text2(&label, b.x(), b.y(), b.w(), b.h(), Align::Center);
            // On the accent, the ring in the text color.
            if primary && keyboard_focused(b) {
                draw::set_draw_color(mix(t.accent_text, t.accent, 0.35));
                draw::set_line_style(fltk::draw::LineStyle::Solid, 2);
                draw::draw_rounded_rect(b.x() + 3, b.y() + 3, b.w() - 6, b.h() - 6, (t.radius - 2).clamp(0, b.h() / 2));
                draw::set_line_style(fltk::draw::LineStyle::Solid, 0);
            } else {
                focus_ring(b, t.radius.min(b.h() / 2));
            }
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
    let mut b = focusable_button({
        let on = on.clone();
        move |b| {
            draw(b, on.get());
            focus_ring(b, crate::theme::current().radius.min(b.h() / 2));
        }
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

/// An on/off switch with a label to its left. The knob slides between
/// states (unless the theme turns animations off).
pub fn toggle<S: 'static, M: 'static>(
    label: &str,
    value: impl Fn(&S) -> bool + 'static,
    on_toggle: impl Fn(bool) -> M + 'static,
) -> Element<S, M> {
    let label = label.to_string();
    Element::new(move |ctx| {
        // `on`: the state; `pos`: where the knob is drawn, 0.0 (off) to 1.0.
        let on = Rc::new(Cell::new(false));
        let pos = crate::anim::Tween::new(0.0);
        let mut b = focusable_button({
            let pos = pos.clone();
            move |b| {
                let t = crate::theme::current();
                let p = pos.get();
                draw::set_font(t.font(), t.font_size);
                draw::set_draw_color(if b.active_r() { t.text } else { t.text_dim });
                draw::draw_text2(&label, b.x(), b.y(), b.w() - 48, b.h(), Align::Left);
                let (tw, th) = (40, 22);
                let (tx, ty) = (b.x() + b.w() - tw, b.y() + (b.h() - th) / 2);
                let pc = p.clamp(0.0, 1.0) as f32;
                let track = mix(t.surface_alt, t.accent, pc);
                let track = mix(track, Color::White, 0.1 * hover_amount(b));
                let track = if b.active_r() { track } else { mix(track, t.background, 0.6) };
                crate::fx::fill_rounded(tx as f64, ty as f64, tw as f64, th as f64, th as f64 / 2.0, track, 1.0);
                // The knob springs across; it stretches with its speed and
                // while pressed (like a finger squashing it), between pixels.
                let knob = (th - 6) as f64;
                let travel = (tw - 6) as f64 - knob;
                let stretch = (pos.velocity().abs() * travel * 0.012).min(7.0) + 4.0 * press_amount(b) as f64;
                let kx = tx as f64 + 3.0 + travel * p - stretch * p.clamp(0.0, 1.0);
                crate::fx::fill_rounded(kx, ty as f64 + 3.0, knob + stretch, knob, knob / 2.0, mix(t.text_dim, t.accent_text, pc), 1.0);
                // Focused: a ring around the switch.
                if keyboard_focused(b) {
                    draw::set_draw_color(t.accent);
                    draw::set_line_style(LineStyle::Solid, 2);
                    draw::draw_rounded_rect(tx - 3, ty - 3, tw + 6, th + 6, th / 2 + 3);
                    draw::set_line_style(LineStyle::Solid, 0);
                }
            }
        });
        let emit = ctx.emitter();
        {
            let on = on.clone();
            b.set_callback(move |_| emit(on_toggle(!on.get())));
        }
        let w = b.clone();
        let first = Cell::new(true);
        ctx.bind(move |s| {
            let v = value(s);
            if on.get() == v && !first.get() {
                return;
            }
            on.set(v);
            let to = if v { 1.0 } else { 0.0 };
            let mut w = w.clone();
            if first.replace(false) || !w.visible_r() {
                // Initial state: no animation.
                pos.set(to);
                w.redraw();
                return;
            }
            pos.spring_to(to, crate::anim::Spring::SNAPPY, move || w.redraw());
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
        let b = bool_button(value, on_toggle, ctx, move |b, on| {
            let t = crate::theme::current();
            let s = 18;
            let (x, y) = (b.x(), b.y() + (b.h() - s) / 2);
            let bg = if on { t.accent } else { t.surface_alt };
            let bg = mix(bg, Color::White, 0.1 * hover_amount(b));
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
        // Copies of the state's options and selection, updated by the
        // binding only when they change.
        let opts: Rc<RefCell<Vec<String>>> = Rc::default();
        let sel = Rc::new(Cell::new(usize::MAX));
        let mut b = focusable_button({
            let (opts, sel) = (opts.clone(), sel.clone());
            move |b| {
                let t = crate::theme::current();
                let bg = mix(t.surface_alt, Color::White, if b.value() { 0.1 } else { 0.1 * hover_amount(b) });
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
                focus_ring(b, t.radius.min(b.h() / 2));
            }
        });
        let emit = ctx.emitter();
        let on_select = Rc::new(on_select);
        {
            let (opts, sel) = (opts.clone(), sel.clone());
            // Open on press, like Fl_Choice: a Wayland popup grab must use
            // the serial of a button press (a release is refused).
            b.set_trigger(CallbackTrigger::Changed);
            b.set_callback(move |b| {
                if !b.value() {
                    return;
                }
                let current = Some(sel.get()).filter(|&i| i < opts.borrow().len());
                // Blocks in FLTK's menu loop until a pick or dismissal.
                let picked = crate::popup::pick(b, &opts.borrow(), current, &crate::theme::current());
                // The menu consumed the release; don't stay drawn pressed.
                b.set_value(false);
                if let Some(i) = picked {
                    emit(on_select(i));
                }
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
    input_element(value, on_change, None::<fn() -> M>)
}

/// Like [`text_input`], and pressing Enter sends `on_submit` (add an item,
/// run a search, confirm a form).
pub fn text_input_submit<S: 'static, M: Clone + 'static>(
    value: impl Fn(&S) -> String + 'static,
    on_change: impl Fn(String) -> M + 'static,
    on_submit: M,
) -> Element<S, M> {
    input_element(value, on_change, Some(move || on_submit.clone()))
}

fn input_element<S: 'static, M: 'static>(
    value: impl Fn(&S) -> String + 'static,
    on_change: impl Fn(String) -> M + 'static,
    on_submit: Option<impl Fn() -> M + 'static>,
) -> Element<S, M> {
    Element::new(move |ctx| {
        let mut input = Input::default();
        input.set_frame(ROUNDED);
        let mut style = {
            let mut input = input.clone();
            move |t: &Theme| {
                input.set_color(t.surface_alt);
                input.set_text_color(t.text);
                input.set_text_font(t.font());
                input.set_text_size(t.font_size);
                input.set_cursor_color(t.accent);
                input.set_selection_color(t.accent);
            }
        };
        style(ctx.theme());
        crate::theme::on_change(&input, style);
        let emit = ctx.emitter();
        match on_submit {
            None => {
                input.set_trigger(CallbackTrigger::Changed);
                input.set_callback(move |i| emit(on_change(i.value())));
            }
            Some(submit) => {
                // Called for edits and for Enter (changed or not).
                input.set_trigger(CallbackTrigger::EnterKeyChanged);
                input.set_callback(move |i| {
                    let enter = fltk::app::event() == fltk::enums::Event::KeyDown
                        && matches!(fltk::app::event_key(), fltk::enums::Key::Enter | fltk::enums::Key::KPEnter);
                    if enter {
                        emit(submit());
                    } else {
                        emit(on_change(i.value()));
                    }
                });
            }
        }
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
        let mut s = HorSlider::default();
        s.set_bounds(*range.start(), *range.end());
        // Draw it all ourselves. (A NoBox knob isn't enough: FLTK falls
        // back to an up-box knob when both box types are NoBox.)
        s.set_frame(FrameType::NoBox);
        s.super_draw(false);
        s.draw(move |s| {
            let t = crate::theme::current();
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
            // Focused (the arrows move it): a ring around the knob.
            if keyboard_focused(s) {
                draw::set_draw_color(t.accent);
                draw::set_line_style(LineStyle::Solid, 2);
                draw::draw_arc(x + filled - knob / 2 - 3, cy - knob / 2 - 3, knob + 6, knob + 6, 0.0, 360.0);
                draw::set_line_style(LineStyle::Solid, 0);
            }
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
        // The drawn fill glides to each new value (instantly with
        // animations off), so coarse updates still look smooth.
        let frac = crate::anim::Tween::new(0.0);
        let mut f = Frame::default();
        f.set_frame(FrameType::NoBox);
        {
            let frac = frac.clone();
            f.draw(move |f| {
                let t = crate::theme::current();
                let h = 6.min(f.h());
                let y = f.y() + (f.h() - h) / 2;
                draw::set_draw_color(t.surface_alt);
                draw::draw_rounded_rectf(f.x(), y, f.w(), h, h / 2);
                // Between pixels, so slow progress creeps instead of stepping.
                let w = f.w() as f64 * frac.get().clamp(0.0, 1.0);
                if w > 0.0 {
                    crate::fx::fill_rounded(f.x() as f64, y as f64, w.max(h as f64), h as f64, h as f64 / 2.0, t.accent, 1.0);
                }
            });
        }
        let w = f.clone();
        let target = Cell::new(f64::NAN);
        ctx.bind(move |s| {
            let v = value(s).clamp(0.0, 1.0);
            if (target.get() - v).abs() <= 0.001 {
                return;
            }
            let first = target.get().is_nan();
            target.set(v);
            let mut w = w.clone();
            if first || !w.visible_r() {
                frac.set(v);
                repaint(&mut w);
            } else {
                // A spring: regular updates join into one flowing motion
                // (it keeps its speed). Shrinking needs the background
                // repainted, so repaint().
                frac.spring_to(v, crate::anim::Spring::SMOOTH, move || repaint(&mut w));
            }
        });
        f.as_base_widget()
    })
}

/// Custom drawing driven by state, for meters, charts, clocks.
/// `data(state)` picks what the drawing depends on; only when it changes
/// is the canvas redrawn, by `paint(&data, x, y, w, h, &theme)` using
/// `fltk::draw`. Keep `D` small (numbers, a short Vec): it's computed and
/// compared after every update.
pub fn canvas<S: 'static, M: 'static, D: PartialEq + 'static>(
    data: impl Fn(&S) -> D + 'static,
    paint: impl Fn(&D, i32, i32, i32, i32, &Theme) + 'static,
) -> Element<S, M> {
    Element::new(move |ctx| {
        let cur: Rc<RefCell<Option<D>>> = Rc::default();
        let mut f = Frame::default();
        f.set_frame(FrameType::NoBox);
        {
            let cur = cur.clone();
            f.draw(move |f| {
                if let Some(d) = cur.borrow().as_ref() {
                    draw::push_clip(f.x(), f.y(), f.w(), f.h());
                    paint(d, f.x(), f.y(), f.w(), f.h(), &crate::theme::current());
                    draw::pop_clip();
                }
            });
        }
        let mut w = f.clone();
        ctx.bind(move |s| {
            let d = data(s);
            let mut cur = cur.borrow_mut();
            if cur.as_ref() != Some(&d) {
                *cur = Some(d);
                repaint(&mut w);
            }
        });
        f.as_base_widget()
    })
}

/// A filled line graph of `values(state)` (oldest first), scaled to
/// `0..=max`: CPU/network history and the like. The copy it keeps is
/// reused, so steady updates don't allocate.
pub fn graph<S: 'static, M: 'static>(values: impl Fn(&S) -> &[f64] + 'static, max: f64) -> Element<S, M> {
    Element::new(move |ctx| {
        let cur: Rc<RefCell<Vec<f64>>> = Rc::default();
        let mut f = Frame::default();
        f.set_frame(FrameType::NoBox);
        {
            let cur = cur.clone();
            f.draw(move |f| {
                let t = crate::theme::current();
                let (x, y, w, h) = (f.x(), f.y(), f.w(), f.h());
                draw::set_draw_color(t.surface_alt);
                draw::draw_rounded_rectf(x, y, w, h, t.radius.min(h / 2).min(6));
                let v = cur.borrow();
                if v.len() < 2 || w < 2 {
                    return;
                }
                let px = |i: usize| x as f64 + i as f64 * (w - 1) as f64 / (v.len() - 1) as f64;
                let py = |val: f64| (y + h - 1) as f64 - (val / max).clamp(0.0, 1.0) * (h - 2) as f64;
                draw::push_clip(x, y, w, h);
                draw::set_draw_color(mix(t.accent, t.surface_alt, 0.7));
                draw::begin_complex_polygon();
                draw::vertex(x as f64, (y + h) as f64);
                for (i, &val) in v.iter().enumerate() {
                    draw::vertex(px(i), py(val));
                }
                draw::vertex((x + w - 1) as f64, (y + h) as f64);
                draw::end_complex_polygon();
                draw::set_draw_color(t.accent);
                draw::set_line_style(LineStyle::Solid | LineStyle::CapRound | LineStyle::JoinRound, 2);
                draw::begin_line();
                for (i, &val) in v.iter().enumerate() {
                    draw::vertex(px(i), py(val));
                }
                draw::end_line();
                draw::set_line_style(LineStyle::Solid, 0);
                draw::pop_clip();
            });
        }
        let mut w = f.clone();
        ctx.bind(move |s| {
            let new = values(s);
            let mut cur = cur.borrow_mut();
            if cur.as_slice() != new {
                cur.clear();
                cur.extend_from_slice(new);
                w.redraw();
            }
        });
        f.as_base_widget()
    })
}

/// Space between scrolled content and its scrollbar.
const SCROLLBAR_GAP: i32 = 8;

/// A vertically scrolling column, for content taller than its space (a
/// settings page). Children need a height: `.fixed(px)`, `.fixed_with(..)`,
/// or a natural one (`list`). The content grows and shrinks with them; the
/// mouse wheel, the scrollbar, and dragging the content vertically (touch
/// or mouse, see [`crate::drag_scroll`]) scroll it.
pub fn scroll<S: 'static, M: 'static>(children: Vec<Element<S, M>>) -> Element<S, M> {
    use fltk::group::{Scroll, ScrollType};
    Element::new(move |ctx| {
        let t = ctx.theme_rc();
        let mut sc = Scroll::default();
        sc.set_type(ScrollType::Vertical);
        // Not Tab stops (scrolling follows the focus instead).
        sc.scrollbar().clear_visible_focus();
        sc.hscrollbar().clear_visible_focus();
        // Opaque, so FLTK can scroll by copying pixels.
        sc.set_frame(FrameType::FlatBox);
        sc.set_scrollbar_size(10);
        let mut bar = sc.scrollbar();
        bar.set_frame(FrameType::FlatBox);
        bar.set_slider_frame(ROUNDED);
        let mut style = {
            let (mut sc, mut bar) = (sc.clone(), bar.clone());
            move |t: &Theme| {
                sc.set_color(t.background);
                bar.set_color(t.background);
                bar.set_selection_color(t.surface_alt);
                bar.set_label_color(t.text_dim);
            }
        };
        style(&t);
        crate::theme::on_change(&sc, style);

        let mut content = Flex::default().column();
        content.end();
        content.set_margin(0);
        content.set_pad(t.spacing);
        let hints: Vec<Rc<Cell<i32>>> = children.iter().map(|c| c.size_hint()).collect();
        ctx.build_children(&mut content, children);
        sc.end();

        let kids: Vec<fltk::widget::Widget> = (0..content.children()).filter_map(|i| content.child(i)).collect();
        let height = Rc::new(Cell::new(0));
        // Width follows the scroll area, leaving room for the scrollbar
        // when it shows.
        let fit_width = {
            let (height, content) = (height.clone(), content.clone());
            move |sc: &Scroll| {
                // Room for the scrollbar plus a small gap, when it shows.
                let bar = if height.get() > sc.h() { sc.scrollbar_size() + SCROLLBAR_GAP } else { 0 };
                let mut content = content.clone();
                let (x, y) = (sc.x(), content.y());
                content.resize(x, y, (sc.w() - bar).max(0), height.get().max(sc.h()));
            }
        };
        {
            let fit_width = fit_width.clone();
            sc.resize_callback(move |sc, _, _, _, _| fit_width(sc));
        }
        crate::drag_scroll::register(&sc);
        let mut last: Vec<i32> = vec![-2; kids.len()];
        let mut scw = sc.clone();
        ctx.bind(move |_| {
            let pad = content.pad();
            let mut total = 0;
            let mut shown = 0;
            let mut changed = false;
            for (i, (kid, hint)) in kids.iter().zip(&hints).enumerate() {
                let h = hint.get().max(0);
                if h != last[i] {
                    last[i] = h;
                    content.fixed(kid, h);
                    changed = true;
                }
                if kid.visible() {
                    total += h;
                    shown += 1;
                }
            }
            total += pad * (shown - 1).max(0);
            if total != height.get() || changed {
                height.set(total);
                fit_width(&scw);
                content.recalc();
                // Don't stay scrolled past the end after shrinking.
                let max = (total - scw.h()).max(0);
                if scw.yposition() > max {
                    scw.scroll_to(0, max);
                }
                scw.redraw();
            }
        });
        sc.as_base_widget()
    })
}

/// An icon from [`crate::icons`], centered in its space: a built-in line
/// icon drawn in the theme's text color, a theme (app) icon, or a file.
/// `name(state)` can change it (battery level, mute state...).
pub fn icon<S: 'static, M: 'static>(name: impl Fn(&S) -> String + 'static, size: i32) -> Element<S, M> {
    Element::new(move |ctx| {
        let cur: Rc<RefCell<String>> = Rc::default();
        let mut f = Frame::default();
        f.set_frame(FrameType::NoBox);
        {
            let cur = cur.clone();
            f.draw(move |f| {
                let t = crate::theme::current();
                let s = size.min(f.w()).min(f.h());
                crate::icons::draw(&cur.borrow(), f.x() + (f.w() - s) / 2, f.y() + (f.h() - s) / 2, s, t.text);
            });
        }
        let mut w = f.clone();
        ctx.bind(move |st| {
            let n = name(st);
            if *cur.borrow() != n {
                *cur.borrow_mut() = n;
                repaint(&mut w);
            }
        });
        f.as_base_widget()
    })
}

/// Options side by side (or stacked, `vertical`), the chosen one
/// highlighted: picking another sends `on_select(index)`, and the
/// highlight slides over to it, stretching a little on the way (tabs,
/// a mode switch, a page list). Give it a fixed size: each option gets an
/// equal share.
pub fn segmented<S: 'static, M: 'static>(
    labels: &[&str],
    vertical: bool,
    selected: impl Fn(&S) -> usize + 'static,
    on_select: impl Fn(usize) -> M + 'static,
) -> Element<S, M> {
    let labels: Vec<String> = labels.iter().map(|l| l.to_string()).collect();
    Element::new(move |ctx| {
        const GAP: i32 = 8;
        let n = labels.len().max(1) as i32;
        // Option i's rectangle within the widget.
        let slot = move |w: i32, h: i32, i: usize| -> (i32, i32, i32, i32) {
            let i = i as i32;
            if vertical {
                let rh = (h - GAP * (n - 1)) / n;
                (0, i * (rh + GAP), w, rh)
            } else {
                let rw = (w - GAP * (n - 1)) / n;
                (i * (rw + GAP), 0, rw, h)
            }
        };
        // The highlight's leading and trailing ends, as fractional indexes
        // (the leading one is quicker: it stretches, then catches up).
        let head = crate::anim::Tween::new(-1.0);
        let tail = crate::anim::Tween::new(-1.0);
        let hover: Rc<RefCell<crate::hover::HoverFade>> = Rc::default();
        let mut f = Frame::default();
        f.set_frame(FrameType::NoBox);
        {
            let (head, tail, hover, labels) = (head.clone(), tail.clone(), hover.clone(), labels.clone());
            f.draw(move |f| {
                let t = crate::theme::current();
                let (fx, fy) = (f.x() as f64, f.y() as f64);
                let at = |v: f64| {
                    let v = v.clamp(0.0, (n - 1) as f64);
                    let (a, b) = (slot(f.w(), f.h(), v.floor() as usize), slot(f.w(), f.h(), v.ceil() as usize));
                    let k = v - v.floor();
                    let l = |p: i32, q: i32| p as f64 + (q - p) as f64 * k;
                    (l(a.0, b.0), l(a.1, b.1), l(a.2, b.2), l(a.3, b.3))
                };
                let hv = hover.borrow();
                for i in 0..labels.len() {
                    let (x, y, w, h) = slot(f.w(), f.h(), i);
                    let bg = mix(t.surface_alt, Color::White, 0.1 * hv.amount(i));
                    draw::set_draw_color(if f.active_r() { bg } else { mix(bg, t.background, 0.6) });
                    draw::draw_rounded_rectf(f.x() + x, f.y() + y, w, h, t.radius.min(h / 2));
                }
                let (h0, t0) = (head.get(), tail.get());
                let span = (h0 >= 0.0).then(|| {
                    let (a, b) = (at(h0), at(t0));
                    let (x0, y0) = (a.0.min(b.0), a.1.min(b.1));
                    let (x1, y1) = ((a.0 + a.2).max(b.0 + b.2), (a.1 + a.3).max(b.1 + b.3));
                    let r = t.radius.min(a.3 as i32 / 2) as f64;
                    crate::fx::fill_rounded(fx + x0, fy + y0, x1 - x0, y1 - y0, r, t.accent, 1.0);
                    (x0, y0, x1, y1)
                });
                draw::set_font(t.font(), t.font_size);
                for (i, label) in labels.iter().enumerate() {
                    let (x, y, w, h) = slot(f.w(), f.h(), i);
                    // Text under the highlight takes its color as it passes.
                    let cover = span.map_or(0.0, |(x0, y0, x1, y1)| {
                        let (a, b) = if vertical { ((y0, y1), (y as f64, (y + h) as f64)) } else { ((x0, x1), (x as f64, (x + w) as f64)) };
                        ((a.1.min(b.1) - a.0.max(b.0)) / (b.1 - b.0)).clamp(0.0, 1.0)
                    });
                    draw::set_draw_color(mix(t.text, t.accent_text, cover as f32));
                    draw::draw_text2(label, f.x() + x, f.y() + y, w, h, Align::Center);
                }
            });
        }
        let emit = ctx.emitter();
        {
            let hover = hover.clone();
            let pressed = Cell::new(None);
            f.handle(move |f, ev| {
                let (px, py) = (fltk::app::event_x() - f.x(), fltk::app::event_y() - f.y());
                let at = (0..n as usize).find(|&i| {
                    let (x, y, w, h) = slot(f.w(), f.h(), i);
                    px >= x && px < x + w && py >= y && py < y + h
                });
                match ev {
                    Event::Enter | Event::Move => {
                        hover.borrow_mut().set(at, &f.as_base_widget());
                        true
                    }
                    Event::Leave => {
                        hover.borrow_mut().set(None, &f.as_base_widget());
                        true
                    }
                    Event::Push => {
                        pressed.set(at);
                        true
                    }
                    Event::Released => {
                        if let Some(i) = pressed.take().filter(|&i| Some(i) == at) {
                            emit(on_select(i));
                        }
                        true
                    }
                    _ => false,
                }
            });
        }
        let w = f.clone();
        ctx.bind(move |s| {
            let i = selected(s) as f64;
            if head.target() == i && head.get() >= 0.0 {
                return;
            }
            let mut w = w.clone();
            if head.get() < 0.0 || !w.visible_r() {
                head.set(i);
                tail.set(i);
                w.redraw();
                return;
            }
            let mut w2 = w.clone();
            head.spring_to(i, crate::anim::Spring { response: 0.22, damping: 0.8 }, move || w.redraw());
            tail.spring_to(i, crate::anim::Spring { response: 0.29, damping: 0.9 }, move || w2.redraw());
        });
        f.as_base_widget()
    })
}
