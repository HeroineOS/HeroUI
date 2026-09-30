//! Cost benchmark: N rows of text + button + toggle + slider, all bound to
//! state, plus a 1 s tick that changes one value (like a conky refresh).
//! cargo run --release --example stress -- 200
//! Then: grep -E 'VmRSS|RssAnon|Threads' /proc/$(pgrep -x stress)/status

use std::time::Duration;

use heroui::prelude::*;

struct Stress {
    rows: usize,
    values: Vec<f64>,
    ticks: u64,
}

#[derive(Clone)]
enum Msg {
    Set(usize, f64),
    Toggle(usize, bool),
    Tick,
}

impl App for Stress {
    type Message = Msg;

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Set(i, v) => self.values[i] = v,
            Msg::Toggle(i, on) => self.values[i] = if on { 100.0 } else { 0.0 },
            Msg::Tick if self.rows > 0 => {
                self.ticks += 1;
                let i = self.ticks as usize % self.rows.max(1);
                if std::env::var_os("NOCHANGE").is_none() { self.values[i] = (self.values[i] + 7.0) % 100.0; }
            }
            Msg::Tick => {}
        }
        Task::none()
    }

    fn view(&self) -> Element<Self, Msg> {
        list(
            |s: &Stress| s.rows,
            |i| {
                row(vec![
                    text(move |s: &Stress| format!("{:.0}", s.values[i])).fixed(40),
                    button("Zero", Msg::Set(i, 0.0)).fixed(60),
                    toggle("", move |s: &Stress| s.values[i] > 50.0, move |on| Msg::Toggle(i, on)).fixed(60),
                    slider(0.0..=100.0, move |s: &Stress| s.values[i], move |v| Msg::Set(i, v)),
                ])
                .fixed(24)
            },
        )
    }

    fn subscriptions(&self) -> Vec<Subscription<Msg>> {
        vec![Subscription::every(Duration::from_secs(1), Msg::Tick)]
    }
}

fn main() {
    let rows = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(200);
    let app = Stress { rows, values: vec![0.0; rows], ticks: 0 };
    heroui::run(app, Settings::new("stress").size(400, 800)).unwrap();
}
