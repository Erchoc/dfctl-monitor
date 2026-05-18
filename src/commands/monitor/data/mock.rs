use super::model::*;
use super::DataSource;
use anyhow::Result;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;

pub struct MockDataSource {
    pub delay: Duration,
    pub seed: u64,
}

impl Default for MockDataSource {
    fn default() -> Self {
        Self {
            delay: Duration::from_millis(100),
            seed: 0xD0F_BEEF,
        }
    }
}

impl DataSource for MockDataSource {
    fn fetch(
        &self,
        query: MonitorQuery,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<MonitorResponse>> + Send + '_>> {
        let delay = self.delay;
        let seed = self.seed;
        Box::pin(async move {
            tokio::time::sleep(delay).await;
            Ok(generate(&query, seed))
        })
    }
}

fn generate(q: &MonitorQuery, seed: u64) -> MonitorResponse {
    // mix the app name into the seed so different apps render different curves
    let mut h: u64 = seed;
    for b in q.app.as_bytes() {
        h = h.wrapping_mul(0x100000001b3).wrapping_add(*b as u64);
    }
    let mut rng = ChaCha8Rng::seed_from_u64(h);

    let total = (q.to - q.from).num_seconds().max(60) as i64;
    // adapt resolution to time range: ~120-180 points
    let resolution = (total / 180).max(30) as u32;
    let n = (total / resolution as i64) as usize;

    let pod_names = vec!["pod-a".to_string(), "pod-b".to_string(), "pod-c".to_string()];
    let pods = generate_pods(&pod_names, &mut rng, q.to);

    let times: Vec<DateTime<Utc>> = (0..n)
        .map(|i| q.from + ChronoDuration::seconds((i as i64) * resolution as i64))
        .collect();

    let mut metrics: HashMap<MetricKind, MetricData> = HashMap::new();

    // ── QPS by Status (stacked) ──
    metrics.insert(MetricKind::Qps, generate_qps(&times, &mut rng));
    // ── Latency P50/P95/P99 ──
    metrics.insert(MetricKind::Latency, generate_latency(&times, &mut rng));
    // ── Error Rate ──
    metrics.insert(MetricKind::ErrorRate, generate_error_rate(&times, &mut rng));
    // ── Upstream P99 (HSF / DB / Redis) ──
    metrics.insert(MetricKind::Upstream, generate_upstream(&times, &mut rng));
    // ── CPU (max + avg + per-pod) ──
    metrics.insert(MetricKind::Cpu, generate_cpu(&times, &pod_names, &mut rng));
    // ── Memory (max + avg + per-pod) ──
    metrics.insert(MetricKind::Memory, generate_memory(&times, &pod_names, &mut rng));
    // ── Runtime: GC pause + goroutines ──
    metrics.insert(MetricKind::Runtime, generate_runtime(&times, &mut rng));

    // Replicas is special — rendered as a card, no points needed but we put empty MetricData.
    metrics.insert(
        MetricKind::Replicas,
        MetricData {
            unit: "pods".into(),
            series: vec![],
        },
    );

    let events = generate_events(&times, q.to);

    MonitorResponse {
        app: q.app.clone(),
        region: "cn-hangzhou".into(),
        env: "production".into(),
        time_range: TimeRange {
            from: q.from,
            to: q.to,
        },
        resolution_seconds: resolution,
        pods,
        metrics,
        events,
    }
}

fn generate_pods(names: &[String], rng: &mut ChaCha8Rng, now: DateTime<Utc>) -> Vec<PodInfo> {
    names
        .iter()
        .enumerate()
        .map(|(i, n)| {
            // pod-c is the unhealthy one (used by the rest of the generators too)
            let restarts = if i == 2 { 1 } else { 0 };
            let last_restart = if restarts > 0 {
                Some(now - ChronoDuration::minutes(26))
            } else {
                None
            };
            let uptime = if restarts > 0 {
                26 * 60
            } else {
                84 * 86_400 + 6 * 3600
            };
            PodInfo {
                name: n.clone(),
                status: "Running".into(),
                uptime_seconds: uptime,
                restarts,
                last_restart_at: last_restart,
                cpu_pct: 28.0 + (i as f64) * 13.0 + rng.gen_range(-2.0..2.0),
                mem_bytes: ((2.0 + (i as f64) * 0.25) * 1024.0 * 1024.0 * 1024.0) as u64,
            }
        })
        .collect()
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn smooth_walk(rng: &mut ChaCha8Rng, n: usize, base: f64, vol: f64, momentum: f64) -> Vec<f64> {
    let mut out = Vec::with_capacity(n);
    let mut v = base;
    let mut drift: f64 = 0.0;
    for _ in 0..n {
        drift = drift * momentum + (rng.gen_range(-vol..vol)) * (1.0 - momentum);
        v += drift;
        v = (v + base) * 0.5 + drift * 0.5;
        out.push(v);
    }
    out
}

fn add_spike(curve: &mut [f64], idx: usize, height: f64, width: usize) {
    for i in 0..width {
        let phase = i as f64 / width as f64;
        let bump = (1.0 - (2.0 * phase - 1.0).abs()) * height;
        let pos = idx + i;
        if pos < curve.len() {
            curve[pos] += bump;
        }
    }
}

fn pair(times: &[DateTime<Utc>], vals: Vec<f64>) -> Vec<(DateTime<Utc>, f64)> {
    times.iter().copied().zip(vals.into_iter()).collect()
}

// ── QPS by status: 2xx (big), 4xx (small), 5xx (tiny except spike near event) ──

fn generate_qps(times: &[DateTime<Utc>], rng: &mut ChaCha8Rng) -> MetricData {
    let n = times.len();
    let spike_at = (n as f64 * 0.78) as usize; // around event window

    // baseline tuned so 5xx + 4xx spike is visually obvious in the stacked chart
    let mut s2xx = smooth_walk(rng, n, 1800.0, 90.0, 0.88);
    let mut s4xx = smooth_walk(rng, n, 85.0, 15.0, 0.75);
    let mut s5xx = smooth_walk(rng, n, 10.0, 3.0, 0.65);

    // big spike during incident — 5xx + 4xx together approach the 2xx baseline so the
    // stacked bar visibly bulges upward
    add_spike(&mut s5xx, spike_at, 950.0, 9);
    add_spike(&mut s4xx, spike_at, 420.0, 9);
    // 2xx dips during incident
    for i in 0..10 {
        if let Some(v) = s2xx.get_mut(spike_at + i) {
            *v *= 0.62;
        }
    }
    // soft ramp-up before the spike for an organic shape
    for i in 0..18 {
        let pos = spike_at.saturating_sub(18 - i);
        if let Some(v) = s5xx.get_mut(pos) {
            *v += (i as f64) * 3.0;
        }
        if let Some(v) = s4xx.get_mut(pos) {
            *v += (i as f64) * 1.0;
        }
    }

    for v in s2xx.iter_mut().chain(s4xx.iter_mut()).chain(s5xx.iter_mut()) {
        *v = v.max(0.0);
    }

    MetricData {
        unit: "rpm".into(),
        series: vec![
            Series {
                label: "2xx".into(),
                kind: SeriesKind::StatusCode(200),
                aggregation: Aggregation::Sum,
                across_pods: true,
                points: pair(times, s2xx),
            },
            Series {
                label: "4xx".into(),
                kind: SeriesKind::StatusCode(400),
                aggregation: Aggregation::Sum,
                across_pods: true,
                points: pair(times, s4xx),
            },
            Series {
                label: "5xx".into(),
                kind: SeriesKind::StatusCode(500),
                aggregation: Aggregation::Sum,
                across_pods: true,
                points: pair(times, s5xx),
            },
        ],
    }
}

// ── Latency P50 / P95 / P99 ──

fn generate_latency(times: &[DateTime<Utc>], rng: &mut ChaCha8Rng) -> MetricData {
    let n = times.len();
    let spike_at = (n as f64 * 0.78) as usize;

    let mut p50 = smooth_walk(rng, n, 24.0, 1.5, 0.85);
    let mut p95 = smooth_walk(rng, n, 88.0, 4.0, 0.85);
    let mut p99 = smooth_walk(rng, n, 130.0, 8.0, 0.8);

    add_spike(&mut p99, spike_at, 90.0, 8);
    add_spike(&mut p95, spike_at, 32.0, 8);
    add_spike(&mut p50, spike_at, 6.0, 6);

    MetricData {
        unit: "ms".into(),
        series: vec![
            Series {
                label: "P50".into(),
                kind: SeriesKind::Percentile(50),
                aggregation: Aggregation::Max,
                across_pods: true,
                points: pair(times, p50),
            },
            Series {
                label: "P95".into(),
                kind: SeriesKind::Percentile(95),
                aggregation: Aggregation::Max,
                across_pods: true,
                points: pair(times, p95),
            },
            Series {
                label: "P99".into(),
                kind: SeriesKind::Percentile(99),
                aggregation: Aggregation::Max,
                across_pods: true,
                points: pair(times, p99),
            },
        ],
    }
}

fn generate_error_rate(times: &[DateTime<Utc>], rng: &mut ChaCha8Rng) -> MetricData {
    let n = times.len();
    let spike_at = (n as f64 * 0.78) as usize;
    let mut e = smooth_walk(rng, n, 2.4, 0.25, 0.85);
    add_spike(&mut e, spike_at, 5.7, 6);
    for v in e.iter_mut() {
        *v = v.max(0.05);
    }
    MetricData {
        unit: "%".into(),
        series: vec![Series {
            label: "error rate".into(),
            kind: SeriesKind::Single,
            aggregation: Aggregation::Avg,
            across_pods: true,
            points: pair(times, e),
        }],
    }
}

fn generate_upstream(times: &[DateTime<Utc>], rng: &mut ChaCha8Rng) -> MetricData {
    let n = times.len();
    let spike_at = (n as f64 * 0.78) as usize;
    let mut hsf = smooth_walk(rng, n, 48.0, 4.0, 0.85);
    let mut db = smooth_walk(rng, n, 24.0, 3.0, 0.85);
    let mut redis = smooth_walk(rng, n, 6.0, 0.6, 0.85);
    add_spike(&mut db, spike_at, 65.0, 7);
    add_spike(&mut hsf, spike_at, 22.0, 5);

    MetricData {
        unit: "ms".into(),
        series: vec![
            Series {
                label: "HSF".into(),
                kind: SeriesKind::Component("hsf".into()),
                aggregation: Aggregation::Max,
                across_pods: true,
                points: pair(times, hsf),
            },
            Series {
                label: "DB".into(),
                kind: SeriesKind::Component("db".into()),
                aggregation: Aggregation::Max,
                across_pods: true,
                points: pair(times, db),
            },
            Series {
                label: "Redis".into(),
                kind: SeriesKind::Component("redis".into()),
                aggregation: Aggregation::Max,
                across_pods: true,
                points: pair(times, redis),
            },
        ],
    }
}

fn generate_cpu(
    times: &[DateTime<Utc>],
    pods: &[String],
    rng: &mut ChaCha8Rng,
) -> MetricData {
    let n = times.len();
    let spike_at = (n as f64 * 0.78) as usize;

    // bases spread wider so max-vs-avg drift > 25% (triggers the "⚠ uneven" subtitle hint)
    let bases = [38.0, 26.0, 68.0];
    let vols = [2.5, 2.0, 3.0];
    let mut per_pod: Vec<Vec<f64>> = pods
        .iter()
        .enumerate()
        .map(|(i, _)| smooth_walk(rng, n, bases[i], vols[i], 0.85))
        .collect();
    // pod-c spikes during incident; clamp to physical CPU range
    add_spike(&mut per_pod[2], spike_at, 28.0, 7);
    for row in per_pod.iter_mut() {
        for v in row.iter_mut() {
            *v = v.clamp(0.0, 99.0);
        }
    }

    let max_curve: Vec<f64> = (0..n)
        .map(|i| per_pod.iter().map(|s| s[i]).fold(f64::MIN, f64::max))
        .collect();
    let avg_curve: Vec<f64> = (0..n)
        .map(|i| per_pod.iter().map(|s| s[i]).sum::<f64>() / per_pod.len() as f64)
        .collect();

    let mut series = vec![
        Series {
            label: "max".into(),
            kind: SeriesKind::Single,
            aggregation: Aggregation::Max,
            across_pods: true,
            points: pair(times, max_curve),
        },
        Series {
            label: "avg".into(),
            kind: SeriesKind::Single,
            aggregation: Aggregation::Avg,
            across_pods: true,
            points: pair(times, avg_curve),
        },
    ];
    for (i, p) in pods.iter().enumerate() {
        series.push(Series {
            label: p.clone(),
            kind: SeriesKind::Pod(p.clone()),
            aggregation: Aggregation::Raw,
            across_pods: false,
            points: pair(times, per_pod[i].clone()),
        });
    }
    MetricData {
        unit: "%".into(),
        series,
    }
}

fn generate_memory(
    times: &[DateTime<Utc>],
    pods: &[String],
    rng: &mut ChaCha8Rng,
) -> MetricData {
    let n = times.len();
    let bases = [2.3, 2.0, 2.5];
    let mut per_pod: Vec<Vec<f64>> = pods
        .iter()
        .enumerate()
        .map(|(i, _)| smooth_walk(rng, n, bases[i], 0.08, 0.9))
        .collect();
    // pod-c grows then drops (after restart at index ~spike_at)
    let restart_at = (n as f64 * 0.78) as usize;
    for i in 0..restart_at {
        per_pod[2][i] += (i as f64 / restart_at as f64) * 0.9;
    }
    for i in restart_at..n {
        per_pod[2][i] = 1.8 + ((i - restart_at) as f64 * 0.02);
    }

    let max_curve: Vec<f64> = (0..n)
        .map(|i| per_pod.iter().map(|s| s[i]).fold(f64::MIN, f64::max))
        .collect();
    let avg_curve: Vec<f64> = (0..n)
        .map(|i| per_pod.iter().map(|s| s[i]).sum::<f64>() / per_pod.len() as f64)
        .collect();

    let mut series = vec![
        Series {
            label: "max".into(),
            kind: SeriesKind::Single,
            aggregation: Aggregation::Max,
            across_pods: true,
            points: pair(times, max_curve),
        },
        Series {
            label: "avg".into(),
            kind: SeriesKind::Single,
            aggregation: Aggregation::Avg,
            across_pods: true,
            points: pair(times, avg_curve),
        },
    ];
    for (i, p) in pods.iter().enumerate() {
        series.push(Series {
            label: p.clone(),
            kind: SeriesKind::Pod(p.clone()),
            aggregation: Aggregation::Raw,
            across_pods: false,
            points: pair(times, per_pod[i].clone()),
        });
    }
    MetricData {
        unit: "GB".into(),
        series,
    }
}

fn generate_runtime(times: &[DateTime<Utc>], rng: &mut ChaCha8Rng) -> MetricData {
    let n = times.len();
    let spike_at = (n as f64 * 0.78) as usize;
    let mut gc = smooth_walk(rng, n, 8.0, 0.8, 0.8);
    add_spike(&mut gc, spike_at, 18.0, 5);
    // goroutine scale (in hundreds) so it cohabits the same Y axis as GC ms cleanly
    let goroutines = smooth_walk(rng, n, 18.5, 0.4, 0.9);

    MetricData {
        unit: "ms".into(),
        series: vec![
            Series {
                label: "GC pause (ms)".into(),
                kind: SeriesKind::Single,
                aggregation: Aggregation::Max,
                across_pods: true,
                points: pair(times, gc),
            },
            Series {
                label: "goroutines / 100".into(),
                kind: SeriesKind::Single,
                aggregation: Aggregation::Max,
                across_pods: true,
                points: pair(times, goroutines),
            },
        ],
    }
}

fn generate_events(times: &[DateTime<Utc>], now: DateTime<Utc>) -> Vec<Event> {
    let n = times.len();
    let spike_idx = (n as f64 * 0.78) as usize;
    let spike_at = *times.get(spike_idx).unwrap_or(&now);
    let restart_at = spike_at;

    let mut events = vec![
        Event {
            at: restart_at,
            kind: EventKind::Restart,
            message: "pod-c restart (OOMKilled, exit 137)".into(),
        },
        Event {
            at: spike_at + ChronoDuration::seconds(160),
            kind: EventKind::AlertFired,
            message: "CPU spike 51%→87% on pod-c (18s)".into(),
        },
    ];
    if n > 30 {
        let deploy_at = times[(n as f64 * 0.15) as usize];
        events.push(Event {
            at: deploy_at,
            kind: EventKind::Deploy,
            message: "deploy v2.4.1 (rolling)".into(),
        });
    }
    events
}
