use super::data::MetricKind;
use super::layout::LayoutTier;
use super::state::{AppState, TrafficUnit, View, RANGE_OPTIONS};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Find the next visible metric in the requested direction, skipping
/// optional panels that the API returned no series for. Returns None
/// if no neighbour exists (shouldn't happen — required metrics always render).
fn neighbour_metric(st: &AppState, current: MetricKind, direction: i32) -> Option<MetricKind> {
    let order = MetricKind::all_default();
    let len = order.len();
    let start = order.iter().position(|x| *x == current).unwrap_or(0);
    let mut idx = start;
    for _ in 0..len {
        idx = (idx as i32 + direction).rem_euclid(len as i32) as usize;
        let candidate = order[idx];
        // If this is a required metric, take it. For optional metrics, check
        // that the API actually returned data (non-empty series).
        let has_data = if candidate.is_optional() {
            st.data
                .as_ref()
                .and_then(|d| d.metrics.get(&candidate))
                .map(|md| !md.series.is_empty())
                .unwrap_or(false)
        } else {
            true
        };
        if has_data && candidate != current {
            return Some(candidate);
        }
    }
    None
}

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
        // `t` for time-range (preferred mnemonic). `R` kept as an undocumented
        // alias for users who started on the old build.
        KeyCode::Char('t') | KeyCode::Char('R') => {
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
            // Phone tier: *all four arrows + hjkl* page through panels. The dot
            // indicator is horizontal, so users naturally expect ←→ to flip pages;
            // ↑↓ also works (vim-style + intuitive for vertical lists).
            KeyCode::Up
            | KeyCode::Left
            | KeyCode::Char('k')
            | KeyCode::Char('h')
                if matches!(tier, LayoutTier::Phone) =>
            {
                // Phone pager cycles only through visible panels so users
                // never land on an empty Upstream/Runtime page.
                st.cycle_focus_visible(-1)
            }
            KeyCode::Down
            | KeyCode::Right
            | KeyCode::Char('j')
            | KeyCode::Char('l')
                if matches!(tier, LayoutTier::Phone) =>
            {
                st.cycle_focus_visible(1)
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
            // ← / h : previous metric in detail view; [ kept as legacy alias.
            // Skips optional panels (Upstream, Runtime) that the API didn't
            // provide data for — bouncing into an empty page would be janky.
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('[') => {
                if let Some(next) = neighbour_metric(st, m, -1) {
                    st.focused_panel = MetricKind::all_default()
                        .iter()
                        .position(|x| *x == next)
                        .unwrap_or(0);
                    st.view = View::SingleMetric(next);
                }
            }
            // → / l : next metric in detail view; ] kept as legacy alias.
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Char(']') => {
                if let Some(next) = neighbour_metric(st, m, 1) {
                    st.focused_panel = MetricKind::all_default()
                        .iter()
                        .position(|x| *x == next)
                        .unwrap_or(0);
                    st.view = View::SingleMetric(next);
                }
            }
            _ => {}
        },
        View::Help | View::RangePicker { .. } => {}
    }
    Action::None
}
