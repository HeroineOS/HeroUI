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
        pub fn cairo_new_path(cr: *mut c_void);
        pub fn cairo_new_sub_path(cr: *mut c_void);
        pub fn cairo_arc(cr: *mut c_void, xc: c_double, yc: c_double, r: c_double, a1: c_double, a2: c_double);
        pub fn cairo_close_path(cr: *mut c_void);
        pub fn cairo_fill(cr: *mut c_void);
        pub fn cairo_set_source_rgba(cr: *mut c_void, r: c_double, g: c_double, b: c_double, a: c_double);
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

/// Fills a rounded rectangle at fractional coordinates (window
/// coordinates, like FLTK's), anti-aliased, with opacity `alpha`: things
/// that move glide between pixels instead of stepping. Off Wayland it's
/// FLTK's whole-pixel rounded rectangle (and `alpha` is ignored).
pub fn fill_rounded(x: f64, y: f64, w: f64, h: f64, r: f64, color: fltk::enums::Color, alpha: f64) {
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    #[cfg(feature = "wayland")]
    if let Some(cr) = context() {
        use std::f64::consts::{FRAC_PI_2, PI};
        let (cr_, cg, cb) = color.to_rgb();
        // FLTK's cairo user space is offset by half a pixel.
        let (x, y) = (x - 0.5, y - 0.5);
        unsafe {
            cairo::cairo_new_path(cr);
            cairo::cairo_new_sub_path(cr);
            cairo::cairo_arc(cr, x + w - r, y + r, r, -FRAC_PI_2, 0.0);
            cairo::cairo_arc(cr, x + w - r, y + h - r, r, 0.0, FRAC_PI_2);
            cairo::cairo_arc(cr, x + r, y + h - r, r, FRAC_PI_2, PI);
            cairo::cairo_arc(cr, x + r, y + r, r, PI, 1.5 * PI);
            cairo::cairo_close_path(cr);
            cairo::cairo_set_source_rgba(cr, cr_ as f64 / 255.0, cg as f64 / 255.0, cb as f64 / 255.0, alpha.clamp(0.0, 1.0));
            cairo::cairo_fill(cr);
        }
        // FLTK sets its own color before each of its drawings.
        return;
    }
    let _ = alpha;
    fltk::draw::set_draw_color(color);
    fltk::draw::draw_rounded_rectf(x.round() as i32, y.round() as i32, w.round() as i32, h.round() as i32, r.round() as i32);
}

/// A filled circle at fractional coordinates (see [`fill_rounded`]).
pub fn fill_circle(cx: f64, cy: f64, radius: f64, color: fltk::enums::Color, alpha: f64) {
    fill_rounded(cx - radius, cy - radius, 2.0 * radius, 2.0 * radius, radius, color, alpha);
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

/// Makes a panel drawn inside a window (an in-window popover: an overlay
/// group that paints a panel and holds its widgets) pop in when shown and
/// out when closed: it grows from the edge next to what opened it, fading
/// in, and settles with a slight overshoot. On Wayland; elsewhere it just
/// appears. Each frame repaints only the area the panel covers.
#[derive(Clone)]
pub struct PopIn(std::rc::Rc<PopInner>);

struct PopInner {
    t: crate::anim::Tween,
    snap: Snapshot,
    closing: Cell<bool>,
    painted: Cell<(i32, i32, i32, i32)>,
    group: std::cell::RefCell<Option<fltk::group::Group>>,
    from_bottom: bool,
}

type Rect = (i32, i32, i32, i32);
/// (point to scale around, scale, offset, opacity)
type Look = ((f64, f64), (f64, f64), (f64, f64), f64);

impl PopInner {
    /// (point it grows from, scale, offset, opacity) at `v` (0 hidden, 1 shown).
    fn shape(&self, r: Rect, v: f64) -> Look {
        let origin = (r.0 as f64 + r.2 as f64 / 2.0, if self.from_bottom { (r.1 + r.3) as f64 } else { r.1 as f64 });
        (origin, (0.94 + 0.06 * v, 0.9 + 0.1 * v), (0.0, 0.0), (v * 1.5).clamp(0.0, 1.0))
    }
}

impl PopIn {
    /// Takes over `g`'s drawing: `paint(g)` draws the panel (in window
    /// coordinates), then the children are drawn; `rect()` is the panel's
    /// rectangle and `from_bottom` whether it grows upward (it opened above
    /// its anchor).
    pub fn attach(
        g: &mut fltk::group::Group,
        rect: impl Fn() -> (i32, i32, i32, i32) + 'static,
        from_bottom: bool,
        mut paint: impl FnMut(&mut fltk::group::Group) + 'static,
    ) -> PopIn {
        use fltk::prelude::*;
        let p = PopIn(std::rc::Rc::new(PopInner {
            t: crate::anim::Tween::new(0.0),
            snap: Snapshot::new(),
            closing: Cell::new(false),
            painted: Cell::new((0, 0, 0, 0)),
            group: std::cell::RefCell::new(Some(g.clone())),
            from_bottom,
        }));
        let rect = std::rc::Rc::new(rect);
        g.super_draw(false);
        {
            let (p, rect) = (p.clone(), rect.clone());
            g.draw(move |g| {
                let v = p.0.t.get();
                if v != 1.0 {
                    let r = grow(rect());
                    let mut g2 = g.clone();
                    draw_no_clip(|| {
                        p.0.snap.record(r, || {
                            paint(&mut g2);
                            g2.draw_children();
                        })
                    });
                    if p.0.snap.is_recorded() {
                        let (o, s, d, a) = p.0.shape(r, v);
                        p.0.snap.paint(o, s, d, a);
                        p.0.snap.clear();
                        return;
                    }
                }
                paint(g);
                g.draw_children();
            });
        }
        if crate::anim::enabled() && context_possible() {
            let p2 = p.clone();
            p.0.t.animate_ease(1.0, std::time::Duration::from_millis(240), crate::anim::snappy, move || p2.frame(&*rect));
        } else {
            p.0.t.set(1.0);
        }
        p
    }

    /// Repaints where the panel was and is.
    fn frame(&self, rect: &dyn Fn() -> Rect) {
        use fltk::prelude::*;
        let Some(g) = self.0.group.borrow().clone() else { return };
        if g.was_deleted() {
            return;
        }
        let r = grow(rect());
        let (o, s, d, _) = self.0.shape(r, self.0.t.get().max(1.0));
        let now = bounds(r, o, s, d);
        let (x, y, w, h) = union(self.0.painted.replace(now), now);
        if let Some(mut win) = g.window() {
            win.set_damage_area(fltk::enums::Damage::All, x, y, w, h);
        }
    }

    /// Whether it's going away (ignore input then).
    pub fn closing(&self) -> bool {
        self.0.closing.get()
    }

    /// Pops out, then calls `done` (which deletes the overlay). At once
    /// where it can't animate.
    pub fn close(&self, rect: impl Fn() -> (i32, i32, i32, i32) + 'static, done: impl FnOnce() + 'static) {
        if self.0.closing.replace(true) {
            return;
        }
        if !crate::anim::enabled() || !context_possible() {
            done();
            return;
        }
        let (p, mut done) = (self.clone(), Some(done));
        self.0.t.animate_ease(0.0, std::time::Duration::from_millis(130), crate::anim::ease_in, move || {
            p.frame(&rect);
            if p.0.t.get() == 0.0 {
                if let Some(d) = done.take() {
                    d();
                }
            }
        });
    }
}

/// A panel's rectangle with room for its shadow.
fn grow((x, y, w, h): Rect) -> Rect {
    (x - 6, y - 6, w + 12, h + 12)
}

/// Runs `f` with FLTK's clipping lifted (to record a whole panel while
/// only part of the window is being repainted).
fn draw_no_clip<R>(f: impl FnOnce() -> R) -> R {
    fltk::draw::push_no_clip();
    let r = f();
    fltk::draw::pop_clip();
    r
}

/// Whether drawing goes through cairo (Wayland), where effects work.
fn context_possible() -> bool {
    cfg!(feature = "wayland") && crate::on_wayland()
}
