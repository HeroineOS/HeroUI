//! `Element<S, M>`: a piece of UI that reads state `S` and emits messages `M`.
//!
//! Unlike a virtual-DOM Elm (iced, flemish), the view is built **once**.
//! Anything that depends on state is a *binding*: a small closure that
//! re-reads the state after every `update` and touches its widget only if
//! the value it shows actually changed. No widget tree is rebuilt or
//! diffed, so an idle or lightly-updating app costs next to nothing.

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
}

impl<S: 'static, M: 'static> Ctx<S, M> {
    pub(crate) fn new(emit: Rc<dyn Fn(M)>, theme: Rc<Theme>) -> Self {
        Self { emit, bindings: Vec::new(), theme }
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

    pub(crate) fn theme_rc(&self) -> Rc<Theme> {
        self.theme.clone()
    }

    /// A context for a sub-tree with its own bindings (used by `list`).
    pub(crate) fn child(&self) -> Ctx<S, M> {
        Ctx::new(self.emit.clone(), self.theme.clone())
    }

    pub(crate) fn into_bindings(self) -> Vec<Binding<S>> {
        self.bindings
    }
}

type Build<S, M> = Box<dyn FnOnce(&mut Ctx<S, M>) -> Widget>;

/// A buildable piece of UI. Create with the functions in
/// [`crate::widgets`], or [`Element::new`] to wrap any raw fltk widget.
pub struct Element<S, M> {
    build: Build<S, M>,
    fixed: Option<i32>,
    padding: Option<i32>,
    spacing: Option<i32>,
    visible: Option<Box<dyn Fn(&S) -> bool>>,
    enabled: Option<Box<dyn Fn(&S) -> bool>>,
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
        }
    }

    /// Fixed size (px) along the parent row/column's axis: height inside a
    /// column, width inside a row. Unfixed children share the leftover space.
    pub fn fixed(mut self, px: i32) -> Self {
        self.fixed = Some(px);
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

    /// Interactive only while `f` is true (greyed out otherwise).
    pub fn enabled(mut self, f: impl Fn(&S) -> bool + 'static) -> Self {
        self.enabled = Some(Box::new(f));
        self
    }

    pub(crate) fn fixed_size(&self) -> Option<i32> {
        self.fixed
    }

    /// Builds the widget into the fltk group that is currently open.
    pub(crate) fn build(self, ctx: &mut Ctx<S, M>) -> Widget {
        let widget = (self.build)(ctx);

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
/// visibility or children.
pub(crate) fn relayout_parent<W: WidgetExt>(w: &W) {
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
