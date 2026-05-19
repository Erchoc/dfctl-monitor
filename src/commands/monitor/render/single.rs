use super::colors::*;
use super::helpers::*;
use crate::commands::monitor::data::{MetricKind, PodInfo, Series, SeriesKind};
use crate::commands::monitor::layout::single::compute_single;
use crate::commands::monitor::state::AppState;
use crate::commands::monitor::theme;
use crate::commands::monitor::widgets;
use crate::commands::monitor::widgets::pod_card::PodCardReason;

/// For Memory KPIs, return a "of XG limit" annotation based on the hottest
/// pod's memory limit. Returns None for other metrics — keeps the helper
/// no-op outside the case the user actually flagged.
fn memory_limit_annotation(metric: MetricKind, pods: &[PodInfo]) -> Option<String> {
    if !matches!(metric, MetricKind::Memory) {
        return None;
    }
    let hot = pods.iter().max_by(|a, b| a.mem_bytes.cmp(&b.mem_bytes))?;
    let limit_gb = hot.mem_limit_bytes? as f64 / (1024.0 * 1024.0 * 1024.0);
    Some(format!("of {:.0}G", limit_gb))
}

const OUTLIER_SIGMA: f64 = 1.5;
const RESTART_GRACE_SECONDS: u64 = 3600;
/// Below this pod count, show every pod with a full card (small-cluster mode).
const SMALL_CLUSTER_THRESHOLD: usize = 3;
/// At and below this count, show outliers + named summary of the calm pods.
const MEDIUM_CLUSTER_THRESHOLD: usize = 10;

#[derive(Debug)]
struct SidebarEntry<'a> {
    pod: &'a PodInfo,
    series: Option<&'a Series>,
    reason: PodCardReason,
    /// Z-score vs peers (used for sort order). NaN means "unhealthy", which
    /// always wins over numeric outliers.
    z: f64,
}

/// Decide what to render in the detail-view sidebar based on cluster size.
///
/// - 1–3 pods (small cluster): every pod gets a card with reason=Routine,
///   so the sidebar is never empty even when the metric has no per-pod data.
/// - 4–10 pods (medium): outliers get cards, the rest are summarised as
///   "+N pods within tolerance".
/// - 11+ pods (large): only outliers, plus a count of healthy peers.
///
/// User feedback: 1-10 实例效果要好, 详情页不能空白也不能突兀; 10+ 不需要太详细。
fn plan_sidebar<'a>(
    pods: &'a [PodInfo],
    series_of: impl Fn(&PodInfo) -> Option<&'a Series>,
) -> (Vec<SidebarEntry<'a>>, SidebarSummary) {
    let mut entries: Vec<SidebarEntry> = Vec::new();
    let total = pods.len();

    // ── compute z-scores once for the whole list ──
    let metric_values: Vec<f64> = pods.iter().filter_map(&series_of).map(|s| s.current()).collect();
    let (mean, std) = if metric_values.len() >= 2 {
        let mean = metric_values.iter().sum::<f64>() / metric_values.len() as f64;
        let var = metric_values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / metric_values.len() as f64;
        (mean, var.sqrt())
    } else {
        (0.0, 0.0)
    };

    let unhealthy = |p: &PodInfo| {
        p.status != "Running"
            || (p.restarts > 0 && p.uptime_seconds < RESTART_GRACE_SECONDS)
    };

    // Small cluster: surface every pod with a routine card.
    if total <= SMALL_CLUSTER_THRESHOLD {
        for p in pods {
            let s = series_of(p);
            let reason = if unhealthy(p) {
                PodCardReason::Unhealthy
            } else {
                PodCardReason::Routine
            };
            let z = if let Some(series) = s {
                if std > 1e-6 { (series.current() - mean) / std } else { 0.0 }
            } else {
                0.0
            };
            entries.push(SidebarEntry { pod: p, series: s, reason, z });
        }
        return (entries, SidebarSummary::All { total });
    }

    // Medium/large: pick outliers + unhealthy pods.
    let mut outlier_count = 0;
    for p in pods {
        let s = series_of(p);
        let mut chosen: Option<PodCardReason> = None;
        if unhealthy(p) {
            chosen = Some(PodCardReason::Unhealthy);
        } else if let Some(series) = s {
            if std > 1e-6 {
                let z = (series.current() - mean) / std;
                if z >= OUTLIER_SIGMA {
                    chosen = Some(PodCardReason::OutlierHigh);
                } else if z <= -OUTLIER_SIGMA {
                    chosen = Some(PodCardReason::OutlierLow);
                }
            }
        }
        if let Some(reason) = chosen {
            outlier_count += 1;
            let z = if matches!(reason, PodCardReason::Unhealthy) {
                f64::INFINITY
            } else if let Some(series) = s {
                if std > 1e-6 { (series.current() - mean) / std } else { 0.0 }
            } else {
                0.0
            };
            entries.push(SidebarEntry { pod: p, series: s, reason, z });
        }
    }

    entries.sort_by(|a, b| b.z.abs().partial_cmp(&a.z.abs()).unwrap_or(std::cmp::Ordering::Equal));
    let calm = total - outlier_count;
    let summary = if total <= MEDIUM_CLUSTER_THRESHOLD {
        SidebarSummary::MediumCluster { calm, total }
    } else {
        SidebarSummary::LargeCluster { calm, total }
    };
    (entries, summary)
}

#[derive(Debug, Clone, Copy)]
enum SidebarSummary {
    /// Every pod is shown; no summary needed.
    All { total: usize },
    /// Medium cluster (4–10): outliers shown, calm pods summarised by name count.
    MediumCluster { calm: usize, total: usize },
    /// Large cluster (10+): only outliers, brief summary at the bottom.
    LargeCluster { calm: usize, total: usize },
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
    let (avg_val, avg_sub) = derive_avg(md, metric);
    let (peak_val, peak_sub) = derive_peak(md);
    let (trend_val, trend_sub, trend_color) = derive_trend(md);

    // Annotate Memory KPIs with `of XG limit` (the hot pod's limit) so users
    // see headroom on the CURRENT/PEAK cards, not just in the panel subtitle.
    let limit_annotation = memory_limit_annotation(metric, &data.pods);

    let curr_color = pick_card_color(metric, curr_val);
    let peak_color = pick_card_color(metric, peak_val);
    let kpi_titles = ["CURRENT", "AVG", "PEAK", "TREND 10m"];
    let with_lim = |sub: String| -> String {
        if let Some(ref lim) = limit_annotation {
            format!("{} · {}", sub, lim)
        } else {
            sub
        }
    };
    let kpi_values = [
        (kpi_format(curr_val, &md.unit), curr_color, with_lim(curr_sub)),
        (kpi_format(avg_val, &md.unit), ACCENT_INFO, avg_sub),
        (kpi_format(peak_val, &md.unit), peak_color, with_lim(peak_sub)),
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

    // ── Sidebar: pods, sized by cluster scale ──
    //
    // Small (1–3): every pod has a full card with PodInfo fields visible even
    // when the metric has no per-pod series.
    // Medium (4–10): outliers + a one-line "+N pods within tolerance" summary.
    // Large (11+): only outliers, plus an overflow count.
    let (entries, summary) = plan_sidebar(&data.pods, |p| {
        md.series
            .iter()
            .find(|s| matches!(&s.kind, SeriesKind::Pod(n) if n == &p.name))
    });

    render_sidebar(rects.sidebar, buf, &entries, summary, &md.unit);

    render_events(&data.events, rects.events, buf);
    widgets::footer::Footer { state: st }.render(rects.footer, buf);
}

/// Render the per-pod sidebar.
///
/// Layout adapts to entry count:
/// - 1 entry: full-height card
/// - 2–3 entries: evenly stacked, each ~⅓ height
/// - 4+ entries (already filtered to outliers): cards on top, then a summary
///   row at the bottom counting the calm peers.
fn render_sidebar(
    area: Rect,
    buf: &mut Buffer,
    entries: &[SidebarEntry],
    summary: SidebarSummary,
    unit: &str,
) {
    if entries.is_empty() {
        // Nothing to surface — say so prominently rather than leave a void.
        let total = match summary {
            SidebarSummary::All { total }
            | SidebarSummary::MediumCluster { total, .. }
            | SidebarSummary::LargeCluster { total, .. } => total,
        };
        let body = vec![
            Line::from(""),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("◉ ", Style::default().fg(ACCENT_OK.to_color())),
                Span::styled(
                    "All pods within tolerance",
                    Style::default()
                        .fg(TEXT_PRIMARY.to_color())
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![Span::styled(
                format!(
                    "    {} pods, max−avg drift < {}σ",
                    total, OUTLIER_SIGMA
                ),
                Style::default().fg(TEXT_SECONDARY.to_color()),
            )]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "    Press ← / → to view another metric",
                Style::default().fg(TEXT_DIM.to_color()),
            )]),
        ];
        Paragraph::new(body).render(area, buf);
        return;
    }

    // ── decide how many cards fit, plus whether to reserve a summary footer ──
    let needs_summary = matches!(
        summary,
        SidebarSummary::MediumCluster { calm, .. } | SidebarSummary::LargeCluster { calm, .. }
            if calm > 0
    );
    let summary_h: u16 = if needs_summary { 2 } else { 0 };
    let card_area_h = area.height.saturating_sub(summary_h);
    // Each card wants ≥ 6 rows; cap visible cards based on space.
    let max_cards_by_space = (card_area_h / 6).max(1) as usize;
    let max_cards_by_policy = match summary {
        SidebarSummary::All { .. } => entries.len(),
        SidebarSummary::MediumCluster { .. } => 4,
        SidebarSummary::LargeCluster { .. } => 3,
    };
    let visible = entries.len().min(max_cards_by_space).min(max_cards_by_policy).max(1);

    let card_area = Rect::new(area.x, area.y, area.width, card_area_h);
    let card_rects = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![Constraint::Ratio(1, visible as u32); visible])
        .split(card_area);
    for (slot, entry) in entries.iter().take(visible).enumerate() {
        if slot >= card_rects.len() {
            break;
        }
        widgets::pod_card::PodCard {
            pod: entry.pod,
            series: entry.series,
            unit,
            reason: entry.reason,
        }
        .render(card_rects[slot], buf);
    }

    // ── summary footer ──
    if needs_summary {
        let footer_y = area.y + card_area_h;
        let footer_rect = Rect::new(area.x, footer_y, area.width, summary_h);
        let (calm, total) = match summary {
            SidebarSummary::MediumCluster { calm, total }
            | SidebarSummary::LargeCluster { calm, total } => (calm, total),
            _ => (0, 0),
        };
        let hidden = entries.len().saturating_sub(visible);
        let line1 = if hidden > 0 {
            Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(
                    format!("+{} more outliers", hidden),
                    Style::default()
                        .fg(ACCENT_WARN.to_color())
                        .add_modifier(Modifier::BOLD),
                ),
            ])
        } else {
            Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled("◉ ", Style::default().fg(ACCENT_OK.to_color())),
                Span::styled(
                    format!("{} pods within tolerance", calm),
                    Style::default().fg(TEXT_PRIMARY.to_color()),
                ),
            ])
        };
        let line2 = Line::from(vec![Span::styled(
            format!("    {} total · {} flagged", total, entries.len()),
            Style::default().fg(TEXT_DIM.to_color()),
        )]);
        Paragraph::new(vec![line1, line2]).render(footer_rect, buf);
    }
}
