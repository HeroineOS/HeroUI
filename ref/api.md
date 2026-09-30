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
`.kind(WindowKind::{Normal,Dock,Desktop,Dialog,Utility,Notification})`, `.above(b)`, `.below(b)`,
`.sticky(b)` (all workspaces), `.skip_taskbar(b)`, `.reserve(Edge::Top, px)` (strut).
Presets: `Settings::panel(title, edge, thickness)`, `Settings::desktop_widget(title, x, y, w, h)`.
Kinds/states/struts are EWMH hints set at startup (X11 and XWayland); skipped with the
`wayland` feature.

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
| `task.map(Msg::Child)` | component task → parent task (wraps messages and background results) |

## Subscription
`Subscription::every(Duration, msg)`: clones `msg` to `update` every interval, on the UI
thread (FLTK timeout, no extra thread). `sub.map(Msg::Child)` converts a component's.

## Element<S, M> modifiers
| Modifier | Meaning |
|---|---|
| `.fixed(px)` | size along parent axis (height in column, width in row); otherwise shares space |
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
