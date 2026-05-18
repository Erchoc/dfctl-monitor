use super::data::MetricKind;
use super::layout::LayoutTier;
use super::state::{AppState, TrafficUnit, View, RANGE_OPTIONS};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy)]
pub enum Action {
    None,
    Quit,
    ToggleWatch,
    TogglePause,
    RefreshNow,
    RangeChanged,
}

pub fn handle_key(key: KeyEvent, st: &mut AppState, tier: LayoutTier) -> Action {
    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
        return Action::Quit;
    }

    if matches!(st.view, View::Help) {
        st.view = View::Overview;
        return Action::None;
    }

    // Range picker overlay handles its own keys
    if let View::RangePicker { previous, selected } = &st.view {
        let n = RANGE_OPTIONS.len();
        let mut sel = *selected;
        let prev = previous.clone();
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                st.view = *prev;
                return Action::None;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                sel = (sel + n - 1) % n;
                st.view = View::RangePicker { previous: prev, selected: sel };
                return Action::None;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                sel = (sel + 1) % n;
                st.view = View::RangePicker { previous: prev, selected: sel };
                return Action::None;
            }
            KeyCode::Enter => {
                let (code, _) = RANGE_OPTIONS[sel];
                if let Ok(d) = code.parse::<humantime::Duration>() {
                    st.args.since = d;
                }
                st.view = *prev;
                return Action::RangeChanged;
            }
            _ => return Action::None,
        }
    }

    match key.code {
        KeyCode::Char('q') => return Action::Quit,
        KeyCode::Char('?') => {
            st.view = View::Help;
            return Action::None;
        }
        KeyCode::Char('w') => return Action::ToggleWatch,
        KeyCode::Char(' ') => return Action::TogglePause,
        KeyCode::Char('r') => return Action::RefreshNow,
        KeyCode::Char('a') => {
            // cycle aggregation mode on focused (overview) or current (detail) metric
            let metric = match st.view {
                View::SingleMetric(m) => m,
                _ => st.focused_metric(),
            };
            st.cycle_agg(metric);
            return Action::None;
        }
        KeyCode::Char('R') => {
            // Open range picker, remembering where to return to
            let previous = Box::new(st.view.clone());
            // pre-select the index that matches current --since if possible
            let cur = st.args.since.to_string();
            let selected = RANGE_OPTIONS
                .iter()
                .position(|(k, _)| *k == cur.as_str())
                .unwrap_or(2);
            st.view = View::RangePicker { previous, selected };
            return Action::None;
        }
        KeyCode::Char('u') => {
            st.force_traffic_unit = match st.force_traffic_unit {
                None => Some(TrafficUnit::Qps),
                Some(TrafficUnit::Qps) => Some(TrafficUnit::Rpm),
                Some(TrafficUnit::Rpm) => None,
            };
            return Action::None;
        }
        _ => {}
    }

    match st.view {
        View::Overview => match key.code {
            // Phone tier: arrows / hjkl all page through panels (no 2D grid)
            KeyCode::Up | KeyCode::Char('k') if matches!(tier, LayoutTier::Phone) => {
                st.cycle_focus(-1)
            }
            KeyCode::Down | KeyCode::Char('j') if matches!(tier, LayoutTier::Phone) => {
                st.cycle_focus(1)
            }
            // Desktop tiers: 2D grid movement
            KeyCode::Left | KeyCode::Char('h') => st.move_focus(-1, 0),
            KeyCode::Right | KeyCode::Char('l') => st.move_focus(1, 0),
            KeyCode::Up | KeyCode::Char('k') => st.move_focus(0, -1),
            KeyCode::Down | KeyCode::Char('j') => st.move_focus(0, 1),
            KeyCode::Tab => st.cycle_focus(1),
            KeyCode::BackTab => st.cycle_focus(-1),
            KeyCode::Enter => {
                let m = st.focused_metric();
                st.view = View::SingleMetric(m);
            }
            _ => {}
        },
        View::SingleMetric(m) => match key.code {
            KeyCode::Esc => {
                st.view = View::Overview;
            }
            KeyCode::Char('[') | KeyCode::Left => {
                let order = MetricKind::all_default();
                let idx = order.iter().position(|x| *x == m).unwrap_or(0);
                let next = (idx + order.len() - 1) % order.len();
                st.view = View::SingleMetric(order[next]);
                st.focused_panel = next;
            }
            KeyCode::Char(']') | KeyCode::Right => {
                let order = MetricKind::all_default();
                let idx = order.iter().position(|x| *x == m).unwrap_or(0);
                let next = (idx + 1) % order.len();
                st.view = View::SingleMetric(order[next]);
                st.focused_panel = next;
            }
            _ => {}
        },
        View::Help | View::RangePicker { .. } => {}
    }
    Action::None
}
