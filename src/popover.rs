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

/// `anchor`, with `content` popping down from it while `open(state)`.
/// `size(state)` is the popover's size; it follows changes while open.
pub fn popover<S: 'static, M: Clone + 'static>(
    anchor: Element<S, M>,
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
                win.set_color(t.surface);
                win.redraw();
            }
        };
        style(ctx.theme());
        crate::theme::on_change(&win, style);
        win.set_frame(FrameType::FlatBox);
        let mut root = content.build(ctx);
        root.resize(0, 0, win.w(), win.h());
        win.resizable(&root);
        win.end();
        // A thin border over the content.
        win.draw(|w| {
            let t = crate::theme::current();
            fltk::draw::set_draw_color(t.border);
            fltk::draw::draw_rect(0, 0, w.w(), w.h());
        });
        if let Some(g) = outer {
            Group::set_current(Some(&g));
        }

        // True while the app wants it open; a hide we didn't ask for
        // (outside click, Escape) sends `on_close`.
        let wanted = Rc::new(Cell::new(false));
        let emit = ctx.emitter();
        win.set_callback(|w| w.hide());
        {
            let wanted = wanted.clone();
            win.handle(move |w, ev| match ev {
                Event::Hide => {
                    if !crate::on_wayland() {
                        fltk::app::set_grab(None::<Window>);
                    }
                    if wanted.replace(false) {
                        emit(on_close.clone());
                    }
                    false
                }
                Event::KeyDown if fltk::app::event_key() == Key::Escape => {
                    w.hide();
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
        ctx.bind(move |s| {
            let want = open(s);
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
                wanted.set(false);
                win.hide();
                return;
            }
            let (w, h) = size(s);
            win.set_size(w.max(1), h.max(1));
            let Some(parent) = anchor.window() else { return };
            wanted.set(true);
            show_at(&mut win, &*parent, &anchor);
        });
        result
    })
}

fn show_at(win: &mut Window, parent: &dyn WindowExt, anchor: &fltk::widget::Widget) {
    let (ax, ay, aw, ah) = (anchor.x(), anchor.y(), anchor.w(), anchor.h());
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
        let x = (parent.x_root() + ax).clamp(sx, (sx + sw - win.w()).max(sx));
        let below = parent.y_root() + ay + ah;
        let y = if below + win.h() <= sy + sh { below } else { parent.y_root() + ay - win.h() };
        win.set_pos(x, y);
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
