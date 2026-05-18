use super::args::MonitorArgs;
use super::data::{MetricKind, MonitorResponse};
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AggMode {
    Default,
    Max,
    Avg,
    Sum,
    P95,
    PerPod,
}

impl AggMode {
    pub fn cycle(self) -> Self {
        match self {
            Self::Default => Self::Max,
            Self::Max => Self::Avg,
            Self::Avg => Self::Sum,
            Self::Sum => Self::P95,
            Self::P95 => Self::PerPod,
            Self::PerPod => Self::Default,
        }
    }
    pub fn label(self) -> Option<&'static str> {
        match self {
            Self::Default => None,
            Self::Max => Some("max"),
            Self::Avg => Some("avg"),
            Self::Sum => Some("sum"),
            Self::P95 => Some("p95"),
            Self::PerPod => Some("per-pod"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum View {
    Overview,
    SingleMetric(MetricKind),
    Help,
    RangePicker { previous: Box<View>, selected: usize },
}

pub const RANGE_OPTIONS: &[(&str, &str)] = &[
    ("15m", "15 minutes"),
    ("1h", "1 hour"),
    ("3h", "3 hours"),
    ("6h", "6 hours"),
    ("12h", "12 hours"),
    ("24h", "24 hours"),
];

#[derive(Clone, Debug)]
pub struct AppState {
    pub args: MonitorArgs,
    pub data: Option<MonitorResponse>,
    pub last_fetch: Option<Instant>,
    pub fetch_in_flight: bool,
    pub next_refresh_at: Option<Instant>,
    pub view: View,
    pub focused_panel: usize,
    pub watch_enabled: bool,
    pub watch_paused: bool,
    pub error: Option<String>,
    pub terminal_size: (u16, u16),
    pub agg_modes: HashMap<MetricKind, AggMode>,
    pub force_traffic_unit: Option<TrafficUnit>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrafficUnit {
    Rpm,
    Qps,
}

impl AppState {
    pub fn new(args: MonitorArgs, size: (u16, u16)) -> Self {
        let watch_enabled = args.watch;
        let initial_view = if args.metric.len() == 1 {
            if let Some(m) = MetricKind::from_str(&args.metric[0]) {
                View::SingleMetric(m)
            } else {
                View::Overview
            }
        } else {
            View::Overview
        };
        let initial_focus = if let View::SingleMetric(m) = initial_view {
            MetricKind::all_default()
                .iter()
                .position(|x| *x == m)
                .unwrap_or(0)
        } else {
            0
        };
        Self {
            args,
            data: None,
            last_fetch: None,
            fetch_in_flight: false,
            next_refresh_at: None,
            view: initial_view,
            focused_panel: initial_focus,
            watch_enabled,
            watch_paused: false,
            error: None,
            terminal_size: size,
            agg_modes: HashMap::new(),
            force_traffic_unit: None,
        }
    }

    pub fn agg_mode(&self, metric: MetricKind) -> AggMode {
        *self.agg_modes.get(&metric).unwrap_or(&AggMode::Default)
    }

    pub fn cycle_agg(&mut self, metric: MetricKind) {
        let cur = self.agg_mode(metric);
        self.agg_modes.insert(metric, cur.cycle());
    }

    pub fn refresh_interval(&self) -> Duration {
        self.args.interval.into()
    }

    pub fn focused_metric(&self) -> MetricKind {
        let order = MetricKind::all_default();
        order[self.focused_panel.min(order.len() - 1)]
    }

    pub fn move_focus(&mut self, dx: i32, dy: i32) {
        // 2×4 grid: 4 rows of 2 cols (left col indices 0,2,4,6; right 1,3,5,7)
        let cur = self.focused_panel as i32;
        let row = cur / 2;
        let col = cur % 2;
        let new_col = (col + dx).rem_euclid(2);
        let new_row = (row + dy).rem_euclid(4);
        self.focused_panel = (new_row * 2 + new_col) as usize;
    }

    pub fn cycle_focus(&mut self, delta: i32) {
        let total = MetricKind::all_default().len() as i32;
        let n = (self.focused_panel as i32 + delta).rem_euclid(total);
        self.focused_panel = n as usize;
    }

    pub fn countdown_seconds(&self) -> Option<u64> {
        if !self.watch_enabled || self.watch_paused {
            return None;
        }
        let next = self.next_refresh_at?;
        let now = Instant::now();
        if next <= now {
            Some(0)
        } else {
            Some((next - now).as_secs())
        }
    }
}
