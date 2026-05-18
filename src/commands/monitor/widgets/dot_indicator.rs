use crate::commands::monitor::render::colors::*;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

pub struct DotIndicator {
    pub count: usize,
    pub current: usize,
    pub status_color: Rgb,
}

impl Widget for DotIndicator {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < self.count as u16 * 3 || area.height == 0 {
            return;
        }
        let mut dots = Vec::new();
        for i in 0..self.count {
            if i == self.current {
                dots.push(Span::styled(
                    " ●",
                    Style::default()
                        .fg(self.status_color.to_color())
                        .add_modifier(Modifier::BOLD),
                ));
            } else {
                dots.push(Span::styled(
                    " •",
                    Style::default().fg(TEXT_DIM.to_color()),
                ));
            }
        }
        // center the row
        let total: u16 = dots.iter().map(|s| s.width() as u16).sum();
        let pad = area.width.saturating_sub(total) / 2;
        let start = Rect::new(area.x + pad, area.y, area.width.saturating_sub(pad), 1);
        Paragraph::new(Line::from(dots)).render(start, buf);

        // second row: index numbers
        if area.height > 1 {
            let mut nums = Vec::new();
            for i in 0..self.count {
                let style = if i == self.current {
                    Style::default()
                        .fg(self.status_color.to_color())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(TEXT_DIM.to_color())
                };
                nums.push(Span::styled(format!(" {}", i + 1), style));
            }
            let total: u16 = nums.iter().map(|s| s.width() as u16).sum();
            let pad = area.width.saturating_sub(total) / 2;
            let start = Rect::new(area.x + pad, area.y + 1, area.width.saturating_sub(pad), 1);
            Paragraph::new(Line::from(nums)).render(start, buf);
        }
    }
}
