//! App-wide hover tracking.
//!
//! Done once per event-loop pass by comparing FLTK's `belowmouse` pointer,
//! instead of a Rust `handle` closure on every widget: in fltk-rs each
//! closure call builds a widget wrapper whose tracker FLTK removes with a
//! linear scan, so per-widget handlers made every event cost
//! O(widgets²). Draw code asks [`is_hovered`].

use std::cell::{Cell, RefCell};

use fltk::button::Button;
use fltk::enums::Cursor;
use fltk::input::Input;
use fltk::prelude::*;
use fltk::widget::Widget;

thread_local! {
    static HOVERED: Cell<usize> = const { Cell::new(0) };
    static CURRENT: RefCell<Option<(Widget, bool)>> = const { RefCell::new(None) };
}

/// True while the mouse is over `w`. For custom draw callbacks; the
/// widget is redrawn automatically when this changes.
pub fn is_hovered<W: WidgetExt>(w: &W) -> bool {
    HOVERED.with(|h| h.get() == w.as_widget_ptr() as usize)
}

/// Called by `run` after every event batch: one FFI call when nothing
/// changed.
pub(crate) fn update() {
    let ptr = unsafe { fltk_sys::fl::Fl_belowmouse() } as usize;
    if HOVERED.with(|h| h.replace(ptr)) == ptr {
        return;
    }
    CURRENT.with(|cur| {
        let mut cur = cur.borrow_mut();
        let old = cur.take().filter(|(w, _)| !w.was_deleted());
        let new = (ptr != 0).then(|| unsafe { Widget::from_widget_ptr(ptr as *mut _) });
        let clickable = new
            .as_ref()
            .is_some_and(|w| w.active_r() && Button::from_dyn_widget(w).is_some());

        let window = new.as_ref().and_then(|w| w.window()).or_else(|| old.as_ref().and_then(|(w, _)| w.window()));
        if let Some(mut win) = window {
            if clickable {
                win.set_cursor(Cursor::Hand);
            } else if old.as_ref().is_some_and(|(_, c)| *c) {
                // Text fields set their own cursor; don't undo it.
                let text = new.as_ref().is_some_and(|w| Input::from_dyn_widget(w).is_some());
                win.set_cursor(if text { Cursor::Insert } else { Cursor::Default });
            }
        }
        // Only clickable widgets draw a hover state.
        // Only clickable widgets draw a hover state. `repaint`, not
        // `redraw`: a hover effect may cover more than the normal look.
        if let Some((mut w, true)) = old {
            crate::widgets::repaint(&mut w);
        }
        *cur = new.map(|mut w| {
            if clickable {
                crate::widgets::repaint(&mut w);
            }
            (w, clickable)
        });
    });
}
