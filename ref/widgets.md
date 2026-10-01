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
| `toggle("label", \|s\| bool, \|bool\| M)` | switch, label on the left; give it `.fixed(28)` in a column |
| `checkbox("label", \|s\| bool, \|bool\| M)` | box + label on the right |
| `dropdown(\|s\| &[T], \|s\| usize, \|usize\| M)` | `T: AsRef<str>`; static list `\|_: &S\| CONST_SLICE` or from state `\|s\| &s.names`; list is FLTK's own menu popup (as Fl_Choice), themed; native xdg_popup on Wayland; opens on press and blocks in FLTK's menu loop until a pick |
| `slider(0.0..=100.0, \|s\| f64, \|f64\| M)` | horizontal, sends while dragging |
| `progress(\|s\| f64)` | read-only bar, value 0.0..=1.0 |
| `graph(\|s\| &[f64], max)` | filled line graph (history, oldest first); no allocation per update |
| `canvas(\|s\| D, paint)` | custom drawing; `D: PartialEq`, redrawn only when it changes; `paint(&D, x, y, w, h, &Theme)` with `fltk::draw` |
| `list(\|s\| usize, \|i\| Element)` | column rebuilt when count changes; items should be `.fixed` |
| `embed(lens, map, child)` | plug in a component (api.md) |

Typical heights: row of buttons/inputs 34, toggle 28, list item 30, progress 10, heading row 36.

Helpers for your own widgets: `custom_button(draw)` (cheap clickable base), `repaint(&mut w)`
(redraw with background), `heroui::hover::is_hovered(&w)`, `mix(a, b, t)` (blend colors).

Not built in yet (use `Element::new`, see patterns.md and `examples/custom_widget.rs`):
multi-line text, scroll area, images/icons, menus, tabs, Enter-to-submit on text_input.
