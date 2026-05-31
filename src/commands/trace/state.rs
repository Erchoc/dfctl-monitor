use super::args::TraceArgs;
use super::data::TraceResponse;
use super::stats::TraceStats;
use std::collections::HashSet;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TraceView {
    Waterfall,
    Summary,
    SpanDetail(String),
    Help,
}

/// Comet/flow animation that walks the request through the call chain in time
/// order. Started by `f`, advances in real time, and auto-stops at the end.
#[derive(Clone, Debug)]
pub struct FlowAnim {
    pub started_at: Instant,
    /// How long (wall clock) one full playthrough takes.
    pub play_secs: f32,
}

impl FlowAnim {
    /// Fraction of the trace [0,1] the comet has reached, or None when done.
    pub fn progress(&self) -> Option<f32> {
        let t = self.started_at.elapsed().as_secs_f32() / self.play_secs;
        if t >= 1.0 {
            None
        } else {
            Some(t)
        }
    }
}

pub struct TraceAppState {
    pub args: TraceArgs,
    pub data: Option<TraceResponse>,
    pub stats: Option<TraceStats>,
    pub last_fetch: Option<Instant>,
    pub fetch_in_flight: bool,
    pub next_refresh_at: Option<Instant>,

    pub view: TraceView,
    /// index into the current visible span order
    pub selected: usize,
    pub collapsed: HashSet<String>,
    pub critical_only: bool,
    pub flow: Option<FlowAnim>,

    pub watch_enabled: bool,
    pub watch_paused: bool,
    pub error: Option<String>,
    pub terminal_size: (u16, u16),

    /// Wall-clock origin for ambient animations (intro reveal, pulse, scan line).
    pub anim_start: Instant,
}

impl TraceAppState {
    pub fn new(args: TraceArgs, size: (u16, u16)) -> Self {
        let watch_enabled = args.watch;
        let view = if args.summary {
            TraceView::Summary
        } else {
            TraceView::Waterfall
        };
        Self {
            args,
            data: None,
            stats: None,
            last_fetch: None,
            fetch_in_flight: false,
            next_refresh_at: None,
            view,
            selected: 0,
            collapsed: HashSet::new(),
            critical_only: false,
            flow: None,
            watch_enabled,
            watch_paused: false,
            error: None,
            terminal_size: size,
            anim_start: Instant::now(),
        }
    }

    pub fn refresh_interval(&self) -> Duration {
        self.args.interval.into()
    }

    /// Visible span ids in display order (depth-first, honoring collapse).
    pub fn visible(&self) -> Vec<(String, u16, bool)> {
        self.stats
            .as_ref()
            .map(|s| s.visible_order(&self.collapsed))
            .unwrap_or_default()
    }

    pub fn selected_span_id(&self) -> Option<String> {
        let v = self.visible();
        v.get(self.selected).map(|(id, _, _)| id.clone())
    }

    pub fn move_selection(&mut self, delta: i32) {
        let len = self.visible().len();
        if len == 0 {
            return;
        }
        let n = (self.selected as i32 + delta).clamp(0, len as i32 - 1);
        self.selected = n as usize;
    }

    /// Jump selection to the next/prev error span in visible order.
    pub fn jump_error(&mut self, dir: i32) {
        let v = self.visible();
        if v.is_empty() {
            return;
        }
        let err_ids: HashSet<&String> = self
            .stats
            .as_ref()
            .map(|s| s.error_spans.iter().collect())
            .unwrap_or_default();
        if err_ids.is_empty() {
            return;
        }
        let n = v.len() as i32;
        let mut i = self.selected as i32;
        for _ in 0..n {
            i = (i + dir).rem_euclid(n);
            if err_ids.contains(&v[i as usize].0) {
                self.selected = i as usize;
                return;
            }
        }
    }

    pub fn toggle_collapse(&mut self) {
        if let Some(id) = self.selected_span_id() {
            if self.collapsed.contains(&id) {
                self.collapsed.remove(&id);
            } else if self.stats.as_ref().map(|s| s.has_children(&id)).unwrap_or(false) {
                self.collapsed.insert(id);
            }
        }
    }

    pub fn expand(&mut self) {
        if let Some(id) = self.selected_span_id() {
            self.collapsed.remove(&id);
        }
    }

    /// Seconds since the view opened — drives intro reveal & ambient pulses.
    pub fn elapsed_secs(&self) -> f32 {
        self.anim_start.elapsed().as_secs_f32()
    }
}
