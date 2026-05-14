use itertools::Itertools;
use ratatui::prelude::*;

pub struct TabsWidget {
    tabs: Vec<(usize, String)>,
    active_key: usize,
}

impl TabsWidget {
    pub fn new(tabs: Vec<(usize, String)>, active: usize) -> Self {
        Self {
            tabs,
            active_key: active,
        }
    }
}

impl Widget for TabsWidget {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        #[allow(unstable_name_collisions)]
        let spans: Vec<Span> = self
            .tabs
            .iter()
            .map(|(key, title)| {
                if key == &self.active_key {
                    Span::from(format!(" {} ", title)).black().on_yellow()
                } else {
                    Span::from(title)
                }
            })
            .intersperse(Span::from("  "))
            .collect();

        Line::from(spans).render(area, buf);
    }
}
