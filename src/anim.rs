//! Short, cheap animations. One frame clock drives every running
//! animation of the app, so all of them step together, once per frame,
//! and a frame repaints only the widgets that move. On Wayland the frames
//! are paced by the display (a step right after each shown frame);
//! elsewhere by a timer at the theme's `frame_rate`. Nothing ticks when nothing moves. With the theme's
//! `animations = false` (reduced motion, battery saving) every animation
//! jumps to its end.
//!
//! Two kinds of motion: timed curves ([`animate_with`], [`Tween::animate_to`])
//! for things that appear and leave, and springs ([`Tween::spring_to`]) for
//! things that follow the user: a spring keeps its speed when it gets a
//! new target mid-move, so fast changes flow instead of restarting.

use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(true) };
    static FRAME_RATE: Cell<u32> = const { Cell::new(60) };
    /// Running animations: each is called once per frame with the frame's
    /// time and returns whether it continues.
    static JOBS: RefCell<Vec<Job>> = const { RefCell::new(Vec::new()) };
    static TICKING: Cell<bool> = const { Cell::new(false) };
}

type Job = Box<dyn FnMut(Instant) -> bool>;

/// Set from the theme by [`crate::Theme::apply`].
pub(crate) fn set_enabled(on: bool) {
    ENABLED.with(|e| e.set(on));
}

/// Set from the theme by [`crate::Theme::apply`].
pub(crate) fn set_frame_rate(fps: i32) {
    FRAME_RATE.with(|f| f.set(fps.clamp(24, 360) as u32));
}

/// Whether animations are on (the theme's `animations` key).
pub fn enabled() -> bool {
    ENABLED.with(|e| e.get())
}

/// Slow motion for checking animations: `HEROUI_SLOW=10` runs them all
/// ten times slower.
fn slow() -> f64 {
    thread_local!(static SLOW: f64 = std::env::var("HEROUI_SLOW").ok().and_then(|v| v.parse().ok()).filter(|v: &f64| *v >= 1.0).unwrap_or(1.0));
    SLOW.with(|s| *s)
}

fn frame_interval() -> f64 {
    1.0 / FRAME_RATE.with(Cell::get) as f64
}

/// Adds `job` to the frame clock (starting it if it's idle). The job runs
/// from the next frame on until it returns false.
fn schedule(job: Job) {
    JOBS.with(|j| j.borrow_mut().push(job));
    pace_by_display();
    if !TICKING.with(|t| t.replace(true)) {
        fltk::app::add_timeout3(frame_interval(), timer_tick);
    }
}

thread_local! {
    /// When the jobs last ran.
    static LAST_TICK: Cell<Option<Instant>> = const { Cell::new(None) };
    static RUNNING: Cell<bool> = const { Cell::new(false) };
    static PACED: Cell<bool> = const { Cell::new(false) };
    static DEFERRED: Cell<bool> = const { Cell::new(false) };
}

/// On Wayland, steps animations when the compositor reports a shown frame
/// (once per display refresh), not on a timer: a timer drifts against the
/// refresh, so some frames would show a step that's nearly a frame old and
/// others a fresh one, and motion looks uneven even at 60 Hz.
fn pace_by_display() {
    #[cfg(feature = "layer-shell")]
    if !PACED.with(|p| p.replace(true)) && crate::on_wayland() {
        unsafe { fltk_sys::window::Fl_wl_frame_hook(Some(frame_shown)) };
    }
}

#[cfg(feature = "layer-shell")]
unsafe extern "C" fn frame_shown() {
    if !TICKING.with(Cell::get) {
        return;
    }
    let now = Instant::now();
    let since = LAST_TICK.with(Cell::get).map_or(f64::MAX, |t| now.duration_since(t).as_secs_f64());
    if since >= frame_interval() * 0.5 {
        run_jobs(now);
    } else if !DEFERRED.with(|d| d.replace(true)) {
        // Early: another window reporting the same refresh, or a
        // compositor that reports frames at once (no vsync). Step when the
        // frame is due instead.
        fltk::app::add_timeout3(frame_interval() - since, |_| {
            DEFERRED.with(|d| d.set(false));
            if TICKING.with(Cell::get) {
                let now = Instant::now();
                if LAST_TICK.with(Cell::get).is_none_or(|t| now.duration_since(t).as_secs_f64() >= frame_interval() * 0.5) {
                    run_jobs(now);
                }
            }
        });
    }
}

/// The timer: the clock off Wayland, and the fallback when no frames are
/// being shown (nothing has been drawn yet, or the window is hidden).
fn timer_tick(handle: fltk::app::TimeoutHandle) {
    let now = Instant::now();
    let paced = PACED.with(Cell::get)
        && LAST_TICK.with(Cell::get).is_some_and(|t| now.duration_since(t).as_secs_f64() < frame_interval() * 1.5);
    let more = if paced { JOBS.with(|j| !j.borrow().is_empty()) } else { run_jobs(now) };
    if more {
        fltk::app::repeat_timeout3(frame_interval(), handle);
    } else {
        TICKING.with(|t| t.set(false));
    }
}

/// Runs every job once; whether any continue.
fn run_jobs(now: Instant) -> bool {
    if RUNNING.with(|r| r.replace(true)) {
        return true;
    }
    LAST_TICK.with(|t| t.set(Some(now)));
    // Taken out while they run: a job may start other animations.
    let mut jobs = JOBS.with(|j| std::mem::take(&mut *j.borrow_mut()));
    jobs.retain_mut(|job| job(now));
    let more = JOBS.with(|j| {
        let mut j = j.borrow_mut();
        jobs.append(&mut j);
        *j = jobs;
        !j.is_empty()
    });
    RUNNING.with(|r| r.set(false));
    more
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
    schedule(Box::new(move |now| {
        let t = (now.duration_since(start).as_secs_f64() / (duration.as_secs_f64() * slow())).min(1.0);
        frame(if t >= 1.0 { 1.0 } else { ease(t) });
        t < 1.0
    }));
}

/// Calls `frame(seconds since the last frame)` on every frame for as long
/// as it returns true: for motion computed step by step (momentum
/// scrolling). With animations off it isn't called.
pub fn each_frame(mut frame: impl FnMut(f64) -> bool + 'static) {
    if !enabled() {
        return;
    }
    let mut last = Instant::now();
    schedule(Box::new(move |now| {
        let dt = now.duration_since(last).as_secs_f64().min(0.1) / slow();
        last = now;
        frame(dt)
    }));
}

/// How a spring moves: `response` is roughly how long a move takes (in
/// seconds), `damping` 1.0 arrives without overshooting, lower bounces.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spring {
    pub response: f64,
    pub damping: f64,
}

impl Spring {
    /// Quick and settled, the default for things following the user
    /// (indicators, knobs, widths).
    pub const SNAPPY: Spring = Spring { response: 0.28, damping: 0.86 };
    /// A visible bounce (things popping in).
    pub const BOUNCY: Spring = Spring { response: 0.38, damping: 0.62 };
    /// Slow and soft, no overshoot (large areas, scrolling).
    pub const SMOOTH: Spring = Spring { response: 0.32, damping: 1.0 };

    /// Stiffness and damping for a unit mass.
    fn constants(self) -> (f64, f64) {
        let w = 2.0 * std::f64::consts::PI / self.response.max(0.01);
        (w * w, 2.0 * self.damping * w)
    }

    /// One step of `dt` seconds from (position, velocity) toward `target`
    /// (semi-implicit Euler in small sub-steps: stable at any frame rate).
    pub fn step(self, (mut x, mut v): (f64, f64), target: f64, dt: f64) -> (f64, f64) {
        let (k, c) = self.constants();
        let n = (dt / (1.0 / 240.0)).ceil().max(1.0);
        let h = dt / n;
        for _ in 0..n as u32 {
            v += (-k * (x - target) - c * v) * h;
            x += v * h;
        }
        (x, v)
    }
}

/// A value that moves smoothly to new targets: what a widget draws (a
/// knob position, a bar's fill) while its state changes. Starting a new
/// timed move cancels the running one; a new spring target keeps the
/// current speed.
#[derive(Clone)]
pub struct Tween(std::rc::Rc<TweenInner>);

struct TweenInner {
    value: Cell<f64>,
    /// Units per second, kept up to date by every kind of move.
    velocity: Cell<f64>,
    /// Bumped by every move; stale animation frames see a different one.
    generation: Cell<u32>,
    /// While a spring runs: its target and kind (a new target joins it).
    spring: Cell<Option<(f64, Spring)>>,
    /// The size of the move, for deciding when a spring has come to rest.
    span: Cell<f64>,
}

impl Default for Tween {
    fn default() -> Self {
        Tween::new(0.0)
    }
}

impl Tween {
    pub fn new(value: f64) -> Tween {
        Tween(std::rc::Rc::new(TweenInner {
            value: Cell::new(value),
            velocity: Cell::new(0.0),
            generation: Cell::new(0),
            spring: Cell::new(None),
            span: Cell::new(0.0),
        }))
    }

    /// The value to draw now.
    pub fn get(&self) -> f64 {
        self.0.value.get()
    }

    /// How fast it's moving (units per second; 0 at rest). For effects
    /// that follow speed (a knob stretching as it slides).
    pub fn velocity(&self) -> f64 {
        self.0.velocity.get()
    }

    /// Where it's going (the value itself when at rest or timed).
    pub fn target(&self) -> f64 {
        self.0.spring.get().map_or(self.get(), |(t, _)| t)
    }

    /// Jumps to `value`, cancelling any running move.
    pub fn set(&self, value: f64) {
        self.0.generation.set(self.0.generation.get().wrapping_add(1));
        self.0.spring.set(None);
        self.0.value.set(value);
        self.0.velocity.set(0.0);
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
        self.0.spring.set(None);
        let (from, me) = (self.get(), self.clone());
        let mut last = (Instant::now(), from);
        animate_with(duration, ease, move |t| {
            if me.0.generation.get() == generation {
                let v = from + (target - from) * t;
                let now = Instant::now();
                let dt = now.duration_since(last.0).as_secs_f64();
                me.0.velocity.set(if t >= 1.0 { 0.0 } else if dt > 0.0 { (v - last.1) / dt } else { me.0.velocity.get() });
                last = (now, v);
                me.0.value.set(v);
                redraw();
            }
        });
    }

    /// Springs toward `target`, calling `redraw` on every frame. Called
    /// again while it moves, it only changes the target (and spring): the
    /// motion carries its speed into the new direction.
    pub fn spring_to(&self, target: f64, spring: Spring, mut redraw: impl FnMut() + 'static) {
        if !enabled() {
            self.set(target);
            redraw();
            return;
        }
        let span = (target - self.get()).abs();
        if self.0.spring.replace(Some((target, spring))).is_some() {
            // Joined the running spring.
            self.0.span.set(self.0.span.get().max(span));
            return;
        }
        if span == 0.0 && self.velocity() == 0.0 {
            self.0.spring.set(None);
            return;
        }
        self.0.span.set(span);
        let generation = self.0.generation.get().wrapping_add(1);
        self.0.generation.set(generation);
        let me = self.clone();
        each_frame(move |dt| {
            if me.0.generation.get() != generation {
                return false;
            }
            let Some((target, spring)) = me.0.spring.get() else { return false };
            let (x, v) = spring.step((me.get(), me.velocity()), target, dt);
            // At rest: within a thousandth of the move, barely moving.
            let eps = (me.0.span.get() * 0.001).max(1e-4);
            let rest = (x - target).abs() < eps && v.abs() < eps * 10.0;
            me.0.value.set(if rest { target } else { x });
            me.0.velocity.set(if rest { 0.0 } else { v });
            if rest {
                me.0.spring.set(None);
            }
            redraw();
            !rest
        });
    }
}

/// Smooth scrolling for widgets that scroll their own drawing (custom
/// lists and grids): wheel steps glide (and add up while gliding), and a
/// drag released while moving keeps going, slowing like a sliding sheet.
/// The widget draws at [`Scroller::pos`] and calls `redraw` when told.
#[derive(Clone)]
pub struct Scroller {
    pos: Tween,
    /// Recent drag positions (time, content position).
    trail: std::rc::Rc<RefCell<Vec<(Instant, f64)>>>,
    /// Bumped to stop a flick.
    flick: std::rc::Rc<Cell<u32>>,
}

impl Default for Scroller {
    fn default() -> Self {
        Scroller::new(0.0)
    }
}

impl Scroller {
    pub fn new(pos: f64) -> Scroller {
        Scroller { pos: Tween::new(pos), trail: Default::default(), flick: Default::default() }
    }

    /// Where the content is scrolled to now (whole pixels).
    pub fn pos(&self) -> i32 {
        self.pos.get().round() as i32
    }

    /// Jumps there (content changed, scrolled into view), stopping motion.
    pub fn set(&self, pos: f64) {
        self.flick.set(self.flick.get().wrapping_add(1));
        self.pos.set(pos);
    }

    /// A wheel step of `delta` pixels, kept within 0..=`max`.
    pub fn wheel(&self, delta: f64, max: f64, redraw: impl FnMut() + 'static) {
        self.flick.set(self.flick.get().wrapping_add(1));
        let target = (self.pos.target() + delta).clamp(0.0, max.max(0.0));
        self.pos.spring_to(target, Spring { response: 0.22, damping: 1.0 }, redraw);
    }

    /// Glides to `pos` (within 0..=`max`): bringing something into view.
    pub fn scroll_to(&self, pos: f64, max: f64, redraw: impl FnMut() + 'static) {
        self.flick.set(self.flick.get().wrapping_add(1));
        self.pos.spring_to(pos.clamp(0.0, max.max(0.0)), Spring { response: 0.26, damping: 1.0 }, redraw);
    }

    /// Where it's heading (its position when still).
    pub fn target(&self) -> f64 {
        self.pos.target()
    }

    /// A finger or button went down: stops any motion.
    pub fn press(&self) {
        self.set(self.pos.get());
        self.trail.borrow_mut().clear();
    }

    /// Dragging: the content follows to `pos` (within 0..=`max`).
    pub fn drag_to(&self, pos: f64, max: f64) {
        let pos = pos.clamp(0.0, max.max(0.0));
        self.pos.set(pos);
        let mut t = self.trail.borrow_mut();
        t.push((Instant::now(), pos));
        if t.len() > 6 {
            t.remove(0);
        }
    }

    /// The drag ended: keeps the last ~100 ms' speed, slowing down to a
    /// stop (or an end), repainting with `redraw`.
    pub fn release(&self, max: f64, mut redraw: impl FnMut() + 'static) {
        let now = Instant::now();
        let t = std::mem::take(&mut *self.trail.borrow_mut());
        let recent: Vec<_> = t.iter().filter(|(at, _)| now.duration_since(*at).as_secs_f64() < 0.1).collect();
        let (Some(a), Some(b)) = (recent.first(), recent.last()) else { return };
        let dt = b.0.duration_since(a.0).as_secs_f64();
        if dt < 0.01 {
            return;
        }
        let mut speed = (b.1 - a.1) / dt;
        if speed.abs() < 120.0 {
            return;
        }
        let generation = self.flick.get().wrapping_add(1);
        self.flick.set(generation);
        let me = self.clone();
        each_frame(move |dt| {
            if me.flick.get() != generation {
                return false;
            }
            // Friction: loses ~95% of its speed per second.
            let pos = (me.pos.get() + speed * dt).clamp(0.0, max.max(0.0));
            speed *= (-3.0 * dt).exp();
            me.pos.0.value.set(pos);
            redraw();
            !(pos <= 0.0 || pos >= max || speed.abs() < 20.0)
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

/// Quintic ease-out: a fast start and a long soft landing (fades, things
/// gliding into place).
pub fn ease_out_quint(t: f64) -> f64 {
    1.0 - (1.0 - t).powi(5)
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

/// Things appearing, the soft way: moves most of the way early, then a
/// long gentle landing with a hint of overshoot (more frames in the part
/// the eye follows, so it reads smoother at 60 Hz).
pub fn glide(t: f64) -> f64 {
    cubic_bezier((0.2, 0.9), (0.25, 1.04), t)
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
        assert_eq!(ease_out_quint(1.0), 1.0);
    }

    #[test]
    fn tween_set_and_disabled_move() {
        set_enabled(false);
        let t = Tween::new(0.0);
        t.animate_to(1.0, SHORT, || {});
        assert_eq!(t.get(), 1.0);
        t.spring_to(3.0, Spring::SNAPPY, || {});
        assert_eq!(t.get(), 3.0);
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

    /// Springs settle on the target in about their response time, a
    /// bouncy one overshooting first, a damped one not; at any frame rate.
    #[test]
    fn springs_settle() {
        for fps in [30.0, 60.0, 144.0] {
            for (s, bounces) in [(Spring::SNAPPY, false), (Spring::BOUNCY, true), (Spring::SMOOTH, false)] {
                let (mut x, mut v, mut max) = (0.0, 0.0, 0.0f64);
                let mut t = 0.0;
                while t < 1.5 {
                    (x, v) = s.step((x, v), 100.0, 1.0 / fps);
                    max = max.max(x);
                    t += 1.0 / fps;
                }
                assert!((x - 100.0).abs() < 0.5 && v.abs() < 5.0, "{s:?} at {fps}: {x} {v}");
                assert_eq!(max > 101.0, bounces, "{s:?} at {fps}: max {max}");
            }
        }
    }
}
