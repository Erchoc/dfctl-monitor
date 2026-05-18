use crate::commands::monitor::data::{PodInfo, Series};
use crate::commands::monitor::render::colors::*;
use crate::commands::monitor::widgets::sparkline::sparkline;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

/// Single-line pod summary for compact layouts (phone single-metric).
pub struct PodRow<'a> {
    pub pod: &'a PodInfo,
    pub series: Option<&'a Series>,
    pub unit: &'a str,
}

impl<'a> Widget for PodRow<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width < 20 {
            return;
        }
        let color = pod_color(&self.pod.name);
        let current = self.series.map(|s| s.current()).unwrap_or(0.0);
        let stats = self.series.map(|s| s.stats()).unwrap_or_default();

        // layout: ◉ name  current  avg  sparkline
        let header = Line::from(vec![
            Span::styled("◉ ", Style::default().fg(color.to_color())),
            Span::styled(
                format!("{:<6}", &self.pod.name),
                Style::default().fg(TEXT_PRIMARY.to_color()).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:>7}", format_value(current, self.unit)),
                Style::default().fg(color.to_color()).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  avg {:<6}", format_value(stats.avg, self.unit)),
                Style::default().fg(TEXT_SECONDARY.to_color()),
            ),
            Span::styled(
                format!("p99 {:<6}", format_value(stats.p99, self.unit)),
                Style::default().fg(TEXT_SECONDARY.to_color()),
            ),
        ]);
        let prefix_len = header.width() as u16;
        Paragraph::new(header).render(
            Rect::new(area.x, area.y, prefix_len.min(area.width), 1),
            buf,
        );
        // sparkline fills the rest of the line
        if let Some(s) = self.series {
            let sx = area.x + prefix_len.min(area.width);
            let sw = area.width.saturating_sub(prefix_len);
            if sw > 4 {
                let vals: Vec<f64> = s.points.iter().map(|p| p.1).collect();
                sparkline(buf, Rect::new(sx, area.y, sw, 1), &vals, color);
            }
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
