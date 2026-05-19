use super::colors::*;
use super::helpers::*;
use crate::commands::monitor::data::{MetricKind, PodInfo, Series, SeriesKind};
use crate::commands::monitor::layout::single::compute_single;
use crate::commands::monitor::state::AppState;
use crate::commands::monitor::theme;
use crate::commands::monitor::widgets;

const OUTLIER_SIGMA: f64 = 1.5;
const RESTART_GRACE_SECONDS: u64 = 3600;

/// Pick the pods whose current value stands out from peers, plus pods that have
/// known anomalies (recent restart, crashing state). Returns at most a handful,
/// sorted by z-score descending.
fn select_outlier_pods<'a>(
    pods: &'a [(&'a PodInfo, Option<&'a Series>)],
) -> Vec<(&'a PodInfo, Option<&'a Series>, f64)> {
    // Always surface pods that aren't Running, regardless of metric numbers.
    let mut sick: Vec<(&PodInfo, Option<&Series>, f64)> = pods
        .iter()
        .filter(|(p, _)| p.status != "Running" || p.restarts > 0 && p.uptime_seconds < RESTART_GRACE_SECONDS)
        .map(|(p, s)| (*p, *s, f64::INFINITY))
        .collect();

    // Then surface metric outliers based on z-score across peers.
    let values: Vec<f64> = pods.iter().filter_map(|(_, s)| s.map(|s| s.current())).collect();
    if values.len() >= 2 {
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let var = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
        let std = var.sqrt();
        if std > 1e-6 {
            for (p, s) in pods {
                if let Some(series) = s {
                    let z = (series.current() - mean) / std;
                    if z.abs() >= OUTLIER_SIGMA && !sick.iter().any(|(ep, _, _)| ep.name == p.name) {
                        sick.push((p, *s, z));
                    }
                }
            }
        }
    }
    sick.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    sick
}
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Widget};

pub fn draw_single(area: Rect, buf: &mut Buffer, st: &AppState, metric: MetricKind) {
    let rects = compute_single(area);
    widgets::header::Header {
        state: st,
        data: st.data.as_ref(),
        compact: false,
    }
    .render(rects.header, buf);

    let data = match &st.data {
        Some(d) => d,
        None => {
            render_loading(rects.chart, buf);
            widgets::footer::Footer { state: st }.render(rects.footer, buf);
            return;
        }
    };
    let md = match data.metrics.get(&metric) {
        Some(m) => m,
        None => return,
    };
    let title_y = rects.header.y + 1;
    let status = theme::assess_health_with_thresholds(metric, &md.series, md.thresholds.as_ref());
    let title_line = Line::from(vec![
        Span::styled(
            "❯ ",
            Style::default()
                .fg(ACCENT_OK.to_color())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            metric.title(),
            Style::default()
                .fg(TEXT_PRIMARY.to_color())
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            status.badge().to_string(),
            Style::default()
                .fg(status.color().to_color())
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    Paragraph::new(title_line).render(Rect::new(area.x, title_y, area.width, 1), buf);

    // KPI cards
    let (curr_val, curr_sub) = derive_current(md);
    let (avg_val, avg_sub) = derive_avg(md);
    let (peak_val, peak_sub) = derive_peak(md);
    let (trend_val, trend_sub, trend_color) = derive_trend(md);

    let curr_color = pick_card_color(metric, curr_val);
    let peak_color = pick_card_color(metric, peak_val);
    let kpi_titles = ["CURRENT", "AVG", "PEAK", "TREND 10m"];
    let kpi_values = [
        (kpi_format(curr_val, &md.unit), curr_color, curr_sub),
        (kpi_format(avg_val, &md.unit), ACCENT_INFO, avg_sub),
        (kpi_format(peak_val, &md.unit), peak_color, peak_sub),
        (trend_val, trend_color, trend_sub),
    ];
    for (i, rect) in rects.kpis.iter().enumerate() {
        let (val, color, sub) = &kpi_values[i];
        widgets::kpi_card::KpiCard {
            title: kpi_titles[i],
            value: val.clone(),
            value_color: *color,
            sub: sub.clone(),
        }
        .render(*rect, buf);
    }

    // Main chart with cursor
    let chart_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .style(Style::default().fg(BORDER_DIM.to_color()))
        .title(Line::from(vec![
            Span::styled(
                "◆ ",
                Style::default().fg(status.color().to_color()),
            ),
            Span::styled(
                metric.title(),
                Style::default()
                    .fg(TEXT_PRIMARY.to_color())
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    let chart_inner = chart_block.inner(rects.chart);
    chart_block.render(rects.chart, buf);
    render_single_chart(metric, md, chart_inner, buf, Some((data.time_range.from, data.time_range.to)));

    // ── Sidebar: pod cards on a need-to-show basis ──
    //
    // Showing every pod in the sidebar is noise when the load is balanced
    // (the user's words: "正常都是负载均衡分摊流量的"). Only render a pod card
    // when that pod's current value stands out from its peers (outlier rule),
    // or when the pod itself has anomalies (restarted, crashing).
    //
    // The list is sorted by deviation desc, then truncated to the sidebar's
    // available slot count → automatic "top-N anomalies" pagination.
    let pod_series: Vec<(&PodInfo, Option<&Series>)> = data
        .pods
        .iter()
        .map(|pod| {
            let s = md
                .series
                .iter()
                .find(|s| matches!(&s.kind, SeriesKind::Pod(n) if n == &pod.name));
            (pod, s)
        })
        .collect();

    let anomalies = select_outlier_pods(&pod_series);
    if anomalies.is_empty() {
        // All pods within tolerance — say so, don't pad the sidebar with empty cards.
        let line = Line::from(vec![
            Span::styled("◉ ", Style::default().fg(ACCENT_OK.to_color())),
            Span::styled(
                "All pods within tolerance",
                Style::default()
                    .fg(TEXT_PRIMARY.to_color())
                    .add_modifier(Modifier::BOLD),
            ),
        ]);
        Paragraph::new(line).render(
            Rect::new(rects.sidebar.x, rects.sidebar.y, rects.sidebar.width, 1),
            buf,
        );
        let sub = Line::from(vec![Span::styled(
            format!(
                "  {} pods, max−avg drift < {}σ",
                data.pods.len(),
                OUTLIER_SIGMA
            ),
            Style::default().fg(TEXT_SECONDARY.to_color()),
        )]);
        Paragraph::new(sub).render(
            Rect::new(rects.sidebar.x, rects.sidebar.y + 1, rects.sidebar.width, 1),
            buf,
        );
    } else {
        let visible = anomalies.iter().take(3).count().max(1);
        let pod_rects = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Ratio(1, visible as u32); visible])
            .split(rects.sidebar);
        for (slot, (pod, series, _z)) in anomalies.iter().take(3).enumerate() {
            if slot >= pod_rects.len() {
                break;
            }
            widgets::pod_card::PodCard {
                pod,
                series: *series,
                unit: &md.unit,
            }
            .render(pod_rects[slot], buf);
        }
        // If there are more outliers than slots, hint at the overflow.
        if anomalies.len() > 3 {
            let last_slot = pod_rects[visible - 1];
            let hint = Line::from(Span::styled(
                format!("  +{} more anomalies", anomalies.len() - 3),
                Style::default().fg(TEXT_DIM.to_color()),
            ));
            Paragraph::new(hint).render(
                Rect::new(
                    last_slot.x,
                    last_slot.y + last_slot.height.saturating_sub(1),
                    last_slot.width,
                    1,
                ),
                buf,
            );
        }
    }

    render_events(&data.events, rects.events, buf);
    widgets::footer::Footer { state: st }.render(rects.footer, buf);
}
