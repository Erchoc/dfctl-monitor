use super::colors::*;
use super::helpers::*;
use crate::commands::monitor::data::{MetricKind, SeriesKind};
use crate::commands::monitor::layout::single::compute_single;
use crate::commands::monitor::state::AppState;
use crate::commands::monitor::theme;
use crate::commands::monitor::widgets;
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
    let status = theme::assess_health(metric, &md.series);
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

    // Sidebar: pod cards stacked
    let pod_rects = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![
            Constraint::Ratio(1, data.pods.len().max(1) as u32);
            data.pods.len().max(1)
        ])
        .split(rects.sidebar);
    for (i, pod) in data.pods.iter().enumerate() {
        if i >= pod_rects.len() {
            break;
        }
        let series = md
            .series
            .iter()
            .find(|s| matches!(&s.kind, SeriesKind::Pod(n) if n == &pod.name));
        widgets::pod_card::PodCard {
            pod,
            series,
            unit: &md.unit,
        }
        .render(pod_rects[i], buf);
    }

    render_events(&data.events, rects.events, buf);
    widgets::footer::Footer { state: st }.render(rects.footer, buf);
}
