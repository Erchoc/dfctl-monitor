use crate::commands::monitor::data::*;
use crate::commands::monitor::render::colors::*;
use crate::commands::monitor::state::AggMode;
use crate::commands::monitor::theme::{assess_health, series_color, HealthStatus};
use crate::commands::monitor::widgets::chart::{format_traffic, AreaChart, AreaSeries};
use crate::commands::monitor::widgets::replicas::ReplicasTable;
use crate::commands::monitor::widgets::stacked_bar::{StackedBar, StackedSeries, TrafficDisplay};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Widget};

pub struct MetricPanel<'a> {
    pub metric: MetricKind,
    pub data: &'a MetricData,
    pub pods: &'a [PodInfo],
    pub focused: bool,
    pub agg_mode: AggMode,
    pub traffic_display: TrafficDisplay,
}

impl<'a> Widget for MetricPanel<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 10 || area.height < 3 {
            return;
        }
        let status = assess_health(self.metric, &self.data.series);
        let (border_color, border_type) = if self.focused {
            (ACCENT_OK, BorderType::Double)
        } else {
            (status.border_color(), BorderType::Rounded)
        };

        // ── Title ──
        let title_spans = build_title(self.metric, &self.data, status, self.focused, self.agg_mode);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(border_type)
            .title(Line::from(title_spans))
            .style(Style::default().fg(border_color.to_color()));
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 3 {
            return;
        }

        // subtitle
        let subtitle = build_subtitle(self.metric, &self.data);
        if !subtitle.is_empty() && inner.height >= 4 {
            Paragraph::new(Line::from(Span::styled(
                subtitle,
                Style::default().fg(TEXT_SECONDARY.to_color()),
            )))
            .render(Rect::new(inner.x, inner.y, inner.width, 1), buf);
        }

        let body_y = if inner.height >= 4 { inner.y + 1 } else { inner.y };
        let body_h = inner.y + inner.height - body_y;
        let body = Rect::new(inner.x, body_y, inner.width, body_h);

        match self.metric {
            MetricKind::Qps => render_stacked(self.data, body, buf, self.traffic_display),
            MetricKind::Replicas => {
                ReplicasTable { pods: self.pods }.render(body, buf);
            }
            _ => render_area(self.metric, self.data, body, buf, false, self.agg_mode),
        }
    }
}

fn build_title(
    metric: MetricKind,
    data: &MetricData,
    status: HealthStatus,
    _focused: bool,
    agg_mode: AggMode,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    spans.push(Span::styled(
        "◆ ",
        Style::default().fg(status.color().to_color()),
    ));
    let title = metric.title().to_string();
    let title_with_unit = match metric {
        MetricKind::Qps => format!("{} (1m avg)", title),
        _ => title,
    };
    spans.push(Span::styled(
        title_with_unit,
        Style::default()
            .fg(TEXT_PRIMARY.to_color())
            .add_modifier(Modifier::BOLD),
    ));
    if let Some(label) = agg_mode.label() {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            format!("({})", label),
            Style::default().fg(ACCENT_INFO.to_color()),
        ));
    }
    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        status.badge().to_string(),
        Style::default()
            .fg(status.color().to_color())
            .add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::raw("  "));
    let preview = build_preview(metric, data);
    if !preview.is_empty() {
        spans.push(Span::styled(
            preview,
            Style::default().fg(TEXT_SECONDARY.to_color()),
        ));
    }
    spans.push(Span::raw(" "));
    spans
}

fn build_preview(metric: MetricKind, data: &MetricData) -> String {
    match metric {
        MetricKind::Qps => {
            let total_2xx: f64 = data
                .series
                .iter()
                .find(|s| matches!(s.kind, SeriesKind::StatusCode(c) if (200..300).contains(&c)))
                .map(|s| s.current())
                .unwrap_or(0.0);
            format_traffic(total_2xx)
        }
        MetricKind::Latency => {
            let p99 = data
                .series
                .iter()
                .find(|s| matches!(s.kind, SeriesKind::Percentile(99)))
                .map(|s| s.current())
                .unwrap_or(0.0);
            format!("P99 {:.0}ms", p99)
        }
        MetricKind::ErrorRate => {
            data.series
                .first()
                .map(|s| format!("{:.2}%", s.current()))
                .unwrap_or_default()
        }
        MetricKind::Upstream => {
            let max_now = data.series.iter().map(|s| s.current()).fold(0.0_f64, f64::max);
            format!("max {:.0}ms", max_now)
        }
        MetricKind::Cpu => {
            let max_now = data
                .series
                .iter()
                .find(|s| s.label == "max")
                .map(|s| s.current())
                .unwrap_or(0.0);
            let avg_now = data
                .series
                .iter()
                .find(|s| s.label == "avg")
                .map(|s| s.current())
                .unwrap_or(0.0);
            format!("max {:.0}% · avg {:.0}%", max_now, avg_now)
        }
        MetricKind::Memory => {
            let max_now = data
                .series
                .iter()
                .find(|s| s.label == "max")
                .map(|s| s.current())
                .unwrap_or(0.0);
            format!("max {:.1}G", max_now)
        }
        MetricKind::Replicas => String::new(),
        MetricKind::Runtime => {
            let gc = data.series.iter().find(|s| s.label.contains("GC")).map(|s| s.current()).unwrap_or(0.0);
            format!("GC {:.0}ms", gc)
        }
    }
}

fn build_subtitle(metric: MetricKind, data: &MetricData) -> String {
    match metric {
        MetricKind::Cpu | MetricKind::Memory => {
            let max_now = data.series.iter().find(|s| s.label == "max").map(|s| s.current()).unwrap_or(0.0);
            let avg_now = data.series.iter().find(|s| s.label == "avg").map(|s| s.current()).unwrap_or(0.0);
            let pods: Vec<String> = data.series
                .iter()
                .filter(|s| matches!(s.kind, SeriesKind::Pod(_)))
                .map(|s| format!("{} {}", s.label, format_short_value(s.current(), &data.unit)))
                .collect();
            // 20% spread is the on-call rule of thumb — if one pod is >20pp above the
            // mean, the aggregate hides interesting per-pod behaviour.
            let uneven = (max_now - avg_now).abs() > 20.0;
            let warn = if uneven { "  ⚠ uneven" } else { "" };
            format!("{}{}", pods.join("  "), warn)
        }
        MetricKind::Latency => {
            let p50 = data.series.iter().find(|s| matches!(s.kind, SeriesKind::Percentile(50))).map(|s| s.current()).unwrap_or(0.0);
            let p95 = data.series.iter().find(|s| matches!(s.kind, SeriesKind::Percentile(95))).map(|s| s.current()).unwrap_or(0.0);
            let p99 = data.series.iter().find(|s| matches!(s.kind, SeriesKind::Percentile(99))).map(|s| s.current()).unwrap_or(0.0);
            format!("P50 {:.0}ms  P95 {:.0}ms  P99 {:.0}ms", p50, p95, p99)
        }
        MetricKind::Upstream => {
            data.series.iter()
                .map(|s| format!("{} {:.0}ms", s.label, s.current()))
                .collect::<Vec<_>>()
                .join("  ")
        }
        MetricKind::Qps => {
            let cur_2xx = data.series.iter().find(|s| matches!(s.kind, SeriesKind::StatusCode(c) if (200..300).contains(&c))).map(|s| s.current()).unwrap_or(0.0);
            let cur_4xx = data.series.iter().find(|s| matches!(s.kind, SeriesKind::StatusCode(c) if (400..500).contains(&c))).map(|s| s.current()).unwrap_or(0.0);
            let cur_5xx = data.series.iter().find(|s| matches!(s.kind, SeriesKind::StatusCode(c) if c >= 500)).map(|s| s.current()).unwrap_or(0.0);
            format!("2xx {}  4xx {}  5xx {}", format_traffic(cur_2xx), format_traffic(cur_4xx), format_traffic(cur_5xx))
        }
        MetricKind::ErrorRate => {
            data.series.first().map(|s| {
                let stats = s.stats();
                format!("now {:.2}%  peak {:.2}%  avg {:.2}%", s.current(), stats.max, stats.avg)
            }).unwrap_or_default()
        }
        MetricKind::Runtime => {
            data.series.iter()
                .map(|s| format!("{} {:.1}", s.label, s.current()))
                .collect::<Vec<_>>()
                .join("  ")
        }
        MetricKind::Replicas => String::new(),
    }
}

fn format_short_value(v: f64, unit: &str) -> String {
    match unit {
        "%" => format!("{:.0}%", v),
        "ms" => format!("{:.0}ms", v),
        "GB" => format!("{:.1}G", v),
        _ => format!("{:.1}", v),
    }
}

fn render_stacked(data: &MetricData, area: Rect, buf: &mut Buffer, display: TrafficDisplay) {
    let series: Vec<StackedSeries> = data
        .series
        .iter()
        .map(|s| StackedSeries {
            label: s.label.clone(),
            color: match s.kind {
                SeriesKind::StatusCode(c) => status_color(c),
                _ => SERIES_GREEN,
            },
            points: s.points.iter().map(|p| p.1).collect(),
        })
        .collect();
    StackedBar {
        series,
        unit: &data.unit,
        display,
    }
    .render(area, buf);
}

fn render_area(
    metric: MetricKind,
    data: &MetricData,
    area: Rect,
    buf: &mut Buffer,
    with_cursor: bool,
    agg_mode: AggMode,
) {
    let mut chart = AreaChart::new(&data.unit).cursor(with_cursor);

    // Pick which series to display based on aggregation mode for CPU/Memory-style
    // multi-series metrics that carry both aggregate lines and per-pod lines.
    let is_cpu_mem = matches!(metric, MetricKind::Cpu | MetricKind::Memory);

    let pick = |s: &Series| -> Option<(Rgb, bool, bool)> {
        let color = series_color(metric, s);
        if is_cpu_mem {
            match agg_mode {
                AggMode::Default => {
                    if s.label == "max" {
                        Some((color, false, true))
                    } else if s.label == "avg" {
                        Some((color, true, true))
                    } else {
                        None
                    }
                }
                AggMode::Max => (s.label == "max").then_some((color, false, true)),
                AggMode::Avg => (s.label == "avg").then_some((color, false, true)),
                AggMode::Sum | AggMode::P95 => {
                    // sum/p95 not meaningful for cpu/mem percentages, fall back to max
                    (s.label == "max").then_some((color, false, true))
                }
                AggMode::PerPod => match s.kind {
                    SeriesKind::Pod(_) => Some((color, false, true)),
                    _ => None,
                },
            }
        } else {
            // For other metrics: render everything by default; per-pod mode skips aggregates
            match agg_mode {
                AggMode::PerPod => match s.kind {
                    SeriesKind::Pod(_) => Some((color, false, true)),
                    _ => None,
                },
                _ => Some((color, false, true)),
            }
        }
    };

    for s in &data.series {
        if let Some((color, dim, fill)) = pick(s) {
            chart = chart.add(AreaSeries {
                points: s.points.iter().map(|p| p.1).collect(),
                color,
                dim,
                fill,
            });
        }
    }
    chart.render(area, buf);
}
