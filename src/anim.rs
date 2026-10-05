//! Short, cheap animations. A running animation repaints only the widget
//! it belongs to, at most ~60 times a second, and only while it runs:
//! nothing ticks when nothing moves. With the theme's `animations = false`
//! (reduced motion, battery saving) every animation jumps to its end.

use std::cell::Cell;
use std::time::{Duration, Instant};

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(true) };
}

/// Set from the theme by [`crate::Theme::apply`].
pub(crate) fn set_enabled(on: bool) {
    ENABLED.with(|e| e.set(on));
}

/// Whether animations are on (the theme's `animations` key).
pub fn enabled() -> bool {
    ENABLED.with(|e| e.get())
}

/// Default length of UI transitions.
pub const SHORT: Duration = Duration::from_millis(150);

/// Calls `frame(t)` with `t` going from 0 to 1 over `duration` (eased
/// out), then once more with exactly 1.0. With animations off, only
/// `frame(1.0)` is called, right away. Repaint the widget in `frame`.
pub fn animate(duration: Duration, frame: impl FnMut(f64) + 'static) {
    animate_with(duration, ease_out, frame)
}

/// Like [`animate`] with another easing curve, e.g. [`linear`] for values
/// that keep moving (a progress bar fed regular updates).
pub fn animate_with(duration: Duration, ease: fn(f64) -> f64, mut frame: impl FnMut(f64) + 'static) {
    if !enabled() || duration.is_zero() {
        frame(1.0);
        return;
    }
    let start = Instant::now();
    frame(0.0);
    fltk::app::add_timeout3(1.0 / 60.0, move |handle| {
        let t = (start.elapsed().as_secs_f64() / duration.as_secs_f64()).min(1.0);
        frame(ease(t));
        if t < 1.0 {
            fltk::app::repeat_timeout3(1.0 / 60.0, handle);
        }
    });
}

/// A value that moves smoothly to new targets: what a widget draws (a
/// knob position, a bar's fill) while its state changes. Starting a new
/// move cancels the running one, so quick changes never fight.
#[derive(Clone)]
pub struct Tween(std::rc::Rc<TweenInner>);

struct TweenInner {
    value: Cell<f64>,
    /// Bumped by every move; stale animation frames see a different one.
    generation: Cell<u32>,
}

impl Tween {
    pub fn new(value: f64) -> Tween {
        Tween(std::rc::Rc::new(TweenInner { value: Cell::new(value), generation: Cell::new(0) }))
    }

    /// The value to draw now.
    pub fn get(&self) -> f64 {
        self.0.value.get()
    }

    /// Jumps to `value`, cancelling any running move.
    pub fn set(&self, value: f64) {
        self.0.generation.set(self.0.generation.get().wrapping_add(1));
        self.0.value.set(value);
    }

    /// Moves from the current value to `target` over `duration`, calling
    /// `redraw` on every frame (it should repaint the widget).
    pub fn animate_to(&self, target: f64, duration: Duration, redraw: impl FnMut() + 'static) {
        self.move_to(target, duration, ease_out, redraw)
    }

    /// Like [`Tween::animate_to`] at constant speed: for targets that keep
    /// changing (a new move starts where the last one was, so successive
    /// moves join into one continuous motion instead of pulsing).
    pub fn follow(&self, target: f64, duration: Duration, redraw: impl FnMut() + 'static) {
        self.move_to(target, duration, linear, redraw)
    }

    /// Like [`Tween::animate_to`] with another easing curve (e.g.
    /// [`snappy`] to open, [`ease_in`] to close).
    pub fn animate_ease(&self, target: f64, duration: Duration, ease: fn(f64) -> f64, redraw: impl FnMut() + 'static) {
        self.move_to(target, duration, ease, redraw)
    }

    fn move_to(&self, target: f64, duration: Duration, ease: fn(f64) -> f64, mut redraw: impl FnMut() + 'static) {
        let generation = self.0.generation.get().wrapping_add(1);
        self.0.generation.set(generation);
        let (from, me) = (self.get(), self.clone());
        animate_with(duration, ease, move |t| {
            if me.0.generation.get() == generation {
                me.0.value.set(from + (target - from) * t);
                redraw();
            }
        });
    }
}

/// Constant speed.
pub fn linear(t: f64) -> f64 {
    t
}

/// Cubic ease-out: fast start, gentle stop.
pub fn ease_out(t: f64) -> f64 {
    1.0 - (1.0 - t).powi(3)
}

/// Quadratic ease-in: slow start, fast end (things leaving).
pub fn ease_in(t: f64) -> f64 {
    t * t
}

/// Quick but visible from the first frame, overshooting a little and
/// settling: things appearing.
pub fn snappy(t: f64) -> f64 {
    cubic_bezier((0.3, 0.7), (0.25, 1.1), t)
}

/// A CSS-style cubic bezier easing curve through (0, 0), `p1`, `p2` and
/// (1, 1), at time `t`.
pub fn cubic_bezier(p1: (f64, f64), p2: (f64, f64), t: f64) -> f64 {
    if t <= 0.0 || t >= 1.0 {
        return t.clamp(0.0, 1.0);
    }
    let b = |a: f64, b: f64, s: f64| 3.0 * a * s * (1.0 - s) * (1.0 - s) + 3.0 * b * s * s * (1.0 - s) + s * s * s;
    // Find s with x(s) = t (x is increasing), by bisection.
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..24 {
        let mid = (lo + hi) / 2.0;
        if b(p1.0, p2.0, mid) < t {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    b(p1.1, p2.1, (lo + hi) / 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easing_ends() {
        assert_eq!(ease_out(0.0), 0.0);
        assert_eq!(ease_out(1.0), 1.0);
        assert!(ease_out(0.5) > 0.5);
        assert!(snappy(0.0).abs() < 1e-4 && (snappy(1.0) - 1.0).abs() < 1e-4);
        assert!(snappy(0.3) > 0.5 && snappy(0.06) < 0.3 && (1..100).any(|i| snappy(i as f64 / 100.0) > 1.0));
        assert_eq!(ease_in(1.0), 1.0);
    }

    #[test]
    fn tween_set_and_disabled_move() {
        set_enabled(false);
        let t = Tween::new(0.0);
        t.animate_to(1.0, SHORT, || {});
        assert_eq!(t.get(), 1.0);
        t.set(0.25);
        assert_eq!(t.get(), 0.25);
        set_enabled(true);
    }

    #[test]
    fn disabled_jumps_to_end() {
        set_enabled(false);
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let s = seen.clone();
        animate(SHORT, move |t| s.borrow_mut().push(t));
        assert_eq!(*seen.borrow(), [1.0]);
        set_enabled(true);
    }
}
