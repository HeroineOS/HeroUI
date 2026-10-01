# HeroUI

A lightweight Elm-style UI library on [fltk-rs](https://github.com/fltk-rs/fltk-rs) for
HeroineOS apps. State is a struct, messages are an enum, `update` changes state, `view`
describes the UI. It's themed modern by default, not the stock FLTK look.

Built for potato hardware:
- **No async runtime by default.** Background work runs on a short-lived plain thread and
  sends a message back. The optional `tokio` feature adds one current-thread runtime on
  one thread, started only on first use. A multi-thread tokio runtime measured +1 MB RSS
  and 12 idle threads per app, which is why it isn't the default.
- **No diffing, no rebuilds.** `view` runs once. State-dependent parts are bindings that
  touch a widget only when its value actually changed.
- **One theme file for every app**: `~/.config/heroui/theme.conf`.

- **Retained drawing.** Nothing is drawn unless something changed, and then only the changed
  widgets' rectangles. An idle app sleeps at 0% CPU.
- **For apps and desktop shells.** Window kinds for panels (with reserved space), desktop
  widgets (conky-like) and OSDs; `graph` and `canvas` for meters; themed dropdowns.
- **Native Wayland** by default, X11 otherwise (one binary, FLTK's hybrid backend).
  Panels, desktop widgets and notifications are native layer-shell surfaces with the
  `layer-shell` feature (see below).

Measured (release, Xvfb, Debian testing amd64; RSS includes ~12 MB of shared X/pango/fontconfig
pages, "anon" is the app's own memory; the 2nd thread is pango's fontconfig helper):

| Example | RSS | anon | CPU |
|---|---|---|---|
| showcase | 16.8 MB | 2.6 MB | 0% idle |
| sysmon (conky-like, 1 s refresh) | 16.3 MB | 2.7 MB | 0.1% |
| stress, 1000 rows × 4 bound widgets, 1 s tick | 18.0 MB | 4.5 MB | 0.2% |

On Wayland (headless sway): showcase 3.7 MB anon, sysmon 3.1 MB, panel 2.7 MB, all
≤0.1% CPU. About 1 MB more than X11 per window: on Wayland the app holds its own
window buffer (the X server holds it on X11).

Startup to visible window: ~130-150 ms. Examples: `counter`, `showcase`, `custom_widget`,
`sysmon`, `panel`, `stress`.

```toml
[dependencies]
heroui = { git = "https://github.com/HeroineOS/HeroUI" }
```

```sh
cargo run --release --example showcase
```

Build deps (Debian): `libx11-dev libxext-dev libxft-dev libxinerama-dev libxcursor-dev
libxrender-dev libxfixes-dev libpango1.0-dev libcairo2-dev libgl-dev cmake
libwayland-dev wayland-protocols libxkbcommon-dev libdbus-1-dev`.

## Wayland and desktop-shell windows

Regular apps need nothing: stock fltk-rs runs them natively on Wayland (popups and
dropdowns included). Panels, desktop widgets and notifications (`WindowKind::Dock`,
`Desktop`, `Notification`) need the wlr-layer-shell protocol, which upstream FLTK doesn't
support. Shell apps enable it with the `layer-shell` feature and the
[HeroineOS/fltk-sys](https://github.com/HeroineOS/fltk-sys) fork (fltk-sys 1.5.23 plus
layer-shell; same API, so other fltk crates keep working):

```toml
[dependencies]
heroui = { git = "https://github.com/HeroineOS/HeroUI", features = ["layer-shell"] }

[patch.crates-io]
fltk-sys = { git = "https://github.com/HeroineOS/fltk-sys" }
```

At startup a shell window checks the compositor. With layer-shell (HeroiWM, sway,
Hyprland, KDE, ...) it's a native layer surface. Without it (GNOME) or without the feature,
it runs on XWayland with X11 dock/desktop hints, or as a normal window if there's no
XWayland; a line on stderr says which.

## AI reference

This repo is also a compact reference for AI coding assistants, modeled on
[jts](https://github.com/Zexolver/jts). **AI: read [`INDEX.md`](INDEX.md) first and load
only the `ref/` file(s) you need.** Always skim `ref/mistakes.md`.

License: MIT OR Apache-2.0.
