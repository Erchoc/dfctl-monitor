//! Deterministic mock trace generator.
//!
//! The shape is fixed (a realistic checkout request fanning out to auth,
//! inventory→mysql, and payment→{redis(error), stripe}); the trace-id seeds a
//! PRNG that jitters durations so the same id always reproduces the same trace
//! (handy for snapshot tests) while different ids look distinct.

use super::model::*;
use super::TraceSource;
use anyhow::Result;
use chrono::Utc;
use rand::RngExt;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use std::future::Future;
use std::pin::Pin;

pub struct MockTraceSource {
    pub latency_ms: u64,
}

impl Default for MockTraceSource {
    fn default() -> Self {
        Self { latency_ms: 80 }
    }
}

fn seed_from_id(id: &str) -> u64 {
    let mut h: u64 = 1469598103934665603; // FNV offset basis
    for b in id.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    h
}

#[allow(clippy::too_many_arguments)]
fn span(
    id: &str,
    parent: Option<&str>,
    service: &str,
    op: &str,
    kind: SpanKind,
    start: u64,
    dur: u64,
    status: SpanStatus,
    code: Option<i32>,
    tags: &[(&str, &str)],
    logs: Vec<SpanLog>,
) -> Span {
    Span {
        span_id: id.to_string(),
        parent_id: parent.map(|s| s.to_string()),
        service: service.to_string(),
        operation: op.to_string(),
        kind,
        start_offset_us: start,
        duration_us: dur,
        status,
        status_code: code,
        tags: tags.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        logs,
    }
}

fn log(at: u64, level: &str, msg: &str) -> SpanLog {
    SpanLog {
        at_offset_us: at,
        level: level.to_string(),
        message: msg.to_string(),
    }
}

pub fn generate(trace_id: &str) -> TraceResponse {
    let mut rng = ChaCha8Rng::seed_from_u64(seed_from_id(trace_id));

    // jitter helper: base ± pct, in µs
    let mut jitter = |base: u64, pct: f64| -> u64 {
        let delta = (base as f64 * pct) as i64;
        let off = rng.random_range(-delta..=delta);
        (base as i64 + off).max(1) as u64
    };

    // ── leaves & critical path durations (seed-jittered) ────────────────────
    let auth_dur = jitter(82_000, 0.20);
    let mysql_dur = jitter(380_000, 0.18);
    let redis_dur = jitter(4_000, 0.30);
    let stripe_dur = jitter(640_000, 0.15);

    // fixed offsets for the fan-out structure
    let auth_start = 20_000;
    let inv_start = 110_000;
    let mysql_start = inv_start + 20_000;
    let pay_start = 540_000;
    let redis_start = pay_start + 20_000;
    let stripe_start = pay_start + 50_000;

    // inventory wraps mysql with a little self time on each side
    let inv_dur = (mysql_start - inv_start) + mysql_dur + 10_000;
    // payment ends shortly after stripe returns
    let stripe_end = stripe_start + stripe_dur;
    let pay_end = stripe_end + 10_000;
    let pay_dur = pay_end - pay_start;
    // root ends shortly after payment returns
    let root_dur = pay_end + 10_000;

    let spans = vec![
        span(
            "s0", None, "checkout-api", "POST /checkout", SpanKind::Server,
            0, root_dur, SpanStatus::Ok, Some(200),
            &[("http.method", "POST"), ("http.route", "/checkout"), ("http.status", "200")],
            vec![log(0, "info", "received checkout request")],
        ),
        span(
            "s1", Some("s0"), "auth-svc", "verifyToken", SpanKind::Server,
            auth_start, auth_dur, SpanStatus::Ok, Some(200),
            &[("rpc.system", "grpc"), ("user.id", "u_8842")],
            vec![],
        ),
        span(
            "s2", Some("s0"), "inventory-svc", "GET /stock", SpanKind::Server,
            inv_start, inv_dur, SpanStatus::Ok, Some(200),
            &[("http.method", "GET"), ("http.route", "/stock"), ("items", "3")],
            vec![],
        ),
        span(
            "s3", Some("s2"), "mysql", "SELECT items", SpanKind::Client,
            mysql_start, mysql_dur, SpanStatus::Ok, None,
            &[
                ("db.system", "mysql"),
                ("db.statement", "SELECT * FROM items WHERE sku IN (?,?,?)"),
                ("db.rows", "3"),
            ],
            vec![log(mysql_start, "debug", "query sent")],
        ),
        span(
            "s4", Some("s0"), "payment-svc", "charge", SpanKind::Server,
            pay_start, pay_dur, SpanStatus::Ok, Some(200),
            &[("amount", "42.00"), ("currency", "USD")],
            vec![],
        ),
        span(
            "s5", Some("s4"), "redis", "GET token", SpanKind::Client,
            redis_start, redis_dur, SpanStatus::Error, None,
            &[("db.system", "redis"), ("net.peer", "redis-0:6379")],
            vec![log(redis_start + redis_dur, "error", "connection reset by peer")],
        ),
        span(
            "s6", Some("s4"), "stripe", "POST /charges", SpanKind::Client,
            stripe_start, stripe_dur, SpanStatus::Ok, Some(200),
            &[
                ("http.method", "POST"),
                ("peer", "api.stripe.com:443"),
                ("http.status", "200"),
                ("net.transport", "tcp"),
            ],
            vec![
                log(stripe_start, "info", "sending charge request $42.00"),
                log(stripe_end, "info", "charge succeeded id=ch_3PqX"),
            ],
        ),
    ];

    let services = vec![
        ServiceInfo { name: "checkout-api".into(), span_count: 1, error_count: 0 },
        ServiceInfo { name: "auth-svc".into(), span_count: 1, error_count: 0 },
        ServiceInfo { name: "inventory-svc".into(), span_count: 1, error_count: 0 },
        ServiceInfo { name: "mysql".into(), span_count: 1, error_count: 0 },
        ServiceInfo { name: "payment-svc".into(), span_count: 1, error_count: 0 },
        ServiceInfo { name: "redis".into(), span_count: 1, error_count: 1 },
        ServiceInfo { name: "stripe".into(), span_count: 1, error_count: 0 },
    ];

    TraceResponse {
        trace_id: trace_id.to_string(),
        root_service: "checkout-api".into(),
        root_operation: "POST /checkout".into(),
        start_time: Utc::now() - chrono::Duration::microseconds(root_dur as i64),
        duration_us: root_dur,
        status: TraceStatus::Error, // redis span failed
        spans,
        services,
        warnings: vec![],
    }
}

impl TraceSource for MockTraceSource {
    fn fetch(
        &self,
        trace_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<TraceResponse>> + Send + '_>> {
        let id = trace_id.to_string();
        let delay = self.latency_ms;
        Box::pin(async move {
            if delay > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
            }
            Ok(generate(&id))
        })
    }
}
