//! `Element<S, M>`: a piece of UI that reads state `S` and emits messages `M`.
//!
//! Unlike a virtual-DOM Elm (iced, flemish), the view is built **once**.
//! Anything that depends on state is a *binding*: a small closure that
//! re-reads the state after every `update` and touches its widget only if
//! the value it shows actually changed. No widget tree is rebuilt or
//! diffed, so an idle or lightly-updating app costs next to nothing.

use std::cell::Cell;
use std::rc::Rc;

use fltk::group::Flex;
use fltk::prelude::*;
use fltk::widget::Widget;

use crate::theme::Theme;

/// A state-dependent update, run after every `update`.
pub type Binding<S> = Box<dyn FnMut(&S)>;

/// Passed to element builders: where messages go, where bindings are
/// registered, and the active theme.
pub struct Ctx<S, M> {
    emit: Rc<dyn Fn(M)>,
    bindings: Vec<Binding<S>>,
    theme: Rc<Theme>,
    /// Size hint of the element being built (see [`Ctx::size_hint`]).
    hint: Option<Rc<Cell<i32>>>,
}

impl<S: 'static, M: 'static> Ctx<S, M> {
    pub(crate) fn new(emit: Rc<dyn Fn(M)>, theme: Rc<Theme>) -> Self {
        Self { emit, bindings: Vec::new(), theme, hint: None }
    }

    /// A cloneable handle that sends `M` to the app's `update`. Capture it in
    /// widget callbacks.
    pub fn emitter(&self) -> Rc<dyn Fn(M)> {
        self.emit.clone()
    }

    /// Registers `f` to run with the current state after every update (and
    /// once right after the view is built).
    pub fn bind(&mut self, f: impl FnMut(&S) + 'static) {
        self.bindings.push(Box::new(f));
    }

    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// The shared theme, for capturing in draw closures (cheaper than
    /// cloning the `Theme`).
    pub fn theme_rc(&self) -> Rc<Theme> {
        self.theme.clone()
    }

    /// Builds `children` into `flex`, in order, applying each child's
    /// `.fixed()`. `flex` is begun and ended here. This is all a container
    /// element needs: create a `Flex` (or anything derived from it), style
    /// it, call this, return it.
    pub fn build_children(&mut self, flex: &mut Flex, children: Vec<Element<S, M>>) {
        flex.begin();
        for child in children {
            let fixed = child.fixed_size();
            let w = child.build(self);
            if let Some(px) = fixed {
                flex.fixed(&w, px);
            }
        }
        flex.end();
    }

    /// A fresh context sharing this one's emitter and theme but with no
    /// bindings. For containers that rebuild their children (see `list`):
    /// build the children with it, then take its [`Ctx::into_bindings`] and
    /// run them from a binding of your own.
    pub fn child(&self) -> Ctx<S, M> {
        Ctx::new(self.emit.clone(), self.theme.clone())
    }

    pub fn into_bindings(self) -> Vec<Binding<S>> {
        self.bindings
    }

    /// Inside [`Element::new`]: the size hint of the element being built,
    /// which an element whose natural size changes (like `list`) can update
    /// for containers that size children themselves (like `scroll`).
    /// A value below 0 means "no hint".
    pub fn size_hint(&self) -> Option<Rc<Cell<i32>>> {
        self.hint.clone()
    }
}

type Build<S, M> = Box<dyn FnOnce(&mut Ctx<S, M>) -> Widget>;
type Predicate<S> = Box<dyn Fn(&S) -> bool>;
type SizeFn<S> = Box<dyn Fn(&S) -> i32>;

/// A buildable piece of UI. Create with the functions in
/// [`crate::widgets`], or [`Element::new`] to wrap any raw fltk widget.
pub struct Element<S, M> {
    build: Build<S, M>,
    fixed: Option<i32>,
    padding: Option<i32>,
    spacing: Option<i32>,
    visible: Option<Predicate<S>>,
    enabled: Option<Predicate<S>>,
    fixed_fn: Option<SizeFn<S>>,
    /// Size along the parent's axis as last known: fixed, computed, or
    /// natural (-1: none). Read by `scroll`.
    hint: Rc<Cell<i32>>,
}

impl<S: 'static, M: 'static> Element<S, M> {
    /// Escape hatch: build any fltk widget. Create it with a zero size
    /// (`Widget::default()`), parent layouts size it. Register bindings on
    /// `ctx` for anything that depends on state.
    ///
    /// ```ignore
    /// Element::new(|ctx| {
    ///     let mut dial = fltk::valuator::Dial::default();
    ///     ctx.bind({ let mut d = dial.clone(); move |s: &State| d.set_value(s.level) });
    ///     dial.as_base_widget()
    /// })
    /// ```
    pub fn new(build: impl FnOnce(&mut Ctx<S, M>) -> Widget + 'static) -> Self {
        Self {
            build: Box::new(build),
            fixed: None,
            padding: None,
            spacing: None,
            visible: None,
            enabled: None,
            fixed_fn: None,
            hint: Rc::new(Cell::new(-1)),
        }
    }

    /// Fixed size (px) along the parent row/column's axis: height inside a
    /// column, width inside a row. Unfixed children share the leftover space.
    pub fn fixed(mut self, px: i32) -> Self {
        self.fixed = Some(px);
        self.hint.set(px);
        self
    }

    /// Like [`Element::fixed`], with the size computed from state, e.g.
    /// `.fixed_with(|s: &App| if s.expanded { 200 } else { 40 })`.
    pub fn fixed_with(mut self, f: impl Fn(&S) -> i32 + 'static) -> Self {
        self.fixed_fn = Some(Box::new(f));
        self
    }

    /// Inner margin, for rows/columns/cards. Defaults to 0 for rows and
    /// columns, theme padding for cards.
    pub fn padding(mut self, px: i32) -> Self {
        self.padding = Some(px);
        self
    }

    /// Gap between children, for rows/columns. Defaults to theme spacing.
    pub fn spacing(mut self, px: i32) -> Self {
        self.spacing = Some(px);
        self
    }

    /// Shown only while `f` is true. Hidden children take no space.
    pub fn visible(mut self, f: impl Fn(&S) -> bool + 'static) -> Self {
        self.visible = Some(Box::new(f));
        self
    }

    /// Like [`Element::visible`], but appearing animates: it rises a
    /// little into place and fades in (pages switching, sections opening).
    /// Hiding is instant. On Wayland; elsewhere it's `visible`.
    pub fn transition(self, f: impl Fn(&S) -> bool + 'static) -> Self {
        let fixed = self.fixed;
        let mut e = Element::new(move |ctx| {
            use fltk::group::Group;
            let mut g = Group::default();
            g.set_frame(fltk::enums::FrameType::NoBox);
            let child = self.build(ctx);
            g.end();
            g.resizable(&child);
            {
                let mut child = child.clone();
                g.resize_callback(move |_, x, y, w, h| child.resize(x, y, w, h));
            }
            let t = crate::anim::Tween::new(1.0);
            g.super_draw(false);
            {
                let t = t.clone();
                let snap = crate::fx::Snapshot::new();
                g.draw(move |g| {
                    let v = t.get();
                    let rect = (g.x(), g.y(), g.w(), g.h());
                    let mut g2 = g.clone();
                    // The page's picture is reused for a few frames: drawing
                    // a whole page every frame is slow on ARM boards.
                    if v < 1.0 && snap.reuse(rect, 4, || g2.draw_children()) {
                        snap.paint((0.0, 0.0), (1.0, 1.0), (0.0, (1.0 - v) * 18.0), v.clamp(0.0, 1.0));
                        return;
                    }
                    snap.clear();
                    g.draw_children();
                });
            }
            let mut w = g.clone();
            let mut last = None;
            ctx.bind(move |s| {
                let v = f(s);
                if last == Some(v) {
                    return;
                }
                let first = last.is_none();
                last = Some(v);
                if v {
                    w.show();
                    if !first && crate::on_wayland() {
                        t.set(0.0);
                        let w2 = w.clone();
                        t.animate_ease(1.0, std::time::Duration::from_millis(340), crate::anim::ease_out_quint, move || {
                            if let Some(mut win) = w2.window() {
                                win.set_damage_area(fltk::enums::Damage::All, w2.x(), w2.y(), w2.w(), w2.h());
                            }
                        });
                    }
                } else {
                    w.hide();
                }
                relayout_parent(&w);
            });
            g.as_base_widget()
        });
        e.fixed = fixed;
        e
    }

    /// Interactive only while `f` is true (greyed out otherwise).
    pub fn enabled(mut self, f: impl Fn(&S) -> bool + 'static) -> Self {
        self.enabled = Some(Box::new(f));
        self
    }

    /// The size set with [`Element::fixed`], for containers that lay out
    /// children themselves.
    pub fn fixed_size(&self) -> Option<i32> {
        self.fixed
    }

    /// The element's current size along its parent's axis (fixed, computed
    /// by `fixed_with`, or natural for lists); below 0 if unknown. Shared:
    /// it keeps updating after the element is built.
    pub fn size_hint(&self) -> Rc<Cell<i32>> {
        self.hint.clone()
    }

    /// Builds the widget into the fltk group that is currently open and
    /// registers its bindings on `ctx`. Containers usually want
    /// [`Ctx::build_children`] instead.
    pub fn build(self, ctx: &mut Ctx<S, M>) -> Widget {
        let outer = ctx.hint.replace(self.hint.clone());
        let widget = (self.build)(ctx);
        ctx.hint = outer;

        if let Some(mut flex) = Flex::from_dyn_widget(&widget) {
            if let Some(p) = self.padding {
                flex.set_margin(p);
            }
            if let Some(s) = self.spacing {
                flex.set_pad(s);
            }
        }
        if let Some(f) = self.visible {
            let mut w = widget.clone();
            let mut last = None;
            ctx.bind(move |s| {
                let v = f(s);
                if last != Some(v) {
                    last = Some(v);
                    if v { w.show() } else { w.hide() }
                    relayout_parent(&w);
                }
            });
        }
        if let Some(f) = self.fixed_fn {
            let w = widget.clone();
            let hint = self.hint.clone();
            let mut first = true;
            ctx.bind(move |s| {
                let v = f(s).max(0);
                if v != hint.get() || first {
                    first = false;
                    hint.set(v);
                    if let Some(mut flex) = w.parent().and_then(|p| Flex::from_dyn_widget(&p)) {
                        flex.fixed(&w, v);
                    }
                    relayout_parent(&w);
                }
            });
        }
        if let Some(f) = self.enabled {
            let mut w = widget.clone();
            let mut last = None;
            ctx.bind(move |s| {
                let v = f(s);
                if last != Some(v) {
                    last = Some(v);
                    if v { w.activate() } else { w.deactivate() }
                    w.redraw();
                }
            });
        }
        widget
    }
}

/// Re-runs the layout of a widget's parent row/column after it changed
/// visibility or children, and redraws the parent.
pub fn relayout_parent<W: WidgetExt>(w: &W) {
    if let Some(parent) = w.parent() {
        if let Some(flex) = Flex::from_dyn_widget(&parent) {
            flex.recalc();
        }
        let mut p = parent;
        p.redraw();
    }
}

/// Embeds a reusable component written against its own state `T` and
/// message `TM` into a parent with state `S` and message `M`.
///
/// `lens` picks the component's state out of the parent's, `map` wraps its
/// messages into the parent's. In the parent's `update`, forward the
/// wrapped message to the component's own update function:
///
/// ```ignore
/// embed(|s: &App| &s.volume, Msg::Volume, volume_control())
/// // update: Msg::Volume(m) => self.volume.update(m),
/// ```
pub fn embed<S, M, T, TM>(
    lens: impl Fn(&S) -> &T + 'static,
    map: impl Fn(TM) -> M + 'static,
    child: Element<T, TM>,
) -> Element<S, M>
where
    S: 'static,
    M: 'static,
    T: 'static,
    TM: 'static,
{
    Element::new(move |ctx: &mut Ctx<S, M>| {
        let parent_emit = ctx.emitter();
        let emit: Rc<dyn Fn(TM)> = Rc::new(move |m| parent_emit(map(m)));
        let mut inner = Ctx::new(emit, ctx.theme_rc());
        let widget = child.build(&mut inner);
        let mut bindings = inner.into_bindings();
        ctx.bind(move |s| {
            let t = lens(s);
            for b in bindings.iter_mut() {
                b(t);
            }
        });
        widget
    })
}
