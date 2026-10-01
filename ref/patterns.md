# Patterns

## Reusable component (own state + messages, plugged in with embed)
```rust
#[derive(Default)] pub struct Volume { level: f64, muted: bool }
#[derive(Clone)] pub enum VolumeMsg { Set(f64), Mute(bool) }
impl Volume {
    pub fn update(&mut self, m: VolumeMsg) { match m { VolumeMsg::Set(v) => self.level = v, VolumeMsg::Mute(b) => self.muted = b } }
    pub fn view() -> Element<Volume, VolumeMsg> {
        row(vec![
            slider(0.0..=100.0, |s: &Volume| s.level, VolumeMsg::Set),
            toggle("Mute", |s: &Volume| s.muted, VolumeMsg::Mute).fixed(90),
        ])
    }
}
// parent view:   embed(|s: &App| &s.volume, Msg::Volume, Volume::view()).fixed(28)
// parent update: Msg::Volume(m) => self.volume.update(m),
```
A component with side effects returns `Task<VolumeMsg>` from its update and may have its
own `fn subscriptions() -> Vec<Subscription<VolumeMsg>>`. The parent maps both:
```rust
Msg::Volume(m) => return self.volume.update(m).map(Msg::Volume),
// fn subscriptions: subs.extend(Volume::subscriptions().into_iter().map(|s| s.map(Msg::Volume)));
```
Full example: `LoadAvg` in `examples/showcase.rs`.

One component per list item: index the lens and the message:
```rust
list(|s: &App| s.devices.len(), |i| {
    embed(move |s: &App| &s.devices[i], move |m| Msg::Device(i, m), Device::view()).fixed(30)
})
// update: Msg::Device(i, m) => if let Some(d) = self.devices.get_mut(i) { return d.update(m).map(move |m| Msg::Device(i, m)) }
```
(`&s.devices[i]` is safe here: `list` rebuilds on count change before running item bindings.)

## Background work (blocking I/O, processes, /proc)
```rust
Msg::Refresh => return Task::perform(|| {
    let s = std::fs::read_to_string("/proc/loadavg").unwrap_or_default();
    Msg::Loaded(s)
}),
Msg::Loaded(s) => self.load = s,
```
One short-lived thread per call, gone after it returns. For a long-running stream (e.g.
reading a socket or `udevadm monitor`), spawn your own `std::thread` in `update` on a
start message, send results through a channel you own, and drain it on a
`Subscription::every` tick. Keep the channel in state (`Option<Receiver>`).

## Periodic refresh
`fn subscriptions(&self) -> Vec<Subscription<Msg>> { vec![Subscription::every(Duration::from_secs(2), Msg::Tick)] }`
Cheap read (e.g. a small `/proc` file) in `update` on Tick is fine. Anything slower → `Task::perform`.

## Editable list
```rust
list(|s: &App| s.items.len(), |i| row(vec![
    text(move |s: &App| s.items.get(i).map(|x| x.name.clone()).unwrap_or_default()),
    button("Remove", Msg::Remove(i)).fixed(80),
]).fixed(30))
// update: Msg::Remove(i) => { if i < self.items.len() { self.items.remove(i); } }
```
Show an empty-state hint with `caption("Nothing here").visible(|s: &App| s.items.is_empty())`.

## Form with a disabled submit
```rust
row(vec![
    text_input(|s: &App| s.draft.clone(), Msg::Draft),
    primary_button("Add", Msg::Add).fixed(70).enabled(|s: &App| !s.draft.trim().is_empty()),
]).fixed(34)
```

## Custom / missing widget (Element::new)
Full working code: `examples/custom_widget.rs` (`swatch`, a clickable custom widget).
Recipe:
1. `Element::new(move |ctx| { .. })`.
2. Theme: `let t = ctx.theme_rc();` (an `Rc`, cheap to move into draw closures).
3. Clickable → `custom_button(move |b| { draw.. })` + `b.set_callback(move |_| emit(msg.clone()))`
   with `let emit = ctx.emitter();`. In draw use `b.value()` (pressed), `is_hovered(b)`,
   `b.active_r()`. NO `handle` closure (see mistakes.md #15).
   Display-only → `Frame::default()` + `set_frame(FrameType::NoBox)` + `draw`, or just `canvas`.
4. State → `ctx.bind(move |s| { if changed { store; w.redraw() or repaint(&mut w) } })`.
   Keep a widget-local copy (`Rc<Cell<_>>`) for the draw closure to read.
5. Return `w.as_base_widget()`.

## Desktop widget (conky-like)
`Settings::desktop_widget("name", x, y, w, h)`: borderless, Desktop type, below, sticky, no
taskbar (Wayland + `layer-shell`: bottom layer, x/y from the screen's top-left). Sample cheap procfs files directly in `update` on a `Subscription::every` tick;
`graph(|s| &s.cpu_history, 100.0)` for history, `canvas(|s| key, paint_fn)` for gauges (quantize
the key, e.g. `(frac * 1000.0) as u16`, so noise doesn't redraw). Full: `examples/sysmon.rs`
(2.6 MB anon RSS, 0.1% CPU at 1 s refresh).

## Panel / dock
`Settings::panel("name", Edge::Top, 36)`: Dock type spanning the edge, sticky, space reserved.
Dropdown popups extend past the panel. Full: `examples/panel.rs`. Other kinds:
`.kind(WindowKind::Notification).above(true)` for OSDs.
On Wayland: enable feature `layer-shell` and add the fltk-sys `[patch]` (README) or it runs on
XWayland. Test natively: headless sway (`WLR_BACKENDS=headless`, `xwayland disable` in its
config), screenshots with `grim`; layer surfaces don't appear in `swaymsg -t get_tree`, but
the workspace rect shrinks by the reserved space.

## Custom container
```rust
fn section<S: 'static, M: 'static>(title: &str, children: Vec<Element<S, M>>) -> Element<S, M> {
    let title = title.to_string();
    Element::new(move |ctx| {
        let t = ctx.theme().clone();
        let mut flex = Flex::default().column();
        flex.end();
        flex.set_pad(t.spacing);
        flex.draw(move |f| { /* border + title; runs after children draw */ });
        ctx.build_children(&mut flex, children);
        flex.as_base_widget()
    })
}
```
Full code: `examples/custom_widget.rs`. `.padding`/`.spacing`/`.fixed`/`.visible` work on it
because it's a Flex.

## Confirm before closing
```rust
fn close_requested(&self) -> Option<Msg> { self.dirty.then_some(Msg::AskClose) }
// update: Msg::AskClose => self.confirm_visible = true,  Msg::ReallyClose => return Task::quit(),
```

## Measuring cost
`cargo run --release --example stress -- 1000` (1000 rows × 4 bound widgets, 1 s tick), then
`grep -E 'VmRSS|RssAnon|Threads' /proc/$(pgrep -x stress)/status` and CPU ticks from
`/proc/PID/stat` fields 14+15 over 10 s. Reference (this repo, Xvfb): 1000 rows = 4.5 MB anon,
0.2% CPU; 3000 rows = 9 MB, 0.5%.

## Async (feature `tokio`)
`heroui = { .., features = ["tokio"] }` and add the tokio features your futures need
(`time`, `net`, ...) to your own tokio dependency. `Task::future(async move { .. ; Msg::Done(x) })`.
All futures share one current-thread runtime on one thread, started on first use.

## Headless test (CI / servers without a display)
```sh
Xvfb :99 -screen 0 800x700x24 & export DISPLAY=:99
./target/release/examples/showcase & sleep 1
xdotool mousemove 412 149 click 1            # coordinates = screen pixels
ffmpeg -loglevel error -y -f x11grab -video_size 470x630 -i :99.0+0,0 -frames:v 1 shot.png
grep -E 'VmRSS|RssAnon|Threads' /proc/$(pgrep -x showcase)/status
pkill -x showcase                            # -x, never -f (see mistakes.md)
```
