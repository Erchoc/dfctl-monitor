use super::colors::*;
use crate::commands::monitor::data::*;
use crate::commands::monitor::state::{AppState, TrafficUnit};
use crate::commands::monitor::widgets;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

pub fn paint_bg(buf: &mut Buffer, area: Rect) {
    for y in area.y..area.y + area.height {
        for x in area.x..area.x + area.width {
            if let Some(c) = buf.cell_mut((x, y)) {
                c.set_bg(BG.to_color());
            }
        }
    }
}

pub fn render_loading(area: Rect, buf: &mut Buffer) {
    if area.width < 24 || area.height == 0 {
        return;
    }
    let inner = Rect::new(
        area.x + area.width / 2 - 12,
        area.y + area.height / 2,
        24,
        1,
    );
    Paragraph::new(Line::from(Span::styled(
        "⠋ loading metrics...",
        Style::default().fg(ACCENT_OK.to_color()),
    )))
    .render(inner, buf);
}

pub fn render_events(events: &[Event], area: Rect, buf: &mut Buffer) {
    if events.is_empty() {
        return;
    }
    let mut spans = Vec::new();
    for (i, e) in events.iter().take(3).enumerate() {
        if i > 0 {
            spans.push(Span::styled("    ", Style::default()));
        }
        let glyph_color = match e.kind {
            EventKind::AlertFired => ACCENT_ALERT,
            EventKind::AlertResolved => ACCENT_OK,
            EventKind::Restart => ACCENT_WARN,
            EventKind::Deploy => ACCENT_INFO,
            EventKind::ScaleEvent => ACCENT_SECONDARY,
        };
        let local = e.at.with_timezone(&chrono::Local);
        spans.push(Span::styled(
            format!("{} ", e.kind.glyph()),
            Style::default().fg(glyph_color.to_color()),
        ));
        spans.push(Span::styled(
            format!("{}  ", local.format("%H:%M:%S")),
            Style::default().fg(TEXT_SECONDARY.to_color()),
        ));
        spans.push(Span::styled(
            e.message.clone(),
            Style::default().fg(TEXT_PRIMARY.to_color()),
        ));
    }
    Paragraph::new(Line::from(spans)).render(Rect::new(area.x, area.y, area.width, 1), buf);
}

pub fn pick_card_color(metric: MetricKind, value: f64) -> Rgb {
    // KPI card thresholds must match the panel-border thresholds in
    // `theme::assess_health` so the cards and the badge agree. If you change
    // a value here, change it there too — or better, factor the shared band
    // table out into a single source of truth.
    match metric {
        MetricKind::ErrorRate => {
            if value > 5.0 { ACCENT_ALERT } else if value > 1.0 { ACCENT_WARN } else { ACCENT_OK }
        }
        MetricKind::Latency => {
            // P95-based gates (120 ms WARN, 200 ms ALERT). Was 150/250 (P99-era).
            if value > 200.0 { ACCENT_ALERT } else if value > 120.0 { ACCENT_WARN } else { ACCENT_OK }
        }
        MetricKind::Cpu => {
            if value > 85.0 { ACCENT_ALERT } else if value > 70.0 { ACCENT_WARN } else { ACCENT_OK }
        }
        MetricKind::Memory => ACCENT_INFO,
        MetricKind::Upstream => {
            if value > 100.0 { ACCENT_WARN } else { ACCENT_OK }
        }
        _ => ACCENT_OK,
    }
}

pub fn pick_traffic_display(st: &AppState) -> widgets::stacked_bar::TrafficDisplay {
    match st.force_traffic_unit {
        Some(TrafficUnit::Qps) => widgets::stacked_bar::TrafficDisplay::Qps,
        Some(TrafficUnit::Rpm) => widgets::stacked_bar::TrafficDisplay::Rpm,
        None => widgets::stacked_bar::TrafficDisplay::Auto,
    }
}

pub fn kpi_format(v: f64, unit: &str) -> String {
    match unit {
        "%" => format!("{:.1}%", v),
        "ms" => format!("{:.0}ms", v),
        "GB" => format!("{:.2} GB", v),
        "rpm" => widgets::chart::format_traffic(v),
        _ => format!("{:.1}", v),
    }
}

pub fn derive_current(md: &MetricData) -> (f64, String) {
    // For percentile metrics (latency) the "current" headline is P95 — P99 is
    // alarmist and dominated by tail outliers; P95 reflects the typical slow
    // user. Falls through to P99 / max / first if no P95 is available.
    if let Some(p95) = md.series.iter().find(|s| matches!(s.kind, SeriesKind::Percentile(95))) {
        return (p95.current(), "P95 · now".into());
    }
    if let Some(p99) = md.series.iter().find(|s| matches!(s.kind, SeriesKind::Percentile(99))) {
        return (p99.current(), "P99 · now".into());
    }
    if let Some(max_s) = md.series.iter().find(|s| s.label == "max") {
        let v = max_s.current();
        let owner = md
            .series
            .iter()
            .filter(|s| matches!(&s.kind, SeriesKind::Pod(_)))
            .max_by(|a, b| a.current().partial_cmp(&b.current()).unwrap())
            .map(|s| s.label.clone())
            .unwrap_or_default();
        return (v, format!("max · {}", owner));
    }
    if let Some(first) = md.series.first() {
        return (first.current(), format!("series · {}", first.label));
    }
    (0.0, "—".into())
}

pub fn derive_avg(md: &MetricData, metric: MetricKind) -> (f64, String) {
    // Pick the right series to average:
    //  - Latency → P95 (matches CURRENT card / panel headline)
    //  - Anything with an "avg" series → use that aggregate
    //  - Otherwise → first non-pod series
    let primary = match metric {
        MetricKind::Latency => md
            .series
            .iter()
            .find(|s| matches!(s.kind, SeriesKind::Percentile(95)))
            .or_else(|| md.series.iter().find(|s| matches!(s.kind, SeriesKind::Percentile(99))))
            .or_else(|| md.series.first()),
        _ => md
            .series
            .iter()
            .find(|s| s.label == "avg")
            .or_else(|| md.series.iter().find(|s| !matches!(s.kind, SeriesKind::Pod(_))))
            .or_else(|| md.series.first()),
    };
    let avg = primary.map(|s| s.stats().avg).unwrap_or(0.0);
    let sub = match primary.map(|s| s.label.as_str()) {
        Some("avg") | None => "time-window mean".to_string(),
        Some(label) => format!("over {}", label),
    };
    (avg, sub)
}

pub fn derive_peak(md: &MetricData) -> (f64, String) {
    let mut peak = 0.0_f64;
    let mut who = String::new();
    let mut when = String::new();
    for s in &md.series {
        for (t, v) in &s.points {
            if *v > peak {
                peak = *v;
                who = s.label.clone();
                when = t.with_timezone(&chrono::Local).format("%H:%M:%S").to_string();
            }
        }
    }
    (peak, format!("{} · {}", who, when))
}

pub fn derive_trend(md: &MetricData) -> (String, String, Rgb) {
    let primary = md
        .series
        .iter()
        .find(|s| !matches!(s.kind, SeriesKind::Pod(_)));
    if let Some(s) = primary {
        let n = s.points.len();
        if n >= 30 {
            let last: f64 = s.points[n - 10..].iter().map(|p| p.1).sum::<f64>() / 10.0;
            let earlier: f64 = s.points[n - 30..n - 20].iter().map(|p| p.1).sum::<f64>() / 10.0;
            let delta = last - earlier;
            let pct = if earlier.abs() > 1e-3 {
                delta / earlier * 100.0
            } else {
                0.0
            };
            let arrow = if delta > 0.0 { "↑" } else if delta < 0.0 { "↓" } else { "·" };
            let color = if pct.abs() > 5.0 { ACCENT_ALERT } else { ACCENT_INFO };
            return (
                format!("{} {:.1}%", arrow, pct.abs()),
                "vs 30m ago".into(),
                color,
            );
        }
    }
    ("· 0.0%".into(), "vs 30m ago".into(), ACCENT_INFO)
}

pub fn render_single_chart(
    metric: MetricKind,
    md: &MetricData,
    area: Rect,
    buf: &mut Buffer,
    time_range: Option<(chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)>,
) {
    use widgets::chart::{AreaChart, AreaSeries};
    let mut chart = AreaChart::new(&md.unit).cursor(true);
    if let Some((from, to)) = time_range {
        chart = chart.time_range(from, to);
    }
    let aggregate_only = matches!(metric, MetricKind::Cpu | MetricKind::Memory);

    if matches!(metric, MetricKind::Qps) {
        let series: Vec<widgets::stacked_bar::StackedSeries> = md
            .series
            .iter()
            .map(|s| widgets::stacked_bar::StackedSeries {
                label: s.label.clone(),
                color: match s.kind {
                    SeriesKind::StatusCode(c) => status_color(c),
                    _ => SERIES_GREEN,
                },
                points: s.points.iter().map(|p| p.1).collect(),
            })
            .collect();
        widgets::stacked_bar::StackedBar {
            series,
            unit: &md.unit,
            display: widgets::stacked_bar::TrafficDisplay::Auto,
            compact: false,
        }
        .render(area, buf);
        return;
    }

    for s in &md.series {
        if aggregate_only && (s.label == "max" || s.label == "avg") {
            let color = crate::commands::monitor::theme::series_color(metric, s);
            chart = chart.add(AreaSeries {
                points: s.points.iter().map(|p| p.1).collect(),
                color,
                dim: s.label == "avg",
                fill: true,
            });
        } else if !aggregate_only {
            let color = crate::commands::monitor::theme::series_color(metric, s);
            chart = chart.add(AreaSeries {
                points: s.points.iter().map(|p| p.1).collect(),
                color,
                dim: false,
                fill: true,
            });
        }
    }
    if aggregate_only {
        for s in &md.series {
            if matches!(&s.kind, SeriesKind::Pod(_)) {
                let color = crate::commands::monitor::theme::series_color(metric, s);
                chart = chart.add(AreaSeries {
                    points: s.points.iter().map(|p| p.1).collect(),
                    color,
                    dim: true,
                    fill: false,
                });
            }
        }
    }
    chart.render(area, buf);
}
