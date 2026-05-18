use crate::commands::monitor::render::colors::*;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Widget};

pub struct HelpOverlay;

impl Widget for HelpOverlay {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // centered overlay
        let w = 70.min(area.width);
        let h = 22.min(area.height);
        let x = area.x + (area.width.saturating_sub(w)) / 2;
        let y = area.y + (area.height.saturating_sub(h)) / 2;
        let outer = Rect::new(x, y, w, h);
        Clear.render(outer, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .style(Style::default().fg(ACCENT_OK.to_color()))
            .title(Line::from(Span::styled(
                " df monitor — help ",
                Style::default()
                    .fg(ACCENT_OK.to_color())
                    .add_modifier(Modifier::BOLD),
            )));
        let inner = block.inner(outer);
        block.render(outer, buf);

        let key = |k: &str| Span::styled(format!("{:<10}", k), Style::default().fg(ACCENT_WARN.to_color()).add_modifier(Modifier::BOLD));
        let desc = |d: &str| Span::styled(d.to_string(), Style::default().fg(TEXT_PRIMARY.to_color()));
        let section = |s: &str| Span::styled(s.to_string(), Style::default().fg(TEXT_SECONDARY.to_color()).add_modifier(Modifier::BOLD));

        let lines = vec![
            Line::from(vec![section(" Navigation")]),
            Line::from(vec![Span::raw(" "), key("↑↓←→/hjkl"), desc("move focus")]),
            Line::from(vec![Span::raw(" "), key("Tab/S-Tab"), desc("cycle panels")]),
            Line::from(vec![Span::raw(" "), key("Enter"), desc("open metric detail")]),
            Line::from(vec![Span::raw(" "), key("Esc"), desc("back to overview")]),
            Line::from(vec![Span::raw("")]),
            Line::from(vec![section(" Filters")]),
            Line::from(vec![Span::raw(" "), key("a"), desc("cycle aggregation")]),
            Line::from(vec![Span::raw(" "), key("u"), desc("toggle traffic unit")]),
            Line::from(vec![Span::raw(" "), key("t"), desc("time-range picker")]),
            Line::from(vec![Span::raw("")]),
            Line::from(vec![section(" Mode")]),
            Line::from(vec![Span::raw(" "), key("w"), desc("toggle watch")]),
            Line::from(vec![Span::raw(" "), key("space"), desc("pause / resume")]),
            Line::from(vec![Span::raw(" "), key("r"), desc("refresh now")]),
            Line::from(vec![Span::raw(" "), key("← →"), desc("prev / next metric (detail)")]),
            Line::from(vec![Span::raw(" "), key("?"), desc("help")]),
            Line::from(vec![Span::raw(" "), key("q / C-c"), desc("quit")]),
            Line::from(vec![Span::raw("")]),
            Line::from(vec![Span::styled(
                "  press any key to dismiss",
                Style::default().fg(TEXT_DIM.to_color()),
            )]),
        ];
        Paragraph::new(lines).render(inner, buf);
    }
}
