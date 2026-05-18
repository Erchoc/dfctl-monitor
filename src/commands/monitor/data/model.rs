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

    pub fn all_default() -> Vec<MetricKind> {
        vec![
            MetricKind::Qps,
            MetricKind::Latency,
            MetricKind::ErrorRate,
            MetricKind::Upstream,
            MetricKind::Cpu,
            MetricKind::Memory,
            MetricKind::Replicas,
            MetricKind::Runtime,
        ]
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
    pub fn glyph(&self) -> &'static str {
        match self {
            EventKind::Restart => "↻",
            EventKind::Deploy => "▲",
            EventKind::AlertFired => "◆",
            EventKind::AlertResolved => "◇",
            EventKind::ScaleEvent => "⇅",
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
