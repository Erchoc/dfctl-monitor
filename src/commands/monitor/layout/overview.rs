use ratatui::layout::{Constraint, Direction, Layout, Rect};

#[derive(Clone, Debug)]
pub struct OverviewRects {
    pub header: Rect,
    pub panels: Vec<Rect>, // 8
    pub sidebar: Option<Rect>,
    pub footer: Rect,
}

pub fn compute_two_by_four(area: Rect, _large: bool, sidebar: bool) -> OverviewRects {
    let total = area;
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(10),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(total);
    let header = vertical[0];
    let body = vertical[2];
    let footer = vertical[4];

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
        footer,
    }
}

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

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![Constraint::Ratio(1, 8); 8])
        .split(body);
    OverviewRects {
        header,
        panels: rows.iter().copied().collect(),
        sidebar: None,
        footer,
    }
}
