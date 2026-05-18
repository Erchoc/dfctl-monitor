use super::colors::*;
use super::helpers::*;
use crate::commands::monitor::data::{MetricKind, SeriesKind};
use crate::commands::monitor::layout::phone::compute_phone;
use crate::commands::monitor::state::AppState;
use crate::commands::monitor::theme;
use crate::commands::monitor::widgets;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Widget};

pub fn draw_phone(area: Rect, buf: &mut Buffer, st: &AppState) {
    let rects = compute_phone(area);
    widgets::header::Header {
        state: st,
        data: st.data.as_ref(),
    }
    .render(rects.header, buf);

    let data = match &st.data {
        Some(d) => d,
        None => {
            render_loading(rects.panel, buf);
            widgets::footer::Footer { state: st }.render(rects.footer, buf);
            return;
        }
    };
    let order = MetricKind::all_default();
    let idx = st.focused_panel.min(order.len() - 1);
    let metric = order[idx];
    if let Some(md) = data.metrics.get(&metric) {
        widgets::panel::MetricPanel {
            metric,
            data: md,
            pods: &data.pods,
            focused: true,
            compact: true,
            agg_mode: st.agg_mode(metric),
            traffic_display: pick_traffic_display(st),
        }
        .render(rects.panel, buf);
    }
    let series_slice: &[crate::commands::monitor::data::Series] =
        data.metrics.get(&metric).map(|m| m.series.as_slice()).unwrap_or(&[]);
    let status_color = theme::assess_health(metric, series_slice).color();
    widgets::dot_indicator::DotIndicator {
        count: order.len(),
        current: idx,
        status_color,
    }
    .render(rects.indicator, buf);
    widgets::footer::Footer { state: st }.render(rects.footer, buf);
}

pub fn draw_single_phone(area: Rect, buf: &mut Buffer, st: &AppState, metric: MetricKind) {
    let pod_count = st.data.as_ref().map(|d| d.pods.len()).unwrap_or(0) as u16;
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(11),
            Constraint::Length(1),
            Constraint::Min(6),
            Constraint::Length(1),
            Constraint::Length(pod_count.max(1)),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);
    let header = vertical[0];
    let title_row = vertical[1];
    let kpi_area = vertical[2];
    let chart_row = vertical[4];
    let pods_row = vertical[6];
    let footer = vertical[8];

    widgets::header::Header {
        state: st,
        data: st.data.as_ref(),
    }
    .render(header, buf);

    let data = match &st.data {
        Some(d) => d,
        None => {
            render_loading(chart_row, buf);
            widgets::footer::Footer { state: st }.render(footer, buf);
            return;
        }
    };
    let md = match data.metrics.get(&metric) {
        Some(m) => m,
        None => return,
    };
    let status = theme::assess_health(metric, &md.series);
    let title_line = Line::from(vec![
        Span::styled(
            "❯ ",
            Style::default().fg(ACCENT_OK.to_color()).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            metric.title(),
            Style::default().fg(TEXT_PRIMARY.to_color()).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            status.badge().to_string(),
            Style::default()
                .fg(status.color().to_color())
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    Paragraph::new(title_line).render(title_row, buf);

    let kpi_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Length(1), Constraint::Length(5)])
        .split(kpi_area);
    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
        .split(kpi_rows[0]);
    let bot = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
        .split(kpi_rows[2]);

    let (curr_val, curr_sub) = derive_current(md);
    let (avg_val, avg_sub) = derive_avg(md);
    let (peak_val, peak_sub) = derive_peak(md);
    let (trend_val, trend_sub, trend_color) = derive_trend(md);

    let cards: [(Rect, &str, String, Rgb, String); 4] = [
        (top[0], "CURRENT", kpi_format(curr_val, &md.unit), pick_card_color(metric, curr_val), curr_sub),
        (top[1], "AVG", kpi_format(avg_val, &md.unit), ACCENT_INFO, avg_sub),
        (bot[0], "PEAK", kpi_format(peak_val, &md.unit), pick_card_color(metric, peak_val), peak_sub),
        (bot[1], "TREND 10m", trend_val, trend_color, trend_sub),
    ];
    for (rect, title, val, color, sub) in cards {
        widgets::kpi_card::KpiCard {
            title,
            value: val,
            value_color: color,
            sub,
        }
        .render(rect, buf);
    }

    let chart_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .style(Style::default().fg(BORDER_DIM.to_color()))
        .title(Line::from(vec![
            Span::styled("◆ ", Style::default().fg(status.color().to_color())),
            Span::styled(
                metric.title(),
                Style::default().fg(TEXT_PRIMARY.to_color()).add_modifier(Modifier::BOLD),
            ),
        ]));
    let chart_inner = chart_block.inner(chart_row);
    chart_block.render(chart_row, buf);
    render_single_chart(metric, md, chart_inner, buf, Some((data.time_range.from, data.time_range.to)));

    let pod_rects = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![Constraint::Length(1); data.pods.len().max(1)])
        .split(pods_row);
    for (i, pod) in data.pods.iter().enumerate() {
        let series = md
            .series
            .iter()
            .find(|s| matches!(&s.kind, SeriesKind::Pod(n) if n == &pod.name));
        if i < pod_rects.len() {
            widgets::pod_row::PodRow {
                pod,
                series,
                unit: &md.unit,
            }
            .render(pod_rects[i], buf);
        }
    }
    widgets::footer::Footer { state: st }.render(footer, buf);
}
