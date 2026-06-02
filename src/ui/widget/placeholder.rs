use ratatui::{
    prelude::*,
    widgets::{Paragraph, Widget, Wrap},
};

pub struct PlaceholderWidget<'a> {
    text: Paragraph<'a>,
}

impl<'a> PlaceholderWidget<'a> {
    pub fn new(text: Paragraph<'a>) -> Self {
        Self { text }
    }

    pub fn dark_gray<T: Into<Text<'a>>>(text: T) -> Self {
        Self {
            text: Paragraph::new(text.into().dark_gray())
                .centered()
                .wrap(Wrap { trim: false }),
        }
    }

    pub fn yellow<T: Into<Text<'a>>>(text: T) -> Self {
        Self {
            text: Paragraph::new(text.into().yellow())
                .centered()
                .wrap(Wrap { trim: false }),
        }
    }

    pub fn red<T: Into<Text<'a>>>(text: T) -> Self {
        Self {
            text: Paragraph::new(text.into().red()).centered().wrap(Wrap { trim: false }),
        }
    }
}

impl<'a> Widget for PlaceholderWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let v = Layout::vertical([
            Constraint::Fill(100),
            Constraint::Length(self.text.line_count(area.width) as u16),
            Constraint::Fill(101),
        ])
        .split(area);

        self.text.render(v[1], buf);
    }
}
