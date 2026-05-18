use crate::commands::monitor::data::MonitorResponse;
use crate::commands::monitor::render::colors::*;
use crate::commands::monitor::state::AppState;
use chrono::Local;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

pub struct Header<'a> {
    pub state: &'a AppState,
    pub data: Option<&'a MonitorResponse>,
    /// Trim header to `app · env · time` only (spec § 3.4) — for phone-sized
    /// terminals where the full header would overflow.
    pub compact: bool,
}

impl<'a> Widget for Header<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 {
            return;
        }
        let now = Local::now();
        let app = self.data.map(|d| d.app.as_str()).unwrap_or(&self.state.args.app);
        let region = self.data.map(|d| d.region.as_str()).unwrap_or("—");
        let env = self.data.map(|d| d.env.as_str()).unwrap_or("—");

        // On phone-width terminals (< 60 cols) we drop the brand prefix entirely so the
        // app/env can fit. Right-aligned LIVE/PAUSED + time still fits in ~12 chars.
        let tiny = self.compact && area.width < 60;
        let mut left = Vec::new();
        if !tiny {
            left.push(Span::styled(
                "dfctl ",
                Style::default().fg(ACCENT_OK.to_color()).add_modifier(Modifier::BOLD),
            ));
        }
        left.push(Span::styled(
            app,
            Style::default().fg(TEXT_PRIMARY.to_color()).add_modifier(Modifier::BOLD),
        ));
        if !tiny {
            left.push(Span::styled(
                format!(" · {}", env),
                Style::default().fg(TEXT_SECONDARY.to_color()),
            ));
        }
        if !self.compact {
            left.push(Span::styled(
                format!(" · {}", region),
                Style::default().fg(TEXT_SECONDARY.to_color()),
            ));
        }

        if let Some(d) = self.data {
            if !self.compact {
                // Show "from → to · duration" so the user can see exactly which
                // time window they're looking at and confirm `t` (range picker)
                // actually changed the query.
                let from = d.time_range.from.with_timezone(&Local);
                let to = d.time_range.to.with_timezone(&Local);
                let dur = to - from;
                let dur_h = dur.num_hours();
                let dur_m = dur.num_minutes() % 60;
                let dur_str = if dur_h > 0 {
                    format!("{}h{:02}m", dur_h, dur_m)
                } else {
                    format!("{}m", dur.num_minutes())
                };
                let range_str = format!(
                    " · {} → {} ({})",
                    from.format("%H:%M"),
                    to.format("%H:%M"),
                    dur_str,
                );
                left.push(Span::styled(
                    range_str,
                    Style::default().fg(TEXT_SECONDARY.to_color()),
                ));
            }
            if !self.compact {
                let pods_count = d.pods.len();
                left.push(Span::styled(
                    format!(" · {} pods", pods_count),
                    Style::default().fg(TEXT_SECONDARY.to_color()),
                ));
            }
        }

        let right = {
            let mut spans = Vec::new();
            if self.state.watch_enabled {
                if self.state.watch_paused {
                    spans.push(Span::styled(
                        "◉ PAUSED ",
                        Style::default().fg(ACCENT_ALERT.to_color()).add_modifier(Modifier::BOLD),
                    ));
                } else {
                    // Breathing LIVE indicator — the dot alternates ●/○ every second
                    // so the user can see the binary is alive even when mock data
                    // hasn't changed yet (real data refreshes every interval).
                    let pulse = (now.timestamp() % 2) == 0;
                    let dot = if pulse { "●" } else { "○" };
                    spans.push(Span::styled(
                        format!("{} LIVE ", dot),
                        Style::default()
                            .fg(ACCENT_OK.to_color())
                            .add_modifier(Modifier::BOLD),
                    ));
                    if let Some(secs) = self.state.countdown_seconds() {
                        spans.push(Span::styled(
                            format!("⟳ {}s  ", secs),
                            Style::default().fg(ACCENT_WARN.to_color()),
                        ));
                    }
                }
            } else {
                spans.push(Span::styled(
                    "◌ paused  ",
                    Style::default().fg(TEXT_DIM.to_color()),
                ));
            }
            spans.push(Span::styled(
                now.format("%H:%M:%S").to_string(),
                Style::default()
                    .fg(TEXT_PRIMARY.to_color())
                    .add_modifier(Modifier::BOLD),
            ));
            spans
        };

        // Left aligned
        Paragraph::new(Line::from(left)).render(area, buf);
        // Right aligned
        let right_line = Line::from(right);
        let right_width: u16 = right_line.width().min(area.width as usize) as u16;
        let right_area = Rect::new(
            area.x + area.width.saturating_sub(right_width),
            area.y,
            right_width,
            1,
        );
        Paragraph::new(right_line).render(right_area, buf);
    }
}
