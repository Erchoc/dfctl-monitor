use super::data::*;
use anyhow::Result;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Serialize)]
struct JsonOutput<'a> {
    app: &'a str,
    region: &'a str,
    env: &'a str,
    time_range: &'a TimeRange,
    resolution_seconds: u32,
    pods: &'a Vec<PodInfo>,
    metrics: HashMap<&'static str, JsonMetric<'a>>,
    events: &'a Vec<Event>,
}

#[derive(Serialize)]
struct JsonMetric<'a> {
    unit: &'a str,
    series: Vec<JsonSeries<'a>>,
}

#[derive(Serialize)]
struct JsonSeries<'a> {
    label: &'a str,
    kind: &'a SeriesKind,
    aggregation: &'a Aggregation,
    across_pods: bool,
    points: &'a Vec<(chrono::DateTime<chrono::Utc>, f64)>,
    stats: SeriesStats,
}

pub fn write_json(resp: &MonitorResponse) -> Result<()> {
    let mut metrics = HashMap::new();
    for (k, md) in &resp.metrics {
        let key = match k {
            MetricKind::Qps => "qps",
            MetricKind::Latency => "latency",
            MetricKind::ErrorRate => "error_rate",
            MetricKind::Upstream => "upstream",
            MetricKind::Cpu => "cpu",
            MetricKind::Memory => "memory",
            MetricKind::Replicas => "replicas",
            MetricKind::Runtime => "runtime",
        };
        let series = md.series.iter().map(|s| JsonSeries {
            label: &s.label,
            kind: &s.kind,
            aggregation: &s.aggregation,
            across_pods: s.across_pods,
            points: &s.points,
            stats: s.stats(),
        }).collect();
        metrics.insert(key, JsonMetric { unit: &md.unit, series });
    }
    let out = JsonOutput {
        app: &resp.app,
        region: &resp.region,
        env: &resp.env,
        time_range: &resp.time_range,
        resolution_seconds: resp.resolution_seconds,
        pods: &resp.pods,
        metrics,
        events: &resp.events,
    };
    let s = serde_json::to_string_pretty(&out)?;
    println!("{}", s);
    Ok(())
}
