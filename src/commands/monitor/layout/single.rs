use ratatui::layout::{Constraint, Direction, Layout, Rect};

#[derive(Clone, Debug)]
pub struct SingleRects {
    pub header: Rect,
    pub kpis: Vec<Rect>, // 4
    pub chart: Rect,
    pub sidebar: Rect,
    pub events: Rect,
    pub footer: Rect,
}

pub fn compute_single(area: Rect) -> SingleRects {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(5),
            Constraint::Length(1),
            Constraint::Min(10),
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);
    let header = vertical[0];
    let kpi_row = vertical[2];
    let chart_row = vertical[4];
    let events_row = vertical[6];
    let footer = vertical[8];

    let kpis = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
        ])
        .split(kpi_row);

    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(40), Constraint::Length(38)])
        .split(chart_row);
    let chart = main[0];
    let sidebar = main[1];

    SingleRects {
        header,
        kpis: kpis.iter().copied().collect(),
        chart,
        sidebar,
        events: events_row,
        footer,
    }
}
