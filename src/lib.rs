//! # HeroUI
//!
//! A lightweight Elm-style layer over [fltk-rs](https://github.com/fltk-rs/fltk-rs)
//! for HeroineOS apps.
//!
//! - **State** lives in your `App` struct.
//! - **Messages** are an enum; widgets send them, [`App::update`] handles them.
//! - **View** is built once. State-dependent parts are bindings that update
//!   only the widgets whose value changed; nothing is rebuilt or diffed.
//! - **Reusable components** are functions returning `Element<TheirState,
//!   TheirMsg>`, plugged into any app with [`embed`].
//! - No async runtime by default: [`Task::perform`] runs blocking work on a
//!   short-lived thread. Optional `tokio` feature for async-heavy apps.
//!
//! ```no_run
//! use heroui::prelude::*;
//!
//! #[derive(Default)]
//! struct Counter { value: i32 }
//!
//! #[derive(Clone)]
//! enum Msg { Inc, Dec }
//!
//! impl App for Counter {
//!     type Message = Msg;
//!     fn update(&mut self, msg: Msg) -> Task<Msg> {
//!         match msg { Msg::Inc => self.value += 1, Msg::Dec => self.value -= 1 }
//!         Task::none()
//!     }
//!     fn view(&self) -> Element<Self, Msg> {
//!         column(vec![
//!             text(|s: &Counter| s.value.to_string()),
//!             row(vec![button("-", Msg::Dec), button("+", Msg::Inc)]).fixed(36),
//!         ])
//!     }
//! }
//!
//! fn main() {
//!     heroui::run(Counter::default(), Settings::new("Counter").size(240, 120)).unwrap();
//! }
//! ```

pub mod anim;
pub mod drag_scroll;
mod element;
pub mod hover;
mod popup;
mod task;
pub mod theme;
pub mod widgets;
#[cfg(all(unix, not(target_os = "macos")))]
mod x11;
#[cfg(feature = "wayland")]
mod wayland;

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::mpsc;

use fltk::prelude::*;
use fltk::window::Window;

pub use element::{embed, relayout_parent, Binding, Ctx, Element};
pub use fltk;
pub use task::{Subscription, Task};
pub use theme::Theme;

pub mod prelude {
    pub use crate::widgets::*;
    pub use crate::{embed, App, Ctx, Edge, Element, Settings, Subscription, Task, Theme, WindowKind};
}

/// An application: its state is `Self`.
pub trait App: Sized + 'static {
    type Message: Clone + Send + 'static;

    /// Handles one message. Change state here; return a [`Task`] for side
    /// effects (background work, follow-up messages, quitting).
    fn update(&mut self, message: Self::Message) -> Task<Self::Message>;

    /// Describes the UI. Called once at startup.
    fn view(&self) -> Element<Self, Self::Message>;

    /// Runs once after the window is shown, e.g. to start loading data in
    /// the background. Its task is handled like one returned by `update`.
    fn init(&mut self) -> Task<Self::Message> {
        Task::none()
    }

    /// Timers etc. Called once at startup.
    fn subscriptions(&self) -> Vec<Subscription<Self::Message>> {
        Vec::new()
    }

    /// The user asked to close the window (close button, Alt+F4). `None`
    /// (default) closes it. Return a message to handle it in `update`
    /// instead, e.g. to confirm unsaved changes; return `Task::quit()`
    /// from there to really close. Escape never closes a HeroUI window.
    fn close_requested(&self) -> Option<Self::Message> {
        None
    }

    /// Defaults to the user's theme file ([`Theme::load`]).
    fn theme(&self) -> Theme {
        Theme::load()
    }
}

/// Window settings for [`run`].
#[derive(Debug, Clone)]
pub struct Settings {
    pub title: String,
    pub size: (i32, i32),
    pub position: Option<(i32, i32)>,
    pub resizable: bool,
    /// False for borderless windows (panels, docks, popups).
    pub decorated: bool,
    /// Window class, used by compositors/WMs for rules.
    pub class: Option<String>,
    /// What the window is for; tells the window manager how to treat it.
    pub kind: WindowKind,
    /// Keep above normal windows (OSDs, popups).
    pub above: bool,
    /// Keep below normal windows (desktop widgets).
    pub below: bool,
    /// Show on every workspace.
    pub sticky: bool,
    /// Leave out of taskbars and pagers.
    pub skip_taskbar: bool,
    /// Reserve screen space along an edge so maximized windows don't cover
    /// this one (panels, docks).
    pub reserve: Option<(Edge, i32)>,
    /// Make the window span the whole reserved edge, whatever the screen
    /// size (set by [`Settings::panel`]).
    pub span: bool,
}

/// Window types from the EWMH spec. On X11 (and XWayland compositors that
/// honor it) the window manager places, stacks and decorates by this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowKind {
    #[default]
    Normal,
    /// Panels, docks, taskbars: undecorated, not focused on click, above
    /// normal windows. Combine with [`Settings::reserve`].
    Dock,
    /// Desktop widgets (conky-like): kept below everything, no taskbar entry.
    Desktop,
    Dialog,
    /// Tool palettes and small helper windows.
    Utility,
    /// Notifications and OSDs.
    Notification,
}

/// A screen edge, for [`Settings::reserve`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

impl Settings {
    pub fn new(title: &str) -> Self {
        Self {
            title: title.to_string(),
            size: (480, 320),
            position: None,
            resizable: true,
            decorated: true,
            class: None,
            kind: WindowKind::Normal,
            above: false,
            below: false,
            sticky: false,
            skip_taskbar: false,
            reserve: None,
            span: false,
        }
    }

    /// A panel/dock along `edge` of the screen, `thickness` px deep,
    /// spanning the whole edge: borderless, on every workspace, space
    /// reserved. A layer-shell surface on Wayland (feature `layer-shell`).
    pub fn panel(title: &str, edge: Edge, thickness: i32) -> Self {
        let mut s = Self::new(title)
            .size(thickness, thickness)
            .resizable(false)
            .decorated(false)
            .kind(WindowKind::Dock)
            .sticky(true)
            .reserve(edge, thickness);
        s.span = true;
        s
    }

    /// A desktop widget (conky-like) at `x, y`: borderless, below other
    /// windows, on every workspace, not in the taskbar. A layer-shell
    /// surface on Wayland (feature `layer-shell`).
    pub fn desktop_widget(title: &str, x: i32, y: i32, w: i32, h: i32) -> Self {
        Self::new(title)
            .size(w, h)
            .position(x, y)
            .resizable(false)
            .decorated(false)
            .kind(WindowKind::Desktop)
            .below(true)
            .sticky(true)
            .skip_taskbar(true)
    }
    pub fn size(mut self, w: i32, h: i32) -> Self {
        self.size = (w, h);
        self
    }
    pub fn position(mut self, x: i32, y: i32) -> Self {
        self.position = Some((x, y));
        self
    }
    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }
    pub fn decorated(mut self, decorated: bool) -> Self {
        self.decorated = decorated;
        self
    }
    pub fn class(mut self, class: &str) -> Self {
        self.class = Some(class.to_string());
        self
    }
    pub fn kind(mut self, kind: WindowKind) -> Self {
        self.kind = kind;
        self
    }
    pub fn above(mut self, on: bool) -> Self {
        self.above = on;
        self
    }
    pub fn below(mut self, on: bool) -> Self {
        self.below = on;
        self
    }
    pub fn sticky(mut self, on: bool) -> Self {
        self.sticky = on;
        self
    }
    pub fn skip_taskbar(mut self, on: bool) -> Self {
        self.skip_taskbar = on;
        self
    }
    pub fn reserve(mut self, edge: Edge, px: i32) -> Self {
        self.reserve = Some((edge, px));
        self
    }
}

/// Opens the window and runs the app until it's closed or `Task::quit`.
pub fn run<A: App>(mut app: A, mut settings: Settings) -> Result<(), fltk::prelude::FltkError> {
    // Shell windows: pick the backend before FLTK opens its display.
    #[cfg(feature = "wayland")]
    let layer = wayland::prepare_backend(&settings);
    #[cfg(not(feature = "wayland"))]
    let layer = false;

    let fl = fltk::app::App::default();
    drag_scroll::install();
    let theme = Rc::new(app.theme());
    theme.apply();

    if settings.span && !layer {
        // On Wayland the compositor sizes a spanning panel; elsewhere use
        // the first screen.
        if let Some((edge, _)) = settings.reserve {
            // The panel's depth is its size across the edge (the reserved
            // space may be smaller, even 0).
            let px = match edge {
                Edge::Top | Edge::Bottom => settings.size.1,
                Edge::Left | Edge::Right => settings.size.0,
            };
            let (sx, sy, sw, sh) = fltk::app::screen_xywh(0);
            let (x, y, w, h) = match edge {
                Edge::Top => (sx, sy, sw, px),
                Edge::Bottom => (sx, sy + sh - px, sw, px),
                Edge::Left => (sx, sy, px, sh),
                Edge::Right => (sx + sw - px, sy, px, sh),
            };
            settings.size = (w, h);
            settings.position = Some((x, y));
        }
    }

    let (w, h) = settings.size;
    let mut win = Window::default().with_size(w, h).with_label(&settings.title);
    if let Some((x, y)) = settings.position.filter(|_| !layer) {
        win.set_pos(x, y);
    }
    // The X11 class / Wayland app_id compositors match rules on; default
    // to the executable's name rather than FLTK's generic "FLTK".
    let class = settings.class.clone().or_else(|| {
        std::env::current_exe().ok()?.file_stem()?.to_str().map(str::to_owned)
    });
    if let Some(class) = &class {
        win.set_xclass(class);
    }
    win.set_border(settings.decorated);
    win.set_color(theme.background);

    // Messages from widget callbacks (UI thread) queue here; results of
    // background work arrive over `rx` and wake the event loop.
    let queue: Rc<RefCell<VecDeque<A::Message>>> = Rc::default();
    let (tx, rx) = mpsc::channel::<A::Message>();
    let emit: Rc<dyn Fn(A::Message)> = {
        let q = queue.clone();
        Rc::new(move |m| q.borrow_mut().push_back(m))
    };

    let mut ctx = Ctx::new(emit.clone(), theme.clone());
    let root = app.view().build(&mut ctx);
    let mut bindings = ctx.into_bindings();
    let mut root = root;
    root.resize(0, 0, w, h);
    // The root always follows the window's size (the compositor sizes
    // panels; WMs may resize anything). "Not resizable" means the user
    // can't resize it, which a fixed size range says. A layer-shell
    // window gets its size from the compositor.
    win.resizable(&root);
    if !settings.resizable && !layer {
        win.size_range(w, h, w, h);
    }
    win.end();

    // FLTK's default window callback also closes on Escape; only react to
    // real close requests, and let the app decide.
    let close_requested = Rc::new(Cell::new(false));
    {
        let flag = close_requested.clone();
        win.set_callback(move |_| {
            if fltk::app::event() == fltk::enums::Event::Close {
                flag.set(true);
            }
        });
    }

    for sub in app.subscriptions() {
        match sub {
            Subscription::Every(interval, msg) => {
                let emit = emit.clone();
                fltk::app::add_timeout3(interval.as_secs_f64(), move |handle| {
                    emit(msg.clone());
                    fltk::app::repeat_timeout3(interval.as_secs_f64(), handle);
                });
            }
        }
    }

    for b in bindings.iter_mut() {
        b(&app);
    }
    #[cfg(feature = "layer-shell")]
    if layer {
        wayland::apply_layer(&win, &settings);
    }
    let _ = layer;
    win.show();
    #[cfg(all(unix, not(target_os = "macos")))]
    if !on_wayland() && x11::needed(&settings) {
        x11::apply(&win, &settings);
    }
    let init = app.init();
    if !run_task(init, &queue, &tx) {
        return Ok(());
    }
    // Messages it queued are handled on the first loop pass.
    fltk::app::awake();

    while fl.wait() {
        let mut changed = false;
        if close_requested.take() {
            match app.close_requested() {
                Some(msg) => queue.borrow_mut().push_back(msg),
                None => {
                    fl.quit();
                    return Ok(());
                }
            }
        }
        loop {
            let next = queue.borrow_mut().pop_front().or_else(|| rx.try_recv().ok());
            let Some(msg) = next else { break };
            changed = true;
            let task = app.update(msg);
            if !run_task(task, &queue, &tx) {
                fl.quit();
                return Ok(());
            }
        }
        if changed {
            for b in bindings.iter_mut() {
                b(&app);
            }
        }
        hover::update();
    }
    Ok(())
}

/// True when FLTK is running on its Wayland backend (hybrid builds pick
/// Wayland when available, X11 otherwise).
pub fn on_wayland() -> bool {
    unsafe { fltk_sys::fl::Fl_using_wayland() != 0 }
}

/// Starts a task's actions. Returns false if the app should quit.
fn run_task<M: Send + 'static>(
    task: Task<M>,
    queue: &Rc<RefCell<VecDeque<M>>>,
    tx: &mpsc::Sender<M>,
) -> bool {
    use task::Action;
    for action in task.0 {
        match action {
            Action::Message(m) => queue.borrow_mut().push_back(m),
            Action::Quit => return false,
            Action::Thread(work) => {
                let tx = tx.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(work());
                    fltk::app::awake();
                });
            }
            #[cfg(feature = "tokio")]
            Action::Future(fut) => {
                let tx = tx.clone();
                task::runtime::spawn(Box::pin(async move {
                    let _ = tx.send(fut.await);
                    fltk::app::awake();
                }));
            }
        }
    }
    true
}
