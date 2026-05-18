pub mod overview;
pub mod phone;
pub mod single;

use super::args::MonitorArgs;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutTier {
    TooSmall,
    Phone,
    SingleColumn,
    TwoByFour,
    TwoByFourLarge,
    TwoByFourSidebar,
    Portrait,
    SingleMetric,
}

impl LayoutTier {
    pub fn from_size(w: u16, h: u16, args: &MonitorArgs) -> Self {
        // Size-based tiers always win — even in single-metric mode we want phone
        // layout on a phone-sized terminal. `--metric` only controls which view
        // is initially shown (see AppState::new).
        if w < 60 || h < 20 {
            return Self::TooSmall;
        }
        if w < 100 || h < 30 {
            return Self::Phone;
        }
        // single-metric CLI flag opts into the dedicated SingleMetric tier only on
        // desktop-sized terminals — and only if user didn't ask for a specific size
        if !args.metric.is_empty() && args.metric.len() == 1 {
            return Self::SingleMetric;
        }
        if h > w && h > 80 {
            return Self::Portrait;
        }
        match w {
            0..=129 => Self::SingleColumn,
            130..=199 => Self::TwoByFour,
            200..=259 => Self::TwoByFourLarge,
            _ => Self::TwoByFourSidebar,
        }
    }
}
