# Common mistakes (check before writing code)

0. **Unwanted stretching (most common layout bug)**: rows/columns are fltk `Flex`. A child
   without `.fixed(px)` shares the leftover space, so a lone button in a column becomes
   window-tall. Give every row, button, input and list item a `.fixed(px)` (height in a
   column, width in a row). Leave exactly the things that should grow unfixed. For empty
   filler use `spacer()`.
1. **Branching in `view`**: `view` runs ONCE at startup. `if self.x { a } else { b }` in
   `view` freezes the first result. Use `.visible(|s| ..)`, `.enabled(|s| ..)`, `text(|s| ..)`,
   or `list(..)` for anything that changes.
2. **Blocking in `update`** (file/net I/O, `/proc` scans, `Command::output`, `sleep`) freezes
   the UI. Return `Task::perform(move || { ...; Msg::Done(result) })`. The closure runs on
   another thread: it must be `Send` and must NOT touch widgets or `self`. Copy what it needs in.
3. **Polling loops on threads**: for periodic refresh use `Subscription::every(dur, Msg::Tick)`
   (an FLTK timeout on the UI thread, no thread) and do the actual read with `Task::perform`
   only if it can block.
4. **Message must be `Clone + Send + 'static`**: no `Rc`, no widgets, no borrowed data in
   messages. Use owned `String`/`Vec`/`Arc`.
5. **Custom draw in fltk-rs runs AFTER the widget's own draw** (`widget.draw(cb)`). Setting a
   label on the widget and also drawing text shows it twice. Either use a `Frame` and draw
   everything yourself, or call `w.super_draw(false)` to disable the stock draw. Setting a
   valuator's box AND slider box to `NoBox` is not enough: FLTK falls back to an `FL_UP_BOX`
   knob.
6. **Disabled parents**: `.enabled(false)` on a row deactivates the group, but each child's
   own `active()` stays true. In custom draw code use `w.active_r()` to dim correctly.
7. **Hard-coded colors**: use `ctx.theme()` (`surface`, `surface_alt`, `accent`, `text`,
   `text_dim`...) so the user's theme file restyles your widget too.
8. **Raw widgets in `Element::new`**: create them with `Widget::default()` (zero size, the
   parent Flex sizes them). If you create a `Group`/`Flex`, call `.end()` before returning.
   Return `w.as_base_widget()`. Register state-dependent updates with `ctx.bind`. Don't
   update widgets anywhere else.
9. **Bindings run after every batch of messages, all of them.** Keep them cheap: compare
   before setting (`if w.value() != v`), and don't do I/O or heavy formatting in them.
10. **`list` item closures get an index**, not the item: read `s.items.get(i)` inside
    bindings (prefer `.get(i)` over `[i]`, so a bug can't panic the UI). The list rebuilds
    only when the count changes, and bindings re-read by index, so reordering and in-place
    edits work without a rebuild.
11. **Don't enable `tokio` by habit.** `Task::perform` covers almost everything. Enable the
    feature only for many concurrent I/O waits (sockets, D-Bus). It only turns on tokio's
    `rt` + `sync`, so add `time`/`net`/... to your own tokio dependency if the futures need them.
12. **Quitting**: return `Task::quit()` from `update`; don't call `std::process::exit` (skips
    cleanup) or `fltk::app::quit` directly.
13. **Headless test self-kill**: in scripts use `pkill -x <binary>`, not `pkill -f <path>`.
    `-f` matches the shell running the script and kills it.
