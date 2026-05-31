//! Pluggable trace summarization.
//!
//! `HeuristicSummary` is the default and produces a one-line natural-language
//! summary purely from the computed stats (no network). `LlmSummary` is a
//! deliberately-unimplemented placeholder: when `--explain` lands it will call
//! out to an LLM endpoint. The trait keeps that future swap a one-liner.

use super::data::{Span, TraceResponse};

/// Everything a summarizer needs, already computed by `stats.rs`.
pub struct SummaryInput<'a> {
    pub trace: &'a TraceResponse,
    pub breakdown: &'a [ServiceBreak],
    pub bottleneck: Option<&'a Span>,
    /// Ordered root→leaf; available to richer summarizers (e.g. LLM).
    #[allow(dead_code)]
    pub critical_path: &'a [String],
    pub error_spans: &'a [&'a Span],
}

/// One service's share of total self-time.
#[derive(Clone, Debug)]
pub struct ServiceBreak {
    pub service: String,
    pub self_us: u64,
    pub pct: f64,
}

pub trait SummarySource: Send + Sync {
    fn summarize(&self, input: &SummaryInput) -> String;
}

/// Default: template-based, deterministic, offline.
pub struct HeuristicSummary;

impl SummarySource for HeuristicSummary {
    fn summarize(&self, input: &SummaryInput) -> String {
        use super::data::fmt_dur_us;
        let total = input.trace.duration_us;
        let mut parts: Vec<String> = Vec::new();
        parts.push(format!(
            "请求耗时 {}（{} 个 span / {} 个服务）",
            fmt_dur_us(total),
            input.trace.spans.len(),
            input.trace.services.len()
        ));

        if let Some(b) = input.bottleneck {
            let pct = if total > 0 {
                self_us_of(input, &b.span_id) as f64 / total as f64 * 100.0
            } else {
                0.0
            };
            parts.push(format!(
                "{:.0}% 花在 {}.{}",
                pct, b.service, b.operation
            ));
        }
        // second-biggest contributor, if material
        if let Some(second) = input.breakdown.get(1) {
            if second.pct > 0.10 {
                parts.push(format!("{} 占 {:.0}%", second.service, second.pct * 100.0));
            }
        }

        if let Some(err) = input.error_spans.first() {
            let why = err
                .logs
                .iter()
                .find(|l| l.level == "error")
                .map(|l| l.message.clone())
                .or_else(|| err.status_code.map(|c| format!("status {}", c)))
                .unwrap_or_else(|| "error".into());
            parts.push(format!("{}.{} 失败（{}）", err.service, err.operation, why));
        }

        let body = parts.join("；");
        format!("{}。", body)
    }
}

fn self_us_of(input: &SummaryInput, span_id: &str) -> u64 {
    input
        .breakdown
        .iter()
        .find(|b| {
            // breakdown is per-service; approximate bottleneck self by matching the
            // bottleneck span's service. Good enough for the headline %.
            input
                .bottleneck
                .map(|sp| sp.service == b.service)
                .unwrap_or(false)
        })
        .map(|b| b.self_us)
        .unwrap_or_else(|| {
            input
                .trace
                .spans
                .iter()
                .find(|s| s.span_id == span_id)
                .map(|s| s.duration_us)
                .unwrap_or(0)
        })
}

/// Placeholder for an LLM-backed summary (`--explain`). Intentionally not wired
/// into the data path yet — constructing one and calling `summarize` will panic
/// so we never silently ship a stub. Fill in when the explain feature lands.
#[allow(dead_code)]
pub struct LlmSummary {
    pub endpoint: String,
    pub model: String,
}

impl SummarySource for LlmSummary {
    fn summarize(&self, _input: &SummaryInput) -> String {
        unimplemented!("LLM-backed trace summary is not implemented yet (--explain)")
    }
}
