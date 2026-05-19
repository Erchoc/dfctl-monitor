use crate::commands::monitor::data::{PodInfo, Series};
use crate::commands::monitor::render::colors::*;
use crate::commands::monitor::widgets::sparkline::sparkline;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Widget};

/// Per-pod card.
///
/// The card always shows *something useful* — even when the metric has no
/// per-pod series (e.g. Latency is usually aggregated across replicas), we
/// fall back to PodInfo health signals (uptime, restart count, CPU, MEM) so
/// the card is never "empty 0ms". User feedback: "pod 信息都是空的" + 体验不好。
pub struct PodCard<'a> {
    pub pod: &'a PodInfo,
    /// Per-pod metric series; `None` means the metric is aggregate-only.
    pub series: Option<&'a Series>,
    pub unit: &'a str,
    /// Why this pod is in the sidebar. Surfaces as a small badge on the card.
    pub reason: PodCardReason,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PodCardReason {
    /// Outlier vs peers (above + N σ).
    OutlierHigh,
    /// Outlier vs peers (below − N σ).
    OutlierLow,
    /// Pod restarted recently or is in a non-Running state.
    Unhealthy,
    /// Listed because user explicitly asked for full pod detail (small cluster).
    Routine,
}

impl PodCardReason {
    fn badge(&self) -> Option<(&'static str, Rgb)> {
        match self {
            PodCardReason::OutlierHigh => Some(("HIGH", ACCENT_WARN)),
            PodCardReason::OutlierLow => Some(("LOW", ACCENT_INFO)),
            PodCardReason::Unhealthy => Some(("RESTART", ACCENT_ALERT)),
            PodCardReason::Routine => None,
        }
    }
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
        if inner.height < 3 {
            return;
        }

        // ── header: ◉ pod-name   [BADGE]   metric value + arrow ──
        let has_metric = self.series.is_some();
        let value = self.series.map(|s| s.current()).unwrap_or(0.0);
        let stats = self.series.map(|s| s.stats()).unwrap_or_default();
        let arrow = if has_metric {
            if value > stats.avg + 0.5 {
                "↑"
            } else if value < stats.avg - 0.5 {
                "↓"
            } else {
                "·"
            }
        } else {
            ""
        };
        let value_str = if has_metric {
            format_value(value, self.unit)
        } else {
            String::new()
        };

        let mut header_spans = vec![
            Span::styled("◉ ", Style::default().fg(color.to_color())),
            Span::styled(
                truncate(&self.pod.name, 12),
                Style::default()
                    .fg(TEXT_PRIMARY.to_color())
                    .add_modifier(Modifier::BOLD),
            ),
        ];
        if let Some((badge_txt, badge_col)) = self.reason.badge() {
            header_spans.push(Span::raw(" "));
            header_spans.push(Span::styled(
                format!(" {} ", badge_txt),
                Style::default()
                    .fg(badge_col.to_color())
                    .add_modifier(Modifier::BOLD),
            ));
        }
        if has_metric {
            header_spans.push(Span::raw(" "));
            header_spans.push(Span::styled(
                value_str,
                Style::default()
                    .fg(color.to_color())
                    .add_modifier(Modifier::BOLD),
            ));
            header_spans.push(Span::raw(" "));
            header_spans.push(Span::styled(
                arrow.to_string(),
                Style::default().fg(match arrow {
                    "↑" => ACCENT_WARN.to_color(),
                    "↓" => ACCENT_OK.to_color(),
                    _ => TEXT_DIM.to_color(),
                }),
            ));
        }
        Paragraph::new(Line::from(header_spans)).render(
            Rect::new(inner.x, inner.y, inner.width, 1),
            buf,
        );

        // ── sparkline (only when per-pod series exists) ──
        let mut next_y = inner.y + 1;
        if let Some(s) = self.series {
            let vals: Vec<f64> = s.points.iter().map(|p| p.1).collect();
            if inner.height > 2 {
                sparkline(
                    buf,
                    Rect::new(inner.x, next_y, inner.width, 1),
                    &vals,
                    color,
                );
                next_y += 1;
            }
        }

        // ── stats body ──
        let label_style = Style::default().fg(TEXT_DIM.to_color());
        let val_style = Style::default().fg(TEXT_PRIMARY.to_color());

        if has_metric {
            // metric stats: avg/min/max + p50/p95/p99 (two columns)
            let rows: [Line; 3] = [
                Line::from(vec![
                    Span::styled(" avg ", label_style),
                    Span::styled(format!("{:<7}", format_value(stats.avg, self.unit)), val_style),
                    Span::styled(" p50 ", label_style),
                    Span::styled(format_value(stats.p50, self.unit), val_style),
                ]),
                Line::from(vec![
                    Span::styled(" min ", label_style),
                    Span::styled(format!("{:<7}", format_value(stats.min, self.unit)), val_style),
                    Span::styled(" p95 ", label_style),
                    Span::styled(format_value(stats.p95, self.unit), val_style),
                ]),
                Line::from(vec![
                    Span::styled(" max ", label_style),
                    Span::styled(format!("{:<7}", format_value(stats.max, self.unit)), val_style),
                    Span::styled(" p99 ", label_style),
                    Span::styled(format_value(stats.p99, self.unit), val_style),
                ]),
            ];
            for line in rows.iter() {
                if next_y >= inner.y + inner.height {
                    break;
                }
                Paragraph::new(line.clone()).render(Rect::new(inner.x, next_y, inner.width, 1), buf);
                next_y += 1;
            }
        } else {
            // PodInfo-only view: surface CPU / Memory / status from the PodInfo
            // header itself. Never show "0ms"-style empty data.
            let cpu_str = format!("{:.1}%", self.pod.cpu_pct);
            let mem_gb = self.pod.mem_bytes as f64 / 1024.0 / 1024.0 / 1024.0;
            let rows = [
                Line::from(vec![
                    Span::styled(" cpu ", label_style),
                    Span::styled(format!("{:<7}", cpu_str), val_style),
                    Span::styled(" mem ", label_style),
                    Span::styled(format!("{:.1}G", mem_gb), val_style),
                ]),
                Line::from(vec![
                    Span::styled(" status ", label_style),
                    Span::styled(
                        self.pod.status.clone(),
                        Style::default().fg(if self.pod.status == "Running" {
                            ACCENT_OK.to_color()
                        } else {
                            ACCENT_ALERT.to_color()
                        }),
                    ),
                ]),
            ];
            for line in rows.iter() {
                if next_y >= inner.y + inner.height {
                    break;
                }
                Paragraph::new(line.clone()).render(Rect::new(inner.x, next_y, inner.width, 1), buf);
                next_y += 1;
            }
        }

        // ── footer line: uptime / restart info (always shown) ──
        // Use Braille / standard punctuation only — ↻ and ✓ render as口字框 on
        // terminals without Nerd Font glyph fallbacks.
        if next_y < inner.y + inner.height {
            let txt = if let Some(at) = self.pod.last_restart_at {
                format!(" » restart {} ago", crate::commands::monitor::util::ago(at))
            } else {
                format!(
                    " · uptime {}",
                    crate::commands::monitor::util::format_duration_short(self.pod.uptime_seconds)
                )
            };
            let color = if self.pod.last_restart_at.is_some() {
                ACCENT_WARN
            } else {
                TEXT_SECONDARY
            };
            Paragraph::new(Line::from(Span::styled(
                txt,
                Style::default().fg(color.to_color()),
            )))
            .render(Rect::new(inner.x, next_y, inner.width, 1), buf);
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
