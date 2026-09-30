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

Measured (showcase example, release build, Xvfb, Debian testing amd64): 16.8 MB RSS, of
which 2.6 MB is anonymous (the rest is shared X/pango/fontconfig pages), 2 threads.

```toml
[dependencies]
heroui = { git = "https://github.com/HeroineOS/HeroUI" }
```

```sh
cargo run --release --example showcase
```

Build deps (Debian): `libx11-dev libxext-dev libxft-dev libxinerama-dev libxcursor-dev
libxrender-dev libxfixes-dev libpango1.0-dev libcairo2-dev libgl-dev cmake`.

## AI reference

This repo is also a compact reference for AI coding assistants, modeled on
[jts](https://github.com/Zexolver/jts). **AI: read [`INDEX.md`](INDEX.md) first and load
only the `ref/` file(s) you need.** Always skim `ref/mistakes.md`.

License: MIT OR Apache-2.0.
