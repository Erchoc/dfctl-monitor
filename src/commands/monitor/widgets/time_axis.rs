use crate::commands::monitor::render::colors::*;
use chrono::{DateTime, Duration as ChronoDuration, Local, Utc};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Span;
use ratatui::widgets::Widget;

/// Horizontal time scale under the overview panels. Confirms that `t`
/// (range picker) actually changed the window — without this, the user had
/// no on-screen evidence that the query updated.
pub struct TimeAxis {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    /// Skip this many columns on the left for the Y-axis gutter + panel border
    /// so the first time label lines up with the chart's first pixel column,
    /// not the screen edge.
    pub align_x_offset: u16,
    /// Same on the right — number of columns to reserve for the panel border
    /// (and any right padding). Defaults to 1 if you use the builder.
    pub align_x_end_pad: u16,
}

impl TimeAxis {
    /// Convenience constructor that sets sensible right padding (1 col for
    /// the panel border).
    pub fn new(from: DateTime<Utc>, to: DateTime<Utc>, align_x_offset: u16) -> Self {
        Self {
            from,
            to,
            align_x_offset,
            align_x_end_pad: 1,
        }
    }
}

impl Widget for TimeAxis {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width < self.align_x_offset + 20 {
            return;
        }
        let axis_x = area.x + self.align_x_offset;
        let axis_w = area
            .width
            .saturating_sub(self.align_x_offset)
            .saturating_sub(self.align_x_end_pad);

        // Pick label count that fits without crowding. Each label is 5 chars
        // (HH:MM) and reads best with ~15 cols of breathing room between
        // ticks. Otherwise the eye can't tell which tick lines up with which
        // chart spike (user feedback: "底部时间刻度在小屏不需要那么多").
        let n: usize = if axis_w < 45 {
            3
        } else if axis_w < 130 {
            5
        } else {
            7
        };
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
            // Anchor each label by position:
            //   first  (i == 0)     → flush left at axis_x
            //   last   (i == n-1)   → flush right at axis_x + axis_w - label_w
            //   middle              → centred on the tick position
            // This is what users expect from a Grafana-style chart scale, and
            // matches the user feedback "左右两个点应该位于两侧".
            let x = if i == 0 {
                axis_x
            } else if i == n - 1 {
                axis_x + axis_w.saturating_sub(label_w)
            } else {
                let center = (axis_w as f64 * frac) as u16;
                let shift = label_w / 2;
                let mut x = axis_x + center;
                if x >= shift {
                    x = x.saturating_sub(shift);
                }
                if x + label_w > axis_x + axis_w {
                    x = axis_x + axis_w - label_w;
                }
                x
            };
            let span = Span::styled(
                label,
                Style::default().fg(TEXT_SECONDARY.to_color()),
            );
            buf.set_span(x, area.y, &span, label_w);
        }
    }
}
