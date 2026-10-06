//! Scrolling by dragging the content (touch, or mouse).
//!
//! FLTK sends a drag to the widget that was pressed, never to the scroll
//! area around it, so a finger on a button inside a page couldn't scroll
//! the page. A global `Fl::event_dispatch` hook sees every event first:
//! when a press inside a HeroUI scroll area turns into a mostly vertical
//! drag of more than [`THRESHOLD`] px, the pressed widget is released
//! without a click and the drag scrolls the area. Taps and horizontal
//! drags (sliders) are untouched. Work is only done on press, drag and
//! release.

use std::cell::{Cell, RefCell};
use std::ffi::{c_int, c_void};

use fltk::button::Button;
use fltk::group::Scroll;
use fltk::prelude::*;
use fltk::widget::Widget;

/// Movement before a press becomes a scroll, in pixels.
pub const THRESHOLD: i32 = 10;

const FL_PUSH: c_int = 1;
const FL_RELEASE: c_int = 2;
const FL_DRAG: c_int = 5;
const FL_MOVE: c_int = 11;
const FL_KEYDOWN: c_int = 8;

thread_local! {
    static SCROLLS: RefCell<Vec<Scroll>> = const { RefCell::new(Vec::new()) };
    static GESTURE: RefCell<Option<Gesture>> = const { RefCell::new(None) };
    static INSTALLED: Cell<bool> = const { Cell::new(false) };
    static BLOCKED: Cell<bool> = const { Cell::new(false) };
    static KEY_HOOKS: RefCell<Vec<std::rc::Rc<dyn Fn() -> bool>>> = RefCell::new(Vec::new());
}

/// See [`crate::on_key`].
pub(crate) fn add_key_hook(f: impl Fn() -> bool + 'static) {
    KEY_HOOKS.with(|h| h.borrow_mut().push(std::rc::Rc::new(f)));
}

struct Gesture {
    scroll: Scroll,
    start: (i32, i32),
    start_pos: i32,
    scrolling: bool,
}

/// Called by `scroll` for every scroll area it builds.
pub(crate) fn register(s: &Scroll) {
    SCROLLS.with(|v| {
        let mut v = v.borrow_mut();
        v.retain(|s| !s.was_deleted());
        v.push(s.clone());
    });
}

/// While set, presses never start a scroll (a popover over the page is
/// being dragged in).
pub(crate) fn set_blocked(on: bool) {
    BLOCKED.with(|b| b.set(on));
}

/// Installs the hook (once). Called by `run`.
pub(crate) fn install() {
    if !INSTALLED.with(|i| i.replace(true)) {
        unsafe { fltk_sys::fl::Fl_event_dispatch(Some(dispatch)) }
    }
}

fn pointer() -> (i32, i32) {
    (fltk::app::event_x_root(), fltk::app::event_y_root())
}

/// The innermost visible registered scroll area under the pointer.
fn scroll_under(x: i32, y: i32) -> Option<Scroll> {
    SCROLLS.with(|v| {
        v.borrow()
            .iter()
            .filter(|s| !s.was_deleted() && s.visible_r())
            .filter(|s| {
                let Some(win) = s.window() else { return false };
                let (sx, sy) = (win.x_root() + s.x(), win.y_root() + s.y());
                x >= sx && x < sx + s.w() && y >= sy && y < sy + s.h()
            })
            .min_by_key(|s| s.w() * s.h())
            .cloned()
    })
}

unsafe extern "C" fn dispatch(event: c_int, window: *mut c_void) -> c_int {
    let pass = || unsafe { fltk_sys::fl::Fl_handle_(event, window as *mut _) };
    match event {
        FL_KEYDOWN => {
            // Copied out: a hook may add hooks or run the event loop.
            let hooks = KEY_HOOKS.with(|h| h.borrow().clone());
            if hooks.iter().any(|f| f()) {
                return 1;
            }
            pass()
        }
        FL_PUSH => {
            if crate::on_wayland() && crate::popover::press_outside(window) {
                return 1;
            }
            let (x, y) = pointer();
            let g = if BLOCKED.with(Cell::get) { None } else { scroll_under(x, y) };
            let g = g.map(|s| Gesture { start_pos: s.yposition(), scroll: s, start: (x, y), scrolling: false });
            GESTURE.with(|gs| *gs.borrow_mut() = g);
            pass()
        }
        // Backends report motion as FL_MOVE; FLTK turns it into FL_DRAG
        // after this hook, so during a press both mean "dragging".
        FL_DRAG | FL_MOVE => {
            let handled = GESTURE.with(|gs| {
                let mut gs = gs.borrow_mut();
                let Some(g) = gs.as_mut() else { return false };
                let (x, y) = pointer();
                let (dx, dy) = (x - g.start.0, y - g.start.1);
                if !g.scrolling {
                    if dy.abs() <= THRESHOLD || dy.abs() <= dx.abs() {
                        return false;
                    }
                    g.scrolling = true;
                    cancel_press();
                }
                let content = content_height(&g.scroll);
                let max = (content - g.scroll.h()).max(0);
                let pos = (g.start_pos - dy).clamp(0, max);
                if pos != g.scroll.yposition() {
                    g.scroll.scroll_to(g.scroll.xposition(), pos);
                }
                true
            });
            if handled {
                1
            } else {
                pass()
            }
        }
        FL_RELEASE => {
            // If a scroll happened, the pressed widget was un-pressed when it
            // began, so this release doesn't click it; FLTK still gets it to
            // clear its own state.
            GESTURE.with(|gs| gs.borrow_mut().take());
            pass()
        }
        _ => pass(),
    }
}

/// Un-presses the widget under the finger, so the coming release isn't a
/// click (buttons only fire when released while pressed).
fn cancel_press() {
    let ptr = unsafe { fltk_sys::fl::Fl_pushed() };
    if ptr.is_null() {
        return;
    }
    let w = unsafe { Widget::from_widget_ptr(ptr as *mut _) };
    if let Some(mut b) = Button::from_dyn_widget(&w) {
        if b.value() {
            b.set_value(false);
            crate::widgets::repaint(&mut b);
        }
    }
}

/// Height of the scrolled content: the scroll's first child (HeroUI puts
/// one column in it).
fn content_height(s: &Scroll) -> i32 {
    s.child(0).map(|c| c.h()).unwrap_or(0)
}
