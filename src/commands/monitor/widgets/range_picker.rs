use crate::commands::monitor::render::colors::*;
use crate::commands::monitor::state::RANGE_OPTIONS;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Widget};

pub struct RangePicker {
    pub selected: usize,
}

impl Widget for RangePicker {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let w = 36.min(area.width);
        let h = (RANGE_OPTIONS.len() as u16 + 4).min(area.height);
        let x = area.x + (area.width.saturating_sub(w)) / 2;
        let y = area.y + (area.height.saturating_sub(h)) / 2;
        let outer = Rect::new(x, y, w, h);
        Clear.render(outer, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .style(Style::default().fg(ACCENT_OK.to_color()))
            .title(Line::from(Span::styled(
                " Range ",
                Style::default().fg(ACCENT_OK.to_color()).add_modifier(Modifier::BOLD),
            )));
        let inner = block.inner(outer);
        block.render(outer, buf);

        let mut lines = Vec::new();
        for (i, (code, label)) in RANGE_OPTIONS.iter().enumerate() {
            let marker = if i == self.selected { "▶ " } else { "  " };
            let style = if i == self.selected {
                Style::default()
                    .fg(ACCENT_OK.to_color())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TEXT_PRIMARY.to_color())
            };
            lines.push(Line::from(vec![
                Span::styled(marker.to_string(), style),
                Span::styled(format!("{:<6}", code), style),
                Span::styled(label.to_string(), Style::default().fg(TEXT_SECONDARY.to_color())),
            ]));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  ↑↓ select   ⏎ apply   esc cancel",
            Style::default().fg(TEXT_DIM.to_color()),
        )));
        Paragraph::new(lines).render(inner, buf);
    }
}
