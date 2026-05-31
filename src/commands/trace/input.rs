use super::state::{FlowAnim, TraceAppState, TraceView};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::time::Instant;

#[derive(Debug, Clone, Copy)]
pub enum Action {
    None,
    Quit,
    ToggleWatch,
    TogglePause,
    RefreshNow,
}

pub fn handle_key(key: KeyEvent, st: &mut TraceAppState) -> Action {
    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
        return Action::Quit;
    }

    // Help overlay: any key dismisses.
    if matches!(st.view, TraceView::Help) {
        st.view = TraceView::Waterfall;
        return Action::None;
    }

    // Global keys available in every data view.
    match key.code {
        KeyCode::Char('q') => return Action::Quit,
        KeyCode::Char('?') => {
            st.view = TraceView::Help;
            return Action::None;
        }
        KeyCode::Char('w') => return Action::ToggleWatch,
        KeyCode::Char(' ') => return Action::TogglePause,
        KeyCode::Char('r') => return Action::RefreshNow,
        _ => {}
    }

    match &st.view {
        TraceView::SpanDetail(_) => match key.code {
            KeyCode::Esc => st.view = TraceView::Waterfall,
            KeyCode::Up | KeyCode::Char('k') => {
                st.move_selection(-1);
                if let Some(id) = st.selected_span_id() {
                    st.view = TraceView::SpanDetail(id);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                st.move_selection(1);
                if let Some(id) = st.selected_span_id() {
                    st.view = TraceView::SpanDetail(id);
                }
            }
            _ => {}
        },
        TraceView::Summary => match key.code {
            KeyCode::Esc | KeyCode::Char('s') => st.view = TraceView::Waterfall,
            _ => {}
        },
        TraceView::Waterfall => match key.code {
            KeyCode::Up | KeyCode::Char('k') => st.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => st.move_selection(1),
            KeyCode::Char('g') => st.selected = 0,
            KeyCode::Char('G') => {
                let len = st.visible().len();
                st.selected = len.saturating_sub(1);
            }
            KeyCode::Left | KeyCode::Char('h') => st.toggle_collapse(),
            KeyCode::Right | KeyCode::Char('l') => st.expand(),
            KeyCode::Enter => {
                if let Some(id) = st.selected_span_id() {
                    st.view = TraceView::SpanDetail(id);
                }
            }
            KeyCode::Char('s') => st.view = TraceView::Summary,
            KeyCode::Char('c') => st.critical_only = !st.critical_only,
            KeyCode::Char('e') => st.jump_error(1),
            KeyCode::Char('E') => st.jump_error(-1),
            KeyCode::Char('f') => {
                st.flow = Some(FlowAnim {
                    started_at: Instant::now(),
                    play_secs: 2.5,
                });
            }
            _ => {}
        },
        TraceView::Help => {}
    }
    Action::None
}
