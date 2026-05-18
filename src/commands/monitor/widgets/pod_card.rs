use crate::commands::monitor::data::{PodInfo, Series};
use crate::commands::monitor::render::colors::*;
use crate::commands::monitor::widgets::sparkline::sparkline;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Widget};

pub struct PodCard<'a> {
    pub pod: &'a PodInfo,
    pub series: Option<&'a Series>,
    pub unit: &'a str,
}

impl<'a> Widget for PodCard<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let color = pod_color(&self.pod.name);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .style(Style::default().fg(BORDER_GRID.to_color()));
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.height < 4 {
            return;
        }
        // header: ◉ pod-name   <value>  ↑x%
        let value = self
            .series
            .map(|s| s.current())
            .unwrap_or(0.0);
        let avg = self
            .series
            .map(|s| s.stats().avg)
            .unwrap_or(0.0);
        let stats = self.series.map(|s| s.stats()).unwrap_or_default();
        let arrow = if value > avg + 0.5 {
            "↑"
        } else if value < avg - 0.5 {
            "↓"
        } else {
            "·"
        };
        let value_str = format_value(value, self.unit);
        let header = Line::from(vec![
            Span::styled("◉ ", Style::default().fg(color.to_color())),
            Span::styled(
                format!("{:<10}", truncate(&self.pod.name, 10)),
                Style::default().fg(TEXT_PRIMARY.to_color()).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:>7} ", value_str),
                Style::default().fg(color.to_color()).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                arrow,
                Style::default().fg(if arrow == "↑" {
                    ACCENT_WARN.to_color()
                } else if arrow == "↓" {
                    ACCENT_OK.to_color()
                } else {
                    TEXT_DIM.to_color()
                }),
            ),
        ]);
        Paragraph::new(header).render(
            Rect::new(inner.x, inner.y, inner.width, 1),
            buf,
        );

        // sparkline
        if let Some(s) = self.series {
            let vals: Vec<f64> = s.points.iter().map(|p| p.1).collect();
            sparkline(
                buf,
                Rect::new(inner.x, inner.y + 1, inner.width, 1),
                &vals,
                color,
            );
        }

        // stats block: 2 columns
        let label_style = Style::default().fg(TEXT_DIM.to_color());
        let val_style = Style::default().fg(TEXT_PRIMARY.to_color());
        let line2 = Line::from(vec![
            Span::styled(" avg  ", label_style),
            Span::styled(format!("{:<6}", format_value(stats.avg, self.unit)), val_style),
            Span::styled(" p50  ", label_style),
            Span::styled(format_value(stats.p50, self.unit), val_style),
        ]);
        let line3 = Line::from(vec![
            Span::styled(" min  ", label_style),
            Span::styled(format!("{:<6}", format_value(stats.min, self.unit)), val_style),
            Span::styled(" p95  ", label_style),
            Span::styled(format_value(stats.p95, self.unit), val_style),
        ]);
        let line4 = Line::from(vec![
            Span::styled(" max  ", label_style),
            Span::styled(format!("{:<6}", format_value(stats.max, self.unit)), val_style),
            Span::styled(" p99  ", label_style),
            Span::styled(format_value(stats.p99, self.unit), val_style),
        ]);
        if inner.height > 3 {
            Paragraph::new(line2).render(Rect::new(inner.x, inner.y + 2, inner.width, 1), buf);
        }
        if inner.height > 4 {
            Paragraph::new(line3).render(Rect::new(inner.x, inner.y + 3, inner.width, 1), buf);
        }
        if inner.height > 5 {
            Paragraph::new(line4).render(Rect::new(inner.x, inner.y + 4, inner.width, 1), buf);
        }
        // last restart hint
        if inner.height > 6 {
            let txt = if let Some(at) = self.pod.last_restart_at {
                format!(" restart {} ago", crate::commands::monitor::util::ago(at))
            } else {
                format!(" uptime {}", crate::commands::monitor::util::format_duration_short(self.pod.uptime_seconds))
            };
            Paragraph::new(Line::from(Span::styled(
                txt,
                Style::default().fg(TEXT_SECONDARY.to_color()),
            )))
            .render(Rect::new(inner.x, inner.y + 5, inner.width, 1), buf);
        }
    }
}

fn format_value(v: f64, unit: &str) -> String {
    match unit {
        "%" => format!("{:.1}%", v),
        "ms" => format!("{:.0}ms", v),
        "GB" => format!("{:.1}G", v),
        _ => format!("{:.1}", v),
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n - 1).collect::<String>() + "…"
    }
}
