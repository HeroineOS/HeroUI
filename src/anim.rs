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
pub fn animate(duration: Duration, mut frame: impl FnMut(f64) + 'static) {
    if !enabled() || duration.is_zero() {
        frame(1.0);
        return;
    }
    let start = Instant::now();
    frame(0.0);
    fltk::app::add_timeout3(1.0 / 60.0, move |handle| {
        let t = (start.elapsed().as_secs_f64() / duration.as_secs_f64()).min(1.0);
        frame(ease_out(t));
        if t < 1.0 {
            fltk::app::repeat_timeout3(1.0 / 60.0, handle);
        }
    });
}

/// Cubic ease-out: fast start, gentle stop.
pub fn ease_out(t: f64) -> f64 {
    1.0 - (1.0 - t).powi(3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easing_ends() {
        assert_eq!(ease_out(0.0), 0.0);
        assert_eq!(ease_out(1.0), 1.0);
        assert!(ease_out(0.5) > 0.5);
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
