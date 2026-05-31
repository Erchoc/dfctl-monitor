use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A complete distributed trace as returned by the backend. The backend is
/// expected to have already stitched spans together (parent/child links set);
/// the frontend only deserializes and computes derived stats (`stats.rs`).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TraceResponse {
    pub trace_id: String,
    pub root_service: String,
    pub root_operation: String,
    pub start_time: DateTime<Utc>,
    /// Total wall-clock duration of the whole trace, in microseconds.
    pub duration_us: u64,
    pub status: TraceStatus,
    /// Flat list of spans; the tree is rebuilt from `parent_id`.
    pub spans: Vec<Span>,
    pub services: Vec<ServiceInfo>,
    /// Non-fatal data-quality notes (clock skew, missing spans, …).
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TraceStatus {
    Ok,
    Error,
    /// Trace still being written (some spans not yet arrived).
    Partial,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Span {
    pub span_id: String,
    /// `None` for the root span.
    pub parent_id: Option<String>,
    pub service: String,
    pub operation: String,
    pub kind: SpanKind,
    /// Offset from trace start, microseconds.
    pub start_offset_us: u64,
    pub duration_us: u64,
    pub status: SpanStatus,
    /// HTTP status / gRPC code, when applicable.
    #[serde(default)]
    pub status_code: Option<i32>,
    #[serde(default)]
    pub tags: Vec<(String, String)>,
    #[serde(default)]
    pub logs: Vec<SpanLog>,
}

impl Span {
    /// Absolute end offset from trace start (µs).
    pub fn end_offset_us(&self) -> u64 {
        self.start_offset_us + self.duration_us
    }

    pub fn is_error(&self) -> bool {
        matches!(self.status, SpanStatus::Error)
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpanKind {
    Server,
    Client,
    Internal,
    Producer,
    Consumer,
}

impl SpanKind {
    pub fn glyph(&self) -> &'static str {
        match self {
            SpanKind::Server => "▼",   // inbound: receiving a request
            SpanKind::Client => "▲",   // outbound: calling a dependency
            SpanKind::Internal => "•",
            SpanKind::Producer => "»",
            SpanKind::Consumer => "«",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpanStatus {
    Ok,
    Error,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SpanLog {
    pub at_offset_us: u64,
    pub level: String,
    pub message: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ServiceInfo {
    pub name: String,
    pub span_count: u32,
    pub error_count: u32,
}

/// Human-friendly duration: `1.24s` / `82ms` / `640µs`.
pub fn fmt_dur_us(us: u64) -> String {
    if us >= 1_000_000 {
        format!("{:.2}s", us as f64 / 1_000_000.0)
    } else if us >= 1_000 {
        format!("{}ms", ((us as f64) / 1000.0).round() as u64)
    } else {
        format!("{}µs", us)
    }
}
