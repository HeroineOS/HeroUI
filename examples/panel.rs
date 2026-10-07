//! A top panel: borderless, on every workspace, with its height reserved so
//! maximized windows stay below it. cargo run --release --example panel

use std::time::Duration;

use heroui::prelude::*;

const PROFILES: &[&str] = &["Power saver", "Balanced", "Performance"];

#[derive(Default)]
struct Panel {
    clock: String,
    profile: usize,
    mute: bool,
}

#[derive(Clone)]
enum Msg {
    Tick,
    Profile(usize),
    Mute(bool),
}

impl App for Panel {
    type Message = Msg;

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Tick => {
                let secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                self.clock = format!("{:02}:{:02} UTC", secs / 3600 % 24, secs / 60 % 60);
            }
            Msg::Profile(i) => self.profile = i,
            Msg::Mute(m) => self.mute = m,
        }
        Task::none()
    }

    fn view(&self) -> Element<Self, Msg> {
        row(vec![
            heading("HeroineOS").fixed(130),
            spacer(),
            dropdown(|_: &Panel| PROFILES, |s: &Panel| s.profile, Msg::Profile).fixed(150),
            toggle("Mute", |s: &Panel| s.mute, Msg::Mute).fixed(96),
            text(|s: &Panel| s.clock.clone()).fixed(80),
        ])
        .padding(4)
        .spacing(12)
    }

    fn subscriptions(&self) -> Vec<Subscription<Msg>> {
        // The clock shows minutes; a 1 s tick keeps it on time, and the
        // text binding only redraws when the string changes.
        vec![Subscription::every(Duration::from_secs(1), Msg::Tick)]
    }
}

fn main() {
    heroui::simple_args("heroui-panel", env!("CARGO_PKG_VERSION"), "HeroUI demo: a panel (dock) at a screen edge.");
    let mut app = Panel::default();
    let _ = app.update(Msg::Tick);
    heroui::run(app, Settings::panel("panel", Edge::Top, 36).class("heroui-panel")).unwrap();
}
