use super::data::TraceResponse;
use super::stats::TraceStats;
use anyhow::Result;
use serde::Serialize;

#[derive(Serialize)]
struct JsonSpan<'a> {
    span_id: &'a str,
    parent_id: Option<&'a str>,
    service: &'a str,
    operation: &'a str,
    start_offset_us: u64,
    duration_us: u64,
    self_us: u64,
    depth: u16,
    status: &'a super::data::SpanStatus,
    status_code: Option<i32>,
    on_critical_path: bool,
}

#[derive(Serialize)]
struct JsonBreak<'a> {
    service: &'a str,
    self_us: u64,
    pct: f64,
}

#[derive(Serialize)]
struct JsonOutput<'a> {
    trace_id: &'a str,
    root_service: &'a str,
    root_operation: &'a str,
    duration_us: u64,
    status: &'a super::data::TraceStatus,
    summary: &'a str,
    critical_path: &'a [String],
    bottleneck: Option<&'a str>,
    service_breakdown: Vec<JsonBreak<'a>>,
    errors: Vec<&'a str>,
    spans: Vec<JsonSpan<'a>>,
    warnings: &'a [String],
}

pub fn write_json(trace: &TraceResponse, stats: &TraceStats) -> Result<()> {
    let spans = trace
        .spans
        .iter()
        .map(|s| JsonSpan {
            span_id: &s.span_id,
            parent_id: s.parent_id.as_deref(),
            service: &s.service,
            operation: &s.operation,
            start_offset_us: s.start_offset_us,
            duration_us: s.duration_us,
            self_us: *stats.self_us.get(&s.span_id).unwrap_or(&0),
            depth: *stats.depth.get(&s.span_id).unwrap_or(&0),
            status: &s.status,
            status_code: s.status_code,
            on_critical_path: stats.critical_set.contains(&s.span_id),
        })
        .collect();

    let service_breakdown = stats
        .breakdown
        .iter()
        .map(|b| JsonBreak {
            service: &b.service,
            self_us: b.self_us,
            pct: b.pct,
        })
        .collect();

    let errors = stats.error_spans.iter().map(|s| s.as_str()).collect();

    let out = JsonOutput {
        trace_id: &trace.trace_id,
        root_service: &trace.root_service,
        root_operation: &trace.root_operation,
        duration_us: trace.duration_us,
        status: &trace.status,
        summary: &stats.summary,
        critical_path: &stats.critical_path,
        bottleneck: stats.bottleneck.as_deref(),
        service_breakdown,
        errors,
        spans,
        warnings: &trace.warnings,
    };
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}
