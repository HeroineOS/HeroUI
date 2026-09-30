# HeroUI index (read this, then load only what you need)

HeroUI = Elm-style layer on fltk-rs 1.5. `App { update, view, subscriptions }`, `heroui::run`.
`use heroui::prelude::*;` brings in everything below. Raw fltk is re-exported as `heroui::fltk`.

| Need | File |
|---|---|
| Common failures (stretching, blocking, custom draw) — skim ALWAYS | ref/mistakes.md |
| App trait, run/Settings, Task, Subscription, Element modifiers, embed, Ctx | ref/api.md |
| Widget functions and their signatures | ref/widgets.md |
| Patterns: components, background work, lists, custom widgets, tokio, headless tests | ref/patterns.md |
| Theme file keys, Theme API, settings-app use | ref/theme.md |
| Minimal working app | ref/example.md |

Rules of thumb: `view` runs ONCE, so state-dependent UI is a closure `|s: &MyApp| ...`, never an
`if` in `view`. Anything that can block goes in `Task::perform`. Size things with `.fixed(px)`.
