//! A conky-like desktop widget: CPU history graph, memory gauge, uptime.
//! Borderless, below other windows, on all workspaces, no taskbar entry.
//! cargo run --release --example sysmon

use std::time::Duration;

use heroui::fltk::draw;
use heroui::prelude::*;

const HISTORY: usize = 60;

struct SysMon {
    cpu: Vec<f64>,
    /// Last (busy, total) jiffies from /proc/stat, to diff against.
    last: (u64, u64),
    mem_used: f64,
    mem_total_gib: f64,
    uptime: String,
}

#[derive(Clone)]
enum Msg {
    Tick,
}

impl SysMon {
    fn new() -> Self {
        let mut s = Self { cpu: vec![0.0; HISTORY], last: cpu_jiffies(), mem_used: 0.0, mem_total_gib: 0.0, uptime: String::new() };
        s.sample();
        s
    }

    /// procfs reads are in-memory and take microseconds, so they're done
    /// right here on the UI thread instead of on a Task::perform thread.
    fn sample(&mut self) {
        let (busy, total) = cpu_jiffies();
        let (db, dt) = (busy.saturating_sub(self.last.0), total.saturating_sub(self.last.1));
        self.last = (busy, total);
        if dt > 0 {
            self.cpu.rotate_left(1);
            *self.cpu.last_mut().unwrap() = 100.0 * db as f64 / dt as f64;
        }
        let (avail, total) = meminfo();
        if total > 0 {
            self.mem_used = 1.0 - avail as f64 / total as f64;
            self.mem_total_gib = total as f64 / 1024.0 / 1024.0;
        }
        let secs = std::fs::read_to_string("/proc/uptime")
            .ok()
            .and_then(|s| s.split('.').next().and_then(|n| n.parse::<u64>().ok()))
            .unwrap_or(0);
        self.uptime = format!("up {}d {}h {:02}m", secs / 86400, secs / 3600 % 24, secs / 60 % 60);
    }
}

fn cpu_jiffies() -> (u64, u64) {
    let stat = std::fs::read_to_string("/proc/stat").unwrap_or_default();
    let nums: Vec<u64> = stat.lines().next().unwrap_or("").split_whitespace().skip(1).filter_map(|n| n.parse().ok()).collect();
    let total: u64 = nums.iter().take(8).sum();
    let idle = nums.get(3).copied().unwrap_or(0) + nums.get(4).copied().unwrap_or(0);
    (total - idle, total)
}

/// (MemAvailable, MemTotal) in KiB.
fn meminfo() -> (u64, u64) {
    let info = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let field = |name: &str| {
        info.lines()
            .find(|l| l.starts_with(name))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|n| n.parse().ok())
            .unwrap_or(0)
    };
    (field("MemAvailable:"), field("MemTotal:"))
}

/// A ring gauge: `frac` of a circle in the accent color.
fn gauge(frac: &u16, x: i32, y: i32, w: i32, h: i32, t: &Theme) {
    let d = w.min(h) - 4;
    let (cx, cy) = (x + (w - d) / 2, y + (h - d) / 2);
    let frac = *frac as f64 / 1000.0;
    draw::set_line_style(draw::LineStyle::Solid | draw::LineStyle::CapRound, 8);
    draw::set_draw_color(t.surface_alt);
    draw::draw_arc(cx + 4, cy + 4, d - 8, d - 8, 0.0, 360.0);
    draw::set_draw_color(t.accent);
    draw::draw_arc(cx + 4, cy + 4, d - 8, d - 8, 90.0 - 360.0 * frac, 90.0);
    draw::set_line_style(draw::LineStyle::Solid, 0);
    draw::set_draw_color(t.text);
    draw::set_font(t.bold_font(), t.font_size + 2);
    draw::draw_text2(&format!("{:.0}%", frac * 100.0), x, y, w, h, heroui::fltk::enums::Align::Center);
}

impl App for SysMon {
    type Message = Msg;

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Tick => self.sample(),
        }
        Task::none()
    }

    fn view(&self) -> Element<Self, Msg> {
        column(vec![
            row(vec![heading("System"), text(|s: &SysMon| s.uptime.clone()).fixed(120)]).fixed(30),
            row(vec![
                caption("CPU"),
                text(|s: &SysMon| format!("{:.0}%", s.cpu.last().copied().unwrap_or(0.0))).fixed(44),
            ])
            .fixed(20),
            graph(|s: &SysMon| &s.cpu, 100.0).fixed(70),
            row(vec![
                // Quantized to 0.1% so tiny changes don't cause redraws.
                canvas(|s: &SysMon| (s.mem_used * 1000.0) as u16, gauge).fixed(96),
                column(vec![
                    spacer(),
                    label("Memory").fixed(22),
                    text(|s: &SysMon| format!("{:.1} / {:.1} GiB", s.mem_used * s.mem_total_gib, s.mem_total_gib)).fixed(22),
                    spacer(),
                ]),
            ])
            .fixed(96),
        ])
        .padding(16)
    }

    fn subscriptions(&self) -> Vec<Subscription<Msg>> {
        vec![Subscription::every(Duration::from_secs(1), Msg::Tick)]
    }
}

fn main() {
    heroui::simple_args("heroui-sysmon", env!("CARGO_PKG_VERSION"), "HeroUI demo: CPU and memory graphs.");
    let settings = Settings::desktop_widget("sysmon", 40, 40, 300, 280).class("heroui-sysmon");
    heroui::run(SysMon::new(), settings).unwrap();
}
