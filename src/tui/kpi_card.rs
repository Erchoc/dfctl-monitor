use crate::tui::colors::*;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Widget};

pub struct KpiCard<'a> {
    pub title: &'a str,
    pub value: String,
    pub value_color: Rgb,
    pub sub: String,
}

impl<'a> Widget for KpiCard<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .style(Style::default().fg(BORDER_GRID.to_color()));
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 3 || inner.width < 6 {
            return;
        }
        // marker
        let marker = Span::styled("◆ ", Style::default().fg(self.value_color.to_color()));
        let title = Span::styled(
            self.title.to_string(),
            Style::default()
                .fg(TEXT_SECONDARY.to_color())
                .add_modifier(Modifier::BOLD),
        );
        let line = Line::from(vec![marker, title]);
        Paragraph::new(line).render(
            Rect::new(inner.x, inner.y, inner.width, 1),
            buf,
        );

        let value_line = Line::from(Span::styled(
            self.value,
            Style::default()
                .fg(self.value_color.to_color())
                .add_modifier(Modifier::BOLD),
        ));
        if inner.height > 1 {
            Paragraph::new(value_line).render(
                Rect::new(inner.x, inner.y + 1, inner.width, 1),
                buf,
            );
        }

        let sub_line = Line::from(Span::styled(
            self.sub,
            Style::default().fg(TEXT_SECONDARY.to_color()),
        ));
        if inner.height > 2 {
            Paragraph::new(sub_line).render(
                Rect::new(inner.x, inner.y + 2, inner.width, 1),
                buf,
            );
        }
    }
}
