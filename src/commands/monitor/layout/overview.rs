use ratatui::layout::{Constraint, Direction, Layout, Rect};

#[derive(Clone, Debug)]
pub struct OverviewRects {
    pub header: Rect,
    pub panels: Vec<Rect>, // 8
    pub sidebar: Option<Rect>,
    pub time_axis: Option<Rect>,
    pub footer: Rect,
}

pub fn compute_two_by_four(area: Rect, _large: bool, sidebar: bool) -> OverviewRects {
    let total = area;
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // header
            Constraint::Length(1), // gutter
            Constraint::Min(10),   // body (8 panels)
            Constraint::Length(1), // X axis time scale
            Constraint::Length(1), // gutter
            Constraint::Length(1), // footer
        ])
        .split(total);
    let header = vertical[0];
    let body = vertical[2];
    let footer = vertical[5];

    let (main, side) = if sidebar {
        let h = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(40), Constraint::Length(36)])
            .split(body);
        (h[0], Some(h[1]))
    } else {
        (body, None)
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(main);

    let mut panels = Vec::with_capacity(8);
    for row in rows.iter() {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(*row);
        panels.push(cols[0]);
        panels.push(cols[1]);
    }
    OverviewRects {
        header,
        panels,
        sidebar: side,
        time_axis: Some(vertical[3]),
        footer,
    }
}

/// Single-column layout for narrow desktop / portrait terminals.
///
/// Eight panels stacked vertically with **weighted heights**: the first three
/// (QPS / Latency / Error Rate — the golden signals) get 2× the height of the
/// other five so they read like real charts instead of a uniform wall. This
/// matches the "重点 + 列表" pattern Codex recommended for the portrait tier.
pub fn compute_single_column(area: Rect) -> OverviewRects {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(10),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);
    let header = vertical[0];
    let body = vertical[2];
    let footer = vertical[4];

    // Weights: golden signals × 2, the rest × 1.
    // Order matches MetricKind::all_default(): Qps, Latency, ErrorRate,
    // Cpu, Memory, Replicas, Upstream, Runtime.
    let weights = [2u16, 2, 2, 1, 1, 1, 1, 1];
    let constraints: Vec<Constraint> = weights.iter().map(|w| Constraint::Ratio(*w as u32, 11)).collect();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(body);
    OverviewRects {
        header,
        panels: rows.iter().copied().collect(),
        sidebar: None,
        time_axis: None,
        footer,
    }
}
