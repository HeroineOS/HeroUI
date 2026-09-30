# Minimal app

Full file: `examples/counter.rs`. Bigger tour: `examples/showcase.rs` (component via embed,
toggle/slider/progress, list, background task, timer). Raw widgets: `examples/custom_widget.rs`.

```rust
use heroui::prelude::*;

#[derive(Default)]
struct Counter { value: i32 }

#[derive(Clone)]
enum Msg { Inc, Dec, Reset }

impl App for Counter {
    type Message = Msg;
    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Inc => self.value += 1,
            Msg::Dec => self.value -= 1,
            Msg::Reset => self.value = 0,
        }
        Task::none()
    }
    fn view(&self) -> Element<Self, Msg> {
        column(vec![
            heading("Counter").fixed(32),
            text(|s: &Counter| s.value.to_string()).fixed(28),
            row(vec![
                button("−", Msg::Dec),
                button("+", Msg::Inc),
                primary_button("Reset", Msg::Reset).enabled(|s: &Counter| s.value != 0),
            ]).fixed(34),
            spacer(),
        ]).padding(16)
    }
}

fn main() {
    heroui::run(Counter::default(), Settings::new("Counter").size(280, 150)).unwrap();
}
```

Cargo.toml: `heroui = { git = "https://github.com/HeroineOS/HeroUI" }`. Release profile tip
for small binaries: `[profile.release] opt-level = "s"`, `lto = true`, `strip = true`,
`panic = "abort"`, `codegen-units = 1`.
