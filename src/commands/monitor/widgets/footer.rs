use crate::commands::monitor::layout::LayoutTier;
use crate::commands::monitor::render::colors::*;
use crate::commands::monitor::state::{AppState, View};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

pub struct Footer<'a> {
    pub state: &'a AppState,
}

impl<'a> Footer<'a> {
    fn tier(&self) -> LayoutTier {
        LayoutTier::from_size(
            self.state.terminal_size.0,
            self.state.terminal_size.1,
            &self.state.args,
        )
    }
}

impl<'a> Widget for Footer<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 {
            return;
        }
        let key_style = Style::default()
            .fg(ACCENT_OK.to_color())
            .add_modifier(Modifier::BOLD);
        let txt_style = Style::default().fg(TEXT_SECONDARY.to_color());
        let dim_style = Style::default().fg(TEXT_DIM.to_color());

        // Footer hints are tiered by width. Listing every key on an 80-col
        // phone overflows and `[q] quit` gets clipped — instead pick a hint
        // set that fits, and surface `?` as the escape hatch to see the rest.
        let tier = self.tier();
        let phone = matches!(tier, LayoutTier::Phone);
        let key = |k: &str, label: &str| {
            vec![
                Span::styled(format!("[{}]", k), key_style),
                Span::styled(format!(" {}  ", label), txt_style),
            ]
        };

        let mut spans = Vec::new();
        match self.state.view {
            View::Overview => {
                if phone {
                    // Compact phone footer: only the essential keys, then `?` for the rest.
                    spans.extend(key("↑↓←→", "flip"));
                    spans.extend(key("⏎", "detail"));
                    spans.extend(key("t", "time"));
                    spans.extend(key("?", "more"));
                    spans.extend(key("q", "quit"));
                } else {
                    spans.extend(key("↑↓←→", "focus"));
                    spans.extend(key("⏎", "detail"));
                    spans.extend(key("a", "agg"));
                    spans.extend(key("t", "time"));
                    spans.extend(key("w", "watch"));
                    spans.extend(key("r", "refresh"));
                    spans.extend(key("?", "help"));
                    spans.extend(key("q", "quit"));
                }
            }
            View::SingleMetric(_) => {
                if phone {
                    spans.extend(key("esc", "back"));
                    spans.extend(key("←→", "metric"));
                    spans.extend(key("t", "time"));
                    spans.extend(key("?", "more"));
                    spans.extend(key("q", "quit"));
                } else {
                    spans.extend(key("esc", "back"));
                    spans.extend(key("←→", "metric"));
                    spans.extend(key("a", "agg"));
                    spans.extend(key("t", "time"));
                    spans.extend(key("w", "watch"));
                    spans.extend(key("?", "help"));
                    spans.extend(key("q", "quit"));
                }
            }
            View::Help => {
                spans.push(Span::styled("press any key to dismiss", dim_style));
            }
            View::RangePicker { .. } => {
                spans.extend(key("↑↓", "select"));
                spans.extend(key("⏎", "apply"));
                spans.extend(key("esc", "cancel"));
            }
        }
        if let Some(err) = &self.state.error {
            spans.push(Span::styled(
                format!("  ⚠ {}", err),
                Style::default().fg(ACCENT_WARN.to_color()),
            ));
        }
        Paragraph::new(Line::from(spans)).render(area, buf);
    }
}
