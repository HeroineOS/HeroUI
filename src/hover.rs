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
    /// Widgets fading in or out of their hover look: (widget pointer, 0..1).
    static FADES: RefCell<Vec<(usize, crate::anim::Tween)>> = const { RefCell::new(Vec::new()) };
    /// Widgets easing in or out of their pressed look: (widget pointer, 0..1).
    static PRESSES: RefCell<Vec<(usize, crate::anim::Tween)>> = const { RefCell::new(Vec::new()) };
    static PUSHED: Cell<usize> = const { Cell::new(0) };
    static PUSHED_W: RefCell<Option<Widget>> = const { RefCell::new(None) };
    static HOVERED: Cell<usize> = const { Cell::new(0) };
    static CURRENT: RefCell<Option<(Widget, bool)>> = const { RefCell::new(None) };
}

/// True while the mouse is over `w`. For custom draw callbacks; the
/// widget is redrawn automatically when this changes.
pub fn is_hovered<W: WidgetExt + ?Sized>(w: &W) -> bool {
    HOVERED.with(|h| h.get() == w.as_widget_ptr() as usize)
}

/// How hovered `w` looks, 0.0 to 1.0: like [`is_hovered`], but it fades
/// in (~110 ms) and softly out (~240 ms); instantly with animations off.
/// Draw code blends the hover color by it.
pub fn hover_amount<W: WidgetExt + ?Sized>(w: &W) -> f32 {
    let ptr = w.as_widget_ptr() as usize;
    let fading = FADES.with(|f| f.borrow().iter().find(|(p, _)| *p == ptr).map(|(_, t)| t.get() as f32));
    fading.unwrap_or(if is_hovered(w) { 1.0 } else { 0.0 })
}

/// How pressed `w` looks, 0.0 to 1.0: it dips in fast (~70 ms) when the
/// pointer goes down on it and springs back (~260 ms) when released, so
/// even a quick tap shows. Draw code dims and shrinks a little by it.
pub fn press_amount<W: WidgetExt + ?Sized>(w: &W) -> f32 {
    let ptr = w.as_widget_ptr() as usize;
    PRESSES.with(|f| f.borrow().iter().find(|(p, _)| *p == ptr).map_or(0.0, |(_, t)| t.get() as f32))
}

/// Eases widget `w`'s entry in `list` toward `to` (1: on, 0: off).
fn ease(list: &'static std::thread::LocalKey<RefCell<Vec<(usize, crate::anim::Tween)>>>, w: &Widget, to: f64, ms: u64, curve: fn(f64) -> f64) {
    let ptr = w.as_widget_ptr() as usize;
    let tween = list.with(|f| {
        let mut f = f.borrow_mut();
        // Finished fade-outs are done with.
        f.retain(|(p, t)| *p == ptr || t.get() > 0.0);
        match f.iter().find(|(p, _)| *p == ptr) {
            Some((_, t)) => t.clone(),
            None => {
                let t = crate::anim::Tween::new(1.0 - to);
                f.push((ptr, t.clone()));
                t
            }
        }
    });
    let mut w = w.clone();
    tween.animate_ease(to, std::time::Duration::from_millis(ms), curve, move || {
        if !w.was_deleted() {
            crate::widgets::repaint(&mut w);
        }
    });
}

/// Fades widget `w` toward hovered (1) or not (0).
fn fade(w: &Widget, to: f64) {
    if to > 0.5 {
        ease(&FADES, w, to, 110, crate::anim::ease_out);
    } else {
        ease(&FADES, w, to, 240, crate::anim::ease_out_quint);
    }
}

/// The pressed widget changed (FLTK's `pushed`): ease the old one out and
/// the new one in. Only buttons draw a press.
fn pressed(ptr: usize) {
    if PUSHED.with(|p| p.replace(ptr)) == ptr {
        return;
    }
    // The released one (a handle made while it was alive: it may be gone).
    if let Some(w) = PUSHED_W.with(|p| p.borrow_mut().take()) {
        if !w.was_deleted() {
            ease(&PRESSES, &w, 0.0, 260, crate::anim::ease_out_quint);
        }
    }
    if ptr != 0 {
        let w = unsafe { Widget::from_widget_ptr(ptr as *mut _) };
        if Button::from_dyn_widget(&w).is_some() {
            ease(&PRESSES, &w, 1.0, 70, crate::anim::ease_out);
            PUSHED_W.with(|p| *p.borrow_mut() = Some(w));
        }
    }
}

/// Called by `run` after every event batch: one FFI call when nothing
/// changed.
pub(crate) fn update() {
    pressed(unsafe { fltk_sys::fl::Fl_pushed() } as usize);
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
        if let Some((w, true)) = old {
            fade(&w, 0.0);
        }
        *cur = new.map(|w| {
            if clickable {
                fade(&w, 1.0);
            }
            (w, clickable)
        });
    });
}

/// Hover fades for a widget that draws several buttons of its own (a
/// taskbar, chips): the newly hovered part fades in while the previous one
/// fades out, ~160 ms (instant with animations off). The widget tracks
/// the pointer itself and calls [`set`](HoverFade::set); its draw code asks
/// [`amount`](HoverFade::amount).
pub struct HoverFade {
    pub cur: Option<usize>,
    prev: Option<usize>,
    t: crate::anim::Tween,
}

impl Default for HoverFade {
    fn default() -> Self {
        HoverFade { cur: None, prev: None, t: crate::anim::Tween::new(1.0) }
    }
}

impl HoverFade {
    /// Part `new` is hovered now (None: none); `w` is repainted while it
    /// fades.
    pub fn set(&mut self, new: Option<usize>, w: &Widget) {
        if new == self.cur {
            return;
        }
        self.prev = self.cur;
        self.cur = new;
        self.t.set(0.0);
        let mut w = w.clone();
        self.t.animate_ease(1.0, std::time::Duration::from_millis(160), crate::anim::ease_out_quint, move || {
            if !w.was_deleted() {
                crate::widgets::repaint(&mut w);
            }
        });
    }

    /// How hovered part `i` looks, 0.0 to 1.0.
    pub fn amount(&self, i: usize) -> f32 {
        let t = self.t.get() as f32;
        if Some(i) == self.cur {
            t
        } else if Some(i) == self.prev {
            1.0 - t
        } else {
            0.0
        }
    }

    /// Forgets the hover (the parts changed).
    pub fn clear(&mut self) {
        self.cur = None;
        self.prev = None;
    }
}
