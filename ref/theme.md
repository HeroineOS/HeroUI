# Theme

File: `$XDG_CONFIG_HOME/heroui/theme.conf` (else `~/.config/heroui/theme.conf`), read by
every HeroUI app at startup (`App::theme` default → `Theme::load()`). Plain `key = value`,
`#` comments, colors `#rrggbb`. Unknown keys and bad values are ignored; missing keys keep
the dark default.

```ini
background = #14141c   # window
surface = #1f1f2b      # cards, buttons
surface_alt = #2a2a3a  # hover, inputs, slider/toggle tracks
text = #e6e6f0
text_dim = #9090a8     # captions, disabled
accent = #b46cff       # primary buttons, active toggles/sliders, selection
accent_text = #14141c  # text on accent
border = #33334a
radius = 10            # px
spacing = 8            # gap in rows/columns
padding = 12           # card inner margin
font_size = 14
font = Inter           # empty = FLTK default sans
animations = true      # false = reduced motion / battery saving: transitions jump to the end
icon_theme = Papirus   # app icons; empty/missing = GTK's gtk-icon-theme-name, then hicolor/Adwaita
```

API: `Theme::dark()`, `Theme::light()`, `Theme::load()`, `Theme::path()`,
`t.apply_conf(&str)`, `t.to_conf() -> String`, `t.save() -> io::Result<()>` (settings app
writes the file), `t.apply()` (sets FLTK globals; `run` does this, only needed for raw fltk
windows), `t.font()` / `t.bold_font()`.

`heroui::theme::ROUNDED` is a box type drawing a rounded rect in the widget's color at the
theme radius: `widget.set_frame(ROUNDED)`.

Per-app override: implement `fn theme(&self) -> Theme { Theme::light() }` (or load then
tweak). Apps don't live-reload the file yet; restart the app to see changes.
