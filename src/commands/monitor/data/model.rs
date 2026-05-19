use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MonitorResponse {
    pub app: String,
    pub region: String,
    pub env: String,
    pub time_range: TimeRange,
    pub resolution_seconds: u32,
    pub pods: Vec<PodInfo>,
    pub metrics: HashMap<MetricKind, MetricData>,
    pub events: Vec<Event>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TimeRange {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PodInfo {
    pub name: String,
    pub status: String,
    pub uptime_seconds: u64,
    pub restarts: u32,
    pub last_restart_at: Option<DateTime<Utc>>,
    pub cpu_pct: f64,
    pub mem_bytes: u64,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MetricKind {
    Qps,
    Latency,
    ErrorRate,
    Upstream,
    Cpu,
    Memory,
    Replicas,
    Runtime,
}

impl MetricKind {
    pub fn title(&self) -> &'static str {
        match self {
            MetricKind::Qps => "QPS by Status",
            MetricKind::Latency => "Latency",
            MetricKind::ErrorRate => "Error Rate",
            MetricKind::Upstream => "Upstream P99",
            MetricKind::Cpu => "CPU Usage",
            MetricKind::Memory => "Memory",
            MetricKind::Replicas => "Replicas & Restarts",
            MetricKind::Runtime => "Runtime",
        }
    }

    /// Default panel order, by real on-call value:
    /// 1. QPS / Latency / Error Rate — RED metrics, always present
    /// 2. CPU / Memory / Replicas    — resources + cluster shape, platform-supplied
    /// 3. Upstream / Runtime          — optional; depend on BaaS gateway and
    ///    per-language Prometheus exporters respectively. If no data, hide.
    pub fn all_default() -> Vec<MetricKind> {
        vec![
            MetricKind::Qps,
            MetricKind::Latency,
            MetricKind::ErrorRate,
            MetricKind::Cpu,
            MetricKind::Memory,
            MetricKind::Replicas,
            MetricKind::Upstream,
            MetricKind::Runtime,
        ]
    }

    /// Metrics that depend on optional data sources (BaaS gateway / per-language
    /// Prometheus exporter). Hidden automatically if the API returns no series.
    pub fn is_optional(&self) -> bool {
        matches!(self, MetricKind::Upstream | MetricKind::Runtime)
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "qps" | "rpm" | "traffic" => Some(Self::Qps),
            "latency" | "lat" => Some(Self::Latency),
            "error_rate" | "error" | "errors" => Some(Self::ErrorRate),
            "upstream" | "deps" => Some(Self::Upstream),
            "cpu" => Some(Self::Cpu),
            "memory" | "mem" => Some(Self::Memory),
            "replicas" | "pods" => Some(Self::Replicas),
            "runtime" | "gc" => Some(Self::Runtime),
            _ => None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MetricData {
    pub unit: String,
    pub series: Vec<Series>,
    /// Per-metric warn / alert thresholds delivered by the API. If absent, the
    /// front-end falls back to sensible defaults from `theme::assess_health`.
    /// The platform team owns these — they reflect SLO contracts, not UI policy.
    #[serde(default)]
    pub thresholds: Option<Thresholds>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Thresholds {
    /// Watch this aggregation (e.g. P99 for latency, max for cpu, current for
    /// error_rate). If `None`, the front-end picks a metric-appropriate default.
    #[serde(default)]
    pub watch_series: Option<String>,
    /// Threshold above which the panel turns WARN. Compared against `watch_series`
    /// current value.
    #[serde(default)]
    pub warn_above: Option<f64>,
    /// Threshold above which the panel turns ALERT.
    #[serde(default)]
    pub alert_above: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Series {
    pub label: String,
    pub kind: SeriesKind,
    pub aggregation: Aggregation,
    pub across_pods: bool,
    pub points: Vec<(DateTime<Utc>, f64)>,
}

impl Series {
    pub fn stats(&self) -> SeriesStats {
        let vals: Vec<f64> = self.points.iter().map(|p| p.1).collect();
        if vals.is_empty() {
            return SeriesStats::default();
        }
        let mut sorted = vals.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let pct = |p: f64| {
            let i = ((sorted.len() - 1) as f64 * p) as usize;
            sorted[i]
        };
        let sum: f64 = vals.iter().sum();
        SeriesStats {
            min: sorted[0],
            max: *sorted.last().unwrap(),
            avg: sum / vals.len() as f64,
            p50: pct(0.5),
            p95: pct(0.95),
            p99: pct(0.99),
        }
    }

    pub fn current(&self) -> f64 {
        self.points.last().map(|p| p.1).unwrap_or(0.0)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SeriesStats {
    pub min: f64,
    pub max: f64,
    pub avg: f64,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", content = "value")]
pub enum SeriesKind {
    Percentile(u8),
    StatusCode(u16),
    Pod(String),
    Component(String),
    Single,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Aggregation {
    Max,
    Avg,
    Sum,
    P50,
    P95,
    P99,
    Raw,
}

impl Aggregation {
    #[allow(dead_code)] // surfaced through future panel subtitles
    pub fn label(&self) -> &'static str {
        match self {
            Aggregation::Max => "max",
            Aggregation::Avg => "avg",
            Aggregation::Sum => "sum",
            Aggregation::P50 => "p50",
            Aggregation::P95 => "p95",
            Aggregation::P99 => "p99",
            Aggregation::Raw => "raw",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Event {
    pub at: DateTime<Utc>,
    pub kind: EventKind,
    pub message: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Restart,
    Deploy,
    AlertFired,
    AlertResolved,
    ScaleEvent,
}

impl EventKind {
    /// Glyphs use only widely-available code points (Geometric Shapes block +
    /// Latin-1 punctuation). No CJK / Nerd Font fallbacks needed.
    pub fn glyph(&self) -> &'static str {
        match self {
            EventKind::Restart => "»",
            EventKind::Deploy => "▲",
            EventKind::AlertFired => "◆",
            EventKind::AlertResolved => "◇",
            EventKind::ScaleEvent => "↕",
        }
    }
}

#[derive(Clone, Debug)]
#[allow(dead_code)] // pods/metrics fields are wired through to the HTTP backend
pub struct MonitorQuery {
    pub app: String,
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub pods: Option<Vec<String>>,
    pub metrics: Option<Vec<MetricKind>>,
}
