use crate::commands::monitor::render::colors::*;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Widget};

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
        let mut y = inner.y + inner.height / 4;
        let line = |s: &str, st: Style| Paragraph::new(Line::from(Span::styled(s.to_string(), st)));
        let warn = Style::default()
            .fg(ACCENT_WARN.to_color())
            .add_modifier(Modifier::BOLD);
        let txt = Style::default().fg(TEXT_PRIMARY.to_color());
        let dim = Style::default().fg(TEXT_SECONDARY.to_color());
        let centered = |area: Rect, txt: &str| {
            let len = txt.chars().count() as u16;
            let x = area.x + (area.width.saturating_sub(len)) / 2;
            Rect::new(x, 0, len.min(area.width), 1)
        };
        let render_centered = |buf: &mut Buffer, area: Rect, y: u16, s: String, st: Style| {
            let r = centered(area, &s);
            Paragraph::new(Line::from(Span::styled(s, st))).render(Rect::new(r.x, y, r.width, 1), buf);
        };
        render_centered(
            buf,
            inner,
            y,
            "⚠  Terminal too small".into(),
            warn,
        );
        y += 2;
        render_centered(
            buf,
            inner,
            y,
            "dfctl monitor needs at least 36 × 16".to_string(),
            txt,
        );
        y += 1;
        render_centered(
            buf,
            inner,
            y,
            format!("your terminal is            {} × {}", self.w, self.h),
            dim,
        );
        y += 2;
        render_centered(
            buf,
            inner,
            y,
            "options: resize · single metric · --json".into(),
            dim,
        );
        y += 2;
        render_centered(
            buf,
            inner,
            y,
            format!(
                "docs: {}",
                crate::commands::monitor::widgets::help::DOCS_URL
            ),
            dim,
        );
        let _ = line;
    }
}
