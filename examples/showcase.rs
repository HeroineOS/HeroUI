//! Tour of HeroUI: reusable components plugged in with `embed` (one with
//! its own background task and timer), inputs, a dynamic list, a timer.
//!
//! cargo run --example showcase

use std::time::Duration;

use heroui::prelude::*;

// ---- A reusable component: its own state, messages and update. ----------
// It knows nothing about the app it's embedded in.

#[derive(Default)]
struct Counter {
    value: i32,
}

#[derive(Clone)]
enum CounterMsg {
    Inc,
    Dec,
}

impl Counter {
    fn update(&mut self, msg: CounterMsg) {
        match msg {
            CounterMsg::Inc => self.value += 1,
            CounterMsg::Dec => self.value -= 1,
        }
    }

    fn view() -> Element<Counter, CounterMsg> {
        row(vec![
            button("−", CounterMsg::Dec).fixed(40),
            text(|c: &Counter| format!("{}", c.value)),
            button("+", CounterMsg::Inc).fixed(40),
        ])
    }
}

// ---- A component with side effects: background reads and its own timer. ----
// The parent maps its Task and Subscription with the same wrapper as embed.

#[derive(Default)]
struct LoadAvg {
    text: String,
}

#[derive(Clone)]
enum LoadMsg {
    Refresh,
    Loaded(String),
}

impl LoadAvg {
    fn update(&mut self, msg: LoadMsg) -> Task<LoadMsg> {
        match msg {
            // Blocking I/O belongs off the UI thread.
            LoadMsg::Refresh => {
                return Task::perform(|| {
                    let s = std::fs::read_to_string("/proc/loadavg").unwrap_or_default();
                    LoadMsg::Loaded(s.split_whitespace().take(3).collect::<Vec<_>>().join("  "))
                })
            }
            LoadMsg::Loaded(s) => self.text = s,
        }
        Task::none()
    }

    fn subscriptions() -> Vec<Subscription<LoadMsg>> {
        vec![Subscription::every(Duration::from_secs(5), LoadMsg::Refresh)]
    }

    fn view() -> Element<LoadAvg, LoadMsg> {
        row(vec![
            button("Read /proc/loadavg", LoadMsg::Refresh).fixed(170),
            text(|s: &LoadAvg| s.text.clone()),
        ])
    }
}

// ---- The app. ---------------------------------------------------------------

#[derive(Default)]
struct Showcase {
    counter: Counter,
    wifi: bool,
    volume: f64,
    draft: String,
    items: Vec<String>,
    load: LoadAvg,
    clock: String,
    /// 0.0..=1.0 while a (simulated) download runs.
    download: Option<f64>,
}

#[derive(Clone)]
enum Msg {
    Counter(CounterMsg),
    Wifi(bool),
    Volume(f64),
    Draft(String),
    Add,
    Remove(usize),
    Load(LoadMsg),
    Tick,
    StartDownload,
    DownloadStep,
}

impl App for Showcase {
    type Message = Msg;

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Counter(m) => self.counter.update(m),
            Msg::Wifi(on) => self.wifi = on,
            Msg::Volume(v) => self.volume = v,
            Msg::Draft(s) => self.draft = s,
            Msg::Add => {
                if !self.draft.trim().is_empty() {
                    self.items.push(std::mem::take(&mut self.draft));
                }
            }
            Msg::Remove(i) => {
                if i < self.items.len() {
                    self.items.remove(i);
                }
            }
            Msg::Load(m) => return self.load.update(m).map(Msg::Load),
            // A stand-in for real background work reporting progress: each
            // step sleeps off the UI thread, then reports back.
            Msg::StartDownload => {
                self.download = Some(0.0);
                return Task::perform(download_step);
            }
            Msg::DownloadStep => {
                let p = self.download.unwrap_or(0.0) + 0.04;
                if p >= 1.0 {
                    self.download = None;
                } else {
                    self.download = Some(p);
                    return Task::perform(download_step);
                }
            }
            Msg::Tick => {
                let secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                self.clock = format!("{:02}:{:02}:{:02} UTC", secs / 3600 % 24, secs / 60 % 60, secs % 60);
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<Self, Msg> {
        column(vec![
            row(vec![
                heading("HeroUI"),
                text(|s: &Showcase| s.clock.clone()).fixed(110),
            ])
            .fixed(36),
            card(vec![
                caption("Reusable component (embedded twice would work too)"),
                embed(|s: &Showcase| &s.counter, Msg::Counter, Counter::view()).fixed(36),
            ])
            .fixed(96),
            card(vec![
                toggle("Wi-Fi", |s: &Showcase| s.wifi, Msg::Wifi).fixed(28),
                row(vec![
                    label("Volume").fixed(70),
                    slider(0.0..=100.0, |s: &Showcase| s.volume, Msg::Volume),
                    text(|s: &Showcase| format!("{:.0}%", s.volume)).fixed(44),
                ])
                .fixed(28)
                .enabled(|s: &Showcase| s.wifi),
            ])
            .fixed(92),
            row(vec![
                primary_button("Download", Msg::StartDownload)
                    .fixed(110)
                    .enabled(|s: &Showcase| s.download.is_none()),
                progress(|s: &Showcase| s.download.unwrap_or(0.0)),
                text(|s: &Showcase| s.download.map(|p| format!("{:.0}%", p * 100.0)).unwrap_or_default()).fixed(44),
            ])
            .fixed(34),
            card(vec![
                row(vec![
                    // Enter adds too.
                    text_input_submit(|s: &Showcase| s.draft.clone(), Msg::Draft, Msg::Add),
                    primary_button("Add", Msg::Add)
                        .fixed(70)
                        .enabled(|s: &Showcase| !s.draft.trim().is_empty()),
                ])
                .fixed(34),
                caption("Nothing here yet").fixed(20).visible(|s: &Showcase| s.items.is_empty()),
                list(
                    |s: &Showcase| s.items.len(),
                    |i| {
                        row(vec![
                            text(move |s: &Showcase| s.items.get(i).cloned().unwrap_or_default()),
                            button("Remove", Msg::Remove(i)).fixed(80),
                        ])
                        .fixed(30)
                    },
                ),
            ]),
            embed(|s: &Showcase| &s.load, Msg::Load, LoadAvg::view()).fixed(34),
        ])
        .padding(16)
        .spacing(12)
    }

    fn subscriptions(&self) -> Vec<Subscription<Msg>> {
        let mut subs = vec![Subscription::every(Duration::from_secs(1), Msg::Tick)];
        subs.extend(LoadAvg::subscriptions().into_iter().map(|s| s.map(Msg::Load)));
        subs
    }
}

fn download_step() -> Msg {
    std::thread::sleep(Duration::from_millis(100));
    Msg::DownloadStep
}

fn main() {
    let app = Showcase { volume: 40.0, wifi: true, ..Default::default() };
    heroui::run(app, Settings::new("HeroUI showcase").size(460, 600).class("heroui-showcase")).unwrap();
}
