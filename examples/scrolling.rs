//! A long settings-style page in a `scroll`: fixed rows, a list whose height
//! follows its items, and a section that grows with `fixed_with`.
//! cargo run --example scrolling

use heroui::prelude::*;

#[derive(Default)]
struct Page {
    flags: Vec<bool>,
    items: Vec<String>,
    more: bool,
}

#[derive(Clone)]
enum Msg {
    Flag(usize, bool),
    Add,
    Remove(usize),
    More(bool),
}

impl App for Page {
    type Message = Msg;

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Flag(i, on) => self.flags[i] = on,
            Msg::Add => {
                let n = self.items.len() + 1;
                self.items.push(format!("Item {n}"));
            }
            Msg::Remove(i) => {
                if i < self.items.len() {
                    self.items.remove(i);
                }
            }
            Msg::More(on) => self.more = on,
        }
        Task::none()
    }

    fn view(&self) -> Element<Self, Msg> {
        let mut rows: Vec<Element<Self, Msg>> = vec![heading("Settings").fixed(36)];
        for i in 0..self.flags.len() {
            rows.push(toggle(&format!("Option {}", i + 1), move |s: &Page| s.flags[i], move |on| Msg::Flag(i, on)).fixed(28));
        }
        rows.push(row(vec![label("Items"), button("Add", Msg::Add).fixed(80)]).fixed(34));
        rows.push(list(
            |s: &Page| s.items.len(),
            |i| {
                row(vec![
                    text(move |s: &Page| s.items.get(i).cloned().unwrap_or_default()),
                    button("Remove", Msg::Remove(i)).fixed(80),
                ])
                .fixed(30)
            },
        ));
        rows.push(toggle("Show more", |s: &Page| s.more, Msg::More).fixed(28));
        rows.push(
            card(vec![caption("Extra settings"), label("Shown when 'Show more' is on")])
                .fixed_with(|s: &Page| if s.more { 120 } else { 0 })
                .visible(|s: &Page| s.more),
        );
        rows.push(caption("End of page").fixed(20));
        column(vec![scroll(rows)]).padding(16)
    }
}

fn main() {
    let page = Page { flags: vec![false; 12], items: vec!["Item 1".into(), "Item 2".into()], more: false };
    heroui::run(page, Settings::new("Scrolling").size(380, 420)).unwrap();
}
