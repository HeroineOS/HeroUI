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

mod element;
pub mod hover;
mod popup;
mod task;
pub mod theme;
pub mod widgets;

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
    pub use crate::{embed, App, Ctx, Element, Settings, Subscription, Task, Theme};
}

/// An application: its state is `Self`.
pub trait App: Sized + 'static {
    type Message: Clone + Send + 'static;

    /// Handles one message. Change state here; return a [`Task`] for side
    /// effects (background work, follow-up messages, quitting).
    fn update(&mut self, message: Self::Message) -> Task<Self::Message>;

    /// Describes the UI. Called once at startup.
    fn view(&self) -> Element<Self, Self::Message>;

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
        }
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
}

/// Opens the window and runs the app until it's closed or `Task::quit`.
pub fn run<A: App>(mut app: A, settings: Settings) -> Result<(), fltk::prelude::FltkError> {
    let fl = fltk::app::App::default();
    let theme = Rc::new(app.theme());
    theme.apply();

    let (w, h) = settings.size;
    let mut win = Window::default().with_size(w, h).with_label(&settings.title);
    if let Some((x, y)) = settings.position {
        win.set_pos(x, y);
    }
    if let Some(class) = &settings.class {
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
    if settings.resizable {
        win.resizable(&root);
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
    win.show();

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
