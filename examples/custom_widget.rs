//! Extending HeroUI from outside the crate with `Element::new`: a clickable
//! custom-drawn widget and a container element, next to the built-in
//! checkbox and dropdown. cargo run --example custom_widget

use std::cell::Cell;
use std::rc::Rc;

use heroui::fltk::draw;
use heroui::fltk::{enums::*, group::Flex, prelude::*};
use heroui::hover::is_hovered;
use heroui::prelude::*;

/// A clickable custom widget: a color swatch that shows whether it's the
/// selected one. Built on `custom_button`, so FLTK handles clicks natively
/// and hover comes for free: no `handle` closure.
fn swatch<S: 'static, M: Clone + 'static>(
    color: Color,
    selected: impl Fn(&S) -> bool + 'static,
    msg: M,
) -> Element<S, M> {
    Element::new(move |ctx| {
        let t = ctx.theme_rc();
        let on = Rc::new(Cell::new(false));
        let mut b = custom_button({
            let on = on.clone();
            move |b| {
                let r = b.w().min(b.h());
                let (x, y) = (b.x() + (b.w() - r) / 2, b.y() + (b.h() - r) / 2);
                if on.get() || is_hovered(b) {
                    draw::set_draw_color(if on.get() { t.text } else { t.text_dim });
                    draw::draw_pie(x, y, r, r, 0.0, 360.0);
                }
                draw::set_draw_color(color);
                draw::draw_pie(x + 3, y + 3, r - 6, r - 6, 0.0, 360.0);
            }
        });
        let emit = ctx.emitter();
        b.set_callback(move |_| emit(msg.clone()));
        let mut w = b.clone();
        ctx.bind(move |s| {
            let v = selected(s);
            if on.replace(v) != v {
                w.redraw();
            }
        });
        b.as_base_widget()
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
    accent: usize,
}

#[derive(Clone)]
enum Msg {
    Autostart(bool),
    Size(usize),
    Accent(usize),
}

const SIZES: &[&str] = &["Small", "Medium", "Large", "Huge", "Enormous"];
const ACCENTS: [u32; 5] = [0xb46cff, 0x4c9aff, 0x3ec99a, 0xffb347, 0xff6b8b];

impl App for Demo {
    type Message = Msg;

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Autostart(on) => self.autostart = on,
            Msg::Size(i) => self.size = i,
            Msg::Accent(i) => self.accent = i,
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
                        dropdown(|_: &Demo| SIZES, |s: &Demo| s.size, Msg::Size),
                    ])
                    .fixed(32),
                ],
            )
            .fixed(110),
            row(ACCENTS
                .iter()
                .enumerate()
                .map(|(i, &c)| swatch(Color::from_hex(c), move |s: &Demo| s.accent == i, Msg::Accent(i)).fixed(32))
                .chain([spacer()])
                .collect())
            .fixed(32),
            text(|s: &Demo| format!("autostart={} size={} accent={}", s.autostart, SIZES[s.size], s.accent)).fixed(24),
            spacer(),
        ])
        .padding(16)
    }
}

fn main() {
    heroui::run(Demo::default(), Settings::new("Custom widgets").size(320, 240)).unwrap();
}
