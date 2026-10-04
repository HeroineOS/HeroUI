# Core API

## App + run
```rust
pub trait App: Sized + 'static {
    type Message: Clone + Send + 'static;
    fn update(&mut self, msg: Self::Message) -> Task<Self::Message>;
    fn view(&self) -> Element<Self, Self::Message>;              // called once
    fn subscriptions(&self) -> Vec<Subscription<Self::Message>> { vec![] } // called once
    fn close_requested(&self) -> Option<Self::Message> { None }  // None = close; Some(msg) = ask update
    fn theme(&self) -> Theme { Theme::load() }                   // user's theme.conf
}
heroui::run(app, Settings::new("Title").size(w, h)) -> Result<(), FltkError>
```
`Settings` builders: `.size(w,h)` (default 480x320), `.position(x,y)`, `.resizable(bool)`
(default true), `.decorated(bool)` (false = borderless), `.class("x")` (WM/compositor rules),
`.kind(WindowKind::{Normal,Dock,Desktop,Dialog,Utility,Notification,Overlay})`, `.above(b)`, `.below(b)`,
`.sticky(b)` (all workspaces), `.skip_taskbar(b)`, `.reserve(Edge::Top, px)` (strut).
Presets: `Settings::panel(title, edge, thickness)`, `Settings::desktop_widget(title, x, y, w, h)`.
On X11/XWayland these are EWMH hints. On Wayland, Dock/Desktop/Notification become
wlr-layer-shell surfaces with feature `layer-shell` (+ the fltk-sys fork, see README):
Dock = top layer anchored along `reserve`'s edge, spanning it, space reserved; Desktop =
bottom layer at `position` from the screen's top-left; Notification = top layer at
`position`. If the compositor lacks layer-shell (GNOME) they fall back to XWayland, else
a normal window. `above/below/sticky/skip_taskbar` have no Wayland meaning (layers cover
it). Wayland apps can't position regular windows; `position` is ignored there.
`heroui::on_wayland()` tells which backend runs. Window class (= Wayland app_id) defaults
to the executable name. Docks never take keyboard focus (a click on a taskbar must not
pull focus from the window it activates). Overlay (launchers, menus): on Wayland a
layer-shell surface over the whole screen (panels too) that takes the keyboard; with
`transparent` the app draws its panel where it wants and clicks elsewhere land on it (close
then). On X11 a borderless override window at `position` that grabs the pointer and gets
the keyboard focus; during the grab FLTK sends keys to the window, not the focused widget,
so forward them (see HeroLauncher). `heroui::is_layer()` tells which you got.

`.transparent(true)`: the window is see-through where nothing is drawn; each full repaint
starts cleared instead of filled with the theme background, so widgets that paint their own
backgrounds float over the desktop (bar islands). Anti-aliased edges blend. Needs feature
`layer-shell` (fltk-sys fork) and Wayland; otherwise the window stays opaque.
`heroui::is_transparent()` says which you got (e.g. draw islands in a contrasting color
when it's false).

Features: `wayland` (default; hybrid Wayland/X11), `layer-shell`, `tokio`.

Drawing model: retained. `view` runs once; after each batch of messages, bindings touch only
widgets whose value changed and FLTK redraws only those rectangles. Idle = asleep in the
event loop, 0% CPU (no frame loop, unlike egui; no view re-run/diff, unlike iced).

Escape never closes the window; the WM close button calls `close_requested`.

Loop: widget callbacks queue messages → `update` for each → returned tasks start → all
bindings run once with the new state.

## Task (returned from update)
| Call | Effect |
|---|---|
| `Task::none()` | nothing |
| `Task::perform(move \|\| Msg::X(work()))` | runs closure on a new plain thread, result → `update` |
| `Task::message(msg)` | `msg` → `update` right after this one |
| `Task::quit()` | close window, `run` returns |
| `Task::batch([t1, t2])` | several at once |
| `Task::future(async move { .. })` | feature `tokio` only: shared current-thread runtime |
| `Task::rebuild()` | builds the view again from `App::view` after this update (old widgets deleted, popovers of deleted anchors too); for a different set of widgets (config changes). Subscriptions stay |
| `task.map(Msg::Child)` | component task → parent task (wraps messages and background results) |

## Subscription
`Subscription::every(Duration, msg)`: clones `msg` to `update` every interval, on the UI
thread (FLTK timeout, no extra thread). `sub.map(Msg::Child)` converts a component's.

`Subscription::worker(|tx: heroui::Sender<Msg>| { .. })`: runs the closure once on its own
thread (small stack) for the app's life; `tx.send(msg)` queues a message and wakes the loop
(returns false after quit: return then). For event streams (compositor IPC, sockets,
inotify): nothing runs while nothing happens. Use `every` for polling.

## Element<S, M> modifiers
| Modifier | Meaning |
|---|---|
| `.fixed(px)` | size along parent axis (height in column, width in row); otherwise shares space |
| `.fixed_with(\|s\| px)` | same, computed from state (re-laid out when it changes) |
| `.padding(px)` | inner margin (rows/columns default 0, cards theme padding) |
| `.spacing(px)` | gap between children (default theme spacing) |
| `.visible(\|s\| bool)` | hidden children take no space; layout re-runs |
| `.enabled(\|s\| bool)` | greyed + inert when false (applies to the whole subtree) |

`Element::new(|ctx: &mut Ctx<S, M>| -> fltk::widget::Widget)` is the escape hatch for any
raw fltk widget (see patterns.md).

## Ctx<S, M> (inside Element::new)
- `ctx.emitter() -> Rc<dyn Fn(M)>`: capture it in widget callbacks to send messages.
- `ctx.bind(move |s: &S| ..)`: run after every update (and once at startup). Update the widget here.
- `ctx.theme() -> &Theme`.
- `ctx.size_hint()`: this element's size hint cell; set it if your element has a natural size
  that changes (as `list` does), so containers like `scroll` can size it. `el.size_hint()`
  reads a child's.
- `ctx.build_children(&mut flex, children)`: build child Elements into a Flex, honoring `.fixed`.
  This is all a custom container needs.
- `ctx.child()` + `ctx.into_bindings()`, `el.build(ctx)`, `el.fixed_size()`,
  `heroui::relayout_parent(&w)`: for containers that rebuild children (see `list` source).

## embed (reusable components)
```rust
embed(lens: Fn(&S) -> &T, map: Fn(TM) -> M, child: Element<T, TM>) -> Element<S, M>
// view:   embed(|s: &App| &s.counter, Msg::Counter, Counter::view())
// update: Msg::Counter(m) => self.counter.update(m),
```
The component knows only its own state `T` and messages `TM`. The parent forwards the messages.
If the component's update returns a Task: `Msg::Load(m) => return self.load.update(m).map(Msg::Load),`
and its subscriptions: `subs.extend(LoadAvg::subscriptions().into_iter().map(|s| s.map(Msg::Load)))`.
