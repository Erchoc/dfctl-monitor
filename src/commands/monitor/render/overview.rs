use super::helpers::{pick_traffic_display, render_loading};
use crate::commands::monitor::data::MetricKind;
use crate::commands::monitor::layout::{
    overview::{compute_single_column, compute_two_by_four},
    LayoutTier,
};
use crate::commands::monitor::state::AppState;
use crate::commands::monitor::widgets;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;

pub fn draw_overview(area: Rect, buf: &mut Buffer, st: &AppState, tier: LayoutTier) {
    let rects = match tier {
        LayoutTier::TwoByFour => compute_two_by_four(area, false, false),
        LayoutTier::TwoByFourLarge => compute_two_by_four(area, true, false),
        LayoutTier::TwoByFourSidebar => compute_two_by_four(area, true, true),
        LayoutTier::SingleColumn | LayoutTier::Phone | LayoutTier::Portrait => {
            compute_single_column(area)
        }
        _ => compute_two_by_four(area, false, false),
    };

    widgets::header::Header {
        state: st,
        data: st.data.as_ref(),
    }
    .render(rects.header, buf);

    let order = MetricKind::all_default();
    let data = match &st.data {
        Some(d) => d,
        None => {
            render_loading(rects.panels[0], buf);
            widgets::footer::Footer { state: st }.render(rects.footer, buf);
            return;
        }
    };
    for (i, panel) in rects.panels.iter().enumerate() {
        if i >= order.len() {
            break;
        }
        let metric = order[i];
        if let Some(md) = data.metrics.get(&metric) {
            widgets::panel::MetricPanel {
                metric,
                data: md,
                pods: &data.pods,
                focused: i == st.focused_panel,
                compact: matches!(tier, LayoutTier::SingleColumn | LayoutTier::Phone),
                agg_mode: st.agg_mode(metric),
                traffic_display: pick_traffic_display(st),
            }
            .render(*panel, buf);
        }
    }

    widgets::footer::Footer { state: st }.render(rects.footer, buf);
}
