use ratatui::layout::{Constraint, Direction, Layout, Rect};

#[derive(Clone, Debug)]
pub struct PhoneRects {
    pub header: Rect,
    pub panel: Rect,
    /// One-row X-axis time scale below the panel, so phone users see the same
    /// time window evidence as desktop.
    pub time_axis: Rect,
    pub indicator: Rect,
    pub footer: Rect,
}

pub fn compute_phone(area: Rect) -> PhoneRects {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // header
            Constraint::Min(8),    // panel
            Constraint::Length(1), // X-axis time scale
            Constraint::Length(2), // dot indicator (dots + numbers)
            Constraint::Length(1), // footer
        ])
        .split(area);
    PhoneRects {
        header: vertical[0],
        panel: vertical[1],
        time_axis: vertical[2],
        indicator: vertical[3],
        footer: vertical[4],
    }
}
