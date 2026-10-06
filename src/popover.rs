//! Popovers: a small window of content that drops down from a widget (a
//! volume slider and device list from a bar module, a network list...).
//!
//! Declared in the view like anything else and shown while `open(state)`
//! is true, so it's driven by state like the rest of the app. Its content
//! is built once, with the view, into a window that's only shown when
//! needed: a hidden window costs its widgets, no surface or buffer.
//!
//! - Wayland, with the fltk-sys fork (feature `layer-shell`): a real
//!   xdg_popup of the anchor's window (also from layer-shell panels). It
//!   takes the keyboard, and the compositor closes it on a click
//!   elsewhere. Open it from a button *press* (see [`press_button`]):
//!   compositors only grant a popup for a recent press.
//! - X11: a borderless window under the anchor that grabs the pointer, so
//!   a click outside closes it.
//! - Wayland without the fork: a borderless regular window.
//!
//! Closing (outside click, Escape, the compositor) sends `on_close`; the
//! app sets its state so `open` is false.

use std::cell::Cell;
use std::rc::Rc;

use fltk::enums::{Event, FrameType, Key};
use fltk::group::Group;
use fltk::prelude::*;
use fltk::window::Window;

use crate::element::Element;

/// `anchor`, with `content` popping down from it (centered under it)
/// while `open(state)`; it unrolls and rolls up quickly (~120 ms).
/// `size(state)` is the popover's size; it follows changes while open.
pub fn popover<S: 'static, M: Clone + 'static>(
    anchor: Element<S, M>,
    open: impl Fn(&S) -> bool + 'static,
    on_close: M,
    size: impl Fn(&S) -> (i32, i32) + 'static,
    content: Element<S, M>,
) -> Element<S, M> {
    popover_at(anchor, |_: &S| None, open, on_close, size, content)
}

/// Like [`popover`], but it drops down from part of the anchor:
/// `rect(state)` is (x, y, w, h) relative to the anchor widget (e.g. one
/// button of a widget that draws several), None for the whole anchor.
pub fn popover_at<S: 'static, M: Clone + 'static>(
    anchor: Element<S, M>,
    rect: impl Fn(&S) -> Option<(i32, i32, i32, i32)> + 'static,
    open: impl Fn(&S) -> bool + 'static,
    on_close: M,
    size: impl Fn(&S) -> (i32, i32) + 'static,
    content: Element<S, M>,
) -> Element<S, M> {
    Element::new(move |ctx| {
        let anchor = anchor.build(ctx);

        // The popover is a top-level window: build it outside the current
        // group, then go back.
        let outer = Group::try_current();
        Group::set_current(None::<&Group>);
        let mut win = Window::default().with_size(240, 160);
        win.set_border(false);
        win.make_resizable(true);
        let mut style = {
            let mut win = win.clone();
            move |t: &crate::Theme| {
                // Like a small window of the app: its background, a border.
                win.set_color(t.background);
                win.redraw();
            }
        };
        style(ctx.theme());
        crate::theme::on_change(&win, style);
        let mut root = content.build(ctx);
        root.resize(0, 0, win.w(), win.h());
        win.resizable(&root);
        win.end();
        // Rounded like the theme where the window can be see-through
        // (Wayland with the fork), square elsewhere. It unrolls when it
        // opens and rolls up when it closes: `reveal` is 0..1 of its height.
        let round = round_corners(&mut win);
        let reveal = crate::anim::Tween::new(1.0);
        win.set_frame(FrameType::NoBox);
        win.super_draw(false);
        // Where the anchor's top is in the parent window (to tell whether
        // the compositor put the popover above it).
        let anchor_y = Rc::new(Cell::new(0));
        {
            let (reveal, anchor_y) = (reveal.clone(), anchor_y.clone());
            let snap = crate::fx::Snapshot::new();
            win.draw(move |w| {
                let t = crate::theme::current();
                let r_ = reveal.get();
                let moving = r_ != 1.0;
                let panel = |w: &mut Window, h: i32| {
                    let r = if round { RADIUS.with(Cell::get).unwrap_or(t.radius).min(16).min(h / 2) } else { 0 };
                    fltk::draw::set_draw_color(t.border);
                    fltk::draw::draw_rounded_rectf(0, 0, w.w(), h, r);
                    fltk::draw::set_draw_color(t.background);
                    fltk::draw::draw_rounded_rectf(1, 1, w.w() - 2, h - 2, (r - 1).max(0));
                    fltk::draw::push_clip(0, 0, w.w(), h);
                    w.draw_children();
                    fltk::draw::pop_clip();
                };
                if round {
                    #[cfg(feature = "layer-shell")]
                    unsafe {
                        fltk_sys::window::Fl_wl_clear_rect(0, 0, w.w(), w.h())
                    };
                    // Pops in: grows out of the edge next to its anchor
                    // (bottom if it opened above it), fading in, settling
                    // with a little overshoot; shrinks and fades away.
                    if moving {
                        let full = (0, 0, w.w(), w.h());
                        let mut w2 = w.clone();
                        if snap.record(full, || panel(&mut w2, full.3)) {
                            let above = popup_y(w).is_some_and(|y| y + w.h() <= anchor_y.get());
                            let origin = (w.w() as f64 / 2.0, if above { w.h() as f64 } else { 0.0 });
                            let scale = (0.94 + 0.06 * r_, 0.88 + 0.12 * r_);
                            snap.paint(origin, scale, (0.0, 0.0), (r_ * 1.5).clamp(0.0, 1.0));
                            snap.clear();
                            return;
                        }
                    }
                } else {
                    // X11: no see-through; the unrolled part is the panel.
                    fltk::draw::set_draw_color(t.background);
                    fltk::draw::draw_rectf(0, 0, w.w(), w.h());
                }
                // Unrolls where it can't pop in (X11).
                let h = ((w.h() as f64) * r_.min(1.0)).round().max(1.0) as i32;
                panel(w, h);
            });
        }
        if let Some(g) = outer {
            Group::set_current(Some(&g));
        }
        // True while the app wants it open; a hide we didn't ask for
        // (the compositor dismissing it) sends `on_close`.
        let wanted = Rc::new(Cell::new(false));
        let emit = ctx.emitter();
        // Escape or a click elsewhere in the app: ask the app to close it,
        // so it goes away with its animation.
        let request: Rc<dyn Fn()> = {
            let (wanted, emit, on_close) = (wanted.clone(), emit.clone(), on_close.clone());
            Rc::new(move || {
                if wanted.replace(false) {
                    emit(on_close.clone());
                }
            })
        };
        REGISTRY.with(|r| r.borrow_mut().push((anchor.clone(), win.clone(), request.clone())));
        win.set_callback(|w| w.hide());
        {
            let wanted = wanted.clone();
            // X11: the widget a drag started on, which FLTK doesn't give
            // the release to when it happens outside the popover (its grab
            // sends it to whatever's under the pointer).
            let dragging: Rc<Cell<Option<fltk::widget::Widget>>> = Rc::default();
            win.handle(move |w, ev| match ev {
                Event::Drag if !crate::on_wayland() => {
                    dragging.set(fltk::app::pushed().map(|p| p.as_base_widget()));
                    false
                }
                Event::Released if !crate::on_wayland() => {
                    let (x, y) = (fltk::app::event_x(), fltk::app::event_y());
                    match dragging.take() {
                        Some(mut d) if (x < 0 || y < 0 || x >= w.w() || y >= w.h()) && !d.was_deleted() => d.handle_event(Event::Released),
                        _ => false,
                    }
                }
                Event::Show => {
                    shown_changed(1);
                    false
                }
                Event::Hide => {
                    shown_changed(-1);
                    if !crate::on_wayland() {
                        fltk::app::set_grab(None::<Window>);
                    }
                    if wanted.replace(false) {
                        emit(on_close.clone());
                    }
                    false
                }
                Event::KeyDown if fltk::app::event_key() == Key::Escape => {
                    request();
                    true
                }
                // X11: the grab sends us clicks anywhere; outside closes.
                Event::Push if !crate::on_wayland() => {
                    let (x, y) = (fltk::app::event_x(), fltk::app::event_y());
                    if x < 0 || y < 0 || x >= w.w() || y >= w.h() {
                        w.hide();
                        return true;
                    }
                    false
                }
                _ => false,
            });
        }

        let result = anchor.clone();
        let closing = Rc::new(Cell::new(false));
        ctx.bind(move |s| {
            let want = open(s);
            if want && win.shown() && closing.get() {
                // Opened again while rolling up: unroll.
                closing.set(false);
                let mut w2 = win.clone();
                reveal.animate_ease(1.0, OPEN, crate::anim::snappy, move || w2.redraw());
                return;
            }
            if want && win.shown() {
                // Content arriving (a list filling in) can resize it.
                let (w, h) = size(s);
                if (w, h) != (win.w(), win.h()) {
                    win.resize(win.x(), win.y(), w.max(1), h.max(1));
                }
                return;
            }
            if want == win.shown() {
                return;
            }
            if !want {
                if closing.get() {
                    return;
                }
                wanted.set(false);
                // Rolls up, then hides (at once with animations off).
                closing.set(true);
                let (mut w2, r2, c2) = (win.clone(), reveal.clone(), closing.clone());
                reveal.animate_ease(0.0, CLOSE, crate::anim::ease_in, move || {
                    if w2.was_deleted() || !c2.get() {
                        return;
                    }
                    if r2.get() <= 0.0 {
                        c2.set(false);
                        w2.hide();
                    } else {
                        w2.redraw();
                    }
                });
                return;
            }
            let (w, h) = size(s);
            win.set_size(w.max(1), h.max(1));
            let Some(parent) = anchor.window() else { return };
            wanted.set(true);
            let r = match rect(s) {
                Some((x, y, w, h)) => (anchor.x() + x, anchor.y() + y, w, h),
                None => (anchor.x(), anchor.y(), anchor.w(), anchor.h()),
            };
            reveal.set(0.0);
            anchor_y.set(r.1);
            show_at(&mut win, &*parent, r);
            let mut w2 = win.clone();
            reveal.animate_ease(1.0, OPEN, crate::anim::snappy, move || w2.redraw());
        });
        result
    })
}

/// How long a popover takes to pop in and to go away.
const OPEN: std::time::Duration = std::time::Duration::from_millis(240);
const CLOSE: std::time::Duration = std::time::Duration::from_millis(130);

thread_local! {
    /// Corner radius of popovers (None: the theme's).
    static RADIUS: Cell<Option<i32>> = const { Cell::new(None) };
    /// Popovers shown: tooltips wait meanwhile (on Wayland a tooltip
    /// can't open beside a popup that holds the pointer).
    static SHOWN: Cell<u32> = const { Cell::new(0) };
}

/// Sets the popovers' corner radius (None: the theme's), e.g. to match a
/// panel's style.
pub fn set_radius(r: Option<i32>) {
    RADIUS.with(|c| c.set(r));
}

fn shown_changed(delta: i32) {
    SHOWN.with(|n| {
        let v = (n.get() as i32 + delta).max(0) as u32;
        n.set(v);
        if v > 0 {
            fltk::misc::Tooltip::disable();
        } else {
            fltk::misc::Tooltip::enable(true);
        }
    });
}

thread_local! {
    /// Popovers and their anchors, to delete a popover with its anchor.
    /// Popovers, their anchors, and how to ask the app to close each.
    static REGISTRY: std::cell::RefCell<Vec<(fltk::widget::Widget, Window, Rc<dyn Fn()>)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Wayland: a press in `window` while popovers are open. Compositors only
/// dismiss a popup for clicks on *other* apps' surfaces, so a click
/// elsewhere in this app (e.g. a full-screen overlay around a menu) would
/// leave it open. Closes them unless `window` is one of them; true if it
/// did (the click is used up, like closing a menu with a click).
pub(crate) fn press_outside(window: *mut std::ffi::c_void) -> bool {
    let open: Vec<(Window, Rc<dyn Fn()>)> = REGISTRY.with(|r| {
        r.borrow().iter().filter(|(_, w, _)| !w.was_deleted() && w.shown()).map(|(_, w, c)| (w.clone(), c.clone())).collect()
    });
    if open.is_empty() || open.iter().any(|(w, _)| w.as_widget_ptr() as *mut std::ffi::c_void == window) {
        return false;
    }
    for (_, close) in open {
        close();
    }
    true
}

/// Deletes the popovers whose anchor is gone (after a rebuild).
pub(crate) fn forget_deleted() {
    REGISTRY.with(|r| {
        r.borrow_mut().retain(|(anchor, win, _)| {
            if anchor.was_deleted() {
                if !win.was_deleted() {
                    let mut w = win.clone();
                    w.hide();
                    fltk::app::delete_widget(w);
                }
                false
            } else {
                true
            }
        })
    });
}

/// Makes `win` see-through outside its rounded panel, where possible.
fn round_corners(win: &mut Window) -> bool {
    #[cfg(feature = "layer-shell")]
    if crate::on_wayland() {
        unsafe { fltk_sys::window::Fl_Window_wl_transparent(win.as_widget_ptr() as *mut _) };
        return true;
    }
    let _ = win;
    false
}

thread_local! {
    /// X11: where each shown popover's parent window was (by popover).
    static PARENT_AT: std::cell::RefCell<std::collections::HashMap<usize, (i32, i32)>> = Default::default();
}

/// Wayland: where popover window `win` is in its parent, vertically.
fn popup_y(win: &Window) -> Option<i32> {
    #[cfg(feature = "layer-shell")]
    if crate::on_wayland() {
        let (mut x, mut y) = (0, 0);
        let known = unsafe { fltk_sys::window::Fl_Window_wl_popup_position(win.as_widget_ptr() as *mut _, &mut x, &mut y) };
        return (known != 0).then_some(y);
    }
    let _ = win;
    None
}

/// Where the popover holding `w` is, relative to the window it drops down
/// from (e.g. to follow a drag from the popover onto that window). None
/// when it isn't shown or it can't be told (Wayland without the fork).
pub fn offset<W: WidgetExt>(w: &W) -> Option<(i32, i32)> {
    let win = w.window()?;
    if !win.shown() {
        return None;
    }
    if crate::on_wayland() {
        #[cfg(feature = "layer-shell")]
        {
            let (mut x, mut y) = (0, 0);
            let known = unsafe { fltk_sys::window::Fl_Window_wl_popup_position(win.as_widget_ptr() as *mut _, &mut x, &mut y) };
            return (known != 0).then_some((x, y));
        }
        #[cfg(not(feature = "layer-shell"))]
        return None;
    }
    let (px, py) = PARENT_AT.with(|p| p.borrow().get(&(win.as_widget_ptr() as usize)).copied())?;
    Some((win.x() - px, win.y() - py))
}

/// During a drag that started in the popover holding `w`: where the
/// pointer is in the window the popover drops down from, once it has left
/// the popover (None while it's inside). Far away when it's over neither.
pub fn dragged_out<W: WidgetExt>(w: &W) -> Option<(i32, i32)> {
    const NOWHERE: (i32, i32) = (i32::MIN / 2, i32::MIN / 2);
    let win = w.window()?;
    let (ex, ey) = (fltk::app::event_x(), fltk::app::event_y());
    // On Wayland, the drag goes on in the window under the pointer, with
    // coordinates relative to it.
    #[cfg(feature = "layer-shell")]
    if crate::on_wayland() {
        let ev = unsafe { fltk_sys::window::Fl_Window_event_window() } as usize;
        if ev != 0 && ev != win.as_widget_ptr() as usize {
            let parent = PARENT_OF.with(|p| p.borrow().get(&(win.as_widget_ptr() as usize)).copied());
            return Some(if parent == Some(ev) { (ex, ey) } else { NOWHERE });
        }
    }
    if ex >= 0 && ey >= 0 && ex < win.w() && ey < win.h() {
        return None;
    }
    // Elsewhere (X11), the popover keeps the pointer.
    Some(offset(w).map_or(NOWHERE, |(ox, oy)| (ox + ex, oy + ey)))
}

thread_local! {
    /// Each shown popover's parent window (by popover).
    static PARENT_OF: std::cell::RefCell<std::collections::HashMap<usize, usize>> = Default::default();
}

fn show_at(win: &mut Window, parent: &dyn WindowExt, (ax, ay, aw, ah): (i32, i32, i32, i32)) {
    PARENT_OF.with(|p| p.borrow_mut().insert(win.as_widget_ptr() as usize, parent.as_widget_ptr() as usize));
    if crate::on_wayland() {
        #[cfg(feature = "layer-shell")]
        unsafe {
            fltk_sys::window::Fl_Window_wl_popup(
                win.as_widget_ptr() as *mut _,
                parent.as_widget_ptr() as *mut _,
                ax,
                ay,
                aw,
                ah,
            );
        }
        let _ = (ax, ay, aw, ah, parent);
        win.show();
    } else {
        // Below the anchor, kept on the screen.
        let (sx, sy, sw, sh) = fltk::app::screen_xywh(parent.screen_num());
        let x = (parent.x_root() + ax + aw / 2 - win.w() / 2).clamp(sx, (sx + sw - win.w()).max(sx));
        let below = parent.y_root() + ay + ah;
        let y = if below + win.h() <= sy + sh { below } else { parent.y_root() + ay - win.h() };
        win.set_pos(x, y);
        PARENT_AT.with(|p| p.borrow_mut().insert(win.as_widget_ptr() as usize, (parent.x_root(), parent.y_root())));
        win.set_override();
        win.show();
        fltk::app::set_grab(Some(win.clone()));
    }
}

/// [`custom_button`](crate::widgets::custom_button) whose callback runs on
/// press as well as release, for opening popovers (Wayland grants popups
/// for a recent press). In the callback, act when `b.value()` is true.
pub fn press_button(draw: impl FnMut(&mut fltk::button::Button) + 'static) -> fltk::button::Button {
    let mut b = crate::widgets::custom_button(draw);
    b.set_trigger(fltk::enums::CallbackTrigger::Changed);
    b
}
