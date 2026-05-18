use ratatui::layout::{Constraint, Direction, Layout, Rect};

#[derive(Clone, Debug)]
pub struct PhoneRects {
    pub header: Rect,
    pub panel: Rect,
    pub indicator: Rect,
    pub footer: Rect,
}

pub fn compute_phone(area: Rect) -> PhoneRects {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(10),
            Constraint::Length(2),
            Constraint::Length(1),
        ])
        .split(area);
    PhoneRects {
        header: vertical[0],
        panel: vertical[1],
        indicator: vertical[2],
        footer: vertical[3],
    }
}
