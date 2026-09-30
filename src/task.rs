//! Side effects returned from `update`, and timers.
//!
//! Background work runs on a plain std thread and comes back as a message.
//! A thread only exists while its work runs, so nothing is spent while idle.
//! With the `tokio` feature, `Task::future` runs async work on a single
//! shared current-thread tokio runtime instead.

use std::time::Duration;

pub(crate) enum Action<M> {
    Thread(Box<dyn FnOnce() -> M + Send>),
    Message(M),
    Quit,
    #[cfg(feature = "tokio")]
    Future(std::pin::Pin<Box<dyn std::future::Future<Output = M> + Send>>),
}

/// What `update` wants done after changing state.
#[must_use = "return the task from update, or it won't run"]
pub struct Task<M>(pub(crate) Vec<Action<M>>);

impl<M> Task<M> {
    pub fn none() -> Self {
        Self(Vec::new())
    }

    /// Runs `work` on a background thread; its result is sent to `update`.
    /// Use for anything that could block: file/network I/O, `/proc` scans,
    /// spawning processes. Never touch widgets from `work`.
    pub fn perform(work: impl FnOnce() -> M + Send + 'static) -> Self {
        Self(vec![Action::Thread(Box::new(work))])
    }

    /// Sends `msg` to `update` right after this one finishes.
    pub fn message(msg: M) -> Self {
        Self(vec![Action::Message(msg)])
    }

    /// Closes the window and returns from `run`.
    pub fn quit() -> Self {
        Self(vec![Action::Quit])
    }

    pub fn batch(tasks: impl IntoIterator<Item = Task<M>>) -> Self {
        Self(tasks.into_iter().flat_map(|t| t.0).collect())
    }

    /// Runs an async `future` on the shared single-thread tokio runtime;
    /// its output is sent to `update`. Only worth it for many concurrent
    /// I/O waits — otherwise prefer [`Task::perform`].
    #[cfg(feature = "tokio")]
    pub fn future(future: impl std::future::Future<Output = M> + Send + 'static) -> Self {
        Self(vec![Action::Future(Box::pin(future))])
    }
}

impl<M> Default for Task<M> {
    fn default() -> Self {
        Self::none()
    }
}

/// A source of messages that exists for the app's whole life.
pub enum Subscription<M> {
    /// Sends a clone of the message every `Duration`, on the UI thread.
    Every(Duration, M),
}

impl<M> Subscription<M> {
    pub fn every(interval: Duration, msg: M) -> Self {
        Self::Every(interval, msg)
    }
}

#[cfg(feature = "tokio")]
pub(crate) mod runtime {
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::OnceLock;

    type Job = Pin<Box<dyn Future<Output = ()> + Send>>;

    static JOBS: OnceLock<tokio::sync::mpsc::UnboundedSender<Job>> = OnceLock::new();

    /// Started on first use: one thread, one current-thread runtime.
    pub fn spawn(job: Job) {
        let tx = JOBS.get_or_init(|| {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Job>();
            std::thread::Builder::new()
                .name("heroui-async".into())
                .spawn(move || {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .expect("tokio runtime");
                    let local = tokio::task::LocalSet::new();
                    local.block_on(&rt, async move {
                        while let Some(job) = rx.recv().await {
                            tokio::task::spawn_local(job);
                        }
                    });
                })
                .expect("spawn heroui-async thread");
            tx
        });
        let _ = tx.send(job);
    }
}
