//! Extending HeroUI from outside the crate with `Element::new`: a themed
//! checkbox, a stock `Choice` dropdown, and a container element.
//! cargo run --example custom_widget

use std::cell::Cell;
use std::rc::Rc;

use heroui::fltk::draw::{self, LineStyle};
use heroui::fltk::{enums::*, frame::Frame, group::Flex, menu::Choice, prelude::*};
use heroui::prelude::*;

/// A checkbox drawn entirely by us on a `Frame` (no stock look).
fn checkbox<S: 'static, M: 'static>(
    label: &str,
    value: impl Fn(&S) -> bool + 'static,
    on_toggle: impl Fn(bool) -> M + 'static,
) -> Element<S, M> {
    let label = label.to_string();
    Element::new(move |ctx| {
        let t = ctx.theme().clone();
        // Widget-local copy of the state value, shared by draw/handle/bind.
        let on = Rc::new(Cell::new(false));
        let mut f = Frame::default();
        f.draw({
            let on = on.clone();
            move |f| {
                let s = 18;
                let (x, y) = (f.x(), f.y() + (f.h() - s) / 2);
                draw::set_draw_color(if on.get() { t.accent } else { t.surface_alt });
                draw::draw_rounded_rectf(x, y, s, s, 4);
                if on.get() {
                    draw::set_draw_color(t.accent_text);
                    draw::set_line_style(LineStyle::Solid, 2);
                    draw::draw_line(x + 4, y + 9, x + 8, y + 13);
                    draw::draw_line(x + 8, y + 13, x + 14, y + 5);
                    draw::set_line_style(LineStyle::Solid, 0);
                }
                draw::set_font(t.font(), t.font_size);
                // active_r(): dims inside a disabled row/column too.
                draw::set_draw_color(if f.active_r() { t.text } else { t.text_dim });
                draw::draw_text2(&label, x + s + 8, f.y(), f.w() - s - 8, f.h(), Align::Left);
            }
        });
        let emit = ctx.emitter();
        f.handle({
            let on = on.clone();
            // Don't flip `on` here: send the message, the binding updates it.
            move |f, ev| match ev {
                Event::Push => true,
                // Releasing outside cancels, like a button.
                Event::Released => {
                    if heroui::fltk::app::event_inside_widget(f) {
                        emit(on_toggle(!on.get()));
                    }
                    true
                }
                _ => false,
            }
        });
        let mut w = f.clone();
        ctx.bind(move |s| {
            let v = value(s);
            if on.get() != v {
                on.set(v);
                w.redraw();
            }
        });
        f.as_base_widget()
    })
}

fn choice<S: 'static, M: 'static>(
    options: &'static [&'static str],
    selected: impl Fn(&S) -> usize + 'static,
    on_select: impl Fn(usize) -> M + 'static,
) -> Element<S, M> {
    Element::new(move |ctx| {
        let t = ctx.theme().clone();
        let mut c = Choice::default();
        c.add_choice(&options.join("|"));
        c.set_frame(heroui::theme::ROUNDED);
        c.set_down_frame(heroui::theme::ROUNDED);
        c.set_color(t.surface_alt);
        c.set_text_color(t.text);
        c.set_selection_color(t.accent);
        let emit = ctx.emitter();
        c.set_callback(move |c| {
            if c.value() >= 0 {
                emit(on_select(c.value() as usize));
            }
        });
        let mut w = c.clone();
        ctx.bind(move |s| {
            let v = selected(s) as i32;
            if w.value() != v {
                w.set_value(v);
            }
        });
        c.as_base_widget()
    })
}

/// A container: a titled, outlined group of children. Any container is
/// "make a Flex, style it, `ctx.build_children`".
fn section<S: 'static, M: 'static>(title: &str, children: Vec<Element<S, M>>) -> Element<S, M> {
    let title = title.to_string();
    Element::new(move |ctx| {
        let t = ctx.theme().clone();
        let mut flex = Flex::default().column();
        flex.end();
        flex.set_pad(t.spacing);
        // Space at the top for the title. This draw callback runs after the
        // children are drawn, so it only touches the border area.
        flex.set_margins(t.padding, t.padding + t.font_size, t.padding, t.padding);
        flex.draw(move |f| {
            draw::set_draw_color(t.border);
            draw::draw_rounded_rect(f.x(), f.y() + t.font_size / 2, f.w(), f.h() - t.font_size / 2, t.radius);
            draw::set_font(t.bold_font(), t.font_size - 2);
            let tw = draw::width(&title) as i32 + 8;
            draw::set_draw_color(t.background);
            draw::draw_rectf(f.x() + t.radius, f.y(), tw, t.font_size);
            draw::set_draw_color(t.text_dim);
            draw::draw_text2(&title, f.x() + t.radius + 4, f.y(), tw, t.font_size, Align::Left);
        });
        ctx.build_children(&mut flex, children);
        flex.as_base_widget()
    })
}

#[derive(Default)]
struct Demo {
    autostart: bool,
    size: usize,
}

#[derive(Clone)]
enum Msg {
    Autostart(bool),
    Size(usize),
}

const SIZES: &[&str] = &["Small", "Medium", "Large"];

impl App for Demo {
    type Message = Msg;

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Autostart(on) => self.autostart = on,
            Msg::Size(i) => self.size = i,
        }
        Task::none()
    }

    fn view(&self) -> Element<Self, Msg> {
        column(vec![
            section(
                "Session",
                vec![
                    checkbox("Start on login", |s: &Demo| s.autostart, Msg::Autostart).fixed(28),
                    row(vec![
                        label("Size").fixed(60),
                        choice(SIZES, |s: &Demo| s.size, Msg::Size),
                    ])
                    .fixed(32),
                ],
            )
            .fixed(110),
            text(|s: &Demo| format!("autostart={} size={}", s.autostart, SIZES[s.size])).fixed(24),
            spacer(),
        ])
        .padding(16)
    }
}

fn main() {
    heroui::run(Demo::default(), Settings::new("Custom widgets").size(320, 200)).unwrap();
}
