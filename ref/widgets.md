# Widgets (all in `heroui::widgets`, via prelude)

`S` = app state, `M` = message. Closures taking `&S` are bindings (re-run after updates;
widget touched only when the value changed).

| Function | Notes |
|---|---|
| `column(vec![..])` / `row(vec![..])` | Flex layouts; children need `.fixed` or they share space |
| `card(vec![..])` | rounded `surface` panel, column inside, theme padding |
| `spacer()` | stretchy empty space; `spacer().fixed(8)` for a fixed gap |
| `label("txt")` | static text |
| `heading("txt")` | static bold, larger |
| `caption("txt")` | static, `text_dim` color |
| `text(\|s\| String)` | dynamic text |
| `button("txt", msg)` | `msg: M + Clone`, sent on click |
| `primary_button("txt", msg)` | accent colored, for the main action |
| `text_input(\|s\| String, \|String\| M)` | sends on every edit; not overwritten while it matches state (cursor stays) |
| `text_input_submit(\|s\| String, \|String\| M, msg)` | same, and Enter sends `msg` (add/search/confirm) |
| `toggle("label", \|s\| bool, \|bool\| M)` | switch, label on the left; give it `.fixed(28)` in a column |
| `checkbox("label", \|s\| bool, \|bool\| M)` | box + label on the right |
| `dropdown(\|s\| &[T], \|s\| usize, \|usize\| M)` | `T: AsRef<str>`; static list `\|_: &S\| CONST_SLICE` or from state `\|s\| &s.names`; list is FLTK's own menu popup (as Fl_Choice), themed; native xdg_popup on Wayland; opens on press and blocks in FLTK's menu loop until a pick |
| `slider(0.0..=100.0, \|s\| f64, \|f64\| M)` | horizontal, sends while dragging |
| `progress(\|s\| f64)` | read-only bar, value 0.0..=1.0 |
| `graph(\|s\| &[f64], max)` | filled line graph (history, oldest first); no allocation per update |
| `canvas(\|s\| D, paint)` | custom drawing; `D: PartialEq`, redrawn only when it changes; `paint(&D, x, y, w, h, &Theme)` with `fltk::draw` |
| `scroll(vec![..])` | vertically scrolling column (settings pages); children need `.fixed`, `.fixed_with` or a natural height (`list`); wheel + thin themed scrollbar |
| `color_button(\|s\| Color, \|Color\| M)` | swatch; click opens a picker popover (saturation/brightness square, hue strip, hex field for pasting, Done). Sends while dragging, so preview live and debounce saving. Closes on Done, Enter, Escape, a tap outside, or a window resize. Drawn inside the app window (overlay group created on open, deleted on close): no extra surface, same on Wayland/X11 |
| `popover(anchor, \|s\| bool, close_msg, \|s\| (w, h), content)` | `content` drops down centered under `anchor`, unrolling (~140 ms; rolls up on close), (above it at the bottom of the screen) while `open(state)`; `close_msg` is sent when the user closes it (click outside, Escape). Size follows `size(state)` while open. Content is built once with the view (hidden window: no surface). Wayland + fork: a real xdg_popup with a grab (also from layer-shell panels; open it on a button *press*: use `press_button`, act when `b.value()`); X11: override window + pointer grab |
| `popover_at(anchor, \|s\| Option<(x,y,w,h)>, ...)` | same, dropping down from part of the anchor (one button of a widget that draws several), rect relative to the anchor |
| `set_popover_radius(Option<i32>)` | popovers' corner radius (None: theme), e.g. to match a panel's style. Tooltips pause while a popover is open (Wayland allows popups only on the topmost one) |
| `press_button(draw)` | `custom_button` whose callback also fires on press (for opening popovers) |
| `icon(\|s\| String, size)` | icon centered in its space: built-in line icon in the theme text color, freedesktop theme icon (app icons, `Icon=` names), or an image path. Give it `.fixed(size + 6)` in a row |
| `list(\|s\| usize, \|i\| Element)` | column rebuilt when count changes; items should be `.fixed` |
| `embed(lens, map, child)` | plug in a component (api.md) |

Typical heights: row of buttons/inputs 34, toggle 28, list item 30, progress 10, heading row 36.

Helpers for your own widgets: `custom_button(draw)` (cheap clickable base), `repaint(&mut w)`
(redraw with background), `heroui::hover::is_hovered(&w)`, `heroui::hover::hover_amount(&w)`
(0..1, fades in/out ~120 ms: blend the hover color by it, e.g. `mix(bg, hover_bg, a)`),
`heroui::popup::context_menu(&["Pin", "-Close"])` (right-click menu at the mouse, blocks,
`Some(index)`; "-" = line above; call it from the click's handler, with no RefCell borrowed:
it runs FLTK's menu loop), `mix(a, b, t)` (blend colors),
`heroui::anim::animate(heroui::anim::SHORT, move |t| { pos.set(..t..); w.redraw() })` for
transitions: eased t 0→1 over ~150 ms, repaints only that widget while it runs, and jumps
straight to 1.0 when the theme has `animations = false`. `toggle` uses it for its knob. For a value that keeps changing, `heroui::anim::Tween`:
`tween.animate_to(target, duration, move || w.redraw())` (a new target cancels the running
move). `tween.follow(..)` moves at constant speed instead: successive targets join into
one continuous motion (what `progress` uses; measured: steady 3-5 px per 30 ms, no stalls).

Icons in custom drawing: `heroui::icons::draw(name, x, y, size, color) -> bool` (false = not
found, nothing drawn). Built-ins (`heroui::icons::BUILTIN`): apps, app, battery,
battery-<0..100>[-charging], clock, cpu, memory, network-wired/-wireless/-offline, power,
search, settings, terminal, volume-high/-low/-muted, network-wireless-<0..100> (signal), lock,
check, refresh, microphone(-muted), bluetooth(-connected/-off), arrow-up/-down. Each (name, size, color) is rasterized
once and kept at drawn size (~2 KB for 24 px); theme lookups are cached, misses too.
`heroui::icons::find(name)` = the file path; `exists(name)`.

Tooltips: plain FLTK, themed by HeroUI (colors, font, 0.5 s delay): `w.set_tooltip(&text)` in
a binding when the text changes; for areas of one widget, `fltk::misc::Tooltip::enter_area`
(needs `&'static CStr`: intern the texts). Shown as popups on Wayland too (layer surfaces).

Not built in yet (use `Element::new`, see patterns.md and `examples/custom_widget.rs`):
multi-line text, menus, tabs, general popovers (a dropdown panel with arbitrary content,
beyond `popover`; `color_picker.rs` shows the in-window overlay technique).
