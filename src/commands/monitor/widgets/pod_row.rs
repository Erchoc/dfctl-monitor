use crate::commands::monitor::data::{PodInfo, Series};
use crate::commands::monitor::render::colors::*;
use crate::commands::monitor::widgets::sparkline::sparkline;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

/// Single-line pod summary for compact layouts (phone single-metric view).
///
/// When the metric has a per-pod series, this renders:
///     ◉ pod-a   42ms  avg 39ms  p99 51ms  ▁▂▃▂▄▆
///
/// When the metric is aggregate-only (e.g. QPS, Latency on most platforms),
/// it falls back to the PodInfo fields the user actually cares about — never
/// "0.0 / 0.0 / 0.0" empty placeholders. User feedback was:
/// "pod 信息都是空的, 除非这台机器明显高于别人否则不如不要".
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

        // ── header chip: ◉ pod-name [STATUS]
        let mut spans: Vec<Span<'_>> = vec![
            Span::styled("◉ ", Style::default().fg(color.to_color())),
            Span::styled(
                format!("{:<6}", &self.pod.name),
                Style::default()
                    .fg(TEXT_PRIMARY.to_color())
                    .add_modifier(Modifier::BOLD),
            ),
        ];

        match self.series {
            Some(s) => {
                // ── per-pod metric available: current + avg + p99 + sparkline ──
                let stats = s.stats();
                spans.extend([
                    Span::styled(
                        format!("{:>7}", format_value(s.current(), self.unit)),
                        Style::default()
                            .fg(color.to_color())
                            .add_modifier(Modifier::BOLD),
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
                let line = Line::from(spans);
                let prefix_len = line.width() as u16;
                Paragraph::new(line).render(
                    Rect::new(area.x, area.y, prefix_len.min(area.width), 1),
                    buf,
                );
                // sparkline fills the rest of the line
                let sx = area.x + prefix_len.min(area.width);
                let sw = area.width.saturating_sub(prefix_len);
                if sw > 4 {
                    let vals: Vec<f64> = s.points.iter().map(|p| p.1).collect();
                    sparkline(buf, Rect::new(sx, area.y, sw, 1), &vals, color);
                }
            }
            None => {
                // ── no per-pod series: show PodInfo fields instead of 0.0/0.0/0.0 ──
                let cpu_str = format!("{:.1}%", self.pod.cpu_pct);
                let mem_gb = self.pod.mem_bytes as f64 / 1024.0 / 1024.0 / 1024.0;
                let status_color = if self.pod.status == "Running" {
                    ACCENT_OK
                } else {
                    ACCENT_ALERT
                };
                spans.extend([
                    Span::styled(
                        format!(
                            "{:<10} ",
                            if self.pod.last_restart_at.is_some() {
                                "RESTART"
                            } else {
                                &self.pod.status
                            }
                        ),
                        Style::default()
                            .fg(if self.pod.last_restart_at.is_some() {
                                ACCENT_WARN.to_color()
                            } else {
                                status_color.to_color()
                            })
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("cpu {:<7}", cpu_str),
                        Style::default().fg(TEXT_SECONDARY.to_color()),
                    ),
                    Span::styled(
                        format!("mem {:<7}", format!("{:.1}G", mem_gb)),
                        Style::default().fg(TEXT_SECONDARY.to_color()),
                    ),
                    Span::styled(
                        if let Some(at) = self.pod.last_restart_at {
                            format!(
                                "» restart {} ago",
                                crate::commands::monitor::util::ago(at)
                            )
                        } else {
                            format!(
                                "· up {}",
                                crate::commands::monitor::util::format_duration_short(
                                    self.pod.uptime_seconds
                                )
                            )
                        },
                        Style::default().fg(if self.pod.last_restart_at.is_some() {
                            ACCENT_WARN.to_color()
                        } else {
                            TEXT_DIM.to_color()
                        }),
                    ),
                ]);
                Paragraph::new(Line::from(spans)).render(
                    Rect::new(area.x, area.y, area.width, 1),
                    buf,
                );
                // No sparkline when there's no per-pod data — explicitly avoids
                // the empty-data sparkline artifact (renders as a row of `▁`s)
                // the user noticed in phone screenshots.
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
