use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

pub struct TerminalSizeWidget {
    required_size: (u16, u16),
}

impl TerminalSizeWidget {
    pub fn new(required_size: (u16, u16)) -> Self {
        TerminalSizeWidget { required_size }
    }
}

impl Widget for TerminalSizeWidget {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let v = Layout::vertical([Constraint::Fill(1), Constraint::Length(3), Constraint::Fill(1)]).split(area);

        Paragraph::new(vec![
            Line::from(Span::from(" TERMINAL SIZE IS TOO SMALL! ").style(Style::new().white().on_red())),
            Line::default(),
            Line::from(format!(
                "{}x{} [{}x{}]",
                self.required_size.0, self.required_size.1, area.width, area.height
            )),
        ])
        .centered()
        .render(v[1], buf);
    }
}
