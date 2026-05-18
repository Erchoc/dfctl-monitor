use crate::commands::monitor::data::{Event, EventKind};
use crate::commands::monitor::render::colors::*;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Widget, Wrap};

pub struct EventsSidebar<'a> {
    pub events: &'a [Event],
}

impl<'a> Widget for EventsSidebar<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 20 || area.height < 5 {
            return;
        }
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .style(Style::default().fg(BORDER_DIM.to_color()))
            .title(Line::from(Span::styled(
                "  Recent events  ",
                Style::default()
                    .fg(TEXT_PRIMARY.to_color())
                    .add_modifier(Modifier::BOLD),
            )));
        let inner = block.inner(area);
        block.render(area, buf);
        if self.events.is_empty() {
            Paragraph::new(Line::from(Span::styled(
                " no events in range",
                Style::default().fg(TEXT_DIM.to_color()),
            )))
            .render(inner, buf);
            return;
        }
        let mut lines: Vec<Line> = Vec::new();
        for e in self.events.iter().rev() {
            let glyph_color = match e.kind {
                EventKind::AlertFired => ACCENT_ALERT,
                EventKind::AlertResolved => ACCENT_OK,
                EventKind::Restart => ACCENT_WARN,
                EventKind::Deploy => ACCENT_INFO,
                EventKind::ScaleEvent => ACCENT_SECONDARY,
            };
            let when = e
                .at
                .with_timezone(&chrono::Local)
                .format("%H:%M:%S")
                .to_string();
            lines.push(Line::from(vec![
                Span::styled(
                    format!(" {} ", e.kind.glyph()),
                    Style::default().fg(glyph_color.to_color()),
                ),
                Span::styled(
                    when,
                    Style::default()
                        .fg(TEXT_SECONDARY.to_color())
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
            lines.push(Line::from(Span::styled(
                format!("   {}", e.message),
                Style::default().fg(TEXT_PRIMARY.to_color()),
            )));
            lines.push(Line::raw(""));
        }
        Paragraph::new(lines).wrap(Wrap { trim: true }).render(inner, buf);
    }
}
