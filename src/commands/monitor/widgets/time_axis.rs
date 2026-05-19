use crate::commands::monitor::render::colors::*;
use chrono::{DateTime, Duration as ChronoDuration, Local, Utc};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Span;
use ratatui::widgets::Widget;

/// Horizontal time scale (7 evenly-spaced labels) under the overview panels.
/// Confirms that `t` (range picker) actually changed the window — without
/// this, the user had no on-screen evidence that the query updated.
pub struct TimeAxis {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub align_x_offset: u16, // skip the gutter where Y-axis labels live
}

impl Widget for TimeAxis {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width < self.align_x_offset + 20 {
            return;
        }
        let axis_x = area.x + self.align_x_offset;
        let axis_w = area.width.saturating_sub(self.align_x_offset);
        let n = 7_usize;
        let span_secs = (self.to - self.from).num_seconds().max(60) as f64;
        let use_full_date = span_secs >= 86_400.0;

        for i in 0..n {
            let frac = i as f64 / (n - 1) as f64;
            let t = self.from + ChronoDuration::seconds((span_secs * frac) as i64);
            let local = t.with_timezone(&Local);
            let label = if use_full_date {
                local.format("%m-%d %H:%M").to_string()
            } else {
                local.format("%H:%M").to_string()
            };
            let label_w = label.chars().count() as u16;
            let center = (axis_w as f64 * frac) as u16;
            let mut x = axis_x + center;
            let shift = label_w / 2;
            if x >= axis_x + shift {
                x -= shift;
            }
            if x + label_w > axis_x + axis_w {
                x = axis_x + axis_w.saturating_sub(label_w);
            }
            let span = Span::styled(
                label,
                Style::default().fg(TEXT_SECONDARY.to_color()),
            );
            buf.set_span(x, area.y, &span, label_w);
        }
    }
}
