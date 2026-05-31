//! Small text-only widgets for the trace TUI: help overlay and the
//! "terminal too small" notice. The heavy graphics (waterfall, minimap,
//! summary) live in `render.rs`.

use crate::tui::colors::*;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Widget};

pub const DOCS_URL: &str = "https://docs.dfctl.com/cli/trace";

pub struct TooSmall {
    pub w: u16,
    pub h: u16,
}

impl Widget for TooSmall {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .style(Style::default().fg(ACCENT_WARN.to_color()));
        let inner = block.inner(area);
        block.render(area, buf);
        let warn = Style::default().fg(ACCENT_WARN.to_color()).add_modifier(Modifier::BOLD);
        let txt = Style::default().fg(TEXT_PRIMARY.to_color());
        let dim = Style::default().fg(TEXT_SECONDARY.to_color());
        let put = |buf: &mut Buffer, y: u16, s: String, st: Style| {
            let len = s.chars().count() as u16;
            let x = inner.x + inner.width.saturating_sub(len) / 2;
            Paragraph::new(Line::from(Span::styled(s, st)))
                .render(Rect::new(x, y, len.min(inner.width), 1), buf);
        };
        let mut y = inner.y + inner.height / 4;
        put(buf, y, "⚠  Terminal too small".into(), warn);
        y += 2;
        put(buf, y, "dfctl trace needs at least 60 × 18".into(), txt);
        y += 1;
        put(buf, y, format!("your terminal is {} × {}", self.w, self.h), dim);
        y += 2;
        put(buf, y, "options: resize  ·  dfctl trace <id> --json".into(), dim);
    }
}

pub struct HelpOverlay;

impl Widget for HelpOverlay {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let w = 64.min(area.width);
        let h = 24.min(area.height);
        let x = area.x + (area.width.saturating_sub(w)) / 2;
        let y = area.y + (area.height.saturating_sub(h)) / 2;
        let outer = Rect::new(x, y, w, h);
        Clear.render(outer, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .style(Style::default().fg(ACCENT_OK.to_color()))
            .title(Line::from(Span::styled(
                " dfctl trace — help ",
                Style::default().fg(ACCENT_OK.to_color()).add_modifier(Modifier::BOLD),
            )));
        let inner = block.inner(outer);
        block.render(outer, buf);

        let key = |k: &str| {
            Span::styled(
                format!("{:<10}", k),
                Style::default().fg(ACCENT_WARN.to_color()).add_modifier(Modifier::BOLD),
            )
        };
        let desc = |d: &str| Span::styled(d.to_string(), Style::default().fg(TEXT_PRIMARY.to_color()));
        let section = |s: &str| {
            Span::styled(
                s.to_string(),
                Style::default().fg(TEXT_SECONDARY.to_color()).add_modifier(Modifier::BOLD),
            )
        };
        let lines = vec![
            Line::from(vec![section(" Navigation")]),
            Line::from(vec![Span::raw(" "), key("↑↓ / jk"), desc("move selected span")]),
            Line::from(vec![Span::raw(" "), key("← / h"), desc("collapse subtree")]),
            Line::from(vec![Span::raw(" "), key("→ / l"), desc("expand subtree")]),
            Line::from(vec![Span::raw(" "), key("g / G"), desc("first / last span")]),
            Line::from(vec![Span::raw(" "), key("Enter"), desc("span detail")]),
            Line::from(vec![Span::raw(" "), key("Esc"), desc("back to waterfall")]),
            Line::from(vec![Span::raw("")]),
            Line::from(vec![section(" View / FX")]),
            Line::from(vec![Span::raw(" "), key("s"), desc("toggle summary")]),
            Line::from(vec![Span::raw(" "), key("c"), desc("focus critical path")]),
            Line::from(vec![Span::raw(" "), key("f"), desc("play flow animation")]),
            Line::from(vec![Span::raw(" "), key("e / E"), desc("next / prev error span")]),
            Line::from(vec![Span::raw(" "), key("w / space"), desc("watch / pause")]),
            Line::from(vec![Span::raw(" "), key("q / C-c"), desc("quit")]),
            Line::from(vec![Span::raw("")]),
            Line::from(vec![
                Span::styled("  docs: ", Style::default().fg(TEXT_DIM.to_color())),
                Span::styled(
                    DOCS_URL,
                    Style::default().fg(ACCENT_INFO.to_color()).add_modifier(Modifier::UNDERLINED),
                ),
            ]),
            Line::from(vec![Span::styled(
                "  press any key to dismiss",
                Style::default().fg(TEXT_DIM.to_color()),
            )]),
        ];
        Paragraph::new(lines).render(inner, buf);
    }
}
