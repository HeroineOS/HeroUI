//! Cheap whole-panel effects: a [`Snapshot`] records what some widgets
//! draw once, then paints that picture moved, scaled and faded for each
//! frame of an animation, without drawing the widgets again. A frame is
//! a single image copy, so opening and closing animations stay smooth on
//! slow ARM boards too.
//!
//! Only on the Wayland backend (FLTK draws with cairo there). Elsewhere
//! [`Snapshot::record`] returns false: draw the widgets as usual.

use std::cell::Cell;
use std::ffi::c_void;

#[cfg(feature = "wayland")]
mod cairo {
    use std::ffi::{c_double, c_int, c_void};

    #[link(name = "cairo")]
    extern "C" {
        pub fn cairo_save(cr: *mut c_void);
        pub fn cairo_restore(cr: *mut c_void);
        pub fn cairo_rectangle(cr: *mut c_void, x: c_double, y: c_double, w: c_double, h: c_double);
        pub fn cairo_clip(cr: *mut c_void);
        pub fn cairo_push_group(cr: *mut c_void);
        pub fn cairo_pop_group(cr: *mut c_void) -> *mut c_void;
        pub fn cairo_translate(cr: *mut c_void, x: c_double, y: c_double);
        pub fn cairo_scale(cr: *mut c_void, x: c_double, y: c_double);
        pub fn cairo_set_source(cr: *mut c_void, pattern: *mut c_void);
        pub fn cairo_paint_with_alpha(cr: *mut c_void, alpha: c_double);
        pub fn cairo_pattern_destroy(pattern: *mut c_void);
        pub fn cairo_pattern_set_filter(pattern: *mut c_void, filter: c_int);
    }

    /// CAIRO_FILTER_GOOD
    pub const FILTER_GOOD: c_int = 1;
}

/// The cairo context FLTK is drawing with now, if any.
#[cfg(feature = "wayland")]
fn context() -> Option<*mut c_void> {
    if !crate::on_wayland() {
        return None;
    }
    let cr = unsafe { fltk_sys::window::Fl_cairo_gc() };
    (!cr.is_null()).then_some(cr)
}

/// A recorded picture of part of a window (see the module docs). Call
/// its methods only while the window is being drawn (in a `draw`
/// callback).
#[derive(Default)]
pub struct Snapshot {
    pattern: Cell<Option<*mut c_void>>,
    rect: Cell<(i32, i32, i32, i32)>,
}

impl Snapshot {
    pub fn new() -> Snapshot {
        Snapshot::default()
    }

    /// Whether a picture is recorded.
    pub fn is_recorded(&self) -> bool {
        self.pattern.get().is_some()
    }

    /// Records what `draw` paints inside `rect` (window coordinates),
    /// without showing it. False if effects aren't available here; `draw`
    /// isn't called then.
    pub fn record(&self, rect: (i32, i32, i32, i32), draw: impl FnOnce()) -> bool {
        #[cfg(feature = "wayland")]
        if let Some(cr) = context() {
            self.clear();
            let (x, y, w, h) = rect;
            unsafe {
                // FLTK's cairo user space is offset by half a pixel;
                // clipping to the rectangle keeps the picture its size.
                cairo::cairo_save(cr);
                cairo::cairo_rectangle(cr, x as f64 - 0.5, y as f64 - 0.5, w as f64, h as f64);
                cairo::cairo_clip(cr);
                cairo::cairo_push_group(cr);
            }
            draw();
            unsafe {
                let p = cairo::cairo_pop_group(cr);
                cairo::cairo_pattern_set_filter(p, cairo::FILTER_GOOD);
                cairo::cairo_restore(cr);
                self.pattern.set(Some(p));
            }
            self.rect.set(rect);
            return true;
        }
        let _ = (rect, draw);
        false
    }

    /// Paints the picture scaled by `scale` (x, y) around the point
    /// `origin` (window coordinates; e.g. the rectangle's center to grow
    /// from the middle), then moved by `offset`, with opacity `alpha`
    /// (0 to 1).
    pub fn paint(&self, origin: (f64, f64), scale: (f64, f64), offset: (f64, f64), alpha: f64) {
        #[cfg(feature = "wayland")]
        if let (Some(p), Some(cr)) = (self.pattern.get(), context()) {
            unsafe {
                cairo::cairo_save(cr);
                cairo::cairo_translate(cr, origin.0 + offset.0, origin.1 + offset.1);
                cairo::cairo_scale(cr, scale.0.max(0.001), scale.1.max(0.001));
                cairo::cairo_translate(cr, -origin.0, -origin.1);
                cairo::cairo_set_source(cr, p);
                cairo::cairo_paint_with_alpha(cr, alpha.clamp(0.0, 1.0));
                cairo::cairo_restore(cr);
            }
        }
        let _ = (origin, scale, offset, alpha);
    }

    /// The rectangle recorded.
    pub fn rect(&self) -> (i32, i32, i32, i32) {
        self.rect.get()
    }

    /// Frees the picture (it holds a copy of the pixels).
    pub fn clear(&self) {
        #[cfg(feature = "wayland")]
        if let Some(p) = self.pattern.take() {
            unsafe { cairo::cairo_pattern_destroy(p) };
        }
    }
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        self.clear();
    }
}

/// Where a rectangle `(x, y, w, h)` ends up when scaled by `scale` around
/// `origin` and moved by `offset`, grown to whole pixels (plus one, for
/// smoothing): the area to repaint for a frame.
pub fn bounds(rect: (i32, i32, i32, i32), origin: (f64, f64), scale: (f64, f64), offset: (f64, f64)) -> (i32, i32, i32, i32) {
    let (x, y, w, h) = rect;
    let tx = |v: f64| origin.0 + offset.0 + (v - origin.0) * scale.0;
    let ty = |v: f64| origin.1 + offset.1 + (v - origin.1) * scale.1;
    let (x0, x1) = (tx(x as f64).floor() as i32 - 1, tx((x + w) as f64).ceil() as i32 + 1);
    let (y0, y1) = (ty(y as f64).floor() as i32 - 1, ty((y + h) as f64).ceil() as i32 + 1);
    (x0, y0, x1 - x0, y1 - y0)
}

/// The smallest rectangle holding both.
pub fn union(a: (i32, i32, i32, i32), b: (i32, i32, i32, i32)) -> (i32, i32, i32, i32) {
    let (x0, y0) = (a.0.min(b.0), a.1.min(b.1));
    let (x1, y1) = ((a.0 + a.2).max(b.0 + b.2), (a.1 + a.3).max(b.1 + b.3));
    (x0, y0, x1 - x0, y1 - y0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_scale_around_origin() {
        let r = (100, 100, 200, 100);
        assert_eq!(bounds(r, (200.0, 150.0), (1.0, 1.0), (0.0, 0.0)), (99, 99, 202, 102));
        assert_eq!(bounds(r, (200.0, 150.0), (0.5, 0.5), (0.0, 10.0)), (149, 134, 102, 52));
        assert_eq!(union((0, 0, 10, 10), (5, 5, 10, 10)), (0, 0, 15, 15));
    }
}
